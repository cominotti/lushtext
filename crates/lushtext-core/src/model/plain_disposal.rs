// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust admission and latest-pending ownership for off-main destruction.

/// Fixed resource ceilings for the app-owned plain-data disposal lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlainDisposalLimits {
    /// Maximum jobs that may be executing concurrently.
    pub worker_limit: usize,
    /// Maximum admitted jobs waiting behind the workers.
    pub queued_job_limit: usize,
    /// Maximum caller-estimated bytes retained by ordinary admitted jobs.
    pub retained_byte_limit: u64,
    /// Extra transient job slots available only to guarded replacements.
    pub replacement_job_headroom: usize,
    /// Largest one-off overweight transit reservation admitted additively.
    pub overweight_progress_byte_limit: u64,
}

impl PlainDisposalLimits {
    /// Build one explicit count-and-byte policy.
    #[must_use]
    pub const fn new(
        worker_limit: usize,
        queued_job_limit: usize,
        retained_byte_limit: u64,
    ) -> Self {
        Self {
            worker_limit,
            queued_job_limit,
            retained_byte_limit,
            replacement_job_headroom: 0,
            overweight_progress_byte_limit: retained_byte_limit,
        }
    }

    /// Reserve transient admission that ordinary long-lived owners cannot consume.
    #[must_use]
    pub const fn with_replacement_headroom(mut self, replacement_job_headroom: usize) -> Self {
        self.replacement_job_headroom = replacement_job_headroom;
        self
    }

    /// Allow one bounded overweight transit owner alongside ordinary ownership.
    #[must_use]
    pub const fn with_overweight_progress_byte_limit(mut self, byte_limit: u64) -> Self {
        self.overweight_progress_byte_limit = byte_limit;
        self
    }
}

/// Scalar admission evidence without payload contents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlainDisposalSnapshot {
    /// Jobs currently executing on disposal workers.
    pub running_jobs: usize,
    /// Admitted jobs currently waiting in the bounded channel.
    pub queued_jobs: usize,
    /// Caller-estimated bytes retained by all admitted jobs.
    pub retained_bytes: u64,
    /// Whether one over-limit job currently owns the otherwise-empty lane.
    pub overweight_exclusive: bool,
    /// Whether one replacement currently borrows transient count or byte headroom.
    pub replacement_headroom_active: bool,
    /// Largest observed executing-job count.
    pub running_high_water: usize,
    /// Largest observed queued-job count.
    pub queued_high_water: usize,
    /// Largest observed admitted-job count.
    pub owned_high_water: usize,
    /// Largest ordinary retained-byte total observed.
    pub retained_bytes_high_water: u64,
    /// Largest exclusively admitted overweight job observed.
    pub overweight_bytes_high_water: u64,
    /// Largest actual retained-byte total while overweight transit was active.
    pub overweight_total_bytes_high_water: u64,
    /// Largest actual retained-byte total while replacement headroom was active.
    pub replacement_bytes_high_water: u64,
    /// Jobs admitted since this policy was created.
    pub admitted_jobs: u64,
    /// Attempts rejected by count or byte capacity.
    pub full_outcomes: u64,
    /// Attempts returned because the worker channel was closed.
    pub closed_outcomes: u64,
    /// Admitted jobs cancelled before a worker started them.
    pub cancelled_jobs: u64,
    /// Worker jobs that reached a terminal, including panics.
    pub completed_jobs: u64,
    /// Worker jobs whose destructor panicked without killing the lane worker.
    pub panicked_jobs: u64,
}

/// Queued ownership returned by a successful policy admission.
#[derive(Debug)]
pub struct QueuedPlainDisposal {
    weight: u64,
}

/// Executing ownership returned when a worker starts an admitted job.
#[derive(Debug)]
pub struct ActivePlainDisposal {
    weight: u64,
}

/// Pure count-and-byte admission state for one disposal lane.
#[derive(Debug)]
pub struct PlainDisposalAdmission {
    limits: PlainDisposalLimits,
    snapshot: PlainDisposalSnapshot,
}

impl PlainDisposalAdmission {
    /// Start an empty lane under the supplied resource ceilings.
    #[must_use]
    pub fn new(limits: PlainDisposalLimits) -> Self {
        Self {
            limits,
            snapshot: PlainDisposalSnapshot::default(),
        }
    }

    /// Try to reserve queued ownership without waiting for capacity.
    pub fn try_queue(&mut self, weight: u64) -> Option<QueuedPlainDisposal> {
        self.try_queue_inner(weight, None)
    }

    /// Try one bounded overweight reservation without requiring an empty lane.
    ///
    /// This is reserved for transit work whose supported maximum exceeds the
    /// ordinary byte ceiling. Only one such owner may exist, and all later
    /// admission remains blocked until aggregate ownership returns within the
    /// ordinary ceiling.
    pub fn try_queue_overweight_progress(&mut self, weight: u64) -> Option<QueuedPlainDisposal> {
        if weight <= self.limits.retained_byte_limit {
            return self.try_queue(weight);
        }

        let owned_jobs = self
            .snapshot
            .running_jobs
            .saturating_add(self.snapshot.queued_jobs);
        let ordinary_owned_limit = self
            .limits
            .worker_limit
            .saturating_add(self.limits.queued_job_limit);
        let available = weight <= self.limits.overweight_progress_byte_limit
            && self.snapshot.queued_jobs < self.limits.queued_job_limit
            && owned_jobs < ordinary_owned_limit
            && !self.snapshot.overweight_exclusive
            && !self.snapshot.replacement_headroom_active;
        if !available {
            self.snapshot.full_outcomes = self.snapshot.full_outcomes.saturating_add(1);
            return None;
        }

        self.snapshot.queued_jobs = self.snapshot.queued_jobs.saturating_add(1);
        self.snapshot.retained_bytes = self.snapshot.retained_bytes.saturating_add(weight);
        self.snapshot.overweight_exclusive = true;
        self.snapshot.admitted_jobs = self.snapshot.admitted_jobs.saturating_add(1);
        self.snapshot.queued_high_water = self
            .snapshot
            .queued_high_water
            .max(self.snapshot.queued_jobs);
        self.snapshot.owned_high_water = self.snapshot.owned_high_water.max(
            self.snapshot
                .running_jobs
                .saturating_add(self.snapshot.queued_jobs),
        );
        self.snapshot.overweight_bytes_high_water =
            self.snapshot.overweight_bytes_high_water.max(weight);
        self.snapshot.overweight_total_bytes_high_water = self
            .snapshot
            .overweight_total_bytes_high_water
            .max(self.snapshot.retained_bytes);

        Some(QueuedPlainDisposal { weight })
    }

    /// Try to reserve a guarded replacement using the current owner's byte credit.
    ///
    /// Ordinary owners cannot consume this transient headroom. At most one
    /// replacement may borrow it until an admitted job reaches a terminal and
    /// the lane returns within its ordinary count-and-byte ceilings.
    pub fn try_queue_replacement(
        &mut self,
        weight: u64,
        replaced_weight: u64,
    ) -> Option<QueuedPlainDisposal> {
        self.try_queue_inner(weight, Some(replaced_weight))
    }

    fn try_queue_inner(
        &mut self,
        weight: u64,
        replacement_credit: Option<u64>,
    ) -> Option<QueuedPlainDisposal> {
        let empty = self.snapshot.running_jobs == 0
            && self.snapshot.queued_jobs == 0
            && self.snapshot.retained_bytes == 0;
        let overweight = weight > self.limits.retained_byte_limit;
        let owned_jobs = self
            .snapshot
            .running_jobs
            .saturating_add(self.snapshot.queued_jobs);
        let ordinary_owned_limit = self
            .limits
            .worker_limit
            .saturating_add(self.limits.queued_job_limit);
        let ordinary_count_available = self.snapshot.queued_jobs < self.limits.queued_job_limit
            && owned_jobs < ordinary_owned_limit;
        let ordinary_bytes_available = !self.snapshot.overweight_exclusive
            && self
                .snapshot
                .retained_bytes
                .checked_add(weight)
                .is_some_and(|total| total <= self.limits.retained_byte_limit);
        let exclusive_progress = overweight && empty && self.limits.queued_job_limit > 0;

        let ordinary_available =
            ordinary_count_available && (ordinary_bytes_available || exclusive_progress);
        let replacement_available = !ordinary_available
            && !overweight
            && !self.snapshot.replacement_headroom_active
            && self.limits.replacement_job_headroom > 0
            && replacement_credit.is_some_and(|replaced_weight| {
                let replacement_queued_limit = self
                    .limits
                    .queued_job_limit
                    .saturating_add(self.limits.replacement_job_headroom);
                let replacement_owned_limit =
                    ordinary_owned_limit.saturating_add(self.limits.replacement_job_headroom);
                let adjusted_retained =
                    self.snapshot.retained_bytes.saturating_sub(replaced_weight);
                self.snapshot.queued_jobs < replacement_queued_limit
                    && owned_jobs < replacement_owned_limit
                    && !self.snapshot.overweight_exclusive
                    && adjusted_retained
                        .checked_add(weight)
                        .is_some_and(|total| total <= self.limits.retained_byte_limit)
            });

        if !ordinary_available && !replacement_available {
            self.snapshot.full_outcomes = self.snapshot.full_outcomes.saturating_add(1);
            return None;
        }

        self.snapshot.queued_jobs = self.snapshot.queued_jobs.saturating_add(1);
        self.snapshot.retained_bytes = self.snapshot.retained_bytes.saturating_add(weight);
        self.snapshot.overweight_exclusive = exclusive_progress;
        if replacement_available {
            self.snapshot.replacement_headroom_active = true;
            self.snapshot.replacement_bytes_high_water = self
                .snapshot
                .replacement_bytes_high_water
                .max(self.snapshot.retained_bytes);
        }
        self.snapshot.admitted_jobs = self.snapshot.admitted_jobs.saturating_add(1);
        self.snapshot.queued_high_water = self
            .snapshot
            .queued_high_water
            .max(self.snapshot.queued_jobs);
        self.snapshot.owned_high_water = self.snapshot.owned_high_water.max(
            self.snapshot
                .running_jobs
                .saturating_add(self.snapshot.queued_jobs),
        );
        if exclusive_progress {
            self.snapshot.overweight_bytes_high_water =
                self.snapshot.overweight_bytes_high_water.max(weight);
        } else if !replacement_available {
            self.snapshot.retained_bytes_high_water = self
                .snapshot
                .retained_bytes_high_water
                .max(self.snapshot.retained_bytes);
        }

        Some(QueuedPlainDisposal { weight })
    }

    /// Move one admitted job from queued to executing ownership.
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the non-Copy token is consumed to preserve exact admission ownership"
    )]
    pub fn start(&mut self, queued: QueuedPlainDisposal) -> ActivePlainDisposal {
        debug_assert!(self.snapshot.queued_jobs > 0);
        debug_assert!(self.snapshot.running_jobs < self.limits.worker_limit);
        self.snapshot.queued_jobs = self.snapshot.queued_jobs.saturating_sub(1);
        self.snapshot.running_jobs = self.snapshot.running_jobs.saturating_add(1);
        self.snapshot.running_high_water = self
            .snapshot
            .running_high_water
            .max(self.snapshot.running_jobs);
        ActivePlainDisposal {
            weight: queued.weight,
        }
    }

    /// Reduce one queued reservation after a worker measures its completed result.
    pub fn shrink_queued(&mut self, queued: &mut QueuedPlainDisposal, new_weight: u64) {
        debug_assert!(new_weight <= queued.weight);
        let released = queued.weight.saturating_sub(new_weight);
        queued.weight = new_weight;
        self.snapshot.retained_bytes = self.snapshot.retained_bytes.saturating_sub(released);
        self.demote_overweight_if_within_limit();
        self.refresh_replacement_headroom();
    }

    /// Release an admitted job that could not enter the worker channel.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the non-Copy token is consumed to preserve exact admission ownership"
    )]
    pub fn cancel_queued(&mut self, queued: QueuedPlainDisposal, closed: bool) {
        debug_assert!(self.snapshot.queued_jobs > 0);
        self.snapshot.queued_jobs = self.snapshot.queued_jobs.saturating_sub(1);
        self.release_bytes(queued.weight);
        self.snapshot.cancelled_jobs = self.snapshot.cancelled_jobs.saturating_add(1);
        if closed {
            self.snapshot.closed_outcomes = self.snapshot.closed_outcomes.saturating_add(1);
        }
    }

    /// Release worker ownership on every normal or panicking terminal path.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the non-Copy token is consumed so worker capacity cannot be released twice"
    )]
    pub fn finish(&mut self, active: ActivePlainDisposal, panicked: bool) {
        debug_assert!(self.snapshot.running_jobs > 0);
        self.snapshot.running_jobs = self.snapshot.running_jobs.saturating_sub(1);
        self.release_bytes(active.weight);
        self.snapshot.completed_jobs = self.snapshot.completed_jobs.saturating_add(1);
        if panicked {
            self.snapshot.panicked_jobs = self.snapshot.panicked_jobs.saturating_add(1);
        }
    }

    fn release_bytes(&mut self, weight: u64) {
        self.snapshot.retained_bytes = self.snapshot.retained_bytes.saturating_sub(weight);
        self.demote_overweight_if_within_limit();
        self.refresh_replacement_headroom();
    }

    fn demote_overweight_if_within_limit(&mut self) {
        if self.snapshot.retained_bytes <= self.limits.retained_byte_limit {
            self.snapshot.overweight_exclusive = false;
        }
    }

    fn refresh_replacement_headroom(&mut self) {
        let owned_jobs = self
            .snapshot
            .running_jobs
            .saturating_add(self.snapshot.queued_jobs);
        let ordinary_owned_limit = self
            .limits
            .worker_limit
            .saturating_add(self.limits.queued_job_limit);
        if self.snapshot.queued_jobs <= self.limits.queued_job_limit
            && owned_jobs <= ordinary_owned_limit
            && self.snapshot.retained_bytes <= self.limits.retained_byte_limit
        {
            self.snapshot.replacement_headroom_active = false;
        }
    }

    /// Return scalar current and high-water evidence.
    #[must_use]
    pub fn snapshot(&self) -> PlainDisposalSnapshot {
        self.snapshot
    }
}

/// Scalar evidence for one producer's replaceable pending slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlainDisposalLatestSnapshot {
    /// Whether one rejected job is retained for retry.
    pub pending_jobs: usize,
    /// Largest pending count, structurally limited to one.
    pub pending_high_water: usize,
    /// Whether the producer owns its single retry or wakeup source.
    pub retry_armed: bool,
    /// Largest retry-source count, structurally limited to one.
    pub retry_high_water: usize,
    /// Pending jobs replaced by newer stale ownership.
    pub replacements: u64,
    /// Pending jobs cancelled during generation or owner teardown.
    pub cancellations: u64,
}

/// One-latest pending ownership used by main-thread disposal producers.
#[derive(Debug)]
pub struct PlainDisposalLatest<T> {
    pending: Option<T>,
    snapshot: PlainDisposalLatestSnapshot,
}

impl<T> Default for PlainDisposalLatest<T> {
    fn default() -> Self {
        Self {
            pending: None,
            snapshot: PlainDisposalLatestSnapshot::default(),
        }
    }
}

impl<T> PlainDisposalLatest<T> {
    /// Retain the latest rejected job and return any superseded pending owner.
    pub fn replace(&mut self, value: T) -> Option<T> {
        let replaced = self.pending.replace(value);
        self.snapshot.pending_jobs = 1;
        self.snapshot.pending_high_water = 1;
        self.snapshot.retry_armed = true;
        self.snapshot.retry_high_water = 1;
        if replaced.is_some() {
            self.snapshot.replacements = self.snapshot.replacements.saturating_add(1);
        }
        replaced
    }

    /// Take the pending job for one non-blocking retry attempt.
    pub fn take_for_retry(&mut self) -> Option<T> {
        let value = self.pending.take();
        self.snapshot.pending_jobs = usize::from(self.pending.is_some());
        value
    }

    /// Restore ownership after another full outcome without adding a source.
    pub fn restore_after_full(&mut self, value: T) {
        debug_assert!(self.pending.is_none());
        self.pending = Some(value);
        self.snapshot.pending_jobs = 1;
        self.snapshot.retry_armed = true;
        self.snapshot.pending_high_water = 1;
        self.snapshot.retry_high_water = 1;
    }

    /// Disarm retry ownership after admission or a closed-lane terminal.
    pub fn finish_retry(&mut self) {
        debug_assert!(self.pending.is_none());
        self.snapshot.pending_jobs = 0;
        self.snapshot.retry_armed = false;
    }

    /// Cancel the pending slot and retry source at a generation or owner terminal.
    pub fn cancel(&mut self) -> Option<T> {
        let pending = self.pending.take();
        if pending.is_some() {
            self.snapshot.cancellations = self.snapshot.cancellations.saturating_add(1);
        }
        self.snapshot.pending_jobs = 0;
        self.snapshot.retry_armed = false;
        pending
    }

    /// Return scalar pending and source evidence.
    #[must_use]
    pub fn snapshot(&self) -> PlainDisposalLatestSnapshot {
        self.snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: PlainDisposalLimits = PlainDisposalLimits::new(2, 2, 100);

    #[test]
    fn count_and_byte_admission_return_full_without_changing_ownership() {
        let mut policy = PlainDisposalAdmission::new(LIMITS);
        let first = policy.try_queue(60).expect("first job");
        let active = policy.start(first);
        let second = policy.try_queue(40).expect("second job");

        assert!(policy.try_queue(1).is_none(), "byte capacity must reject");
        let snapshot = policy.snapshot();
        assert_eq!(snapshot.running_jobs, 1);
        assert_eq!(snapshot.queued_jobs, 1);
        assert_eq!(snapshot.retained_bytes, 100);
        assert_eq!(snapshot.full_outcomes, 1);

        policy.cancel_queued(second, false);
        policy.finish(active, false);
        assert_eq!(policy.snapshot().retained_bytes, 0);
    }

    #[test]
    fn overweight_job_progresses_only_when_lane_is_exclusively_empty() {
        let mut policy = PlainDisposalAdmission::new(LIMITS);
        let overweight = policy.try_queue(101).expect("exclusive overweight job");
        assert!(policy.snapshot().overweight_exclusive);
        assert!(policy.try_queue(0).is_none());

        let active = policy.start(overweight);
        assert!(policy.try_queue(1).is_none());
        policy.finish(active, false);
        assert!(!policy.snapshot().overweight_exclusive);
        assert_eq!(policy.snapshot().overweight_bytes_high_water, 101);

        let ordinary = policy.try_queue(1).expect("ordinary job after overweight");
        assert!(policy.try_queue(101).is_none());
        policy.cancel_queued(ordinary, false);
    }

    #[test]
    fn bounded_overweight_progress_can_coexist_with_ordinary_transit() {
        let limits = LIMITS.with_overweight_progress_byte_limit(200);
        let mut policy = PlainDisposalAdmission::new(limits);
        let ordinary = policy.try_queue(40).expect("ordinary transit");
        let mut overweight = policy
            .try_queue_overweight_progress(150)
            .expect("bounded additive overweight transit");

        assert!(policy.snapshot().overweight_exclusive);
        assert_eq!(policy.snapshot().retained_bytes, 190);
        assert_eq!(policy.snapshot().overweight_total_bytes_high_water, 190);
        assert!(policy.try_queue(1).is_none());
        assert!(policy.try_queue_overweight_progress(150).is_none());

        policy.shrink_queued(&mut overweight, 50);
        assert!(!policy.snapshot().overweight_exclusive);
        let active = policy.start(ordinary);
        let resumed = policy
            .try_queue(10)
            .expect("ordinary admission resumes within remaining byte capacity");
        policy.cancel_queued(resumed, false);
        policy.finish(active, false);
        policy.cancel_queued(overweight, false);
    }

    #[test]
    fn unrelated_release_demotes_shrunk_overweight_owner_at_aggregate_ceiling() {
        let limits = LIMITS.with_overweight_progress_byte_limit(200);
        let mut policy = PlainDisposalAdmission::new(limits);
        let first = policy.try_queue(40).expect("first ordinary transit");
        let first = policy.start(first);
        let second = policy.try_queue(40).expect("second ordinary transit");
        let mut overweight = policy
            .try_queue_overweight_progress(150)
            .expect("bounded additive overweight transit");

        policy.shrink_queued(&mut overweight, 50);
        assert!(policy.snapshot().overweight_exclusive);
        policy.finish(first, false);
        assert!(!policy.snapshot().overweight_exclusive);

        let second = policy.start(second);
        let resumed = policy
            .try_queue(10)
            .expect("ordinary admission resumes while the shrunk owner remains");
        policy.cancel_queued(resumed, false);
        policy.finish(second, false);
        policy.cancel_queued(overweight, false);
    }

    #[test]
    fn demoted_overweight_token_cannot_clear_a_new_additive_owner() {
        let limits = LIMITS.with_overweight_progress_byte_limit(200);
        let mut policy = PlainDisposalAdmission::new(limits);
        let ordinary = policy.try_queue(40).expect("ordinary transit");
        let mut first = policy
            .try_queue_overweight_progress(150)
            .expect("first additive owner");

        policy.shrink_queued(&mut first, 50);
        assert!(!policy.snapshot().overweight_exclusive);
        let ordinary = policy.start(ordinary);
        let first = policy.start(first);
        let second = policy
            .try_queue_overweight_progress(150)
            .expect("demoted first token permits one new additive owner");
        assert!(policy.snapshot().overweight_exclusive);

        policy.finish(first, false);
        assert!(policy.snapshot().overweight_exclusive);
        assert!(policy.try_queue_overweight_progress(150).is_none());

        policy.cancel_queued(second, false);
        policy.finish(ordinary, false);
        assert!(!policy.snapshot().overweight_exclusive);
    }

    #[test]
    fn queued_cancellation_and_panicking_terminal_release_exact_accounting() {
        let mut policy = PlainDisposalAdmission::new(LIMITS);
        let cancelled = policy.try_queue(25).expect("queued job");
        policy.cancel_queued(cancelled, true);
        assert_eq!(policy.snapshot().closed_outcomes, 1);
        assert_eq!(policy.snapshot().cancelled_jobs, 1);

        let queued = policy.try_queue(75).expect("active job");
        let active = policy.start(queued);
        policy.finish(active, true);
        let snapshot = policy.snapshot();
        assert_eq!(snapshot.running_jobs, 0);
        assert_eq!(snapshot.queued_jobs, 0);
        assert_eq!(snapshot.retained_bytes, 0);
        assert_eq!(snapshot.completed_jobs, 1);
        assert_eq!(snapshot.panicked_jobs, 1);
    }

    #[test]
    fn completed_result_can_shrink_a_conservative_queued_reservation() {
        let mut policy = PlainDisposalAdmission::new(LIMITS);
        let mut queued = policy.try_queue(90).expect("conservative reservation");

        policy.shrink_queued(&mut queued, 12);

        assert_eq!(policy.snapshot().queued_jobs, 1);
        assert_eq!(policy.snapshot().retained_bytes, 12);
        let active = policy.start(queued);
        policy.finish(active, false);
        assert_eq!(policy.snapshot().retained_bytes, 0);
    }

    #[test]
    fn guarded_replacement_uses_headroom_that_ordinary_owners_cannot_consume() {
        let limits = PlainDisposalLimits::new(1, 2, 100).with_replacement_headroom(1);
        let mut policy = PlainDisposalAdmission::new(limits);
        let first = policy.try_queue(50).expect("first retained owner");
        let second = policy.try_queue(50).expect("second retained owner");

        assert!(
            policy.try_queue(0).is_none(),
            "ordinary admission must leave replacement headroom untouched"
        );
        let replacement = policy
            .try_queue_replacement(50, 50)
            .expect("guarded replacement borrows count and byte headroom");
        let saturated = policy.snapshot();
        assert!(saturated.replacement_headroom_active);
        assert_eq!(saturated.queued_jobs, 3);
        assert_eq!(saturated.retained_bytes, 150);
        assert_eq!(saturated.replacement_bytes_high_water, 150);
        assert!(
            policy.try_queue_replacement(1, 50).is_none(),
            "only one replacement may overlap at a time"
        );

        policy.cancel_queued(second, false);
        assert!(
            !policy.snapshot().replacement_headroom_active,
            "headroom becomes reusable once ownership returns within ordinary limits"
        );
        policy.cancel_queued(replacement, false);
        policy.cancel_queued(first, false);
        assert_eq!(policy.snapshot().retained_bytes, 0);
    }

    #[test]
    fn guarded_replacement_still_obeys_post_replacement_byte_limit() {
        let limits = PlainDisposalLimits::new(1, 2, 100).with_replacement_headroom(1);
        let mut policy = PlainDisposalAdmission::new(limits);
        let current = policy.try_queue(60).expect("current owner");
        let active = policy.start(current);
        let unrelated = policy.try_queue(40).expect("unrelated owner");

        assert!(policy.try_queue_replacement(61, 60).is_none());
        let replacement = policy
            .try_queue_replacement(60, 60)
            .expect("equal-size replacement fits after current-owner credit");
        policy.cancel_queued(replacement, false);
        policy.cancel_queued(unrelated, false);
        policy.finish(active, false);
    }

    #[test]
    fn producer_retains_one_latest_and_teardown_returns_it() {
        let mut latest = PlainDisposalLatest::default();
        assert_eq!(latest.replace(1), None);
        assert_eq!(latest.replace(2), Some(1));
        assert_eq!(latest.replace(3), Some(2));
        assert_eq!(
            latest.snapshot(),
            PlainDisposalLatestSnapshot {
                pending_jobs: 1,
                pending_high_water: 1,
                retry_armed: true,
                retry_high_water: 1,
                replacements: 2,
                cancellations: 0,
            }
        );
        assert_eq!(latest.cancel(), Some(3));
        assert_eq!(latest.snapshot().pending_jobs, 0);
        assert!(!latest.snapshot().retry_armed);
        assert_eq!(latest.snapshot().cancellations, 1);
    }
}

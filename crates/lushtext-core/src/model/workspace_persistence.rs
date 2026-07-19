// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain latest-generation policy for workspace JSON persistence.

use std::time::Duration;

const RETRY_DELAYS: [Duration; 4] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];

/// Typed identity for one requested workspace snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkspacePersistenceGeneration(u64);

impl WorkspacePersistenceGeneration {
    /// Return the scalar generation for diagnostics and deterministic tests.
    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }

    fn next(self) -> Self {
        let next = self.0.wrapping_add(1);
        Self(if next == 0 { 1 } else { next })
    }
}

/// User-safe failed terminal retained until a newer durable success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspacePersistenceFailure {
    /// Generation whose write failed.
    pub generation: WorkspacePersistenceGeneration,
    /// Consecutive failures for the current requested snapshot.
    pub attempts: usize,
    /// Sanitized summary suitable for application feedback.
    pub summary: String,
}

/// Reason a pending generation is allowed to enter the one worker slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePersistenceStartReason {
    /// The normal mutation debounce elapsed.
    Debounce,
    /// A bounded automatic retry delay elapsed.
    RetryWakeup,
    /// The user or a caller explicitly requested a retry.
    ExplicitRetry,
    /// Window close is flushing the newest state without debounce.
    CloseFlush,
}

/// Effect of applying one matching worker terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePersistenceTerminalEffect {
    /// The terminal did not match the current in-flight generation.
    IgnoredStale,
    /// No pending generation remains.
    Settled,
    /// A newer requested generation is ready to start immediately.
    StartNewest,
    /// Retry the current generation after the bounded delay.
    RetryAfter(Duration),
    /// Automatic retries are exhausted; wait for explicit retry, mutation, or close.
    AwaitExplicitRetry,
}

/// Decision used by asynchronous close coordination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePersistenceCloseDecision {
    /// The newest requested generation is already durable.
    Durable,
    /// One matching worker must terminate before close can decide again.
    WaitForInFlight(WorkspacePersistenceGeneration),
    /// Start the newest requested generation immediately, bypassing debounce.
    StartNow(WorkspacePersistenceGeneration),
}

/// Requested, in-flight, durable, and failed workspace-persistence state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspacePersistenceState {
    requested: WorkspacePersistenceGeneration,
    durable: WorkspacePersistenceGeneration,
    in_flight: Option<WorkspacePersistenceGeneration>,
    failed: Option<WorkspacePersistenceFailure>,
}

impl WorkspacePersistenceState {
    /// Advance the newest requested snapshot and wake it independently of old failure state.
    pub fn request_mutation(&mut self) -> WorkspacePersistenceGeneration {
        self.requested = self.requested.next();
        if self
            .failed
            .as_ref()
            .is_some_and(|failure| failure.generation != self.requested)
        {
            self.failed = None;
        }
        self.requested
    }

    /// Start the newest pending generation while preserving non-durable state.
    pub fn start(
        &mut self,
        reason: WorkspacePersistenceStartReason,
    ) -> Option<WorkspacePersistenceGeneration> {
        if self.in_flight.is_some() || self.requested == self.durable {
            return None;
        }
        if self.failed.is_some() && reason == WorkspacePersistenceStartReason::Debounce {
            return None;
        }

        let generation = self.requested;
        self.in_flight = Some(generation);
        Some(generation)
    }

    /// Apply one successful terminal only when it owns the current worker slot.
    pub fn apply_success(
        &mut self,
        generation: WorkspacePersistenceGeneration,
    ) -> WorkspacePersistenceTerminalEffect {
        if self.in_flight != Some(generation) {
            return WorkspacePersistenceTerminalEffect::IgnoredStale;
        }
        self.in_flight = None;
        self.durable = generation;
        self.failed = None;
        if self.requested == self.durable {
            WorkspacePersistenceTerminalEffect::Settled
        } else {
            WorkspacePersistenceTerminalEffect::StartNewest
        }
    }

    /// Apply one failed terminal and choose bounded retry or newest-state progress.
    pub fn apply_failure(
        &mut self,
        generation: WorkspacePersistenceGeneration,
        summary: impl Into<String>,
    ) -> WorkspacePersistenceTerminalEffect {
        if self.in_flight != Some(generation) {
            return WorkspacePersistenceTerminalEffect::IgnoredStale;
        }
        self.in_flight = None;
        if self.requested != generation {
            self.failed = None;
            return WorkspacePersistenceTerminalEffect::StartNewest;
        }

        let attempts = self
            .failed
            .as_ref()
            .filter(|failure| failure.generation == generation)
            .map_or(1, |failure| failure.attempts.saturating_add(1));
        self.failed = Some(WorkspacePersistenceFailure {
            generation,
            attempts,
            summary: summary.into(),
        });
        RETRY_DELAYS
            .get(attempts.saturating_sub(1))
            .copied()
            .map_or(
                WorkspacePersistenceTerminalEffect::AwaitExplicitRetry,
                WorkspacePersistenceTerminalEffect::RetryAfter,
            )
    }

    /// Return whether dirty, in-flight, failed, or retry-waiting work remains.
    #[must_use]
    pub fn has_pending_work(&self) -> bool {
        self.requested != self.durable || self.in_flight.is_some() || self.failed.is_some()
    }

    /// Return whether the current requested generation retains a failed terminal.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.failed.is_some()
    }

    /// Return the newest requested generation.
    #[must_use]
    pub fn requested_generation(&self) -> WorkspacePersistenceGeneration {
        self.requested
    }

    /// Return the newest durably accepted generation.
    #[must_use]
    pub fn durable_generation(&self) -> WorkspacePersistenceGeneration {
        self.durable
    }

    /// Return the generation currently occupying the worker slot.
    #[must_use]
    pub fn in_flight_generation(&self) -> Option<WorkspacePersistenceGeneration> {
        self.in_flight
    }

    /// Return the retained current failure, if any.
    #[must_use]
    pub fn failure(&self) -> Option<&WorkspacePersistenceFailure> {
        self.failed.as_ref()
    }

    /// Decide how close should flush the newest requested snapshot.
    #[must_use]
    pub fn close_decision(&self) -> WorkspacePersistenceCloseDecision {
        if let Some(generation) = self.in_flight {
            WorkspacePersistenceCloseDecision::WaitForInFlight(generation)
        } else if self.requested == self.durable {
            WorkspacePersistenceCloseDecision::Durable
        } else {
            WorkspacePersistenceCloseDecision::StartNow(self.requested)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_a_write_does_not_make_it_durable() {
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(generation)
        );
        assert!(state.has_pending_work());
        assert_eq!(
            state.durable_generation(),
            WorkspacePersistenceGeneration::default()
        );
    }

    #[test]
    fn older_success_schedules_the_newest_requested_generation() {
        let mut state = WorkspacePersistenceState::default();
        let older = state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(older)
        );
        let newer = state.request_mutation();
        assert_eq!(
            state.apply_success(older),
            WorkspacePersistenceTerminalEffect::StartNewest
        );
        assert_eq!(state.requested_generation(), newer);
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(newer)
        );
    }

    #[test]
    fn current_failure_stays_pending_and_uses_bounded_backoff() {
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        for expected_delay in RETRY_DELAYS {
            assert_eq!(
                state.start(if state.failure().is_some() {
                    WorkspacePersistenceStartReason::RetryWakeup
                } else {
                    WorkspacePersistenceStartReason::Debounce
                }),
                Some(generation)
            );
            assert_eq!(
                state.apply_failure(generation, "Workspace changes could not be saved."),
                WorkspacePersistenceTerminalEffect::RetryAfter(expected_delay)
            );
            assert!(state.has_pending_work());
            assert!(state.is_failed());
            assert_eq!(state.start(WorkspacePersistenceStartReason::Debounce), None);
        }

        assert_eq!(
            state.start(WorkspacePersistenceStartReason::RetryWakeup),
            Some(generation)
        );
        assert_eq!(
            state.apply_failure(generation, "Workspace changes could not be saved."),
            WorkspacePersistenceTerminalEffect::AwaitExplicitRetry
        );
        assert_eq!(state.failure().map(|failure| failure.attempts), Some(5));
    }

    #[test]
    fn newer_mutation_wakes_progress_after_an_older_failure() {
        let mut state = WorkspacePersistenceState::default();
        let failed = state.request_mutation();
        state.start(WorkspacePersistenceStartReason::Debounce);
        state.apply_failure(failed, "failed");
        let newest = state.request_mutation();
        assert!(!state.is_failed());
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(newest)
        );
    }

    #[test]
    fn close_bypasses_debounce_and_waits_for_inflight_work() {
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        assert_eq!(
            state.close_decision(),
            WorkspacePersistenceCloseDecision::StartNow(generation)
        );
        state.start(WorkspacePersistenceStartReason::CloseFlush);
        assert_eq!(
            state.close_decision(),
            WorkspacePersistenceCloseDecision::WaitForInFlight(generation)
        );
        state.apply_success(generation);
        assert_eq!(
            state.close_decision(),
            WorkspacePersistenceCloseDecision::Durable
        );
    }

    #[test]
    fn stale_terminals_cannot_mutate_current_state() {
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        state.start(WorkspacePersistenceStartReason::Debounce);
        let stale = WorkspacePersistenceGeneration(generation.value().saturating_add(1));
        assert_eq!(
            state.apply_success(stale),
            WorkspacePersistenceTerminalEffect::IgnoredStale
        );
        assert_eq!(state.in_flight_generation(), Some(generation));
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust policy for transient editor file-load ownership.
//!
//! GTK adapters keep weak page references and dispatch workers, while this
//! module owns byte weights, queue fairness, exclusive oversize admission, and
//! exact-once permit accounting without knowing about widgets or threads.

use std::collections::BTreeMap;

/// Shared transient payload budget for ordinary file loads.
///
/// This matches the live-editor upper budget so ordinary restore bursts cannot
/// retain more document-sized source/decode/install ownership than one full
/// steady-state editor budget. A supported request above this charge remains
/// possible, but only as the sole admitted payload.
pub const TRANSIENT_LOAD_SHARED_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

/// Fixed per-load allowance for result metadata and allocator slack.
pub const TRANSIENT_LOAD_FIXED_OVERHEAD_BYTES: u64 = 1024 * 1024;

/// Conservative source-byte multiplier across read, decode, and GTK install.
///
/// Eight covers one raw source byte, up to three UTF-8 bytes after a legacy
/// single-byte decode, and the live editor's four-byte-per-character residency
/// estimate while decoded ownership overlaps installation. It is a policy
/// bound rather than an allocator/RSS measurement.
pub const TRANSIENT_LOAD_SOURCE_MULTIPLIER: u64 = 8;

/// Maximum UTF-8 body bytes produced from one admitted source byte.
///
/// The supported legacy single-byte encodings can expand one source byte to
/// one three-byte Unicode scalar. Reserving this bound before decode ensures a
/// successful body never waits unguarded for worker-side disposal capacity.
pub const DECODED_BODY_SOURCE_MULTIPLIER: u64 = 3;

/// Decoded content up to this size may be installed in one GTK turn.
pub const SYNCHRONOUS_INSTALL_THRESHOLD_BYTES: usize =
    super::buffer_replacement::SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES;

/// Maximum decoded UTF-8 bytes considered by one chunked GTK install turn.
pub const INSTALL_SLICE_BYTES: usize = super::buffer_replacement::REPLACEMENT_INSERT_SLICE_BYTES;

/// Match the generic worker cap without making workers wait for admission.
pub const MAX_ADMITTED_FILE_LOADS: usize = 8;

/// Bound consecutive times active-tab priority may bypass the oldest request.
const MAX_CONSECUTIVE_ACTIVE_BYPASSES: u8 = 2;

/// Calculate one conservative transient charge with saturating arithmetic.
#[must_use]
pub const fn transient_load_weight(source_bytes: u64) -> u64 {
    source_bytes
        .saturating_mul(TRANSIENT_LOAD_SOURCE_MULTIPLIER)
        .saturating_add(TRANSIENT_LOAD_FIXED_OVERHEAD_BYTES)
}

/// Calculate the conservative future-disposal charge for decoded text.
#[must_use]
pub const fn decoded_body_reservation_weight(source_bytes: u64) -> u64 {
    source_bytes.saturating_mul(DECODED_BODY_SOURCE_MULTIPLIER)
}

/// Current UI priority for one compact queued request.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum FileLoadPriority {
    /// Background or non-selected tab.
    #[default]
    Normal,
    /// Currently selected editor tab.
    Active,
}

/// Scalar request retained before payload work is admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileLoadAdmissionRequest {
    pub request_id: u64,
    pub owner_id: u64,
    pub sequence: u64,
    pub weight: u64,
    pub priority: FileLoadPriority,
}

/// One admitted payload charge that must later be released exactly once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileLoadAdmissionGrant {
    pub request_id: u64,
    pub weight: u64,
    pub exclusive: bool,
}

/// Observable scalar accounting for tests, diagnostics, and smoke evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FileLoadAdmissionSnapshot {
    pub queued_count: usize,
    pub active_count: usize,
    pub active_weight: u64,
    pub high_water_weight: u64,
    pub exclusive_active: bool,
}

/// Deterministic byte-weighted queue and active-permit accounting.
#[derive(Debug)]
pub struct FileLoadAdmissionPolicy {
    budget: u64,
    max_active: usize,
    queued: BTreeMap<(u64, u64), FileLoadAdmissionRequest>,
    queued_by_id: BTreeMap<u64, (u64, u64)>,
    active: BTreeMap<u64, FileLoadAdmissionGrant>,
    active_weight: u64,
    high_water_weight: u64,
    exclusive_active: bool,
    consecutive_active_bypasses: u8,
}

impl Default for FileLoadAdmissionPolicy {
    fn default() -> Self {
        Self::new(TRANSIENT_LOAD_SHARED_BUDGET_BYTES, MAX_ADMITTED_FILE_LOADS)
    }
}

impl FileLoadAdmissionPolicy {
    /// Build a policy with explicit bounds for deterministic tests and tools.
    #[must_use]
    pub const fn new(budget: u64, max_active: usize) -> Self {
        Self {
            budget,
            max_active,
            queued: BTreeMap::new(),
            queued_by_id: BTreeMap::new(),
            active: BTreeMap::new(),
            active_weight: 0,
            high_water_weight: 0,
            exclusive_active: false,
            consecutive_active_bypasses: 0,
        }
    }

    /// Queue or replace one scalar request without admitting payload ownership.
    pub fn queue(&mut self, request: FileLoadAdmissionRequest) {
        self.cancel_queued(request.request_id);
        let key = (request.sequence, request.request_id);
        self.queued.insert(key, request);
        self.queued_by_id.insert(request.request_id, key);
    }

    /// Remove a request that has not yet acquired payload ownership.
    pub fn cancel_queued(&mut self, request_id: u64) -> bool {
        self.queued_by_id
            .remove(&request_id)
            .and_then(|key| self.queued.remove(&key))
            .is_some()
    }

    /// Refresh selected-tab priority without changing sequence fairness.
    pub fn update_priority(&mut self, request_id: u64, priority: FileLoadPriority) -> bool {
        let Some(key) = self.queued_by_id.get(&request_id).copied() else {
            return false;
        };
        let Some(request) = self.queued.get_mut(&key) else {
            return false;
        };
        request.priority = priority;
        true
    }

    /// Admit at most one current request.
    ///
    /// When protected live residency is already above its preferred bound, one
    /// payload may still make progress, but a second waits until it releases.
    /// The oldest capacity-blocked request also prevents later small requests
    /// from starving it indefinitely.
    pub fn admit_next(
        &mut self,
        protected_residency_over_budget: bool,
    ) -> Option<FileLoadAdmissionGrant> {
        self.admit_next_with_external(protected_residency_over_budget, 0, false)
    }

    /// Admit at most one request while accounting another transient lane.
    pub fn admit_next_with_external(
        &mut self,
        protected_residency_over_budget: bool,
        external_active_weight: u64,
        external_exclusive_active: bool,
    ) -> Option<FileLoadAdmissionGrant> {
        if self.queued.is_empty()
            || self.max_active == 0
            || self.active.len() >= self.max_active
            || self.exclusive_active
            || external_exclusive_active
            || (protected_residency_over_budget
                && (!self.active.is_empty() || external_active_weight > 0))
        {
            return None;
        }

        let (&oldest_key, &oldest_request) = self.queued.first_key_value()?;
        if !self.request_fits(oldest_request, external_active_weight) {
            return None;
        }
        let chosen_key = if oldest_request.priority == FileLoadPriority::Normal
            && self.consecutive_active_bypasses < MAX_CONSECUTIVE_ACTIVE_BYPASSES
        {
            self.queued
                .iter()
                .skip(1)
                .find(|(_, request)| {
                    request.priority == FileLoadPriority::Active
                        && self.request_fits(**request, external_active_weight)
                })
                .map_or(oldest_key, |(key, _)| *key)
        } else {
            oldest_key
        };

        if chosen_key == oldest_key {
            self.consecutive_active_bypasses = 0;
        } else {
            self.consecutive_active_bypasses = self.consecutive_active_bypasses.saturating_add(1);
        }

        let request = self.queued.remove(&chosen_key)?;
        self.queued_by_id.remove(&request.request_id);
        let exclusive = request.weight > self.budget;
        let grant = FileLoadAdmissionGrant {
            request_id: request.request_id,
            weight: request.weight,
            exclusive,
        };
        self.active_weight = self.active_weight.saturating_add(request.weight);
        self.high_water_weight = self.high_water_weight.max(self.active_weight);
        self.exclusive_active = exclusive;
        self.active.insert(request.request_id, grant);
        Some(grant)
    }

    /// Release one active request, returning false for stale or duplicate drops.
    pub fn release(&mut self, request_id: u64) -> bool {
        let Some(grant) = self.active.remove(&request_id) else {
            return false;
        };
        self.active_weight = self.active_weight.saturating_sub(grant.weight);
        if grant.exclusive {
            self.exclusive_active = false;
        }
        true
    }

    /// Return scalar state without exposing queue or permit internals.
    #[must_use]
    pub fn snapshot(&self) -> FileLoadAdmissionSnapshot {
        FileLoadAdmissionSnapshot {
            queued_count: self.queued.len(),
            active_count: self.active.len(),
            active_weight: self.active_weight,
            high_water_weight: self.high_water_weight,
            exclusive_active: self.exclusive_active,
        }
    }

    fn request_fits(&self, request: FileLoadAdmissionRequest, external_weight: u64) -> bool {
        if request.weight > self.budget {
            self.active.is_empty() && external_weight == 0
        } else {
            self.active_weight
                .saturating_add(external_weight)
                .saturating_add(request.weight)
                <= self.budget
        }
    }
}

/// Return the next UTF-8-safe byte boundary for one install slice.
#[must_use]
pub fn next_install_boundary(text: &str, start: usize) -> usize {
    super::buffer_replacement::next_replacement_boundary(text, start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        request_id: u64,
        owner_id: u64,
        sequence: u64,
        weight: u64,
        priority: FileLoadPriority,
    ) -> FileLoadAdmissionRequest {
        FileLoadAdmissionRequest {
            request_id,
            owner_id,
            sequence,
            weight,
            priority,
        }
    }

    #[test]
    fn weight_and_install_bounds_saturate_and_preserve_unicode() {
        assert_eq!(
            transient_load_weight(10),
            TRANSIENT_LOAD_FIXED_OVERHEAD_BYTES + 10 * TRANSIENT_LOAD_SOURCE_MULTIPLIER
        );
        assert_eq!(transient_load_weight(u64::MAX), u64::MAX);
        assert_eq!(decoded_body_reservation_weight(10), 30);
        assert_eq!(decoded_body_reservation_weight(u64::MAX), u64::MAX);

        let text = format!("{}\n🙂tail", "a".repeat(INSTALL_SLICE_BYTES - 1));
        let end = next_install_boundary(&text, 0);
        assert!(text.is_char_boundary(end));
        assert_eq!(end, INSTALL_SLICE_BYTES);
        assert!(text[..end].ends_with('\n'));
        assert_eq!(next_install_boundary(&text, text.len()), text.len());
    }

    #[test]
    fn ordinary_requests_share_budget_and_release_exactly_once() {
        let mut policy = FileLoadAdmissionPolicy::new(10, 8);
        policy.queue(request(1, 1, 1, 4, FileLoadPriority::Normal));
        policy.queue(request(2, 2, 2, 6, FileLoadPriority::Normal));

        assert_eq!(
            policy.admit_next(false).map(|grant| grant.request_id),
            Some(1)
        );
        assert_eq!(
            policy.admit_next(false).map(|grant| grant.request_id),
            Some(2)
        );
        assert_eq!(policy.snapshot().active_weight, 10);
        assert!(policy.release(1));
        assert!(!policy.release(1));
        assert!(policy.release(2));
        assert_eq!(policy.snapshot().active_weight, 0);
    }

    #[test]
    fn exclusive_oversize_runs_alone_and_blocks_later_small_work() {
        let mut policy = FileLoadAdmissionPolicy::new(10, 8);
        policy.queue(request(1, 1, 1, 11, FileLoadPriority::Normal));
        policy.queue(request(2, 2, 2, 1, FileLoadPriority::Normal));

        let exclusive = policy.admit_next(false).expect("exclusive grant");
        assert!(exclusive.exclusive);
        assert!(policy.admit_next(false).is_none());
        assert!(policy.release(exclusive.request_id));
        assert_eq!(
            policy.admit_next(false).map(|grant| grant.request_id),
            Some(2)
        );
    }

    #[test]
    fn capacity_blocked_oldest_is_not_starved_by_later_small_requests() {
        let mut policy = FileLoadAdmissionPolicy::new(10, 8);
        policy.queue(request(1, 1, 1, 6, FileLoadPriority::Normal));
        policy.queue(request(2, 2, 2, 6, FileLoadPriority::Normal));
        policy.queue(request(3, 3, 3, 4, FileLoadPriority::Normal));

        assert_eq!(
            policy.admit_next(false).map(|grant| grant.request_id),
            Some(1)
        );
        assert!(policy.admit_next(false).is_none());
        assert!(policy.release(1));
        assert_eq!(
            policy.admit_next(false).map(|grant| grant.request_id),
            Some(2)
        );
    }

    #[test]
    fn active_priority_is_bounded_by_fifo_fairness() {
        let mut policy = FileLoadAdmissionPolicy::new(1, 1);
        policy.queue(request(1, 1, 1, 1, FileLoadPriority::Normal));
        policy.queue(request(2, 2, 2, 1, FileLoadPriority::Active));
        policy.queue(request(3, 3, 3, 1, FileLoadPriority::Active));
        policy.queue(request(4, 4, 4, 1, FileLoadPriority::Active));

        for expected in [2, 3, 1, 4] {
            let grant = policy.admit_next(false).expect("grant");
            assert_eq!(grant.request_id, expected);
            assert!(policy.release(grant.request_id));
        }
    }

    #[test]
    fn active_priority_can_change_while_queued() {
        let mut policy = FileLoadAdmissionPolicy::new(1, 1);
        policy.queue(request(1, 1, 1, 1, FileLoadPriority::Normal));
        policy.queue(request(2, 2, 2, 1, FileLoadPriority::Normal));
        assert!(policy.update_priority(2, FileLoadPriority::Active));
        assert_eq!(
            policy.admit_next(false).map(|grant| grant.request_id),
            Some(2)
        );
    }

    #[test]
    fn protected_over_budget_state_allows_one_payload_then_waits() {
        let mut policy = FileLoadAdmissionPolicy::new(10, 8);
        policy.queue(request(1, 1, 1, 4, FileLoadPriority::Normal));
        policy.queue(request(2, 2, 2, 4, FileLoadPriority::Normal));

        let first = policy.admit_next(true).expect("first progress grant");
        assert!(policy.admit_next(true).is_none());
        assert!(policy.release(first.request_id));
        assert!(policy.admit_next(true).is_some());
    }

    #[test]
    fn external_save_pressure_shares_capacity_and_exclusivity() {
        let mut policy = FileLoadAdmissionPolicy::new(10, 8);
        policy.queue(request(1, 1, 1, 6, FileLoadPriority::Active));
        assert!(policy.admit_next_with_external(false, 5, false).is_none());
        assert!(policy.admit_next_with_external(false, 4, false).is_some());

        let mut policy = FileLoadAdmissionPolicy::new(10, 8);
        policy.queue(request(2, 2, 2, 1, FileLoadPriority::Active));
        assert!(policy.admit_next_with_external(false, 0, true).is_none());
    }

    #[test]
    fn stale_requests_cancel_without_consuming_budget() {
        let mut policy = FileLoadAdmissionPolicy::new(10, 8);
        policy.queue(request(1, 10, 1, 4, FileLoadPriority::Normal));
        policy.queue(request(2, 20, 2, 4, FileLoadPriority::Normal));
        assert!(policy.cancel_queued(1));
        assert!(!policy.cancel_queued(1));
        assert_eq!(
            policy.admit_next(false).map(|grant| grant.request_id),
            Some(2)
        );
        assert_eq!(policy.snapshot().high_water_weight, 4);
    }

    #[test]
    fn multi_owner_restore_interleaving_tracks_high_water() {
        let mut policy = FileLoadAdmissionPolicy::new(12, 2);
        for (id, owner, sequence) in [(1, 100, 1), (2, 200, 2), (3, 100, 3), (4, 200, 4)] {
            policy.queue(request(id, owner, sequence, 6, FileLoadPriority::Normal));
        }

        let first = policy.admit_next(false).expect("first window grant");
        let second = policy.admit_next(false).expect("second window grant");
        assert_eq!((first.request_id, second.request_id), (1, 2));
        assert!(policy.admit_next(false).is_none());
        assert_eq!(policy.snapshot().high_water_weight, 12);

        assert!(policy.release(second.request_id));
        let third = policy.admit_next(false).expect("third restore grant");
        assert_eq!(third.request_id, 3);
        assert!(policy.release(first.request_id));
        assert!(policy.release(third.request_id));
        assert_eq!(
            policy.admit_next(false).map(|grant| grant.request_id),
            Some(4)
        );
    }
}

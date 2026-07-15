// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust policy for transient editor-save payload admission.
//!
//! GTK adapters retain weak editor ownership and compact request metadata until
//! this policy admits the document-sized snapshot/transform/write lifecycle.

use std::collections::BTreeMap;

use super::file_load::TRANSIENT_LOAD_SHARED_BUDGET_BYTES;

/// Process-wide ordinary budget shared with transient editor file loads.
pub const SAVE_PAYLOAD_SHARED_BUDGET_BYTES: u64 = TRANSIENT_LOAD_SHARED_BUDGET_BYTES;

/// Fixed allowance for writer metadata, encoder state, and allocator slack.
pub const SAVE_PAYLOAD_FIXED_OVERHEAD_BYTES: u64 = 1024 * 1024;

/// Worst-case overlap relative to the live editor's conservative residency.
///
/// The charge covers the captured UTF-8 body, formatting output, line-ending
/// normalization, encoded bytes, and retained clean/history state. It is a
/// deterministic policy bound rather than an RSS estimate.
pub const SAVE_PAYLOAD_RESIDENCY_MULTIPLIER: u64 = 8;

/// Match the bounded background executor without making workers wait on bytes.
pub const MAX_ADMITTED_SAVE_PAYLOADS: usize = 8;

/// Calculate one conservative save charge with saturating arithmetic.
#[must_use]
pub const fn conservative_save_payload_weight(live_residency_bytes: u64) -> u64 {
    live_residency_bytes
        .saturating_mul(SAVE_PAYLOAD_RESIDENCY_MULTIPLIER)
        .saturating_add(SAVE_PAYLOAD_FIXED_OVERHEAD_BYTES)
}

/// User-work priority for one compact queued save request.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum SaveAdmissionPriority {
    /// An explicit ordinary save while the editor remains open.
    #[default]
    Ordinary,
    /// A selected save that gates tab or window closure.
    Close,
}

/// Scalar identity retained before document payload ownership is admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaveAdmissionRequest {
    pub request_id: u64,
    pub owner_id: u64,
    pub save_generation: u64,
    pub destination_identity: u64,
    pub close_session_identity: Option<u64>,
    pub sequence: u64,
    pub weight: u64,
    pub priority: SaveAdmissionPriority,
}

/// Other process-owned document payloads that share the transient budget.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExternalTransientPressure {
    pub active_weight: u64,
    pub exclusive_active: bool,
    pub protected_residency_over_budget: bool,
}

/// One admitted payload charge that must later be released exactly once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaveAdmissionGrant {
    pub request_id: u64,
    pub weight: u64,
    pub exclusive: bool,
    pub priority: SaveAdmissionPriority,
}

/// Scalar accounting exposed to tests, diagnostics, and benchmark evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaveAdmissionSnapshot {
    pub queued_count: usize,
    pub queued_close_count: usize,
    pub active_count: usize,
    pub active_close_count: usize,
    pub active_weight: u64,
    pub high_water_weight: u64,
    pub high_water_combined_weight: u64,
    pub exclusive_active: bool,
}

/// Deterministic byte-weighted queue and active-permit accounting.
#[derive(Debug)]
pub struct SaveAdmissionPolicy {
    budget: u64,
    max_active: usize,
    queued: BTreeMap<(u64, u64), SaveAdmissionRequest>,
    queued_by_id: BTreeMap<u64, (u64, u64)>,
    active: BTreeMap<u64, SaveAdmissionGrant>,
    active_weight: u64,
    high_water_weight: u64,
    high_water_combined_weight: u64,
    exclusive_active: bool,
}

impl Default for SaveAdmissionPolicy {
    fn default() -> Self {
        Self::new(SAVE_PAYLOAD_SHARED_BUDGET_BYTES, MAX_ADMITTED_SAVE_PAYLOADS)
    }
}

impl SaveAdmissionPolicy {
    /// Build a policy with explicit bounds for deterministic tests and probes.
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
            high_water_combined_weight: 0,
            exclusive_active: false,
        }
    }

    /// Queue or replace one compact request without acquiring payload bytes.
    pub fn queue(&mut self, request: SaveAdmissionRequest) {
        self.cancel_queued(request.request_id);
        let key = (request.sequence, request.request_id);
        self.queued.insert(key, request);
        self.queued_by_id.insert(request.request_id, key);
    }

    /// Replace current scalar freshness/weight fields without changing fairness.
    pub fn refresh_queued(
        &mut self,
        request_id: u64,
        save_generation: u64,
        destination_identity: u64,
        weight: u64,
    ) -> bool {
        let Some(key) = self.queued_by_id.get(&request_id).copied() else {
            return false;
        };
        let Some(request) = self.queued.get_mut(&key) else {
            return false;
        };
        request.save_generation = save_generation;
        request.destination_identity = destination_identity;
        request.weight = weight;
        true
    }

    /// Remove a request that has not acquired document payload ownership.
    pub fn cancel_queued(&mut self, request_id: u64) -> bool {
        self.queued_by_id
            .remove(&request_id)
            .and_then(|key| self.queued.remove(&key))
            .is_some()
    }

    /// Admit at most one current request under shared transient pressure.
    ///
    /// Close work may bypass older ordinary saves, but sequence remains FIFO
    /// within each priority. An overweight request runs only when every shared
    /// document payload lane is otherwise idle.
    pub fn admit_next(
        &mut self,
        external: ExternalTransientPressure,
    ) -> Option<SaveAdmissionGrant> {
        if self.queued.is_empty()
            || self.max_active == 0
            || self.active.len() >= self.max_active
            || self.exclusive_active
            || external.exclusive_active
            || (external.protected_residency_over_budget
                && (!self.active.is_empty() || external.active_weight > 0))
        {
            return None;
        }

        let chosen_key = self
            .queued
            .iter()
            .find(|(_, request)| {
                request.priority == SaveAdmissionPriority::Close
                    && self.request_fits(**request, external.active_weight)
            })
            .or_else(|| {
                let first = self.queued.first_key_value()?;
                self.request_fits(*first.1, external.active_weight)
                    .then_some(first)
            })
            .map(|(key, _)| *key)?;

        let request = self.queued.remove(&chosen_key)?;
        self.queued_by_id.remove(&request.request_id);
        let exclusive = request.weight > self.budget;
        let grant = SaveAdmissionGrant {
            request_id: request.request_id,
            weight: request.weight,
            exclusive,
            priority: request.priority,
        };
        self.active_weight = self.active_weight.saturating_add(request.weight);
        self.high_water_weight = self.high_water_weight.max(self.active_weight);
        self.high_water_combined_weight = self
            .high_water_combined_weight
            .max(self.active_weight.saturating_add(external.active_weight));
        self.exclusive_active = exclusive;
        self.active.insert(request.request_id, grant);
        Some(grant)
    }

    /// Release one active charge, returning false for stale or duplicate drops.
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

    /// Return bounded scalar state without exposing queue/payload ownership.
    #[must_use]
    pub fn snapshot(&self) -> SaveAdmissionSnapshot {
        SaveAdmissionSnapshot {
            queued_count: self.queued.len(),
            queued_close_count: self
                .queued
                .values()
                .filter(|request| request.priority == SaveAdmissionPriority::Close)
                .count(),
            active_count: self.active.len(),
            active_close_count: self
                .active
                .values()
                .filter(|grant| grant.priority == SaveAdmissionPriority::Close)
                .count(),
            active_weight: self.active_weight,
            high_water_weight: self.high_water_weight,
            high_water_combined_weight: self.high_water_combined_weight,
            exclusive_active: self.exclusive_active,
        }
    }

    fn request_fits(&self, request: SaveAdmissionRequest, external_weight: u64) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        request_id: u64,
        sequence: u64,
        weight: u64,
        priority: SaveAdmissionPriority,
    ) -> SaveAdmissionRequest {
        SaveAdmissionRequest {
            request_id,
            owner_id: request_id + 100,
            save_generation: request_id + 200,
            destination_identity: request_id + 300,
            close_session_identity: (priority == SaveAdmissionPriority::Close).then_some(9),
            sequence,
            weight,
            priority,
        }
    }

    #[test]
    fn conservative_charge_is_saturating() {
        assert_eq!(
            conservative_save_payload_weight(10),
            SAVE_PAYLOAD_FIXED_OVERHEAD_BYTES + 10 * SAVE_PAYLOAD_RESIDENCY_MULTIPLIER
        );
        assert_eq!(conservative_save_payload_weight(u64::MAX), u64::MAX);
    }

    #[test]
    fn ordinary_saves_share_weighted_capacity_and_release_exactly_once() {
        let mut policy = SaveAdmissionPolicy::new(10, 8);
        policy.queue(request(1, 1, 4, SaveAdmissionPriority::Ordinary));
        policy.queue(request(2, 2, 6, SaveAdmissionPriority::Ordinary));

        assert_eq!(
            policy
                .admit_next(ExternalTransientPressure::default())
                .map(|grant| grant.request_id),
            Some(1)
        );
        assert_eq!(
            policy
                .admit_next(ExternalTransientPressure::default())
                .map(|grant| grant.request_id),
            Some(2)
        );
        assert_eq!(policy.snapshot().active_weight, 10);
        assert!(policy.release(1));
        assert!(!policy.release(1));
        assert!(policy.release(2));
    }

    #[test]
    fn one_overweight_save_runs_exclusively_across_external_pressure() {
        let mut policy = SaveAdmissionPolicy::new(10, 8);
        policy.queue(request(1, 1, 11, SaveAdmissionPriority::Ordinary));
        assert!(
            policy
                .admit_next(ExternalTransientPressure {
                    active_weight: 1,
                    ..ExternalTransientPressure::default()
                })
                .is_none()
        );

        let grant = policy
            .admit_next(ExternalTransientPressure::default())
            .expect("exclusive save grant");
        assert!(grant.exclusive);
        policy.queue(request(2, 2, 1, SaveAdmissionPriority::Close));
        assert!(
            policy
                .admit_next(ExternalTransientPressure::default())
                .is_none()
        );
        assert!(policy.release(grant.request_id));
    }

    #[test]
    fn close_save_bypasses_older_ordinary_work() {
        let mut policy = SaveAdmissionPolicy::new(10, 1);
        policy.queue(request(1, 1, 10, SaveAdmissionPriority::Ordinary));
        policy.queue(request(2, 2, 10, SaveAdmissionPriority::Close));

        assert_eq!(
            policy
                .admit_next(ExternalTransientPressure::default())
                .map(|grant| grant.request_id),
            Some(2)
        );
    }

    #[test]
    fn stale_compact_request_can_be_refreshed_or_cancelled_without_payload() {
        let mut policy = SaveAdmissionPolicy::new(10, 1);
        policy.queue(request(1, 1, 4, SaveAdmissionPriority::Ordinary));
        assert!(policy.refresh_queued(1, 99, 77, 6));
        assert_eq!(policy.snapshot().queued_count, 1);
        assert_eq!(policy.snapshot().active_weight, 0);
        assert!(policy.cancel_queued(1));
        assert!(!policy.cancel_queued(1));
        assert!(
            policy
                .admit_next(ExternalTransientPressure::default())
                .is_none()
        );
    }

    #[test]
    fn external_weight_is_included_in_capacity_and_high_water() {
        let mut policy = SaveAdmissionPolicy::new(10, 2);
        policy.queue(request(1, 1, 6, SaveAdmissionPriority::Ordinary));
        let pressure = ExternalTransientPressure {
            active_weight: 5,
            ..ExternalTransientPressure::default()
        };
        assert!(policy.admit_next(pressure).is_none());

        let pressure = ExternalTransientPressure {
            active_weight: 4,
            ..ExternalTransientPressure::default()
        };
        assert!(policy.admit_next(pressure).is_some());
        assert_eq!(policy.snapshot().high_water_combined_weight, 10);
    }

    #[test]
    fn protected_residency_allows_only_one_shared_payload() {
        let mut policy = SaveAdmissionPolicy::new(10, 2);
        policy.queue(request(1, 1, 4, SaveAdmissionPriority::Close));
        policy.queue(request(2, 2, 4, SaveAdmissionPriority::Ordinary));
        let pressure = ExternalTransientPressure {
            protected_residency_over_budget: true,
            ..ExternalTransientPressure::default()
        };

        let first = policy.admit_next(pressure).expect("one progress grant");
        assert!(policy.admit_next(pressure).is_none());
        assert!(policy.release(first.request_id));
    }
}

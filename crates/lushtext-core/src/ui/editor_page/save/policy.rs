// SPDX-License-Identifier: GPL-3.0-or-later

//! The document-save workflow's pure decisions.
//!
//! Everything here is plain Rust: no `gtk4`, `glib`, `gio`, `libadwaita`, or
//! `sourceview5` import may appear, because that purity is what keeps the module
//! inside the default mutation scope (`ui/**/policy.rs`) and what lets these
//! decisions be tested without a widget, a compositor, or a tempdir.
//!
//! Three groups live here:
//!
//! - **Payload admission.** The process-wide byte-weighted queue and active-permit
//!   accounting ([`SaveAdmissionPolicy`]). GTK adapters retain weak editor
//!   ownership and compact request metadata until this policy admits the
//!   document-sized snapshot/transform/write lifecycle.
//! - **The admission seam.** `QueuedSaveTicket` captures what the workflow
//!   expected when the save was queued, `QueuedSaveFacts` captures the live
//!   editor state observed when admission is decided, and
//!   `queued_save_is_current` validates the pair as a unit.
//! - **Save-stage decisions.** Whether a save may pre-empt an in-flight load,
//!   whether the worker's formatted text must be mirrored back into the buffer,
//!   and how the write outcome is retained.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::model::file_load::TRANSIENT_LOAD_SHARED_BUDGET_BYTES;

/// Process-wide ordinary budget shared with transient editor file loads.
pub const SAVE_PAYLOAD_SHARED_BUDGET_BYTES: u64 = TRANSIENT_LOAD_SHARED_BUDGET_BYTES;

/// Fixed allowance for writer metadata, encoder state, and allocator slack.
pub(crate) const SAVE_PAYLOAD_FIXED_OVERHEAD_BYTES: u64 = 1024 * 1024;

/// Worst-case overlap relative to the live editor's conservative residency.
///
/// The charge covers the captured UTF-8 body, formatting output, line-ending
/// normalization, encoded bytes, and retained clean/history state. It is a
/// deterministic policy bound rather than an RSS estimate.
pub(crate) const SAVE_PAYLOAD_RESIDENCY_MULTIPLIER: u64 = 8;

/// Match the bounded background executor without making workers wait on bytes.
pub(crate) const MAX_ADMITTED_SAVE_PAYLOADS: usize = 8;

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
    pub(crate) const fn new(budget: u64, max_active: usize) -> Self {
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
    pub(crate) fn refresh_queued(
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
    pub(crate) fn cancel_queued(&mut self, request_id: u64) -> bool {
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
    pub(crate) fn release(&mut self, request_id: u64) -> bool {
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

/// Whether a save may pre-empt an in-flight load instead of refusing.
///
/// This is the **named derivation** half of the archetype-defect fix, and it is
/// deliberately not the load-bearing half. One request property — the user gave
/// this save an explicit destination — drives two different stages, and before
/// this function existed the second stage read the first stage's field directly
/// under a different name (`cancel_pending_load` was passed positionally into a
/// parameter called `explicit_destination`).
///
/// What makes that mismatch impossible now is `QueuedSaveTicket`: the
/// freshness predicate takes the ticket instead of five positional scalars, so
/// the miswired call is a type error. This function is `bool -> bool` and proves
/// nothing to the compiler; what it adds is that the inference lives in the code
/// under a name, rather than being implied by one boolean serving two meanings.
///
/// The reasoning: a save with an explicit destination does not depend on the
/// in-flight load's result, because the user named the target and the buffer is
/// already what they want written — so the load is cancelled and the save
/// proceeds. A save *without* an explicit destination targets the very path the
/// load is still establishing, so it must refuse rather than write bytes derived
/// from a half-installed buffer.
#[must_use]
pub(crate) const fn save_may_preempt_pending_load(explicit_destination: bool) -> bool {
    explicit_destination
}

/// What the workflow expected when one save was queued.
///
/// Constructed once at the workflow entry point and carried, as a unit, through
/// the queue, the stale-request drain, and admission. Nothing downstream
/// rebuilds it clause by clause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueuedSaveTicket {
    /// The editor's save generation at the moment this request was queued.
    pub save_generation: u64,
    /// The destination this request was queued to write.
    pub path: PathBuf,
    /// Whether the user named the destination, as Save As does.
    ///
    /// Never spell this `cancel_pending_load`: that names one consequence of the
    /// property rather than the property, and carrying both names for one value
    /// is what made the original defect invisible. Derive the consequence with
    /// `save_may_preempt_pending_load`.
    pub explicit_destination: bool,
    /// Whether the request required the buffer to be modified when queued.
    pub required_modified: bool,
    /// The close session this save gates, when a close-with-changes owns it.
    pub close_session_identity: Option<u64>,
}

/// Live editor state observed when a queued save's freshness is decided.
///
/// Captured on the GTK side against a ticket, so `close_session_current` already
/// answers *this* ticket's identity question and is `true` when the ticket names
/// no close session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueuedSaveFacts {
    /// Whether the editor still reports an in-flight save.
    pub saving: bool,
    /// The editor's current save generation.
    pub save_generation: u64,
    /// Whether the buffer is currently modified.
    pub modified: bool,
    /// The editor's current tracked path, if it has one.
    pub current_path: Option<PathBuf>,
    /// Whether the ticket's close session, if any, is still the current one.
    pub close_session_current: bool,
}

/// Whether one queued save still describes the editor it was queued against.
///
/// The path comparison is skipped for an explicit destination because Save As
/// deliberately writes somewhere other than the tracked path; for every other
/// save the comparison is what stops a re-pathed editor from writing its bytes
/// to a stale target.
#[must_use]
pub(crate) fn queued_save_is_current(ticket: &QueuedSaveTicket, facts: &QueuedSaveFacts) -> bool {
    facts.saving
        && facts.save_generation == ticket.save_generation
        && (!ticket.required_modified || facts.modified)
        && (ticket.explicit_destination
            || facts.current_path.as_deref() == Some(ticket.path.as_path()))
        && facts.close_session_current
}

/// What the workflow must do with the text it captured, once the write returns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SaveTextDisposition {
    /// Save formatting rewrote the text, so the buffer must be mirrored back.
    ///
    /// The saved bytes and the live buffer must agree before the tab is marked
    /// clean, so this arm is not optional decoration: skipping it would show a
    /// clean tab whose visible text differs from disk.
    MirrorFormattedIntoBuffer {
        /// Whether the mirrored body becomes the new clean history baseline.
        retain_as_clean: bool,
    },
    /// The written text matched the buffer and becomes the clean baseline.
    RetainCapturedAsClean,
    /// The written text matched the buffer and nothing needs it afterwards.
    RetireCaptured,
}

/// Decide what happens to captured save text after a successful write.
#[must_use]
pub(crate) const fn classify_saved_text(
    formatting_changed: bool,
    automatic_capture_available: bool,
) -> SaveTextDisposition {
    if formatting_changed {
        SaveTextDisposition::MirrorFormattedIntoBuffer {
            retain_as_clean: automatic_capture_available,
        }
    } else if automatic_capture_available {
        SaveTextDisposition::RetainCapturedAsClean
    } else {
        SaveTextDisposition::RetireCaptured
    }
}

/// How one save captured the buffer text it wrote.
///
/// The threshold itself is **not** owned here: it belongs to the cross-cutting
/// buffer-snapshot workflow (`ui/buffer_snapshot`), and duplicating it would
/// fork a shared limit. What the save workflow owns is naming the two modes so
/// the choice is observable on its evidence surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SaveCaptureMode {
    /// The buffer was copied in one turn.
    #[default]
    Direct,
    /// The buffer was copied in main-loop slices while the view stayed read-only.
    Chunked,
}

/// Name the capture mode the buffer-snapshot threshold selected.
#[must_use]
pub(crate) const fn save_capture_mode(requires_chunked: bool) -> SaveCaptureMode {
    if requires_chunked {
        SaveCaptureMode::Chunked
    } else {
        SaveCaptureMode::Direct
    }
}

/// How the last durable write for one editor ended.
///
/// The three arms mirror the durable-write contract exactly, because conflating
/// them is a data-safety failure: `BeforeRename` leaves the previous bytes
/// intact and the document modified, while `AfterRename` means the new bytes are
/// on disk but their directory entry was never proven crash-safe. Reporting the
/// second as a generic lost save would tell the user to redo work that is
/// already written; reporting the first as a success would lose it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SaveWriteClassification {
    /// No durable write has completed for this editor yet.
    #[default]
    None,
    /// The write completed and the editor accepted it.
    Accepted,
    /// The write failed before the rename; the previous bytes are intact.
    FailedBeforeRename,
    /// The bytes were renamed into place but durability is unconfirmed.
    DurabilityUnconfirmed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket() -> QueuedSaveTicket {
        QueuedSaveTicket {
            save_generation: 7,
            path: PathBuf::from("/tmp/current.md"),
            explicit_destination: false,
            required_modified: true,
            close_session_identity: None,
        }
    }

    fn facts() -> QueuedSaveFacts {
        QueuedSaveFacts {
            saving: true,
            save_generation: 7,
            modified: true,
            current_path: Some(PathBuf::from("/tmp/current.md")),
            close_session_current: true,
        }
    }

    #[test]
    fn a_current_queued_save_passes_every_clause() {
        assert!(queued_save_is_current(&ticket(), &facts()));
    }

    #[test]
    fn a_plain_save_is_rejected_when_the_editor_was_repathed() {
        let mut facts = facts();
        facts.current_path = Some(PathBuf::from("/tmp/elsewhere.md"));
        assert!(!queued_save_is_current(&ticket(), &facts));
    }

    #[test]
    fn an_explicit_destination_skips_the_path_comparison() {
        // This is the clause the renamed value used to control. A Save As
        // deliberately writes somewhere other than the tracked path, so the
        // comparison must not reject it; a plain save must still be rejected,
        // which the previous case proves.
        let mut ticket = ticket();
        ticket.explicit_destination = true;
        let mut facts = facts();
        facts.current_path = Some(PathBuf::from("/tmp/elsewhere.md"));
        assert!(queued_save_is_current(&ticket, &facts));
    }

    #[test]
    fn an_explicit_destination_still_fails_every_other_clause() {
        let mut ticket = ticket();
        ticket.explicit_destination = true;
        let mut stale_generation = facts();
        stale_generation.save_generation = 8;
        assert!(!queued_save_is_current(&ticket, &stale_generation));
        let mut not_saving = facts();
        not_saving.saving = false;
        assert!(!queued_save_is_current(&ticket, &not_saving));
        let mut unmodified = facts();
        unmodified.modified = false;
        assert!(!queued_save_is_current(&ticket, &unmodified));
        let mut stale_session = facts();
        stale_session.close_session_current = false;
        assert!(!queued_save_is_current(&ticket, &stale_session));
    }

    #[test]
    fn a_save_not_requiring_modification_ignores_a_clean_buffer() {
        let mut ticket = ticket();
        ticket.required_modified = false;
        let mut facts = facts();
        facts.modified = false;
        assert!(queued_save_is_current(&ticket, &facts));
    }

    #[test]
    fn a_plain_save_with_no_tracked_path_is_rejected() {
        let mut facts = facts();
        facts.current_path = None;
        assert!(!queued_save_is_current(&ticket(), &facts));
    }

    #[test]
    fn preemption_is_derived_from_explicit_destination_only() {
        assert!(save_may_preempt_pending_load(true));
        assert!(!save_may_preempt_pending_load(false));
        assert!(!save_may_preempt_pending_load(
            ticket().explicit_destination
        ));
    }

    #[test]
    fn explicit_destination_and_pending_load_cancellation_stay_distinct() {
        // The archetype defect this migration removes: one value stored as
        // `cancel_pending_load` and received as `explicit_destination`. The
        // ticket now carries the user's intent and the cancellation consequence
        // is a named derivation, so a mismatched positional edit is a type error
        // rather than an invisible rename. Pin both meanings against each other
        // so a future edit cannot quietly re-fuse them.
        let mut ticket = ticket();
        let mut repathed = facts();
        repathed.current_path = Some(PathBuf::from("/tmp/moved.md"));

        // A plain save whose editor was re-pathed must not write the stale
        // target, and must not pre-empt an in-flight load.
        assert!(!queued_save_is_current(&ticket, &repathed));
        assert!(!save_may_preempt_pending_load(ticket.explicit_destination));

        // A Save As deliberately writes elsewhere, and is the only kind that may
        // pre-empt an in-flight load.
        ticket.explicit_destination = true;
        assert!(queued_save_is_current(&ticket, &repathed));
        assert!(save_may_preempt_pending_load(ticket.explicit_destination));
    }

    #[test]
    fn saved_text_disposition_covers_every_combination() {
        assert_eq!(
            classify_saved_text(true, true),
            SaveTextDisposition::MirrorFormattedIntoBuffer {
                retain_as_clean: true
            }
        );
        assert_eq!(
            classify_saved_text(true, false),
            SaveTextDisposition::MirrorFormattedIntoBuffer {
                retain_as_clean: false
            }
        );
        assert_eq!(
            classify_saved_text(false, true),
            SaveTextDisposition::RetainCapturedAsClean
        );
        assert_eq!(
            classify_saved_text(false, false),
            SaveTextDisposition::RetireCaptured
        );
    }

    #[test]
    fn capture_mode_names_the_threshold_result() {
        assert_eq!(save_capture_mode(true), SaveCaptureMode::Chunked);
        assert_eq!(save_capture_mode(false), SaveCaptureMode::Direct);
        assert_eq!(SaveCaptureMode::default(), SaveCaptureMode::Direct);
    }

    #[test]
    fn write_classification_defaults_to_no_completed_write() {
        assert_eq!(
            SaveWriteClassification::default(),
            SaveWriteClassification::None
        );
    }

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
    fn documented_payload_policy_constants_hold_their_values() {
        // Asserted as literals, not as the expressions that compute them: these
        // are documented policy limits, and a test that restates the expression
        // symbolically cannot notice the expression changing.
        assert_eq!(SAVE_PAYLOAD_FIXED_OVERHEAD_BYTES, 0x0010_0000);
        assert_eq!(SAVE_PAYLOAD_RESIDENCY_MULTIPLIER, 8);
        assert_eq!(MAX_ADMITTED_SAVE_PAYLOADS, 8);
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

    #[test]
    fn refreshing_an_unknown_request_reports_no_match() {
        let mut policy = SaveAdmissionPolicy::new(10, 1);
        assert!(!policy.refresh_queued(404, 1, 1, 1));
        policy.queue(request(1, 1, 4, SaveAdmissionPriority::Ordinary));
        assert!(!policy.refresh_queued(404, 1, 1, 1));
        assert!(policy.refresh_queued(1, 1, 1, 1));
    }

    #[test]
    fn every_admission_guard_clause_blocks_on_its_own() {
        // Each disjunct in `admit_next`'s guard is independently sufficient, so
        // assert them one at a time against an otherwise-admissible queue.
        let admissible = || {
            let mut policy = SaveAdmissionPolicy::new(10, 2);
            policy.queue(request(1, 1, 4, SaveAdmissionPriority::Ordinary));
            policy
        };

        // Nothing queued.
        let mut empty = SaveAdmissionPolicy::new(10, 2);
        assert!(
            empty
                .admit_next(ExternalTransientPressure::default())
                .is_none()
        );

        // No active slots configured at all.
        let mut no_slots = SaveAdmissionPolicy::new(10, 0);
        no_slots.queue(request(1, 1, 4, SaveAdmissionPriority::Ordinary));
        assert!(
            no_slots
                .admit_next(ExternalTransientPressure::default())
                .is_none()
        );

        // Every active slot already taken.
        let mut full = SaveAdmissionPolicy::new(10, 1);
        full.queue(request(1, 1, 4, SaveAdmissionPriority::Ordinary));
        full.queue(request(2, 2, 4, SaveAdmissionPriority::Ordinary));
        assert!(
            full.admit_next(ExternalTransientPressure::default())
                .is_some()
        );
        assert!(
            full.admit_next(ExternalTransientPressure::default())
                .is_none()
        );

        // Another lane is running one exclusive payload.
        let mut external_exclusive = admissible();
        assert!(
            external_exclusive
                .admit_next(ExternalTransientPressure {
                    exclusive_active: true,
                    ..ExternalTransientPressure::default()
                })
                .is_none()
        );

        // Protected residency is over budget and this lane is already busy.
        let mut protected_busy = admissible();
        assert!(
            protected_busy
                .admit_next(ExternalTransientPressure::default())
                .is_some()
        );
        protected_busy.queue(request(2, 2, 4, SaveAdmissionPriority::Ordinary));
        assert!(
            protected_busy
                .admit_next(ExternalTransientPressure {
                    protected_residency_over_budget: true,
                    ..ExternalTransientPressure::default()
                })
                .is_none()
        );

        // Protected residency is over budget and another lane holds weight, so
        // even a wholly idle save lane must wait.
        let mut protected_external = admissible();
        assert!(
            protected_external
                .admit_next(ExternalTransientPressure {
                    protected_residency_over_budget: true,
                    active_weight: 1,
                    ..ExternalTransientPressure::default()
                })
                .is_none()
        );
        // With no active save and no external weight, the same pressure admits.
        assert!(
            protected_external
                .admit_next(ExternalTransientPressure {
                    protected_residency_over_budget: true,
                    ..ExternalTransientPressure::default()
                })
                .is_some()
        );
    }

    #[test]
    fn an_exactly_budget_sized_request_is_not_exclusive() {
        // `exclusive` is `weight > budget`, so the boundary case must admit
        // normally rather than seizing the whole lane.
        let mut policy = SaveAdmissionPolicy::new(10, 2);
        policy.queue(request(1, 1, 10, SaveAdmissionPriority::Ordinary));
        let grant = policy
            .admit_next(ExternalTransientPressure::default())
            .expect("a budget-sized request fits");
        assert!(!grant.exclusive);
        assert!(!policy.snapshot().exclusive_active);
    }

    #[test]
    fn an_exactly_budget_sized_overweight_request_needs_a_wholly_idle_lane() {
        // `request_fits` treats an overweight request as needing zero external
        // weight, so one byte of external pressure must block it.
        let mut policy = SaveAdmissionPolicy::new(10, 2);
        policy.queue(request(1, 1, 11, SaveAdmissionPriority::Ordinary));
        assert!(
            policy
                .admit_next(ExternalTransientPressure {
                    active_weight: 1,
                    ..ExternalTransientPressure::default()
                })
                .is_none()
        );
        assert!(
            policy
                .admit_next(ExternalTransientPressure::default())
                .is_some()
        );
    }

    #[test]
    fn snapshot_counts_close_work_separately_from_ordinary_work() {
        let mut policy = SaveAdmissionPolicy::new(100, 4);
        policy.queue(request(1, 1, 4, SaveAdmissionPriority::Close));
        policy.queue(request(2, 2, 4, SaveAdmissionPriority::Ordinary));
        policy.queue(request(3, 3, 4, SaveAdmissionPriority::Ordinary));
        let queued = policy.snapshot();
        assert_eq!(queued.queued_count, 3);
        assert_eq!(queued.queued_close_count, 1);
        assert_eq!(queued.active_close_count, 0);

        // Close work is admitted first, so the close count moves from queued to
        // active while the ordinary requests stay behind.
        let grant = policy
            .admit_next(ExternalTransientPressure::default())
            .expect("close grant");
        assert_eq!(grant.priority, SaveAdmissionPriority::Close);
        let active = policy.snapshot();
        assert_eq!(active.queued_close_count, 0);
        assert_eq!(active.queued_count, 2);
        assert_eq!(active.active_count, 1);
        assert_eq!(active.active_close_count, 1);

        let ordinary = policy
            .admit_next(ExternalTransientPressure::default())
            .expect("ordinary grant");
        assert_eq!(ordinary.priority, SaveAdmissionPriority::Ordinary);
        let mixed = policy.snapshot();
        assert_eq!(mixed.active_count, 2);
        assert_eq!(mixed.active_close_count, 1);
    }
}

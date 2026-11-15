// SPDX-License-Identifier: GPL-3.0-or-later

//! Copy of the draft journal's decisions, for V2 and V3.
//!
//! Copied from `crates/lushtext-core/src/services/draft_service/journal_core.rs`
//! at `85758eaf8fb8190439719dd6926a5b2b3ed21fb0` (`origin/main`, 2026-11-15),
//! with the three types it imports from elsewhere in `lushtext-core`
//! (`DraftManifestCompleteness`, `DraftManifestAuthority` and its
//! replacement eligibility, `DraftOrphanCleanupRetentionReason`) copied
//! beside it. `verus-drift` checks every decision against production over
//! every input.
//!
//! Each decision keeps its production body. Its attribute
//! `#[verus_verify(dual_spec, open)]` makes Verus generate an open spec
//! function from that same body, so a V2 model would reason about the
//! decision itself, never a restated copy of it: a mutant of the body would
//! change what the invariant proof sees. V2 was not completed (the report,
//! `docs/next/formal-verification-verus.md` §5), so no model exists yet.
//!
//! Rewrite footprint against production (the report lists it):
//! - the `#[cfg_attr(kani, derive(kani::Arbitrary))]` attributes are dropped
//!   (no Kani here);
//! - `impl From<FileDraftRestoreSkip> for RestoreEnding` is not copied: it
//!   converts a type of `model::draft` that the journal decisions never take;
//! - `run_deletion` stays plain Rust outside `verus!`: it takes an
//!   `impl FnMut`, which Verus supports only partly; it is drift-checked, not
//!   verified, and no model step calls it (the model drives
//!   `next_deletion_step` one step at a time, as the Kani model does);
//! - the three small helpers production calls as associated items
//!   (`DraftManifestAuthority::untrusted` and `is_trusted`,
//!   `BodyFacts::from_window_view`) sit inside `verus!` with
//!   `#[verifier::allow_in_spec]`, whose `returns` clause repeats the body as
//!   an expression, and `is_trusted`
//!   compares the two fields instead of `matches!` on the whole struct value;
//! - `orphan_body_decision`'s early `return`s are one `if` chain: a
//!   `dual_spec` body must be a pure expression ("return is not allowed in
//!   pure context");
//! - the associated constant `DraftManifestAuthority::TRUSTED` is inside
//!   `verus!`: items outside it are external, and a verified body cannot
//!   read an external constant.

use vstd::prelude::*;

verus! {

// --- copied supporting types -------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftManifestCompleteness {
    Complete,
    Partial,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftManifestReplacementEligibility {
    Eligible,
    Ineligible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraftManifestAuthority {
    pub completeness: DraftManifestCompleteness,
    pub replacement: DraftManifestReplacementEligibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftOrphanCleanupRetentionReason {
    UnknownFile,
    DuplicateManifestId,
    StatusUncertain,
    BodyNotRegularFile,
    CandidatePathMismatch,
    ManifestEntryPresent,
    FingerprintChanged,
    BodyReappeared,
    BodyGenerationChanged,
    DeleteFailed,
    ManifestCommitFailed,
}

// --- 1. ownership -------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryState {
    Absent,
    Current,
    Superseded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyFacts {
    pub body_present: bool,
    pub entry: EntryState,
    pub restore_pending: bool,
    pub backing_stale: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyOwner {
    Nothing,
    Unseen,
    OtherVersion,
    Unregistered,
    Journal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyWriteDecision {
    Hold,
    RegisterFirst,
    PreserveThenWrite,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UntrustedCommitDisposition {
    Additive,
    Refuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeletionStep {
    Preserve,
    DeleteBody,
    RemoveEntry,
    Done,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreEnding {
    Applied,
    Stale,
    Oversized,
    ReadFailed,
    EditedOver,
    InstallCancelled,
    Unavailable,
    MissingBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreDisposition {
    Nothing,
    PreserveCopy,
    PreserveThenRetire,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupEntryState {
    Unreferenced,
    Referenced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupBodyIdentity {
    Missing,
    Inspected,
    Changed,
    NotRegularFile,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupFacts {
    pub manifest_trusted: bool,
    pub path_matches: bool,
    pub entry: CleanupEntryState,
    pub write_guard_held: bool,
    pub identity: CleanupBodyIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrphanBodyDecision {
    Delete,
    AlreadyAbsent,
    Retain(DraftOrphanCleanupRetentionReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetAsideSlot {
    Free,
    SameBody,
    OtherBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetAsideNameStep {
    Place,
    AlreadyKept,
    NextName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalLaneHolder {
    Nobody,
    ThisWindow,
    OtherWindow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalLaneAdmission {
    Admit,
    MarkPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftClaim {
    Unclaimed,
    ThisWindow,
    OtherWindow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftOpenDecision {
    OpenHere,
    PresentOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupRestore {
    RestoreSession,
    StartEmpty,
}

} // verus!

verus! {

impl DraftManifestAuthority {
    pub const TRUSTED: Self = Self {
        completeness: DraftManifestCompleteness::Complete,
        replacement: DraftManifestReplacementEligibility::Eligible,
    };

    #[verifier::allow_in_spec]
    pub const fn untrusted(completeness: DraftManifestCompleteness) -> Self
        returns
            (Self { completeness, replacement: DraftManifestReplacementEligibility::Ineligible }),
    {
        Self {
            completeness,
            replacement: DraftManifestReplacementEligibility::Ineligible,
        }
    }

    // Production: `matches!(self, Self { completeness: Complete, replacement: Eligible })`.
    #[verifier::allow_in_spec]
    pub const fn is_trusted(self) -> bool
        returns
            (self.completeness == DraftManifestCompleteness::Complete
                && self.replacement == DraftManifestReplacementEligibility::Eligible),
    {
        match (self.completeness, self.replacement) {
            (DraftManifestCompleteness::Complete, DraftManifestReplacementEligibility::Eligible) => true,
            _ => false,
        }
    }
}

impl BodyFacts {
    #[verifier::allow_in_spec]
    pub const fn from_window_view(registered: bool, restore_pending: bool) -> Self
        returns
            (Self {
                body_present: restore_pending || registered,
                entry: if registered { EntryState::Current } else { EntryState::Absent },
                restore_pending,
                backing_stale: false,
            }),
    {
        Self {
            body_present: restore_pending || registered,
            entry: if registered {
                EntryState::Current
            } else {
                EntryState::Absent
            },
            restore_pending,
            backing_stale: false,
        }
    }
}

} // verus!

// --- 1. ownership -------------------------------------------------------------

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns ownership(facts))]
pub const fn ownership(facts: BodyFacts) -> BodyOwner {
    if !facts.body_present {
        BodyOwner::Nothing
    } else if facts.restore_pending {
        BodyOwner::Unseen
    } else if facts.backing_stale || matches!(facts.entry, EntryState::Superseded) {
        BodyOwner::OtherVersion
    } else if matches!(facts.entry, EntryState::Absent) {
        BodyOwner::Unregistered
    } else {
        BodyOwner::Journal
    }
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns must_preserve_before_replacing(owner))]
pub const fn must_preserve_before_replacing(owner: BodyOwner) -> bool {
    matches!(owner, BodyOwner::Unseen | BodyOwner::OtherVersion)
}

// --- 2. registration before a body write ----------------------------------------

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns registration_required(file_backed, registered_in_known_manifest, known_manifest_trusted))]
pub const fn registration_required(
    file_backed: bool,
    registered_in_known_manifest: bool,
    known_manifest_trusted: bool,
) -> bool {
    file_backed && (!registered_in_known_manifest || !known_manifest_trusted)
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns may_write_after_registration(required_registration, registration_committed))]
pub const fn may_write_after_registration(
    required_registration: bool,
    registration_committed: bool,
) -> bool {
    !required_registration || registration_committed
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns body_write_decision(owner, registration_needed))]
pub const fn body_write_decision(owner: BodyOwner, registration_needed: bool) -> BodyWriteDecision {
    match owner {
        BodyOwner::Unseen => BodyWriteDecision::Hold,
        _ if registration_needed => BodyWriteDecision::RegisterFirst,
        BodyOwner::OtherVersion => BodyWriteDecision::PreserveThenWrite,
        BodyOwner::Nothing | BodyOwner::Unregistered | BodyOwner::Journal => {
            BodyWriteDecision::Write
        }
    }
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns untrusted_commit_disposition(insert_only, replacement_allowed))]
pub const fn untrusted_commit_disposition(
    insert_only: bool,
    replacement_allowed: bool,
) -> UntrustedCommitDisposition {
    if insert_only && replacement_allowed {
        UntrustedCommitDisposition::Additive
    } else {
        UntrustedCommitDisposition::Refuse
    }
}

// --- 3. body-then-entry deletion -------------------------------------------------

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns deletion_start(owner))]
pub const fn deletion_start(owner: BodyOwner) -> DeletionStep {
    if must_preserve_before_replacing(owner) {
        DeletionStep::Preserve
    } else {
        DeletionStep::DeleteBody
    }
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns next_deletion_step(step, succeeded))]
pub const fn next_deletion_step(step: DeletionStep, succeeded: bool) -> DeletionStep {
    match step {
        DeletionStep::Preserve if succeeded => DeletionStep::DeleteBody,
        DeletionStep::DeleteBody if succeeded => DeletionStep::RemoveEntry,
        DeletionStep::RemoveEntry if succeeded => DeletionStep::Done,
        DeletionStep::Preserve | DeletionStep::DeleteBody | DeletionStep::RemoveEntry => {
            DeletionStep::Stopped
        }
        DeletionStep::Done => DeletionStep::Done,
        DeletionStep::Stopped => DeletionStep::Stopped,
    }
}

/// Not verified (it takes an `impl FnMut`); drift-checked only.
#[verifier::external]
pub fn run_deletion(
    start: DeletionStep,
    mut perform: impl FnMut(DeletionStep) -> bool,
) -> DeletionStep {
    let mut step = start;
    while matches!(
        step,
        DeletionStep::Preserve | DeletionStep::DeleteBody | DeletionStep::RemoveEntry
    ) {
        let succeeded = perform(step);
        step = next_deletion_step(step, succeeded);
    }
    step
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns startup_may_retire_stale(manifest_trusted))]
pub const fn startup_may_retire_stale(manifest_trusted: bool) -> bool {
    manifest_trusted
}

// --- 4. unapplied restores ---------------------------------------------------------

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns unapplied_restore_disposition(ending))]
pub const fn unapplied_restore_disposition(ending: RestoreEnding) -> RestoreDisposition {
    match ending {
        RestoreEnding::Stale => RestoreDisposition::PreserveThenRetire,
        RestoreEnding::Oversized
        | RestoreEnding::ReadFailed
        | RestoreEnding::EditedOver
        | RestoreEnding::InstallCancelled
        | RestoreEnding::Unavailable => RestoreDisposition::PreserveCopy,
        RestoreEnding::Applied | RestoreEnding::MissingBody => RestoreDisposition::Nothing,
    }
}

// --- 5. cleanup eligibility ----------------------------------------------------------

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns orphan_body_decision(facts))]
pub const fn orphan_body_decision(facts: CleanupFacts) -> OrphanBodyDecision {
    // Production returns early from each guard; a `dual_spec` body is a pure
    // expression, so the guards are one `if` chain here (same order).
    if !facts.manifest_trusted {
        OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::StatusUncertain)
    } else if !facts.path_matches {
        OrphanBodyDecision::Retain(
            DraftOrphanCleanupRetentionReason::CandidatePathMismatch,
        )
    } else if matches!(facts.entry, CleanupEntryState::Referenced) {
        OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::ManifestEntryPresent)
    } else if !facts.write_guard_held {
        OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::StatusUncertain)
    } else {
        match facts.identity {
            CleanupBodyIdentity::Missing => OrphanBodyDecision::AlreadyAbsent,
            CleanupBodyIdentity::Inspected => OrphanBodyDecision::Delete,
            CleanupBodyIdentity::Changed => {
                OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::BodyGenerationChanged)
            }
            CleanupBodyIdentity::NotRegularFile => {
                OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::BodyNotRegularFile)
            }
            CleanupBodyIdentity::Uncertain => {
                OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::StatusUncertain)
            }
        }
    }
}

// --- 6. reconciliation authority -------------------------------------------------------

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns commit_authority(completeness, durably_written))]
pub const fn commit_authority(
    completeness: DraftManifestCompleteness,
    durably_written: bool,
) -> DraftManifestAuthority {
    if matches!(completeness, DraftManifestCompleteness::Complete) && durably_written {
        DraftManifestAuthority::TRUSTED
    } else {
        DraftManifestAuthority::untrusted(completeness)
    }
}

// --- 7. set-aside naming ------------------------------------------------------------------

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns set_aside_name_step(slot))]
pub const fn set_aside_name_step(slot: SetAsideSlot) -> SetAsideNameStep {
    match slot {
        SetAsideSlot::Free => SetAsideNameStep::Place,
        SetAsideSlot::SameBody => SetAsideNameStep::AlreadyKept,
        SetAsideSlot::OtherBody => SetAsideNameStep::NextName,
    }
}

// --- 8. one journal per process --------------------------------------------------------------

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns journal_lane_admission(holder))]
pub const fn journal_lane_admission(holder: JournalLaneHolder) -> JournalLaneAdmission {
    match holder {
        JournalLaneHolder::Nobody => JournalLaneAdmission::Admit,
        JournalLaneHolder::ThisWindow | JournalLaneHolder::OtherWindow => {
            JournalLaneAdmission::MarkPending
        }
    }
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns draft_open_decision(claim))]
pub const fn draft_open_decision(claim: DraftClaim) -> DraftOpenDecision {
    match claim {
        DraftClaim::Unclaimed | DraftClaim::ThisWindow => DraftOpenDecision::OpenHere,
        DraftClaim::OtherWindow => DraftOpenDecision::PresentOwner,
    }
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns window_may_journal_draft(claim))]
pub const fn window_may_journal_draft(claim: DraftClaim) -> bool {
    !matches!(claim, DraftClaim::OtherWindow)
}

#[verus_verify(dual_spec, open)]
#[verus_spec(r => returns startup_restore(process_already_restored))]
pub const fn startup_restore(process_already_restored: bool) -> StartupRestore {
    if process_already_restored {
        StartupRestore::StartEmpty
    } else {
        StartupRestore::RestoreSession
    }
}

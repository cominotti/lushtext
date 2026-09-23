// SPDX-License-Identifier: GPL-3.0-or-later

//! The draft journal's ordering and ownership decisions, as one pure state
//! machine that both `draft_service` and the GTK drafts workflow
//! (`ui/window/drafts/`) drive.
//!
//! Nothing here touches the filesystem, GTK, a clock, or a collection: every
//! function takes small `Copy` facts about **one** draft id and returns what the
//! journal must do next. That is what lets Kani (`kani_proofs.rs` beside this
//! module) check the very functions production calls, over every interleaving
//! of edits, writes, commits, deletes, cleanup, crashes, and restarts within its
//! stated bounds, instead of a separate model that could drift from them.
//!
//! The decisions, in the order a draft meets them:
//!
//! 1. **Ownership** ([`ownership`]) — who the bytes on disk belong to right now.
//!    The single body-ownership check: the restore hold, every preserve-first
//!    call site, and the registration overwrite copy all ask it.
//! 2. **Registration before a body write** ([`registration_required`],
//!    [`body_write_decision`]) — a file-backed body is written only for an id
//!    the persisted manifest describes, so no crash point leaves a body no entry
//!    explains (the phase-0 wedge).
//! 3. **Body-then-entry deletion** ([`deletion_start`], [`next_deletion_step`],
//!    driven by [`run_deletion`]) — a stale or unseen body is preserved first; the body goes before its
//!    entry, so the entry stays the durable retry marker until the body is gone.
//! 4. **Unapplied restores** ([`unapplied_restore_disposition`]) — a recovery
//!    body the user never saw is preserved before autosave may replace it.
//! 5. **Cleanup eligibility** ([`orphan_body_decision`]) — orphan cleanup
//!    deletes a body only when it is unreferenced, its identity revalidated, the
//!    manifest trusted, and no write in flight.
//! 6. **Reconciliation authority** ([`commit_authority`],
//!    [`untrusted_commit_disposition`]) — a manifest is trusted only after a
//!    complete reconciliation that was durably written.
//!
//! Invariants Kani checks over these functions (see the programme record,
//! `docs/next/formal-verification.md`, phase 4):
//!
//! - **S1 acceptance durability** — an accepted generation stays recoverable
//!   (its body, a newer body, or a preserved copy) until discard, clean save, or
//!   a stale file, across crashes.
//! - **S2 cleanup safety** — a body is deleted only when unreferenced, identity
//!   revalidated, the inventory trusted, and no write in flight.
//! - **S3 delete ordering** — delete intent never coexists with a present body
//!   and a missing manifest entry.
//! - **S4 trust** — a trusted manifest implies the last reconciliation was
//!   complete.
//! - **L1 bounded liveness** — without I/O faults, a dirty open editor becomes
//!   clean within six journal steps.

use crate::model::draft::{
    DraftManifestAuthority, DraftManifestCompleteness, FileDraftRestoreSkip,
};
use crate::services::draft_service::DraftOrphanCleanupRetentionReason;

// --- 1. ownership ------------------------------------------------------------

/// What the persisted manifest says about one draft id, relative to the body
/// write or deletion being decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum EntryState {
    /// No entry describes the id.
    Absent,
    /// An entry describes the id for the backing version being written, or the
    /// id is untitled (its entry needs no backing version).
    Current,
    /// An entry describes the id for a **different** backing version: the body
    /// on disk holds edits against a file version the write about to follow is
    /// not based on.
    Superseded,
}

/// Observable facts about one draft id's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct BodyFacts {
    /// A body file exists (or may exist, when the caller cannot tell: callers
    /// that do not know must pass `true`, the conservative answer).
    pub body_present: bool,
    /// What the persisted manifest records for the id.
    pub entry: EntryState,
    /// A restore of this body is queued or reading: the user has not seen it.
    pub restore_pending: bool,
    /// The backing file is confirmed to have changed since the body's entry
    /// was written, so the body must never be applied over it.
    pub backing_stale: bool,
}

impl BodyFacts {
    /// The facts as the GTK window sees them, from its own manifest copy and
    /// restore hold: a body may exist whenever an entry is registered or a
    /// restore is pending, and the window never knows the backing is stale.
    #[must_use]
    pub const fn from_window_view(registered: bool, restore_pending: bool) -> Self {
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

/// Who the bytes of one draft body belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyOwner {
    /// There is no body to protect.
    Nothing,
    /// A restore is pending: the body holds work the user has not seen, so it
    /// must not be overwritten, and deleting it preserves it first.
    Unseen,
    /// The body belongs to a different backing version of its file (stale, or
    /// superseded by the write about to follow): preserve it before it is
    /// overwritten or retired.
    OtherVersion,
    /// A body no entry describes: reconciliation's to attribute or set aside.
    Unregistered,
    /// The journal's ordinary current body for its entry: the next autosave of
    /// the same editor may replace it, and the journal's ordering retires it.
    Journal,
}

/// The single body-ownership check.
///
/// Precedence is the safety order: work the user has not seen outranks a
/// version conflict, which outranks registration state.
#[must_use]
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

/// Whether a body's bytes must be preserved before anything replaces or
/// removes them.
#[must_use]
pub const fn must_preserve_before_replacing(owner: BodyOwner) -> bool {
    matches!(owner, BodyOwner::Unseen | BodyOwner::OtherVersion)
}

// --- 2. registration before a body write ----------------------------------

/// Whether a candidate's draft id must be registered in the persisted manifest
/// before its body may be written.
///
/// A file-backed body written for an id the persisted manifest does not know is
/// exactly the crash leftover that used to wedge the journal. An untitled id
/// needs no registration: its `untitled-…` id already makes its body
/// recoverable. When the caller's authority is not trusted its manifest copy may
/// not match disk, so every file-backed candidate is registered; registration
/// only ever inserts an absent entry, so doing it for a known id is harmless.
#[must_use]
pub const fn registration_required(
    file_backed: bool,
    registered_in_known_manifest: bool,
    known_manifest_trusted: bool,
) -> bool {
    file_backed && (!registered_in_known_manifest || !known_manifest_trusted)
}

/// Whether a candidate may proceed to its body write after the pass's
/// registration step: only when no registration was needed or it committed.
#[must_use]
pub const fn may_write_after_registration(
    required_registration: bool,
    registration_committed: bool,
) -> bool {
    !required_registration || registration_committed
}

/// What an autosave or close pass does with one candidate's body write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyWriteDecision {
    /// The body holds unseen work: do not write, retry after the restore.
    Hold,
    /// Register the id first; write only after the registration committed.
    RegisterFirst,
    /// Keep a preserved copy of the existing body, then write.
    PreserveThenWrite,
    /// Write the body.
    Write,
}

/// Decide one body write from its owner and registration need.
#[must_use]
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

/// How an insert-only commit (write-ahead registration) proceeds when the
/// complete reconciliation it attempted first could not prove completeness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UntrustedCommitDisposition {
    /// Add the absent entries to the persisted manifest as-is, keeping the
    /// journal untrusted: adding entries never forgets evidence.
    Additive,
    /// The persisted recovery evidence forbids any replacement: refuse, and the
    /// caller must not write the bodies.
    Refuse,
}

/// Decide an insert-only commit after an incomplete reconciliation.
#[must_use]
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

// --- 3. body-then-entry deletion --------------------------------------------

/// The next thing a serialized draft deletion does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum DeletionStep {
    /// Durably preserve the body before anything is deleted.
    Preserve,
    /// Delete the body file.
    DeleteBody,
    /// Remove the manifest entry.
    RemoveEntry,
    /// The deletion is complete: the tombstone may retire.
    Done,
    /// A step failed: stop with the journal in the state the last successful
    /// step left, which is always recoverable, and keep the delete retryable.
    Stopped,
}

/// Where a deletion of a body with this owner starts.
#[must_use]
pub const fn deletion_start(owner: BodyOwner) -> DeletionStep {
    if must_preserve_before_replacing(owner) {
        DeletionStep::Preserve
    } else {
        DeletionStep::DeleteBody
    }
}

/// The step after `step`, given whether it succeeded.
///
/// The order is the invariant: preserve, then the body, then the entry. The
/// persisted entry therefore stays the durable retry marker until the body is
/// gone, and a failure at any point leaves a fully recoverable pre-delete state
/// across unrelated manifest updates and restart.
#[must_use]
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

/// Drive a deletion from `start` to its terminal ([`DeletionStep::Done`] or
/// [`DeletionStep::Stopped`]) in [`next_deletion_step`]'s order. `perform`
/// runs one step's I/O and reports whether it succeeded; it is never called
/// again after a failure.
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

/// Whether startup may preserve-and-retire a stale body itself, or must leave
/// both the body and its entry for the editor-open path to retry.
///
/// Retiring rewrites the manifest, which only a trusted journal may do.
#[must_use]
pub const fn startup_may_retire_stale(manifest_trusted: bool) -> bool {
    manifest_trusted
}

// --- 4. unapplied restores ---------------------------------------------------

/// How one restore attempt ended, from the journal's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum RestoreEnding {
    /// The body was installed into the editor: the user sees it.
    Applied,
    /// The backing file changed on disk: never applied.
    Stale,
    /// Too large to restore automatically.
    Oversized,
    /// The body could not be read.
    ReadFailed,
    /// The user edited the tab before the body could be applied.
    EditedOver,
    /// The bounded install was cancelled or superseded before it completed.
    InstallCancelled,
    /// The file metadata could not be read this time (the file vanished or its
    /// mount is away), so the body was not applied — and was not shown.
    Unavailable,
    /// The body was already gone.
    MissingBody,
}

/// What happens to a body after its restore attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreDisposition {
    /// Nothing: the body was applied, is already gone, or stays for a retry.
    Nothing,
    /// Keep a preserved copy; the body stays in the journal and autosave may
    /// then replace it.
    PreserveCopy,
    /// Preserve the body, then retire it through the serialized deletion.
    PreserveThenRetire,
}

impl From<FileDraftRestoreSkip> for RestoreEnding {
    fn from(skip: FileDraftRestoreSkip) -> Self {
        match skip {
            FileDraftRestoreSkip::Stale => Self::Stale,
            FileDraftRestoreSkip::Oversized => Self::Oversized,
            FileDraftRestoreSkip::Unavailable => Self::Unavailable,
            FileDraftRestoreSkip::MissingDraft => Self::MissingBody,
        }
    }
}

/// Decide what a restore attempt that did not apply its body leaves behind.
///
/// Every ending that was never shown to the user and whose body still exists
/// keeps a preserved copy before autosave may replace it; a stale body is also
/// retired, because it can never be applied.
///
/// `Unavailable` preserves too. It once did nothing, on the reading that the
/// body simply stays for a retry — but the restore hold is released with it,
/// so the next autosave of the still-open tab replaced a body the user never
/// saw. The Kani journal harness found it (S1); see
/// `test_an_unavailable_restore_keeps_the_unshown_draft_before_autosave_replaces_it`.
#[must_use]
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

// --- 5. cleanup eligibility -------------------------------------------------

/// What the latest persisted manifest, reloaded under the write lock, says
/// about an orphan-cleanup candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum CleanupEntryState {
    /// No entry references the body.
    Unreferenced,
    /// An entry (one or several) references it.
    Referenced,
}

/// What the candidate's path holds when re-checked under its target guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum CleanupBodyIdentity {
    /// Already gone.
    Missing,
    /// The regular file inspection saw (same inode).
    Inspected,
    /// A regular file with a different inode: a newer body generation.
    Changed,
    /// Not a regular file.
    NotRegularFile,
    /// Metadata could not tell.
    Uncertain,
}

/// Facts that decide one orphan-body deletion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct CleanupFacts {
    /// The latest persisted manifest loaded cleanly (trusted inventory).
    pub manifest_trusted: bool,
    /// The candidate names the canonical body path of its id.
    pub path_matches: bool,
    /// What the latest manifest says about the id.
    pub entry: CleanupEntryState,
    /// The stable target guard atomic replacement uses is held, so no body
    /// write is in flight for this path.
    pub write_guard_held: bool,
    /// What the path holds now.
    pub identity: CleanupBodyIdentity,
}

/// What orphan cleanup does with one candidate body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrphanBodyDecision {
    /// Delete the body.
    Delete,
    /// Nothing to delete; record it as already absent.
    AlreadyAbsent,
    /// Keep the body, for this reason.
    Retain(DraftOrphanCleanupRetentionReason),
}

/// Decide one orphan-body deletion (S2).
#[must_use]
pub const fn orphan_body_decision(facts: CleanupFacts) -> OrphanBodyDecision {
    if !facts.manifest_trusted {
        return OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::StatusUncertain);
    }
    if !facts.path_matches {
        return OrphanBodyDecision::Retain(
            DraftOrphanCleanupRetentionReason::CandidatePathMismatch,
        );
    }
    if matches!(facts.entry, CleanupEntryState::Referenced) {
        return OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::ManifestEntryPresent);
    }
    if !facts.write_guard_held {
        return OrphanBodyDecision::Retain(DraftOrphanCleanupRetentionReason::StatusUncertain);
    }
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

// --- 6. reconciliation authority --------------------------------------------

/// The authority a manifest command leaves behind (S4).
///
/// Trusted only when the reconciliation was complete **and** its result was
/// durably written; a complete in-memory inventory that failed to persist is
/// still not authoritative.
#[must_use]
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

#[cfg(test)]
mod tests {
    //! Characterization of every decision the journal core took over from its
    //! call sites: each table is today's behaviour at those sites.

    use super::*;

    fn facts(body_present: bool, entry: EntryState, pending: bool, stale: bool) -> BodyFacts {
        BodyFacts {
            body_present,
            entry,
            restore_pending: pending,
            backing_stale: stale,
        }
    }

    #[test]
    fn ownership_orders_unseen_over_version_over_registration() {
        assert_eq!(
            ownership(facts(false, EntryState::Current, true, true)),
            BodyOwner::Nothing
        );
        assert_eq!(
            ownership(facts(true, EntryState::Superseded, true, true)),
            BodyOwner::Unseen
        );
        assert_eq!(
            ownership(facts(true, EntryState::Current, false, true)),
            BodyOwner::OtherVersion
        );
        assert_eq!(
            ownership(facts(true, EntryState::Superseded, false, false)),
            BodyOwner::OtherVersion
        );
        assert_eq!(
            ownership(facts(true, EntryState::Absent, false, false)),
            BodyOwner::Unregistered
        );
        assert_eq!(
            ownership(facts(true, EntryState::Current, false, false)),
            BodyOwner::Journal
        );
    }

    #[test]
    fn a_pending_restore_holds_every_write_even_before_registration() {
        // The restore hold in both autosave collectors.
        assert_eq!(
            body_write_decision(BodyOwner::Unseen, true),
            BodyWriteDecision::Hold
        );
        assert_eq!(
            body_write_decision(BodyOwner::Unseen, false),
            BodyWriteDecision::Hold
        );
        assert_eq!(
            body_write_decision(BodyOwner::Journal, true),
            BodyWriteDecision::RegisterFirst
        );
        // The registration overwrite copy.
        assert_eq!(
            body_write_decision(BodyOwner::OtherVersion, false),
            BodyWriteDecision::PreserveThenWrite
        );
        assert_eq!(
            body_write_decision(BodyOwner::Journal, false),
            BodyWriteDecision::Write
        );
        assert_eq!(
            body_write_decision(BodyOwner::Nothing, false),
            BodyWriteDecision::Write
        );
    }

    #[test]
    fn registration_is_required_for_unknown_or_untrusted_file_backed_ids() {
        assert!(!registration_required(true, true, true));
        assert!(registration_required(true, false, true));
        assert!(registration_required(true, true, false));
        assert!(registration_required(true, false, false));
        assert!(!registration_required(false, false, false));
        assert!(!registration_required(false, true, false));
        assert!(may_write_after_registration(false, false));
        assert!(may_write_after_registration(false, true));
        assert!(may_write_after_registration(true, true));
        assert!(!may_write_after_registration(true, false));
    }

    #[test]
    fn an_incomplete_reconciliation_admits_only_additive_inserts() {
        assert_eq!(
            untrusted_commit_disposition(true, true),
            UntrustedCommitDisposition::Additive
        );
        assert_eq!(
            untrusted_commit_disposition(true, false),
            UntrustedCommitDisposition::Refuse
        );
        assert_eq!(
            untrusted_commit_disposition(false, true),
            UntrustedCommitDisposition::Refuse
        );
    }

    #[test]
    fn run_deletion_performs_steps_in_order_and_never_after_a_failure() {
        let mut seen = Vec::new();
        let terminal = run_deletion(DeletionStep::Preserve, |step| {
            seen.push(step);
            true
        });
        assert_eq!(terminal, DeletionStep::Done);
        assert_eq!(
            seen,
            [
                DeletionStep::Preserve,
                DeletionStep::DeleteBody,
                DeletionStep::RemoveEntry
            ]
        );
        seen.clear();
        let terminal = run_deletion(DeletionStep::DeleteBody, |step| {
            seen.push(step);
            false
        });
        assert_eq!(terminal, DeletionStep::Stopped);
        assert_eq!(seen, [DeletionStep::DeleteBody]);
    }

    #[test]
    fn deletion_preserves_first_then_body_then_entry_and_stops_on_failure() {
        assert_eq!(deletion_start(BodyOwner::Unseen), DeletionStep::Preserve);
        assert_eq!(
            deletion_start(BodyOwner::OtherVersion),
            DeletionStep::Preserve
        );
        assert_eq!(deletion_start(BodyOwner::Journal), DeletionStep::DeleteBody);
        assert_eq!(deletion_start(BodyOwner::Nothing), DeletionStep::DeleteBody);
        assert_eq!(
            next_deletion_step(DeletionStep::Preserve, true),
            DeletionStep::DeleteBody
        );
        assert_eq!(
            next_deletion_step(DeletionStep::DeleteBody, true),
            DeletionStep::RemoveEntry
        );
        assert_eq!(
            next_deletion_step(DeletionStep::RemoveEntry, true),
            DeletionStep::Done
        );
        for step in [
            DeletionStep::Preserve,
            DeletionStep::DeleteBody,
            DeletionStep::RemoveEntry,
        ] {
            assert_eq!(next_deletion_step(step, false), DeletionStep::Stopped);
        }
        assert!(startup_may_retire_stale(true));
        assert!(!startup_may_retire_stale(false));
    }

    #[test]
    fn unapplied_restores_are_preserved_and_stale_ones_retired() {
        use RestoreDisposition::{Nothing, PreserveCopy, PreserveThenRetire};
        for (ending, expected) in [
            (RestoreEnding::Applied, Nothing),
            (RestoreEnding::Stale, PreserveThenRetire),
            (RestoreEnding::Oversized, PreserveCopy),
            (RestoreEnding::ReadFailed, PreserveCopy),
            (RestoreEnding::EditedOver, PreserveCopy),
            (RestoreEnding::InstallCancelled, PreserveCopy),
            (RestoreEnding::Unavailable, PreserveCopy),
            (RestoreEnding::MissingBody, Nothing),
        ] {
            assert_eq!(
                unapplied_restore_disposition(ending),
                expected,
                "{ending:?}"
            );
        }
        assert_eq!(
            RestoreEnding::from(FileDraftRestoreSkip::Stale),
            RestoreEnding::Stale
        );
        assert_eq!(
            RestoreEnding::from(FileDraftRestoreSkip::MissingDraft),
            RestoreEnding::MissingBody
        );
    }

    #[test]
    fn orphan_cleanup_deletes_only_an_unreferenced_revalidated_guarded_body() {
        let base = CleanupFacts {
            manifest_trusted: true,
            path_matches: true,
            entry: CleanupEntryState::Unreferenced,
            write_guard_held: true,
            identity: CleanupBodyIdentity::Inspected,
        };
        assert_eq!(orphan_body_decision(base), OrphanBodyDecision::Delete);
        assert_eq!(
            orphan_body_decision(CleanupFacts {
                identity: CleanupBodyIdentity::Missing,
                ..base
            }),
            OrphanBodyDecision::AlreadyAbsent
        );
        for (facts, reason) in [
            (
                CleanupFacts {
                    manifest_trusted: false,
                    ..base
                },
                DraftOrphanCleanupRetentionReason::StatusUncertain,
            ),
            (
                CleanupFacts {
                    path_matches: false,
                    ..base
                },
                DraftOrphanCleanupRetentionReason::CandidatePathMismatch,
            ),
            (
                CleanupFacts {
                    entry: CleanupEntryState::Referenced,
                    ..base
                },
                DraftOrphanCleanupRetentionReason::ManifestEntryPresent,
            ),
            (
                CleanupFacts {
                    write_guard_held: false,
                    ..base
                },
                DraftOrphanCleanupRetentionReason::StatusUncertain,
            ),
            (
                CleanupFacts {
                    identity: CleanupBodyIdentity::Changed,
                    ..base
                },
                DraftOrphanCleanupRetentionReason::BodyGenerationChanged,
            ),
            (
                CleanupFacts {
                    identity: CleanupBodyIdentity::NotRegularFile,
                    ..base
                },
                DraftOrphanCleanupRetentionReason::BodyNotRegularFile,
            ),
            (
                CleanupFacts {
                    identity: CleanupBodyIdentity::Uncertain,
                    ..base
                },
                DraftOrphanCleanupRetentionReason::StatusUncertain,
            ),
        ] {
            assert_eq!(
                orphan_body_decision(facts),
                OrphanBodyDecision::Retain(reason),
                "{facts:?}"
            );
        }
    }

    #[test]
    fn only_a_complete_durable_reconciliation_is_trusted() {
        assert!(commit_authority(DraftManifestCompleteness::Complete, true).is_trusted());
        assert!(!commit_authority(DraftManifestCompleteness::Complete, false).is_trusted());
        assert!(!commit_authority(DraftManifestCompleteness::Partial, true).is_trusted());
        assert!(!commit_authority(DraftManifestCompleteness::Failed, true).is_trusted());
        assert_eq!(
            commit_authority(DraftManifestCompleteness::Partial, true).completeness,
            DraftManifestCompleteness::Partial
        );
    }
}

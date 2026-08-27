// SPDX-License-Identifier: GPL-3.0-or-later

//! The draft workflow's seam value objects and worker-boundary payloads.
//!
//! Each of these crosses two or more boundaries or is reconstructed at two or
//! more call sites, which is what the seam rule targets. They are gathered here
//! rather than duplicated per role module, and every one of them exists to stop a
//! value from being **renamed while crossing a seam** — the archetype defect,
//! which on the cleanup path would authorize deleting the wrong body.
//!
//! `DraftRestoreTicket` + `DraftRestoreFacts` + `draft_restore_is_current` is the
//! established Ticket/Facts/predicate shape: the ticket captures the expectation
//! at dispatch, the facts capture observed live state, and one predicate
//! validates them as a unit so no positional scalar can arrive in the wrong
//! parameter.

use std::collections::HashMap;
use std::path::PathBuf;

use glib::prelude::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use crate::model::draft::{
    DraftEntry, DraftManifestAuthority, FileDraftRestoreSkip, PreloadedDraftSkip,
};
use crate::services::draft_service;
use crate::ui::editor_page::LushtextEditorPage;

use super::policy::DraftMutationIntent;
use crate::ui::window::LushtextWindow;

/// Main-thread editor token paired with one accepted autosave snapshot.
pub(super) struct DirtyDraftCompletion {
    /// Stable identity accepted by the body writer.
    pub(super) draft_id: String,
    /// Dirty generation that may be cleared after manifest acceptance.
    pub(super) dirty_generation: u64,
    /// Weak target so pending work never retains a closed tab.
    pub(super) editor: glib::WeakRef<LushtextEditorPage>,
    /// Main-thread intent assigned before snapshot admission.
    pub(super) intent: DraftMutationIntent,
}

/// Dirty editor state captured before autosave starts copying buffer text.
pub(super) struct DirtyDraftCandidate {
    /// Stable identity captured before snapshotting begins.
    pub(super) draft_id: String,
    /// Live path used to record file freshness metadata.
    pub(super) original_path: Option<PathBuf>,
    /// Generation that must still match before publishing the snapshot.
    pub(super) dirty_generation: u64,
    /// Weak editor used for freshness checks without extending tab lifetime.
    pub(super) editor: glib::WeakRef<LushtextEditorPage>,
    /// GTK-owned buffer read only by the main-loop snapshot stage.
    pub(super) buffer: sourceview5::Buffer,
    /// Main-thread intent assigned before snapshot admission.
    pub(super) intent: DraftMutationIntent,
}

/// Compact metadata retained after one draft body has been durably written.
pub(super) struct AcceptedDraft {
    pub(super) entry: DraftEntry,
    pub(super) completion: DirtyDraftCompletion,
}

/// Compact manifest failure returned from a worker with its proven authority.
pub(super) struct DraftManifestFailure {
    pub(super) authority: DraftManifestAuthority,
    pub(super) detail: String,
}

impl DraftManifestFailure {
    pub(super) fn injected(error: &anyhow::Error) -> Self {
        Self {
            authority: DraftManifestAuthority::default(),
            detail: error.to_string(),
        }
    }
}

impl From<draft_service::DraftManifestUpdateError> for DraftManifestFailure {
    fn from(error: draft_service::DraftManifestUpdateError) -> Self {
        Self {
            authority: error.authority(),
            detail: error.to_string(),
        }
    }
}

/// Typed close-safety failure used by callers and deterministic widget tests.
#[derive(Debug, thiserror::Error)]
pub enum DraftFlushError {
    /// One or more eligible drafts never reached manifest acceptance.
    #[error(
        "automatic recovery could not confirm {total} draft(s) (cancelled: {cancelled}, over limit: {over_limit}, body write: {body_write})"
    )]
    Unconfirmed {
        /// Total candidates that failed an acceptance stage.
        total: usize,
        /// Candidates cancelled or made stale before acceptance.
        cancelled: usize,
        /// Candidates whose UTF-8 body exceeded recovery policy.
        over_limit: usize,
        /// Candidates whose durable body write failed.
        body_write: usize,
    },
    /// Successful bodies could not be published through the shared manifest.
    #[error("failed to save draft manifest on close: {detail}")]
    Manifest {
        /// Strongest manifest authority proven by the failed command.
        authority: DraftManifestAuthority,
        /// Bounded diagnostic text for the retryable failure.
        detail: String,
    },
}

/// Complete freshness ticket shared by every asynchronous draft restore path.
#[derive(Clone)]
pub(crate) struct DraftRestoreTicket {
    /// Exact manifest generation resolved by the background reader.
    pub(super) entry: DraftEntry,
    /// Weak target so queued recovery cannot retain a closed tab.
    pub(super) editor: glib::WeakRef<LushtextEditorPage>,
    /// File identity captured before dispatch for stale rejection.
    pub(super) expected_path: Option<PathBuf>,
    /// Buffer generation that must match before applying recovered text.
    pub(super) dirty_generation: u64,
    /// File-load generation that prevents restore crossing a reopen.
    pub(super) load_generation: u64,
}

#[derive(Clone, Copy)]
pub(super) enum DraftRestoreTracking {
    Ordinary,
    Lazy,
}

pub(super) enum GuardedDraftRestoreResolution {
    Restore(crate::ui::plain_disposal::DisposalOwned<String>),
    Compact(FileDraftRestoreSkip),
}

pub(super) enum GuardedPreloadedDraftRestore {
    Content(crate::ui::plain_disposal::DisposalOwned<String>),
    Compact(PreloadedDraftSkip),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftRestoreFacts {
    pub(super) draft_id: Option<String>,
    pub(super) path: Option<PathBuf>,
    pub(super) dirty_generation: u64,
    pub(super) load_generation: u64,
    pub(super) manifest_entry: Option<DraftEntry>,
}

pub(super) fn draft_restore_is_current(
    ticket: &DraftRestoreTicket,
    facts: &DraftRestoreFacts,
) -> bool {
    facts.manifest_entry.as_ref() == Some(&ticket.entry)
        && facts.draft_id.as_deref() == Some(ticket.entry.draft_id.as_str())
        && facts.path == ticket.expected_path
        && facts.dirty_generation == ticket.dirty_generation
        && facts.load_generation == ticket.load_generation
}

impl DraftRestoreTicket {
    pub(super) fn capture(editor: &LushtextEditorPage, entry: DraftEntry) -> Self {
        Self {
            entry,
            editor: editor.downgrade(),
            expected_path: editor.file_path(),
            dirty_generation: editor.draft_dirty_generation(),
            load_generation: editor.load_generation(),
        }
    }

    pub(super) fn current_editor(&self, window: &LushtextWindow) -> Option<LushtextEditorPage> {
        let editor = self.editor.upgrade()?;
        let facts = DraftRestoreFacts {
            draft_id: editor.draft_id(),
            path: editor.file_path(),
            dirty_generation: editor.draft_dirty_generation(),
            load_generation: editor.load_generation(),
            manifest_entry: window
                .imp()
                .drafts
                .manifest
                .borrow()
                .find_by_id(&self.entry.draft_id)
                .cloned(),
        };
        draft_restore_is_current(self, &facts).then_some(editor)
    }
}

/// Worker-sized cleanup result used by the GTK completion callback.
///
/// The full persisted manifest is dropped on the worker. The adapter needs only
/// exact removals, grouped failures, and continuation state, avoiding a second
/// potentially large manifest allocation on the main loop.
pub(super) struct OrphanCleanupUiResult {
    pub(super) outcome: draft_service::DraftOrphanCleanupOutcome,
    pub(super) committed_by_id: HashMap<String, draft_service::DraftEntryFingerprint>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, saved_at_secs: u64) -> DraftEntry {
        DraftEntry {
            draft_id: id.to_string(),
            original_path: Some(PathBuf::from(format!("/{id}.rs"))),
            original_mtime_secs: Some(saved_at_secs),
            saved_at_secs,
        }
    }

    /// A ticket whose weak editor ref is deliberately empty: this test covers
    /// only the value-comparison half of the predicate, which is the half that
    /// needs no GTK object and therefore stays unit-testable.
    fn restore_ticket() -> DraftRestoreTicket {
        DraftRestoreTicket {
            entry: entry("restore", 1),
            editor: glib::WeakRef::new(),
            expected_path: Some(PathBuf::from("/restore.rs")),
            dirty_generation: 3,
            load_generation: 5,
        }
    }

    fn restore_facts() -> DraftRestoreFacts {
        let ticket = restore_ticket();
        DraftRestoreFacts {
            draft_id: Some(ticket.entry.draft_id.clone()),
            path: ticket.expected_path.clone(),
            dirty_generation: ticket.dirty_generation,
            load_generation: ticket.load_generation,
            manifest_entry: Some(ticket.entry),
        }
    }

    /// Each of the five identity dimensions is rejected **on its own**.
    ///
    /// Perturbing one field at a time is the point: a predicate that dropped
    /// any single `&&` term would still accept a facts value that differs in
    /// several dimensions at once, so a combined-mismatch test cannot catch it.
    /// Restoring a draft across a stale generation writes one document's
    /// recovered text into another's buffer, so every term is load-bearing.
    #[test]
    fn restore_ticket_rejects_every_stale_identity_dimension() {
        let ticket = restore_ticket();
        assert!(
            draft_restore_is_current(&ticket, &restore_facts()),
            "unperturbed facts must be accepted"
        );

        let mut edited = restore_facts();
        edited.dirty_generation += 1;
        assert!(
            !draft_restore_is_current(&ticket, &edited),
            "an edit after dispatch must reject the restore"
        );

        let mut reloaded = restore_facts();
        reloaded.load_generation += 1;
        assert!(
            !draft_restore_is_current(&ticket, &reloaded),
            "a reopen after dispatch must reject the restore"
        );

        let mut renamed = restore_facts();
        renamed.path = Some(PathBuf::from("/renamed.rs"));
        assert!(
            !draft_restore_is_current(&ticket, &renamed),
            "a path change must reject the restore"
        );

        let mut cleared = restore_facts();
        cleared.path = None;
        assert!(
            !draft_restore_is_current(&ticket, &cleared),
            "an untitled tab must not accept a file-backed ticket"
        );

        let mut reused = restore_facts();
        reused.draft_id = Some("different".to_string());
        assert!(
            !draft_restore_is_current(&ticket, &reused),
            "a different draft id must reject the restore"
        );

        let mut missing_id = restore_facts();
        missing_id.draft_id = None;
        assert!(
            !draft_restore_is_current(&ticket, &missing_id),
            "an absent draft id must reject rather than default to a match"
        );

        let mut replaced = restore_facts();
        replaced.manifest_entry = Some(entry("restore", 2));
        assert!(
            !draft_restore_is_current(&ticket, &replaced),
            "a newer same-id manifest generation must reject the restore"
        );

        let mut absent = restore_facts();
        absent.manifest_entry = None;
        assert!(
            !draft_restore_is_current(&ticket, &absent),
            "a deleted manifest entry must reject the restore"
        );
    }
}

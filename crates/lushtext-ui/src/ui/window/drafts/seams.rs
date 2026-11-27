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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;

use glib::prelude::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::SupersedingTimer;
use gtk4::glib;

use crate::ui::buffer_snapshot::BufferSnapshotHandle;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::plain_disposal::{DisposalOwned, ProgressDisposalCapacityWakeup};
use lushtext_model::persistence::draft::{
    DraftEntry, DraftManifest, DraftManifestAuthority, FileDraftRestoreSkip, PreloadedDraftRestore,
    PreloadedDraftSkip,
};
use lushtext_model::values::{DraftGeneration, LoadGeneration};
use lushtext_services::persistence::draft_service;

use super::InjectedDraftTestError;
use super::admission::ProcessDraftJournal;
use super::policy::{DraftMutationIntent, DraftMutationOrder};
use crate::ui::window::LushtextWindow;

/// Main-thread editor token paired with one accepted autosave snapshot.
pub(super) struct DirtyDraftCompletion {
    /// Stable identity accepted by the body writer.
    pub(super) draft_id: String,
    /// Dirty generation that may be cleared after manifest acceptance.
    pub(super) dirty_generation: DraftGeneration,
    /// Weak target so pending work never retains a closed tab.
    pub(super) editor: glib::WeakRef<LushtextEditorPage>,
    /// Main-thread intent assigned before snapshot admission.
    pub(super) intent: DraftMutationIntent,
}

/// Dirty editor state captured before autosave starts copying buffer text.
///
/// It holds no body-write token: a candidate becomes writable only through
/// [`DirtyDraftCandidate::into_writable`], so the pipelines, which take only
/// [`WritableDraftCandidate`], cannot be handed one whose id was never
/// registered.
pub(super) struct DirtyDraftCandidate {
    /// Stable identity captured before snapshotting begins.
    pub(super) draft_id: String,
    /// Live path used to record file freshness metadata.
    pub(super) original_path: Option<PathBuf>,
    /// Generation that must still match before publishing the snapshot.
    pub(super) dirty_generation: DraftGeneration,
    /// Weak editor used for freshness checks without extending tab lifetime.
    pub(super) editor: glib::WeakRef<LushtextEditorPage>,
    /// GTK-owned buffer read only by the main-loop snapshot stage.
    pub(super) buffer: sourceview5::Buffer,
    /// Main-thread intent assigned before snapshot admission.
    pub(super) intent: DraftMutationIntent,
}

impl DirtyDraftCandidate {
    /// Pair this candidate with the body-write token for its own id.
    ///
    /// The only way to make a [`WritableDraftCandidate`]. A token minted for
    /// another id is refused, and both values come back unchanged, so the
    /// caller still holds the candidate (to report its id as refused) and the
    /// token (to pair with its own candidate).
    #[expect(
        clippy::result_large_err,
        reason = "the Err hands the candidate back by move, as large as the Ok; boxing it \
                  would allocate to return a value the caller already owned"
    )]
    pub(super) fn into_writable(
        self,
        token: draft_service::RegisteredDraft,
    ) -> Result<WritableDraftCandidate, (Self, draft_service::RegisteredDraft)> {
        let draft_id = self.draft_id.clone();
        claim_token(self, &draft_id, token).map(|(candidate, registered)| WritableDraftCandidate {
            candidate,
            registered,
        })
    }
}

/// The check behind [`DirtyDraftCandidate::into_writable`]: `owner` (whose id
/// is `owner_id`) keeps `token` only when the token authorizes that id.
///
/// Generic over the owner so the check is unit-testable without the GTK
/// buffer a real candidate holds.
fn claim_token<T>(
    owner: T,
    owner_id: &str,
    token: draft_service::RegisteredDraft,
) -> Result<(T, draft_service::RegisteredDraft), (T, draft_service::RegisteredDraft)> {
    if token.draft_id() == owner_id {
        Ok((owner, token))
    } else {
        Err((owner, token))
    }
}

/// A dirty candidate together with the body-write token for its id: the only
/// candidate the autosave and asynchronous close pipelines take.
///
/// Its fields are private to this module, so it exists only by
/// [`DirtyDraftCandidate::into_writable`], which checked the pairing; the
/// token reaches the worker's `write_draft` call by move, never cloned
/// (`RegisteredDraft` is not `Clone`).
pub(super) struct WritableDraftCandidate {
    candidate: DirtyDraftCandidate,
    registered: draft_service::RegisteredDraft,
}

impl WritableDraftCandidate {
    /// The candidate and its token, for the stage that snapshots the body and
    /// hands the token to the worker that writes it.
    pub(super) fn into_parts(self) -> (DirtyDraftCandidate, draft_service::RegisteredDraft) {
        (self.candidate, self.registered)
    }
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
    pub(super) fn injected(error: &InjectedDraftTestError) -> Self {
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

/// Typed body-stage error returned by the worker writing one draft body.
#[derive(Debug, thiserror::Error)]
pub(super) enum DraftBodyStageError {
    /// Test-only injected body write failure.
    #[error(transparent)]
    Injected(#[from] InjectedDraftTestError),
    /// Service-level durable body write failure.
    #[error(transparent)]
    Service(#[from] draft_service::DraftBodyWriteError),
}

/// Typed errors returned by the synchronous draft flush helper.
#[derive(Debug, thiserror::Error)]
pub enum DraftSyncFlushError {
    /// Another draft journal transaction or cleanup pass was already running.
    #[error("draft persistence is already in progress")]
    AlreadyInProgress,
    /// Updating the draft manifest on close failed.
    #[error("failed to save draft manifest on close: {detail}")]
    ManifestSave {
        /// Diagnostic summary from manifest registration or commit.
        detail: String,
    },
    /// Writing one or more draft bodies on close failed.
    #[error("failed to write {count} drafts on close: {details}")]
    BodyWrite {
        /// Number of draft bodies that failed to write.
        count: usize,
        /// Joined failure details.
        details: String,
    },
}

/// Typed close-safety failure used by callers and deterministic widget tests.
#[derive(Debug, thiserror::Error)]
pub enum DraftFlushError {
    /// One or more drafts are still restoring or owned by another window.
    #[error(
        "{count} drafts are still being restored or are owned by another window; close remains retryable"
    )]
    RestoreBlocked {
        /// Number of drafts blocked from close flush.
        count: usize,
    },
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
    pub(super) dirty_generation: DraftGeneration,
    /// File-load generation that prevents restore crossing a reopen.
    pub(super) load_generation: LoadGeneration,
}

#[derive(Clone, Copy)]
pub(super) enum DraftRestoreTracking {
    Ordinary,
    Lazy,
}

pub(super) enum GuardedDraftRestoreResolution {
    Restore(
        crate::ui::plain_disposal::DisposalOwned<String>,
        // Boxed: the profile (with its optional fold plan) is most of the
        // variant, and the compact variant is a byte.
        Box<lushtext_model::document::paragraph_boundary::DocumentProfile>,
    ),
    Compact(FileDraftRestoreSkip),
}

pub(super) enum GuardedPreloadedDraftRestore {
    Content(
        crate::ui::plain_disposal::DisposalOwned<String>,
        // Boxed: the profile (with its optional fold plan) is most of the
        // variant, and the compact variant is a byte.
        Box<lushtext_model::document::paragraph_boundary::DocumentProfile>,
    ),
    Compact(PreloadedDraftSkip),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftRestoreFacts {
    pub(super) draft_id: Option<String>,
    pub(super) path: Option<PathBuf>,
    pub(super) dirty_generation: DraftGeneration,
    pub(super) load_generation: LoadGeneration,
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
            dirty_generation: editor.draft_generation(),
            load_generation: editor.typed_load_generation(),
        }
    }

    pub(super) fn current_editor(&self, window: &LushtextWindow) -> Option<LushtextEditorPage> {
        let editor = self.editor.upgrade()?;
        let facts = DraftRestoreFacts {
            draft_id: editor.draft_id(),
            path: editor.file_path(),
            dirty_generation: editor.draft_generation(),
            load_generation: editor.typed_load_generation(),
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

/// A queued delete whose worker must first preserve the body.
///
/// `announce_as_stale` selects the inline "file changed on disk" warning; any
/// other preserve-first delete (a save or discard while the restore was still
/// pending) reports where the earlier edits went in the status bar instead.
#[derive(Clone, Debug)]
pub(crate) struct PendingPreservation {
    /// The persisted entry whose body is preserved before deletion.
    pub(crate) entry: lushtext_model::persistence::draft::DraftEntry,
    /// Whether the outcome is announced as a stale-draft inline warning.
    pub(crate) announce_as_stale: bool,
}

/// One-shot timer armed state for the first dirty draft after a clean cycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FirstDirtyTimerPhase {
    #[default]
    Disarmed,
    Armed,
}

/// Orphan-cleanup timer armed state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum OrphanCleanupTimerPhase {
    #[default]
    Idle,
    Armed,
}

/// Orphan-cleanup worker lifecycle and any coalesced follow-up manifest offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum OrphanCleanupWorkerPhase {
    #[default]
    Idle,
    Inflight {
        pending_offset: Option<usize>,
    },
}

/// Window-local draft mutation and autosave execution lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DraftMutationPhase {
    Idle {
        autosave_pending: bool,
    },
    AutosaveInflight {
        mutation_active: bool,
        autosave_pending: bool,
    },
    MutationInflight {
        autosave_pending: bool,
    },
}

impl Default for DraftMutationPhase {
    fn default() -> Self {
        Self::Idle {
            autosave_pending: false,
        }
    }
}

/// Cancellation handle for whichever chunked draft buffer capture is active.
#[derive(Default)]
pub(super) enum ActiveDraftSnapshot {
    #[default]
    None,
    Autosave(BufferSnapshotHandle),
    Close(BufferSnapshotHandle),
}

/// Bounded lazy draft recovery read across the worker boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum LazyRestorePhase {
    #[default]
    Idle,
    Inflight,
}

/// This window's membership in its application's shared draft journal.
#[derive(Default)]
pub(super) enum DraftJournalMembership {
    #[default]
    Unjoined,
    Joined {
        key: u64,
        journal: Rc<ProcessDraftJournal>,
    },
}

/// Draft persistence and autosave state owned by the window shell.
#[derive(Default)]
pub struct DraftState {
    /// Source ID for the global autosave timer. Removed on dispose.
    pub autosave_source_id: RefCell<Option<glib::SourceId>>,
    /// Periodic autosave ticks run so far: the clock unrestored-copy retries
    /// are scheduled on. Never persisted.
    pub(super) autosave_timer_ticks: Cell<u64>,
    /// Superseding one-shot for the first dirty draft after a clean cycle.
    pub first_dirty_autosave_timer: SupersedingTimer,
    /// Whether the first-dirty timer currently has a callback armed.
    pub(super) first_dirty_timer_phase: Cell<FirstDirtyTimerPhase>,
    /// Superseding startup/follow-up timer for bounded orphan cleanup.
    pub orphan_cleanup_timer: SupersedingTimer,
    /// Whether the owned orphan-cleanup timer currently has a callback armed.
    orphan_cleanup_timer_phase: Cell<OrphanCleanupTimerPhase>,
    /// Active orphan-cleanup worker and latest requested manifest offset.
    orphan_cleanup_worker: Cell<OrphanCleanupWorkerPhase>,
    /// Consecutive retryable cleanup failures used for bounded backoff.
    pub orphan_cleanup_failure_streak: Cell<u32>,
    /// Number of orphan-cleanup workers started by this window.
    pub(super) orphan_cleanup_workers_started: Cell<usize>,
    /// Peak simultaneous orphan-cleanup workers observed by this window.
    pub(super) orphan_cleanup_workers_high_water: Cell<usize>,
    /// In-memory draft manifest kept in sync with disk.
    pub manifest: RefCell<DraftManifest>,
    /// Completeness and durable replacement authority for the in-memory manifest.
    pub manifest_authority: Cell<DraftManifestAuthority>,
    /// Draft restore outcomes preloaded during session restore and consumed once.
    pub preloaded: RefCell<DisposalOwned<HashMap<String, PreloadedDraftRestore>>>,
    /// Autosave / mutation execution lane and coalesced autosave rerun flag.
    mutation: Cell<DraftMutationPhase>,
    /// Main-thread intent allocator shared by autosave and deletion workflows.
    pub(super) mutation_order: RefCell<DraftMutationOrder>,
    /// Compact deletes waiting behind an earlier draft mutation.
    pub(super) pending_deletes: RefCell<VecDeque<DraftMutationIntent>>,
    /// IDs represented in `pending_deletes`, keeping common admission O(1).
    pub(super) pending_delete_ids: RefCell<HashSet<String>>,
    /// Current deletion intents whose durable manifest removal may outlive a
    /// failed body deletion and must remain explicit on a later retry.
    pub(super) delete_tombstones: RefCell<HashMap<String, DraftMutationIntent>>,
    /// Stale file-backed drafts whose queued delete must first preserve the body.
    pub(super) stale_preservations: RefCell<HashMap<String, PendingPreservation>>,
    /// Draft ids whose recovery body is queued or being read for restore.
    pub(super) restore_pending_ids: RefCell<HashMap<String, usize>>,
    /// Unrestored recovery bodies whose set-aside copy failed.
    pub(super) unrestored_copy_retries: RefCell<HashMap<String, (DraftEntry, u32, u64)>>,
    /// Cancellation token for the active chunked autosave or close buffer copy.
    active_snapshot: RefCell<ActiveDraftSnapshot>,
    /// Draft IDs explicitly discarded during an in-progress close flow.
    pub close_discard_ids: RefCell<HashSet<String>>,
    /// Serialized non-preloaded recovery reads, including startup budget skips.
    pub(super) lazy_restore_queue: RefCell<VecDeque<DraftRestoreTicket>>,
    /// Whether one lazy draft body is currently crossing the worker boundary.
    lazy_restore: Cell<LazyRestorePhase>,
    /// One paced progress-lane wakeup retained while a lazy ticket stays compact.
    pub(in crate::ui::window) lazy_restore_capacity_wakeup: ProgressDisposalCapacityWakeup,
    /// Number of asynchronous draft resolutions not yet delivered to GTK.
    pub(super) restore_inflight_count: Cell<usize>,
    /// Number of complete autosave bodies currently held across a worker handoff.
    pub(super) retained_complete_bodies: Cell<usize>,
    /// Peak complete-body count observed by this window.
    pub(super) max_retained_complete_bodies: Cell<usize>,
    /// This window's membership in the application's shared draft journal.
    pub(super) journal_membership: RefCell<DraftJournalMembership>,
    /// Draft ids whose autosave this window holds because another window owns them.
    pub(super) claim_holds_reported: RefCell<HashSet<String>>,
}

impl DraftState {
    pub(in crate::ui::window) fn is_autosave_inflight(&self) -> bool {
        matches!(
            self.mutation.get(),
            DraftMutationPhase::AutosaveInflight { .. }
        )
    }

    pub(in crate::ui::window) fn is_mutation_inflight(&self) -> bool {
        matches!(
            self.mutation.get(),
            DraftMutationPhase::AutosaveInflight {
                mutation_active: true,
                ..
            } | DraftMutationPhase::MutationInflight { .. }
        )
    }

    pub fn is_autosave_pending(&self) -> bool {
        matches!(
            self.mutation.get(),
            DraftMutationPhase::Idle {
                autosave_pending: true
            } | DraftMutationPhase::AutosaveInflight {
                autosave_pending: true,
                ..
            } | DraftMutationPhase::MutationInflight {
                autosave_pending: true
            }
        )
    }

    pub fn set_autosave_pending(&self, pending: bool) {
        let next = match self.mutation.get() {
            DraftMutationPhase::Idle { .. } => DraftMutationPhase::Idle {
                autosave_pending: pending,
            },
            DraftMutationPhase::AutosaveInflight {
                mutation_active, ..
            } => DraftMutationPhase::AutosaveInflight {
                mutation_active,
                autosave_pending: pending,
            },
            DraftMutationPhase::MutationInflight { .. } => DraftMutationPhase::MutationInflight {
                autosave_pending: pending,
            },
        };
        self.mutation.set(next);
    }

    pub(super) fn replace_autosave_pending(&self, pending: bool) -> bool {
        let previous = self.is_autosave_pending();
        self.set_autosave_pending(pending);
        previous
    }

    pub(super) fn begin_autosave_run(&self) {
        let autosave_pending = self.is_autosave_pending();
        self.mutation.set(DraftMutationPhase::AutosaveInflight {
            mutation_active: true,
            autosave_pending,
        });
    }

    pub(super) fn finish_autosave_run(&self) {
        let autosave_pending = self.is_autosave_pending();
        self.mutation
            .set(DraftMutationPhase::Idle { autosave_pending });
    }

    pub(super) fn begin_mutation_run(&self) {
        let autosave_pending = self.is_autosave_pending();
        let next = match self.mutation.get() {
            DraftMutationPhase::AutosaveInflight { .. } => DraftMutationPhase::AutosaveInflight {
                mutation_active: true,
                autosave_pending,
            },
            _ => DraftMutationPhase::MutationInflight { autosave_pending },
        };
        self.mutation.set(next);
    }

    pub(super) fn finish_mutation_run(&self) {
        let autosave_pending = self.is_autosave_pending();
        let next = match self.mutation.get() {
            DraftMutationPhase::AutosaveInflight { .. } => DraftMutationPhase::AutosaveInflight {
                mutation_active: false,
                autosave_pending,
            },
            _ => DraftMutationPhase::Idle { autosave_pending },
        };
        self.mutation.set(next);
    }

    pub(super) fn finish_close_flush_run(&self) {
        self.mutation.set(DraftMutationPhase::Idle {
            autosave_pending: false,
        });
    }

    #[cfg(feature = "test-utils")]
    pub fn set_autosave_inflight(&self, inflight: bool) {
        let autosave_pending = self.is_autosave_pending();
        let next = match (inflight, self.mutation.get()) {
            (
                true,
                DraftMutationPhase::AutosaveInflight {
                    mutation_active, ..
                },
            ) => DraftMutationPhase::AutosaveInflight {
                mutation_active,
                autosave_pending,
            },
            (true, DraftMutationPhase::MutationInflight { .. }) => {
                DraftMutationPhase::AutosaveInflight {
                    mutation_active: true,
                    autosave_pending,
                }
            }
            (true, DraftMutationPhase::Idle { .. }) => DraftMutationPhase::AutosaveInflight {
                mutation_active: false,
                autosave_pending,
            },
            (
                false,
                DraftMutationPhase::AutosaveInflight {
                    mutation_active: true,
                    ..
                },
            ) => DraftMutationPhase::MutationInflight { autosave_pending },
            (false, _) => DraftMutationPhase::Idle { autosave_pending },
        };
        self.mutation.set(next);
    }

    pub(in crate::ui::window) fn is_first_dirty_autosave_pending(&self) -> bool {
        self.first_dirty_timer_phase.get() == FirstDirtyTimerPhase::Armed
    }

    pub(super) fn set_first_dirty_autosave_pending(&self, pending: bool) {
        self.first_dirty_timer_phase.set(if pending {
            FirstDirtyTimerPhase::Armed
        } else {
            FirstDirtyTimerPhase::Disarmed
        });
    }

    pub(super) fn is_orphan_cleanup_inflight(&self) -> bool {
        matches!(
            self.orphan_cleanup_worker.get(),
            OrphanCleanupWorkerPhase::Inflight { .. }
        )
    }

    pub(super) fn is_orphan_cleanup_timer_pending(&self) -> bool {
        self.orphan_cleanup_timer_phase.get() == OrphanCleanupTimerPhase::Armed
    }

    pub(super) fn set_orphan_cleanup_timer_pending(&self, pending: bool) {
        self.orphan_cleanup_timer_phase.set(if pending {
            OrphanCleanupTimerPhase::Armed
        } else {
            OrphanCleanupTimerPhase::Idle
        });
    }

    pub(super) fn begin_orphan_cleanup_worker(&self) {
        self.orphan_cleanup_worker
            .set(OrphanCleanupWorkerPhase::Inflight {
                pending_offset: None,
            });
    }

    pub(super) fn set_orphan_cleanup_pending_offset(&self, offset: usize) {
        if self.is_orphan_cleanup_inflight() {
            self.orphan_cleanup_worker
                .set(OrphanCleanupWorkerPhase::Inflight {
                    pending_offset: Some(offset),
                });
        }
    }

    pub(super) fn clear_orphan_cleanup_pending_offset(&self) {
        if self.is_orphan_cleanup_inflight() {
            self.orphan_cleanup_worker
                .set(OrphanCleanupWorkerPhase::Inflight {
                    pending_offset: None,
                });
        }
    }

    pub(super) fn finish_orphan_cleanup_worker(&self) -> Option<usize> {
        let pending_offset = match self.orphan_cleanup_worker.get() {
            OrphanCleanupWorkerPhase::Idle => None,
            OrphanCleanupWorkerPhase::Inflight { pending_offset } => pending_offset,
        };
        self.orphan_cleanup_worker
            .set(OrphanCleanupWorkerPhase::Idle);
        pending_offset
    }

    pub(super) fn orphan_cleanup_pending_offset(&self) -> Option<usize> {
        match self.orphan_cleanup_worker.get() {
            OrphanCleanupWorkerPhase::Idle => None,
            OrphanCleanupWorkerPhase::Inflight { pending_offset } => pending_offset,
        }
    }

    pub(super) fn set_autosave_snapshot(&self, snapshot: BufferSnapshotHandle) {
        *self.active_snapshot.borrow_mut() = ActiveDraftSnapshot::Autosave(snapshot);
    }

    pub(super) fn take_autosave_snapshot(&self) -> Option<BufferSnapshotHandle> {
        let mut slot = self.active_snapshot.borrow_mut();
        if matches!(*slot, ActiveDraftSnapshot::Autosave(_)) {
            match std::mem::take(&mut *slot) {
                ActiveDraftSnapshot::Autosave(handle) => Some(handle),
                _ => None,
            }
        } else {
            None
        }
    }

    pub(super) fn set_close_snapshot(&self, snapshot: BufferSnapshotHandle) {
        *self.active_snapshot.borrow_mut() = ActiveDraftSnapshot::Close(snapshot);
    }

    pub(super) fn take_close_snapshot(&self) -> Option<BufferSnapshotHandle> {
        let mut slot = self.active_snapshot.borrow_mut();
        if matches!(*slot, ActiveDraftSnapshot::Close(_)) {
            match std::mem::take(&mut *slot) {
                ActiveDraftSnapshot::Close(handle) => Some(handle),
                _ => None,
            }
        } else {
            None
        }
    }

    pub(super) fn is_lazy_restore_inflight(&self) -> bool {
        self.lazy_restore.get() == LazyRestorePhase::Inflight
    }

    pub(super) fn set_lazy_restore_inflight(&self, inflight: bool) {
        self.lazy_restore.set(if inflight {
            LazyRestorePhase::Inflight
        } else {
            LazyRestorePhase::Idle
        });
    }

    /// Cancel owned orphan-cleanup scheduling: window teardown, lost manifest
    /// authority, or a pass that has nothing left to do.
    pub(super) fn dispose_orphan_cleanup(&self) {
        self.set_orphan_cleanup_timer_pending(false);
        self.clear_orphan_cleanup_pending_offset();
        let _ = self.orphan_cleanup_timer.invalidate();
    }

    /// Cancel owned timers on window teardown.
    pub(in crate::ui::window) fn dispose_timers(&self) {
        if let Some(source_id) = self.autosave_source_id.take() {
            source_id.remove();
        }
        self.set_first_dirty_autosave_pending(false);
        let _ = self.first_dirty_autosave_timer.invalidate();
        self.dispose_orphan_cleanup();
    }

    /// Cancel any active chunked draft snapshot on window teardown.
    pub(in crate::ui::window) fn dispose_snapshots(&self) {
        match std::mem::take(&mut *self.active_snapshot.borrow_mut()) {
            ActiveDraftSnapshot::None => {}
            ActiveDraftSnapshot::Autosave(snapshot) | ActiveDraftSnapshot::Close(snapshot) => {
                snapshot.dispose();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn untitled_token(draft_id: &str) -> draft_service::RegisteredDraft {
        draft_service::RegisteredDraft::without_registration(
            draft_id,
            draft_service::journal_core::RegistrationFacts {
                file_backed: false,
                registered_in_known_manifest: false,
                known_manifest_trusted: false,
            },
        )
        .expect("an untitled id needs no registration")
    }

    /// The pairing check behind `into_writable` keeps a token only for its
    /// own id, and gives both values back otherwise.
    #[test]
    fn into_writable_refuses_a_token_for_a_different_id() {
        let (owner, token) = claim_token("candidate", "untitled-a", untitled_token("untitled-a"))
            .expect("a token for the candidate's own id is accepted");
        assert_eq!(owner, "candidate");
        assert_eq!(token.draft_id(), "untitled-a");

        let (owner, token) = claim_token("candidate", "untitled-a", untitled_token("untitled-b"))
            .expect_err("a token for another id is refused");
        assert_eq!(owner, "candidate", "the refused candidate comes back");
        assert_eq!(
            token.draft_id(),
            "untitled-b",
            "the refused token comes back for its own candidate"
        );
    }

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
            dirty_generation: DraftGeneration::new(3),
            load_generation: LoadGeneration::new(5),
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
        edited.dirty_generation = edited.dirty_generation.next();
        assert!(
            !draft_restore_is_current(&ticket, &edited),
            "an edit after dispatch must reject the restore"
        );

        let mut reloaded = restore_facts();
        reloaded.load_generation = reloaded.load_generation.next();
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft persistence, recovery, and autosave flows for the main window.
//!
//! This slice owns the data-safety-sensitive draft lifecycle: close-time flush,
//! crash recovery, autosave, and manifest maintenance. Session-only tab-state
//! capture lives separately in `session_persistence.rs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "test-utils")]
use std::sync::{Mutex, mpsc::Sender};
use std::time::Duration;

use crate::model::draft::{
    DraftEntry, DraftManifestAuthority, FileDraftRestoreResolution, PreloadedDraftRestore,
};
use crate::services::notifications::{
    InlineActionNotification, InlineNotificationStyle, NotificationSeverity,
};
use crate::services::{draft_service, editor_io, json_store};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::{
    BufferReplacementOutcome, BufferReplacementRequest, BufferReplacementTicket,
    BufferReplacementWorkflow, LushtextEditorPage,
};
use anyhow::Result;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

use super::draft_ordering::DraftMutationIntent;

/// First-dirty draft autosave delay after a clean edit cycle.
///
/// 750ms persists new unsaved work sooner than the regular 5s autosave tick
/// while still coalescing quick typing into one draft write.
const FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS: u64 = 750;

/// Delay before startup releases preloaded bodies and begins orphan inspection.
///
/// Two seconds lets restored editors consume their recovery snapshots before a
/// background cleanup worker revalidates the same persisted artifacts.
#[cfg(not(feature = "test-utils"))]
const ORPHAN_CLEANUP_START_DELAY: Duration = Duration::from_secs(2);
/// Delay for the one permitted follow-up bounded cleanup pass.
///
/// Thirty seconds avoids a tight retry loop when permissions or storage remain
/// unavailable while still making progress on a directory that exceeded the cap.
const ORPHAN_CLEANUP_FOLLOWUP_DELAY: Duration = Duration::from_secs(30);
/// Maximum delay between retryable orphan-cleanup attempts.
const ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF: Duration = Duration::from_secs(15 * 60);
/// Low-frequency close/readiness poll while ordered recovery work drains.
const DRAFT_MUTATION_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);
/// Conservative pre-read reservation for one maximum automatic draft body.
const DRAFT_RESTORE_DISPOSAL_RESERVATION_BYTES: u64 =
    draft_service::MAX_AUTOMATIC_DRAFT_BYTES.saturating_add(1024 * 1024);

#[cfg(feature = "test-utils")]
/// Test override for first-dirty autosave timing without changing production policy.
static FIRST_DIRTY_AUTOSAVE_DELAY_MS: AtomicU64 = AtomicU64::new(FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS);
#[cfg(feature = "test-utils")]
/// Test override for the automatic recovery byte limit without huge fixtures.
static AUTOMATIC_DRAFT_LIMIT_BYTES: AtomicU64 =
    AtomicU64::new(draft_service::MAX_AUTOMATIC_DRAFT_BYTES);
#[cfg(feature = "test-utils")]
/// Test-only worker delay for deterministic stale restore completions.
static DRAFT_RESTORE_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static DRAFT_BODY_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static DRAFT_MANIFEST_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static DRAFT_MANIFEST_COMPLETION_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static DRAFT_DELETE_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static ORPHAN_CLEANUP_START_DELAY_MS: AtomicU64 = AtomicU64::new(2_000);
#[cfg(feature = "test-utils")]
static ORPHAN_CLEANUP_FOLLOWUP_DELAY_MS: AtomicU64 = AtomicU64::new(30_000);
#[cfg(feature = "test-utils")]
static ORPHAN_CLEANUP_WORKER_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static NEXT_DRAFT_BODY_DISPOSAL_PROBE: Mutex<Option<Sender<std::thread::ThreadId>>> =
    Mutex::new(None);

/// Observe the worker thread that finally retires the next restored draft body.
#[cfg(feature = "test-utils")]
pub fn set_next_draft_body_disposal_probe_for_test(sender: Sender<std::thread::ThreadId>) {
    NEXT_DRAFT_BODY_DISPOSAL_PROBE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace(sender);
}

fn attach_draft_body_disposal_probe(
    owner: crate::ui::plain_disposal::DisposalOwned<String>,
) -> crate::ui::plain_disposal::DisposalOwned<String> {
    #[cfg(feature = "test-utils")]
    {
        let sender = NEXT_DRAFT_BODY_DISPOSAL_PROBE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(sender) = sender {
            return owner.with_disposal_terminal(move || {
                let _ = sender.send(std::thread::current().id());
            });
        }
    }
    owner
}
#[cfg(feature = "test-utils")]
static FAIL_NEXT_DRAFT_BODY: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "test-utils")]
static FAIL_NEXT_DRAFT_MANIFEST: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "test-utils")]
static FAIL_NEXT_DRAFT_DELETE: AtomicBool = AtomicBool::new(false);

/// Configure the first-dirty autosave debounce for timing-sensitive widget tests.
#[cfg(feature = "test-utils")]
pub fn set_first_dirty_autosave_delay_for_test(delay_ms: u64) {
    FIRST_DIRTY_AUTOSAVE_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Configure the automatic draft limit for focused widget tests.
#[cfg(feature = "test-utils")]
pub fn set_automatic_draft_limit_for_test(max_bytes: u64) {
    AUTOMATIC_DRAFT_LIMIT_BYTES.store(max_bytes, Ordering::Release);
}

/// Delay every asynchronous draft read for deterministic freshness tests.
#[cfg(feature = "test-utils")]
pub fn set_draft_restore_delay_for_test(delay_ms: u64) {
    DRAFT_RESTORE_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Backwards-compatible name for existing aggregate-budget tests.
#[cfg(feature = "test-utils")]
pub fn set_lazy_draft_read_delay_for_test(delay_ms: u64) {
    set_draft_restore_delay_for_test(delay_ms);
}

/// Delay body, manifest, and delete stages independently for ordered race tests.
#[cfg(feature = "test-utils")]
pub fn set_draft_mutation_delays_for_test(body_ms: u64, manifest_ms: u64, delete_ms: u64) {
    DRAFT_BODY_DELAY_MS.store(body_ms, Ordering::Release);
    DRAFT_MANIFEST_DELAY_MS.store(manifest_ms, Ordering::Release);
    DRAFT_DELETE_DELAY_MS.store(delete_ms, Ordering::Release);
}

/// Delay worker return after a manifest upsert is already durable.
#[cfg(feature = "test-utils")]
pub fn set_draft_manifest_completion_delay_for_test(delay_ms: u64) {
    DRAFT_MANIFEST_COMPLETION_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Inject one failure at each selected production-routed mutation stage.
#[cfg(feature = "test-utils")]
pub fn fail_next_draft_mutations_for_test(body: bool, manifest: bool, delete: bool) {
    FAIL_NEXT_DRAFT_BODY.store(body, Ordering::Release);
    FAIL_NEXT_DRAFT_MANIFEST.store(manifest, Ordering::Release);
    FAIL_NEXT_DRAFT_DELETE.store(delete, Ordering::Release);
}

/// Configure orphan-cleanup timer and worker delays for deterministic widget tests.
#[cfg(feature = "test-utils")]
pub fn set_orphan_cleanup_delays_for_test(start_ms: u64, followup_ms: u64, worker_ms: u64) {
    ORPHAN_CLEANUP_START_DELAY_MS.store(start_ms, Ordering::Release);
    ORPHAN_CLEANUP_FOLLOWUP_DELAY_MS.store(followup_ms, Ordering::Release);
    ORPHAN_CLEANUP_WORKER_DELAY_MS.store(worker_ms, Ordering::Release);
}

/// Scalar window-owned orphan-cleanup scheduling evidence.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrphanCleanupRuntimeSnapshot {
    /// Whether a cleanup timer is armed.
    pub timer_pending: bool,
    /// Whether a cleanup worker is active.
    pub worker_active: bool,
    /// Workers started during this window lifetime.
    pub workers_started: usize,
    /// Peak simultaneous workers observed during this window lifetime.
    pub workers_high_water: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrphanCleanupFollowUp {
    Stop,
    Schedule {
        manifest_offset: usize,
        delay: Duration,
        next_failure_streak: u32,
    },
}

fn orphan_cleanup_follow_up(
    has_more_work: bool,
    next_manifest_offset: Option<usize>,
    retryable_failure: bool,
    failure_streak: u32,
) -> OrphanCleanupFollowUp {
    if !has_more_work {
        return OrphanCleanupFollowUp::Stop;
    }

    let next_failure_streak = if retryable_failure {
        failure_streak.saturating_add(1)
    } else {
        0
    };
    let delay = if retryable_failure {
        let exponent = next_failure_streak.saturating_sub(1).min(31);
        ORPHAN_CLEANUP_FOLLOWUP_DELAY
            .saturating_mul(1u32 << exponent)
            .min(ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF)
    } else {
        ORPHAN_CLEANUP_FOLLOWUP_DELAY
    };
    OrphanCleanupFollowUp::Schedule {
        manifest_offset: next_manifest_offset.unwrap_or(0),
        delay,
        next_failure_streak,
    }
}

#[cfg(feature = "test-utils")]
fn orphan_cleanup_start_delay() -> Duration {
    Duration::from_millis(ORPHAN_CLEANUP_START_DELAY_MS.load(Ordering::Acquire))
}

#[cfg(not(feature = "test-utils"))]
fn orphan_cleanup_start_delay() -> Duration {
    ORPHAN_CLEANUP_START_DELAY
}

fn orphan_cleanup_followup_delay(delay: Duration) -> Duration {
    #[cfg(feature = "test-utils")]
    {
        if delay == ORPHAN_CLEANUP_FOLLOWUP_DELAY {
            return Duration::from_millis(ORPHAN_CLEANUP_FOLLOWUP_DELAY_MS.load(Ordering::Acquire));
        }
    }
    delay
}

fn delay_orphan_cleanup_worker_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = ORPHAN_CLEANUP_WORKER_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
    }
}

/// Main-thread editor token paired with one accepted autosave snapshot.
struct DirtyDraftCompletion {
    /// Stable identity accepted by the body writer.
    draft_id: String,
    /// Dirty generation that may be cleared after manifest acceptance.
    dirty_generation: u64,
    /// Weak target so pending work never retains a closed tab.
    editor: glib::WeakRef<LushtextEditorPage>,
    /// Main-thread intent assigned before snapshot admission.
    intent: DraftMutationIntent,
}

/// Dirty editor state captured before autosave starts copying buffer text.
struct DirtyDraftCandidate {
    /// Stable identity captured before snapshotting begins.
    draft_id: String,
    /// Live path used to record file freshness metadata.
    original_path: Option<PathBuf>,
    /// Generation that must still match before publishing the snapshot.
    dirty_generation: u64,
    /// Weak editor used for freshness checks without extending tab lifetime.
    editor: glib::WeakRef<LushtextEditorPage>,
    /// GTK-owned buffer read only by the main-loop snapshot stage.
    buffer: sourceview5::Buffer,
    /// Main-thread intent assigned before snapshot admission.
    intent: DraftMutationIntent,
}

/// Compact metadata retained after one draft body has been durably written.
struct AcceptedDraft {
    entry: DraftEntry,
    completion: DirtyDraftCompletion,
}

/// Failures accumulated without retaining any completed draft bodies.
#[derive(Default)]
struct DraftPipelineFailures {
    /// Candidates cancelled or invalidated before acceptance.
    snapshot_cancelled: usize,
    /// Candidates rejected by the automatic-recovery byte policy.
    over_limit: usize,
    /// Body-write details retained without retaining body text.
    body_write: Vec<String>,
}

/// Compact manifest failure returned from a worker with its proven authority.
struct DraftManifestFailure {
    authority: DraftManifestAuthority,
    detail: String,
}

impl DraftManifestFailure {
    fn injected(error: &anyhow::Error) -> Self {
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
pub(super) struct DraftRestoreTicket {
    /// Exact manifest generation resolved by the background reader.
    entry: DraftEntry,
    /// Weak target so queued recovery cannot retain a closed tab.
    editor: glib::WeakRef<LushtextEditorPage>,
    /// File identity captured before dispatch for stale rejection.
    expected_path: Option<PathBuf>,
    /// Buffer generation that must match before applying recovered text.
    dirty_generation: u64,
    /// File-load generation that prevents restore crossing a reopen.
    load_generation: u64,
}

#[derive(Clone, Copy)]
enum DraftRestoreTracking {
    Ordinary,
    Lazy,
}

enum GuardedDraftRestoreResolution {
    Restore(crate::ui::plain_disposal::DisposalOwned<String>),
    Compact(FileDraftRestoreResolution),
}

enum GuardedPreloadedDraftRestore {
    Content(crate::ui::plain_disposal::DisposalOwned<String>),
    Compact(PreloadedDraftRestore),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DraftRestoreFacts {
    draft_id: Option<String>,
    path: Option<PathBuf>,
    dirty_generation: u64,
    load_generation: u64,
    manifest_entry: Option<DraftEntry>,
}

fn draft_restore_is_current(ticket: &DraftRestoreTicket, facts: &DraftRestoreFacts) -> bool {
    facts.manifest_entry.as_ref() == Some(&ticket.entry)
        && facts.draft_id.as_deref() == Some(ticket.entry.draft_id.as_str())
        && facts.path == ticket.expected_path
        && facts.dirty_generation == ticket.dirty_generation
        && facts.load_generation == ticket.load_generation
}

impl DraftRestoreTicket {
    fn capture(editor: &LushtextEditorPage, entry: DraftEntry) -> Self {
        Self {
            entry,
            editor: editor.downgrade(),
            expected_path: editor.file_path(),
            dirty_generation: editor.draft_dirty_generation(),
            load_generation: editor.load_generation(),
        }
    }

    fn current_editor(&self, window: &super::LushtextWindow) -> Option<LushtextEditorPage> {
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
struct OrphanCleanupUiResult {
    outcome: draft_service::DraftOrphanCleanupOutcome,
    committed_by_id: HashMap<String, draft_service::DraftEntryFingerprint>,
}

impl super::LushtextWindow {
    /// Accept one trusted manifest commit and reapply compact pending tombstones.
    fn accept_draft_manifest_commit(&self, mut commit: draft_service::DraftManifestCommit) {
        let drafts = &self.imp().drafts;
        let order = drafts.mutation_order.borrow();
        let mut tombstones = drafts.delete_tombstones.borrow_mut();
        tombstones.retain(|_, intent| order.is_current(intent));
        commit
            .manifest
            .drafts
            .retain(|entry| !tombstones.contains_key(entry.draft_id.as_str()));
        drop(tombstones);
        drop(order);
        let became_trusted = !self.imp().drafts.manifest_authority.get().is_trusted()
            && commit.authority.is_trusted();
        self.imp().drafts.manifest_authority.set(commit.authority);
        *self.imp().drafts.manifest.borrow_mut() = commit.manifest;
        if became_trusted {
            self.schedule_orphan_cleanup(true);
        }
    }

    /// Revoke destructive cleanup immediately after a manifest command loses
    /// completeness or durable replacement eligibility.
    fn reject_draft_manifest_authority(&self, authority: DraftManifestAuthority) {
        self.imp().drafts.manifest_authority.set(authority);
        self.imp().drafts.orphan_cleanup_pending_offset.set(None);
        self.imp().drafts.orphan_cleanup_timer_pending.set(false);
        let _ = self.imp().drafts.orphan_cleanup_timer.invalidate();
    }

    /// Write all dirty drafts synchronously during window close.
    ///
    /// Regular autosave uses chunked snapshots plus background writes. Close
    /// handling is the intentional exception: the process is about to exit, so
    /// this blocks briefly to preserve the last recoverable buffer state before
    /// GTK tears down the window.
    ///
    /// # Errors
    ///
    /// Returns an error when any dirty draft file cannot be written or when
    /// the draft manifest cannot be updated after successful draft writes.
    pub fn flush_dirty_drafts(&self) -> Result<()> {
        if self.imp().drafts.mutation_inflight.get()
            || self.imp().drafts.orphan_cleanup_inflight.get()
        {
            anyhow::bail!("draft persistence is already in progress");
        }
        let tab_view = &self.imp().tab_view;
        let data_dir = json_store::data_dir();
        let now = editor_io::now_epoch_secs();
        let mut manifest_updates = Vec::new();
        let mut write_errors = Vec::new();
        let discarded_draft_ids = self.imp().drafts.close_discard_ids.borrow().clone();

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            // AdwTabPage exposes a generic GtkWidget. GObject's runtime downcast
            // checks for EditorPage before exposing editor-specific APIs.
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if !editor.is_modified() || editor.is_evicted() {
                continue;
            }
            let Some(draft_id) = editor.draft_id() else {
                continue;
            };
            if discarded_draft_ids.contains(&draft_id) {
                continue;
            }
            let buffer = editor.buffer();
            let text = match buffer_snapshot::snapshot_buffer_text_direct_budgeted(
                &buffer,
                automatic_draft_limit(),
            ) {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => text.into_direct_string(),
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                    Self::show_automatic_recovery_limit(editor);
                    write_errors.push(format!(
                        "{draft_id}: document exceeds the automatic recovery limit"
                    ));
                    continue;
                }
                buffer_snapshot::BufferSnapshotOutcome::Cancelled(_) => {
                    write_errors.push(format!("{draft_id}: snapshot was cancelled"));
                    continue;
                }
            };
            if let Err(e) = draft_service::write_draft(&data_dir, &draft_id, &text) {
                tracing::error!("Failed to write draft on close: {e}");
                write_errors.push(format!("{draft_id}: {e}"));
                continue;
            }
            let original_path = editor.file_path();
            let mtime = original_path
                .as_ref()
                .and_then(|path| editor_io::mtime_secs(path));
            manifest_updates.push(DraftEntry {
                draft_id,
                original_path,
                original_mtime_secs: mtime,
                saved_at_secs: now,
            });
        }
        let had_manifest_updates = !manifest_updates.is_empty();
        if had_manifest_updates {
            let session = self.collect_session();
            let authority = self.imp().drafts.manifest_authority.get();
            let commit =
                match draft_service::update_manifest(&data_dir, &session, authority, |manifest| {
                    for entry in manifest_updates {
                        manifest.upsert(entry);
                    }
                }) {
                    Ok(commit) => commit,
                    Err(error) => {
                        self.reject_draft_manifest_authority(error.authority());
                        return Err(anyhow::anyhow!(
                            "failed to save draft manifest on close: {error}"
                        ));
                    }
                };
            self.accept_draft_manifest_commit(commit);
        }
        if !write_errors.is_empty() {
            return Err(anyhow::anyhow!(
                "failed to write {} drafts on close: {}",
                write_errors.len(),
                write_errors.join("; ")
            ));
        }
        if !had_manifest_updates {
            self.clear_close_discard_drafts();
            return Ok(());
        }
        self.clear_close_discard_drafts();
        Ok(())
    }

    /// Move one eager body together with a replacement disposal reservation.
    ///
    /// The aggregate startup permit continues to own all other bodies until
    /// they are detached for worker retirement. If replacement headroom is
    /// unavailable, every eager body becomes a compact lazy marker before this
    /// method returns, so GTK never owns an unguarded recovery body.
    fn take_preloaded_draft(&self, draft_id: &str) -> Option<GuardedPreloadedDraftRestore> {
        let mut preloaded = self.imp().drafts.preloaded.borrow_mut();
        let restore = preloaded.get(draft_id)?;
        let PreloadedDraftRestore::Content(content) = restore else {
            return preloaded
                .remove(draft_id)
                .map(GuardedPreloadedDraftRestore::Compact);
        };
        let body_weight = u64::try_from(content.capacity()).unwrap_or(u64::MAX);
        let reservation = preloaded.reservation_weight().map_or_else(
            || crate::ui::plain_disposal::try_reserve_progress_for_gtk(body_weight),
            |aggregate_weight| {
                crate::ui::plain_disposal::try_reserve_progress_replacement_for_gtk(
                    body_weight,
                    aggregate_weight,
                )
            },
        );
        let Some(reservation) = reservation else {
            release_eager_preloads(&mut preloaded);
            return preloaded
                .remove(draft_id)
                .map(GuardedPreloadedDraftRestore::Compact);
        };

        let PreloadedDraftRestore::Content(content) = preloaded.remove(draft_id)? else {
            unreachable!("preloaded body kind was checked before transfer")
        };
        if let Some(aggregate_weight) = preloaded.reservation_weight() {
            preloaded.shrink_reservation_to(aggregate_weight.saturating_sub(body_weight));
        }
        Some(GuardedPreloadedDraftRestore::Content(
            attach_draft_body_disposal_probe(reservation.own(content)),
        ))
    }

    /// Flush dirty drafts for close without monopolizing a GTK main-loop turn.
    ///
    /// Copies are serialized on GTK, writes run on workers, and `on_done` runs
    /// back on GTK after every candidate is accepted or classified.
    pub fn flush_dirty_drafts_async<F: FnOnce(Result<()>) + 'static>(&self, on_done: F) {
        if self.imp().drafts.mutation_inflight.get()
            || self.imp().drafts.orphan_cleanup_inflight.get()
            || !self.imp().drafts.pending_deletes.borrow().is_empty()
            || self.imp().drafts.restore_inflight_count.get() > 0
        {
            let window_weak = self.downgrade();
            glib::timeout_add_local_once(DRAFT_MUTATION_WAIT_POLL_INTERVAL, move || {
                if let Some(window) = window_weak.upgrade() {
                    window.flush_dirty_drafts_async(on_done);
                }
            });
            return;
        }
        let candidates = self.collect_close_draft_candidates();
        if candidates.is_empty() {
            self.clear_close_discard_drafts();
            on_done(Ok(()));
            return;
        }
        self.imp().drafts.mutation_inflight.set(true);
        self.drive_close_draft_pipeline(
            candidates,
            Vec::new(),
            DraftPipelineFailures::default(),
            on_done,
        );
    }

    /// Load draft content for an untitled tab by draft ID.
    pub fn check_draft_by_id(&self, editor: &LushtextEditorPage, draft_id: &str) {
        let entry = self
            .imp()
            .drafts
            .manifest
            .borrow()
            .find_by_id(draft_id)
            .cloned();

        let Some(entry) = entry else {
            return;
        };

        if let Some(preloaded) = self.take_preloaded_draft(draft_id) {
            match preloaded {
                GuardedPreloadedDraftRestore::Content(draft_content) => {
                    self.note_draft_restore_started();
                    self.apply_draft(
                        &DraftRestoreTicket::capture(editor, entry),
                        draft_content,
                        DraftRestoreTracking::Ordinary,
                    );
                }
                GuardedPreloadedDraftRestore::Compact(PreloadedDraftRestore::SkipStaleFile) => {
                    tracing::warn!(
                        "Untitled draft {draft_id} unexpectedly carried a stale file warning"
                    );
                }
                GuardedPreloadedDraftRestore::Compact(PreloadedDraftRestore::SkipOversized) => {
                    Self::show_oversized_draft_skipped(editor);
                }
                GuardedPreloadedDraftRestore::Compact(
                    PreloadedDraftRestore::LazyAggregateBudget,
                ) => {
                    self.queue_lazy_draft_restore(editor, entry);
                }
                GuardedPreloadedDraftRestore::Compact(PreloadedDraftRestore::Content(_)) => {
                    unreachable!("eager bodies cross GTK only with transferable disposal ownership")
                }
            }
            return;
        }

        self.queue_lazy_draft_restore(editor, entry);
    }

    /// Enqueue one non-preloaded body and start the serialized reader.
    ///
    /// Startup aggregate-budget skips and later on-demand fallbacks share this
    /// gate so completed 64 MiB reads cannot accumulate behind GTK installers.
    fn queue_lazy_draft_restore(&self, editor: &LushtextEditorPage, entry: DraftEntry) {
        self.imp()
            .drafts
            .lazy_restore_queue
            .borrow_mut()
            .push_back(DraftRestoreTicket::capture(editor, entry));
        self.drive_lazy_draft_restore_queue();
    }

    /// Admit at most one lazy draft body to GTK and reject stale completions.
    fn drive_lazy_draft_restore_queue(&self) {
        if self.imp().drafts.lazy_restore_inflight.get() {
            return;
        }
        if self.imp().drafts.lazy_restore_queue.borrow().is_empty() {
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::progress_disposal_capacity_epoch();
        let Some(reservation) = crate::ui::plain_disposal::try_reserve_progress_for_gtk(
            DRAFT_RESTORE_DISPOSAL_RESERVATION_BYTES,
        ) else {
            let window_weak = self.downgrade();
            self.imp()
                .drafts
                .lazy_restore_capacity_wakeup
                .arm(observed_epoch, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.drive_lazy_draft_restore_queue();
                    }
                });
            return;
        };
        let Some(candidate) = self
            .imp()
            .drafts
            .lazy_restore_queue
            .borrow_mut()
            .pop_front()
        else {
            return;
        };
        self.imp().drafts.lazy_restore_inflight.set(true);
        self.note_draft_restore_started();
        let data_dir = json_store::data_dir();
        let entry = candidate.entry.clone();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                delay_draft_restore_for_test();
                let mut reservation = reservation;
                draft_service::resolve_draft_restore(&data_dir, &entry).map(|resolution| {
                    match resolution {
                        FileDraftRestoreResolution::Restore { content } => {
                            reservation
                                .shrink_to(u64::try_from(content.capacity()).unwrap_or(u64::MAX));
                            GuardedDraftRestoreResolution::Restore(
                                attach_draft_body_disposal_probe(reservation.own(content)),
                            )
                        }
                        compact => GuardedDraftRestoreResolution::Compact(compact),
                    }
                })
            },
            move |(), result| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                window.finish_draft_restore(&candidate, result, DraftRestoreTracking::Lazy);
            },
        );
    }

    /// Apply one worker result only while its complete editor and manifest ticket is current.
    fn finish_draft_restore(
        &self,
        ticket: &DraftRestoreTicket,
        result: Result<GuardedDraftRestoreResolution>,
        tracking: DraftRestoreTracking,
    ) {
        let Some(editor) = ticket.current_editor(self) else {
            self.finish_draft_restore_tracking(tracking);
            return;
        };
        let draft_id = ticket.entry.draft_id.clone();
        match result {
            Ok(GuardedDraftRestoreResolution::Restore(content)) => {
                self.apply_draft(ticket, content, tracking);
                return;
            }
            Ok(GuardedDraftRestoreResolution::Compact(FileDraftRestoreResolution::SkipStale)) => {
                Self::show_stale_draft_skipped(&editor);
                self.delete_draft_by_id(&draft_id);
            }
            Ok(GuardedDraftRestoreResolution::Compact(
                FileDraftRestoreResolution::SkipOversized,
            )) => {
                Self::show_oversized_draft_skipped(&editor);
            }
            Ok(GuardedDraftRestoreResolution::Compact(
                FileDraftRestoreResolution::SkipUnavailable
                | FileDraftRestoreResolution::MissingDraft,
            )) => {}
            Ok(GuardedDraftRestoreResolution::Compact(FileDraftRestoreResolution::Restore {
                ..
            })) => unreachable!("restored bodies are guarded on the worker"),
            Err(error) => {
                tracing::warn!("Failed to restore draft {draft_id}: {error}");
                editor.emit_inline_notification(InlineActionNotification {
                    style: InlineNotificationStyle::Warning,
                    title: "Draft Restore Failed".to_string(),
                    body: "The preserved recovery draft could not be read. The tab remains usable and the recovery files were kept.".to_string(),
                    primary_button: None,
                    secondary_button: None,
                });
            }
        }
        self.finish_draft_restore_tracking(tracking);
    }

    /// Install restored draft content without publishing partial recovery state.
    fn apply_draft(
        &self,
        ticket: &DraftRestoreTicket,
        content: crate::ui::plain_disposal::DisposalOwned<String>,
        tracking: DraftRestoreTracking,
    ) {
        let Some(editor) = ticket.current_editor(self) else {
            self.finish_draft_restore_tracking(tracking);
            return;
        };
        let freshness_window = self.downgrade();
        let terminal_window = self.downgrade();
        let freshness_ticket = ticket.clone();
        let terminal_ticket = ticket.clone();
        let accepted_body = Rc::new(RefCell::new(None));
        let accepted_body_for_terminal = Rc::clone(&accepted_body);
        let request = BufferReplacementRequest::new_guarded(
            BufferReplacementTicket {
                workflow: BufferReplacementWorkflow::DraftRecovery,
                generation: ticket.dirty_generation,
            },
            content,
            move |_| {
                freshness_window
                    .upgrade()
                    .and_then(|window| freshness_ticket.current_editor(&window))
                    .is_some()
            },
            move |outcome| {
                let Some(window) = terminal_window.upgrade() else {
                    return;
                };
                if let BufferReplacementOutcome::Complete {
                    ticket:
                        BufferReplacementTicket {
                            workflow: BufferReplacementWorkflow::DraftRecovery,
                            generation,
                        },
                    ..
                } = outcome
                    && generation == terminal_ticket.dirty_generation
                    && let Some(editor) = terminal_ticket.current_editor(&window)
                    && let Some(body) = accepted_body_for_terminal.borrow_mut().take()
                {
                    Self::finish_applied_draft(&editor, body);
                }
                window.finish_draft_restore_tracking(tracking);
            },
        )
        .return_guarded_body_on_complete(move |body| {
            accepted_body.borrow_mut().replace(body);
        });
        editor.replace_buffer_bounded(request);
    }

    fn finish_applied_draft(
        editor: &LushtextEditorPage,
        content: crate::ui::plain_disposal::DisposalOwned<String>,
    ) {
        let buffer = editor.buffer();
        editor.seed_local_history_from_guarded_restored_draft(content);
        buffer.set_modified(true);
        editor.capture_restored_draft_baseline();
        if editor.file_path().is_some() {
            editor.mark_entire_buffer_modified();
        } else {
            editor.schedule_minimap_refresh();
        }
        let has_backing_file = editor.file_path().is_some();
        editor.set_draft_restored(true);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: if has_backing_file {
                "Draft Changes Restored".to_string()
            } else {
                "Document Restored".to_string()
            },
            body: if has_backing_file {
                "Unsaved changes from a previous session have been restored.".to_string()
            } else {
                "Unsaved document has been restored.".to_string()
            },
            primary_button: Some("_Discard…".to_string()),
            secondary_button: Some(if has_backing_file {
                "_Save…".to_string()
            } else {
                "Save _As…".to_string()
            }),
        });
    }

    fn finish_draft_restore_tracking(&self, tracking: DraftRestoreTracking) {
        if matches!(tracking, DraftRestoreTracking::Lazy) {
            self.imp().drafts.lazy_restore_inflight.set(false);
        }
        self.note_draft_restore_finished();
        if matches!(tracking, DraftRestoreTracking::Lazy) {
            self.drive_lazy_draft_restore_queue();
        }
    }

    /// Warn that a file-backed draft was skipped because the file changed on disk.
    fn show_stale_draft_skipped(editor: &LushtextEditorPage) {
        editor.set_draft_restored(false);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Draft Not Restored".to_string(),
            body: "Unsaved changes from a previous session were not restored because the file changed on disk.".to_string(),
            primary_button: None,
            secondary_button: None,
        });
    }

    /// Warn that a draft was preserved on disk but skipped because it is too large.
    fn show_oversized_draft_skipped(editor: &LushtextEditorPage) {
        editor.set_draft_restored(false);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Draft Not Restored".to_string(),
            body: "Unsaved changes from a previous session were not restored automatically because the draft is very large.".to_string(),
            primary_button: None,
            secondary_button: None,
        });
    }

    /// Warn that the current buffer is too large for automatic crash recovery.
    fn show_automatic_recovery_limit(editor: &LushtextEditorPage) {
        editor.set_automatic_recovery_limited(true);
        editor.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Automatic Recovery Paused".to_string(),
            body: "This document is over the 64 MiB automatic recovery limit. Keep editing, or use Save / Save As to protect the latest changes.".to_string(),
            primary_button: None,
            secondary_button: None,
        });
    }

    /// Clear the limit warning only after a matching generation is accepted.
    fn clear_automatic_recovery_limit(&self, editor: &LushtextEditorPage) {
        if editor.automatic_recovery_limited() {
            editor.set_automatic_recovery_limited(false);
            let warning_is_visible = self
                .imp()
                .notification_bus
                .editor_info_bar_view(editor.notification_owner_id())
                .is_some_and(|notification| notification.title == "Automatic Recovery Paused");
            if warning_is_visible {
                self.resolve_editor_inline_notification(editor);
            }
        }
    }

    /// Deferred orphan cleanup — runs after restore so startup stays responsive.
    ///
    /// Cleanup is skipped when startup recovery did not trust the manifest,
    /// preventing deletion based on unsafe metadata.
    pub(super) fn schedule_orphan_cleanup(&self, cleanup_allowed: bool) {
        let drafts = &self.imp().drafts;
        drafts.orphan_cleanup_failure_streak.set(0);
        drafts.orphan_cleanup_pending_offset.set(None);
        drafts.orphan_cleanup_timer_pending.set(true);
        drafts.orphan_cleanup_timer.arm(
            self,
            orphan_cleanup_start_delay(),
            move |window, _| {
                window
                    .imp()
                    .drafts
                    .orphan_cleanup_timer_pending
                    .set(false);
                // Eager strings can be released after the ordinary restore window,
                // but compact lazy markers must survive slow file loads so they
                // cannot bypass the serialized admission queue.
                release_eager_preloads(&mut window.imp().drafts.preloaded.borrow_mut());
                if !cleanup_allowed {
                    tracing::warn!(
                        "Skipped draft orphan cleanup because startup recovery did not trust the draft manifest"
                    );
                    return;
                }
                window.run_orphan_cleanup_pass(0);
            },
        );
    }

    /// Run one inspect/execute pass off the GTK thread and merge exact commits.
    fn run_orphan_cleanup_pass(&self, manifest_offset: usize) {
        let drafts = &self.imp().drafts;
        if !drafts.manifest_authority.get().is_trusted() {
            drafts.orphan_cleanup_pending_offset.set(None);
            drafts.orphan_cleanup_timer_pending.set(false);
            let _ = drafts.orphan_cleanup_timer.invalidate();
            return;
        }
        if drafts.mutation_inflight.get() {
            self.arm_orphan_cleanup_follow_up(manifest_offset, DRAFT_MUTATION_WAIT_POLL_INTERVAL);
            return;
        }
        if drafts.orphan_cleanup_inflight.replace(true) {
            drafts
                .orphan_cleanup_pending_offset
                .set(Some(manifest_offset));
            return;
        }
        #[cfg(feature = "test-utils")]
        {
            drafts.orphan_cleanup_workers_started.set(
                drafts
                    .orphan_cleanup_workers_started
                    .get()
                    .saturating_add(1),
            );
            drafts
                .orphan_cleanup_workers_high_water
                .set(drafts.orphan_cleanup_workers_high_water.get().max(1));
        }
        let data_dir = json_store::data_dir();
        // Clone GTK-owned state before dispatch so the worker receives plain
        // owned data and never borrows through the window's interior mutability.
        let manifest = self.imp().drafts.manifest.borrow().clone();
        spawn_blocking_then(
            self.clone(),
            move || {
                delay_orphan_cleanup_worker_for_test();
                draft_service::inspect_orphan_cleanup_from(&data_dir, &manifest, manifest_offset)
                    .map(|plan| {
                        let mut outcome = draft_service::execute_orphan_cleanup(&data_dir, plan);
                        // Drop the full manifest before crossing back to GTK; the
                        // callback needs only fingerprints, failures, and continuation.
                        outcome.latest_persisted_manifest.take();
                        let committed_by_id = outcome
                            .committed_manifest_removals
                            .iter()
                            .map(|fingerprint| (fingerprint.draft_id.clone(), fingerprint.clone()))
                            .collect();
                        OrphanCleanupUiResult {
                            outcome,
                            committed_by_id,
                        }
                    })
            },
            move |window, result| {
                window.imp().drafts.orphan_cleanup_inflight.set(false);
                let follow_up = match result {
                    Ok(result) => {
                        let OrphanCleanupUiResult {
                            outcome,
                            committed_by_id,
                        } = result;
                        // Merge exact generations instead of replacing live state;
                        // autosaves accepted while the worker ran must survive.
                        draft_service::merge_committed_orphan_removals(
                            &mut window.imp().drafts.manifest.borrow_mut(),
                            &committed_by_id,
                        );
                        if !outcome.failures.is_empty() {
                            let message = orphan_cleanup_failure_message(&outcome.failures);
                            tracing::warn!("{message}");
                            window.publish_status_message(&message, NotificationSeverity::Warning);
                        }
                        orphan_cleanup_follow_up(
                            outcome.has_more_work,
                            outcome.next_manifest_offset,
                            !outcome.failures.is_empty(),
                            window.imp().drafts.orphan_cleanup_failure_streak.get(),
                        )
                    }
                    Err(error) => {
                        let message = format!("Draft recovery cleanup scan failed: {error}");
                        tracing::warn!("{message}");
                        window.publish_status_message(&message, NotificationSeverity::Warning);
                        orphan_cleanup_follow_up(
                            true,
                            None,
                            true,
                            window.imp().drafts.orphan_cleanup_failure_streak.get(),
                        )
                    }
                };
                window.finish_orphan_cleanup_pass(follow_up);
                window.drive_pending_draft_mutations();
            },
        );
    }

    fn finish_orphan_cleanup_pass(&self, follow_up: OrphanCleanupFollowUp) {
        if let Some(manifest_offset) = self.imp().drafts.orphan_cleanup_pending_offset.take() {
            self.imp().drafts.orphan_cleanup_failure_streak.set(0);
            self.arm_orphan_cleanup_follow_up(
                manifest_offset,
                orphan_cleanup_followup_delay(ORPHAN_CLEANUP_FOLLOWUP_DELAY),
            );
            return;
        }

        match follow_up {
            OrphanCleanupFollowUp::Stop => {
                self.imp().drafts.orphan_cleanup_failure_streak.set(0);
                self.imp().drafts.orphan_cleanup_timer_pending.set(false);
                let _ = self.imp().drafts.orphan_cleanup_timer.invalidate();
            }
            OrphanCleanupFollowUp::Schedule {
                manifest_offset,
                delay,
                next_failure_streak,
            } => {
                self.imp()
                    .drafts
                    .orphan_cleanup_failure_streak
                    .set(next_failure_streak);
                self.arm_orphan_cleanup_follow_up(
                    manifest_offset,
                    orphan_cleanup_followup_delay(delay),
                );
            }
        }
    }

    fn arm_orphan_cleanup_follow_up(&self, manifest_offset: usize, delay: Duration) {
        self.imp().drafts.orphan_cleanup_timer_pending.set(true);
        self.imp()
            .drafts
            .orphan_cleanup_timer
            .arm(self, delay, move |window, _| {
                window.imp().drafts.orphan_cleanup_timer_pending.set(false);
                window.run_orphan_cleanup_pass(manifest_offset);
            });
    }

    /// Start the global 5-second autosave timer.
    pub fn start_autosave_timer(&self) {
        let window_weak = self.downgrade();
        let source_id = glib::timeout_add_local(Duration::from_secs(5), move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            window.autosave_tick();
            glib::ControlFlow::Continue
        });
        *self.imp().drafts.autosave_source_id.borrow_mut() = Some(source_id);
    }

    /// Single autosave tick: collect dirty tabs and write drafts.
    fn autosave_tick(&self) {
        self.cancel_first_dirty_draft_autosave();
        if self.imp().drafts.autosave_inflight.get()
            || self.imp().drafts.mutation_inflight.get()
            || self.imp().drafts.orphan_cleanup_inflight.get()
        {
            self.imp().drafts.autosave_pending.set(true);
            return;
        }

        let dirty_tabs = self.collect_dirty_draft_candidates();
        if dirty_tabs.is_empty() {
            return;
        }

        self.imp().drafts.autosave_inflight.set(true);
        self.imp().drafts.mutation_inflight.set(true);
        self.drive_dirty_draft_pipeline(dirty_tabs, Vec::new(), DraftPipelineFailures::default());
    }

    /// Drive one autosave pass without waiting for the production timer.
    #[cfg(feature = "test-utils")]
    pub fn autosave_tick_for_test(&self) {
        self.autosave_tick();
    }

    /// Whether a draft autosave batch is currently snapshotting or writing.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn draft_autosave_inflight_for_test(&self) -> bool {
        self.imp().drafts.autosave_inflight.get()
    }

    /// Peak number of complete bodies retained by the autosave pipeline.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn draft_pipeline_max_retained_bodies_for_test(&self) -> usize {
        self.imp().drafts.max_retained_complete_bodies.get()
    }

    /// Schedule startup orphan cleanup through the production timer owner.
    #[cfg(feature = "test-utils")]
    pub fn schedule_orphan_cleanup_for_test(&self, cleanup_allowed: bool) {
        self.schedule_orphan_cleanup(cleanup_allowed);
    }

    /// Return scalar orphan-cleanup timer and worker ownership evidence.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn orphan_cleanup_runtime_snapshot_for_test(&self) -> OrphanCleanupRuntimeSnapshot {
        OrphanCleanupRuntimeSnapshot {
            timer_pending: self.imp().drafts.orphan_cleanup_timer_pending.get(),
            worker_active: self.imp().drafts.orphan_cleanup_inflight.get(),
            workers_started: self.imp().drafts.orphan_cleanup_workers_started.get(),
            workers_high_water: self.imp().drafts.orphan_cleanup_workers_high_water.get(),
        }
    }

    /// Exercise the same orphan-cleanup cancellation used by window disposal.
    #[cfg(feature = "test-utils")]
    pub fn dispose_orphan_cleanup_for_test(&self) {
        self.imp().drafts.dispose_orphan_cleanup();
    }

    /// Whether one aggregate-budget draft read is currently active.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn lazy_draft_restore_inflight_for_test(&self) -> bool {
        self.imp().drafts.lazy_restore_inflight.get()
    }

    /// Whether any ordinary or aggregate-budget restore resolution is pending.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn draft_restore_inflight_for_test(&self) -> bool {
        self.imp().drafts.restore_inflight_count.get() > 0
    }

    /// Cancel the active autosave snapshot to exercise retry semantics.
    #[cfg(feature = "test-utils")]
    pub fn cancel_draft_snapshot_for_test(&self) {
        if let Some(cancellation) = self.imp().drafts.autosave_snapshot.borrow().as_ref() {
            cancellation.cancel();
        }
    }

    /// Schedule a short autosave after the first dirty edit in a clean cycle.
    pub(crate) fn schedule_first_dirty_draft_autosave(&self) {
        if self.imp().drafts.autosave_inflight.get() || self.imp().drafts.mutation_inflight.get() {
            self.imp().drafts.autosave_pending.set(true);
            return;
        }
        self.imp().drafts.first_dirty_autosave_timer.arm(
            self,
            first_dirty_autosave_debounce(),
            move |window, _| {
                window.autosave_tick();
            },
        );
    }

    fn cancel_first_dirty_draft_autosave(&self) {
        let _ = self.imp().drafts.first_dirty_autosave_timer.invalidate();
    }

    /// Capture retry-eligible autosave candidates, excluding clean generations.
    fn collect_dirty_draft_candidates(&self) -> Vec<DirtyDraftCandidate> {
        let tab_view = &self.imp().tab_view;
        let mut dirty_tabs = Vec::new();
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if !editor.is_modified() || !editor.draft_dirty() || editor.is_evicted() {
                continue;
            }
            let Some(draft_id) = editor.draft_id() else {
                continue;
            };
            let intent = self
                .imp()
                .drafts
                .mutation_order
                .borrow_mut()
                .advance(&draft_id);
            dirty_tabs.push(DirtyDraftCandidate {
                draft_id,
                original_path: editor.file_path(),
                dirty_generation: editor.draft_dirty_generation(),
                editor: editor.downgrade(),
                buffer: editor.buffer(),
                intent,
            });
        }
        dirty_tabs
    }

    /// Capture every modified close candidate except explicit discards.
    fn collect_close_draft_candidates(&self) -> Vec<DirtyDraftCandidate> {
        let tab_view = &self.imp().tab_view;
        let discarded_draft_ids = self.imp().drafts.close_discard_ids.borrow().clone();
        let mut dirty_tabs = Vec::new();
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if !editor.is_modified() || editor.is_evicted() {
                continue;
            }
            let Some(draft_id) = editor.draft_id() else {
                continue;
            };
            if discarded_draft_ids.contains(&draft_id) {
                continue;
            }
            let intent = self
                .imp()
                .drafts
                .mutation_order
                .borrow_mut()
                .advance(&draft_id);
            dirty_tabs.push(DirtyDraftCandidate {
                draft_id,
                original_path: editor.file_path(),
                dirty_generation: editor.draft_dirty_generation(),
                editor: editor.downgrade(),
                buffer: editor.buffer(),
                intent,
            });
        }
        dirty_tabs
    }

    /// Snapshot and write one close candidate before admitting the next body.
    fn drive_close_draft_pipeline<F: FnOnce(Result<()>) + 'static>(
        &self,
        mut candidates: Vec<DirtyDraftCandidate>,
        accepted: Vec<AcceptedDraft>,
        failures: DraftPipelineFailures,
        on_done: F,
    ) {
        let Some(candidate) = candidates.pop() else {
            self.commit_close_draft_pipeline(accepted, failures, on_done);
            return;
        };

        let window_weak = self.downgrade();
        // Every terminal outcome clears this capture's token before the next
        // candidate starts, preventing stale disposal cancellation.
        let finish_snapshot = move |outcome: buffer_snapshot::BufferSnapshotOutcome| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            match outcome {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                    window.imp().drafts.close_snapshot.take();
                    let Some(editor) = candidate.editor.upgrade() else {
                        let mut failures = failures;
                        failures.snapshot_cancelled += 1;
                        window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                        return;
                    };
                    // Chunked capture yields to GTK. A mismatch is unconfirmed and
                    // must block close rather than publish stale text.
                    if editor.draft_id().as_deref() != Some(candidate.draft_id.as_str())
                        || editor.draft_dirty_generation() != candidate.dirty_generation
                        || !editor.is_modified()
                        || editor.is_evicted()
                    {
                        let mut failures = failures;
                        failures.snapshot_cancelled += 1;
                        window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                        return;
                    }
                    let data_dir = json_store::data_dir();
                    let draft_id = candidate.draft_id.clone();
                    let original_path = candidate.original_path;
                    let completion = DirtyDraftCompletion {
                        draft_id: candidate.draft_id,
                        dirty_generation: candidate.dirty_generation,
                        editor: candidate.editor,
                        intent: candidate.intent,
                    };
                    let window_weak = window.downgrade();
                    // Move the only complete body to the worker and admit the next
                    // candidate only after this durable write releases it.
                    spawn_blocking_then(
                        (),
                        move || {
                            let text = text.into_string_on_worker();
                            delay_draft_body_for_test();
                            fail_next_draft_body_for_test()?;
                            draft_service::write_draft(&data_dir, &draft_id, &text)?;
                            Ok::<_, anyhow::Error>(DraftEntry {
                                draft_id,
                                original_mtime_secs: original_path
                                    .as_deref()
                                    .and_then(editor_io::mtime_secs),
                                original_path,
                                saved_at_secs: editor_io::now_epoch_secs(),
                            })
                        },
                        move |(), result| {
                            let Some(window) = window_weak.upgrade() else {
                                return;
                            };
                            let mut accepted = accepted;
                            let mut failures = failures;
                            match result {
                                Ok(entry) => accepted.push(AcceptedDraft { entry, completion }),
                                Err(error) => {
                                    tracing::error!("Failed to write draft on close: {error}");
                                    failures.body_write.push(error.to_string());
                                }
                            }
                            window.drive_close_draft_pipeline(
                                candidates, accepted, failures, on_done,
                            );
                        },
                    );
                }
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                    window.imp().drafts.close_snapshot.take();
                    let mut failures = failures;
                    failures.over_limit += 1;
                    if let Some(editor) = candidate.editor.upgrade() {
                        Self::show_automatic_recovery_limit(&editor);
                    }
                    window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                }
                buffer_snapshot::BufferSnapshotOutcome::Cancelled(_) => {
                    window.imp().drafts.close_snapshot.take();
                    let mut failures = failures;
                    failures.snapshot_cancelled += 1;
                    window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                }
            }
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&candidate.buffer) {
            let snapshot = buffer_snapshot::snapshot_buffer_text_async_budgeted(
                candidate.buffer,
                automatic_draft_limit(),
                finish_snapshot,
            );
            *self.imp().drafts.close_snapshot.borrow_mut() = Some(snapshot);
        } else {
            finish_snapshot(buffer_snapshot::snapshot_buffer_text_direct_budgeted(
                &candidate.buffer,
                automatic_draft_limit(),
            ));
        }
    }

    /// Commit successful close bodies once and report every unconfirmed draft.
    fn commit_close_draft_pipeline<F: FnOnce(Result<()>) + 'static>(
        &self,
        accepted: Vec<AcceptedDraft>,
        mut failures: DraftPipelineFailures,
        on_done: F,
    ) {
        let mut accepted_entries = Vec::new();
        for accepted in accepted {
            let completion = accepted.completion;
            let Some(editor) = completion.editor.upgrade() else {
                failures.snapshot_cancelled += 1;
                continue;
            };
            if editor.draft_id().as_deref() == Some(completion.draft_id.as_str())
                && editor.draft_dirty_generation() == completion.dirty_generation
                && editor.is_modified()
                && !editor.is_evicted()
                && self
                    .imp()
                    .drafts
                    .mutation_order
                    .borrow()
                    .is_current(&completion.intent)
            {
                accepted_entries.push(accepted.entry);
            } else {
                failures.snapshot_cancelled += 1;
            }
        }
        let data_dir = json_store::data_dir();
        let session = self.collect_session();
        let authority = self.imp().drafts.manifest_authority.get();
        let window_weak = self.downgrade();

        spawn_blocking_then(
            (),
            move || {
                delay_draft_manifest_for_test();
                if let Err(error) = fail_next_draft_manifest_for_test() {
                    return (
                        None,
                        Err(DraftFlushError::Manifest {
                            authority: DraftManifestAuthority::default(),
                            detail: error.to_string(),
                        }),
                    );
                }
                let commit = if accepted_entries.is_empty() {
                    None
                } else {
                    match draft_service::update_manifest(
                        &data_dir,
                        &session,
                        authority,
                        |manifest| {
                            for entry in accepted_entries {
                                manifest.upsert(entry);
                            }
                        },
                    ) {
                        Ok(commit) => Some(commit),
                        Err(error) => {
                            return (
                                None,
                                Err(DraftFlushError::Manifest {
                                    authority: error.authority(),
                                    detail: error.to_string(),
                                }),
                            );
                        }
                    }
                };
                let result = if failures.snapshot_cancelled == 0
                    && failures.over_limit == 0
                    && failures.body_write.is_empty()
                {
                    Ok(())
                } else {
                    Err(DraftFlushError::Unconfirmed {
                        total: failures.snapshot_cancelled
                            + failures.over_limit
                            + failures.body_write.len(),
                        cancelled: failures.snapshot_cancelled,
                        over_limit: failures.over_limit,
                        body_write: failures.body_write.len(),
                    })
                };
                (commit, result)
            },
            move |(), (commit, result)| {
                if let Some(window) = window_weak.upgrade() {
                    if let Some(commit) = commit {
                        window.accept_draft_manifest_commit(commit);
                    }
                    if let Err(DraftFlushError::Manifest { authority, .. }) = &result {
                        window.reject_draft_manifest_authority(*authority);
                    }
                    if result.is_ok() {
                        window.clear_close_discard_drafts();
                    }
                    // Close flush owns this transaction's acceptance result.
                    // Do not let an edit-coalesced regular tick clear retry state
                    // before the close caller observes success or failure.
                    window.imp().drafts.autosave_pending.set(false);
                    window.imp().drafts.mutation_inflight.set(false);
                    window.drive_pending_draft_mutations();
                    window.wait_for_draft_mutations_then(move || {
                        on_done(result.map_err(anyhow::Error::from));
                    });
                }
            },
        );
    }

    /// Run a close continuation only after queued draft mutations have drained.
    fn wait_for_draft_mutations_then<F: FnOnce() + 'static>(&self, on_done: F) {
        if self.imp().drafts.mutation_inflight.get()
            || !self.imp().drafts.pending_deletes.borrow().is_empty()
            || self.imp().drafts.restore_inflight_count.get() > 0
        {
            let window_weak = self.downgrade();
            glib::timeout_add_local_once(DRAFT_MUTATION_WAIT_POLL_INTERVAL, move || {
                if let Some(window) = window_weak.upgrade() {
                    window.wait_for_draft_mutations_then(on_done);
                }
            });
            return;
        }
        on_done();
    }

    /// Snapshot and durably write one autosave candidate at a time.
    fn drive_dirty_draft_pipeline(
        &self,
        mut candidates: Vec<DirtyDraftCandidate>,
        accepted: Vec<AcceptedDraft>,
        failures: DraftPipelineFailures,
    ) {
        let Some(candidate) = candidates.pop() else {
            self.commit_dirty_draft_pipeline(accepted, failures);
            return;
        };

        let window_weak = self.downgrade();
        // Every terminal outcome clears this capture's token before the next
        // candidate starts, preventing stale disposal cancellation.
        let finish_snapshot = move |outcome: buffer_snapshot::BufferSnapshotOutcome| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            match outcome {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                    window.imp().drafts.autosave_snapshot.take();
                    let Some(editor) = candidate.editor.upgrade() else {
                        window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                        return;
                    };
                    // Capture spans main-loop turns. Discard stale text and
                    // request a new pass when identity or generation changed.
                    if editor.draft_id().as_deref() != Some(candidate.draft_id.as_str())
                        || editor.draft_dirty_generation() != candidate.dirty_generation
                        || !editor.is_modified()
                        || editor.is_evicted()
                    {
                        window.imp().drafts.autosave_pending.set(true);
                        window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                        return;
                    }

                    let data_dir = json_store::data_dir();
                    let draft_id = candidate.draft_id.clone();
                    let original_path = candidate.original_path.clone();
                    let completion = DirtyDraftCompletion {
                        draft_id: candidate.draft_id,
                        dirty_generation: candidate.dirty_generation,
                        editor: candidate.editor,
                        intent: candidate.intent,
                    };
                    let window_weak = window.downgrade();
                    window.note_complete_draft_body_admitted();
                    // The worker owns the only complete body and drops it as
                    // soon as the durable write finishes.
                    spawn_blocking_then(
                        (),
                        move || {
                            let text = text.into_string_on_worker();
                            delay_draft_body_for_test();
                            fail_next_draft_body_for_test()?;
                            let result = draft_service::write_draft(&data_dir, &draft_id, &text)
                                .map(|()| DraftEntry {
                                    draft_id,
                                    original_mtime_secs: original_path
                                        .as_deref()
                                        .and_then(editor_io::mtime_secs),
                                    original_path,
                                    saved_at_secs: editor_io::now_epoch_secs(),
                                });
                            drop(text);
                            result
                        },
                        move |(), result| {
                            let Some(window) = window_weak.upgrade() else {
                                return;
                            };
                            window.note_complete_draft_body_released();
                            let mut accepted = accepted;
                            let mut failures = failures;
                            match result {
                                Ok(entry) => accepted.push(AcceptedDraft { entry, completion }),
                                Err(error) => {
                                    tracing::warn!("Failed to write draft: {error}");
                                    failures.body_write.push(completion.draft_id);
                                }
                            }
                            window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                        },
                    );
                }
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                    window.imp().drafts.autosave_snapshot.take();
                    let mut failures = failures;
                    failures.over_limit += 1;
                    if let Some(editor) = candidate.editor.upgrade()
                        && editor.draft_id().as_deref() == Some(candidate.draft_id.as_str())
                        && editor.draft_dirty_generation() == candidate.dirty_generation
                    {
                        Self::show_automatic_recovery_limit(&editor);
                    }
                    window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                }
                buffer_snapshot::BufferSnapshotOutcome::Cancelled(_) => {
                    window.imp().drafts.autosave_snapshot.take();
                    let mut failures = failures;
                    failures.snapshot_cancelled += 1;
                    window.imp().drafts.autosave_pending.set(true);
                    window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                }
            }
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&candidate.buffer) {
            let snapshot = buffer_snapshot::snapshot_buffer_text_async_budgeted(
                candidate.buffer,
                automatic_draft_limit(),
                finish_snapshot,
            );
            *self.imp().drafts.autosave_snapshot.borrow_mut() = Some(snapshot);
        } else {
            finish_snapshot(buffer_snapshot::snapshot_buffer_text_direct_budgeted(
                &candidate.buffer,
                automatic_draft_limit(),
            ));
        }
    }

    /// Commit compact successful entries once, then accept matching generations.
    fn commit_dirty_draft_pipeline(
        &self,
        accepted: Vec<AcceptedDraft>,
        failures: DraftPipelineFailures,
    ) {
        if accepted.is_empty() {
            self.finish_autosave_pipeline(&failures);
            return;
        }
        let data_dir = json_store::data_dir();
        let window_weak = self.downgrade();
        let entries: Vec<DraftEntry> = accepted.iter().map(|item| item.entry.clone()).collect();
        let session = self.collect_session();
        let authority = self.imp().drafts.manifest_authority.get();

        spawn_blocking_then(
            (),
            move || {
                delay_draft_manifest_for_test();
                if let Err(error) = fail_next_draft_manifest_for_test() {
                    return Err(DraftManifestFailure::injected(&error));
                }
                let result =
                    draft_service::update_manifest(&data_dir, &session, authority, |manifest| {
                        for entry in entries {
                            manifest.upsert(entry);
                        }
                    })
                    .map_err(DraftManifestFailure::from);
                delay_draft_manifest_completion_for_test();
                result
            },
            move |(), result| {
                if let Some(window) = window_weak.upgrade() {
                    match result {
                        Ok(commit) => {
                            window.accept_draft_manifest_commit(commit);
                            for accepted in accepted {
                                let completion = accepted.completion;
                                let Some(editor) = completion.editor.upgrade() else {
                                    continue;
                                };
                                // Durability covers only this captured generation;
                                // a newer edit must remain dirty for a later pass.
                                if editor.draft_id().as_deref()
                                    == Some(completion.draft_id.as_str())
                                    && editor.draft_dirty_generation()
                                        == completion.dirty_generation
                                    && window
                                        .imp()
                                        .drafts
                                        .mutation_order
                                        .borrow()
                                        .is_current(&completion.intent)
                                {
                                    editor.set_draft_dirty(false);
                                    window.clear_automatic_recovery_limit(&editor);
                                }
                            }
                        }
                        Err(error) => {
                            window.reject_draft_manifest_authority(error.authority);
                            tracing::warn!("Failed to save draft manifest: {}", error.detail);
                            window.publish_status_message(
                                "Draft autosave could not confirm recovery metadata; changes remain retryable.",
                                NotificationSeverity::Warning,
                            );
                        }
                    }
                    window.finish_autosave_pipeline(&failures);
                }
            },
        );
    }

    /// Release the in-flight gate and run one coalesced follow-up when needed.
    fn finish_autosave_pipeline(&self, failures: &DraftPipelineFailures) {
        if failures.snapshot_cancelled > 0
            || failures.over_limit > 0
            || !failures.body_write.is_empty()
        {
            self.publish_status_message(
                &format!(
                    "Draft autosave left {} document(s) retryable (cancelled: {}, over limit: {}, write: {}).",
                    failures.snapshot_cancelled + failures.over_limit + failures.body_write.len(),
                    failures.snapshot_cancelled,
                    failures.over_limit,
                    failures.body_write.len(),
                ),
                NotificationSeverity::Warning,
            );
        }
        self.imp().drafts.autosave_inflight.set(false);
        self.imp().drafts.mutation_inflight.set(false);
        self.drive_pending_draft_mutations();
    }

    #[cfg(feature = "test-utils")]
    fn note_complete_draft_body_admitted(&self) {
        let retained = self.imp().drafts.retained_complete_bodies.get() + 1;
        self.imp().drafts.retained_complete_bodies.set(retained);
        self.imp().drafts.max_retained_complete_bodies.set(
            self.imp()
                .drafts
                .max_retained_complete_bodies
                .get()
                .max(retained),
        );
    }

    #[cfg(not(feature = "test-utils"))]
    fn note_complete_draft_body_admitted(&self) {}

    #[cfg(feature = "test-utils")]
    fn note_complete_draft_body_released(&self) {
        self.imp().drafts.retained_complete_bodies.set(0);
    }

    #[cfg(not(feature = "test-utils"))]
    fn note_complete_draft_body_released(&self) {}

    /// Remember that a fresh autosave pass is needed after the active batch.
    pub(crate) fn mark_draft_autosave_pending_if_inflight(&self) {
        if self.imp().drafts.autosave_inflight.get() || self.imp().drafts.mutation_inflight.get() {
            self.imp().drafts.autosave_pending.set(true);
        }
    }

    /// Whether draft persistence or deferred startup restore blocks readiness.
    pub(crate) fn draft_workflow_blocks_readiness(&self) -> bool {
        self.imp().drafts.autosave_inflight.get()
            || self.imp().drafts.mutation_inflight.get()
            || !self.imp().drafts.pending_deletes.borrow().is_empty()
            || self.imp().drafts.restore_inflight_count.get() > 0
            || self.imp().drafts.lazy_restore_inflight.get()
            || !self.imp().drafts.lazy_restore_queue.borrow().is_empty()
    }

    /// Check whether a file-backed editor has restored draft content available.
    pub fn check_draft_on_open(&self, editor: &LushtextEditorPage, path: &Path) {
        if self.apply_preloaded_draft_for_path(editor, path) {
            return;
        }

        let draft_entry = self
            .imp()
            .drafts
            .manifest
            .borrow()
            .find_by_path(path)
            .cloned();

        let Some(entry) = draft_entry else {
            return;
        };

        self.queue_lazy_draft_restore(editor, entry);
    }

    /// Apply startup-preloaded draft data for a path, if one was prepared.
    ///
    /// Failed first-open placeholders use this before their path identity is
    /// cleared so crash-recovered edits remain tied to the user-requested file.
    pub(crate) fn apply_preloaded_draft_for_path(
        &self,
        editor: &LushtextEditorPage,
        path: &Path,
    ) -> bool {
        let draft_id = draft_service::draft_id_for_path(path);
        let Some(preloaded) = self.take_preloaded_draft(&draft_id) else {
            return false;
        };
        match preloaded {
            GuardedPreloadedDraftRestore::Content(draft_content) => {
                let Some(entry) = self
                    .imp()
                    .drafts
                    .manifest
                    .borrow()
                    .find_by_id(&draft_id)
                    .cloned()
                else {
                    return false;
                };
                self.note_draft_restore_started();
                self.apply_draft(
                    &DraftRestoreTicket::capture(editor, entry),
                    draft_content,
                    DraftRestoreTracking::Ordinary,
                );
            }
            GuardedPreloadedDraftRestore::Compact(PreloadedDraftRestore::SkipStaleFile) => {
                Self::show_stale_draft_skipped(editor);
            }
            GuardedPreloadedDraftRestore::Compact(PreloadedDraftRestore::SkipOversized) => {
                Self::show_oversized_draft_skipped(editor);
            }
            GuardedPreloadedDraftRestore::Compact(PreloadedDraftRestore::LazyAggregateBudget) => {
                let Some(entry) = self
                    .imp()
                    .drafts
                    .manifest
                    .borrow()
                    .find_by_id(&draft_id)
                    .cloned()
                else {
                    return false;
                };
                self.queue_lazy_draft_restore(editor, entry);
            }
            GuardedPreloadedDraftRestore::Compact(PreloadedDraftRestore::Content(_)) => {
                unreachable!("eager bodies cross GTK only with transferable disposal ownership")
            }
        }
        true
    }

    /// Delete the draft for a given file path.
    pub fn delete_draft_for_path(&self, path: &Path) {
        let draft_id = {
            let manifest = self.imp().drafts.manifest.borrow();
            manifest
                .find_by_path(path)
                .map(|entry| entry.draft_id.clone())
        };
        if let Some(draft_id) = draft_id {
            self.delete_draft_by_id(&draft_id);
        }
    }

    /// Delete a draft by its ID and persist the manifest update.
    pub fn delete_draft_by_id(&self, draft_id: &str) {
        // Intent is assigned on GTK before an older body worker can finish and
        // before this compact delete waits behind the single-flight mutation.
        let intent = self
            .imp()
            .drafts
            .mutation_order
            .borrow_mut()
            .advance(draft_id);
        self.imp()
            .drafts
            .delete_tombstones
            .borrow_mut()
            .insert(draft_id.to_string(), intent.clone());
        self.imp()
            .drafts
            .manifest
            .borrow_mut()
            .remove_by_id(draft_id);

        let drafts = &self.imp().drafts;
        let already_pending = !drafts
            .pending_delete_ids
            .borrow_mut()
            .insert(draft_id.to_string());
        let mut pending_deletes = drafts.pending_deletes.borrow_mut();
        // Preserve global order by moving a superseded same-ID command to the
        // tail. Distinct-ID admission stays O(1) for large close batches.
        if already_pending
            && let Some(index) = pending_deletes
                .iter()
                .position(|pending| pending.draft_id == draft_id)
        {
            pending_deletes.remove(index);
        }
        pending_deletes.push_back(intent);
        drop(pending_deletes);
        self.drive_pending_draft_mutations();
    }

    /// Run queued compact deletes only after every earlier body/manifest command.
    fn drive_pending_draft_mutations(&self) {
        if self.imp().drafts.mutation_inflight.get()
            || self.imp().drafts.orphan_cleanup_inflight.get()
        {
            return;
        }
        let Some(intent) = self.imp().drafts.pending_deletes.borrow_mut().pop_front() else {
            let rerun = self.imp().drafts.autosave_pending.replace(false);
            if rerun {
                self.autosave_tick();
            }
            return;
        };
        self.imp()
            .drafts
            .pending_delete_ids
            .borrow_mut()
            .remove(&intent.draft_id);
        self.imp().drafts.mutation_inflight.set(true);

        let data_dir = json_store::data_dir();
        let draft_id = intent.draft_id.clone();
        let session = self.collect_session();
        let authority = self.imp().drafts.manifest_authority.get();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                // Keep the persisted manifest as the durable retry marker until
                // the body is gone. A failed body deletion therefore leaves a
                // fully recoverable pre-delete state across unrelated manifest
                // mutations and process restart.
                delay_draft_delete_for_test();
                let body_error = fail_next_draft_delete_for_test()
                    .and_then(|()| draft_service::delete_draft_file(&data_dir, &draft_id))
                    .err()
                    .map(|error| error.to_string());
                let manifest_result = if body_error.is_none() {
                    delay_draft_manifest_for_test();
                    Some(match fail_next_draft_manifest_for_test() {
                        Ok(()) => draft_service::remove_manifest_entry(
                            &data_dir, &session, authority, &draft_id,
                        )
                        .map_err(DraftManifestFailure::from),
                        Err(error) => Err(DraftManifestFailure::injected(&error)),
                    })
                } else {
                    None
                };
                (body_error, manifest_result)
            },
            move |(), (body_error, manifest_result)| {
                if let Some(window) = window_weak.upgrade() {
                    let deletion_terminal =
                        body_error.is_none() && manifest_result.as_ref().is_some_and(Result::is_ok);
                    if let Some(error) = body_error.as_deref() {
                        tracing::warn!("Failed to delete draft file {}: {error}", intent.draft_id);
                        window.publish_status_message(
                            "Draft cleanup could not remove one recovery body; cleanup remains retryable.",
                            NotificationSeverity::Warning,
                        );
                    }
                    match manifest_result {
                        Some(Ok(commit)) => window.accept_draft_manifest_commit(commit),
                        Some(Err(error)) => {
                            window.reject_draft_manifest_authority(error.authority);
                            tracing::warn!(
                                "Failed to save manifest before draft deletion {}: {}",
                                intent.draft_id,
                                error.detail,
                            );
                            window.publish_status_message(
                                "Draft cleanup could not confirm recovery metadata; cleanup remains retryable.",
                                NotificationSeverity::Warning,
                            );
                        }
                        None => {}
                    }
                    if deletion_terminal {
                        let drafts = &window.imp().drafts;
                        let tombstone_is_current =
                            drafts.delete_tombstones.borrow().get(&intent.draft_id)
                                == Some(&intent);
                        if tombstone_is_current {
                            drafts
                                .delete_tombstones
                                .borrow_mut()
                                .remove(&intent.draft_id);
                            drafts
                                .mutation_order
                                .borrow_mut()
                                .retire_if_current(&intent);
                        }
                    }
                    window.imp().drafts.mutation_inflight.set(false);
                    window.drive_pending_draft_mutations();
                }
            },
        );
    }

    /// Allocate a draft ID for a new editor page.
    pub fn assign_draft_id(&self, editor: &LushtextEditorPage) {
        let id = if let Some(ref path) = editor.file_path() {
            draft_service::draft_id_for_path(path)
        } else {
            draft_service::new_untitled_draft_id()
        };
        editor.set_draft_id(id);
    }

    /// Whether a failed deletion still owns an explicit retry tombstone.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn draft_delete_tombstoned_for_test(&self, draft_id: &str) -> bool {
        self.imp()
            .drafts
            .delete_tombstones
            .borrow()
            .contains_key(draft_id)
    }

    /// Whether one serialized draft body/manifest mutation still owns its worker.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn draft_mutation_inflight_for_test(&self) -> bool {
        self.imp().drafts.mutation_inflight.get()
    }

    fn note_draft_restore_started(&self) {
        let count = self.imp().drafts.restore_inflight_count.get();
        self.imp()
            .drafts
            .restore_inflight_count
            .set(count.saturating_add(1));
    }

    fn note_draft_restore_finished(&self) {
        let count = self.imp().drafts.restore_inflight_count.get();
        self.imp()
            .drafts
            .restore_inflight_count
            .set(count.saturating_sub(1));
    }
}

/// Build one grouped status message without exposing private recovery contents.
fn orphan_cleanup_failure_message(failures: &[draft_service::DraftOrphanCleanupFailure]) -> String {
    let mut status = 0usize;
    let mut delete = 0usize;
    let mut manifest = 0usize;
    for failure in failures {
        match failure {
            draft_service::DraftOrphanCleanupFailure::Status(_) => status += 1,
            draft_service::DraftOrphanCleanupFailure::Delete(_) => delete += 1,
            draft_service::DraftOrphanCleanupFailure::Manifest(_) => manifest += 1,
        }
    }
    format!(
        "Draft recovery cleanup preserved retryable items (status: {status}, delete: {delete}, manifest: {manifest})"
    )
}

fn delay_draft_restore_for_test() {
    #[cfg(feature = "test-utils")]
    std::thread::sleep(Duration::from_millis(
        DRAFT_RESTORE_DELAY_MS.load(Ordering::Acquire),
    ));
}

fn delay_draft_body_for_test() {
    #[cfg(feature = "test-utils")]
    std::thread::sleep(Duration::from_millis(
        DRAFT_BODY_DELAY_MS.load(Ordering::Acquire),
    ));
}

fn delay_draft_manifest_for_test() {
    #[cfg(feature = "test-utils")]
    std::thread::sleep(Duration::from_millis(
        DRAFT_MANIFEST_DELAY_MS.load(Ordering::Acquire),
    ));
}

fn delay_draft_manifest_completion_for_test() {
    #[cfg(feature = "test-utils")]
    std::thread::sleep(Duration::from_millis(
        DRAFT_MANIFEST_COMPLETION_DELAY_MS.load(Ordering::Acquire),
    ));
}

fn delay_draft_delete_for_test() {
    #[cfg(feature = "test-utils")]
    std::thread::sleep(Duration::from_millis(
        DRAFT_DELETE_DELAY_MS.load(Ordering::Acquire),
    ));
}

fn fail_next_draft_body_for_test() -> Result<()> {
    #[cfg(feature = "test-utils")]
    if FAIL_NEXT_DRAFT_BODY.swap(false, Ordering::AcqRel) {
        anyhow::bail!("injected draft body failure");
    }
    Ok(())
}

fn fail_next_draft_manifest_for_test() -> Result<()> {
    #[cfg(feature = "test-utils")]
    if FAIL_NEXT_DRAFT_MANIFEST.swap(false, Ordering::AcqRel) {
        anyhow::bail!("injected draft manifest failure");
    }
    Ok(())
}

fn fail_next_draft_delete_for_test() -> Result<()> {
    #[cfg(feature = "test-utils")]
    if FAIL_NEXT_DRAFT_DELETE.swap(false, Ordering::AcqRel) {
        anyhow::bail!("injected draft delete failure");
    }
    Ok(())
}

fn first_dirty_autosave_debounce() -> Duration {
    #[cfg(feature = "test-utils")]
    {
        Duration::from_millis(FIRST_DIRTY_AUTOSAVE_DELAY_MS.load(Ordering::Acquire))
    }
    #[cfg(not(feature = "test-utils"))]
    {
        Duration::from_millis(FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS)
    }
}

fn automatic_draft_limit() -> u64 {
    #[cfg(feature = "test-utils")]
    {
        AUTOMATIC_DRAFT_LIMIT_BYTES.load(Ordering::Acquire)
    }
    #[cfg(not(feature = "test-utils"))]
    {
        draft_service::MAX_AUTOMATIC_DRAFT_BYTES
    }
}

fn release_eager_preloads(
    preloaded: &mut crate::ui::plain_disposal::DisposalOwned<
        HashMap<String, PreloadedDraftRestore>,
    >,
) {
    let guarded = std::mem::take(preloaded);
    let (compact, retiring) = guarded.split_for_worker_retirement(detach_eager_preload_bodies);
    *preloaded = crate::ui::plain_disposal::DisposalOwned::small_unreserved(compact);
    drop(retiring);
}

fn detach_eager_preload_bodies(
    preloaded: &mut HashMap<String, PreloadedDraftRestore>,
) -> Vec<String> {
    let mut retiring = Vec::new();
    for restore in preloaded.values_mut() {
        if matches!(restore, PreloadedDraftRestore::Content(_)) {
            let PreloadedDraftRestore::Content(content) =
                std::mem::replace(restore, PreloadedDraftRestore::LazyAggregateBudget)
            else {
                unreachable!("content match was checked before replacement");
            };
            retiring.push(content);
        }
    }
    retiring
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::draft::{DraftEntry, DraftManifest};

    fn entry(id: &str, saved_at_secs: u64) -> DraftEntry {
        DraftEntry {
            draft_id: id.to_string(),
            original_path: Some(PathBuf::from(format!("/{id}.rs"))),
            original_mtime_secs: Some(saved_at_secs),
            saved_at_secs,
        }
    }

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

    #[test]
    fn restore_ticket_rejects_every_stale_identity_dimension() {
        let ticket = restore_ticket();
        assert!(draft_restore_is_current(&ticket, &restore_facts()));

        let mut edited = restore_facts();
        edited.dirty_generation += 1;
        assert!(!draft_restore_is_current(&ticket, &edited));

        let mut reloaded = restore_facts();
        reloaded.load_generation += 1;
        assert!(!draft_restore_is_current(&ticket, &reloaded));

        let mut renamed = restore_facts();
        renamed.path = Some(PathBuf::from("/renamed.rs"));
        assert!(!draft_restore_is_current(&ticket, &renamed));

        let mut reused = restore_facts();
        reused.draft_id = Some("different".to_string());
        assert!(!draft_restore_is_current(&ticket, &reused));

        let mut replaced = restore_facts();
        replaced.manifest_entry = Some(entry("restore", 2));
        assert!(!draft_restore_is_current(&ticket, &replaced));
    }

    #[test]
    fn cleanup_merge_removes_only_exact_committed_generation() {
        let old = entry("same", 1);
        let newer = entry("same", 2);
        let unrelated = entry("other", 1);
        let mut manifest = DraftManifest {
            drafts: vec![newer.clone(), unrelated.clone()],
            cleanup_continuation: None,
        };

        draft_service::merge_committed_orphan_removals(
            &mut manifest,
            &HashMap::from([(
                old.draft_id.clone(),
                draft_service::DraftEntryFingerprint::from_entry(&old),
            )]),
        );

        assert_eq!(manifest.drafts, vec![newer, unrelated]);
    }

    #[test]
    fn orphan_cleanup_follow_up_resumes_manifest_pagination() {
        assert_eq!(
            orphan_cleanup_follow_up(true, Some(256), false, 4),
            OrphanCleanupFollowUp::Schedule {
                manifest_offset: 256,
                delay: ORPHAN_CLEANUP_FOLLOWUP_DELAY,
                next_failure_streak: 0,
            }
        );
    }

    #[test]
    fn orphan_cleanup_follow_up_restarts_cursorless_failure_from_zero() {
        assert_eq!(
            orphan_cleanup_follow_up(true, None, true, 0),
            OrphanCleanupFollowUp::Schedule {
                manifest_offset: 0,
                delay: ORPHAN_CLEANUP_FOLLOWUP_DELAY,
                next_failure_streak: 1,
            }
        );
    }

    #[test]
    fn orphan_cleanup_follow_up_caps_failure_backoff() {
        assert_eq!(
            orphan_cleanup_follow_up(true, Some(512), true, u32::MAX),
            OrphanCleanupFollowUp::Schedule {
                manifest_offset: 512,
                delay: ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF,
                next_failure_streak: u32::MAX,
            }
        );
    }

    #[test]
    fn orphan_cleanup_follow_up_stops_when_has_more_work_is_false() {
        assert_eq!(
            orphan_cleanup_follow_up(false, Some(256), true, 8),
            OrphanCleanupFollowUp::Stop
        );
    }

    #[test]
    fn cleanup_merge_removes_matching_generation_and_preserves_additions() {
        let removed = entry("removed", 1);
        let concurrent = entry("concurrent", 2);
        let mut manifest = DraftManifest {
            drafts: vec![removed.clone(), concurrent.clone()],
            cleanup_continuation: None,
        };

        draft_service::merge_committed_orphan_removals(
            &mut manifest,
            &HashMap::from([(
                removed.draft_id.clone(),
                draft_service::DraftEntryFingerprint::from_entry(&removed),
            )]),
        );

        assert_eq!(manifest.drafts, vec![concurrent]);
    }

    #[test]
    fn cleanup_failure_message_groups_failure_categories() {
        let path = PathBuf::from("/drafts/item.draft");
        let failures = vec![
            draft_service::DraftOrphanCleanupStatusError {
                path: path.clone(),
                detail: "denied".to_string(),
            }
            .into(),
            draft_service::DraftOrphanCleanupDeleteError {
                draft_id: "item".to_string(),
                path: path.clone(),
                detail: "read-only".to_string(),
            }
            .into(),
            draft_service::DraftOrphanCleanupManifestError::Write {
                path,
                detail: "disk full".to_string(),
            }
            .into(),
        ];

        assert_eq!(
            orphan_cleanup_failure_message(&failures),
            "Draft recovery cleanup preserved retryable items (status: 1, delete: 1, manifest: 1)"
        );
    }

    #[test]
    fn eager_preload_release_preserves_lazy_markers_for_slow_file_loads() {
        let mut preloaded = HashMap::from([
            (
                "eager".to_string(),
                PreloadedDraftRestore::Content("body".to_string()),
            ),
            (
                "lazy".to_string(),
                PreloadedDraftRestore::LazyAggregateBudget,
            ),
            (
                "oversized".to_string(),
                PreloadedDraftRestore::SkipOversized,
            ),
        ]);

        let retired = detach_eager_preload_bodies(&mut preloaded);

        assert_eq!(
            preloaded,
            HashMap::from([
                (
                    "eager".to_string(),
                    PreloadedDraftRestore::LazyAggregateBudget
                ),
                (
                    "lazy".to_string(),
                    PreloadedDraftRestore::LazyAggregateBudget
                ),
                (
                    "oversized".to_string(),
                    PreloadedDraftRestore::SkipOversized
                ),
            ])
        );
        assert_eq!(retired, vec!["body".to_string()]);
    }
}

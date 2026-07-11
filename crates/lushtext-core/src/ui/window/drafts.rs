// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft persistence, recovery, and autosave flows for the main window.
//!
//! This slice owns the data-safety-sensitive draft lifecycle: close-time flush,
//! crash recovery, autosave, and manifest maintenance. Session-only tab-state
//! capture lives separately in `session_persistence.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::model::draft::{DraftEntry, FileDraftRestoreResolution, PreloadedDraftRestore};
use crate::services::notifications::{
    InlineActionNotification, InlineNotificationStyle, NotificationSeverity,
};
use crate::services::{draft_service, editor_io, json_store};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::LushtextEditorPage;
use anyhow::Result;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

/// First-dirty draft autosave delay after a clean edit cycle.
///
/// 750ms persists new unsaved work sooner than the regular 5s autosave tick
/// while still coalescing quick typing into one draft write.
const FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS: u64 = 750;

/// Delay before startup releases preloaded bodies and begins orphan inspection.
///
/// Two seconds lets restored editors consume their recovery snapshots before a
/// background cleanup worker revalidates the same persisted artifacts.
const ORPHAN_CLEANUP_START_DELAY: Duration = Duration::from_secs(2);
/// Delay for the one permitted follow-up bounded cleanup pass.
///
/// Thirty seconds avoids a tight retry loop when permissions or storage remain
/// unavailable while still making progress on a directory that exceeded the cap.
const ORPHAN_CLEANUP_FOLLOWUP_DELAY: Duration = Duration::from_secs(30);

#[cfg(feature = "test-utils")]
/// Test override for first-dirty autosave timing without changing production policy.
static FIRST_DIRTY_AUTOSAVE_DELAY_MS: AtomicU64 = AtomicU64::new(FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS);
#[cfg(feature = "test-utils")]
/// Test override for the automatic recovery byte limit without huge fixtures.
static AUTOMATIC_DRAFT_LIMIT_BYTES: AtomicU64 =
    AtomicU64::new(draft_service::MAX_AUTOMATIC_DRAFT_BYTES);
#[cfg(feature = "test-utils")]
/// Test-only worker delay for deterministic stale lazy-restore completions.
static LAZY_DRAFT_READ_DELAY_MS: AtomicU64 = AtomicU64::new(0);

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

/// Delay lazy draft reads for deterministic freshness tests.
#[cfg(feature = "test-utils")]
pub fn set_lazy_draft_read_delay_for_test(delay_ms: u64) {
    LAZY_DRAFT_READ_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Main-thread editor token paired with one accepted autosave snapshot.
struct DirtyDraftCompletion {
    /// Stable identity accepted by the body writer.
    draft_id: String,
    /// Dirty generation that may be cleared after manifest acceptance.
    dirty_generation: u64,
    /// Weak target so pending work never retains a closed tab.
    editor: glib::WeakRef<LushtextEditorPage>,
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
    #[error("failed to save draft manifest on close: {0}")]
    Manifest(String),
}

/// Freshness token for one startup draft deferred by the eager aggregate cap.
pub(super) struct LazyDraftRestoreCandidate {
    /// Manifest snapshot resolved by the background reader.
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
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => text,
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                    Self::show_automatic_recovery_limit(editor);
                    write_errors.push(format!(
                        "{draft_id}: document exceeds the automatic recovery limit"
                    ));
                    continue;
                }
                buffer_snapshot::BufferSnapshotOutcome::Cancelled => {
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
            draft_service::update_manifest(&data_dir, |manifest| {
                for entry in manifest_updates {
                    manifest.upsert(entry);
                }
            })
            .map_err(|e| anyhow::anyhow!("failed to save draft manifest on close: {e}"))?;
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

    /// Flush dirty drafts for close without monopolizing a GTK main-loop turn.
    ///
    /// Copies are serialized on GTK, writes run on workers, and `on_done` runs
    /// back on GTK after every candidate is accepted or classified.
    pub fn flush_dirty_drafts_async<F: FnOnce(Result<()>) + 'static>(&self, on_done: F) {
        let candidates = self.collect_close_draft_candidates();
        if candidates.is_empty() {
            self.clear_close_discard_drafts();
            on_done(Ok(()));
            return;
        }
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

        if let Some(preloaded) = self.imp().drafts.preloaded.borrow_mut().remove(draft_id) {
            match preloaded {
                PreloadedDraftRestore::Content(draft_content) => {
                    Self::apply_draft(editor, &draft_content);
                }
                PreloadedDraftRestore::SkipStaleFile => {
                    tracing::warn!(
                        "Untitled draft {draft_id} unexpectedly carried a stale file warning"
                    );
                }
                PreloadedDraftRestore::SkipOversized => {
                    Self::show_oversized_draft_skipped(editor);
                }
                PreloadedDraftRestore::LazyAggregateBudget => {
                    self.queue_lazy_draft_restore(editor, entry);
                }
            }
            return;
        }

        let data_dir = json_store::data_dir();
        let draft_id = draft_id.to_string();
        let editor_weak = editor.downgrade();

        // Run disk I/O on a worker and deliver the result on GTK's main loop.
        // The weak reference avoids retaining an editor that closes mid-read.
        spawn_blocking_then(
            (),
            move || draft_service::read_draft(&data_dir, &draft_id),
            move |(), result| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(Some(draft_content)) => {
                        Self::apply_draft(&editor, &draft_content);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::error!("Failed to read draft from disk: {e}");
                        if e.downcast_ref::<draft_service::DraftReadError>()
                            .is_some_and(|error| {
                                matches!(error, draft_service::DraftReadError::Oversized { .. })
                            })
                        {
                            Self::show_oversized_draft_skipped(&editor);
                        }
                    }
                }
            },
        );
    }

    /// Enqueue one aggregate-budget skip and start the serialized reader.
    fn queue_lazy_draft_restore(&self, editor: &LushtextEditorPage, entry: DraftEntry) {
        self.imp()
            .drafts
            .lazy_restore_queue
            .borrow_mut()
            .push_back(LazyDraftRestoreCandidate {
                entry,
                editor: editor.downgrade(),
                expected_path: editor.file_path(),
                dirty_generation: editor.draft_dirty_generation(),
                load_generation: editor.load_generation(),
            });
        self.drive_lazy_draft_restore_queue();
    }

    /// Admit at most one lazy draft body to GTK and reject stale completions.
    fn drive_lazy_draft_restore_queue(&self) {
        if self.imp().drafts.lazy_restore_inflight.get() {
            return;
        }
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
        let data_dir = json_store::data_dir();
        let entry = candidate.entry.clone();
        let draft_id = entry.draft_id.clone();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                #[cfg(feature = "test-utils")]
                std::thread::sleep(Duration::from_millis(
                    LAZY_DRAFT_READ_DELAY_MS.load(Ordering::Acquire),
                ));
                draft_service::resolve_draft_restore(&data_dir, &entry)
            },
            move |(), result| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                window.imp().drafts.lazy_restore_inflight.set(false);
                // The read may outlive edits, tab reuse, or a newer file load.
                // Apply only when every captured identity still matches.
                if let Some(editor) = candidate.editor.upgrade()
                    && editor.draft_id().as_deref() == Some(draft_id.as_str())
                    && editor.file_path() == candidate.expected_path
                    && editor.draft_dirty_generation() == candidate.dirty_generation
                    && editor.load_generation() == candidate.load_generation
                {
                    match result {
                        Ok(FileDraftRestoreResolution::Restore { content }) => {
                            Self::apply_draft(&editor, &content);
                        }
                        Ok(FileDraftRestoreResolution::SkipStale) => {
                            Self::show_stale_draft_skipped(&editor);
                            window.delete_draft_by_id(&draft_id);
                        }
                        Ok(FileDraftRestoreResolution::SkipOversized) => {
                            Self::show_oversized_draft_skipped(&editor);
                        }
                        Ok(
                            FileDraftRestoreResolution::SkipUnavailable
                            | FileDraftRestoreResolution::MissingDraft,
                        ) => {}
                        Err(error) => {
                            tracing::warn!("Failed to lazily restore draft {draft_id}: {error}");
                            editor.emit_inline_notification(InlineActionNotification {
                                style: InlineNotificationStyle::Warning,
                                title: "Draft Restore Failed".to_string(),
                                body: "The preserved recovery draft could not be read. The tab remains usable and the recovery files were kept.".to_string(),
                                primary_button: None,
                                secondary_button: None,
                            });
                        }
                    }
                }
                window.drive_lazy_draft_restore_queue();
            },
        );
    }

    /// Apply restored draft content to the editor buffer and show the inline alert action.
    fn apply_draft(editor: &LushtextEditorPage, content: &str) {
        let buffer = editor.buffer();
        // Seed local history before mutating the buffer because `set_text()`
        // can already flip the modified state and trigger the baseline path.
        // Restored drafts should baseline the restored work, not the stale file.
        editor.seed_local_history_from_restored_draft(content);
        editor.set_minimap_tracking_suspended(true);
        buffer.begin_irreversible_action();
        buffer.set_text(content);
        buffer.end_irreversible_action();
        editor.set_minimap_tracking_suspended(false);
        buffer.set_modified(true);
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
        let window_weak = self.downgrade();
        // `timeout_add_local_once` schedules a main-thread callback after a
        // delay and permits non-`Send` GTK captures through the local main loop.
        glib::timeout_add_local_once(ORPHAN_CLEANUP_START_DELAY, move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
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
            window.run_orphan_cleanup_pass(true, 0);
        });
    }

    /// Run one inspect/execute pass off the GTK thread and merge exact commits.
    ///
    /// `allow_followup` limits startup to one later bounded pass. Persistent
    /// failures remain retryable on disk and visible in diagnostics instead of
    /// recursively scheduling workers for as long as the app stays open.
    fn run_orphan_cleanup_pass(&self, allow_followup: bool, manifest_offset: usize) {
        let data_dir = json_store::data_dir();
        // Clone GTK-owned state before dispatch so the worker receives plain
        // owned data and never borrows through the window's interior mutability.
        let manifest = self.imp().drafts.manifest.borrow().clone();
        spawn_blocking_then(
            self.clone(),
            move || {
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
            move |window, result| match result {
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
                    if allow_followup && outcome.has_more_work {
                        // Directory-cap retries have no manifest cursor, so they
                        // restart the manifest page while directory cleanup advances.
                        let next_manifest_offset = outcome.next_manifest_offset.unwrap_or(0);
                        let window_weak = window.downgrade();
                        glib::timeout_add_local_once(ORPHAN_CLEANUP_FOLLOWUP_DELAY, move || {
                            if let Some(window) = window_weak.upgrade() {
                                window.run_orphan_cleanup_pass(false, next_manifest_offset);
                            }
                        });
                    }
                }
                Err(error) => {
                    let message = format!("Draft recovery cleanup scan failed: {error}");
                    tracing::warn!("{message}");
                    window.publish_status_message(&message, NotificationSeverity::Warning);
                }
            },
        );
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
        if self.imp().drafts.autosave_inflight.get() {
            self.imp().drafts.autosave_pending.set(true);
            return;
        }

        let dirty_tabs = self.collect_dirty_draft_candidates();
        if dirty_tabs.is_empty() {
            return;
        }

        self.imp().drafts.autosave_inflight.set(true);
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

    /// Whether one aggregate-budget draft read is currently active.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn lazy_draft_restore_inflight_for_test(&self) -> bool {
        self.imp().drafts.lazy_restore_inflight.get()
    }

    /// Cancel the active autosave snapshot to exercise retry semantics.
    #[cfg(feature = "test-utils")]
    pub fn cancel_draft_snapshot_for_test(&self) {
        if let Some(cancellation) = self
            .imp()
            .drafts
            .autosave_snapshot_cancellation
            .borrow()
            .as_ref()
        {
            cancellation.cancel();
        }
    }

    /// Schedule a short autosave after the first dirty edit in a clean cycle.
    pub(crate) fn schedule_first_dirty_draft_autosave(&self) {
        if self.imp().drafts.autosave_inflight.get() {
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
            dirty_tabs.push(DirtyDraftCandidate {
                draft_id,
                original_path: editor.file_path(),
                dirty_generation: editor.draft_dirty_generation(),
                editor: editor.downgrade(),
                buffer: editor.buffer(),
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
            dirty_tabs.push(DirtyDraftCandidate {
                draft_id,
                original_path: editor.file_path(),
                dirty_generation: editor.draft_dirty_generation(),
                editor: editor.downgrade(),
                buffer: editor.buffer(),
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

        let window = self.clone();
        // Every terminal outcome clears this capture's token before the next
        // candidate starts, preventing stale disposal cancellation.
        let finish_snapshot = move |outcome: buffer_snapshot::BufferSnapshotOutcome| match outcome {
            buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                window.imp().drafts.close_snapshot_cancellation.take();
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
                };
                let window_weak = window.downgrade();
                // Move the only complete body to the worker and admit the next
                // candidate only after this durable write releases it.
                spawn_blocking_then(
                    (),
                    move || {
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
                        window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                    },
                );
            }
            buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                window.imp().drafts.close_snapshot_cancellation.take();
                let mut failures = failures;
                failures.over_limit += 1;
                if let Some(editor) = candidate.editor.upgrade() {
                    Self::show_automatic_recovery_limit(&editor);
                }
                window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
            }
            buffer_snapshot::BufferSnapshotOutcome::Cancelled => {
                window.imp().drafts.close_snapshot_cancellation.take();
                let mut failures = failures;
                failures.snapshot_cancelled += 1;
                window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
            }
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&candidate.buffer) {
            let cancellation = buffer_snapshot::BufferSnapshotCancellation::default();
            *self.imp().drafts.close_snapshot_cancellation.borrow_mut() =
                Some(cancellation.clone());
            buffer_snapshot::snapshot_buffer_text_async_budgeted(
                candidate.buffer,
                automatic_draft_limit(),
                cancellation,
                finish_snapshot,
            );
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
            {
                accepted_entries.push(accepted.entry);
            } else {
                failures.snapshot_cancelled += 1;
            }
        }
        let data_dir = json_store::data_dir();
        let window_weak = self.downgrade();

        spawn_blocking_then(
            (),
            move || {
                if !accepted_entries.is_empty() {
                    draft_service::update_manifest(&data_dir, |manifest| {
                        for entry in accepted_entries {
                            manifest.upsert(entry);
                        }
                    })
                    .map_err(|error| DraftFlushError::Manifest(error.to_string()))?;
                }
                if failures.snapshot_cancelled == 0
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
                }
            },
            move |(), result| {
                if result.is_ok()
                    && let Some(window) = window_weak.upgrade()
                {
                    window.clear_close_discard_drafts();
                }
                on_done(result.map_err(anyhow::Error::from));
            },
        );
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

        let window = self.clone();
        // Every terminal outcome clears this capture's token before the next
        // candidate starts, preventing stale disposal cancellation.
        let finish_snapshot =
            move |outcome: buffer_snapshot::BufferSnapshotOutcome| match outcome {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                    window.imp().drafts.autosave_snapshot_cancellation.take();
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
                    };
                    let window_weak = window.downgrade();
                    window.note_complete_draft_body_admitted();
                    // The worker owns the only complete body and drops it as
                    // soon as the durable write finishes.
                    spawn_blocking_then(
                        (),
                        move || {
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
                    window.imp().drafts.autosave_snapshot_cancellation.take();
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
                buffer_snapshot::BufferSnapshotOutcome::Cancelled => {
                    window.imp().drafts.autosave_snapshot_cancellation.take();
                    let mut failures = failures;
                    failures.snapshot_cancelled += 1;
                    window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                }
            };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&candidate.buffer) {
            let cancellation = buffer_snapshot::BufferSnapshotCancellation::default();
            *self
                .imp()
                .drafts
                .autosave_snapshot_cancellation
                .borrow_mut() = Some(cancellation.clone());
            buffer_snapshot::snapshot_buffer_text_async_budgeted(
                candidate.buffer,
                automatic_draft_limit(),
                cancellation,
                finish_snapshot,
            );
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

        spawn_blocking_then(
            (),
            move || {
                draft_service::update_manifest(&data_dir, |manifest| {
                    for entry in entries {
                        manifest.upsert(entry);
                    }
                })
            },
            move |(), result| {
                if let Some(window) = window_weak.upgrade() {
                    match result {
                        Ok(manifest) => {
                            *window.imp().drafts.manifest.borrow_mut() = manifest;
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
                                {
                                    editor.set_draft_dirty(false);
                                    window.clear_automatic_recovery_limit(&editor);
                                }
                            }
                        }
                        Err(error) => {
                            tracing::warn!("Failed to save draft manifest: {error}");
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
        let rerun = self.imp().drafts.autosave_pending.get();
        self.imp().drafts.autosave_pending.set(false);
        if rerun {
            self.autosave_tick();
        }
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
        if self.imp().drafts.autosave_inflight.get() {
            self.imp().drafts.autosave_pending.set(true);
        }
    }

    /// Whether draft persistence or deferred startup restore blocks readiness.
    pub(crate) fn draft_workflow_blocks_readiness(&self) -> bool {
        self.imp().drafts.autosave_inflight.get()
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

        let data_dir = json_store::data_dir();
        let draft_id = entry.draft_id.clone();
        let editor_weak = editor.downgrade();
        let window_weak = self.downgrade();

        spawn_blocking_then(
            (),
            move || draft_service::resolve_file_draft_restore(&data_dir, &entry),
            move |(), result| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                match result {
                    Ok(FileDraftRestoreResolution::Restore { content }) => {
                        Self::apply_draft(&editor, &content);
                    }
                    Ok(FileDraftRestoreResolution::SkipStale) => {
                        Self::show_stale_draft_skipped(&editor);
                        if let Some(window) = window_weak.upgrade() {
                            window.delete_draft_by_id(&draft_id);
                        }
                    }
                    Ok(FileDraftRestoreResolution::SkipOversized) => {
                        Self::show_oversized_draft_skipped(&editor);
                    }
                    Ok(
                        FileDraftRestoreResolution::SkipUnavailable
                        | FileDraftRestoreResolution::MissingDraft,
                    ) => {}
                    Err(e) => {
                        tracing::error!("Failed to resolve draft for open file: {e}");
                    }
                }
            },
        );
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
        let Some(preloaded) = self.imp().drafts.preloaded.borrow_mut().remove(&draft_id) else {
            return false;
        };
        match preloaded {
            PreloadedDraftRestore::Content(draft_content) => {
                Self::apply_draft(editor, &draft_content);
            }
            PreloadedDraftRestore::SkipStaleFile => {
                Self::show_stale_draft_skipped(editor);
            }
            PreloadedDraftRestore::SkipOversized => {
                Self::show_oversized_draft_skipped(editor);
            }
            PreloadedDraftRestore::LazyAggregateBudget => {
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
        self.imp()
            .drafts
            .manifest
            .borrow_mut()
            .remove_by_id(draft_id);

        let data_dir = json_store::data_dir();
        let draft_id = draft_id.to_string();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                if let Err(e) = draft_service::delete_draft_file(&data_dir, &draft_id) {
                    tracing::warn!("Failed to delete draft file {draft_id}: {e}");
                }
                draft_service::update_manifest(&data_dir, |manifest| {
                    manifest.remove_by_id(&draft_id);
                })
            },
            move |(), result| {
                if let Some(window) = window_weak.upgrade() {
                    match result {
                        Ok(manifest) => {
                            *window.imp().drafts.manifest.borrow_mut() = manifest;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to save manifest after draft deletion: {e}");
                        }
                    }
                }
            },
        );
    }

    /// Allocate a draft ID for a new editor page.
    pub fn assign_draft_id(&self, editor: &LushtextEditorPage) {
        let id = if let Some(ref path) = editor.file_path() {
            draft_service::draft_id_for_path(path)
        } else {
            let counter = self.imp().drafts.next_tab_id.get();
            self.imp().drafts.next_tab_id.set(counter.wrapping_add(1));
            draft_service::draft_id_for_untitled(counter)
        };
        editor.set_draft_id(id);
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

fn release_eager_preloads(preloaded: &mut HashMap<String, PreloadedDraftRestore>) {
    preloaded.retain(|_, restore| matches!(restore, PreloadedDraftRestore::LazyAggregateBudget));
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

    #[test]
    fn cleanup_merge_removes_only_exact_committed_generation() {
        let old = entry("same", 1);
        let newer = entry("same", 2);
        let unrelated = entry("other", 1);
        let mut manifest = DraftManifest {
            drafts: vec![newer.clone(), unrelated.clone()],
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
    fn cleanup_merge_removes_matching_generation_and_preserves_additions() {
        let removed = entry("removed", 1);
        let concurrent = entry("concurrent", 2);
        let mut manifest = DraftManifest {
            drafts: vec![removed.clone(), concurrent.clone()],
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

        release_eager_preloads(&mut preloaded);

        assert_eq!(
            preloaded,
            HashMap::from([(
                "lazy".to_string(),
                PreloadedDraftRestore::LazyAggregateBudget
            )])
        );
    }
}

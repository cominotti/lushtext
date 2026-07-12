// SPDX-License-Identifier: GPL-3.0-or-later

//! Tab-local local-history capture state and automatic snapshot cadence.
//!
//! The window shell owns browse and restore UX, but the editor tab is the
//! right place to track "clean versus modified" transitions and save lifecycle
//! details. Keeping that state here avoids re-deriving it from broader window
//! orchestration and lets automatic capture stay tightly coupled to one buffer.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::model::local_history::LocalHistorySnapshotOrigin;
use crate::services::{json_store, local_history_service};
use crate::ui::buffer_snapshot;

use super::LushtextEditorPage;

/// Default interval between automatic periodic snapshots while a document stays modified.
const DEFAULT_PERIODIC_CAPTURE_INTERVAL_MS: u64 = 5 * 60 * 1000;

/// Automatic snapshots are admitted before owning worker payloads so the
/// unbounded worker FIFO can retain only scalar/weak retry state, never one
/// document-sized string per modified tab.
static AUTOMATIC_HISTORY_CAPTURE_INFLIGHT: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Contended baselines wait as weak/scalar state and are admitted one at a
    /// time when the current automatic capture releases its payload permit.
    static BASELINE_CAPTURE_WAITERS: RefCell<VecDeque<glib::WeakRef<LushtextEditorPage>>> =
        const { RefCell::new(VecDeque::new()) };
}

struct AutomaticHistoryCapturePermit;

impl AutomaticHistoryCapturePermit {
    fn try_acquire() -> Option<Self> {
        AUTOMATIC_HISTORY_CAPTURE_INFLIGHT
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
            .then_some(Self)
    }
}

impl Drop for AutomaticHistoryCapturePermit {
    fn drop(&mut self) {
        AUTOMATIC_HISTORY_CAPTURE_INFLIGHT.store(false, Ordering::Release);
        glib::MainContext::default().invoke(drain_next_baseline_capture_waiter);
    }
}

fn enqueue_baseline_capture_waiter(editor: &LushtextEditorPage) {
    if editor
        .imp()
        .local_history
        .baseline_retry_pending
        .replace(true)
    {
        return;
    }
    BASELINE_CAPTURE_WAITERS.with(|waiters| waiters.borrow_mut().push_back(editor.downgrade()));
}

fn drain_next_baseline_capture_waiter() {
    loop {
        let waiter = BASELINE_CAPTURE_WAITERS.with(|waiters| waiters.borrow_mut().pop_front());
        let Some(waiter) = waiter else {
            return;
        };
        let Some(editor) = waiter.upgrade() else {
            continue;
        };
        editor.imp().local_history.baseline_retry_pending.set(false);
        if editor.is_modified()
            && editor.file_path().is_some()
            && editor
                .live_local_history_availability()
                .allows_automatic_capture()
        {
            editor.capture_local_history_baseline();
            return;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PeriodicCaptureTicket {
    editor_generation: u64,
    path_generation: u64,
    periodic_generation: u32,
    edit_generation: u64,
    path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PeriodicCaptureFacts {
    editor_generation: u64,
    path_generation: u64,
    periodic_generation: u32,
    edit_generation: u64,
    path: Option<PathBuf>,
    modified: bool,
    availability: local_history_service::LocalHistoryAvailability,
}

fn periodic_capture_is_current(
    ticket: &PeriodicCaptureTicket,
    facts: &PeriodicCaptureFacts,
) -> bool {
    facts.editor_generation == ticket.editor_generation
        && facts.path_generation == ticket.path_generation
        && facts.periodic_generation == ticket.periodic_generation
        && facts.edit_generation == ticket.edit_generation
        && facts.path.as_ref() == Some(&ticket.path)
        && facts.modified
        && facts.availability.allows_automatic_capture()
}

fn should_reschedule_periodic_capture(
    modified: bool,
    has_path: bool,
    automatic_capture_suppressed: bool,
) -> bool {
    modified && has_path && !automatic_capture_suppressed
}

impl LushtextEditorPage {
    /// Extend the editor's native context menu with note and local-history actions.
    pub(crate) fn setup_local_history_context_menu(&self) {
        let menu = gio::Menu::new();

        let notes_section = gio::Menu::new();
        notes_section.append(Some("Toggle Bookmark"), Some("win.toggle-bookmark"));
        notes_section.append(Some("Edit Bookmark…"), Some("win.edit-bookmark-label"));
        notes_section.append(Some("Open Document Note…"), Some("win.open-document-note"));
        menu.append_section(None, &notes_section);

        let history_section = gio::Menu::new();
        history_section.append(Some("Local History…"), Some("win.show-local-history"));
        menu.append_section(None, &history_section);

        self.source_view().set_extra_menu(Some(&menu));
    }

    /// Install the tab-local signal tracking used by automatic local history capture.
    pub(crate) fn setup_local_history_tracking(&self) {
        let buffer = self.buffer();
        let editor_weak = self.downgrade();
        let handler_id = buffer.connect_changed(move |_| {
            if let Some(editor) = editor_weak.upgrade() {
                let state = &editor.imp().local_history;
                if let Some(cancellation) = state.periodic_snapshot_cancellation.take() {
                    cancellation.cancel();
                }
                state
                    .edit_generation
                    .set(state.edit_generation.get().wrapping_add(1));
            }
        });
        self.imp()
            .local_history
            .buffer_signals
            .track(&buffer, handler_id);

        let editor_weak = self.downgrade();
        let handler_id = buffer.connect_modified_changed(move |buffer| {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor
                .imp()
                .local_history
                .automatic_capture_suppressed
                .get()
            {
                return;
            }

            if !buffer.is_modified() {
                editor.cancel_local_history_periodic_capture();
                editor.set_local_history_restore_undo_text(None);
                return;
            }

            if editor.file_path().is_none()
                || !editor
                    .live_local_history_availability()
                    .allows_automatic_capture()
            {
                editor.cancel_local_history_periodic_capture();
                return;
            }

            editor.capture_local_history_baseline();
            editor.schedule_local_history_periodic_capture();
        });
        self.imp()
            .local_history
            .buffer_signals
            .track(&buffer, handler_id);
    }

    /// Return the large-file-aware local-history mode for this editor.
    #[must_use]
    pub(crate) fn local_history_availability(
        &self,
    ) -> local_history_service::LocalHistoryAvailability {
        local_history_service::availability_for_size_check(self.size_check())
    }

    fn live_local_history_availability(&self) -> local_history_service::LocalHistoryAvailability {
        local_history_service::availability_for_live_buffer_chars(self.buffer().char_count())
    }

    pub(crate) fn advance_local_history_path_generation(&self) {
        let state = &self.imp().local_history;
        if let Some(cancellation) = state.periodic_snapshot_cancellation.take() {
            cancellation.cancel();
        }
        state
            .path_generation
            .set(state.path_generation.get().wrapping_add(1));
    }

    /// Seed the tab's "last clean text" after a file load or reload completes.
    pub(crate) fn seed_local_history_from_loaded_content(&self, content: &str) {
        let clean_text = if self.file_path().is_some()
            && self
                .live_local_history_availability()
                .allows_automatic_capture()
        {
            Some(content.to_string())
        } else {
            None
        };
        self.imp().local_history.last_clean_text.replace(clean_text);
        self.set_local_history_restore_undo_text(None);
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(false);
        self.cancel_local_history_periodic_capture();
    }

    /// Treat restored draft content as the baseline for future local-history capture.
    pub(crate) fn seed_local_history_from_restored_draft(&self, content: &str) {
        let clean_text = if self.file_path().is_some()
            && self
                .live_local_history_availability()
                .allows_automatic_capture()
        {
            Some(content.to_string())
        } else {
            None
        };
        self.imp().local_history.last_clean_text.replace(clean_text);
        self.set_local_history_restore_undo_text(None);
        self.cancel_local_history_periodic_capture();
    }

    /// Suspend automatic capture while the save workflow toggles the modified flag.
    pub(crate) fn prepare_local_history_for_save(&self) {
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(true);
        self.cancel_local_history_periodic_capture();
    }

    /// Finalize automatic-capture state after a successful save or Save As.
    pub(crate) fn complete_local_history_after_save_success(&self, clean_text: Option<String>) {
        self.imp().local_history.last_clean_text.replace(clean_text);
        self.set_local_history_restore_undo_text(None);
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(false);
        self.cancel_local_history_periodic_capture();
    }

    /// Resume normal capture tracking after a failed save restored the modified flag.
    pub(crate) fn complete_local_history_after_save_failure(&self) {
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(false);
        if self.is_modified()
            && self
                .live_local_history_availability()
                .allows_automatic_capture()
        {
            self.schedule_local_history_periodic_capture();
        }
    }

    /// Replace the editor buffer with history text while suppressing automatic baseline capture.
    pub(crate) fn replace_buffer_with_local_history_text(&self, text: &str) {
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(true);
        self.cancel_local_history_periodic_capture();

        let buffer = self.buffer();
        self.set_minimap_tracking_suspended(true);
        buffer.begin_irreversible_action();
        buffer.set_text(text);
        if self.size_check().undo_enabled() {
            buffer.end_irreversible_action();
        }
        buffer.set_modified(true);

        let start = buffer.start_iter();
        buffer.place_cursor(&start);
        let mark = buffer.create_mark(None, &start, true);
        self.source_view()
            .scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
        buffer.delete_mark(&mark);

        self.set_minimap_tracking_suspended(false);
        self.clear_modified_line_marks();
        self.refresh_minimap();
        self.notify_memory_policy_changed();

        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(false);
        if self
            .live_local_history_availability()
            .allows_automatic_capture()
        {
            self.schedule_local_history_periodic_capture();
        }
    }

    /// Record or clear the one-shot text used by the browser's undo-restore affordance.
    pub(crate) fn set_local_history_restore_undo_text(&self, text: Option<String>) {
        self.imp().local_history.restore_undo_text.replace(text);
    }

    /// Consume the pending undo-restore text after the user activates it.
    #[must_use]
    pub(crate) fn take_local_history_restore_undo_text(&self) -> Option<String> {
        self.imp()
            .local_history
            .restore_undo_text
            .borrow_mut()
            .take()
    }

    fn capture_local_history_baseline(&self) {
        let Some(path) = self.file_path() else {
            return;
        };
        let Some(permit) = AutomaticHistoryCapturePermit::try_acquire() else {
            enqueue_baseline_capture_waiter(self);
            return;
        };
        let Some(clean_text) = self.imp().local_history.last_clean_text.borrow_mut().take() else {
            return;
        };

        spawn_blocking_then(
            (self.downgrade(), permit),
            move || {
                let data_dir = json_store::data_dir();
                local_history_service::capture_snapshot_for_path(
                    &data_dir,
                    &path,
                    &clean_text,
                    LocalHistorySnapshotOrigin::Baseline,
                    local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
                )
            },
            |(_editor_weak, _permit), result| {
                if let Err(error) = result {
                    tracing::warn!("Failed to capture local-history baseline snapshot: {error}");
                }
            },
        );
    }

    fn schedule_local_history_periodic_capture(&self) {
        let generation = self
            .imp()
            .local_history
            .periodic_generation
            .get()
            .wrapping_add(1);
        self.imp().local_history.periodic_generation.set(generation);

        let editor_weak = self.downgrade();
        glib::timeout_add_local_once(local_history_capture_interval(), move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            editor.run_local_history_periodic_capture(generation);
        });
    }

    pub(crate) fn cancel_local_history_periodic_capture(&self) {
        if let Some(cancellation) = self
            .imp()
            .local_history
            .periodic_snapshot_cancellation
            .take()
        {
            cancellation.cancel();
        }
        let generation = self
            .imp()
            .local_history
            .periodic_generation
            .get()
            .wrapping_add(1);
        self.imp().local_history.periodic_generation.set(generation);
    }

    fn run_local_history_periodic_capture(&self, generation: u32) {
        if self.imp().local_history.periodic_generation.get() != generation
            || !self.is_modified()
            || self.file_path().is_none()
        {
            return;
        }

        let buffer = self.buffer();
        if !self
            .live_local_history_availability()
            .allows_automatic_capture()
        {
            self.schedule_local_history_periodic_capture();
            return;
        }
        let Some(path) = self.file_path() else {
            return;
        };
        let Some(permit) = AutomaticHistoryCapturePermit::try_acquire() else {
            self.schedule_local_history_periodic_capture();
            return;
        };
        let state = &self.imp().local_history;
        let ticket = PeriodicCaptureTicket {
            editor_generation: state.editor_generation.get(),
            path_generation: state.path_generation.get(),
            periodic_generation: generation,
            edit_generation: state.edit_generation.get(),
            path,
        };

        let editor_weak = self.downgrade();
        let finish = move |outcome: buffer_snapshot::BufferSnapshotOutcome| {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            editor
                .imp()
                .local_history
                .periodic_snapshot_cancellation
                .borrow_mut()
                .take();
            match outcome {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                    editor.persist_periodic_snapshot_if_current(ticket.clone(), text, permit);
                }
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. }
                | buffer_snapshot::BufferSnapshotOutcome::Cancelled => {
                    drop(permit);
                    editor.reschedule_local_history_after_capture();
                }
            }
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
            let cancellation = buffer_snapshot::BufferSnapshotCancellation::default();
            self.imp()
                .local_history
                .periodic_snapshot_cancellation
                .replace(Some(cancellation.clone()));
            buffer_snapshot::snapshot_buffer_text_async_budgeted(
                buffer,
                buffer_snapshot::BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD,
                cancellation,
                finish,
            );
        } else {
            finish(buffer_snapshot::snapshot_buffer_text_direct_budgeted(
                &buffer,
                buffer_snapshot::BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD,
            ));
        }
    }

    /// Start one periodic capture immediately for GTK lifecycle regression tests.
    #[cfg(feature = "test-utils")]
    pub fn run_local_history_periodic_capture_for_test(&self) {
        let generation = self
            .imp()
            .local_history
            .periodic_generation
            .get()
            .wrapping_add(1);
        self.imp().local_history.periodic_generation.set(generation);
        self.run_local_history_periodic_capture(generation);
    }

    /// Whether a chunked periodic snapshot still owns its cancellation handle.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn local_history_periodic_snapshot_inflight_for_test(&self) -> bool {
        self.imp()
            .local_history
            .periodic_snapshot_cancellation
            .borrow()
            .is_some()
    }

    fn persist_periodic_snapshot_if_current(
        &self,
        ticket: PeriodicCaptureTicket,
        text: String,
        permit: AutomaticHistoryCapturePermit,
    ) {
        if !periodic_capture_is_current(&ticket, &self.periodic_capture_facts()) {
            drop(permit);
            self.reschedule_local_history_after_capture();
            return;
        }

        let path = ticket.path;
        spawn_blocking_then(
            (self.downgrade(), permit),
            move || {
                let data_dir = json_store::data_dir();
                local_history_service::capture_snapshot_for_path(
                    &data_dir,
                    &path,
                    &text,
                    LocalHistorySnapshotOrigin::Periodic,
                    local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
                )
            },
            |(editor_weak, _permit), result| {
                if let Err(error) = result {
                    tracing::warn!("Failed to capture periodic local-history snapshot: {error}");
                }
                if let Some(editor) = editor_weak.upgrade() {
                    editor.reschedule_local_history_after_capture();
                }
            },
        );
    }

    fn reschedule_local_history_after_capture(&self) {
        if should_reschedule_periodic_capture(
            self.is_modified(),
            self.file_path().is_some(),
            self.imp().local_history.automatic_capture_suppressed.get(),
        ) {
            self.schedule_local_history_periodic_capture();
        }
    }

    fn periodic_capture_facts(&self) -> PeriodicCaptureFacts {
        let state = &self.imp().local_history;
        PeriodicCaptureFacts {
            editor_generation: state.editor_generation.get(),
            path_generation: state.path_generation.get(),
            periodic_generation: state.periodic_generation.get(),
            edit_generation: state.edit_generation.get(),
            path: self.file_path(),
            modified: self.is_modified(),
            availability: self.live_local_history_availability(),
        }
    }
}

fn local_history_capture_interval() -> Duration {
    #[cfg(debug_assertions)]
    if let Ok(raw) = std::env::var("LUSHTEXT_LOCAL_HISTORY_INTERVAL_MS")
        && let Ok(parsed) = raw.parse::<u64>()
    {
        return Duration::from_millis(parsed.max(1));
    }

    Duration::from_millis(DEFAULT_PERIODIC_CAPTURE_INTERVAL_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket() -> PeriodicCaptureTicket {
        PeriodicCaptureTicket {
            editor_generation: 3,
            path_generation: 5,
            periodic_generation: 7,
            edit_generation: 11,
            path: PathBuf::from("/workspace/current.md"),
        }
    }

    fn facts() -> PeriodicCaptureFacts {
        let ticket = ticket();
        PeriodicCaptureFacts {
            editor_generation: ticket.editor_generation,
            path_generation: ticket.path_generation,
            periodic_generation: ticket.periodic_generation,
            edit_generation: ticket.edit_generation,
            path: Some(ticket.path),
            modified: true,
            availability: local_history_service::LocalHistoryAvailability::Full,
        }
    }

    #[test]
    fn periodic_capture_accepts_only_the_unchanged_live_editor() {
        assert!(periodic_capture_is_current(&ticket(), &facts()));
    }

    #[test]
    fn periodic_capture_rejects_close_edit_timer_and_identity_changes() {
        let ticket = ticket();
        for mutate in [
            |facts: &mut PeriodicCaptureFacts| facts.editor_generation += 1,
            |facts: &mut PeriodicCaptureFacts| facts.path_generation += 1,
            |facts: &mut PeriodicCaptureFacts| facts.periodic_generation += 1,
            |facts: &mut PeriodicCaptureFacts| facts.edit_generation += 1,
        ] {
            let mut changed = facts();
            mutate(&mut changed);
            assert!(!periodic_capture_is_current(&ticket, &changed));
        }

        let mut renamed = facts();
        renamed.path = Some(PathBuf::from("/workspace/renamed.md"));
        assert!(!periodic_capture_is_current(&ticket, &renamed));

        let mut save_as = facts();
        save_as.path = Some(PathBuf::from("/elsewhere/saved-as.md"));
        assert!(!periodic_capture_is_current(&ticket, &save_as));
    }

    #[test]
    fn periodic_capture_rejects_clean_or_no_longer_full_history_state() {
        let ticket = ticket();
        let mut clean = facts();
        clean.modified = false;
        assert!(!periodic_capture_is_current(&ticket, &clean));

        for availability in [
            local_history_service::LocalHistoryAvailability::SaveOnly,
            local_history_service::LocalHistoryAvailability::Unavailable,
        ] {
            let mut limited = facts();
            limited.availability = availability;
            assert!(!periodic_capture_is_current(&ticket, &limited));
        }
    }

    #[test]
    fn modified_file_backed_editors_reschedule_without_tight_retry() {
        assert!(should_reschedule_periodic_capture(true, true, false));
        assert!(!should_reschedule_periodic_capture(false, true, false));
        assert!(!should_reschedule_periodic_capture(true, false, false));
        assert!(!should_reschedule_periodic_capture(true, true, true));
    }

    #[test]
    fn periodic_capture_admission_allows_only_one_text_payload() {
        let first = AutomaticHistoryCapturePermit::try_acquire().expect("first capture admitted");
        assert!(AutomaticHistoryCapturePermit::try_acquire().is_none());
        drop(first);
        assert!(AutomaticHistoryCapturePermit::try_acquire().is_some());
    }
}

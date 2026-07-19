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
#[cfg(feature = "test-utils")]
use std::sync::atomic::AtomicU64;
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

#[cfg(feature = "test-utils")]
static BASELINE_CAPTURE_FAILURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static BASELINE_CAPTURE_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Fail the next `count` baseline attempts before production persistence runs.
#[cfg(feature = "test-utils")]
pub fn set_local_history_baseline_failures_for_test(count: u64) {
    BASELINE_CAPTURE_FAILURES.store(count, Ordering::Release);
}

/// Delay baseline persistence for deterministic ownership-generation tests.
#[cfg(feature = "test-utils")]
pub fn set_local_history_baseline_delay_for_test(delay_ms: u64) {
    BASELINE_CAPTURE_DELAY_MS.store(delay_ms, Ordering::Release);
}

thread_local! {
    /// Contended baselines wait as weak/scalar state and are admitted one at a
    /// time when the current automatic capture releases its payload permit.
    static BASELINE_CAPTURE_WAITERS: RefCell<VecDeque<glib::WeakRef<LushtextEditorPage>>> =
        const { RefCell::new(VecDeque::new()) };
}

struct AutomaticHistoryCapturePermit;

#[derive(Clone, Debug, PartialEq, Eq)]
struct BaselineCaptureTicket {
    editor_generation: u64,
    path_generation: u64,
    clean_baseline_generation: u64,
    path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BaselineCaptureFacts {
    editor_generation: u64,
    path_generation: u64,
    clean_baseline_generation: u64,
    path: Option<PathBuf>,
    modified: bool,
    baseline_slot_empty: bool,
}

fn baseline_capture_is_current(
    ticket: &BaselineCaptureTicket,
    facts: &BaselineCaptureFacts,
) -> bool {
    facts.editor_generation == ticket.editor_generation
        && facts.path_generation == ticket.path_generation
        && facts.clean_baseline_generation == ticket.clean_baseline_generation
        && facts.path.as_ref() == Some(&ticket.path)
        && facts.modified
        && facts.baseline_slot_empty
}

enum BaselineCaptureOutcome {
    Captured,
    Failed {
        detail: String,
        text: crate::ui::plain_disposal::DisposalOwned<String>,
    },
}

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
                if editor.load_projection_suspended() {
                    return;
                }
                let state = &editor.imp().local_history;
                if let Some(snapshot) = state.periodic_snapshot.borrow().as_ref() {
                    snapshot.cancel();
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
            if editor.load_projection_suspended() {
                return;
            }
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

    pub(super) fn live_local_history_availability(
        &self,
    ) -> local_history_service::LocalHistoryAvailability {
        #[cfg(feature = "test-utils")]
        if let Some(availability) = self.imp().local_history.availability_override.get() {
            return availability;
        }

        local_history_service::availability_for_live_buffer_chars(self.buffer().char_count())
    }

    /// Override live local-history admission without allocating a policy-sized test buffer.
    #[cfg(feature = "test-utils")]
    pub fn set_local_history_availability_for_test(
        &self,
        availability: local_history_service::LocalHistoryAvailability,
    ) {
        self.cancel_local_history_periodic_capture();
        self.imp()
            .local_history
            .availability_override
            .set(Some(availability));
    }

    pub(crate) fn advance_local_history_path_generation(&self) {
        self.cancel_local_history_periodic_capture();
        let state = &self.imp().local_history;
        state
            .path_generation
            .set(state.path_generation.get().wrapping_add(1));
    }

    fn replace_clean_baseline(
        &self,
        text: Option<crate::ui::plain_disposal::DisposalOwned<String>>,
    ) {
        let state = &self.imp().local_history;
        let has_text = text.is_some();
        state.last_clean_text.replace(text);
        state
            .clean_baseline_generation
            .set(state.clean_baseline_generation.get().wrapping_add(1));
        state.baseline_retry_budget.set(u8::from(has_text));
    }

    /// Seed the tab's "last clean text" after a file load or reload completes.
    pub(crate) fn seed_local_history_from_guarded_loaded_content(
        &self,
        content: crate::ui::plain_disposal::DisposalOwned<String>,
    ) {
        let clean_text = if self.file_path().is_some()
            && self
                .live_local_history_availability()
                .allows_automatic_capture()
        {
            Some(content.into_retained_current())
        } else {
            None
        };
        self.replace_clean_baseline(clean_text);
        self.set_local_history_restore_undo_text(None);
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(false);
        self.cancel_local_history_periodic_capture();
    }

    /// Treat restored draft content as the baseline for future local-history capture.
    pub(crate) fn seed_local_history_from_guarded_restored_draft(
        &self,
        content: crate::ui::plain_disposal::DisposalOwned<String>,
    ) {
        let availability = local_history_service::availability_for_utf8_bytes(content.len());
        let clean_text = if self.file_path().is_some() && availability.allows_automatic_capture() {
            Some(content.into_retained_current())
        } else {
            None
        };
        self.replace_clean_baseline(clean_text);
        self.set_local_history_restore_undo_text(None);
        self.cancel_local_history_periodic_capture();
    }

    /// Persist the seeded restored-draft baseline after replacement publication.
    ///
    /// Bounded replacement suppresses intermediate buffer signals. GTK may have
    /// already marked the buffer modified by the terminal callback, so setting
    /// the flag to `true` again is not guaranteed to emit `modified-changed`.
    /// Start the baseline explicitly once the complete draft is visible.
    pub(crate) fn capture_restored_draft_baseline(&self) {
        self.capture_local_history_baseline();
        if self
            .live_local_history_availability()
            .allows_automatic_capture()
        {
            self.schedule_local_history_periodic_capture();
        }
    }

    /// Release document-sized history ownership after terminal memory eviction.
    pub(crate) fn release_local_history_residency_for_eviction(&self) {
        self.replace_clean_baseline(None);
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
    pub(crate) fn complete_local_history_after_save_success(
        &self,
        clean_text: Option<crate::ui::plain_disposal::DisposalOwned<String>>,
    ) {
        self.replace_clean_baseline(clean_text);
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

    /// Publish local-history state after one complete current replacement.
    pub(crate) fn finish_local_history_buffer_replacement(&self) {
        let buffer = self.buffer();
        buffer.set_modified(true);

        let start = buffer.start_iter();
        buffer.place_cursor(&start);
        let mark = buffer.create_mark(None, &start, true);
        self.source_view()
            .scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
        buffer.delete_mark(&mark);

        self.clear_modified_line_marks();
        self.refresh_minimap();
        self.notify_memory_policy_changed();
        if self
            .live_local_history_availability()
            .allows_automatic_capture()
        {
            self.schedule_local_history_periodic_capture();
        }
    }

    /// Record or clear the guarded body used by the browser's undo-restore affordance.
    pub(crate) fn set_local_history_restore_undo_text(
        &self,
        text: Option<crate::ui::plain_disposal::DisposalOwned<String>>,
    ) {
        self.imp().local_history.restore_undo_text.replace(text);
    }

    /// Consume the pending undo-restore body without releasing its worker-drop guard.
    #[must_use]
    pub(crate) fn take_local_history_restore_undo_text(
        &self,
    ) -> Option<crate::ui::plain_disposal::DisposalOwned<String>> {
        self.imp()
            .local_history
            .restore_undo_text
            .borrow_mut()
            .take()
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn has_local_history_restore_undo_for_test(&self) -> bool {
        self.imp()
            .local_history
            .restore_undo_text
            .borrow()
            .is_some()
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
        let state = &self.imp().local_history;
        let ticket = BaselineCaptureTicket {
            editor_generation: state.editor_generation.get(),
            path_generation: state.path_generation.get(),
            clean_baseline_generation: state.clean_baseline_generation.get(),
            path: path.clone(),
        };
        let retry_allowed = state.baseline_retry_budget.get() > 0;

        spawn_blocking_then(
            (self.downgrade(), permit),
            move || {
                delay_baseline_capture_for_test();
                if fail_baseline_capture_for_test() {
                    return BaselineCaptureOutcome::Failed {
                        detail: "injected baseline persistence failure".to_string(),
                        text: clean_text,
                    };
                }
                let data_dir = json_store::data_dir();
                match local_history_service::capture_snapshot_for_path(
                    &data_dir,
                    &path,
                    clean_text.as_str(),
                    LocalHistorySnapshotOrigin::Baseline,
                    local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
                ) {
                    Ok(_) => {
                        drop(clean_text.into_inner_on_worker());
                        BaselineCaptureOutcome::Captured
                    }
                    Err(error) => BaselineCaptureOutcome::Failed {
                        detail: error.to_string(),
                        text: clean_text,
                    },
                }
            },
            move |(editor_weak, _permit), outcome| {
                let BaselineCaptureOutcome::Failed { detail, text } = outcome else {
                    return;
                };
                tracing::warn!("Failed to capture local-history baseline snapshot: {detail}");
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                let state = &editor.imp().local_history;
                let facts = BaselineCaptureFacts {
                    editor_generation: state.editor_generation.get(),
                    path_generation: state.path_generation.get(),
                    clean_baseline_generation: state.clean_baseline_generation.get(),
                    path: editor.file_path(),
                    modified: editor.is_modified(),
                    baseline_slot_empty: state.last_clean_text.borrow().is_none(),
                };
                if !baseline_capture_is_current(&ticket, &facts) {
                    return;
                }
                state.last_clean_text.replace(Some(text));
                if retry_allowed && state.baseline_retry_budget.get() > 0 {
                    state
                        .baseline_retry_budget
                        .set(state.baseline_retry_budget.get() - 1);
                    enqueue_baseline_capture_waiter(&editor);
                }
            },
        );
    }

    /// Start baseline capture immediately through the production admission path.
    #[cfg(feature = "test-utils")]
    pub fn capture_local_history_baseline_for_test(&self) {
        self.capture_local_history_baseline();
    }

    /// Whether failed baseline text is retained without exposing its contents.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn local_history_baseline_candidate_present_for_test(&self) -> bool {
        self.imp().local_history.last_clean_text.borrow().is_some()
    }

    /// Whether this editor owns one weak retry waiter in the global admission FIFO.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn local_history_baseline_retry_pending_for_test(&self) -> bool {
        self.imp().local_history.baseline_retry_pending.get()
    }

    /// Whether one automatic baseline or periodic payload currently owns admission.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn local_history_automatic_capture_inflight_for_test(&self) -> bool {
        AUTOMATIC_HISTORY_CAPTURE_INFLIGHT.load(Ordering::Acquire)
    }

    fn schedule_local_history_periodic_capture(&self) {
        let generation = self
            .imp()
            .local_history
            .periodic_generation
            .get()
            .wrapping_add(1);
        self.imp().local_history.periodic_generation.set(generation);

        let token = self.imp().local_history.periodic_timer.arm(
            self,
            local_history_capture_interval(),
            move |editor, token| {
                if editor.imp().local_history.periodic_timer_token.get() == Some(token) {
                    editor.imp().local_history.periodic_timer_token.set(None);
                }
                editor.run_local_history_periodic_capture(generation);
            },
        );
        self.imp()
            .local_history
            .periodic_timer_token
            .set(Some(token));
    }

    pub(crate) fn cancel_local_history_periodic_capture(&self) {
        if let Some(snapshot) = self.imp().local_history.periodic_snapshot.take() {
            snapshot.dispose();
        }
        let _ = self.imp().local_history.periodic_timer.invalidate();
        self.imp().local_history.periodic_timer_token.set(None);
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
                .periodic_snapshot
                .borrow_mut()
                .take();
            match outcome {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                    editor.persist_periodic_snapshot_if_current(ticket.clone(), text, permit);
                }
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. }
                | buffer_snapshot::BufferSnapshotOutcome::Cancelled(_) => {
                    drop(permit);
                    editor.reschedule_local_history_after_capture();
                }
            }
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
            let snapshot = buffer_snapshot::snapshot_buffer_text_async_budgeted(
                buffer,
                buffer_snapshot::BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD,
                finish,
            );
            self.imp()
                .local_history
                .periodic_snapshot
                .replace(Some(snapshot));
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
        let _ = self.imp().local_history.periodic_timer.invalidate();
        self.imp().local_history.periodic_timer_token.set(None);
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
            .periodic_snapshot
            .borrow()
            .is_some()
    }

    /// Whether the tab currently owns one scheduled periodic timer source.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn local_history_periodic_timer_pending_for_test(&self) -> bool {
        self.imp()
            .local_history
            .periodic_timer_token
            .get()
            .is_some()
    }

    fn persist_periodic_snapshot_if_current(
        &self,
        ticket: PeriodicCaptureTicket,
        text: buffer_snapshot::BufferSnapshotPayload,
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
                let text = text.into_string_on_worker();
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

fn delay_baseline_capture_for_test() {
    #[cfg(feature = "test-utils")]
    std::thread::sleep(Duration::from_millis(
        BASELINE_CAPTURE_DELAY_MS.load(Ordering::Acquire),
    ));
}

fn fail_baseline_capture_for_test() -> bool {
    #[cfg(feature = "test-utils")]
    {
        BASELINE_CAPTURE_FAILURES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
    }

    #[cfg(not(feature = "test-utils"))]
    false
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

    #[test]
    fn failed_baseline_returns_only_to_its_original_cycle() {
        let ticket = BaselineCaptureTicket {
            editor_generation: 3,
            path_generation: 5,
            clean_baseline_generation: 7,
            path: PathBuf::from("/workspace/current.md"),
        };
        let facts = BaselineCaptureFacts {
            editor_generation: 3,
            path_generation: 5,
            clean_baseline_generation: 7,
            path: Some(ticket.path.clone()),
            modified: true,
            baseline_slot_empty: true,
        };
        assert!(baseline_capture_is_current(&ticket, &facts));

        for mutate in [
            |facts: &mut BaselineCaptureFacts| facts.editor_generation += 1,
            |facts: &mut BaselineCaptureFacts| facts.path_generation += 1,
            |facts: &mut BaselineCaptureFacts| facts.clean_baseline_generation += 1,
        ] {
            let mut stale = facts.clone();
            mutate(&mut stale);
            assert!(!baseline_capture_is_current(&ticket, &stale));
        }

        let mut renamed = facts.clone();
        renamed.path = Some(PathBuf::from("/workspace/renamed.md"));
        assert!(!baseline_capture_is_current(&ticket, &renamed));

        let mut newer_baseline = facts;
        newer_baseline.baseline_slot_empty = false;
        assert!(!baseline_capture_is_current(&ticket, &newer_baseline));
    }
}

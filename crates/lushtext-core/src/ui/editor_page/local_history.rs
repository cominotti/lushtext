// SPDX-License-Identifier: GPL-3.0-or-later

//! Tab-local local-history capture state and automatic snapshot cadence.
//!
//! The window shell owns browse and restore UX, but the editor tab is the
//! right place to track "clean versus modified" transitions and save lifecycle
//! details. Keeping that state here avoids re-deriving it from broader window
//! orchestration and lets automatic capture stay tightly coupled to one buffer.

use std::time::Duration;

use gtk4::gio;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::model::local_history::LocalHistorySnapshotOrigin;
use crate::services::{async_task, json_store, local_history_service};

use super::LushtextEditorPage;

/// Default interval between automatic periodic snapshots while a document stays modified.
const DEFAULT_PERIODIC_CAPTURE_INTERVAL_MS: u64 = 5 * 60 * 1000;

impl LushtextEditorPage {
    /// Extend the editor's native context menu with local-history browsing.
    pub(crate) fn setup_local_history_context_menu(&self) {
        let menu = gio::Menu::new();
        menu.append(Some("Local History…"), Some("win.show-local-history"));
        self.source_view().set_extra_menu(Some(&menu));
    }

    /// Install the tab-local signal tracking used by automatic local history capture.
    pub(crate) fn setup_local_history_tracking(&self) {
        let buffer = self.buffer();
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
                    .local_history_availability()
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
            .modified_changed_handler_id
            .replace(Some(handler_id));
    }

    /// Return the large-file-aware local-history mode for this editor.
    #[must_use]
    pub(crate) fn local_history_availability(
        &self,
    ) -> local_history_service::LocalHistoryAvailability {
        local_history_service::availability_for_size_check(self.size_check())
    }

    /// Seed the tab's "last clean text" after a file load or reload completes.
    pub(crate) fn seed_local_history_from_loaded_content(&self, content: &str) {
        let clean_text = if self.file_path().is_some()
            && self.local_history_availability().allows_automatic_capture()
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
            && self.local_history_availability().allows_automatic_capture()
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
        if self.local_history_availability().allows_automatic_capture() {
            self.imp().local_history.last_clean_text.replace(clean_text);
        } else {
            self.imp().local_history.last_clean_text.replace(None);
        }
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
        if self.is_modified() && self.local_history_availability().allows_automatic_capture() {
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
        self.notify_estimated_memory_changed();

        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(false);
        if self.local_history_availability().allows_automatic_capture() {
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
        let Some(clean_text) = self.imp().local_history.last_clean_text.borrow().clone() else {
            return;
        };

        async_task::spawn_blocking_then(
            (),
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
            |(), result| {
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
            || !self.local_history_availability().allows_automatic_capture()
        {
            return;
        }

        // Periodic capture only runs for documents at or below the 10MB
        // "full local history" threshold, so a direct buffer snapshot stays
        // bounded and avoids introducing more complex read-only save-style UI.
        let buffer = self.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        let Some(path) = self.file_path() else {
            return;
        };

        async_task::spawn_blocking_then(
            self.clone(),
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
            |editor, result| {
                if let Err(error) = result {
                    tracing::warn!("Failed to capture periodic local-history snapshot: {error}");
                }
                if editor.is_modified()
                    && editor
                        .local_history_availability()
                        .allows_automatic_capture()
                {
                    editor.schedule_local_history_periodic_capture();
                }
            },
        );
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

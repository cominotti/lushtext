// SPDX-License-Identifier: GPL-3.0-or-later

//! Recent-document Open popover integration for the main window.
//!
//! This window workflow owns when a successful file open should affect recent
//! history. The pure service owns persistence/search rules, and
//! `LushtextOpenPopover` owns only GTK presentation.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::recent_document::RecentDocumentEntry;
use crate::services::json_store;
use crate::services::recent_documents;
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;

const RECENT_DOCUMENTS_SAVE_DEBOUNCE_MS: u64 = 250;

impl LushtextWindow {
    /// Wire Open popover callbacks to the same production workflows as actions.
    pub(super) fn setup_open_popover_callbacks(&self) {
        let window_weak = self.downgrade();
        self.imp()
            .open_popover
            .connect_open_file_requested(move || {
                if let Some(window) = window_weak.upgrade() {
                    window.show_open_file_dialog();
                }
            });

        let window_weak = self.downgrade();
        self.imp()
            .open_popover
            .connect_recent_activated(move |path| {
                if let Some(window) = window_weak.upgrade() {
                    window.open_document(&path);
                    window.focus_selected_editor_after_action();
                }
            });

        let window_weak = self.downgrade();
        self.imp()
            .open_popover
            .connect_remove_requested(move |path| {
                if let Some(window) = window_weak.upgrade() {
                    window.remove_recent_document(&path);
                }
            });

        let window_weak = self.downgrade();
        self.imp()
            .open_popover
            .connect_dismissed_from_keyboard(move || {
                if let Some(window) = window_weak.upgrade() {
                    window.focus_selected_editor_after_action();
                }
            });
    }

    /// Open the recent-document popover and focus its search entry.
    pub(super) fn open_recent_popover(&self) {
        self.rebuild_open_popover_rows();
        self.imp().open_popover.prepare_to_show();
        self.imp().open_menu_button.popup();
    }

    /// Load recent-document persistence without blocking startup.
    pub(super) fn load_recent_documents_async(&self) {
        let state = &self.imp().recent_documents;
        state.loading.set(true);
        state.removed_while_loading.borrow_mut().clear();
        let load_generation = state.generation.get();
        spawn_blocking_then(
            self.clone(),
            move || {
                let data_dir = json_store::data_dir();
                let loaded = recent_documents::load(&data_dir);
                let should_save = loaded.pruned;
                (loaded, should_save)
            },
            move |window, (loaded, should_save)| {
                let state = &window.imp().recent_documents;
                state.loading.set(false);
                let removed_while_loading = state.removed_while_loading.take();
                #[cfg(feature = "test-utils")]
                if state.test_seeded.get() {
                    return;
                }
                for diagnostic in &loaded.diagnostics {
                    tracing::warn!("{diagnostic}");
                }
                let mut loaded_entries = loaded.entries;
                if !removed_while_loading.is_empty() {
                    loaded_entries.retain(|entry| {
                        !removed_while_loading
                            .iter()
                            .any(|path| entry.matches_path(path))
                    });
                }
                let changed_after_load_started = state.generation.get() != load_generation;
                let merged_loaded_rows = if changed_after_load_started {
                    let mut entries = state.entries.borrow_mut();
                    recent_documents::merge_loaded_entries(&mut entries, loaded_entries)
                } else {
                    state.entries.replace(loaded_entries);
                    true
                };
                window.refresh_open_popover_rows();
                if should_save || (changed_after_load_started && merged_loaded_rows) {
                    window.schedule_recent_documents_save();
                }
            },
        );
    }

    /// Record a successful explicit local file-backed open.
    pub(super) fn record_recent_open_for_editor(&self, editor: &LushtextEditorPage, path: &Path) {
        let canonical = editor.canonical_file_path();
        self.record_recent_path(path.to_path_buf(), canonical);
    }

    fn record_recent_path(&self, path: PathBuf, canonical_path: Option<PathBuf>) {
        {
            let mut entries = self.imp().recent_documents.entries.borrow_mut();
            recent_documents::add_or_update(
                &mut entries,
                path,
                canonical_path,
                recent_documents::now_secs(),
            );
        }
        self.refresh_open_popover_rows();
        self.mark_recent_documents_changed();
        self.schedule_recent_documents_save();
    }

    fn remove_recent_document(&self, path: &Path) {
        let state = &self.imp().recent_documents;
        if state.loading.get() {
            state
                .removed_while_loading
                .borrow_mut()
                .push(path.to_path_buf());
        }
        {
            let mut entries = state.entries.borrow_mut();
            recent_documents::remove(&mut entries, path);
        }
        self.refresh_open_popover_rows();
        self.mark_recent_documents_changed();
        self.schedule_recent_documents_save();
    }

    /// Refresh the popover's visible rows after recents or open-tab state changes.
    pub(super) fn refresh_open_popover_rows(&self) {
        if !self.should_rebuild_open_popover_rows() {
            self.imp().recent_documents.rows_dirty.set(true);
            return;
        }
        self.rebuild_open_popover_rows();
    }

    fn should_rebuild_open_popover_rows(&self) -> bool {
        self.imp().open_menu_button.is_active() || self.imp().open_popover.is_visible() || {
            #[cfg(feature = "test-utils")]
            {
                self.imp().recent_documents.test_seeded.get()
            }
            #[cfg(not(feature = "test-utils"))]
            {
                false
            }
        }
    }

    fn rebuild_open_popover_rows(&self) {
        let entries = self.imp().recent_documents.entries.borrow();
        let open_identities = self.imp().open_paths.borrow();
        let rows = recent_documents::visible_rows_for_open_set(
            &entries,
            &open_identities,
            recent_documents::now_secs(),
        );
        self.imp().open_popover.set_recent_rows(rows);
        self.imp().recent_documents.rows_dirty.set(false);
    }

    fn mark_recent_documents_changed(&self) {
        let state = &self.imp().recent_documents;
        state.generation.set(state.generation.get().wrapping_add(1));
    }

    fn schedule_recent_documents_save(&self) {
        self.imp().recent_documents.save_debounce.schedule(
            self,
            Duration::from_millis(RECENT_DOCUMENTS_SAVE_DEBOUNCE_MS),
            |window, _token| {
                window.start_recent_documents_save();
            },
        );
    }

    fn start_recent_documents_save(&self) {
        let state = &self.imp().recent_documents;
        if state.save_inflight.get() {
            state.save_pending.set(true);
            return;
        }
        state.save_inflight.set(true);
        let data_dir = json_store::data_dir();
        let snapshot: Vec<RecentDocumentEntry> =
            self.imp().recent_documents.entries.borrow().clone();
        spawn_blocking_then(
            self.clone(),
            move || recent_documents::save(&data_dir, &snapshot).map_err(|error| error.to_string()),
            |window, result| {
                let state = &window.imp().recent_documents;
                state.save_inflight.set(false);
                if let Err(error) = result {
                    tracing::warn!("failed to save recent documents: {error}");
                }
                if state.save_pending.replace(false) {
                    window.schedule_recent_documents_save();
                }
            },
        );
    }

    /// Seed recent documents for widget tests without disk I/O.
    #[cfg(feature = "test-utils")]
    pub fn set_recent_documents_for_test(&self, entries: Vec<RecentDocumentEntry>) {
        self.imp().recent_documents.test_seeded.set(true);
        self.imp().recent_documents.entries.replace(entries);
        self.refresh_open_popover_rows();
    }

    /// Return current in-memory recent entries for widget assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn recent_documents_for_test(&self) -> Vec<RecentDocumentEntry> {
        self.imp().recent_documents.entries.borrow().clone()
    }
}

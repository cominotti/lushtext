// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace-preview and undo state for the search panel widget.
//!
//! These methods stay on the widget because they mutate GTK state and preview
//! models, but isolating them here keeps the runtime search loop separate from
//! replace/undo behavior.

use crate::model::content_search::generate_replacement_preview;
use crate::services::content_search::ReplaceUndoBackup;
use crate::services::{json_store, search_backup};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};

use super::LushtextSearchPanel;

impl LushtextSearchPanel {
    /// Show the undo button (called after a successful replace).
    pub fn show_undo_button(&self) {
        self.imp().undo_button.set_visible(true);
    }

    /// Hide the undo button.
    pub fn hide_undo_button(&self) {
        self.imp().undo_button.set_visible(false);
    }

    /// Store undo backup and persist it as the current retryable journal.
    pub fn set_undo_backup(&self, backup: &ReplaceUndoBackup) {
        self.set_undo_backup_in_memory(backup);

        let data_dir = json_store::data_dir();
        let Ok(_disk_guard) = undo_backup_disk_lock().lock() else {
            tracing::warn!("Replace undo backup disk lock was poisoned; skipping backup save");
            return;
        };
        if let Err(e) = search_backup::save(&data_dir, backup) {
            tracing::error!("Failed to persist replace backup: {e}");
        }
    }

    /// Store undo backup after the replace service already wrote per-file journal entries.
    pub fn set_persisted_undo_backup(&self, backup: &ReplaceUndoBackup) {
        self.set_undo_backup_in_memory(backup);
    }

    fn set_undo_backup_in_memory(&self, backup: &ReplaceUndoBackup) {
        self.imp()
            .preview
            .undo_backup_generation
            .fetch_add(1, Ordering::AcqRel);
        self.imp().preview.undo_backup.replace(Some(backup.clone()));
    }

    /// Clear stale Replace All journal data left by a prior session.
    pub(crate) fn load_persisted_undo_backup(&self) {
        let data_dir = json_store::data_dir();
        let generation = self
            .imp()
            .preview
            .undo_backup_generation
            .load(Ordering::Acquire);
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        crate::services::async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let _disk_guard = undo_backup_disk_lock()
                    .lock()
                    .map_err(|_| anyhow::anyhow!("replace undo backup disk lock poisoned"))?;
                if generation_counter.load(Ordering::Acquire) != generation {
                    return Ok(());
                }
                search_backup::delete(&data_dir)
            },
            move |_panel, result| match result {
                Ok(()) => {}
                Err(e) => {
                    tracing::warn!("Failed to clear stale replace backup: {e}");
                }
            },
        );
    }

    /// Clear undo backup and hide the undo button.
    pub(crate) fn clear_undo_backup(&self) {
        self.imp()
            .preview
            .undo_backup_generation
            .fetch_add(1, Ordering::AcqRel);
        self.imp().preview.undo_backup.replace(None);
        self.hide_undo_button();

        let data_dir = json_store::data_dir();
        let Ok(_disk_guard) = undo_backup_disk_lock().lock() else {
            tracing::warn!("Replace undo backup disk lock was poisoned; skipping backup cleanup");
            return;
        };
        if let Err(e) = search_backup::delete(&data_dir) {
            tracing::warn!("Failed to delete replace backup after undo: {e}");
        }
    }

    /// Whether the panel is in preview mode.
    #[must_use]
    pub fn is_preview_mode(&self) -> bool {
        self.imp().preview.preview_mode.get()
    }

    /// Enter preview mode: generate replacement previews and switch the results
    /// list to show before/after with checkboxes.
    pub fn enter_preview_mode(&self, replacement_text: &str) {
        let imp = self.imp();

        let search_matches = self.collect_search_matches();
        if search_matches.is_empty() {
            return;
        }

        let query_spec = self.current_query_spec();
        let previews = generate_replacement_preview(
            &search_matches,
            &query_spec.query,
            replacement_text,
            &query_spec.options,
        );

        let all_indices: std::collections::HashSet<usize> = (0..previews.len()).collect();
        imp.preview.checked_indices.replace(all_indices);
        imp.preview.preview_replacements.replace(previews);
        imp.preview.preview_mode.set(true);

        let total = imp.preview.preview_replacements.borrow().len();
        imp.replace_all_button
            .set_label(&format!("Replace {total} of {total}"));
        imp.replace_all_button.set_sensitive(total > 0);

        self.refresh_results_display();
    }

    /// Exit preview mode: clear preview state and restore normal result display.
    pub fn exit_preview_mode(&self) {
        let imp = self.imp();
        imp.preview.preview_mode.set(false);
        imp.preview.preview_replacements.borrow_mut().clear();
        imp.preview.checked_indices.borrow_mut().clear();
        imp.replace_all_button.set_label("Replace All");
        self.update_replace_button_sensitivity();
        self.refresh_results_display();
    }

    /// Update the "Replace All" / "Confirm Replace" button sensitivity.
    pub fn update_replace_button_sensitivity(&self) {
        let imp = self.imp();
        if imp.preview.preview_mode.get() {
            imp.replace_all_button
                .set_sensitive(!imp.preview.checked_indices.borrow().is_empty());
        } else {
            // Empty replacement text is allowed (deletes matches).
            imp.replace_all_button
                .set_sensitive(imp.runtime.total_matches.get() > 0);
        }
    }
}

fn undo_backup_disk_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

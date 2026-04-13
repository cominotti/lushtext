// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace-preview and undo state for the search panel widget.
//!
//! These methods stay on the widget because they mutate GTK state and preview
//! models, but isolating them here keeps the runtime search loop separate from
//! replace/undo behavior.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::content_search::generate_replacement_preview;
use crate::services::{json_store, search_backup};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

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

    /// Store undo backup after a successful replace.
    pub fn set_undo_backup(&self, backup: &HashMap<PathBuf, Vec<u8>>) {
        let generation = self.imp().undo_backup_generation.get().wrapping_add(1);
        self.imp().undo_backup_generation.set(generation);
        self.imp().undo_backup.replace(Some(backup.clone()));

        let data_dir = json_store::data_dir();
        if let Err(e) = search_backup::save(&data_dir, backup) {
            tracing::error!("Failed to persist replace backup: {e}");
            if let Err(delete_err) = search_backup::delete(&data_dir) {
                tracing::warn!(
                    "Failed to clear stale replace backup after save failure: {delete_err}"
                );
            }
        }
    }

    /// Clear undo backup and hide the undo button.
    pub(crate) fn clear_undo_backup(&self) {
        let generation = self.imp().undo_backup_generation.get().wrapping_add(1);
        self.imp().undo_backup_generation.set(generation);
        self.imp().undo_backup.replace(None);
        self.hide_undo_button();

        let data_dir = json_store::data_dir();
        if let Err(e) = search_backup::delete(&data_dir) {
            tracing::warn!("Failed to delete replace backup after undo: {e}");
        }
    }

    /// Delete any stale persisted undo backup from an earlier session.
    pub(crate) fn clear_stale_persisted_undo_backup(&self) {
        let data_dir = json_store::data_dir();
        crate::services::async_task::spawn_blocking_then(
            self.clone(),
            move || search_backup::delete(&data_dir),
            |_panel, result| {
                if let Err(e) = result {
                    tracing::warn!("Failed to clear stale replace backup: {e}");
                }
            },
        );
    }

    /// Whether the panel is in preview mode.
    #[must_use]
    pub fn is_preview_mode(&self) -> bool {
        self.imp().preview_mode.get()
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
        imp.checked_indices.replace(all_indices);
        imp.preview_replacements.replace(previews);
        imp.preview_mode.set(true);

        let total = imp.preview_replacements.borrow().len();
        imp.replace_all_button
            .set_label(&format!("Replace {total} of {total}"));
        imp.replace_all_button.set_sensitive(total > 0);

        self.refresh_results_display();
    }

    /// Exit preview mode: clear preview state and restore normal result display.
    pub fn exit_preview_mode(&self) {
        let imp = self.imp();
        imp.preview_mode.set(false);
        imp.preview_replacements.borrow_mut().clear();
        imp.checked_indices.borrow_mut().clear();
        imp.replace_all_button.set_label("Replace All");
        self.update_replace_button_sensitivity();
        self.refresh_results_display();
    }

    /// Update the "Replace All" / "Confirm Replace" button sensitivity.
    pub fn update_replace_button_sensitivity(&self) {
        let imp = self.imp();
        if imp.preview_mode.get() {
            imp.replace_all_button
                .set_sensitive(!imp.checked_indices.borrow().is_empty());
        } else {
            // Empty replacement text is allowed (deletes matches).
            imp.replace_all_button
                .set_sensitive(imp.total_matches.get() > 0);
        }
    }
}

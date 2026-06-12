// SPDX-License-Identifier: GPL-3.0-or-later

//! Search history and saved-search UI flows for the search panel widget.
//!
//! This keeps persistence and dropdown population separate from the runtime
//! search loop so saved-search maintenance can evolve without touching the
//! streaming search machinery.

use crate::model::content_search::{SavedSearch, SearchHistoryEntry, SearchQuerySpec};
use crate::services::{json_store, saved_searches};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita::prelude::*;

use super::LushtextSearchPanel;

impl LushtextSearchPanel {
    /// Store loaded search history entries.
    pub fn set_search_history(&self, entries: Vec<SearchHistoryEntry>) {
        self.imp().history.history_entries.replace(entries);
    }

    /// Clone the current search history entries.
    #[must_use]
    pub fn search_history(&self) -> Vec<SearchHistoryEntry> {
        self.imp().history.history_entries.borrow().clone()
    }

    /// Store saved search entries loaded from disk.
    pub fn set_saved_searches(&self, entries: Vec<SavedSearch>) {
        self.imp().history.saved_searches.replace(entries);
    }

    /// Clone the current saved search entries.
    #[must_use]
    pub fn saved_searches(&self) -> Vec<SavedSearch> {
        self.imp().history.saved_searches.borrow().clone()
    }

    /// Populate both sections of the dropdown popover.
    pub fn populate_dropdown(&self) {
        let imp = self.imp();

        while let Some(child) = imp.saved_searches_list.first_child() {
            imp.saved_searches_list.remove(&child);
        }
        while let Some(child) = imp.history_list.first_child() {
            imp.history_list.remove(&child);
        }

        let saved = imp.history.saved_searches.borrow();
        let history = imp.history.history_entries.borrow();
        let has_saved = !saved.is_empty();
        let has_history = !history.is_empty();

        imp.saved_header.set_visible(has_saved);
        imp.saved_searches_list.set_visible(has_saved);
        imp.dropdown_separator.set_visible(has_saved && has_history);
        imp.recent_header.set_visible(has_saved && has_history);

        for (idx, entry) in saved.iter().enumerate() {
            let row = libadwaita::ActionRow::new();
            row.set_title(&glib::markup_escape_text(&entry.name));

            let subtitle = entry.row_subtitle();
            if !subtitle.is_empty() {
                row.set_subtitle(&subtitle);
            }

            let delete_btn = gtk4::Button::from_icon_name("edit-delete-symbolic");
            delete_btn.add_css_class("flat");
            delete_btn.set_valign(gtk4::Align::Center);

            let panel_weak = self.downgrade();
            delete_btn.connect_clicked(move |_| {
                if let Some(panel) = panel_weak.upgrade() {
                    panel.remove_saved_search(idx);
                }
            });

            row.add_suffix(&delete_btn);
            imp.saved_searches_list.append(&row);
        }

        for entry in history.iter() {
            let row = libadwaita::ActionRow::new();

            row.set_title(&glib::markup_escape_text(&entry.display_query(60)));

            let subtitle = entry.toggle_summary();
            if !subtitle.is_empty() {
                row.set_subtitle(&subtitle);
            }

            imp.history_list.append(&row);
        }
    }

    /// Restore search state from a saved search and trigger immediate search.
    pub fn restore_from_saved_search(&self, entry: &SavedSearch) {
        self.restore_search_state(&entry.query_spec());
    }

    /// Restore search state from a history entry and trigger immediate search.
    pub fn restore_from_history(&self, entry: &SearchHistoryEntry) {
        self.restore_search_state(&entry.query_spec());
    }

    /// Show the save search dialog. Builds a `SavedSearch` from the current
    /// panel state and persists it to `saved-searches.json`.
    pub fn show_save_search_dialog(&self) {
        let imp = self.imp();
        let query_text = imp.search_entry.text().to_string();
        if query_text.is_empty() {
            return;
        }

        let dialog = libadwaita::AlertDialog::new(
            Some("Save Search"),
            Some("Enter a name for this search."),
        );
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("save", "Save");
        dialog.set_response_appearance("save", libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");

        let name_entry = gtk4::Entry::new();
        name_entry.set_text(&query_text);
        name_entry.set_activates_default(true);
        dialog.set_extra_child(Some(&name_entry));

        let panel = self.clone();
        dialog.choose(
            Some(&*imp.search_entry),
            None::<&gio::Cancellable>,
            move |response| {
                if response != "save" {
                    return;
                }
                let name = name_entry.text().to_string();
                if name.is_empty() {
                    return;
                }

                let display_name = name.clone();
                let entry = SavedSearch::from_spec(name, panel.current_query_spec());

                saved_searches::add(&mut panel.imp().history.saved_searches.borrow_mut(), entry);
                if let Some(ref cb) = *panel.imp().callbacks.message_callback.borrow() {
                    cb(&format!("Search saved as '{display_name}'"));
                }
                // Persist a snapshot so later UI edits do not race this
                // background save.
                let entries_clone = panel.imp().history.saved_searches.borrow().clone();

                let data_dir = json_store::data_dir();
                gtk_lush_tasks::spawn_blocking_then(
                    panel,
                    move || saved_searches::save(&data_dir, &entries_clone),
                    |_panel, result| {
                        if let Err(e) = result {
                            tracing::error!("Failed to save saved searches: {e}");
                        }
                    },
                );
            },
        );
    }

    /// Remove a saved search by index and persist.
    fn remove_saved_search(&self, index: usize) {
        let imp = self.imp();
        saved_searches::remove(&mut imp.history.saved_searches.borrow_mut(), index);
        let entries_clone = imp.history.saved_searches.borrow().clone();

        let data_dir = json_store::data_dir();
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || saved_searches::save(&data_dir, &entries_clone),
            |_panel, result| {
                if let Err(e) = result {
                    tracing::error!("Failed to save saved searches: {e}");
                }
            },
        );

        self.populate_dropdown();
    }

    /// Restore widget state from one query spec and trigger a single direct search.
    fn restore_search_state(&self, spec: &SearchQuerySpec) {
        let imp = self.imp();

        imp.history.restoring_history.set(true);
        imp.search_entry.set_text(&spec.query);
        imp.case_toggle.set_active(spec.options.case_sensitive);
        imp.regex_toggle.set_active(spec.options.regex);
        imp.word_toggle.set_active(spec.options.whole_word);
        imp.gitignore_toggle.set_active(spec.options.gitignore);
        imp.glob_entry
            .set_text(spec.options.glob.as_deref().unwrap_or(""));
        imp.history_popover.popdown();

        imp.history.restoring_history.set(false);
        self.start_search(spec);
    }
}

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
        self.imp().history_entries.replace(entries);
    }

    /// Clone the current search history entries.
    #[must_use]
    pub fn search_history(&self) -> Vec<SearchHistoryEntry> {
        self.imp().history_entries.borrow().clone()
    }

    /// Store saved search entries loaded from disk.
    pub fn set_saved_searches(&self, entries: Vec<SavedSearch>) {
        self.imp().saved_searches.replace(entries);
    }

    /// Clone the current saved search entries.
    #[must_use]
    pub fn saved_searches(&self) -> Vec<SavedSearch> {
        self.imp().saved_searches.borrow().clone()
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

        let saved = imp.saved_searches.borrow();
        let history = imp.history_entries.borrow();
        let has_saved = !saved.is_empty();
        let has_history = !history.is_empty();

        imp.saved_header.set_visible(has_saved);
        imp.saved_searches_list.set_visible(has_saved);
        imp.dropdown_separator.set_visible(has_saved && has_history);
        imp.recent_header.set_visible(has_saved && has_history);

        for (idx, entry) in saved.iter().enumerate() {
            let row = libadwaita::ActionRow::new();
            row.set_title(&glib::markup_escape_text(&entry.name));

            let subtitle = build_saved_toggle_summary(entry);
            if !subtitle.is_empty() {
                row.set_subtitle(&subtitle);
            }

            let delete_btn = gtk4::Button::from_icon_name("edit-delete-symbolic");
            delete_btn.add_css_class("flat");
            delete_btn.set_valign(gtk4::Align::Center);

            let panel = self.clone();
            delete_btn.connect_clicked(move |_| {
                panel.remove_saved_search(idx);
            });

            row.add_suffix(&delete_btn);
            imp.saved_searches_list.append(&row);
        }

        for entry in history.iter() {
            let row = libadwaita::ActionRow::new();

            let title = if entry.query.len() > 60 {
                format!("{}…", &entry.query[..entry.query.floor_char_boundary(60)])
            } else {
                entry.query.clone()
            };
            row.set_title(&glib::markup_escape_text(&title));

            let subtitle = build_toggle_summary(entry);
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

                saved_searches::add(&mut panel.imp().saved_searches.borrow_mut(), entry);
                if let Some(ref cb) = *panel.imp().message_callback.borrow() {
                    cb(&format!("Search saved as '{display_name}'"));
                }
                let entries_clone = panel.imp().saved_searches.borrow().clone();

                let data_dir = json_store::data_dir();
                crate::services::async_task::spawn_blocking_then(
                    panel.clone(),
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
        saved_searches::remove(&mut imp.saved_searches.borrow_mut(), index);
        let entries_clone = imp.saved_searches.borrow().clone();

        let data_dir = json_store::data_dir();
        crate::services::async_task::spawn_blocking_then(
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

        imp.restoring_history.set(true);
        imp.search_entry.set_text(&spec.query);
        imp.case_toggle.set_active(spec.options.case_sensitive);
        imp.regex_toggle.set_active(spec.options.regex);
        imp.word_toggle.set_active(spec.options.whole_word);
        imp.gitignore_toggle.set_active(spec.options.gitignore);
        imp.glob_entry
            .set_text(spec.options.glob.as_deref().unwrap_or(""));
        imp.history_popover.popdown();

        imp.restoring_history.set(false);
        self.start_search(&spec.query);
    }
}

/// Build a compact toggle summary string for a history entry subtitle.
fn build_toggle_summary(entry: &SearchHistoryEntry) -> String {
    build_summary_parts(
        entry.case_sensitive,
        entry.regex,
        entry.whole_word,
        entry.gitignore,
        entry.glob.as_deref(),
    )
}

/// Build a compact toggle summary string for a saved search subtitle.
fn build_saved_toggle_summary(entry: &SavedSearch) -> String {
    let toggles = build_summary_parts(
        entry.case_sensitive,
        entry.regex,
        entry.whole_word,
        entry.gitignore,
        entry.glob.as_deref(),
    );
    let query = if entry.query.len() > 40 {
        format!("{}…", &entry.query[..entry.query.floor_char_boundary(40)])
    } else {
        entry.query.clone()
    };
    if toggles.is_empty() {
        query
    } else {
        format!("{query}  {toggles}")
    }
}

/// Common helper to build toggle summary parts.
fn build_summary_parts(
    case_sensitive: bool,
    regex: bool,
    whole_word: bool,
    gitignore: bool,
    glob: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if case_sensitive {
        parts.push("Aa".to_string());
    }
    if regex {
        parts.push(".*".to_string());
    }
    if whole_word {
        parts.push("W".to_string());
    }
    if !gitignore {
        parts.push("no .gitignore".to_string());
    }
    if let Some(glob) = glob
        && !glob.is_empty()
    {
        parts.push(glob.to_string());
    }
    parts.join("  ")
}

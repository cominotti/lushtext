// SPDX-License-Identifier: GPL-3.0-or-later

//! Search history and saved-search UI flows for the search panel widget.
//!
//! This keeps persistence and dropdown population separate from the runtime
//! search loop so saved-search maintenance can evolve without touching the
//! streaming search machinery.
//!
//! # Role
//!
//! This module carries **no role**. It is a **called presentation surface** of
//! `WFR-SEARCH-REPLACE` — it projects the workflow onto widgets (history and saved-search dropdown projection) — so under
//! `gtk-adapter-module-boundaries` it is outside the five-name role taxonomy,
//! takes none of those names, and owns no `policy.rs` and no `evidence.rs`. Its
//! behavior obligations are unchanged. Named in that workflow's matrix row.

use std::cell::{Cell, RefCell};

use glib::subclass::prelude::{ObjectSubclassExt, ObjectSubclassIsExt};
use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita::prelude::*;
use lushtext_model::workspace::content_search::{SavedSearch, SearchHistoryEntry, SearchQuerySpec};
use lushtext_services::{persistence::json_store, search::saved_searches};

use super::LushtextSearchPanel;

/// Saved-search and recent-history state for the search entry popover.
#[derive(Default)]
pub struct SearchHistoryState {
    /// Guards against spurious searches during construction-time GSettings restore.
    pub constructed_complete: Cell<bool>,
    /// Guards against redundant searches while restoring one saved query spec.
    pub restoring_history: Cell<bool>,
    /// Persisted recent-search entries (most recent first, capped by the service).
    pub history_entries: RefCell<Vec<SearchHistoryEntry>>,
    /// Named saved searches shown in the popover's first section.
    pub saved_searches: RefCell<Vec<SavedSearch>>,
}

pub(super) fn create_history_popover() -> gtk4::Popover {
    let popover = gtk4::Popover::new();
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    crate::ui::accessibility::set_label(&popover, "Saved searches and recent history");
    popover
}

pub(super) fn create_dropdown_heading(title: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(title));
    label.add_css_class("heading");
    label.set_halign(gtk4::Align::Start);
    label.set_margin_start(8);
    label.set_margin_top(6);
    label.set_margin_bottom(4);
    label
}

pub(super) fn create_dropdown_list() -> gtk4::ListBox {
    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    list
}

impl super::imp::LushtextSearchPanel {
    /// Assemble the saved-search/history dropdown popover and parent it to `search_entry`.
    pub(super) fn assemble_history_dropdown(&self) {
        self.dropdown_box.append(&self.saved_header);
        self.dropdown_box.append(&self.saved_searches_list);
        self.dropdown_box.append(&self.dropdown_separator);
        self.dropdown_box.append(&self.recent_header);
        self.dropdown_box.append(&self.history_list);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&self.dropdown_box));
        self.history_popover.set_child(Some(&scroll));
        self.history_popover.set_parent(&*self.search_entry);
    }

    /// Wire focus -> popover show and row-activated -> restore history entry.
    pub(super) fn setup_history(&self) {
        let panel_weak = self.obj().downgrade();
        self.search_entry
            .connect_notify_local(Some("has-focus"), move |entry, _| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let imp = panel.imp();
                // Only show on focus-in (not focus-out), when entries exist,
                // and not during preview mode.
                let has_entries = !imp.history.history_entries.borrow().is_empty()
                    || !imp.history.saved_searches.borrow().is_empty();
                if entry.has_focus() && has_entries && !panel.is_preview_mode() {
                    panel.populate_dropdown();
                    imp.history_popover.popup();
                }
            });

        let panel_weak = self.obj().downgrade();
        self.history_list.connect_row_activated(move |_, row| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            #[expect(
                clippy::cast_sign_loss,
                reason = "GtkListBoxRow indices are non-negative when a row exists"
            )]
            let idx = row.index() as usize;
            let entry = {
                let entries = panel.imp().history.history_entries.borrow();
                entries.get(idx).cloned()
            };
            if let Some(entry) = entry {
                panel.restore_from_history(&entry);
            }
        });

        let panel_weak = self.obj().downgrade();
        self.saved_searches_list
            .connect_row_activated(move |_, row| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "GtkListBoxRow indices are non-negative when a row exists"
                )]
                let idx = row.index() as usize;
                let entry = {
                    let entries = panel.imp().history.saved_searches.borrow();
                    entries.get(idx).cloned()
                };
                if let Some(entry) = entry {
                    panel.restore_from_saved_search(&entry);
                }
            });
    }

    /// Wire the save button to open the save search dialog.
    pub(super) fn setup_save_button(&self) {
        let panel_weak = self.obj().downgrade();
        self.save_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.show_save_search_dialog();
        });
    }
}

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
        crate::ui::entry_paste_cap::cap_single_line_inserts(&name_entry);
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
                let cb = panel.imp().callbacks.message_callback.borrow().clone();
                if let Some(cb) = cb {
                    cb(
                        &format!("Search saved as '{display_name}'"),
                        lushtext_services::coordination::notifications::NotificationSeverity::Info,
                    );
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
        imp.hidden_toggle.set_active(spec.options.hidden);
        imp.glob_entry
            .set_text(spec.options.glob.as_deref().unwrap_or(""));
        imp.history_popover.popdown();

        imp.history.restoring_history.set(false);
        self.start_search(spec);
    }
}

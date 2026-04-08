// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-wide content search panel.
//!
//! Opened via Ctrl+Shift+F, this panel slides up from below the content stack
//! and provides streaming file content search across all workspace roots. Results
//! are grouped by file in a `GtkTreeListModel` with match rows as children.
//!
//! Uses `std::thread::spawn` + `crossbeam_channel::bounded(1024)` for streaming
//! results (not `spawn_blocking_then` which is single-result). The GTK main thread
//! polls the channel via `glib::timeout_add_local` every 50ms.

mod imp;
pub mod item;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use imp::make_display_path;
use item::SearchResultItem;
use libadwaita::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::model::content_search::{
    ContentSearchOptions, Replacement, SavedSearch, SearchEvent, SearchHistoryEntry,
    generate_replacement_preview,
};
use crate::services::{content_search, json_store, saved_searches, search_history};

glib::wrapper! {
    pub struct LushtextSearchPanel(ObjectSubclass<imp::LushtextSearchPanel>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextSearchPanel {
    /// Prepare the panel for display: grab focus on the search entry.
    pub fn open(&self) {
        self.imp().search_entry.grab_focus();
    }

    /// Called when the panel is being hidden. Clears undo backup but preserves
    /// query text and results for next open.
    pub fn close(&self) {
        // Don't cancel the search — preserve results for when the panel reopens.
        // The polling timer is self-managing (stops when Done is received).
        self.clear_undo_backup();
    }

    /// Pre-fill the search entry with text (e.g., editor selection).
    pub fn set_query(&self, text: &str) {
        self.imp().search_entry.set_text(text);
    }

    /// Get the current query text.
    pub fn query(&self) -> String {
        self.imp().search_entry.text().to_string()
    }

    /// Update the workspace roots to search. Called when workspaces change.
    pub fn set_workspace_roots(&self, roots: Vec<PathBuf>) {
        self.imp().workspace_roots.replace(roots);
    }

    /// Register a callback invoked when the user activates a match result.
    pub fn connect_open_file<F: Fn(&Path, u32) + 'static>(&self, f: F) {
        self.imp().open_file_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when the user presses Escape.
    pub fn connect_close_requested<F: Fn() + 'static>(&self, f: F) {
        self.imp()
            .close_requested_callback
            .replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when F4/Shift+F4 navigates to a match.
    pub fn connect_navigate_to_match<F: Fn(&Path, u32) + 'static>(&self, f: F) {
        self.imp().navigate_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback invoked on search progress and completion.
    /// Signature: `(files_searched, is_done)`. `is_done=true` on `SearchEvent::Done`.
    pub fn connect_search_progress<F: Fn(usize, bool) + 'static>(&self, f: F) {
        self.imp().progress_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when "Confirm Replace" is clicked with checked replacements.
    pub fn connect_replace_all<F: Fn(Vec<Replacement>) + 'static>(&self, f: F) {
        self.imp().replace_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when "Undo" is clicked with the backup to restore.
    pub fn connect_undo_all<F: Fn(HashMap<PathBuf, Vec<u8>>) + 'static>(&self, f: F) {
        self.imp().undo_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback for pushing status messages to the window's status bar.
    pub fn connect_message<F: Fn(&str) + 'static>(&self, f: F) {
        self.imp().message_callback.replace(Some(Box::new(f)));
    }

    /// Show the undo button (called after a successful replace).
    pub fn show_undo_button(&self) {
        self.imp().undo_button.set_visible(true);
    }

    /// Hide the undo button.
    pub fn hide_undo_button(&self) {
        self.imp().undo_button.set_visible(false);
    }

    /// Store undo backup after a successful replace.
    pub fn set_undo_backup(&self, backup: HashMap<PathBuf, Vec<u8>>) {
        self.imp().undo_backup.replace(Some(backup));
    }

    /// Take the undo backup (returns and clears it).
    pub fn take_undo_backup(&self) -> Option<HashMap<PathBuf, Vec<u8>>> {
        self.imp().undo_backup.take()
    }

    /// Clear undo backup and hide the undo button.
    fn clear_undo_backup(&self) {
        self.imp().undo_backup.replace(None);
        self.hide_undo_button();
    }

    /// Whether the panel is in preview mode.
    pub fn is_preview_mode(&self) -> bool {
        self.imp().preview_mode.get()
    }

    /// Enter preview mode: generate replacement previews and switch the results
    /// list to show before/after with checkboxes.
    pub fn enter_preview_mode(&self, replacement_text: &str) {
        let imp = self.imp();

        // Gather all SearchMatch data from the flat navigation index + tree model.
        let search_matches = self.collect_search_matches();
        if search_matches.is_empty() {
            return;
        }

        // Build options from current toggle state.
        let options = ContentSearchOptions {
            case_sensitive: imp.case_toggle.is_active(),
            regex: imp.regex_toggle.is_active(),
            whole_word: imp.word_toggle.is_active(),
            gitignore: imp.gitignore_toggle.is_active(),
            glob: {
                let text = imp.glob_entry.text();
                if text.is_empty() {
                    None
                } else {
                    Some(text.to_string())
                }
            },
        };

        let query = imp.search_entry.text().to_string();
        let previews =
            generate_replacement_preview(&search_matches, &query, replacement_text, &options);

        // Initialize all checked.
        let all_indices: std::collections::HashSet<usize> = (0..previews.len()).collect();
        imp.checked_indices.replace(all_indices);
        imp.preview_replacements.replace(previews);
        imp.preview_mode.set(true);

        // Switch button label.
        let total = imp.preview_replacements.borrow().len();
        imp.replace_all_button
            .set_label(&format!("Replace {total} of {total}"));
        imp.replace_all_button.set_sensitive(total > 0);

        // Rebuild the results list to show preview rows.
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

        // Rebuild the results list to show normal rows.
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
            let has_results = imp.total_matches.get() > 0;
            imp.replace_all_button.set_sensitive(has_results);
        }
    }

    /// Whether the panel has any search results.
    pub fn has_results(&self) -> bool {
        self.imp().total_matches.get() > 0
    }

    /// Navigate to the next match (F4). Wraps around at the end.
    pub fn navigate_next_match(&self) {
        let imp = self.imp();
        let positions = imp.match_positions.borrow();
        let len = positions.len();
        if len == 0 {
            return;
        }

        let next = imp
            .current_match_index
            .get()
            .map(|i| (i + 1) % len)
            .unwrap_or(0);
        imp.current_match_index.set(Some(next));

        let (path, line) = positions[next].clone();
        drop(positions); // Release borrow before callbacks.

        self.select_match_in_results(next);

        if let Some(ref cb) = *imp.navigate_callback.borrow() {
            cb(&path, line);
        }
    }

    /// Navigate to the previous match (Shift+F4). Wraps around at the beginning.
    pub fn navigate_prev_match(&self) {
        let imp = self.imp();
        let positions = imp.match_positions.borrow();
        let len = positions.len();
        if len == 0 {
            return;
        }

        let prev = imp
            .current_match_index
            .get()
            .map(|i| if i == 0 { len - 1 } else { i - 1 })
            .unwrap_or(len - 1);
        imp.current_match_index.set(Some(prev));

        let (path, line) = positions[prev].clone();
        drop(positions);

        self.select_match_in_results(prev);

        if let Some(ref cb) = *imp.navigate_callback.borrow() {
            cb(&path, line);
        }
    }

    /// Visually select the match row corresponding to `match_positions[match_index]`
    /// in the `SingleSelection` model, and scroll to make it visible.
    fn select_match_in_results(&self, match_index: usize) {
        let imp = self.imp();
        let positions = imp.match_positions.borrow();
        let Some((target_path, target_line)) = positions.get(match_index) else {
            return;
        };
        let target_path_str = target_path.display().to_string();
        let target_line = *target_line;
        drop(positions);

        let Some(model) = imp.results_list.model() else {
            return;
        };

        // Walk the flat model to find the TreeListRow whose SearchResultItem
        // matches the target path and line number.
        let n = model.n_items();
        for i in 0..n {
            if let Some(obj) = model.item(i)
                && let Some(row) = obj.downcast_ref::<gtk4::TreeListRow>()
                && let Some(item) = row.item().and_downcast::<item::SearchResultItem>()
                && item.is_match_item()
                && item.line_number() == target_line
                && item.file_path() == target_path_str
            {
                if let Some(selection) = model.downcast_ref::<gtk4::SingleSelection>() {
                    selection.set_selected(i);
                }
                imp.results_list
                    .scroll_to(i, gtk4::ListScrollFlags::FOCUS, None);
                break;
            }
        }
    }

    /// Get the search entry widget (for re-invocation focus/selection).
    pub fn search_entry(&self) -> &gtk4::SearchEntry {
        &self.imp().search_entry
    }

    /// Clamp the results scroll area to at most `max_height` pixels.
    /// Called from the window's `size_allocate` with `window_height / 3`
    /// so the search panel never dominates the vertical layout.
    /// Guarded to avoid triggering a re-layout on every allocation (~60Hz).
    pub fn clamp_results_height(&self, max_height: i32) {
        let imp = self.imp();
        let clamped = max_height.max(100); // never below min-content-height
        if imp.results_scroll.max_content_height() != clamped {
            imp.results_scroll.set_max_content_height(clamped);
        }
    }

    /// Collect all `SearchMatch` data from the current results for preview generation.
    /// Uses the unclamped original line content and match range (not the truncated UI values)
    /// so that Replace All produces correct replacements even on long lines.
    fn collect_search_matches(&self) -> Vec<crate::model::content_search::SearchMatch> {
        let imp = self.imp();
        let groups = imp.file_groups.borrow();
        let mut matches = Vec::new();

        for (path, (_, child_store)) in groups.iter() {
            for i in 0..child_store.n_items() {
                if let Some(item) = child_store.item(i).and_downcast::<SearchResultItem>()
                    && item.is_match_item()
                {
                    matches.push(crate::model::content_search::SearchMatch {
                        path: path.clone(),
                        line_number: u64::from(item.line_number()),
                        line_content: item.original_line_content(),
                        match_range: (item.original_match_start() as usize)
                            ..(item.original_match_end() as usize),
                    });
                }
            }
        }

        matches
    }

    /// Trigger a visual refresh of the results list by invalidating the factory.
    /// This causes `connect_bind` to re-fire for all visible rows, which picks up
    /// the `preview_mode` flag and renders accordingly.
    fn refresh_results_display(&self) {
        let imp = self.imp();
        // Force ListView to re-bind all visible rows by resetting the model.
        // This is the simplest way to trigger a full visual refresh when the
        // rendering mode changes (normal vs preview).
        if let Some(model) = imp.results_list.model() {
            imp.results_list.set_model(Some(&model));
        }
    }

    // --- Search history ---

    /// Store loaded search history entries.
    pub fn set_search_history(&self, entries: Vec<SearchHistoryEntry>) {
        self.imp().history_entries.replace(entries);
    }

    /// Clone the current search history entries.
    pub fn search_history(&self) -> Vec<SearchHistoryEntry> {
        self.imp().history_entries.borrow().clone()
    }

    /// Store saved search entries loaded from disk.
    pub fn set_saved_searches(&self, entries: Vec<SavedSearch>) {
        self.imp().saved_searches.replace(entries);
    }

    /// Clone the current saved search entries.
    pub fn saved_searches(&self) -> Vec<SavedSearch> {
        self.imp().saved_searches.borrow().clone()
    }

    /// Populate both sections of the dropdown popover.
    pub fn populate_dropdown(&self) {
        let imp = self.imp();

        // Clear both list boxes.
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

        // Show/hide section headers and separator.
        imp.saved_header.set_visible(has_saved);
        imp.saved_searches_list.set_visible(has_saved);
        imp.dropdown_separator.set_visible(has_saved && has_history);
        imp.recent_header.set_visible(has_saved && has_history);

        // Populate saved searches section.
        for (idx, entry) in saved.iter().enumerate() {
            let row = libadwaita::ActionRow::new();
            row.set_title(&glib::markup_escape_text(&entry.name));

            let subtitle = build_saved_toggle_summary(entry);
            if !subtitle.is_empty() {
                row.set_subtitle(&subtitle);
            }

            // Delete button as suffix.
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

        // Populate recent history section.
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
        let imp = self.imp();
        imp.restoring_history.set(true);

        imp.search_entry.set_text(&entry.query);
        imp.case_toggle.set_active(entry.case_sensitive);
        imp.regex_toggle.set_active(entry.regex);
        imp.word_toggle.set_active(entry.whole_word);
        imp.gitignore_toggle.set_active(entry.gitignore);
        imp.glob_entry.set_text(entry.glob.as_deref().unwrap_or(""));

        imp.history_popover.popdown();

        imp.restoring_history.set(false);
        self.start_search(&entry.query);
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

    /// Restore search state from a history entry and trigger immediate search.
    pub fn restore_from_history(&self, entry: &SearchHistoryEntry) {
        let imp = self.imp();

        // Set the guard to suppress redundant searches during state restoration.
        imp.restoring_history.set(true);

        imp.search_entry.set_text(&entry.query);
        imp.case_toggle.set_active(entry.case_sensitive);
        imp.regex_toggle.set_active(entry.regex);
        imp.word_toggle.set_active(entry.whole_word);
        imp.gitignore_toggle.set_active(entry.gitignore);
        imp.glob_entry.set_text(entry.glob.as_deref().unwrap_or(""));

        imp.history_popover.popdown();

        // Clear the guard and trigger one search directly (bypassing debounce).
        imp.restoring_history.set(false);
        self.start_search(&entry.query);
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

                let imp = panel.imp();
                let display_name = name.clone();
                let entry = SavedSearch {
                    name,
                    query: imp.search_entry.text().to_string(),
                    case_sensitive: imp.case_toggle.is_active(),
                    regex: imp.regex_toggle.is_active(),
                    whole_word: imp.word_toggle.is_active(),
                    gitignore: imp.gitignore_toggle.is_active(),
                    glob: {
                        let text = imp.glob_entry.text();
                        if text.is_empty() {
                            None
                        } else {
                            Some(text.to_string())
                        }
                    },
                };

                saved_searches::add(&mut imp.saved_searches.borrow_mut(), entry);
                if let Some(ref cb) = *imp.message_callback.borrow() {
                    cb(&format!("Search saved as '{display_name}'"));
                }
                let entries_clone = imp.saved_searches.borrow().clone();

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

    /// Start a new search, cancelling any in-flight search first.
    pub fn start_search(&self, query: &str) {
        let imp = self.imp();

        // Cancel previous search.
        if let Some(old_cancel) = imp.cancel_token.take() {
            old_cancel.store(true, Ordering::Relaxed);
        }

        // Clear previous results.
        self.clear_results();

        // Empty query → clear and done.
        if query.is_empty() {
            imp.count_label.set_text("");
            return;
        }

        let roots = imp.workspace_roots.borrow().clone();
        if roots.is_empty() {
            imp.count_label.set_text("No workspace roots");
            return;
        }

        // Set up channel and cancel token.
        let (tx, rx) = crossbeam_channel::bounded(1024);
        let cancel = Arc::new(AtomicBool::new(false));
        imp.cancel_token.replace(Some(cancel.clone()));
        imp.searching.set(true);
        imp.result_capped.set(false);

        // Clone the cancel token for the polling timer BEFORE moving the
        // original into the search thread. This way the timer checks *its own*
        // token, not whatever is currently in imp.cancel_token (which may be
        // replaced by a newer search).
        let timer_cancel = cancel.clone();

        // Hide "No results" and error states.
        imp.error_label.set_visible(false);

        // Spawn search thread.
        let query = query.to_string();
        let options = ContentSearchOptions {
            case_sensitive: imp.case_toggle.is_active(),
            regex: imp.regex_toggle.is_active(),
            whole_word: imp.word_toggle.is_active(),
            gitignore: imp.gitignore_toggle.is_active(),
            glob: {
                let text = imp.glob_entry.text();
                if text.is_empty() {
                    None
                } else {
                    Some(text.to_string())
                }
            },
        };
        std::thread::spawn(move || {
            let root_refs: Vec<&Path> = roots.iter().map(|p| p.as_path()).collect();
            content_search::search(&query, &root_refs, &options, tx, cancel);
        });
        let panel_weak = self.downgrade();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            let Some(panel) = panel_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };

            // If this search was cancelled (new search started), stop immediately.
            if timer_cancel.load(Ordering::Relaxed) {
                return glib::ControlFlow::Break;
            }

            let imp = panel.imp();

            let mut done = false;
            let mut items_this_tick = 0;
            let workspace_roots = imp.workspace_roots.borrow().clone();

            // Drain up to 50 results per tick.
            loop {
                if items_this_tick >= 50 {
                    break;
                }
                match rx.try_recv() {
                    Ok(SearchEvent::Match(m)) => {
                        items_this_tick += 1;
                        let path = m.path.clone();
                        let display = make_display_path(&path, &workspace_roots);

                        let mut groups = imp.file_groups.borrow_mut();
                        let is_new_file = !groups.contains_key(&path);
                        let (file_item, child_store) =
                            groups.entry(path.clone()).or_insert_with(|| {
                                let item = SearchResultItem::new_file(
                                    &path.display().to_string(),
                                    &display,
                                    0,
                                );
                                let store = gtk4::gio::ListStore::new::<SearchResultItem>();
                                (item, store)
                            });

                        // Clone before dropping the borrow — append() and
                        // set_match_count() emit GLib signals that could trigger
                        // the TreeListModel children callback, which calls
                        // file_groups.borrow(). Holding the RefMut would panic.
                        let file_item = file_item.clone();
                        let child_store = child_store.clone();
                        drop(groups);

                        // Preserve the full line content for Replace All (unclamped).
                        let original_line_content = m.line_content.clone();

                        // Truncate very long lines to avoid excessive memory
                        // from minified files (e.g., 10k matches × 1MB line).
                        let truncated_len = if m.line_content.len() > 500 {
                            Some(m.line_content.floor_char_boundary(500))
                        } else {
                            None
                        };
                        let content = if let Some(end) = truncated_len {
                            format!("{}…", &m.line_content[..end])
                        } else {
                            m.line_content
                        };

                        // Clamp match range to the original content boundary (before
                        // ellipsis was appended) so highlights don't land on the "…".
                        let clamp_len = truncated_len.unwrap_or(content.len());
                        let match_start =
                            u32::try_from(m.match_range.start.min(clamp_len)).unwrap_or(u32::MAX);
                        let match_end =
                            u32::try_from(m.match_range.end.min(clamp_len)).unwrap_or(u32::MAX);

                        // Add match to child store.
                        let line_number = u32::try_from(m.line_number).unwrap_or(u32::MAX);
                        let original_match_start =
                            u32::try_from(m.match_range.start).unwrap_or(u32::MAX);
                        let original_match_end =
                            u32::try_from(m.match_range.end).unwrap_or(u32::MAX);
                        let match_item = SearchResultItem::new_match(
                            &m.path.display().to_string(),
                            line_number,
                            &content,
                            match_start,
                            match_end,
                            &original_line_content,
                            original_match_start,
                            original_match_end,
                        );
                        child_store.append(&match_item);

                        // Update match count on file item.
                        file_item.set_match_count(file_item.match_count() + 1);

                        // If this is a new file group, add to root store and expand.
                        if is_new_file {
                            let root_store = &imp.root_store;
                            root_store.append(&file_item);

                            imp.total_files.set(imp.total_files.get() + 1);

                            // Auto-expand: find the TreeListRow and expand it.
                            // The TreeListModel wraps root_store items; we need
                            // to find the row for the item we just appended.
                            let selection_model = imp.results_list.model();
                            if let Some(model) = selection_model {
                                let n = model.n_items();
                                // The newly appended file item is the last root item;
                                // its TreeListRow may not be at position n-1 because
                                // expanded children shift indices. Scan from the end.
                                for i in (0..n).rev() {
                                    if let Some(obj) = model.item(i)
                                        && let Some(row) = obj.downcast_ref::<gtk4::TreeListRow>()
                                        && let Some(ri) =
                                            row.item().and_downcast::<SearchResultItem>()
                                        && ri.file_path() == path.display().to_string()
                                    {
                                        row.set_expanded(true);
                                        break;
                                    }
                                }
                            }
                        }

                        imp.total_matches.set(imp.total_matches.get() + 1);

                        // Append to the flat navigation index for F4/Shift+F4.
                        imp.match_positions.borrow_mut().push((path, line_number));
                    }
                    Ok(SearchEvent::Done) => {
                        done = true;
                        break;
                    }
                    Ok(SearchEvent::ResultCap) => {
                        imp.result_capped.set(true);
                        imp.count_label
                            .set_text("10,000+ results (truncated) \u{2014} narrow your search");
                        imp.count_label.add_css_class("warning");
                    }
                    Ok(SearchEvent::Progress(count)) => {
                        imp.last_progress_count.set(count);
                        if let Some(ref cb) = *imp.progress_callback.borrow() {
                            cb(count, false);
                        }
                    }
                    Ok(SearchEvent::Error(msg)) => {
                        imp.error_label.set_text(&msg);
                        imp.error_label.add_css_class("error");
                        imp.error_label.set_visible(true);
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }

            // Update count label (skip when result cap already set the AC-specified text).
            let total = imp.total_matches.get();
            let files = imp.total_files.get();
            if total > 0 && !imp.result_capped.get() {
                let text = format!("{total} results in {files} files");
                imp.count_label.set_text(&text);
            } else if imp.searching.get() && total == 0 {
                imp.count_label.set_text("Searching…");
            }

            if done {
                imp.searching.set(false);
                if total == 0 {
                    imp.count_label.set_text("No results found");
                }
                if let Some(ref cb) = *imp.progress_callback.borrow() {
                    cb(imp.last_progress_count.get(), true);
                }
                panel.update_replace_button_sensitivity();

                // Show the save button when results exist and not in preview mode.
                if total > 0 && !imp.preview_mode.get() {
                    imp.save_button.set_visible(true);
                }

                // Save to search history if query is non-empty and has results (AC #1).
                let query = imp.search_entry.text().to_string();
                if !query.is_empty() && total > 0 {
                    let new_entry = SearchHistoryEntry {
                        query,
                        case_sensitive: imp.case_toggle.is_active(),
                        regex: imp.regex_toggle.is_active(),
                        whole_word: imp.word_toggle.is_active(),
                        gitignore: imp.gitignore_toggle.is_active(),
                        glob: {
                            let text = imp.glob_entry.text();
                            if text.is_empty() {
                                None
                            } else {
                                Some(text.to_string())
                            }
                        },
                    };
                    let mut entries = imp.history_entries.borrow_mut();
                    search_history::add_entry(&mut entries, new_entry);
                    let entries_clone = entries.clone();
                    drop(entries);

                    let data_dir = json_store::data_dir();
                    crate::services::async_task::spawn_blocking_then(
                        panel.clone(),
                        move || search_history::save(&data_dir, &entries_clone),
                        |_panel, result| {
                            if let Err(e) = result {
                                tracing::error!("Failed to save search history: {e}");
                            }
                        },
                    );
                }

                return glib::ControlFlow::Break;
            }

            glib::ControlFlow::Continue
        });
    }

    /// Clear all results and reset state.
    fn clear_results(&self) {
        let imp = self.imp();
        imp.root_store.remove_all();
        imp.file_groups.borrow_mut().clear();
        imp.total_matches.set(0);
        imp.total_files.set(0);
        imp.result_capped.set(false);
        imp.match_positions.borrow_mut().clear();
        imp.current_match_index.set(None);
        imp.last_progress_count.set(0);
        imp.count_label.set_text("");
        imp.count_label.remove_css_class("warning");
        imp.save_button.set_visible(false);
        imp.error_label.set_visible(false);
        imp.error_label.set_text("");
        imp.error_label.remove_css_class("error");
        // Clear preview and undo state (AC #10: new search clears undo).
        imp.preview_mode.set(false);
        imp.preview_replacements.borrow_mut().clear();
        imp.checked_indices.borrow_mut().clear();
        imp.replace_all_button.set_label("Replace All");
        self.clear_undo_backup();
        self.update_replace_button_sensitivity();
    }
}

/// Build a compact toggle summary string for a history entry subtitle.
/// Example output: `"Aa .* *.rs"` (case-sensitive, regex, glob=*.rs).
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
/// Includes the query text followed by toggle indicators.
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

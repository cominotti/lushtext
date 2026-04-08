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
use gtk4::glib;
use gtk4::prelude::*;
use imp::make_display_path;
use item::SearchResultItem;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::model::content_search::{ContentSearchOptions, SearchEvent};
use crate::services::content_search;

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

    /// Called when the panel is being hidden. Cancels any in-flight search polling
    /// but preserves query text and results for next open.
    pub fn close(&self) {
        // Don't cancel the search — preserve results for when the panel reopens.
        // The polling timer is self-managing (stops when Done is received).
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
                        let match_item = SearchResultItem::new_match(
                            &m.path.display().to_string(),
                            line_number,
                            &content,
                            match_start,
                            match_end,
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
        imp.count_label.set_text("");
        imp.count_label.remove_css_class("warning");
        imp.error_label.set_visible(false);
        imp.error_label.set_text("");
        imp.error_label.remove_css_class("error");
    }
}

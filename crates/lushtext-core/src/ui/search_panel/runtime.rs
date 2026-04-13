// SPDX-License-Identifier: GPL-3.0-or-later

//! Streaming search execution for the search panel widget.
//!
//! This file owns the thread/channel polling loop that translates pure-Rust
//! `SearchEvent` values into GTK list-model updates.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{self, glib};

use crate::model::content_search::{SearchEvent, SearchHistoryEntry};
use crate::services::{content_search, json_store, search_history};

use super::LushtextSearchPanel;
use super::imp::make_display_path;
use super::item::SearchResultItem;

impl LushtextSearchPanel {
    /// Start a new search, cancelling any in-flight search first.
    pub fn start_search(&self, query: &str) {
        let imp = self.imp();

        if let Some(old_cancel) = imp.cancel_token.take() {
            old_cancel.store(true, Ordering::Relaxed);
            if let Some(ref cb) = *imp.progress_callback.borrow() {
                cb(0, true);
            }
        }

        let preserve_feedback = !query.is_empty() && imp.results_feedback_revealer.reveals_child();
        let preserve_results_body = !query.is_empty() && imp.results_body_revealer.reveals_child();
        self.clear_results(preserve_feedback, preserve_results_body);

        if query.is_empty() {
            imp.count_label.set_text("");
            return;
        }

        let roots = imp.workspace_roots.borrow().clone();
        if roots.is_empty() {
            imp.count_label.set_text("No workspace roots");
            self.reveal_results_feedback();
            return;
        }

        let (tx, rx) = crossbeam_channel::bounded(1024);
        let cancel = Arc::new(AtomicBool::new(false));
        let progress_counter = Arc::new(AtomicUsize::new(0));
        let worker_finished = Arc::new(AtomicBool::new(false));
        imp.cancel_token.replace(Some(cancel.clone()));
        imp.searching.set(true);
        imp.result_capped.set(false);

        let timer_cancel = cancel.clone();
        imp.error_label.set_visible(false);

        let mut search_spec = self.current_query_spec();
        search_spec.query = query.to_string();
        let worker_progress_counter = Arc::clone(&progress_counter);
        let worker_finished_for_search = Arc::clone(&worker_finished);
        std::thread::spawn(move || {
            let root_refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
            content_search::search(
                &search_spec.query,
                &root_refs,
                &search_spec.options,
                tx,
                cancel,
                Some(worker_progress_counter),
                Some(worker_finished_for_search),
            );
        });

        let panel_weak = self.downgrade();
        let mut completion_notified = false;
        glib::timeout_add_local(Duration::from_millis(50), move || {
            let Some(panel) = panel_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };

            if timer_cancel.load(Ordering::Relaxed) {
                return glib::ControlFlow::Break;
            }

            let imp = panel.imp();
            let mut done = false;
            let mut items_this_tick = 0;
            let workspace_roots = imp.workspace_roots.borrow().clone();

            const MAX_EVENTS_PER_TICK: usize = 250;

            loop {
                if items_this_tick >= MAX_EVENTS_PER_TICK {
                    break;
                }
                match rx.try_recv() {
                    Ok(SearchEvent::Match(search_match)) => {
                        items_this_tick += 1;
                        append_match_result(&panel, search_match, &workspace_roots);
                    }
                    Ok(SearchEvent::Done) | Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                    Ok(SearchEvent::ResultCap) => {
                        imp.result_capped.set(true);
                        imp.count_label
                            .set_text("10,000+ results (truncated) \u{2014} narrow your search");
                        imp.count_label.add_css_class("warning");
                    }
                    Ok(SearchEvent::Progress(_)) => {}
                    Ok(SearchEvent::Error(msg)) => {
                        imp.error_label.set_text(&msg);
                        imp.error_label.add_css_class("error");
                        imp.error_label.set_visible(true);
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                }
            }

            let files_visited = progress_counter.load(Ordering::Relaxed);
            if !completion_notified && files_visited > imp.last_progress_count.get() {
                imp.last_progress_count.set(files_visited);
                if let Some(ref cb) = *imp.progress_callback.borrow() {
                    cb(files_visited, false);
                }
            }

            if worker_finished.load(Ordering::Relaxed) && !completion_notified {
                completion_notified = true;
                imp.searching.set(false);
                if files_visited > imp.last_progress_count.get() {
                    imp.last_progress_count.set(files_visited);
                }
                if let Some(ref cb) = *imp.progress_callback.borrow() {
                    cb(imp.last_progress_count.get(), true);
                }
            }

            let total = imp.total_matches.get();
            let files = imp.total_files.get();
            if total > 0 && !imp.result_capped.get() {
                imp.count_label
                    .set_text(&format!("{total} results in {files} files"));
            } else if !completion_notified && imp.searching.get() && total == 0 {
                imp.count_label.set_text("Searching\u{2026}");
            }

            if done {
                if !completion_notified {
                    completion_notified = true;
                    imp.searching.set(false);
                    let files_visited = progress_counter.load(Ordering::Relaxed);
                    if files_visited > imp.last_progress_count.get() {
                        imp.last_progress_count.set(files_visited);
                    }
                    if let Some(ref cb) = *imp.progress_callback.borrow() {
                        cb(imp.last_progress_count.get(), true);
                    }
                }
                if total == 0 {
                    imp.count_label.set_text("No results found");
                    panel.reveal_results_feedback();
                }
                panel.update_replace_button_sensitivity();

                if total > 0 && !imp.preview_mode.get() {
                    imp.save_button.set_visible(true);
                }

                if total > 0 {
                    persist_search_history(&panel);
                }

                return glib::ControlFlow::Break;
            }

            glib::ControlFlow::Continue
        });
    }

    /// Clear all results and reset state.
    fn clear_results(&self, preserve_feedback: bool, preserve_results_body: bool) {
        let imp = self.imp();
        if preserve_feedback {
            imp.results_feedback_revealer.set_reveal_child(true);
            imp.results_body_revealer
                .set_reveal_child(preserve_results_body);
        } else {
            self.hide_results_feedback();
        }
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
        imp.preview_mode.set(false);
        imp.preview_replacements.borrow_mut().clear();
        imp.checked_indices.borrow_mut().clear();
        imp.replace_all_button.set_label("Replace All");
        self.clear_undo_backup();
        self.update_replace_button_sensitivity();
    }
}

/// Append one streamed match into the grouped file model and flat navigation index.
fn append_match_result(
    panel: &LushtextSearchPanel,
    search_match: crate::model::content_search::SearchMatch,
    workspace_roots: &[PathBuf],
) {
    let imp = panel.imp();
    let path = search_match.path.clone();
    let display = make_display_path(&path, workspace_roots);

    let mut groups = imp.file_groups.borrow_mut();
    let is_new_file = !groups.contains_key(&path);
    let (file_item, child_store) = groups.entry(path.clone()).or_insert_with(|| {
        let item = SearchResultItem::new_file(&path.display().to_string(), &display, 0);
        let store = gtk4::gio::ListStore::new::<SearchResultItem>();
        (item, store)
    });

    // Clone before dropping the borrow — append() and set_match_count() emit
    // GLib signals that can re-enter the file-groups map.
    let file_item = file_item.clone();
    let child_store = child_store.clone();
    drop(groups);

    let original_line_content = search_match.line_content.clone();
    let truncated_len = if search_match.line_content.len() > 500 {
        Some(search_match.line_content.floor_char_boundary(500))
    } else {
        None
    };
    let content = if let Some(end) = truncated_len {
        format!("{}…", &search_match.line_content[..end])
    } else {
        search_match.line_content
    };

    let clamp_len = truncated_len.unwrap_or(content.len());
    let match_start =
        u32::try_from(search_match.match_range.start.min(clamp_len)).unwrap_or(u32::MAX);
    let match_end = u32::try_from(search_match.match_range.end.min(clamp_len)).unwrap_or(u32::MAX);
    let line_number = u32::try_from(search_match.line_number).unwrap_or(u32::MAX);
    let original_match_start = u32::try_from(search_match.match_range.start).unwrap_or(u32::MAX);
    let original_match_end = u32::try_from(search_match.match_range.end).unwrap_or(u32::MAX);

    let match_item = SearchResultItem::new_match(
        &search_match.path.display().to_string(),
        line_number,
        &content,
        match_start,
        match_end,
        &original_line_content,
        original_match_start,
        original_match_end,
    );
    child_store.append(&match_item);

    file_item.set_match_count(file_item.match_count() + 1);

    if is_new_file {
        imp.root_store.append(&file_item);
        imp.total_files.set(imp.total_files.get() + 1);

        if let Some(model) = imp.results_list.model() {
            for i in (0..model.n_items()).rev() {
                if let Some(obj) = model.item(i)
                    && let Some(row) = obj.downcast_ref::<gtk4::TreeListRow>()
                    && let Some(item) = row.item().and_downcast::<SearchResultItem>()
                    && item.file_path() == path.display().to_string()
                {
                    row.set_expanded(true);
                    break;
                }
            }
        }
    }

    if imp.total_matches.get() == 0 {
        panel.reveal_results_body();
    }
    imp.total_matches.set(imp.total_matches.get() + 1);
    imp.match_positions.borrow_mut().push((path, line_number));
}

/// Persist the latest successful query into the recent-history file.
fn persist_search_history(panel: &LushtextSearchPanel) {
    let query_spec = panel.current_query_spec();
    if query_spec.query.is_empty() {
        return;
    }

    let mut entries = panel.imp().history_entries.borrow_mut();
    search_history::add_entry(&mut entries, SearchHistoryEntry::from_spec(query_spec));
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! Streaming execution of one workspace content search.
//!
//! This module owns the coordination job that runs between "the user asked for
//! results" and "the panel has them": single-flight admission of the latest
//! query, the worker thread that walks the workspace, and the paced GTK turn
//! that translates pure-Rust `SearchEvent` values into list-model rows.
//!
//! Control inversion: [`LushtextSearchPanel::start_search`] returns as soon as
//! the worker thread and the 50 ms poll timer are armed. Work resumes in that
//! timer callback, once per tick, bounded by `MAX_SEARCH_EVENTS_PER_TICK`
//! events. The terminal tick is where the flight finishes and any retained
//! latest query is admitted, so the workflow can loop back into this module
//! without returning to the facade.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{self, glib};

use crate::model::content_search::{SearchEvent, SearchHistoryEntry, SearchQuerySpec};
use crate::model::workspace_search::WorkspaceSearchTraversalPlan;
use crate::services::filesystem::metadata as fs_metadata;
use crate::services::{content_search, json_store, search_history};

use super::LushtextSearchPanel;
use super::imp::make_display_path;
use super::item::SearchResultItem;
use super::policy::{WorkspaceSearchRequest, WorkspaceSearchStart, WorkspaceSearchSubmission};
use super::{SearchFileGroup, SearchMatchLocation, SearchProgressUpdate};

/// Maximum channel events received and dispatched by one scheduled GTK turn.
const MAX_SEARCH_EVENTS_PER_TICK: usize = 250;

enum SearchEventPoll {
    Event(SearchEvent),
    Empty,
    Disconnected,
    BudgetExhausted,
}

fn receive_search_event(
    receiver: &crossbeam_channel::Receiver<SearchEvent>,
    received: &mut usize,
) -> SearchEventPoll {
    if *received >= MAX_SEARCH_EVENTS_PER_TICK {
        return SearchEventPoll::BudgetExhausted;
    }
    match receiver.try_recv() {
        Ok(event) => {
            *received = (*received).saturating_add(1);
            SearchEventPoll::Event(event)
        }
        Err(crossbeam_channel::TryRecvError::Empty) => SearchEventPoll::Empty,
        Err(crossbeam_channel::TryRecvError::Disconnected) => SearchEventPoll::Disconnected,
    }
}

impl LushtextSearchPanel {
    /// Start a new search from one immutable query snapshot, cancelling any
    /// active worker and retaining only the latest compact superseding request.
    pub fn start_search(&self, spec: &SearchQuerySpec) {
        let imp = self.imp();

        if !spec.query.is_empty() && self.result_retirement_saturated() {
            imp.runtime.deferred_search.replace(Some(spec.clone()));
            imp.runtime.flight.borrow_mut().clear_pending();
            self.cancel_active_token();
            return;
        }

        let preserve_feedback =
            !spec.query.is_empty() && imp.results_feedback_revealer.reveals_child();
        let preserve_results_body =
            !spec.query.is_empty() && imp.results_body_revealer.reveals_child();
        self.clear_results(preserve_feedback, preserve_results_body);

        if spec.query.is_empty() {
            self.cancel_active_search();
            imp.count_label.set_text("");
            self.refresh_accessibility_state();
            return;
        }

        let folders = Arc::clone(&imp.runtime.workspace_folders.borrow());
        if folders.is_empty() {
            self.cancel_active_search();
            imp.count_label.set_text("No workspace folders");
            self.reveal_results_feedback();
            self.refresh_accessibility_state();
            return;
        }

        let request = WorkspaceSearchRequest {
            spec: spec.clone(),
            folders,
        };
        let submission = imp.runtime.flight.borrow_mut().submit(request);
        match submission {
            WorkspaceSearchSubmission::Start(start) => self.spawn_search_request(start),
            WorkspaceSearchSubmission::Supersede { .. } => self.cancel_active_token(),
        }
    }

    /// Cancel all pending intent and let the active worker disconnect precisely.
    pub(super) fn cancel_active_search(&self) {
        let imp = self.imp();
        imp.runtime.deferred_search.take();
        imp.runtime.flight.borrow_mut().clear_pending();
        self.cancel_active_token();
    }

    /// Signal the one active worker without changing compact pending ownership.
    fn cancel_active_token(&self) {
        let imp = self.imp();
        if let Some(cancel) = imp.runtime.cancel_token.borrow().as_ref()
            && !cancel.swap(true, Ordering::AcqRel)
        {
            let files_searched = imp.runtime.last_progress_count.get();
            if let Some(ref cb) = *imp.callbacks.progress_callback.borrow() {
                cb(SearchProgressUpdate::Cancelled { files_searched });
            }
        }
        if imp.runtime.flight.borrow().snapshot().active == 0 {
            imp.runtime.cancel_token.take();
            imp.runtime.active_worker_groups.set(0);
        }
        imp.runtime
            .searching
            .set(imp.runtime.cancel_token.borrow().is_some());
        self.refresh_accessibility_state();
    }

    /// Launch one request only after the previous controller/walker disconnected.
    fn spawn_search_request(&self, start: WorkspaceSearchStart) {
        let imp = self.imp();
        debug_assert!(imp.runtime.cancel_token.borrow().is_none());
        let WorkspaceSearchStart {
            generation,
            request,
        } = start;
        imp.runtime.active_worker_groups.set(1);
        imp.runtime
            .active_worker_groups_high_water
            .set(imp.runtime.active_worker_groups_high_water.get().max(1));
        let WorkspaceSearchRequest { spec, folders } = request;

        // A bounded channel gives the worker backpressure when GTK is busy
        // rendering results, instead of letting a huge search allocate
        // unbounded match rows before the main loop can catch up.
        let (tx, rx) = crossbeam_channel::bounded(1024);
        let (plan_tx, plan_rx) = crossbeam_channel::bounded(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let progress_counter = Arc::new(AtomicUsize::new(0));
        imp.runtime.cancel_token.replace(Some(cancel.clone()));
        imp.runtime.searching.set(true);
        imp.runtime.result_capped.set(false);
        self.refresh_accessibility_state();
        if let Some(ref cb) = *imp.callbacks.progress_callback.borrow() {
            cb(SearchProgressUpdate::Started);
        }

        let timer_cancel = cancel.clone();
        imp.error_label.set_visible(false);

        let history_spec = spec.clone();
        let worker_spec = spec;
        let worker_folders = Arc::clone(&folders);
        let worker_progress_counter = Arc::clone(&progress_counter);
        let worker_finished_for_search = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || {
            #[cfg(feature = "test-utils")]
            super::test_policy::delay_search_worker();
            let plan = Arc::new(WorkspaceSearchTraversalPlan::build(
                worker_folders.iter().cloned(),
                fs_metadata::canonical_path,
            ));
            let _ = plan_tx.send(Arc::clone(&plan));
            content_search::search_with_plan(
                &worker_spec.query,
                &plan,
                &worker_spec.options,
                tx,
                cancel,
                Some(worker_progress_counter),
                Some(worker_finished_for_search),
            );
        });

        let panel_weak = self.downgrade();
        let mut completion_notified = false;
        let mut traversal_plan: Option<Arc<WorkspaceSearchTraversalPlan>> = None;
        let mut search_incomplete = false;
        // Poll at UI cadence instead of waking GTK for every worker event; the
        // per-tick cap below keeps input and redraws responsive on noisy searches.
        glib::timeout_add_local(Duration::from_millis(50), move || {
            let Some(panel) = panel_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };

            let imp = panel.imp();
            if imp.runtime.flight.borrow().snapshot().active_generation != Some(generation) {
                return glib::ControlFlow::Break;
            }
            let cancelled = timer_cancel.load(Ordering::Acquire);
            if traversal_plan.is_none()
                && let Ok(plan) = plan_rx.try_recv()
            {
                traversal_plan = Some(plan);
            }
            let mut done = false;
            let mut items_this_tick = 0;
            // Keep each GTK tick bounded so a large streaming search cannot
            // monopolize the main loop while thousands of matches arrive. The
            // 250-event cap drains bursts quickly without starving input and
            // frame work on slower machines.
            loop {
                match receive_search_event(&rx, &mut items_this_tick) {
                    SearchEventPoll::Event(SearchEvent::Match(search_match)) => {
                        if !cancelled {
                            append_match_result(
                                &panel,
                                search_match,
                                traversal_plan.as_deref(),
                                &folders,
                            );
                        }
                    }
                    SearchEventPoll::Event(SearchEvent::Done) | SearchEventPoll::Disconnected => {
                        done = true;
                        break;
                    }
                    SearchEventPoll::Event(SearchEvent::ResultCap) => {
                        if !cancelled {
                            imp.runtime.result_capped.set(true);
                            imp.count_label.set_text(
                                "10,000+ results (truncated) \u{2014} narrow your search",
                            );
                            imp.count_label.add_css_class("warning");
                            panel.refresh_accessibility_state();
                        }
                    }
                    SearchEventPoll::Event(
                        SearchEvent::Progress(_) | SearchEvent::TraversalMetrics(_),
                    ) => {}
                    SearchEventPoll::Event(SearchEvent::Error(msg)) => {
                        if !cancelled {
                            imp.error_label.set_text(&msg);
                            imp.error_label.add_css_class("error");
                            imp.error_label.set_visible(true);
                            panel.refresh_accessibility_state();
                        }
                    }
                    SearchEventPoll::Event(SearchEvent::Incomplete(_reason)) => {
                        if !cancelled {
                            search_incomplete = true;
                            imp.count_label.set_text(
                                "Search incomplete — overlapping workspace paths reached the identity safety limit",
                            );
                            imp.count_label.add_css_class("warning");
                            panel.reveal_results_feedback();
                            panel.refresh_accessibility_state();
                        }
                    }
                    SearchEventPoll::Empty | SearchEventPoll::BudgetExhausted => break,
                }
            }

            let files_visited = progress_counter.load(Ordering::Relaxed);
            if !completion_notified && files_visited > imp.runtime.last_progress_count.get() {
                imp.runtime.last_progress_count.set(files_visited);
                if let Some(ref cb) = *imp.callbacks.progress_callback.borrow() {
                    cb(SearchProgressUpdate::Progress {
                        files_searched: files_visited,
                    });
                }
            }

            let total = imp.runtime.total_matches.get();
            let files = imp.runtime.total_files.get();
            if !cancelled && total > 0 && !imp.runtime.result_capped.get() && !search_incomplete {
                imp.count_label
                    .set_text(&format!("{total} results in {files} files"));
                panel.refresh_accessibility_state();
            } else if !cancelled
                && !completion_notified
                && imp.runtime.searching.get()
                && total == 0
            {
                imp.count_label.set_text("Searching\u{2026}");
                panel.refresh_accessibility_state();
            }

            if done {
                if !cancelled && !completion_notified {
                    completion_notified = true;
                    imp.runtime.searching.set(false);
                    panel.refresh_accessibility_state();
                    let files_visited = progress_counter.load(Ordering::Relaxed);
                    if files_visited > imp.runtime.last_progress_count.get() {
                        imp.runtime.last_progress_count.set(files_visited);
                    }
                    if let Some(ref cb) = *imp.callbacks.progress_callback.borrow() {
                        cb(SearchProgressUpdate::Done {
                            files_searched: imp.runtime.last_progress_count.get(),
                        });
                    }
                }
                if !cancelled && total == 0 && !search_incomplete {
                    imp.count_label.set_text("No results found");
                    panel.reveal_results_feedback();
                    panel.refresh_accessibility_state();
                }
                if !cancelled {
                    panel.update_replace_button_sensitivity();
                    let accepted = std::mem::take(&mut *imp.runtime.search_matches.borrow_mut());
                    imp.runtime
                        .accepted_matches
                        .replace(Some(Arc::new(accepted)));
                }

                if !cancelled && total > 0 && !imp.preview.preview_mode.get() {
                    imp.save_button.set_visible(true);
                }

                if !cancelled && total > 0 {
                    persist_search_history(&panel, &history_spec);
                }

                imp.runtime.cancel_token.take();
                imp.runtime.active_worker_groups.set(0);
                let next = imp.runtime.flight.borrow_mut().finish(generation);
                if let Some(next) = next {
                    if panel.current_query_spec() == next.request.spec
                        && *imp.runtime.workspace_folders.borrow() == next.request.folders
                    {
                        panel.spawn_search_request(next);
                    } else {
                        imp.runtime.flight.borrow_mut().finish(next.generation);
                        imp.runtime.searching.set(false);
                        panel.refresh_accessibility_state();
                    }
                } else {
                    imp.runtime.searching.set(false);
                    panel.refresh_accessibility_state();
                }

                return glib::ControlFlow::Break;
            }

            glib::ControlFlow::Continue
        });
    }

    /// Clear all results and reset state.
    pub(super) fn clear_results(&self, preserve_feedback: bool, preserve_results_body: bool) {
        let imp = self.imp();
        if preserve_feedback {
            imp.results_feedback_revealer.set_reveal_child(true);
            imp.results_body_revealer
                .set_reveal_child(preserve_results_body);
        } else {
            self.hide_results_feedback();
        }
        let preview_outcome = imp.preview.preview_outcome.take();
        let checked_match_ids = std::mem::take(&mut *imp.preview.checked_match_ids.borrow_mut());
        self.detach_visible_results();
        self.release_superseded_preview(preview_outcome, checked_match_ids);
        imp.runtime.total_matches.set(0);
        imp.runtime.total_files.set(0);
        imp.runtime.result_capped.set(false);
        imp.navigation.current_match_index.set(None);
        imp.runtime.last_progress_count.set(0);
        imp.count_label.set_text("");
        imp.count_label.remove_css_class("warning");
        imp.save_button.set_visible(false);
        imp.error_label.set_visible(false);
        imp.error_label.set_text("");
        imp.error_label.remove_css_class("error");
        self.invalidate_active_preview();
        imp.preview.preview_pending.set(false);
        imp.preview.preview_mode.set(false);
        imp.replace_all_button.set_label("Replace All");
        self.update_replace_button_sensitivity();
        self.refresh_accessibility_state();
    }
}

/// Append one streamed match into the grouped file model and flat navigation index.
fn append_match_result(
    panel: &LushtextSearchPanel,
    search_match: crate::model::content_search::SearchMatch,
    traversal_plan: Option<&WorkspaceSearchTraversalPlan>,
    workspace_folders: &[PathBuf],
) {
    let imp = panel.imp();
    let path = search_match.path.clone();
    let display = traversal_plan
        .and_then(|plan| plan.display_relative_path(search_match.traversal_root_index, &path))
        .map_or_else(
            || make_display_path(&path, workspace_folders),
            |relative| relative.display().to_string(),
        );
    let match_id = crate::model::content_search::SearchMatchId::from_index(
        imp.runtime.search_matches.borrow().len(),
    );
    let search_match = search_match.with_id(match_id);
    imp.runtime
        .search_matches
        .borrow_mut()
        .push(search_match.clone());

    let mut groups = imp.runtime.file_groups.borrow_mut();
    let is_new_file = !groups.contains_key(&path);
    let group = groups.entry(path.clone()).or_insert_with(|| {
        SearchFileGroup::new(
            SearchResultItem::new_file(&path.display().to_string(), &display, 0),
            gtk4::gio::ListStore::new::<SearchResultItem>(),
        )
    });

    // Clone before dropping the borrow — append() and set_match_count() emit
    // GLib signals that can re-enter the file-groups map.
    let file_item = group.header_item.clone();
    let child_store = group.child_store.clone();
    drop(groups);

    let truncated_len = if search_match.line_content.len() > 500 {
        Some(search_match.line_content.floor_char_boundary(500))
    } else {
        None
    };
    let content = if let Some(end) = truncated_len {
        format!("{}…", &search_match.line_content[..end])
    } else {
        search_match.line_content.to_string()
    };

    let clamp_len = truncated_len.unwrap_or(content.len());
    let match_start =
        u32::try_from(search_match.match_range.start.min(clamp_len)).unwrap_or(u32::MAX);
    let match_end = u32::try_from(search_match.match_range.end.min(clamp_len)).unwrap_or(u32::MAX);
    let line_number = u32::try_from(search_match.line_number).unwrap_or(u32::MAX);
    let match_item = SearchResultItem::new_match(
        &search_match.path.display().to_string(),
        line_number,
        &content,
        match_start,
        match_end,
        match_id,
    );
    let child_position = child_store.n_items();
    child_store.append(&match_item);

    file_item.set_match_count(file_item.match_count() + 1);

    let mut file_row = None;
    if is_new_file {
        imp.runtime.root_store.borrow().append(&file_item);
        imp.runtime
            .total_files
            .set(imp.runtime.total_files.get() + 1);

        // The new file row is appended after every existing visible root and
        // expanded child. Expand that final row directly instead of walking the
        // flattened model for each new file in a broad search result stream.
        if let Some(model) = imp.results_list.model()
            && let Some(index) = model.n_items().checked_sub(1)
            && let Some(obj) = model.item(index)
            && let Some(row) = obj.downcast_ref::<gtk4::TreeListRow>()
        {
            row.set_expanded(true);
            imp.navigation
                .file_rows
                .borrow_mut()
                .insert(path.clone(), row.clone());
            file_row = Some(row.clone());
        }
    } else {
        file_row = imp.navigation.file_rows.borrow().get(&path).cloned();
    }

    if imp.runtime.total_matches.get() == 0 {
        panel.reveal_results_body();
    }
    imp.runtime
        .total_matches
        .set(imp.runtime.total_matches.get() + 1);
    imp.navigation
        .match_positions
        .borrow_mut()
        .push(SearchMatchLocation::new(path, line_number));
    let match_row = file_row.and_then(|row| {
        if !row.is_expanded() {
            row.set_expanded(true);
        }
        row.child_row(child_position)
    });
    imp.navigation.match_rows.borrow_mut().push(match_row);
}

/// Persist the latest successful query into the recent-history file.
fn persist_search_history(panel: &LushtextSearchPanel, query_spec: &SearchQuerySpec) {
    if query_spec.query.is_empty() {
        return;
    }

    let mut entries = panel.imp().history.history_entries.borrow_mut();
    search_history::add_entry(
        &mut entries,
        SearchHistoryEntry::from_spec(query_spec.clone()),
    );
    let entries_clone = entries.clone();
    drop(entries);

    let data_dir = json_store::data_dir();
    gtk_lush_tasks::spawn_blocking_then(
        panel.clone(),
        move || search_history::save(&data_dir, &entries_clone),
        |_panel, result| {
            if let Err(e) = result {
                tracing::error!("Failed to save search history: {e}");
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receive_one_turn(receiver: &crossbeam_channel::Receiver<SearchEvent>) -> (usize, bool) {
        let mut received = 0;
        let mut terminal = false;
        loop {
            match receive_search_event(receiver, &mut received) {
                SearchEventPoll::Event(SearchEvent::Done) | SearchEventPoll::Disconnected => {
                    terminal = true;
                    break;
                }
                SearchEventPoll::Event(_) => {}
                SearchEventPoll::Empty | SearchEventPoll::BudgetExhausted => break,
            }
        }
        (received, terminal)
    }

    #[test]
    fn progress_only_burst_stops_at_total_event_budget() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        for index in 0..=MAX_SEARCH_EVENTS_PER_TICK {
            sender
                .send(SearchEvent::Progress(index))
                .expect("progress fixture receiver");
        }

        assert_eq!(
            receive_one_turn(&receiver),
            (MAX_SEARCH_EVENTS_PER_TICK, false)
        );
        assert_eq!(receive_one_turn(&receiver), (1, false));
    }

    #[test]
    fn mixed_non_match_events_share_one_budget() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        for index in 0..260 {
            let event = match index % 3 {
                0 => SearchEvent::Progress(index),
                1 => SearchEvent::ResultCap,
                _ => SearchEvent::Error(format!("error-{index}")),
            };
            sender.send(event).expect("mixed fixture receiver");
        }

        assert_eq!(
            receive_one_turn(&receiver),
            (MAX_SEARCH_EVENTS_PER_TICK, false)
        );
        assert_eq!(receive_one_turn(&receiver), (10, false));
        eprintln!(
            "search-event-budget-evidence fixture_events=260 first_turn_events={MAX_SEARCH_EVENTS_PER_TICK} second_turn_events=10"
        );
    }

    #[test]
    fn done_is_charged_before_terminal_dispatch() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        sender
            .send(SearchEvent::Progress(1))
            .expect("terminal fixture receiver");
        sender
            .send(SearchEvent::ResultCap)
            .expect("terminal fixture receiver");
        sender
            .send(SearchEvent::Done)
            .expect("terminal fixture receiver");

        assert_eq!(receive_one_turn(&receiver), (3, true));
    }
}

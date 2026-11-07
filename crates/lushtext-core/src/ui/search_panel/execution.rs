// SPDX-License-Identifier: GPL-3.0-or-later

//! Streaming execution of one workspace content search.
//!
//! This is the `execution` role for the **search** stage order only. The Replace
//! All stage order has its own execution module, `replace_execution.rs`, which
//! carries the stage-order qualifier because this unqualified name was taken
//! first; neither serves the other's stages.
//!
//! This module owns the coordination job that runs between "the user asked for
//! results" and "the panel has them": single-flight admission of the latest
//! query, the worker thread that walks the workspace, and the paced GTK turn
//! that translates pure-Rust `SearchEvent` values into list-model rows.
//!
//! Control inversion: [`LushtextSearchPanel::start_search`] returns as soon as
//! the worker thread and the 50 ms poll timer are armed. Work resumes in that
//! timer callback, once per tick, bounded by `MAX_SEARCH_EVENTS_PER_TICK`
//! events and by the shared list budget's time slice
//! (`model::list_budget::turn_continues`). A turn collects its matches and
//! applies them at its end as one splice per touched file's child store plus
//! one root splice (GTK axiom A87: a presented list pays per `items-changed`,
//! not per item), then expands the new tail file rows by position (A89). The terminal tick is where the flight finishes and any retained
//! latest query is admitted, so the workflow can loop back into this module
//! without returning to the facade.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{self, glib};

use crate::model::content_search::{
    SearchEvent, SearchHistoryEntry, SearchMatch, SearchQuerySpec, SearchRetention,
};
use crate::model::list_budget::turn_continues;
use crate::model::workspace_search::WorkspaceSearchTraversalPlan;
use crate::services::filesystem::metadata as fs_metadata;
use crate::services::{content_search, json_store, search_history};

use super::LushtextSearchPanel;
use super::imp::make_display_path;
use super::item::SearchResultItem;
use super::policy::{
    self, WorkspaceSearchRequest, WorkspaceSearchStart, WorkspaceSearchSubmission,
};
use super::{SearchFileGroup, SearchMatchLocation, SearchProgressUpdate};
use crate::ui::accessibility;

/// Maximum channel events received and dispatched by one scheduled GTK turn.
///
/// Kind: cap; 250 rounded to the power of two 256 (`power-of-two-thresholds`
/// D8): a full turn of 256 `Match` events stays within `B_SETTLE_US`.
const MAX_SEARCH_EVENTS_PER_TICK: usize = 256;

enum SearchEventPoll {
    Event(SearchEvent),
    Empty,
    Disconnected,
    BudgetExhausted,
}

fn receive_search_event(
    receiver: &crossbeam_channel::Receiver<SearchEvent>,
    received: &mut usize,
    event_cap: usize,
) -> SearchEventPoll {
    if *received >= event_cap {
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

/// The retained-byte budget and charge the worker applies: the policy's,
/// unless a widget test or a smoke binary overrides the budget.
fn search_retention() -> SearchRetention {
    let mut budget_bytes = policy::SEARCH_RESULTS_RETAINED_BYTES;
    if let Some(bytes) = crate::services::smoke_list_budgets::search_retained_bytes() {
        budget_bytes = bytes;
    }
    #[cfg(feature = "test-utils")]
    if let Some(bytes) = super::test_policy::search_retained_bytes_override() {
        budget_bytes = bytes;
    }
    SearchRetention {
        budget_bytes,
        match_fixed_charge: policy::SEARCH_MATCH_FIXED_CHARGE,
    }
}

/// Microseconds since `started_us` (a `glib::monotonic_time` reading).
fn elapsed_since(started_us: i64) -> u64 {
    u64::try_from(glib::monotonic_time().saturating_sub(started_us)).unwrap_or(0)
}

impl LushtextSearchPanel {
    /// Seed the per-search hidden-files override from the global preference.
    ///
    /// Runs when the panel maps and when the global key changes while the panel
    /// is idle, so the panel owns its own sync with the mode. The restore guard
    /// keeps the seed from starting a search on its own.
    pub fn seed_hidden_toggle(&self) {
        let imp = self.imp();
        let show_hidden = imp
            .settings
            .boolean(crate::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
        if imp.hidden_toggle.is_active() == show_hidden {
            return;
        }
        let was_restoring = imp.history.restoring_history.replace(true);
        imp.hidden_toggle.set_active(show_hidden);
        imp.history.restoring_history.set(was_restoring);
    }

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
        imp.runtime.matches_found.set(0);
        self.refresh_accessibility_state();
        if let Some(ref cb) = *imp.callbacks.progress_callback.borrow() {
            cb(SearchProgressUpdate::Started);
        }

        let timer_cancel = cancel.clone();
        imp.error_label.set_visible(false);

        let history_spec = spec.clone();
        let worker_spec = spec;
        let worker_folders = Arc::clone(&folders);
        let worker_visibility = self.imp().current_entry_visibility();
        let worker_progress_counter = Arc::clone(&progress_counter);
        let worker_finished_for_search = Arc::new(AtomicBool::new(false));
        let retention = search_retention();
        std::thread::spawn(move || {
            #[cfg(feature = "test-utils")]
            super::test_policy::delay_search_worker();
            let plan = Arc::new(WorkspaceSearchTraversalPlan::build(
                worker_folders.iter().cloned(),
                fs_metadata::canonical_path,
            ));
            let _ = plan_tx.send(Arc::clone(&plan));
            content_search::search_with_plan(
                content_search::ContentSearchRequest {
                    query: &worker_spec.query,
                    plan: &plan,
                    options: &worker_spec.options,
                    visibility: &worker_visibility,
                    retention,
                },
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
            let turn_started_us = glib::monotonic_time();
            // The turn that reveals the first results, and those after it until
            // the list holds one full turn, carry small batches; later turns
            // fill (bound-lists-by-budget-not-count D4 b).
            let reveal_turn = imp.runtime.total_matches.get() == 0;
            let warming_up =
                (imp.runtime.total_matches.get() as usize) < MAX_SEARCH_EVENTS_PER_TICK;
            let event_cap = if warming_up {
                policy::REVEAL_TURN_MAX_EVENTS
            } else {
                MAX_SEARCH_EVENTS_PER_TICK
            };
            let mut turn_matches: Vec<SearchMatch> = Vec::new();
            let mut budget_reached = None;
            // Keep each GTK tick bounded by an event count and by the shared
            // list budget's time slice, so a large streaming search cannot
            // monopolize the main loop while thousands of matches arrive.
            while turn_continues(elapsed_since(turn_started_us), items_this_tick) {
                match receive_search_event(&rx, &mut items_this_tick, event_cap) {
                    SearchEventPoll::Event(SearchEvent::Match(search_match)) => {
                        if !cancelled {
                            turn_matches.push(search_match);
                        }
                    }
                    SearchEventPoll::Event(SearchEvent::Done { matches_found }) => {
                        if !cancelled {
                            note_matches_found(imp, matches_found);
                        }
                        done = true;
                        break;
                    }
                    SearchEventPoll::Disconnected => {
                        done = true;
                        break;
                    }
                    SearchEventPoll::Event(SearchEvent::BudgetReached { retained }) => {
                        if !cancelled {
                            budget_reached = Some(retained);
                        }
                    }
                    SearchEventPoll::Event(SearchEvent::BeyondBudget(found)) => {
                        if !cancelled {
                            note_matches_found(imp, found);
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
            if !cancelled && !turn_matches.is_empty() {
                if reveal_turn {
                    imp.runtime.reveal_turn_events.set(items_this_tick);
                }
                apply_match_batch(&panel, turn_matches, traversal_plan.as_deref(), &folders);
            }
            if let Some(retained) = budget_reached
                && !imp.runtime.result_capped.get()
            {
                imp.runtime.result_capped.set(true);
                imp.count_label.add_css_class("warning");
                accessibility::announce_with_lane(
                    &*imp.count_label,
                    &policy::budget_reached_announcement(retained),
                    accessibility::AnnouncementLane::ProgressMilestone,
                );
            }
            #[cfg(feature = "test-utils")]
            if items_this_tick > 0 {
                imp.runtime
                    .event_turn_observations
                    .borrow_mut()
                    .push((items_this_tick, glib::monotonic_time() - turn_started_us));
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
            if !cancelled && total > 0 && !search_incomplete {
                let label = if imp.runtime.result_capped.get() {
                    policy::limited_count_label(
                        total as usize,
                        files as usize,
                        imp.runtime.matches_found.get(),
                    )
                } else {
                    policy::count_label(total as usize, files as usize)
                };
                if imp.count_label.text() != label {
                    imp.count_label.set_text(&label);
                    panel.refresh_accessibility_state();
                }
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

                // A limited search is not saved, as a capped one never was.
                if !cancelled && total > 0 && !imp.runtime.result_capped.get() {
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
        imp.runtime.matches_found.set(0);
        imp.runtime.reveal_turn_events.set(0);
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

/// Raise the running found count (never lower it: `BeyondBudget` is
/// best-effort and `Done` is final).
fn note_matches_found(imp: &super::imp::LushtextSearchPanel, found: usize) {
    if found > imp.runtime.matches_found.get() {
        imp.runtime.matches_found.set(found);
    }
}

/// The display text of a match line: at most 500 bytes, cut on a character
/// boundary with an ellipsis, and the match range clamped to it.
fn display_line(search_match: &SearchMatch) -> (String, u32, u32) {
    let truncated_len = (search_match.line_content.len() > 500)
        .then(|| search_match.line_content.floor_char_boundary(500));
    let content = truncated_len.map_or_else(
        || search_match.line_content.to_string(),
        |end| format!("{}…", &search_match.line_content[..end]),
    );
    let clamp_len = truncated_len.unwrap_or(content.len());
    let start = u32::try_from(search_match.match_range.start.min(clamp_len)).unwrap_or(u32::MAX);
    let end = u32::try_from(search_match.match_range.end.min(clamp_len)).unwrap_or(u32::MAX);
    (content, start, end)
}

/// One touched file's matches within a turn.
struct FileBatch {
    child_store: gtk4::gio::ListStore,
    header_item: SearchResultItem,
    items: Vec<SearchResultItem>,
}

/// Apply one turn's streamed matches to the grouped file model and the flat
/// navigation index: one splice per touched child store, one splice of new
/// file rows on the root store, then the new tail file rows expanded by
/// position (`bound-lists-by-budget-not-count` D4).
fn apply_match_batch(
    panel: &LushtextSearchPanel,
    matches: Vec<SearchMatch>,
    traversal_plan: Option<&WorkspaceSearchTraversalPlan>,
    workspace_folders: &[PathBuf],
) {
    let imp = panel.imp();
    let was_empty = imp.runtime.total_matches.get() == 0;
    let match_count = matches.len();
    let mut order: Vec<Arc<Path>> = Vec::new();
    let mut batches: HashMap<Arc<Path>, FileBatch> = HashMap::new();
    let mut new_files: Vec<(Arc<Path>, SearchResultItem)> = Vec::new();
    // (file, position in its child store) per match, for the match rows.
    let mut placements: Vec<(Arc<Path>, u32)> = Vec::with_capacity(match_count);
    {
        let mut groups = imp.runtime.file_groups.borrow_mut();
        let mut plain = imp.runtime.search_matches.borrow_mut();
        let mut positions = imp.navigation.match_positions.borrow_mut();
        for search_match in matches {
            let match_id = crate::model::content_search::SearchMatchId::from_index(plain.len());
            let search_match = search_match.with_id(match_id);
            let path = Arc::clone(&search_match.path);
            let group = groups.entry(Arc::clone(&path)).or_insert_with(|| {
                let display = traversal_plan
                    .and_then(|plan| {
                        plan.display_relative_path(search_match.traversal_root_index, &path)
                    })
                    .map_or_else(
                        || make_display_path(&path, workspace_folders),
                        |relative| relative.display().to_string(),
                    );
                let path_text: Arc<str> = Arc::from(path.display().to_string());
                let header = SearchResultItem::new_file_shared(path_text, &display, 0);
                new_files.push((Arc::clone(&path), header.clone()));
                SearchFileGroup::new(header, gtk4::gio::ListStore::new::<SearchResultItem>())
            });
            let batch = batches.entry(Arc::clone(&path)).or_insert_with(|| {
                order.push(Arc::clone(&path));
                FileBatch {
                    child_store: group.child_store.clone(),
                    header_item: group.header_item.clone(),
                    items: Vec::new(),
                }
            });
            let (content, start, end) = display_line(&search_match);
            let line_number = u32::try_from(search_match.line_number).unwrap_or(u32::MAX);
            let child_position = batch
                .child_store
                .n_items()
                .saturating_add(u32::try_from(batch.items.len()).unwrap_or(u32::MAX));
            batch.items.push(SearchResultItem::new_match_shared(
                batch.header_item.shared_file_path(),
                line_number,
                &content,
                start,
                end,
                match_id,
            ));
            positions.push(SearchMatchLocation::new(Arc::clone(&path), line_number));
            placements.push((path, child_position));
            plain.push(search_match);
        }
    }

    // GLib signals below can re-enter the panel, so no runtime borrow is held.
    let mut child_splices = 0;
    for path in &order {
        let Some(batch) = batches.remove(path) else {
            continue;
        };
        let start = batch.child_store.n_items();
        batch.child_store.splice(start, 0, &batch.items);
        child_splices += 1;
        let added = u32::try_from(batch.items.len()).unwrap_or(u32::MAX);
        batch
            .header_item
            .set_match_count(batch.header_item.match_count().saturating_add(added));
    }
    let mut root_splices = 0;
    if !new_files.is_empty() {
        let headers: Vec<SearchResultItem> =
            new_files.iter().map(|(_, header)| header.clone()).collect();
        let root = imp.runtime.root_store.borrow().clone();
        root.splice(root.n_items(), 0, &headers);
        root_splices = 1;
        imp.runtime.total_files.set(
            imp.runtime
                .total_files
                .get()
                .saturating_add(u32::try_from(headers.len()).unwrap_or(u32::MAX)),
        );
        // The new file rows are the collapsed tail of the flattened model:
        // expand them by position, without walking the model (A89).
        if let Some(model) = imp.results_list.model() {
            let new_rows = u32::try_from(new_files.len()).unwrap_or(u32::MAX);
            let mut position = model.n_items().saturating_sub(new_rows);
            let mut file_rows = imp.navigation.file_rows.borrow_mut();
            for (path, _) in &new_files {
                let Some(row) = model.item(position).and_downcast::<gtk4::TreeListRow>() else {
                    break;
                };
                row.set_expanded(true);
                let children = row.children().map_or(0, |children| children.n_items());
                position = position.saturating_add(1).saturating_add(children);
                file_rows.insert(Arc::clone(path), row);
            }
        }
    }
    if was_empty {
        panel.reveal_results_body();
    }
    imp.runtime.total_matches.set(
        imp.runtime
            .total_matches
            .get()
            .saturating_add(u32::try_from(match_count).unwrap_or(u32::MAX)),
    );
    {
        let file_rows = imp.navigation.file_rows.borrow();
        let mut match_rows = imp.navigation.match_rows.borrow_mut();
        for (path, child_position) in placements {
            let match_row = file_rows.get(&path).and_then(|row| {
                if !row.is_expanded() {
                    row.set_expanded(true);
                }
                row.child_row(child_position)
            });
            match_rows.push(match_row);
        }
    }
    imp.runtime
        .last_match_batch
        .set((match_count, child_splices, root_splices));
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
            match receive_search_event(receiver, &mut received, MAX_SEARCH_EVENTS_PER_TICK) {
                SearchEventPoll::Event(SearchEvent::Done { .. })
                | SearchEventPoll::Disconnected => {
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
        let fixture_events = MAX_SEARCH_EVENTS_PER_TICK + 10;
        for index in 0..fixture_events {
            let event = match index % 3 {
                0 => SearchEvent::Progress(index),
                1 => SearchEvent::BudgetReached { retained: index },
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
            "search-event-budget-evidence fixture_events={fixture_events} first_turn_events={MAX_SEARCH_EVENTS_PER_TICK} second_turn_events=10"
        );
    }

    #[test]
    fn done_is_charged_before_terminal_dispatch() {
        let (sender, receiver) = crossbeam_channel::unbounded();
        sender
            .send(SearchEvent::Progress(1))
            .expect("terminal fixture receiver");
        sender
            .send(SearchEvent::BudgetReached { retained: 1 })
            .expect("terminal fixture receiver");
        sender
            .send(SearchEvent::Done { matches_found: 2 })
            .expect("terminal fixture receiver");

        assert_eq!(receive_one_turn(&receiver), (3, true));
    }
}

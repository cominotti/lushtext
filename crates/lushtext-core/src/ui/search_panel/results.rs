// SPDX-License-Identifier: GPL-3.0-or-later

//! Result-list rendering and match navigation for the search panel widget.
//!
//! These helpers stay in the UI layer because they manipulate `GtkListView`,
//! `TreeListRow`, and the panel's revealers directly.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::model::content_search::SearchMatch;

use super::item::SearchResultItem;
use super::LushtextSearchPanel;

impl LushtextSearchPanel {
    /// Whether the panel has any search results.
    #[must_use]
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

        let next = imp.current_match_index.get().map_or(0, |i| (i + 1) % len);
        imp.current_match_index.set(Some(next));

        let (path, line) = positions[next].clone();
        drop(positions);

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
            .map_or(len - 1, |i| if i == 0 { len - 1 } else { i - 1 });
        imp.current_match_index.set(Some(prev));

        let (path, line) = positions[prev].clone();
        drop(positions);

        self.select_match_in_results(prev);

        if let Some(ref cb) = *imp.navigate_callback.borrow() {
            cb(&path, line);
        }
    }

    /// Get the search entry widget (for re-invocation focus/selection).
    #[must_use]
    pub fn search_entry(&self) -> &gtk4::SearchEntry {
        &self.imp().search_entry
    }

    /// Clamp the results scroll area to at most `max_height` pixels.
    /// Called from the window's `size_allocate` with `window_height / 3`
    /// so the search panel never dominates the vertical layout.
    pub fn clamp_results_height(&self, max_height: i32) {
        let imp = self.imp();
        let clamped = max_height.max(100); // never below min-content-height
        if imp.results_scroll.max_content_height() != clamped {
            imp.results_scroll.set_max_content_height(clamped);
        }
        if imp.results_scroll.height_request() != clamped {
            imp.results_scroll.set_height_request(clamped);
        }
    }

    pub(super) fn reveal_results_feedback(&self) {
        let imp = self.imp();
        imp.results_body_revealer.set_reveal_child(false);
        imp.results_feedback_revealer.set_reveal_child(true);
    }

    pub(super) fn reveal_results_body(&self) {
        let imp = self.imp();
        imp.results_feedback_revealer.set_reveal_child(true);
        imp.results_body_revealer.set_reveal_child(true);
    }

    pub(super) fn hide_results_feedback(&self) {
        let imp = self.imp();
        imp.results_body_revealer.set_reveal_child(false);
        imp.results_feedback_revealer.set_reveal_child(false);
    }

    /// Collect all `SearchMatch` data from the current results for preview generation.
    /// Uses the unclamped original line content and match range so Replace All
    /// works from the service-layer data rather than the shortened display text.
    pub(super) fn collect_search_matches(&self) -> Vec<SearchMatch> {
        let imp = self.imp();
        let groups = imp.file_groups.borrow();
        let mut matches = Vec::new();

        for (path, (_, child_store)) in groups.iter() {
            for i in 0..child_store.n_items() {
                if let Some(item) = child_store.item(i).and_downcast::<SearchResultItem>()
                    && item.is_match_item()
                {
                    matches.push(SearchMatch {
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
    pub(super) fn refresh_results_display(&self) {
        let imp = self.imp();
        if let Some(model) = imp.results_list.model() {
            imp.results_list.set_model(Some(&model));
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

        for i in 0..model.n_items() {
            if let Some(obj) = model.item(i)
                && let Some(row) = obj.downcast_ref::<gtk4::TreeListRow>()
                && let Some(item) = row.item().and_downcast::<SearchResultItem>()
                && item.is_match_item()
                && item.line_number() == target_line
                && item.file_path() == target_path_str
            {
                if let Some(selection) = model.downcast_ref::<gtk4::SingleSelection>() {
                    selection.set_selected(i);
                }
                self.imp()
                    .results_list
                    .scroll_to(i, gtk4::ListScrollFlags::FOCUS, None);
                break;
            }
        }
    }
}

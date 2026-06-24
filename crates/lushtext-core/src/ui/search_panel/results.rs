// SPDX-License-Identifier: GPL-3.0-or-later

//! Result-list rendering and match navigation for the search panel widget.
//!
//! These helpers stay in the UI layer because they manipulate `GtkListView`,
//! `TreeListRow`, and the panel's revealers directly.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::item::SearchResultItem;
use super::{LushtextSearchPanel, SearchMatchLocation};

impl LushtextSearchPanel {
    /// Whether the panel has any search results.
    #[must_use]
    pub fn has_results(&self) -> bool {
        self.imp().runtime.total_matches.get() > 0
    }

    /// Navigate to the next match (F4). Wraps around at the end.
    pub fn navigate_next_match(&self) {
        let imp = self.imp();
        let positions = imp.navigation.match_positions.borrow();
        let len = positions.len();
        if len == 0 {
            return;
        }

        let next = imp
            .navigation
            .current_match_index
            .get()
            .map_or(0, |i| (i + 1) % len);
        imp.navigation.current_match_index.set(Some(next));

        let SearchMatchLocation { path, line_number } = positions[next].clone();
        drop(positions);

        self.select_match_in_results(next);

        if let Some(ref cb) = *imp.callbacks.navigate_callback.borrow() {
            cb(&path, line_number);
        }
    }

    /// Navigate to the previous match (Shift+F4). Wraps around at the beginning.
    pub fn navigate_prev_match(&self) {
        let imp = self.imp();
        let positions = imp.navigation.match_positions.borrow();
        let len = positions.len();
        if len == 0 {
            return;
        }

        let prev = imp
            .navigation
            .current_match_index
            .get()
            .map_or(len - 1, |i| if i == 0 { len - 1 } else { i - 1 });
        imp.navigation.current_match_index.set(Some(prev));

        let SearchMatchLocation { path, line_number } = positions[prev].clone();
        drop(positions);

        self.select_match_in_results(prev);

        if let Some(ref cb) = *imp.callbacks.navigate_callback.borrow() {
            cb(&path, line_number);
        }
    }

    /// Get the search entry widget (for re-invocation focus/selection).
    #[must_use]
    pub fn search_entry(&self) -> &gtk4::SearchEntry {
        &self.imp().search_entry
    }

    /// Clamp the results scroll area to at most `max_height` pixels.
    ///
    /// The search panel is optional chrome, so it must be allowed to collapse
    /// in very short windows instead of forcing the persistent status bar below
    /// the allocation.
    pub fn clamp_results_height(&self, max_height: i32) {
        let imp = self.imp();
        let clamped = max_height.max(0);

        // GTK validates min <= max at each setter call, so update the bound
        // that is moving outward first and the bound that is moving inward
        // second. Otherwise a resize from 60px to 100px can briefly set
        // min-content-height above the old max-content-height and warn.
        let current_max = imp.results_scroll.max_content_height();
        if current_max != -1 && clamped > current_max {
            imp.results_scroll.set_max_content_height(clamped);
        }
        if imp.results_scroll.min_content_height() != clamped {
            imp.results_scroll.set_min_content_height(clamped);
        }
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

    /// Snapshot service-layer match data for preview generation.
    ///
    /// The search worker streams each match into this plain Rust cache as it
    /// arrives, so entering Replace preview does not have to walk GTK tree
    /// models or reconstruct original ranges from row objects.
    pub(super) fn collect_search_matches(&self) -> Vec<crate::model::content_search::SearchMatch> {
        self.imp().runtime.search_matches.borrow().clone()
    }

    /// Trigger a visual refresh of the results list by invalidating the factory.
    pub(super) fn refresh_results_display(&self) {
        let imp = self.imp();
        if let Some(model) = imp.results_list.model() {
            imp.results_list.set_model(None::<&gtk4::SelectionModel>);
            imp.results_list.set_model(Some(&model));
        }
    }

    /// Visually select the match row corresponding to `match_positions[match_index]`
    /// in the `SingleSelection` model, and scroll to make it visible.
    fn select_match_in_results(&self, match_index: usize) {
        let imp = self.imp();
        let positions = imp.navigation.match_positions.borrow();
        let Some(target) = positions.get(match_index).cloned() else {
            return;
        };
        let target_path_str = target.path.display().to_string();
        let target_line = target.line_number;
        drop(positions);

        let rows = imp.navigation.match_rows.borrow();
        let Some(Some(row)) = rows.get(match_index).cloned() else {
            return;
        };
        drop(rows);

        let Some(model) = imp.results_list.model() else {
            return;
        };

        if let Some(parent) = row.parent() {
            parent.set_expanded(true);
        }

        let position = row.position();
        if let Some(obj) = model.item(position)
            && let Some(visible_row) = obj.downcast_ref::<gtk4::TreeListRow>()
            && let Some(item) = visible_row.item().and_downcast::<SearchResultItem>()
            && item.is_match_item()
            && item.line_number() == target_line
            && item.file_path() == target_path_str
        {
            if let Some(selection) = model.downcast_ref::<gtk4::SingleSelection>() {
                selection.set_selected(position);
            }
            self.imp()
                .results_list
                .scroll_to(position, gtk4::ListScrollFlags::FOCUS, None);
        }
    }
}

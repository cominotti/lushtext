// SPDX-License-Identifier: GPL-3.0-or-later

//! In-editor find/replace bar flows for one editor tab.
//!
//! These methods stay on the widget because they attach GTK search objects and
//! manipulate focus directly, but extracting them keeps the main facade from
//! mixing file-I/O code with search-bar choreography.

use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::LushtextEditorPage;

/// Maximum selected characters a find surface copies into its query. Users
/// sometimes press Ctrl+Shift+F with a whole document selected; the prefill
/// runs on the GTK thread, so a larger selection is skipped instead of copied.
///
/// Kind: cap; a power of two, rounded down.
const MAX_SEARCH_SELECTION_PREFILL_CHARS: i32 = 1_024;

/// The selection text a find surface (the in-tab bar or the workspace search
/// panel) prefills its query with: at most `MAX_SEARCH_SELECTION_PREFILL_CHARS`
/// characters of selection, visible text only, so a selection across a
/// Protected View fold never copies a hidden tail (design D5).
pub(crate) fn search_prefill_text(buffer: &gtk4::TextBuffer) -> Option<glib::GString> {
    let (start, end) = buffer.selection_bounds()?;
    selection_within_prefill_limit(start.offset(), end.offset())
        .then(|| buffer.text(&start, &end, false))
        .filter(|text| !text.is_empty())
}

fn selection_within_prefill_limit(start_offset: i32, end_offset: i32) -> bool {
    start_offset.abs_diff(end_offset) <= MAX_SEARCH_SELECTION_PREFILL_CHARS.unsigned_abs()
}

impl LushtextEditorPage {
    /// Open the search bar in find-only mode.
    pub fn show_search(&self) {
        self.open_search_bar(false);
    }

    /// Open the search bar in find-and-replace mode.
    pub fn show_replace(&self) {
        self.open_search_bar(true);
    }

    /// Close the search bar, restore the cursor if the user did not navigate,
    /// detach the `SearchContext`, and return focus to the editor.
    pub fn hide_search(&self) {
        let imp = self.imp();
        let navigated = imp.search_bar.has_navigated();

        imp.search_bar.detach();
        imp.search_revealer.set_reveal_child(false);
        self.refresh_minimap();

        if !navigated {
            self.restore_pre_search_cursor();
        }

        imp.source_view.grab_focus();
    }

    /// Access the search bar widget (for window-level next/prev delegation).
    #[must_use]
    pub fn search_bar(&self) -> &crate::ui::search_bar::LushtextSearchBar {
        &self.imp().search_bar
    }

    /// Whether the search bar is currently visible.
    #[must_use]
    pub fn is_search_visible(&self) -> bool {
        self.imp().search_revealer.reveals_child()
    }

    /// Common logic for opening the search bar.
    fn open_search_bar(&self, replace_mode: bool) {
        let imp = self.imp();
        let search_bar = &imp.search_bar;
        let revealer = &imp.search_revealer;

        let was_visible = revealer.reveals_child();
        if !was_visible {
            self.save_pre_search_cursor();
            revealer.set_reveal_child(true);

            // The latest reveal intent is preserved during a file install, but
            // attaching a SearchContext here would duplicate buffer projection
            // work for every bounded text slice. Finalization attaches it to
            // the exact completed buffer when the bar is still visible.
            if self.load_projection_suspended() {
                search_bar.set_replace_mode(replace_mode);
                return;
            }
            self.attach_search_context();

            if let Some(text) = search_prefill_text(self.buffer().upcast_ref()) {
                search_bar.search_entry().set_text(text.as_str());
            }
        }

        // Protected View has no Replace (the buffer is immutable, design D5).
        search_bar.set_replace_mode(replace_mode && !self.is_protected_view());

        let entry = search_bar.search_entry();
        entry.grab_focus();
        entry.select_region(0, -1);
    }

    /// Attach the in-tab search context, except in Protected View: a
    /// `GtkSourceSearchContext` scans the whole buffer from an idle (25–28 s on
    /// the reference giant line) and skips hidden text (ledger A33), so a
    /// protected document's find runs on a worker over its text source
    /// instead (design D5, task 6.1).
    pub(crate) fn attach_search_context(&self) {
        if self.is_protected_view() {
            return;
        }
        // A paced reveal (a restyle, or a dense document's reattach) hides
        // paragraphs, which the search context would skip (A33): reveal them
        // first when a retained query will search at once. No reveal starts
        // while the context has a query (`restyle::prepare_paced_restyle_around`),
        // and a query typed later reveals (`search_bar::execution`).
        if !self.imp().search_bar.search_entry().text().is_empty() {
            self.reveal_everything_now();
        }
        self.imp()
            .search_bar
            .attach(&self.buffer(), self.source_view());
    }

    /// Save the current cursor position as a text mark for later restoration.
    fn save_pre_search_cursor(&self) {
        let buffer = self.buffer();
        let iter = buffer.iter_at_mark(&buffer.get_insert());
        let mark = buffer.create_mark(Some("pre-search-cursor"), &iter, true);
        let _ = mark;
    }

    /// Restore the cursor to the pre-search position.
    fn restore_pre_search_cursor(&self) {
        let buffer = self.buffer();
        if let Some(mark) = buffer.mark("pre-search-cursor") {
            let iter = buffer.iter_at_mark(&mark);
            buffer.place_cursor(&iter);
            buffer.delete_mark(&mark);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SEARCH_SELECTION_PREFILL_CHARS, selection_within_prefill_limit};

    #[test]
    fn search_prefill_skips_large_selection_ranges() {
        assert!(selection_within_prefill_limit(
            0,
            MAX_SEARCH_SELECTION_PREFILL_CHARS
        ));
        assert!(!selection_within_prefill_limit(
            0,
            MAX_SEARCH_SELECTION_PREFILL_CHARS + 1
        ));
        assert!(selection_within_prefill_limit(42, 1));
    }
}

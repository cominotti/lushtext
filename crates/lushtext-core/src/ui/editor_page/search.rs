// SPDX-License-Identifier: GPL-3.0-or-later

//! In-editor find/replace bar flows for one editor tab.
//!
//! These methods stay on the widget because they attach GTK search objects and
//! manipulate focus directly, but extracting them keeps the main facade from
//! mixing file-I/O code with search-bar choreography.

use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::LushtextEditorPage;

/// Maximum selected characters copied into the in-editor Find/Replace query.
const MAX_SEARCH_SELECTION_PREFILL_CHARS: i32 = 1_024;

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
            search_bar.attach(&self.buffer(), self.source_view());

            let buffer = self.buffer();
            if let Some((start, end)) = buffer.selection_bounds() {
                let selection_chars = end.offset().saturating_sub(start.offset());
                if selection_chars > 0 && selection_chars <= MAX_SEARCH_SELECTION_PREFILL_CHARS {
                    let text = buffer.text(&start, &end, true);
                    search_bar.search_entry().set_text(text.as_str());
                }
            }
        }

        search_bar.set_replace_mode(replace_mode);

        let entry = search_bar.search_entry();
        entry.grab_focus();
        entry.select_region(0, -1);
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

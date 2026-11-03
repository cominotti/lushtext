// SPDX-License-Identifier: GPL-3.0-or-later

//! Deferred cursor and scroll restoration for one editor tab.
//!
//! **Five workflows own this group**, which is what makes it cross-cutting
//! editor-page state rather than any one workflow's:
//!
//! | Workflow | Row | Uses |
//! | --- | --- | --- |
//! | session restore | `WFR-SESSION-RESTORE` | `set_restore_position` from the restored tab record, `cursor_position` / `visible_top_line` when persisting |
//! | editor find | `WFR-EDITOR-FIND` | `set_restore_position` before reloading a file to a match |
//! | notes and bookmarks | `WFR-NOTES-BOOKMARKS` | `cursor_position` when anchoring a record |
//! | document load | `WFR-DOCUMENT-LOAD` | `apply_restore_position`, called once from its publish stage |
//! | the window's tab handling | `WFR-TAB-STRIP` | reads the live position for status and titles |
//!
//! Cross-cutting eligibility counts **owning workflows**, not consuming files,
//! so this group stays in a shared `ui/editor_page/` location and each workflow
//! reaches it through a named operation. Slot 3a recorded the same conclusion
//! and left it in place; slot 3b moved it here only because the file it used to
//! share, `load_save.rs`, no longer exists.

use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::LushtextEditorPage;

impl LushtextEditorPage {
    /// Store a cursor and scroll position to apply after the next async load.
    pub fn set_restore_position(&self, cursor_line: u32, cursor_col: u32, scroll_line: u32) {
        self.imp().restore.cursor_line.set(Some(cursor_line));
        self.imp().restore.cursor_col.set(Some(cursor_col));
        self.imp().restore.scroll_line.set(Some(scroll_line));
    }

    /// Reload `path` into this editor keeping the caret's line and column and
    /// the scroll position (Reload, Replace All's reload, Discard Changes), so
    /// the document reappears where the user was, and text typed during the
    /// reload is inserted there rather than at the start of the document.
    pub fn reload_preserving_position(&self, path: &std::path::Path) {
        let (line, column) = self.cursor_position();
        let scroll_line = self.visible_top_line();
        self.set_restore_position(line, column, scroll_line);
        self.load_file_async(path);
    }

    /// Read the current cursor position as (line, column).
    #[must_use]
    #[expect(
        clippy::cast_sign_loss,
        reason = "GtkTextIter line and line_offset values are non-negative i32 coordinates"
    )]
    pub fn cursor_position(&self) -> (u32, u32) {
        if self.is_protected_view()
            && let Some(position) = self.imp().restore.protected_cursor.get()
        {
            return position;
        }
        let buffer = self.buffer();
        let iter = buffer.iter_at_mark(&buffer.get_insert());
        (iter.line() as u32, iter.line_offset() as u32)
    }

    /// Read the line number at the top of the visible scroll area.
    #[must_use]
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "Cursor offsets and scroll positions are derived from non-negative GTK coordinates and persisted as u32 session data"
    )]
    pub fn visible_top_line(&self) -> u32 {
        let view = self.source_view();
        let Some(vadj) = view.vadjustment() else {
            return 0;
        };
        let (iter, _line_top) = view.line_at_y(vadj.value() as i32);
        iter.line() as u32
    }

    /// Apply stored cursor/scroll position after a file load, then clear it.
    ///
    /// Called once, from the document-load workflow's publish stage. Taking the
    /// stored values is what makes the restoration one-shot: a later load with
    /// no stored position must not resurrect an old one.
    pub(crate) fn apply_restore_position(&self) {
        let line = self.imp().restore.cursor_line.take();
        let col = self.imp().restore.cursor_col.take();
        let scroll_line = self.imp().restore.scroll_line.take();

        let buffer = self.buffer();

        // Protected View places no caret: it keeps the requested cursor for
        // later session saves and scrolls the navigation mark, never
        // positioning an iterator inside a hidden tail (ledger A66).
        if self.is_protected_view() {
            if let Some(line) = line {
                self.imp()
                    .restore
                    .protected_cursor
                    .set(Some((line, col.unwrap_or(0))));
            }
            if let Some(scroll_line) = scroll_line.or(line) {
                self.scroll_protected_navigation_to_top(u64::from(scroll_line));
            }
            return;
        }
        self.imp().restore.protected_cursor.set(None);

        if let Some(line) = line
            && let Some(mut iter) = buffer.iter_at_line(line as i32)
        {
            if let Some(col) = col {
                iter.forward_chars(col as i32);
            }
            buffer.place_cursor(&iter);
        }

        if let Some(scroll_line) = scroll_line
            && let Some(iter) = buffer.iter_at_line(scroll_line as i32)
        {
            let mark = buffer.create_mark(None, &iter, true);
            self.source_view()
                .scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
            buffer.delete_mark(&mark);
        }
    }
}

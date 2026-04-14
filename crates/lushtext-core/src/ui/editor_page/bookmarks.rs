// SPDX-License-Identifier: GPL-3.0-or-later

//! Live bookmark projection for one editor tab.
//!
//! Bookmarks are persisted as pure model records, but while a file is open the
//! source buffer owns the live line tracking through `GtkSourceMark`. This
//! module bridges those two shapes and keeps the public editor facade small.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::model::bookmark::{BookmarkId, BookmarkRecord};

use super::{LushtextEditorPage, imp};

/// Source-mark category used for bookmark gutter indicators.
///
/// A dedicated category lets the editor remove only bookmark marks without
/// disturbing any future source-mark users.
const BOOKMARK_MARK_CATEGORY: &str = "lushtext-bookmark";
/// Mark priority used for bookmark gutter icons.
const BOOKMARK_MARK_PRIORITY: i32 = 150;

/// Result of toggling the bookmark on the active cursor line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkToggleState {
    Added(u32),
    Removed(u32),
}

/// Direction used by the bookmark navigation action pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkNavigationDirection {
    Next,
    Previous,
}

/// Install bookmark gutter attributes and tooltip behavior on the source view.
pub(super) fn setup_bookmark_projection(editor: &LushtextEditorPage) {
    let attributes = sourceview5::MarkAttributes::new();
    attributes.set_icon_name("bookmark-new-symbolic");

    let editor_weak = editor.downgrade();
    attributes.connect_query_tooltip_text(move |_, mark| {
        editor_weak
            .upgrade()
            .and_then(|editor| {
                bookmark_id_for_mark(mark).and_then(|id| editor_bookmark(&editor, &id))
            })
            .map_or_else(
                || "Bookmark".to_string(),
                |bookmark| bookmark.display_label(),
            )
    });

    editor.source_view().set_mark_attributes(
        BOOKMARK_MARK_CATEGORY,
        &attributes,
        BOOKMARK_MARK_PRIORITY,
    );
}

/// Return a pure-model snapshot of the current live bookmark marks.
#[must_use]
pub(super) fn bookmark_records(editor: &LushtextEditorPage) -> Vec<BookmarkRecord> {
    let mut bookmarks: Vec<BookmarkRecord> = editor
        .imp()
        .bookmarks
        .entries
        .borrow()
        .iter()
        .filter_map(|entry| {
            current_mark_line(editor, &entry.mark).map(|line| {
                let mut record = entry.record.clone();
                record.line = line;
                record
            })
        })
        .collect();
    bookmarks.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    bookmarks
}

/// Replace the live bookmark projection with freshly loaded sidecar records.
pub(super) fn load_bookmarks(editor: &LushtextEditorPage, bookmarks: &[BookmarkRecord]) {
    clear_bookmarks(editor);

    let buffer = editor.buffer();
    let mut live_entries = Vec::with_capacity(bookmarks.len());
    for bookmark in bookmarks {
        let iter = iter_at_line_or_last(&buffer, bookmark.line);
        let mark = buffer.create_source_mark(Some(&bookmark.id.0), BOOKMARK_MARK_CATEGORY, &iter);

        live_entries.push(imp::LiveBookmark {
            record: bookmark.clone(),
            mark,
        });
    }

    live_entries.sort_by(|left, right| {
        left.record
            .line
            .cmp(&right.record.line)
            .then_with(|| left.record.id.0.cmp(&right.record.id.0))
    });
    *editor.imp().bookmarks.entries.borrow_mut() = live_entries;
    editor.schedule_minimap_refresh();
}

/// Remove all live bookmark marks for the current file identity.
pub(super) fn clear_bookmarks(editor: &LushtextEditorPage) {
    let buffer = editor.buffer();
    buffer.remove_source_marks(
        &buffer.start_iter(),
        &buffer.end_iter(),
        Some(BOOKMARK_MARK_CATEGORY),
    );
    editor.imp().bookmarks.entries.borrow_mut().clear();
    editor.schedule_minimap_refresh();
}

/// Toggle the bookmark on the active cursor line.
#[must_use]
pub(super) fn toggle_bookmark_at_cursor(editor: &LushtextEditorPage) -> BookmarkToggleState {
    let line = editor.cursor_position().0;

    if let Some(index) = bookmark_index_at_line(editor, line) {
        let bookmark = editor.imp().bookmarks.entries.borrow_mut().remove(index);
        editor.buffer().delete_mark(&bookmark.mark);
        emit_bookmarks_changed(editor);
        return BookmarkToggleState::Removed(line);
    }

    let iter = iter_at_line_or_last(&editor.buffer(), line);
    let record = BookmarkRecord::new(line, None);
    let mark =
        editor
            .buffer()
            .create_source_mark(Some(&record.id.0), BOOKMARK_MARK_CATEGORY, &iter);
    editor
        .imp()
        .bookmarks
        .entries
        .borrow_mut()
        .push(imp::LiveBookmark { record, mark });
    editor
        .imp()
        .bookmarks
        .entries
        .borrow_mut()
        .sort_by(|left, right| {
            left.record
                .line
                .cmp(&right.record.line)
                .then_with(|| left.record.id.0.cmp(&right.record.id.0))
        });
    emit_bookmarks_changed(editor);
    BookmarkToggleState::Added(line)
}

/// Update the label for the bookmark on the active cursor line.
#[must_use]
pub(super) fn set_bookmark_label_at_cursor(
    editor: &LushtextEditorPage,
    label: Option<String>,
) -> Option<BookmarkRecord> {
    let line = editor.cursor_position().0;
    let index = bookmark_index_at_line(editor, line)?;
    let mut entries = editor.imp().bookmarks.entries.borrow_mut();
    entries[index].record.set_label(label);
    let bookmark = entries[index].record.clone();
    drop(entries);
    emit_bookmarks_changed(editor);
    Some(bookmark)
}

/// Return the bookmark on the active cursor line, if one exists.
#[must_use]
pub(super) fn current_bookmark(editor: &LushtextEditorPage) -> Option<BookmarkRecord> {
    let line = editor.cursor_position().0;
    bookmark_index_at_line(editor, line).map(|index| {
        editor.imp().bookmarks.entries.borrow()[index]
            .record
            .clone()
    })
}

/// Jump to the next or previous bookmark in the active file, wrapping around.
#[must_use]
pub(super) fn navigate_bookmark(
    editor: &LushtextEditorPage,
    direction: BookmarkNavigationDirection,
) -> Option<BookmarkRecord> {
    let bookmarks = bookmark_records(editor);
    if bookmarks.is_empty() {
        return None;
    }

    let current_line = editor.cursor_position().0;
    let target = match direction {
        BookmarkNavigationDirection::Next => bookmarks
            .iter()
            .find(|bookmark| bookmark.line > current_line)
            .cloned()
            .unwrap_or_else(|| bookmarks[0].clone()),
        BookmarkNavigationDirection::Previous => bookmarks
            .iter()
            .rev()
            .find(|bookmark| bookmark.line < current_line)
            .cloned()
            .unwrap_or_else(|| bookmarks.last().cloned().expect("non-empty")),
    };

    scroll_cursor_to_line(editor, target.line);
    Some(target)
}

/// Emit the bookmark-changed callback when one is registered.
pub(super) fn emit_bookmarks_changed(editor: &LushtextEditorPage) {
    if let Some(callback) = editor.imp().bookmarks.changed_callback.borrow().as_ref() {
        callback();
    }
    editor.schedule_minimap_refresh();
}

/// Reconcile persisted bookmark lines after the user edits the buffer.
#[must_use]
pub(super) fn reconcile_bookmarks_after_edit(editor: &LushtextEditorPage) -> bool {
    let mut changed = false;
    for entry in editor.imp().bookmarks.entries.borrow_mut().iter_mut() {
        if let Some(line) = current_mark_line(editor, &entry.mark) {
            changed |= entry.record.move_to_line(line);
        }
    }
    if changed {
        editor
            .imp()
            .bookmarks
            .entries
            .borrow_mut()
            .sort_by(|left, right| {
                left.record
                    .line
                    .cmp(&right.record.line)
                    .then_with(|| left.record.id.0.cmp(&right.record.id.0))
            });
    }
    changed
}

/// Return a bookmark by ID from the live projection.
#[must_use]
fn editor_bookmark(editor: &LushtextEditorPage, id: &BookmarkId) -> Option<BookmarkRecord> {
    editor
        .imp()
        .bookmarks
        .entries
        .borrow()
        .iter()
        .find(|entry| entry.record.id == *id)
        .map(|entry| entry.record.clone())
}

/// Return the current live bookmark line for a source mark.
#[must_use]
fn current_mark_line(editor: &LushtextEditorPage, mark: &sourceview5::Mark) -> Option<u32> {
    let iter = editor.buffer().iter_at_mark(mark);
    u32::try_from(iter.line()).ok()
}

/// Resolve the bookmark index that currently occupies `line`.
#[must_use]
fn bookmark_index_at_line(editor: &LushtextEditorPage, line: u32) -> Option<usize> {
    editor
        .imp()
        .bookmarks
        .entries
        .borrow()
        .iter()
        .position(|entry| current_mark_line(editor, &entry.mark) == Some(line))
}

/// Convert the source-mark name back into a bookmark ID.
#[must_use]
fn bookmark_id_for_mark(mark: &sourceview5::Mark) -> Option<BookmarkId> {
    mark.name().map(|name| BookmarkId(name.to_string()))
}

/// Return an iterator for `line`, clamping stale sidecar line numbers to the last line.
#[must_use]
fn iter_at_line_or_last(buffer: &sourceview5::Buffer, line: u32) -> gtk4::TextIter {
    i32::try_from(line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or_else(|| {
            let end = buffer.end_iter();
            if end.line() > 0 {
                buffer
                    .iter_at_line(end.line())
                    .unwrap_or_else(|| buffer.start_iter())
            } else {
                buffer.start_iter()
            }
        })
}

/// Move the cursor to `line` and keep the focused editor in view.
fn scroll_cursor_to_line(editor: &LushtextEditorPage, line: u32) {
    let iter = iter_at_line_or_last(&editor.buffer(), line);
    editor.buffer().place_cursor(&iter);
    let mark = editor.buffer().create_mark(None, &iter, true);
    editor
        .source_view()
        .scroll_to_mark(&mark, 0.2, true, 0.0, 0.0);
    editor.buffer().delete_mark(&mark);
    editor.source_view().grab_focus();
}

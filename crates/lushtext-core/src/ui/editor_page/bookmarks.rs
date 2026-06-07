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

/// Validation failure while editing an existing bookmark.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookmarkEditError {
    /// The bookmark ID from the dialog no longer exists in the live projection.
    NotFound,
    /// The requested user-facing line is outside the active buffer.
    LineOutOfRange {
        /// 1-based line entered by the user.
        requested_line: u32,
        /// Highest 1-based line currently accepted for this buffer.
        max_line: u32,
    },
    /// Another bookmark already owns the requested user-facing line.
    LineOccupied {
        /// 1-based line that is already occupied.
        line: u32,
    },
}

/// Narrow result of a successful bookmark edit command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BookmarkEditOutcome {
    /// Zero-based buffer line where the bookmark now lives.
    pub line: u32,
}

/// Install bookmark gutter attributes, tooltip behavior, and click activation.
///
/// The line-mark signal runs on the GTK main thread and resolves activation
/// through the live bookmark projection before the window layer shows editing UI.
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

    // GtkSourceView owns gutter hit-testing for source marks. The editor checks
    // the clicked line against its own bookmark marks, then notifies the window
    // because modal presentation belongs to the window layer.
    let editor_weak = editor.downgrade();
    editor
        .source_view()
        .connect_line_mark_activated(move |_, iter, button, _, n_presses| {
            if button != 1 || n_presses != 1 {
                return;
            }
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if let Ok(line) = u32::try_from(iter.line()) {
                let _ = activate_bookmark_at_line(&editor, line);
            }
        });
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
    let bookmark = bookmark_at_line(editor, line)?;
    let bookmark_id = bookmark.id;
    update_bookmark(editor, &bookmark_id, label, line.saturating_add(1)).ok()?;
    editor_bookmark(editor, &bookmark_id)
}

/// Return the bookmark on the active cursor line, if one exists.
#[must_use]
pub(super) fn current_bookmark(editor: &LushtextEditorPage) -> Option<BookmarkRecord> {
    let line = editor.cursor_position().0;
    bookmark_at_line(editor, line)
}

/// Return the bookmark whose live mark currently occupies a zero-based buffer line.
#[must_use]
pub(super) fn bookmark_at_line(editor: &LushtextEditorPage, line: u32) -> Option<BookmarkRecord> {
    bookmark_index_at_line(editor, line).and_then(|index| bookmark_record_at_index(editor, index))
}

/// Notify the window layer that the user activated a zero-based buffer line.
#[must_use]
pub(super) fn activate_bookmark_at_line(
    editor: &LushtextEditorPage,
    line: u32,
) -> Option<BookmarkRecord> {
    let bookmark = bookmark_at_line(editor, line)?;
    if let Some(callback) = editor.imp().bookmarks.activated_callback.borrow().as_ref() {
        callback(bookmark.clone());
    }
    Some(bookmark)
}

/// Move or relabel an existing bookmark while preserving its stable ID.
///
/// `target_line` is the 1-based line number used by dialogs. This runs on the
/// GTK main thread, moves the live `GtkSourceMark`, rejects invalid or occupied
/// target lines before mutating state, and emits the bookmark-changed callback
/// so minimap refresh and sidecar persistence use the existing path.
pub(super) fn update_bookmark(
    editor: &LushtextEditorPage,
    id: &BookmarkId,
    label: Option<String>,
    target_line: u32,
) -> Result<BookmarkEditOutcome, BookmarkEditError> {
    let max_line = buffer_line_count(editor);
    if target_line == 0 || target_line > max_line {
        return Err(BookmarkEditError::LineOutOfRange {
            requested_line: target_line,
            max_line,
        });
    }

    let target_zero_based = target_line - 1;
    if !bookmark_exists(editor, id) {
        return Err(BookmarkEditError::NotFound);
    }
    if bookmark_line_occupied_by_other(editor, target_zero_based, id) {
        return Err(BookmarkEditError::LineOccupied { line: target_line });
    }

    let mark = editor
        .imp()
        .bookmarks
        .entries
        .borrow()
        .iter()
        .find(|entry| entry.record.id == *id)
        .map(|entry| entry.mark.clone())
        .ok_or(BookmarkEditError::NotFound)?;

    let iter = iter_at_line_or_last(&editor.buffer(), target_zero_based);
    editor.buffer().move_mark(&mark, &iter);

    let mut entries = editor.imp().bookmarks.entries.borrow_mut();
    let entry = entries
        .iter_mut()
        .find(|entry| entry.record.id == *id)
        .ok_or(BookmarkEditError::NotFound)?;
    entry.record.set_label(label);
    let _ = entry.record.move_to_line(target_zero_based);
    entries.sort_by(|left, right| {
        left.record
            .line
            .cmp(&right.record.line)
            .then_with(|| left.record.id.0.cmp(&right.record.id.0))
    });
    drop(entries);
    emit_bookmarks_changed(editor);
    Ok(BookmarkEditOutcome {
        line: target_zero_based,
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

/// Register a callback for bookmark gutter activation.
pub(super) fn connect_bookmark_activated<F: Fn(BookmarkRecord) + 'static>(
    editor: &LushtextEditorPage,
    f: F,
) {
    *editor.imp().bookmarks.activated_callback.borrow_mut() = Some(Box::new(f));
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
        .position(|entry| entry.record.id == *id)
        .and_then(|index| bookmark_record_at_index(editor, index))
}

/// Return a bookmark record at `index`, refreshed from its live mark line.
#[must_use]
fn bookmark_record_at_index(editor: &LushtextEditorPage, index: usize) -> Option<BookmarkRecord> {
    let entries = editor.imp().bookmarks.entries.borrow();
    let entry = entries.get(index)?;
    let mut record = entry.record.clone();
    record.line = current_mark_line(editor, &entry.mark)?;
    Some(record)
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

#[must_use]
fn bookmark_exists(editor: &LushtextEditorPage, id: &BookmarkId) -> bool {
    editor
        .imp()
        .bookmarks
        .entries
        .borrow()
        .iter()
        .any(|entry| entry.record.id == *id)
}

/// Return whether a different bookmark already occupies the target line.
#[must_use]
fn bookmark_line_occupied_by_other(
    editor: &LushtextEditorPage,
    line: u32,
    id: &BookmarkId,
) -> bool {
    editor
        .imp()
        .bookmarks
        .entries
        .borrow()
        .iter()
        .any(|entry| entry.record.id != *id && current_mark_line(editor, &entry.mark) == Some(line))
}

/// Return the current buffer line count in the 1-based domain used by dialogs.
#[must_use]
fn buffer_line_count(editor: &LushtextEditorPage) -> u32 {
    u32::try_from(editor.buffer().line_count())
        .unwrap_or(1)
        .max(1)
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

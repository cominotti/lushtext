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
                |bookmark| bookmark_gutter_tooltip(&bookmark),
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
    bookmark_records_bounded(editor, usize::MAX)
}

pub(super) fn bookmark_records_bounded(
    editor: &LushtextEditorPage,
    max_records: usize,
) -> Vec<BookmarkRecord> {
    bookmark_records_bounded_by_retained_bytes(editor, max_records, u64::MAX).0
}

/// Clone a prefix of live bookmark metadata under count and retained-byte caps.
///
/// Heap sizes are checked on the existing records before cloning, preventing one
/// oversized label from becoming an unaccounted duplicate in a deferred Notes
/// request on the GTK thread.
pub(super) fn bookmark_records_bounded_by_retained_bytes(
    editor: &LushtextEditorPage,
    max_records: usize,
    max_retained_bytes: u64,
) -> (Vec<BookmarkRecord>, u64, bool) {
    let entries = editor.imp().bookmarks.entries.borrow();
    let record_size = std::mem::size_of::<BookmarkRecord>();
    let byte_limited_records =
        usize::try_from(max_retained_bytes / u64::try_from(record_size.max(1)).unwrap_or(u64::MAX))
            .unwrap_or(usize::MAX);
    let capacity = max_records.min(entries.len()).min(byte_limited_records);
    let mut bookmarks = Vec::with_capacity(capacity);
    let mut retained_bytes =
        u64::try_from(capacity.saturating_mul(record_size)).unwrap_or(u64::MAX);
    let mut truncated = false;
    for entry in entries.iter() {
        let Some(line) = current_mark_line(editor, &entry.mark) else {
            continue;
        };
        if bookmarks.len() == capacity {
            truncated = true;
            break;
        }
        let heap_bytes = entry
            .record
            .id
            .0
            .capacity()
            .saturating_add(entry.record.label.as_ref().map_or(0, String::capacity));
        let heap_bytes = u64::try_from(heap_bytes).unwrap_or(u64::MAX);
        if retained_bytes.saturating_add(heap_bytes) > max_retained_bytes {
            truncated = true;
            break;
        }
        let mut record = entry.record.clone();
        record.line = line;
        bookmarks.push(record);
        retained_bytes = retained_bytes.saturating_add(heap_bytes);
    }
    bookmarks.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    (bookmarks, retained_bytes, truncated)
}

/// Return the live bookmark projection generation for async race guards.
#[must_use]
pub(super) fn bookmark_change_generation(editor: &LushtextEditorPage) -> u64 {
    editor.imp().bookmarks.change_generation.get()
}

/// Apply a sidecar bookmark snapshot only when no local bookmark edit won the race.
#[must_use]
pub(super) fn load_bookmarks_if_generation_matches(
    editor: &LushtextEditorPage,
    bookmarks: &[BookmarkRecord],
    expected_generation: u64,
) -> bool {
    if bookmark_change_generation(editor) != expected_generation {
        return false;
    }

    load_bookmarks(editor, bookmarks);
    true
}

/// Advance the live bookmark projection generation after a real mutation.
fn bump_bookmark_change_generation(editor: &LushtextEditorPage) {
    let generation = bookmark_change_generation(editor).wrapping_add(1);
    editor.imp().bookmarks.change_generation.set(generation);
}

/// Replace the live bookmark projection with freshly loaded sidecar records.
pub(super) fn load_bookmarks(editor: &LushtextEditorPage, bookmarks: &[BookmarkRecord]) {
    let previous = bookmark_records(editor);
    clear_bookmark_projection(editor);

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
    if previous != bookmark_records(editor) {
        bump_bookmark_change_generation(editor);
    }
    editor.schedule_minimap_refresh();
}

/// Remove all live bookmark marks for the current file identity.
pub(super) fn clear_bookmarks(editor: &LushtextEditorPage) {
    let had_bookmarks = !editor.imp().bookmarks.entries.borrow().is_empty();
    clear_bookmark_projection(editor);
    if had_bookmarks {
        bump_bookmark_change_generation(editor);
    }
}

/// Clear source marks and live entries without recording a semantic change.
fn clear_bookmark_projection(editor: &LushtextEditorPage) {
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
    bump_bookmark_change_generation(editor);
    if let Some(callback) = editor.imp().bookmarks.changed_callback.borrow().as_ref() {
        callback();
    }
    editor.schedule_minimap_refresh();
}

/// Return the user-facing tooltip for one bookmark gutter marker.
///
/// GtkSourceView exposes source marks as compact gutter icons, so the tooltip is
/// the only text metadata attached to that pointer target. Include both the
/// optional label and the current line so the icon is meaningful on its own.
#[must_use]
fn bookmark_gutter_tooltip(bookmark: &BookmarkRecord) -> String {
    let line = bookmark.line.saturating_add(1);
    match bookmark.label.as_deref() {
        Some(label) => format!("Bookmark {label} at line {line}"),
        None => format!("Bookmark at line {line}"),
    }
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

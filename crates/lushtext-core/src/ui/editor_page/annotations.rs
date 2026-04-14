// SPDX-License-Identifier: GPL-3.0-or-later

//! Live annotation projection for one editor tab.
//!
//! Persisted annotations store pure line ranges and note metadata, while the
//! live editor needs anchors that survive inserts/deletes plus visible range
//! highlights. This module owns that per-buffer projection.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::model::annotation::{AnnotationId, AnnotationRecord, AnnotationStyle};

use super::{LushtextEditorPage, imp};

/// Search/edit context derived from the current cursor and selection state.
#[derive(Debug, Clone)]
pub enum AnnotationEditSelection {
    Existing(AnnotationRecord),
    NewRange { start_line: u32, end_line: u32 },
}

/// Prefix for text-tag names created for annotation highlights.
const ANNOTATION_TAG_PREFIX: &str = "lushtext-annotation-";

/// Return a pure-model snapshot of the current live annotation anchors.
#[must_use]
pub(super) fn annotation_records(editor: &LushtextEditorPage) -> Vec<AnnotationRecord> {
    let mut annotations: Vec<AnnotationRecord> = editor
        .imp()
        .annotations
        .entries
        .borrow()
        .iter()
        .filter_map(|entry| {
            current_annotation_range(editor, entry).map(|(start, end, _, _)| {
                let mut record = entry.record.clone();
                record.start_line = start;
                record.end_line = end;
                record
            })
        })
        .collect();
    annotations.sort_by(|left, right| {
        left.start_line
            .cmp(&right.start_line)
            .then_with(|| left.end_line.cmp(&right.end_line))
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    annotations
}

/// Replace the live annotation projection with freshly loaded sidecar records.
pub(super) fn load_annotations(editor: &LushtextEditorPage, annotations: &[AnnotationRecord]) {
    clear_annotations(editor);

    let mut live_entries = Vec::with_capacity(annotations.len());
    for annotation in annotations {
        live_entries.push(create_live_annotation(editor, annotation.clone()));
    }

    *editor.imp().annotations.entries.borrow_mut() = live_entries;
    editor.imp().annotations.loaded.set(true);
    refresh_annotation_highlights(editor);
}

/// Remove all live annotations for the current file identity.
pub(super) fn clear_annotations(editor: &LushtextEditorPage) {
    let buffer = editor.buffer();
    let table = buffer.tag_table();
    for entry in editor.imp().annotations.entries.borrow().iter() {
        buffer.delete_mark(&entry.start_mark);
        buffer.delete_mark(&entry.end_mark);
        if let Some(tag) = table.lookup(&entry.tag_name) {
            table.remove(&tag);
        }
    }
    editor.imp().annotations.entries.borrow_mut().clear();
    editor.imp().annotations.loaded.set(false);
}

/// Create a new annotation from the current selection (or current line).
#[must_use]
pub(super) fn create_annotation_from_selection(
    editor: &LushtextEditorPage,
    note_text: String,
    style: AnnotationStyle,
) -> AnnotationRecord {
    let (start_line, end_line) = selected_line_range(editor);
    let record = AnnotationRecord::new(start_line, end_line, note_text, style);
    let live = create_live_annotation(editor, record.clone());
    editor.imp().annotations.entries.borrow_mut().push(live);
    editor
        .imp()
        .annotations
        .entries
        .borrow_mut()
        .sort_by(|left, right| {
            left.record
                .start_line
                .cmp(&right.record.start_line)
                .then_with(|| left.record.end_line.cmp(&right.record.end_line))
                .then_with(|| left.record.id.0.cmp(&right.record.id.0))
        });
    refresh_annotation_highlights(editor);
    emit_annotations_changed(editor);
    record
}

/// Update an existing annotation's note body and presentation style.
#[must_use]
pub(super) fn update_annotation(
    editor: &LushtextEditorPage,
    annotation_id: &AnnotationId,
    note_text: String,
    style: AnnotationStyle,
) -> Option<AnnotationRecord> {
    let mut entries = editor.imp().annotations.entries.borrow_mut();
    let entry = entries
        .iter_mut()
        .find(|entry| entry.record.id == *annotation_id)?;
    entry.record.update_content(note_text, style);
    let record = entry.record.clone();
    drop(entries);
    refresh_annotation_highlights(editor);
    emit_annotations_changed(editor);
    Some(record)
}

/// Delete an existing annotation from the live editor state.
#[must_use]
pub(super) fn delete_annotation(editor: &LushtextEditorPage, annotation_id: &AnnotationId) -> bool {
    let Some(index) = editor
        .imp()
        .annotations
        .entries
        .borrow()
        .iter()
        .position(|entry| entry.record.id == *annotation_id)
    else {
        return false;
    };

    remove_annotation_at_index(editor, index);
    emit_annotations_changed(editor);
    true
}

/// Return the annotation currently covering the cursor line, if one exists.
#[must_use]
pub(super) fn current_annotation(editor: &LushtextEditorPage) -> Option<AnnotationRecord> {
    let line = editor.cursor_position().0;
    annotation_records(editor)
        .into_iter()
        .find(|annotation| annotation.start_line <= line && line <= annotation.end_line)
}

/// Find a specific annotation by ID in the current live projection.
#[must_use]
pub(super) fn annotation_by_id(
    editor: &LushtextEditorPage,
    annotation_id: &AnnotationId,
) -> Option<AnnotationRecord> {
    editor
        .imp()
        .annotations
        .entries
        .borrow()
        .iter()
        .find(|entry| entry.record.id == *annotation_id)
        .map(|entry| entry.record.clone())
}

/// Record an annotation that should reopen once the next file load finishes.
pub(super) fn set_pending_annotation_focus(
    editor: &LushtextEditorPage,
    annotation_id: Option<AnnotationId>,
) {
    *editor.imp().annotations.pending_focus_id.borrow_mut() = annotation_id;
}

/// Consume the pending annotation focus request after load completes.
#[must_use]
pub(super) fn take_pending_annotation_focus(editor: &LushtextEditorPage) -> Option<AnnotationId> {
    editor
        .imp()
        .annotations
        .pending_focus_id
        .borrow_mut()
        .take()
}

/// Describe whether the current cursor should edit an existing annotation or create a new one.
#[must_use]
pub(super) fn annotation_edit_selection(editor: &LushtextEditorPage) -> AnnotationEditSelection {
    current_annotation(editor).map_or_else(
        || {
            let (start_line, end_line) = selected_line_range(editor);
            AnnotationEditSelection::NewRange {
                start_line,
                end_line,
            }
        },
        AnnotationEditSelection::Existing,
    )
}

/// Emit the annotation-changed callback when one is registered.
pub(super) fn emit_annotations_changed(editor: &LushtextEditorPage) {
    if let Some(callback) = editor.imp().annotations.changed_callback.borrow().as_ref() {
        callback();
    }
}

/// Reconcile annotation ranges after the user edits the buffer.
#[must_use]
pub(super) fn reconcile_annotations_after_edit(editor: &LushtextEditorPage) -> bool {
    let mut changed = false;
    let mut removed_indices = Vec::new();

    {
        let mut entries = editor.imp().annotations.entries.borrow_mut();
        for (index, entry) in entries.iter_mut().enumerate() {
            match current_annotation_range(editor, entry) {
                Some((start, end, _, _)) => {
                    changed |= entry.record.move_to_range(start, end);
                }
                None => removed_indices.push(index),
            }
        }
    }

    for index in removed_indices.into_iter().rev() {
        remove_annotation_at_index(editor, index);
        changed = true;
    }

    if changed {
        editor
            .imp()
            .annotations
            .entries
            .borrow_mut()
            .sort_by(|left, right| {
                left.record
                    .start_line
                    .cmp(&right.record.start_line)
                    .then_with(|| left.record.end_line.cmp(&right.record.end_line))
                    .then_with(|| left.record.id.0.cmp(&right.record.id.0))
            });
    }

    changed
}

/// Refresh annotation highlight colors and visibility after theme or settings changes.
pub(super) fn refresh_annotation_highlights(editor: &LushtextEditorPage) {
    let visible = editor
        .imp()
        .settings
        .boolean(crate::config::keys::ANNOTATION_HIGHLIGHTS_VISIBLE);
    let is_dark = libadwaita::StyleManager::default().is_dark();
    let buffer = editor.buffer();

    for entry in editor.imp().annotations.entries.borrow().iter() {
        let tag = ensure_annotation_tag(&buffer, &entry.tag_name, entry.record.style);
        apply_annotation_tag_style(&tag, entry.record.style, is_dark);

        if let Some((_, _, start_iter, end_iter)) = current_annotation_range(editor, entry) {
            buffer.remove_tag(&tag, &buffer.start_iter(), &buffer.end_iter());
            if visible {
                buffer.apply_tag(&tag, &start_iter, &end_iter);
            }
        }
    }
}

/// Toggle whether annotation highlights are applied to the current buffer.
pub(super) fn set_annotation_highlights_visible(editor: &LushtextEditorPage, _visible: bool) {
    refresh_annotation_highlights(editor);
}

/// Build one live annotation from a persisted sidecar record.
fn create_live_annotation(
    editor: &LushtextEditorPage,
    record: AnnotationRecord,
) -> imp::LiveAnnotation {
    let buffer = editor.buffer();
    let start_iter = iter_at_line_or_last(&buffer, record.start_line);
    let end_iter = exclusive_end_iter_for_range(&buffer, record.end_line);

    let start_mark =
        buffer.create_mark(Some(&format!("{}-start", record.id.0)), &start_iter, false);
    let end_mark = buffer.create_mark(Some(&format!("{}-end", record.id.0)), &end_iter, true);
    let tag_name = annotation_tag_name(&record.id);

    imp::LiveAnnotation {
        record,
        start_mark,
        end_mark,
        tag_name,
    }
}

/// Remove one annotation entry by index without emitting the changed callback.
fn remove_annotation_at_index(editor: &LushtextEditorPage, index: usize) {
    let buffer = editor.buffer();
    let table = buffer.tag_table();
    let entry = editor.imp().annotations.entries.borrow_mut().remove(index);
    buffer.delete_mark(&entry.start_mark);
    buffer.delete_mark(&entry.end_mark);
    if let Some(tag) = table.lookup(&entry.tag_name) {
        table.remove(&tag);
    }
}

/// Resolve the current inclusive line range and iter pair for one live annotation.
#[must_use]
fn current_annotation_range(
    editor: &LushtextEditorPage,
    entry: &imp::LiveAnnotation,
) -> Option<(u32, u32, gtk4::TextIter, gtk4::TextIter)> {
    let buffer = editor.buffer();
    let start_iter = buffer.iter_at_mark(&entry.start_mark);
    let end_iter = buffer.iter_at_mark(&entry.end_mark);

    if start_iter.offset() == end_iter.offset() {
        return None;
    }

    let start_line = u32::try_from(start_iter.line()).ok()?;
    let end_line = if end_iter.starts_line() && end_iter.line() > start_iter.line() {
        u32::try_from(end_iter.line() - 1).ok()?
    } else {
        u32::try_from(end_iter.line()).ok()?
    };

    Some((start_line, end_line.max(start_line), start_iter, end_iter))
}

/// Resolve the currently selected inclusive line range, falling back to the cursor line.
#[must_use]
fn selected_line_range(editor: &LushtextEditorPage) -> (u32, u32) {
    let buffer = editor.buffer();
    let Some((mut start, mut end)) = buffer.selection_bounds() else {
        let line = editor.cursor_position().0;
        return (line, line);
    };

    if start.offset() > end.offset() {
        std::mem::swap(&mut start, &mut end);
    }

    let start_line = u32::try_from(start.line()).unwrap_or(0);
    let end_line = if end.starts_line() && end.offset() > start.offset() {
        u32::try_from(end.line().saturating_sub(1)).unwrap_or(start_line)
    } else {
        u32::try_from(end.line()).unwrap_or(start_line)
    };

    (start_line, end_line.max(start_line))
}

/// Return the iterator at `line`, clamping stale sidecar lines to the last line.
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

/// Return the exclusive end iterator for an inclusive annotation range.
#[must_use]
fn exclusive_end_iter_for_range(buffer: &sourceview5::Buffer, end_line: u32) -> gtk4::TextIter {
    let next_line = end_line.saturating_add(1);
    i32::try_from(next_line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or_else(|| buffer.end_iter())
}

/// Resolve the tag name used for a specific annotation ID.
#[must_use]
fn annotation_tag_name(annotation_id: &AnnotationId) -> String {
    format!("{ANNOTATION_TAG_PREFIX}{}", annotation_id.0)
}

/// Ensure the text-tag table contains the annotation highlight tag.
#[must_use]
fn ensure_annotation_tag(
    buffer: &sourceview5::Buffer,
    tag_name: &str,
    style: AnnotationStyle,
) -> gtk4::TextTag {
    if let Some(tag) = buffer.tag_table().lookup(tag_name) {
        tag
    } else {
        let tag = gtk4::TextTag::new(Some(tag_name));
        apply_annotation_tag_style(&tag, style, libadwaita::StyleManager::default().is_dark());
        buffer.tag_table().add(&tag);
        tag
    }
}

/// Apply theme-aware colors to one annotation highlight tag.
fn apply_annotation_tag_style(tag: &gtk4::TextTag, style: AnnotationStyle, is_dark: bool) {
    let background = match (style, is_dark) {
        (AnnotationStyle::Note, false) => "#f8efc8",
        (AnnotationStyle::Todo, false) => "#d9f1d6",
        (AnnotationStyle::Warning, false) => "#ffd9d1",
        (AnnotationStyle::Question, false) => "#dce7ff",
        (AnnotationStyle::Note, true) => "#5c4a1c",
        (AnnotationStyle::Todo, true) => "#204b2a",
        (AnnotationStyle::Warning, true) => "#5b2621",
        (AnnotationStyle::Question, true) => "#243a66",
    };
    tag.set_background(Some(background));
    tag.set_underline(gtk4::pango::Underline::Single);
}

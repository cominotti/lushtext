// SPDX-License-Identifier: GPL-3.0-or-later

//! Buffer text-flow primitives shared by the Markdown projection continuation.
//!
//! These are the small, stateless operations the projection loop composes:
//! inserting tagged text, keeping blockquote rails and row breaks correct,
//! tracking list-item and definition paragraph flow, flushing the delayed list
//! marker, numbering footnotes, and deriving an embedded block's text column.
//! They hold no continuation state of their own, so keeping them beside the
//! continuation rather than inside it leaves that module about one thing:
//! applying a batch to the state a generation carries between GTK turns.

use gtk4::prelude::*;
use pulldown_cmark::{Event, HeadingLevel, TagEnd};
use std::collections::HashMap;

use super::imp::{
    ALERT_BODY_LEFT_MARGIN, ALERT_BODY_RIGHT_MARGIN, DEFINITION_DEF_LEFT_MARGIN,
    DEFINITION_DEF_RIGHT_MARGIN, FOOTNOTE_DEF_LEFT_MARGIN, FOOTNOTE_DEF_RIGHT_MARGIN,
    TAG_ALERT_BODY, TAG_BLOCKQUOTE, TAG_FOOTNOTE_DEF, TAG_TASK_MARKER, blockquote_left_margin,
    blockquote_rail_prefix, list_item_text_margin,
};
use super::seams::{DefinitionRenderState, EmbeddedBlockLayout, ListFrame, ListItemRenderState};

/// Derive the effective text column for an embedded block from active Markdown state.
pub(super) fn embedded_block_layout(
    tag_stack: &[String],
    list_stack: &[ListFrame],
    list_item_stack: &[ListItemRenderState],
    generic_blockquote_depth: usize,
    definition_stack: &[DefinitionRenderState],
) -> EmbeddedBlockLayout {
    let mut layout = EmbeddedBlockLayout::default();

    if !definition_stack.is_empty() {
        layout.include_margin(DEFINITION_DEF_LEFT_MARGIN, DEFINITION_DEF_RIGHT_MARGIN);
    }

    if !list_item_stack.is_empty() {
        layout.include_margin(list_item_text_margin(list_stack.len().max(1)), 0);
    }

    if generic_blockquote_depth > 0 {
        layout.include_margin(blockquote_left_margin(generic_blockquote_depth), 0);
    }

    if tag_stack.iter().any(|tag| tag == TAG_ALERT_BODY) {
        layout.include_margin(ALERT_BODY_LEFT_MARGIN, ALERT_BODY_RIGHT_MARGIN);
    }

    if tag_stack.iter().any(|tag| tag == TAG_FOOTNOTE_DEF) {
        layout.include_margin(FOOTNOTE_DEF_LEFT_MARGIN, FOOTNOTE_DEF_RIGHT_MARGIN);
    }

    layout
}

/// Insert text at the given iter with the specified tag names applied.
pub(super) fn insert_with_tags(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    text: &str,
    tag_names: &[&str],
) {
    if tag_names.is_empty() {
        buffer.insert(iter, text);
        return;
    }

    let start_offset = iter.offset();
    buffer.insert(iter, text);
    let start = buffer.iter_at_offset(start_offset);

    for name in tag_names {
        if let Some(tag) = buffer.tag_table().lookup(name) {
            buffer.apply_tag(&tag, &start, iter);
        }
    }
}

/// Insert the visible generic blockquote rail when the next rendered content
/// starts a quoted line.
///
/// The rail carries only quote-structure tags so a line that starts with
/// emphasis or a link does not make the structural rail look like inline text.
pub(super) fn insert_blockquote_rail_if_needed(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    tag_stack: &[String],
    depth: usize,
) {
    if depth == 0 || !iter.starts_line() {
        return;
    }

    let tags: Vec<&str> = tag_stack
        .iter()
        .map(std::string::String::as_str)
        .filter(|name| *name == TAG_BLOCKQUOTE || name.starts_with("blockquote-depth-"))
        .collect();
    insert_with_tags(buffer, iter, &blockquote_rail_prefix(depth), &tags);
}

/// Insert one newline only when the current rendered position is mid-row.
pub(super) fn ensure_rendered_line_break(buffer: &gtk4::TextBuffer, iter: &mut gtk4::TextIter) {
    if iter.offset() > 0 && !iter.starts_line() {
        buffer.insert(iter, "\n");
    }
}

/// Mark the current list item as having emitted visible content.
pub(super) fn mark_current_list_item_content(items: &mut [ListItemRenderState]) {
    if let Some(item) = items.last_mut() {
        item.has_content = true;
    }
}

/// Record that a paragraph ended inside the current list item.
pub(super) fn mark_current_list_item_paragraph_end(items: &mut [ListItemRenderState]) {
    if let Some(item) = items.last_mut() {
        item.paragraph_ended = true;
    }
}

/// Clear the pending loose-list paragraph separator for the current item.
pub(super) fn clear_current_list_item_paragraph_end(items: &mut [ListItemRenderState]) {
    if let Some(item) = items.last_mut() {
        item.paragraph_ended = false;
    }
}

/// Return whether the next paragraph in this list item should be separated.
pub(super) fn current_list_item_needs_paragraph_separator(items: &[ListItemRenderState]) -> bool {
    items
        .last()
        .is_some_and(|item| item.has_content && item.paragraph_ended)
}

/// Mark the current definition as having emitted visible content.
pub(super) fn mark_current_definition_content(definitions: &mut [DefinitionRenderState]) {
    if let Some(definition) = definitions.last_mut() {
        definition.has_content = true;
    }
}

/// Record that a paragraph ended inside the current definition body.
pub(super) fn mark_current_definition_paragraph_end(definitions: &mut [DefinitionRenderState]) {
    if let Some(definition) = definitions.last_mut() {
        definition.paragraph_ended = true;
    }
}

/// Clear the pending loose-definition paragraph separator for the current body.
pub(super) fn clear_current_definition_paragraph_end(definitions: &mut [DefinitionRenderState]) {
    if let Some(definition) = definitions.last_mut() {
        definition.paragraph_ended = false;
    }
}

/// Return whether the next paragraph in this definition should be separated.
pub(super) fn current_definition_needs_paragraph_separator(
    definitions: &[DefinitionRenderState],
) -> bool {
    definitions
        .last()
        .is_some_and(|definition| definition.has_content && definition.paragraph_ended)
}

/// Return whether the current event should force any delayed list marker to be
/// inserted before the renderer processes the event itself.
pub(super) fn should_flush_pending_list_prefix(event: &Event<'_>) -> bool {
    !matches!(event, Event::TaskListMarker(_) | Event::End(TagEnd::Item))
}

/// Insert a delayed list marker using whatever formatting tags are active for
/// the current list item.
pub(super) fn flush_pending_list_prefix(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    tag_stack: &[String],
    pending_list_prefix: &mut Option<String>,
) -> bool {
    let Some(prefix) = pending_list_prefix.take() else {
        return false;
    };

    let tags: Vec<&str> = tag_stack.iter().map(std::string::String::as_str).collect();
    insert_with_tags(buffer, iter, &prefix, &tags);
    true
}

/// Insert the checked or unchecked marker for a task list item and clear the
/// delayed default bullet/number prefix for that item.
pub(super) fn insert_task_list_marker(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    tag_stack: &[String],
    pending_list_prefix: &mut Option<String>,
    checked: bool,
) {
    pending_list_prefix.take();

    let mut tags: Vec<&str> = tag_stack.iter().map(std::string::String::as_str).collect();
    tags.push(TAG_TASK_MARKER);
    let marker = if checked { "\u{2611} " } else { "\u{2610} " };
    insert_with_tags(buffer, iter, marker, &tags);
}

/// Assign or look up the stable preview-local number for one footnote label.
pub(super) fn footnote_number(
    footnote_numbers: &mut HashMap<String, usize>,
    next_footnote_number: &mut usize,
    label: &str,
) -> usize {
    if let Some(number) = footnote_numbers.get(label) {
        return *number;
    }

    let number = *next_footnote_number;
    *next_footnote_number += 1;
    footnote_numbers.insert(label.to_string(), number);
    number
}

/// Convert a `HeadingLevel` to a 0-based index for the tag name array.
pub(super) fn heading_level_to_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}

/// Pop the last tag from the stack. No-op if the stack is empty.
pub(super) fn pop_tag(stack: &mut Vec<String>) {
    stack.pop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_footnote_number_reuses_existing_labels() {
        let mut footnote_numbers = HashMap::new();
        let mut next_footnote_number = 1;

        assert_eq!(
            footnote_number(&mut footnote_numbers, &mut next_footnote_number, "alpha"),
            1
        );
        assert_eq!(
            footnote_number(&mut footnote_numbers, &mut next_footnote_number, "beta"),
            2
        );
        assert_eq!(
            footnote_number(&mut footnote_numbers, &mut next_footnote_number, "alpha"),
            1
        );
        assert_eq!(next_footnote_number, 3);
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The GTK half of the per-paragraph wrap gate (`explore-editing-long-
//! paragraphs`, design D3): paragraphs longer than `L_WRAP_EDIT` are shown
//! unwrapped, every other paragraph keeps the user's wrap setting.
//!
//! Cross-cutting with **no role**, beside the document scale watch
//! (`WFR-DOCUMENT-SCALE`): it owns no decision. The decision, with its
//! hysteresis, is `model::paragraph_wrap::paragraph_wrap`; this module only
//! keeps one tag on the paragraphs it names.
//!
//! A paragraph's wrap mode comes from the tags on its **first character**
//! (ledger A53), so a `wrap-mode` none tag there unwraps it. The gate tags the
//! **whole** paragraph, not only that character, because the tag is also the
//! hysteresis state: text inserted before the first character is untagged
//! (A25) and deleting the first characters deletes their tags, and either
//! would otherwise forget that a paragraph inside the hysteresis band was
//! unwrapped. A paragraph is unwrapped when any of its characters carries the
//! tag; the gate then re-tags the untagged prefix up to the first tagged
//! character, so the first character decides again. An undo re-inserts text
//! without tags, so both handlers re-check the paragraphs an edit touched in
//! the same `insert-text` or `delete-range` emission, before GTK's commit
//! handler lays the paragraph out (it does so once per keystroke, inside the
//! key press). They also run while an installation owns the buffer: the tag
//! must be on a long paragraph before its first layout. Tags change no text,
//! set no `modified`, and record no history (A27), so drafts, saves and undo
//! never see them.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::LushtextEditorPage;
use super::document_scale::{inserted_run_start, read_paragraph_chars};
use crate::model::paragraph_wrap::{ParagraphWrap, long_interior_paragraphs, paragraph_wrap};

/// The tag that unwraps a paragraph: the gate applies it to the whole
/// paragraph, and GTK reads it from the first character (ledger A53).
pub const NO_WRAP_TAG: &str = "lushtext-long-paragraph-no-wrap";

/// The no-wrap tag of `buffer`, created on first use.
fn no_wrap_tag(buffer: &sourceview5::Buffer) -> gtk4::TextTag {
    let table = buffer.tag_table();
    table.lookup(NO_WRAP_TAG).unwrap_or_else(|| {
        let tag = gtk4::TextTag::builder()
            .name(NO_WRAP_TAG)
            .wrap_mode(gtk4::WrapMode::None)
            .build();
        table.add(&tag);
        tag
    })
}

/// What the gate did to one paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WrapChange {
    None,
    Unwrapped,
    Rewrapped,
}

/// The first tagged character in `[start, end)`, if any (one B-tree toggle
/// search, not a character walk).
fn first_tagged(
    start: &gtk4::TextIter,
    end: &gtk4::TextIter,
    tag: &gtk4::TextTag,
) -> Option<gtk4::TextIter> {
    if start.has_tag(tag) {
        return Some(*start);
    }
    let mut toggle = *start;
    (toggle.forward_to_tag_toggle(Some(tag)) && toggle < *end).then_some(toggle)
}

/// Apply the decision to GTK line `line`: tag the paragraph when it must be
/// unwrapped (only its untagged prefix when part of it already carries the
/// tag), clear the tag from it when it may wrap.
fn gate_paragraph(buffer: &sourceview5::Buffer, line: i32) -> WrapChange {
    let Some(start) = buffer.iter_at_line(line) else {
        return WrapChange::None;
    };
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    let tag = no_wrap_tag(buffer);
    let tagged = first_tagged(&start, &end, &tag);
    let current = if tagged.is_some() {
        ParagraphWrap::Unwrapped
    } else {
        ParagraphWrap::FollowsView
    };
    let chars = read_paragraph_chars(buffer, u64::try_from(line).unwrap_or(0));
    match (tagged, paragraph_wrap(chars, current)) {
        (None, ParagraphWrap::Unwrapped) => {
            buffer.apply_tag(&tag, &start, &end);
            WrapChange::Unwrapped
        }
        (Some(first), ParagraphWrap::Unwrapped) => {
            if first != start {
                buffer.apply_tag(&tag, &start, &first);
            }
            WrapChange::None
        }
        (Some(_), ParagraphWrap::FollowsView) => {
            buffer.remove_tag(&tag, &start, &end);
            WrapChange::Rewrapped
        }
        (None, ParagraphWrap::FollowsView) => WrapChange::None,
    }
}

impl LushtextEditorPage {
    /// Install the gate's buffer handlers (from `constructed`).
    pub(super) fn setup_paragraph_wrap(&self) {
        let buffer = self.buffer();
        let signals = &self.imp().document_scale.signals;
        let editor_weak = self.downgrade();
        let id = buffer.connect_local("insert-text", true, move |values| {
            let editor = editor_weak.upgrade()?;
            let end = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            let run = values.get(2)?.get::<&str>().ok()?;
            let buffer = editor.buffer();
            let first = inserted_run_start(&buffer, &end, run).line();
            let last = end.line();
            editor.record_wrap_change(gate_paragraph(&buffer, first));
            if last != first {
                editor.record_wrap_change(gate_paragraph(&buffer, last));
            }
            // Paragraphs wholly inside the run are new and untagged: only one
            // longer than the threshold needs the gate, and the run's own
            // text tells which, so an installation of many short lines reads
            // no line from the buffer.
            for line in long_interior_paragraphs(run) {
                if let Some(line) = first.checked_add(line).filter(|line| *line < last) {
                    editor.record_wrap_change(gate_paragraph(&buffer, line));
                }
            }
            None
        });
        signals.track(&buffer, id);
        let editor_weak = self.downgrade();
        let id = buffer.connect_local("delete-range", true, move |values| {
            let editor = editor_weak.upgrade()?;
            let start = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            let buffer = editor.buffer();
            editor.record_wrap_change(gate_paragraph(&buffer, start.line()));
            None
        });
        signals.track(&buffer, id);
    }

    fn record_wrap_change(&self, change: WrapChange) {
        let state = &self.imp().document_scale;
        match change {
            WrapChange::Unwrapped => state.record_unwrap(),
            WrapChange::Rewrapped => state.record_rewrap(),
            WrapChange::None => {}
        }
    }
}

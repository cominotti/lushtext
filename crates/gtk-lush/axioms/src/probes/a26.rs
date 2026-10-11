// SPDX-License-Identifier: MIT OR Apache-2.0

//! A26: `GtkTextBuffer` computes a paragraph's log attrs (word, cursor and
//! sentence boundaries) over the **whole logical paragraph, hidden text
//! included**. A word that starts in a visible head and continues into an
//! `invisible` run ends inside the hidden run: `forward_word_end` from the
//! word's start lands past the head.
//!
//! The other half — any character change anywhere in the buffer invalidates
//! the cached attrs, so the next cursor movement recomputes the whole
//! paragraph — has no structural observation (the cache is private). It is
//! recorded as cost evidence only: the first, repeated, and after-an-edit
//! durations of one word movement inside a long paragraph.
//!
//! The protected view (C2) makes giant paragraphs caret-free for this reason:
//! folding does not bound cursor, word or Backspace cost.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, invisible_tag, offset, paragraph, time_us};
use crate::{AxiomId, Observation};

/// The visible head, ending in the first half of the spanning word.
pub const HEAD: &str = "visible head ends with abc";
/// The hidden run, starting with the second half of the spanning word.
pub const HIDDEN: &str = "def and hidden words after it";
/// Characters of the long paragraph the cost is recorded on.
pub const LONG_CHARS: usize = 300_000;

/// A buffer holding [`HEAD`] then [`HIDDEN`] (tagged invisible) on one line.
#[must_use]
pub fn spanning_word_buffer() -> gtk4::TextBuffer {
    let buffer = gtk4::TextBuffer::new(None);
    let tag = invisible_tag(&buffer);
    buffer.set_text(&format!("{HEAD}{HIDDEN}"));
    let start = buffer.iter_at_offset(offset(HEAD.len()));
    let end = buffer.end_iter();
    buffer.apply_tag(&tag, &start, &end);
    buffer
}

/// Offset of the spanning word's first character.
#[must_use]
pub fn spanning_word_start() -> i32 {
    offset(HEAD.len() - "abc".len())
}

/// Microseconds of one `forward_word_end` from the middle of a
/// `LONG_CHARS`-character paragraph: first call, repeated call, and the call
/// after a one-character edit on another line.
#[must_use]
pub fn word_movement_costs() -> (u128, u128, u128) {
    let buffer = gtk4::TextBuffer::new(None);
    buffer.set_text(&format!(
        "short first line\n{}",
        paragraph(ParagraphKind::Ascii, LONG_CHARS, 26)
    ));
    let middle = offset("short first line\n".len() + LONG_CHARS / 2);
    let step = |buffer: &gtk4::TextBuffer| {
        let mut iter = buffer.iter_at_offset(middle);
        time_us(|| iter.forward_word_end()).1
    };
    let first = step(&buffer);
    let repeat = step(&buffer);
    let mut elsewhere = buffer.start_iter();
    buffer.insert(&mut elsewhere, "x");
    let after_edit = step(&buffer);
    (first, repeat, after_edit)
}

/// Probe A26.
#[must_use]
pub fn probe_a26() -> Observation {
    Recorder::run(AxiomId::new(26), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let buffer = spanning_word_buffer();
        let head_end = offset(HEAD.len());
        let hidden_end = buffer.char_count();
        let mut word = buffer.iter_at_offset(spanning_word_start());
        let moved = word.forward_word_end();
        recorder.measure("head_end_offset", head_end);
        recorder.measure("word_end_offset", word.offset());
        let mut visible_word = buffer.iter_at_offset(spanning_word_start());
        visible_word.forward_visible_word_end();
        recorder.measure("visible_word_end_offset", visible_word.offset());
        recorder.control(moved, "control: forward_word_end found a word end")?;

        let (first, repeat, after_edit) = word_movement_costs();
        recorder.measure("long_paragraph_chars", LONG_CHARS);
        recorder.measure("word_end_first_call_us", first);
        recorder.measure("word_end_repeat_call_us", repeat);
        recorder.measure("word_end_after_edit_elsewhere_us", after_edit);

        recorder.axiom(
            word.offset() > head_end && word.offset() < hidden_end,
            "a word spanning the visible/hidden boundary ends inside the hidden run",
        )
    })
}

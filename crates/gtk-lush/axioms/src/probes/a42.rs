// SPDX-License-Identifier: MIT OR Apache-2.0

//! A42, two parts. **Fresh text:** text inserted whole into a
//! `GtkTextBuffer` gets exactly Pango's paragraphs as lines: a new line starts
//! after `\n`, after `\r`, after `\r\n` (one delimiter of two characters)
//! and after U+2029 PARAGRAPH SEPARATOR, and nowhere else (U+2028 and U+0085
//! are ordinary characters, as they are to `pango_find_paragraph_boundary`).
//! A line's `chars_in_line` counts its delimiter. **Edits:** the line
//! structure is history-dependent. An insertion splits only its own inserted
//! text at Pango's delimiters, never joining with or splitting around the
//! neighbouring characters (a `\n` inserted after a buffer `\r` starts a line
//! of its own; text inserted between `\r` and `\n` stays on one line), and a
//! deletion joins exactly the lines whose starts it removes (a line start `x`
//! is removed when `start < x <= end`), so `aa\r\nbb` reached by deleting the
//! `X` of `aa\rX\nbb` has three lines.
//!
//! The document scale policy (C1) slices installs, profiles paragraphs, and
//! watches edits with one boundary function written against this rule
//! (`model/paragraph_boundary.rs`).

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::{AxiomId, Observation};

/// The labelled texts the probe splits both ways.
pub const CASES: &[(&str, &str)] = &[
    ("lf", "ab\ncd"),
    ("cr", "ab\rcd"),
    ("crlf", "ab\r\ncd"),
    ("u2029", "ab\u{2029}cd"),
    ("u2028", "ab\u{2028}cd"),
    ("u0085", "ab\u{85}cd"),
    ("mixed", "a\rb\r\nc\nd\u{2029}e\u{2028}f\u{85}g\r"),
    ("trailing_crlf", "\r\n\r\n"),
];

/// One edit applied after `set_text`, in character offsets.
#[derive(Clone, Copy, Debug)]
pub enum Edit {
    /// Insert the text at the offset.
    Insert(i32, &'static str),
    /// Delete the half-open character range.
    Delete(i32, i32),
}

/// The labelled edit cases: initial text, edit, and the expected line texts
/// (each with its delimiter).
pub const EDIT_CASES: &[(&str, &str, Edit, &[&str])] = &[
    (
        "delete_between_cr_lf",
        "aa\rX\nbb",
        Edit::Delete(3, 4),
        &["aa\r", "\n", "bb"],
    ),
    (
        "insert_between_cr_lf",
        "aa\r\nbb",
        Edit::Insert(3, "Y"),
        &["aa\rY\n", "bb"],
    ),
    (
        "insert_lf_after_cr",
        "ab\rcd",
        Edit::Insert(3, "\nxy"),
        &["ab\r", "\n", "xycd"],
    ),
    (
        "insert_cr_before_lf",
        "ab\ncd",
        Edit::Insert(2, "q\r"),
        &["abq\r", "\n", "cd"],
    ),
    (
        "delete_lf_of_crlf",
        "aa\r\nbb",
        Edit::Delete(3, 4),
        &["aa\rbb"],
    ),
    (
        "delete_cr_of_crlf",
        "aa\r\nbb",
        Edit::Delete(2, 3),
        &["aa\n", "bb"],
    ),
    (
        "append_lf_after_cr",
        "ab\r",
        Edit::Insert(3, "\n"),
        &["ab\r", "\n", ""],
    ),
    (
        "insert_whole_paragraphs",
        "ab",
        Edit::Insert(1, "1\r\n2\u{2029}3"),
        &["a1\r\n", "2\u{2029}", "3b"],
    ),
];

/// The buffer `initial` becomes after `edit`.
#[must_use]
pub fn edited_buffer(initial: &str, edit: Edit) -> gtk4::TextBuffer {
    let buffer = gtk4::TextBuffer::new(None);
    buffer.set_text(initial);
    match edit {
        Edit::Insert(offset, text) => {
            let mut iter = buffer.iter_at_offset(offset);
            buffer.insert(&mut iter, text);
        }
        Edit::Delete(start, end) => {
            let (mut start, mut end) = (buffer.iter_at_offset(start), buffer.iter_at_offset(end));
            buffer.delete(&mut start, &mut end);
        }
    }
    buffer
}

/// Each line's text (delimiter included) and `chars_in_line`.
#[must_use]
pub fn line_texts(buffer: &gtk4::TextBuffer) -> Vec<(String, i32)> {
    (0..buffer.line_count())
        .filter_map(|line| {
            let start = buffer.iter_at_line(line)?;
            let end = buffer
                .iter_at_line(line + 1)
                .unwrap_or_else(|| buffer.end_iter());
            Some((
                buffer.text(&start, &end, true).to_string(),
                start.chars_in_line(),
            ))
        })
        .collect()
}

/// Byte offsets where Pango starts each paragraph of `text` (0 first).
#[must_use]
pub fn pango_paragraph_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    let mut position = 0;
    while position <= text.len() {
        let (delimiter, next) = pango::find_paragraph_boundary(&text[position..]);
        if delimiter >= next {
            break;
        }
        position += usize::try_from(next).unwrap_or(0);
        starts.push(position);
    }
    starts
}

/// Byte offsets where a `GtkTextBuffer` holding `text` starts each line, and
/// each line's `chars_in_line`.
#[must_use]
pub fn buffer_line_starts(text: &str) -> (Vec<usize>, Vec<i32>) {
    let buffer = gtk4::TextBuffer::new(None);
    buffer.set_text(text);
    let mut starts = Vec::new();
    let mut chars = Vec::new();
    for line in 0..buffer.line_count() {
        let Some(iter) = buffer.iter_at_line(line) else {
            break;
        };
        let char_offset = usize::try_from(iter.offset()).unwrap_or(0);
        starts.push(
            text.char_indices()
                .nth(char_offset)
                .map_or(text.len(), |(byte, _)| byte),
        );
        chars.push(iter.chars_in_line());
    }
    (starts, chars)
}

/// Probe A42.
#[must_use]
pub fn probe_a42() -> Observation {
    Recorder::run(AxiomId::new(42), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let mut agree = true;
        let mut lines = Vec::new();
        let mut chars = Vec::new();
        for (label, text) in CASES {
            let pango = pango_paragraph_starts(text);
            let (buffer, in_line) = buffer_line_starts(text);
            agree &= pango == buffer;
            lines.push(format!("{label}: gtk {buffer:?} pango {pango:?}"));
            chars.push(format!("{label}: {in_line:?}"));
        }
        recorder.measure("line_starts", super::join_entries(&lines));
        recorder.measure("chars_in_line", super::join_entries(&chars));
        let (lf_starts, _) = buffer_line_starts("ab\ncd");
        recorder.control(
            lf_starts == [0, 3],
            "control: a buffer splits ab\\ncd into two lines",
        )?;
        recorder.axiom(
            agree,
            "every buffer line starts where Pango starts a paragraph, and nowhere else",
        )?;
        let first_line = |text: &str| buffer_line_starts(text).1.first().copied();
        recorder.axiom(
            first_line("ab\ncd") == Some(3)
                && first_line("ab\rcd") == Some(3)
                && first_line("ab\r\ncd") == Some(4)
                && first_line("ab\u{2029}cd") == Some(3),
            "chars_in_line counts the delimiter: one for \\n, \\r and U+2029, two for \\r\\n",
        )?;
        recorder.axiom(
            buffer_line_starts("ab\u{2028}cd").0.len() == 1
                && buffer_line_starts("ab\u{85}cd").0.len() == 1,
            "U+2028 and U+0085 do not end a line",
        )?;

        let mut edits_agree = true;
        let mut edited = Vec::new();
        for (label, initial, edit, expected) in EDIT_CASES {
            let lines = line_texts(&edited_buffer(initial, *edit));
            let chars_match = lines
                .iter()
                .all(|(text, chars)| usize::try_from(*chars) == Ok(text.chars().count()));
            let texts: Vec<&str> = lines.iter().map(|(text, _)| text.as_str()).collect();
            edits_agree &= chars_match && texts == *expected;
            let chars: Vec<i32> = lines.iter().map(|line| line.1).collect();
            edited.push(format!("{label}: {texts:?} chars {chars:?}"));
        }
        recorder.measure("edited_lines", super::join_entries(&edited));
        recorder.axiom(
            edits_agree,
            "an insertion splits only its own text, and a deletion joins exactly the lines whose \
             starts it removes, never re-deriving the neighbours' delimiters",
        )
    })
}

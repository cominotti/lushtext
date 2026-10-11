// SPDX-License-Identifier: MIT OR Apache-2.0

//! A29: the GtkSourceView context engine analyses a line **atomically**, in
//! one call, with a fixed **2000 ms** cut-off. A line whose analysis runs past
//! it makes the engine log a `GtkSourceView-CRITICAL` ("Highlighting a single
//! line took too much time, syntax highlighting will be disabled") from
//! inside that one call, and turn highlighting **off for the whole buffer**:
//! a short line added afterwards gets no highlighting. The engine also
//! **reads hidden text**: on a short line, highlight tags land inside a range
//! tagged `invisible`.
//!
//! Whether a given line reaches 2000 ms depends on the runner's speed, so the
//! probe doubles a minified-markup line until the cut-off fires, up to a cap;
//! reaching the cap without it is `FixtureInvalid`, never `Violated`. The
//! line length that reached it and the call's duration are recorded values.
//!
//! The probe runs for a few seconds (the cut-off itself is 2000 ms), far
//! below the pull-request probe job's timeout, so it is not marked slow.
//!
//! The document scale policy (C1) turns syntax off before the engine sees a
//! giant paragraph; folding (C2) cannot hide text from it.

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{
    LogCapture, ParagraphKind, SourceViewFixture, invisible_tag, offset, paragraph, time_us,
};
use crate::{AxiomId, Observation};

/// The line length the doubling starts at.
pub const FIRST_LENGTH: usize = 4_096;
/// The longest line the doubling tries before calling the fixture invalid.
pub const LENGTH_CAP: usize = 1 << 24;
/// The text of the cut-off's critical message.
pub const CUT_OFF_MESSAGE: &str = "Highlighting a single line took too much time";
/// The short line the hidden-text check highlights: a visible element, then
/// an element inside the hidden range.
pub const HIDDEN_LINE: (&str, &str) = ("<b class=\"v\">visible</b>", "<i class=\"h\">hidden</i>");

/// The highlighting tags (every tag but `except`) at `offset` in `buffer`.
#[must_use]
pub fn highlight_tags_at(
    buffer: &sourceview5::Buffer,
    offset: i32,
    except: Option<&gtk4::TextTag>,
) -> Vec<String> {
    buffer
        .iter_at_offset(offset)
        .tags()
        .into_iter()
        .filter(|tag| except != Some(tag))
        .map(|tag| {
            tag.name()
                .map_or_else(|| "(anonymous)".to_owned(), |name| name.to_string())
        })
        .collect()
}

/// One highlighting attempt on a fresh buffer holding one minified-markup
/// line of `chars` characters.
#[derive(Clone, Debug)]
pub struct CutOffAttempt {
    /// The line length.
    pub chars: usize,
    /// How long the single `ensure_highlight` call took.
    pub micros: u128,
    /// Whether the call logged the cut-off critical.
    pub cut_off: bool,
    /// Highlight tags on a short line added after the call.
    pub later_line_tags: Vec<String>,
    /// Every message the call logged.
    pub messages: Vec<String>,
}

/// Highlight one `chars`-character minified-markup line in one call, then a
/// short line added after it.
#[must_use]
pub fn highlight_attempt(chars: usize) -> Option<CutOffAttempt> {
    let fixture = SourceViewFixture::html(gtk4::WrapMode::None)?;
    let buffer = fixture.buffer;
    buffer.set_text(&paragraph(ParagraphKind::MinifiedMarkup, chars, 29));
    let capture = LogCapture::start();
    let ((), micros) = time_us(|| {
        let (start, end) = buffer.bounds();
        buffer.ensure_highlight(&start, &end);
    });
    let messages = capture.messages();
    let cut_off = messages
        .iter()
        .any(|message| message.level == "critical" && message.message.contains(CUT_OFF_MESSAGE));
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, "\n<b class=\"x\">later</b>");
    let line_start = buffer.char_count() - 20;
    let (start, end) = buffer.bounds();
    buffer.ensure_highlight(&start, &end);
    let later_line_tags = highlight_tags_at(&buffer, line_start + 2, None);
    drop(capture);
    Some(CutOffAttempt {
        chars,
        micros,
        cut_off,
        later_line_tags,
        messages: messages.iter().map(ToString::to_string).collect(),
    })
}

/// Probe A29.
#[must_use]
pub fn probe_a29() -> Observation {
    Recorder::run(AxiomId::new(29), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = recorder.require(
            SourceViewFixture::html(gtk4::WrapMode::None),
            "control: GtkSourceView has an html language",
        )?;
        let buffer = &fixture.buffer;
        let tag = invisible_tag(buffer.upcast_ref());
        let (visible, hidden) = HIDDEN_LINE;
        buffer.set_text(&format!("{visible}{hidden}"));
        let hidden_start = offset(visible.chars().count());
        let start = buffer.iter_at_offset(hidden_start);
        buffer.apply_tag(&tag, &start, &buffer.end_iter());
        let (start, end) = buffer.bounds();
        buffer.ensure_highlight(&start, &end);
        let visible_tags = highlight_tags_at(buffer, 1, Some(&tag));
        let hidden_tags = highlight_tags_at(buffer, hidden_start + 1, Some(&tag));
        recorder.measure("visible_element_tags", super::join_entries(&visible_tags));
        recorder.measure("hidden_element_tags", super::join_entries(&hidden_tags));
        recorder.control(
            !visible_tags.is_empty(),
            "control: the engine highlights a visible element",
        )?;
        recorder.axiom(
            !hidden_tags.is_empty(),
            "the engine highlights text inside an invisible range",
        )?;

        let mut chars = FIRST_LENGTH;
        let mut attempts = Vec::new();
        let reached = loop {
            let attempt = recorder.require(
                highlight_attempt(chars),
                "control: GtkSourceView has an html language",
            )?;
            attempts.push(format!(
                "{}: {} us{}",
                attempt.chars,
                attempt.micros,
                if attempt.cut_off { " cut-off" } else { "" }
            ));
            if attempt.cut_off || chars >= LENGTH_CAP {
                break attempt;
            }
            chars = chars.saturating_mul(2);
        };
        recorder.measure("attempts", super::join_entries(&attempts));
        recorder.measure("cut_off_line_chars", reached.chars);
        recorder.measure("cut_off_call_us", reached.micros);
        recorder.measure("captured_messages", super::join_entries(&reached.messages));
        recorder.measure(
            "later_line_tags",
            super::join_entries(&reached.later_line_tags),
        );
        recorder.control(
            reached.cut_off,
            "control: one analysis call reached the 2000 ms cut-off below the length cap",
        )?;
        recorder.axiom(
            reached.later_line_tags.is_empty(),
            "after the cut-off, a short line added later gets no highlighting",
        )
    })
}

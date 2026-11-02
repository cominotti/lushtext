// SPDX-License-Identifier: MIT OR Apache-2.0

//! A53: a paragraph's **wrap mode** is a paragraph value that `GtkTextLayout`
//! takes from the tags on the paragraph's **first character**: a tag with
//! `wrap-mode` none on that one character lays the whole paragraph out
//! unwrapped in a word-wrapping view, while the paragraph next to it keeps
//! wrapping. Text inserted **before** that character does not carry the tag
//! (A25), so the paragraph wraps again until the tag is re-applied.
//!
//! LushText turns wrapping off for paragraphs longer than `L_wrap_edit` by
//! tagging their first character (`explore-editing-long-paragraphs`, design
//! D3): an unwrapped layout of a long paragraph costs about half a wrapped
//! one, and every keystroke lays the edited paragraph out once.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, TextViewFixture, paragraph};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters in each of the two paragraphs.
pub const PARAGRAPH_CHARS: usize = 2_000;

/// A word-wrapping view of two long paragraphs and a no-wrap tag.
#[derive(Clone, Debug)]
pub struct FirstCharacterWrap {
    /// The view, buffer and scroller.
    pub fixture: TextViewFixture,
    /// A tag whose only property is `wrap-mode` none.
    pub no_wrap: gtk4::TextTag,
}

impl FirstCharacterWrap {
    /// Two paragraphs of [`PARAGRAPH_CHARS`] characters, untagged.
    #[must_use]
    pub fn new() -> Self {
        let fixture = TextViewFixture::new(gtk4::WrapMode::Word);
        fixture.buffer.set_text(&format!(
            "{}\n{}\n",
            paragraph(ParagraphKind::Ascii, PARAGRAPH_CHARS, 530),
            paragraph(ParagraphKind::Ascii, PARAGRAPH_CHARS, 531)
        ));
        let no_wrap = gtk4::TextTag::builder()
            .name("a53-no-wrap")
            .wrap_mode(gtk4::WrapMode::None)
            .build();
        fixture.buffer.tag_table().add(&no_wrap);
        Self { fixture, no_wrap }
    }

    /// Tag the first character of the first paragraph only.
    pub fn tag_first_character(&self) {
        let start = self.fixture.buffer.start_iter();
        let mut next = start;
        next.forward_char();
        self.fixture.buffer.apply_tag(&self.no_wrap, &start, &next);
    }

    /// Insert one character before the first paragraph's first character.
    pub fn insert_at_paragraph_start(&self) {
        let mut start = self.fixture.buffer.start_iter();
        self.fixture.buffer.insert(&mut start, "x");
    }

    /// The height the view records for paragraph `line` (0 or 1).
    #[must_use]
    pub fn paragraph_height(&self, line: i32) -> i32 {
        self.fixture
            .buffer
            .iter_at_line(line)
            .map_or(0, |iter| self.fixture.view.line_yrange(&iter).1)
    }
}

impl Default for FirstCharacterWrap {
    fn default() -> Self {
        Self::new()
    }
}

/// Probe A53.
#[must_use]
pub fn probe_a53() -> Observation {
    Recorder::run(AxiomId::new(53), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = FirstCharacterWrap::new();
        let _shown = Presented::checked(
            recorder,
            &fixture.fixture.scrolled,
            "control: the word-wrapping view was presented",
        )?;
        settle(LAYOUT_SETTLE);
        let wrapped_first = fixture.paragraph_height(0);
        let wrapped_second = fixture.paragraph_height(1);
        recorder.measure(
            "wrapped_heights",
            format!("{wrapped_first} / {wrapped_second}"),
        );
        recorder.control(
            wrapped_first > 0 && wrapped_second == wrapped_first,
            "control: both paragraphs wrap to the same height untagged",
        )?;
        fixture.tag_first_character();
        settle(LAYOUT_SETTLE);
        let tagged_first = fixture.paragraph_height(0);
        let tagged_second = fixture.paragraph_height(1);
        recorder.measure(
            "tagged_heights",
            format!("{tagged_first} / {tagged_second}"),
        );
        recorder.axiom(
            tagged_first < wrapped_first && tagged_second == wrapped_second,
            "a wrap-mode tag on a paragraph's first character unwraps that whole paragraph only",
        )?;
        fixture.insert_at_paragraph_start();
        settle(LAYOUT_SETTLE);
        let after_insert = fixture.paragraph_height(0);
        recorder.measure("height_after_insert_at_start", after_insert);
        recorder.axiom(
            after_insert > tagged_first,
            "text inserted before the tagged first character is untagged, so the paragraph wraps again",
        )
    })
}

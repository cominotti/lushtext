// SPDX-License-Identifier: MIT OR Apache-2.0

//! A24: text tagged `invisible` is left out of a paragraph's layout. A hidden
//! tail adds no display height and no x-advance: the paragraph's height and
//! the location of its last visible character equal those of a buffer that
//! holds only the visible head.
//!
//! Recorded beside it, as values only: how long applying and removing the
//! `invisible` tag over a long tail takes, including the re-layout the next
//! location query forces. Tag changes re-lay out the whole paragraph.
//!
//! The protected view (C2) folds a giant paragraph for display by tagging all
//! but a bounded head invisible; this is why that bounds layout cost.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, TextViewFixture, invisible_tag, paragraph, time_us};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters in the visible head.
pub const HEAD_CHARS: usize = 2_000;
/// Characters in the hidden tail.
pub const TAIL_CHARS: usize = 200_000;
/// The paragraph seed.
pub const SEED: u64 = 24;

/// What the view reports for the visible head.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeadGeometry {
    /// `line_yrange` height of the paragraph.
    pub paragraph_height: i32,
    /// `iter_location` of the head's last character: x, y, width, height.
    pub head_end: (i32, i32, i32, i32),
}

/// The geometry of `fixture`'s first paragraph, read at its head's last
/// character.
#[must_use]
pub fn head_geometry(fixture: &TextViewFixture) -> HeadGeometry {
    let start = fixture.buffer.start_iter();
    let (_, paragraph_height) = fixture.view.line_yrange(&start);
    let head_end = fixture
        .buffer
        .iter_at_offset(i32::try_from(HEAD_CHARS - 1).unwrap_or(i32::MAX));
    let rect = fixture.view.iter_location(&head_end);
    HeadGeometry {
        paragraph_height,
        head_end: (rect.x(), rect.y(), rect.width(), rect.height()),
    }
}

/// Load the head followed by the tail into `fixture` and hide the tail with
/// `tag` (`None` loads the head alone). Returns the hide's duration in
/// microseconds, including the re-layout the next location query forces.
#[must_use]
pub fn load(fixture: &TextViewFixture, tag: Option<&gtk4::TextTag>) -> u128 {
    let text = paragraph(ParagraphKind::Ascii, HEAD_CHARS + TAIL_CHARS, SEED);
    match tag {
        None => {
            let head: String = text.chars().take(HEAD_CHARS).collect();
            fixture.buffer.set_text(&head);
            0
        }
        Some(tag) => {
            fixture.buffer.set_text(&text);
            let _ = head_geometry(fixture);
            let (start, end) = tail_bounds(fixture);
            let ((), micros) = time_us(|| {
                fixture.buffer.apply_tag(tag, &start, &end);
                let _ = head_geometry(fixture);
            });
            micros
        }
    }
}

/// The tail's bounds in `fixture`'s buffer.
#[must_use]
pub fn tail_bounds(fixture: &TextViewFixture) -> (gtk4::TextIter, gtk4::TextIter) {
    let start = fixture
        .buffer
        .iter_at_offset(i32::try_from(HEAD_CHARS).unwrap_or(i32::MAX));
    (start, fixture.buffer.end_iter())
}

/// Probe A24.
#[must_use]
pub fn probe_a24() -> Observation {
    Recorder::run(AxiomId::new(24), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
        let tag = invisible_tag(&fixture.buffer);
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;

        let _: u128 = load(&fixture, None);
        settle(LAYOUT_SETTLE);
        let head_only = head_geometry(&fixture);
        recorder.measure("head_only_paragraph_height", head_only.paragraph_height);
        recorder.measure("head_only_head_end", format!("{:?}", head_only.head_end));

        let apply_us = load(&fixture, Some(&tag));
        settle(LAYOUT_SETTLE);
        let hidden = head_geometry(&fixture);
        recorder.measure("hidden_tail_paragraph_height", hidden.paragraph_height);
        recorder.measure("hidden_tail_head_end", format!("{:?}", hidden.head_end));
        recorder.measure("tag_apply_relayout_us", apply_us);

        let (start, end) = tail_bounds(&fixture);
        let ((), remove_us) = time_us(|| {
            fixture.buffer.remove_tag(&tag, &start, &end);
            let _ = head_geometry(&fixture);
        });
        recorder.measure("tag_remove_relayout_us", remove_us);
        settle(LAYOUT_SETTLE);
        let shown_tail = head_geometry(&fixture);
        recorder.measure("shown_tail_paragraph_height", shown_tail.paragraph_height);

        recorder.control(
            head_only.paragraph_height > 0
                && shown_tail.paragraph_height > hidden.paragraph_height.saturating_mul(10),
            "control: showing the tail makes the paragraph far taller than the head alone",
        )?;
        recorder.axiom(
            hidden.paragraph_height == head_only.paragraph_height,
            "a hidden tail adds no display height",
        )?;
        recorder.axiom(
            hidden.head_end == head_only.head_end,
            "the head's geometry equals that of an untagged head-only buffer",
        )
    })
}

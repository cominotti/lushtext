// SPDX-License-Identifier: MIT OR Apache-2.0

//! A28: Pango measures a line in `int32` Pango units (1/1024 px), so a single
//! unwrapped line wider than `i32::MAX` units (about 2,097,152 px) overflows.
//! In a monospace font the x position of a character stops growing at the
//! overflow length and wraps around — non-monotone, negative — past it. The
//! overflow length is `i32::MAX / advance` characters for the font's glyph
//! advance, about 238k characters for a 9 px advance.
//!
//! The document scale policy (C1) treats wrap-off giant paragraphs past this
//! length as unrenderable, whatever their layout cost.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{TextViewFixture, time_us};
use crate::session::Presented;
use crate::{AxiomId, Observation};

/// Characters in the line the glyph advance is measured on.
pub const ADVANCE_SAMPLE_CHARS: usize = 1_000;
/// Characters past the predicted overflow length the long line runs to.
pub const PAST_OVERFLOW_CHARS: usize = 20_000;

/// The monospace glyph advance of `view`'s font, in Pango units per
/// character, and the line length at which a line's width first exceeds
/// `i32::MAX` units.
#[must_use]
pub fn predicted_overflow_length(view: &gtk4::TextView) -> (i64, usize) {
    let sample = "a".repeat(ADVANCE_SAMPLE_CHARS);
    let layout = view.create_pango_layout(Some(&sample));
    let (_, logical) = layout.extents();
    let total = i64::from(logical.width());
    let advance = total / ADVANCE_SAMPLE_CHARS as i64;
    let length = if advance > 0 {
        usize::try_from(i64::from(i32::MAX) / advance + 1).unwrap_or(usize::MAX)
    } else {
        0
    };
    (advance, length)
}

/// The x position, in Pango units, of each character index in `indexes` on
/// one unwrapped line of `chars` monospace characters laid out with `view`'s
/// font, and the layout time in microseconds.
#[must_use]
pub fn x_positions(view: &gtk4::TextView, chars: usize, indexes: &[usize]) -> (Vec<i32>, u128) {
    let text = "a".repeat(chars);
    let layout = view.create_pango_layout(Some(&text));
    let (_, micros) = time_us(|| layout.line_count());
    let positions = indexes
        .iter()
        .map(|index| {
            layout
                .index_to_pos(i32::try_from(*index).unwrap_or(i32::MAX))
                .x()
        })
        .collect();
    (positions, micros)
}

/// Probe A28.
#[must_use]
pub fn probe_a28() -> Observation {
    Recorder::run(AxiomId::new(28), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = TextViewFixture::new(gtk4::WrapMode::None);
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the monospace text view was presented",
        )?;
        let (advance, overflow) = predicted_overflow_length(&fixture.view);
        recorder.measure("glyph_advance_units", advance);
        recorder.measure("predicted_overflow_chars", overflow);
        recorder.control(
            advance > 0 && overflow > 1_000,
            "control: the font has a positive monospace advance",
        )?;

        let below = overflow - 1_000;
        let indexes = [below, overflow - 2, overflow - 1, overflow, overflow + 1];
        let (positions, micros) =
            x_positions(&fixture.view, overflow + PAST_OVERFLOW_CHARS, &indexes);
        recorder.measure("layout_us", micros);
        recorder.measure(
            "x_units_at_indexes",
            super::join_entries(
                indexes
                    .iter()
                    .zip(&positions)
                    .map(|(index, x)| format!("{index}: {x}")),
            ),
        );
        let first_wrap = indexes
            .iter()
            .zip(positions.array_windows::<2>())
            .find(|(_, [left, right])| right < left)
            .map(|(index, _)| index + 1);
        recorder.measure(
            "measured_overflow_index",
            first_wrap.map_or_else(|| "none".to_owned(), |index| index.to_string()),
        );
        let expected_below = i64::try_from(below).unwrap_or(i64::MAX) * advance;
        recorder.control(
            (i64::from(positions[0]) - expected_below).abs() <= advance,
            "control: below the overflow length x grows by one advance per character",
        )?;
        recorder.axiom(
            positions
                .array_windows::<2>()
                .any(|[left, right]| right < left)
                || positions.iter().any(|x| *x < 0),
            "past the overflow length the x position is non-monotone or negative",
        )
    })
}

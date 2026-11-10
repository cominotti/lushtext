// SPDX-License-Identifier: MIT OR Apache-2.0

//! A107: what one arrow-key step costs on a long display line, and when the
//! cheap step lands at the same place.
//!
//! The Left and Right keys emit `move-cursor` with
//! `GTK_MOVEMENT_VISUAL_POSITIONS`, which `GtkTextView` answers through
//! `gtk_text_layout_move_iter_visually` and Pango's
//! `pango_layout_move_cursor_visually`. That function lists every cursor
//! position of the display line (`pango_layout_line_get_cursors`), each found
//! by its own extents pass over the line, so **one visual step costs time
//! quadratic in the display line's length**: on a 10,000-character unwrapped
//! line about a second per key press. A `GTK_MOVEMENT_LOGICAL_POSITIONS` step
//! moves the iter by cursor positions in the buffer (log attributes, A26)
//! and lays out nothing.
//!
//! In a paragraph of **left-to-right text only**, the two steps land on the
//! same iter, including when a selection collapses to its edge. In a
//! paragraph holding a right-to-left run they differ: inside the run a
//! visual step to the right moves backwards in the buffer.
//!
//! LushText answers a visual step with a logical one when the cursor's
//! paragraph holds no right-to-left text (`ui/editor_page/cursor_steps.rs`):
//! the C5 wrap gate shows paragraphs longer than `L_WRAP_EDIT` unwrapped, so
//! one such paragraph is one display line.

use std::time::Instant;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, TextViewFixture, paragraph, record_text_toolkit};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters of the short line.
pub const SHORT_CHARS: usize = 2_500;
/// Characters of the long line: four times the short one.
pub const LONG_CHARS: usize = 10_000;
/// The offset the cursor starts from.
pub const START_OFFSET: i32 = 5;
/// How much slower the long line's visual step must be than the short
/// line's for the cost to count as super-linear (linear would be 4, the
/// quadratic prediction 16).
pub const SUPERLINEAR_RATIO: u128 = 8;
/// Timed steps per measurement; the fastest counts.
const ROUNDS: usize = 3;
/// A mixed paragraph: an ASCII run, a Hebrew run, an ASCII run.
pub const MIXED_TEXT: &str = "abc \u{05d0}\u{05d1}\u{05d2}\u{05d3} def";
/// An offset inside the Hebrew run of [`MIXED_TEXT`].
pub const MIXED_OFFSET: i32 = 6;

/// A view with one unwrapped line of `chars` ASCII characters.
#[must_use]
pub fn line_view(chars: usize) -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::None);
    fixture
        .buffer
        .set_text(&paragraph(ParagraphKind::Ascii, chars, 1070));
    fixture
}

/// Place the cursor at `offset` (with the selection bound at `bound`), step
/// once to the right with `step`, and return where the cursor landed and how
/// long the emission took, in microseconds.
#[must_use]
pub fn step_right(
    fixture: &TextViewFixture,
    step: gtk4::MovementStep,
    offset: i32,
    bound: i32,
) -> (i32, u128) {
    let buffer = &fixture.buffer;
    buffer.select_range(
        &buffer.iter_at_offset(offset),
        &buffer.iter_at_offset(bound),
    );
    let started = Instant::now();
    fixture.view.emit_move_cursor(step, 1, false);
    let micros = started.elapsed().as_micros();
    let landed = buffer.iter_at_mark(&buffer.get_insert()).offset();
    (landed, micros)
}

/// The fastest of [`ROUNDS`] steps from [`START_OFFSET`], and where it landed.
fn fastest_step(fixture: &TextViewFixture, step: gtk4::MovementStep) -> (i32, u128) {
    let mut best = (0, u128::MAX);
    for _ in 0..ROUNDS {
        let (landed, micros) = step_right(fixture, step, START_OFFSET, START_OFFSET);
        best = (landed, best.1.min(micros));
    }
    best
}

/// Probe A107.
#[must_use]
pub fn probe_a107() -> Observation {
    Recorder::run(AxiomId::new(107), |recorder| {
        record_text_toolkit(recorder);
        let short = line_view(SHORT_CHARS);
        let _short_shown = Presented::checked(
            recorder,
            &short.scrolled,
            "control: the short line was presented",
        )?;
        settle(LAYOUT_SETTLE);
        let (short_landed, short_visual) =
            fastest_step(&short, gtk4::MovementStep::VisualPositions);
        drop(_short_shown);

        let long = line_view(LONG_CHARS);
        let _long_shown = Presented::checked(
            recorder,
            &long.scrolled,
            "control: the long line was presented",
        )?;
        settle(LAYOUT_SETTLE);
        let (visual_landed, long_visual) = fastest_step(&long, gtk4::MovementStep::VisualPositions);
        let (logical_landed, long_logical) =
            fastest_step(&long, gtk4::MovementStep::LogicalPositions);
        // A selection without extend collapses to its right edge either way.
        let (visual_collapsed, _) = step_right(
            &long,
            gtk4::MovementStep::VisualPositions,
            START_OFFSET,
            START_OFFSET + 4,
        );
        let (logical_collapsed, _) = step_right(
            &long,
            gtk4::MovementStep::LogicalPositions,
            START_OFFSET,
            START_OFFSET + 4,
        );
        recorder.measure("visual_step_us_short", short_visual);
        recorder.measure("visual_step_us_long", long_visual);
        recorder.measure("logical_step_us_long", long_logical);
        recorder.measure(
            "landed_visual_logical",
            format!("{visual_landed} / {logical_landed}"),
        );
        recorder.measure(
            "collapsed_visual_logical",
            format!("{visual_collapsed} / {logical_collapsed}"),
        );
        recorder.control(
            short_landed == START_OFFSET + 1 && visual_landed == START_OFFSET + 1,
            "control: a visual step to the right moves one character on an ASCII line",
        )?;
        recorder.axiom(
            long_visual >= short_visual.saturating_mul(SUPERLINEAR_RATIO)
                && long_logical.saturating_mul(SUPERLINEAR_RATIO) <= long_visual,
            "a visual step costs time super-linear in the display line's length, a logical \
             step does not",
        )?;
        recorder.axiom(
            logical_landed == visual_landed && logical_collapsed == visual_collapsed,
            "on left-to-right text a logical step lands where a visual one does, a collapsing \
             selection included",
        )?;
        drop(_long_shown);

        let mixed = TextViewFixture::new(gtk4::WrapMode::None);
        mixed.buffer.set_text(MIXED_TEXT);
        let _mixed_shown = Presented::checked(
            recorder,
            &mixed.scrolled,
            "control: the mixed line was presented",
        )?;
        settle(LAYOUT_SETTLE);
        let (mixed_visual, _) = step_right(
            &mixed,
            gtk4::MovementStep::VisualPositions,
            MIXED_OFFSET,
            MIXED_OFFSET,
        );
        let (mixed_logical, _) = step_right(
            &mixed,
            gtk4::MovementStep::LogicalPositions,
            MIXED_OFFSET,
            MIXED_OFFSET,
        );
        recorder.measure(
            "mixed_visual_logical",
            format!("{mixed_visual} / {mixed_logical}"),
        );
        recorder.axiom(
            mixed_visual != mixed_logical,
            "inside a right-to-left run a visual step lands elsewhere than a logical one",
        )
    })
}

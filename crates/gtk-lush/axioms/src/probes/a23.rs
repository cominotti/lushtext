// SPDX-License-Identifier: MIT OR Apache-2.0

//! A23: `GtkTextView` lays a paragraph out as **one** `PangoLayout` over its
//! visible text, and never lays out part of a paragraph. Once any part of a
//! wrapped paragraph is shown, its display height is the whole paragraph's
//! wrap count — far taller than the viewport — and the last character's
//! location is already valid, on the paragraph's last display line.
//!
//! Recorded beside it, as values only: the microseconds the first layout of
//! the paragraph took (the first `iter_location` after `set_text`), per
//! visible character, at 2k, 20k and 200k characters, with wrapping on and
//! off. A probe verdict never reads a duration.
//!
//! The long-line designs (document scale policy, protected view) rely on it:
//! layout cost grows with a paragraph's visible length, whatever part of it
//! is on screen.

use gtk4::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::probes::text::{ParagraphKind, TextViewFixture, paragraph, time_us};
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// The paragraph lengths the layout cost is recorded at.
pub const LENGTHS: [usize; 3] = [2_000, 20_000, 200_000];
/// The paragraph length the structural verdict is read on.
pub const VERDICT_LENGTH: usize = 20_000;
/// The paragraph seed.
pub const SEED: u64 = 23;

/// The paragraph's display height and the bottom of its last character, in
/// buffer coordinates, after the view has laid it out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParagraphGeometry {
    /// `line_yrange` height of the paragraph.
    pub height: i32,
    /// Bottom edge (`y + height`) of the last character's location.
    pub last_char_bottom: i32,
    /// Height of the first character's location (one display line).
    pub line_height: i32,
}

/// The geometry of the first paragraph of `fixture`'s buffer.
#[must_use]
pub fn paragraph_geometry(fixture: &TextViewFixture) -> ParagraphGeometry {
    let start = fixture.buffer.start_iter();
    let (_, height) = fixture.view.line_yrange(&start);
    let mut last = start;
    last.forward_to_line_end();
    last.backward_char();
    let last_rect = fixture.view.iter_location(&last);
    let first_rect = fixture.view.iter_location(&start);
    ParagraphGeometry {
        height,
        last_char_bottom: last_rect.y() + last_rect.height(),
        line_height: first_rect.height(),
    }
}

/// Microseconds the first layout of a `chars`-character paragraph takes in
/// `fixture`: `set_text`, then the first `iter_location` of its last
/// character, which builds the paragraph's layout.
#[must_use]
pub fn first_layout_us(fixture: &TextViewFixture, chars: usize) -> u128 {
    fixture
        .buffer
        .set_text(&paragraph(ParagraphKind::Ascii, chars, SEED));
    let mut last = fixture.buffer.end_iter();
    last.backward_char();
    let (_, micros) = time_us(|| fixture.view.iter_location(&last));
    flush_events();
    micros
}

/// Probe A23.
#[must_use]
pub fn probe_a23() -> Observation {
    Recorder::run(AxiomId::new(23), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let wrapped = TextViewFixture::new(gtk4::WrapMode::WordChar);
        let _shown = Presented::checked(
            recorder,
            &wrapped.scrolled,
            "control: the wrapped text view was presented",
        )?;
        wrapped
            .buffer
            .set_text(&paragraph(ParagraphKind::Ascii, VERDICT_LENGTH, SEED));
        settle(LAYOUT_SETTLE);
        let viewport = wrapped.scrolled.height();
        let geometry = paragraph_geometry(&wrapped);
        recorder.measure("viewport_height", viewport);
        recorder.measure("paragraph_height", geometry.height);
        recorder.measure("last_char_bottom", geometry.last_char_bottom);
        recorder.measure("line_height", geometry.line_height);
        recorder.control(
            viewport > 0 && geometry.line_height > 0,
            "control: the view has a viewport and a display line height",
        )?;
        let (pango_lines, pango_height) = independent_layout(&wrapped);
        recorder.measure("independent_pango_lines", pango_lines);
        recorder.measure("independent_pango_height", pango_height);
        recorder.measure("text_area_width", text_area_width(&wrapped.view));
        record_layout_costs(recorder, &wrapped, "wrap");
        check_whole_paragraph(recorder, geometry, viewport)?;
        recorder.axiom(
            geometry.height == pango_height,
            "the paragraph height is the height Pango gives the whole paragraph laid out alone",
        )?;

        let unwrapped = TextViewFixture::new(gtk4::WrapMode::None);
        let _unwrapped_shown = Presented::checked(
            recorder,
            &unwrapped.scrolled,
            "control: the unwrapped text view was presented",
        )?;
        record_layout_costs(recorder, &unwrapped, "nowrap");
        Ok(())
    })
}

/// The width a wrapped line may use: the view's width less its margins.
fn text_area_width(view: &gtk4::TextView) -> i32 {
    view.width() - view.left_margin() - view.right_margin()
}

/// How many lines Pango wraps the fixture's first paragraph into, and how
/// tall it is in pixels, when it lays the whole paragraph out on its own at
/// the view's text width and font.
fn independent_layout(fixture: &TextViewFixture) -> (i32, i32) {
    let start = fixture.buffer.start_iter();
    let mut end = start;
    end.forward_to_line_end();
    let text = fixture.buffer.text(&start, &end, false);
    let layout = fixture.view.create_pango_layout(Some(&text));
    layout.set_width(text_area_width(&fixture.view) * pango::SCALE);
    layout.set_wrap(pango::WrapMode::WordChar);
    (layout.line_count(), layout.pixel_size().1)
}

fn record_layout_costs(recorder: &mut Recorder, fixture: &TextViewFixture, mode: &'static str) {
    for chars in LENGTHS {
        let micros = first_layout_us(fixture, chars);
        let key: &'static str = match (mode, chars) {
            ("wrap", 2_000) => "wrap_2k_first_layout_us",
            ("wrap", 20_000) => "wrap_20k_first_layout_us",
            ("wrap", _) => "wrap_200k_first_layout_us",
            (_, 2_000) => "nowrap_2k_first_layout_us",
            (_, 20_000) => "nowrap_20k_first_layout_us",
            _ => "nowrap_200k_first_layout_us",
        };
        recorder.measure(key, micros);
        let nanos_per_char = micros.saturating_mul(1_000) / chars as u128;
        let per_char_key: &'static str = match (mode, chars) {
            ("wrap", 2_000) => "wrap_2k_ns_per_char",
            ("wrap", 20_000) => "wrap_20k_ns_per_char",
            ("wrap", _) => "wrap_200k_ns_per_char",
            (_, 2_000) => "nowrap_2k_ns_per_char",
            (_, 20_000) => "nowrap_20k_ns_per_char",
            _ => "nowrap_200k_ns_per_char",
        };
        recorder.measure(per_char_key, nanos_per_char);
    }
}

fn check_whole_paragraph(
    recorder: &mut Recorder,
    geometry: ParagraphGeometry,
    viewport: i32,
) -> Result<(), Stop> {
    recorder.axiom(
        geometry.height > viewport.saturating_mul(4),
        "the shown paragraph's height covers its whole wrap count, far past the viewport",
    )?;
    recorder.axiom(
        geometry.last_char_bottom == geometry.height,
        "the last character's location is valid and on the paragraph's last display line",
    )
}

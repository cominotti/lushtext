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
    /// Top edge of the last character's location.
    pub last_char_y: i32,
    /// Height of the first character's location (one display line).
    pub line_height: i32,
    /// The display line, counted from 0, that holds the last character.
    pub last_char_line: i32,
    /// How many display lines the paragraph wraps into.
    pub display_lines: i32,
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
        last_char_y: last_rect.y(),
        line_height: first_rect.height(),
        last_char_line: display_line_index(fixture, &last),
        display_lines: display_line_count(fixture),
    }
}

/// The pixels by which the last character's bottom edge may differ from the
/// paragraph's display height. `gtk_text_layout_get_iter_location` rounds the
/// character's top and its height to pixels separately (`PANGO_PIXELS` of
/// each), while the paragraph's display height is `PANGO_PIXELS` of the whole
/// layout's height, rounded once (`gtktextlayout.c`, GTK 4.22–4.25). With a
/// font whose line height is a fractional number of pixels, the two sums of
/// rounded values differ by at most one pixel (measured: 6,627 against 6,628
/// in the GNOME 51 and nightly SDKs).
pub const SEPARATE_ROUNDING_PX: i32 = 1;

/// The rounding [`last_char_on_last_display_line`] may allow for a
/// paragraph whose last display line Pango places at `last_line_y_units` with
/// a height of `last_line_height_units` (Pango units, laid out alone at the
/// view's wrap width): [`SEPARATE_ROUNDING_PX`] when either is a fractional
/// number of pixels, which is what makes the separate roundings differ, and
/// none when both are whole pixels.
#[must_use]
pub const fn separate_rounding_px(last_line_y_units: i32, last_line_height_units: i32) -> i32 {
    if last_line_y_units % pango::SCALE != 0 || last_line_height_units % pango::SCALE != 0 {
        SEPARATE_ROUNDING_PX
    } else {
        0
    }
}

/// Whether the last character's location is valid and on the paragraph's
/// last display line: it is on the last display line by index, and its
/// rectangle lies in that line's band at the paragraph's bottom, up to
/// `rounding_px` (from [`separate_rounding_px`]).
#[must_use]
pub const fn last_char_on_last_display_line(geometry: ParagraphGeometry, rounding_px: i32) -> bool {
    let on_last_line =
        geometry.display_lines > 0 && geometry.last_char_line == geometry.display_lines - 1;
    let bottom_at_paragraph_bottom =
        (geometry.last_char_bottom - geometry.height).abs() <= rounding_px;
    let top_in_last_line_band =
        geometry.last_char_y >= geometry.height - geometry.line_height - rounding_px;
    on_last_line && bottom_at_paragraph_bottom && top_in_last_line_band
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
        recorder.measure("last_char_y", geometry.last_char_y);
        recorder.measure("last_char_line", geometry.last_char_line);
        recorder.measure(
            "font",
            wrapped
                .view
                .pango_context()
                .font_description()
                .map_or_else(String::new, |font| font.to_string()),
        );
        let rounding_px = record_pango_line_units(recorder, &wrapped)
            .map_or(0, |(y, height)| separate_rounding_px(y, height));
        recorder.measure("allowed_rounding_px", rounding_px);
        recorder.control(
            viewport > 0 && geometry.line_height > 0,
            "control: the view has a viewport and a display line height",
        )?;
        let (pango_lines, pango_height) = independent_layout(&wrapped);
        let view_lines = display_line_count(&wrapped);
        recorder.measure("view_display_lines", view_lines);
        recorder.measure("independent_pango_lines", pango_lines);
        recorder.measure("independent_pango_height", pango_height);
        recorder.measure("text_area_width", text_area_width(&wrapped.view));
        record_layout_costs(recorder, &wrapped, &WRAP_COST_KEYS);
        check_whole_paragraph(recorder, geometry, viewport, rounding_px)?;
        // Lines, not pixels: the view sums each display line's rounded pixel
        // height while Pango rounds the whole layout once, so the heights may
        // differ by a pixel when the line counts agree.
        recorder.axiom(
            view_lines == pango_lines,
            "the paragraph wraps into the lines Pango gives the whole paragraph laid out alone",
        )?;

        let unwrapped = TextViewFixture::new(gtk4::WrapMode::None);
        let _unwrapped_shown = Presented::checked(
            recorder,
            &unwrapped.scrolled,
            "control: the unwrapped text view was presented",
        )?;
        record_layout_costs(recorder, &unwrapped, &NOWRAP_COST_KEYS);
        Ok(())
    })
}

/// The display line, counted from 0, of `target` within its paragraph.
fn display_line_index(fixture: &TextViewFixture, target: &gtk4::TextIter) -> i32 {
    let mut iter = *target;
    iter.set_line_offset(0);
    let mut index = 0;
    while fixture.view.forward_display_line(&mut iter) && iter <= *target {
        index += 1;
    }
    index
}

/// How many display lines the view wraps the first paragraph into.
fn display_line_count(fixture: &TextViewFixture) -> i32 {
    let mut iter = fixture.buffer.start_iter();
    let mut end = iter;
    end.forward_to_line_end();
    let mut lines = 1;
    while fixture.view.forward_display_line(&mut iter) && iter < end {
        lines += 1;
    }
    lines
}

/// The width a wrapped line may use: the view's width less its margins.
fn text_area_width(view: &gtk4::TextView) -> i32 {
    view.width() - view.left_margin() - view.right_margin()
}

/// GtkTextView wraps at its text width less one pixel kept free for the
/// cursor (`SPACE_FOR_CURSOR` in `gtktextview.c`). An independent layout at
/// the full text width agrees with the view only when no line happens to need
/// that last pixel, which depends on the font: the CI container's font
/// wrapped one more line than the reference machine's.
const SPACE_FOR_CURSOR: i32 = 1;

/// How many lines Pango wraps the fixture's first paragraph into, and how
/// tall it is in pixels, when it lays the whole paragraph out on its own at
/// the view's wrap width and font.
fn independent_layout(fixture: &TextViewFixture) -> (i32, i32) {
    let start = fixture.buffer.start_iter();
    let mut end = start;
    end.forward_to_line_end();
    let text = fixture.buffer.text(&start, &end, false);
    let layout = fixture.view.create_pango_layout(Some(&text));
    layout.set_width((text_area_width(&fixture.view) - SPACE_FOR_CURSOR).max(1) * pango::SCALE);
    layout.set_wrap(pango::WrapMode::WordChar);
    (layout.line_count(), layout.pixel_size().1)
}

/// The Pango-unit geometry behind the pixel values, for attributing a
/// rounding difference: the whole layout's logical height and the last two
/// lines' logical tops, laid out alone at the view's wrap width and font.
/// Returns the last line's logical top and height, in Pango units.
fn record_pango_line_units(
    recorder: &mut Recorder,
    fixture: &TextViewFixture,
) -> Option<(i32, i32)> {
    let start = fixture.buffer.start_iter();
    let mut end = start;
    end.forward_to_line_end();
    let text = fixture.buffer.text(&start, &end, false);
    let layout = fixture.view.create_pango_layout(Some(&text));
    layout.set_width((text_area_width(&fixture.view) - SPACE_FOR_CURSOR).max(1) * pango::SCALE);
    layout.set_wrap(pango::WrapMode::WordChar);
    let (_, logical) = layout.extents();
    recorder.measure("pango_height_units", logical.height());
    let mut iter = layout.iter();
    let mut tops = Vec::new();
    loop {
        let (_, line_logical) = iter.line_extents();
        tops.push((line_logical.y(), line_logical.height()));
        if !iter.next_line() {
            break;
        }
    }
    if let [.., previous, last] = tops.as_slice() {
        recorder.measure("pango_previous_line_y_units", previous.0);
        recorder.measure("pango_last_line_y_units", last.0);
        recorder.measure("pango_last_line_height_units", last.1);
    }
    tops.last().copied()
}

/// The recorded keys for each of [`LENGTHS`]: first layout time, and time per
/// character.
type CostKeys = [(&'static str, &'static str); LENGTHS.len()];

const WRAP_COST_KEYS: CostKeys = [
    ("wrap_2k_first_layout_us", "wrap_2k_ns_per_char"),
    ("wrap_20k_first_layout_us", "wrap_20k_ns_per_char"),
    ("wrap_200k_first_layout_us", "wrap_200k_ns_per_char"),
];

const NOWRAP_COST_KEYS: CostKeys = [
    ("nowrap_2k_first_layout_us", "nowrap_2k_ns_per_char"),
    ("nowrap_20k_first_layout_us", "nowrap_20k_ns_per_char"),
    ("nowrap_200k_first_layout_us", "nowrap_200k_ns_per_char"),
];

fn record_layout_costs(recorder: &mut Recorder, fixture: &TextViewFixture, keys: &CostKeys) {
    for (chars, (layout_key, per_char_key)) in LENGTHS.into_iter().zip(keys) {
        let micros = first_layout_us(fixture, chars);
        recorder.measure(layout_key, micros);
        recorder.measure(per_char_key, micros.saturating_mul(1_000) / chars as u128);
    }
}

fn check_whole_paragraph(
    recorder: &mut Recorder,
    geometry: ParagraphGeometry,
    viewport: i32,
    rounding_px: i32,
) -> Result<(), Stop> {
    recorder.axiom(
        geometry.height > viewport.saturating_mul(4),
        "the shown paragraph's height covers its whole wrap count, far past the viewport",
    )?;
    recorder.axiom(
        last_char_on_last_display_line(geometry, rounding_px),
        "the last character's location is valid and on the paragraph's last display line",
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ParagraphGeometry, SEPARATE_ROUNDING_PX, last_char_on_last_display_line,
        separate_rounding_px,
    };

    /// The SDK's last display line in Pango units (`a23-attribution-run.txt`):
    /// top 6,770,418 and height 15,893, neither a whole number of pixels.
    const SDK_ROUNDING: i32 = separate_rounding_px(6_770_418, 15_893);

    /// The GNOME 51 and nightly SDK geometry (`baseline-runtimes.out`): a
    /// 427-line paragraph, 6,627 px tall, whose last character's separately
    /// rounded bottom edge is 6,628.
    const SDK: ParagraphGeometry = ParagraphGeometry {
        height: 6_627,
        last_char_bottom: 6_628,
        last_char_y: 6_612,
        line_height: 16,
        last_char_line: 426,
        display_lines: 427,
    };

    #[test]
    fn the_separately_rounded_sdk_geometry_holds() {
        assert!(last_char_on_last_display_line(SDK, SDK_ROUNDING));
    }

    #[test]
    fn equal_edges_on_the_last_line_hold() {
        assert!(last_char_on_last_display_line(
            ParagraphGeometry {
                last_char_bottom: 6_627,
                last_char_y: 6_611,
                ..SDK
            },
            SDK_ROUNDING
        ));
    }

    #[test]
    fn a_character_on_an_earlier_display_line_is_violated() {
        assert!(!last_char_on_last_display_line(
            ParagraphGeometry {
                last_char_line: 425,
                ..SDK
            },
            SDK_ROUNDING
        ));
    }

    #[test]
    fn an_edge_beyond_the_rounding_is_violated() {
        assert!(!last_char_on_last_display_line(
            ParagraphGeometry {
                last_char_bottom: 6_629,
                last_char_y: 6_613,
                ..SDK
            },
            SDK_ROUNDING
        ));
        assert!(!last_char_on_last_display_line(
            ParagraphGeometry {
                last_char_bottom: 6_611,
                last_char_y: 6_595,
                ..SDK
            },
            SDK_ROUNDING
        ));
    }

    #[test]
    fn a_location_above_the_last_line_band_is_violated() {
        assert!(!last_char_on_last_display_line(
            ParagraphGeometry {
                last_char_y: 6_600,
                ..SDK
            },
            SDK_ROUNDING
        ));
    }

    #[test]
    fn a_fractional_last_line_allows_the_separate_rounding() {
        assert_eq!(SDK_ROUNDING, SEPARATE_ROUNDING_PX);
        assert_eq!(
            separate_rounding_px(16 * 1024, 15_893),
            SEPARATE_ROUNDING_PX
        );
        assert_eq!(
            separate_rounding_px(6_770_418, 16 * 1024),
            SEPARATE_ROUNDING_PX
        );
    }

    /// Whole-pixel line positions round the same either way, so nothing
    /// explains a one-pixel difference: the comparison is exact.
    #[test]
    fn whole_pixel_lines_allow_no_rounding() {
        assert_eq!(separate_rounding_px(6_611 * 1024, 16 * 1024), 0);
        assert!(!last_char_on_last_display_line(SDK, 0));
        assert!(last_char_on_last_display_line(
            ParagraphGeometry {
                last_char_bottom: 6_627,
                last_char_y: 6_611,
                ..SDK
            },
            0
        ));
    }
}

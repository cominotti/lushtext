// SPDX-License-Identifier: MIT OR Apache-2.0

//! A73: a `pixels-below-lines` tag cannot carry a view's end-of-document
//! overscroll, and changing `bottom-margin` instead costs a revalidation.
//!
//! - Applying the tag to the **empty last paragraph** of a text that ends in
//!   a newline is a **no-op** (`gtk_text_buffer_apply_tag` on an empty range
//!   tags nothing): neither the last line's height nor the content extent
//!   changes. Tagging the paragraph before it puts the space **above** the
//!   last line instead.
//! - On a **non-empty** last paragraph the space is part of that line's
//!   y-range (`gtk_text_view_get_line_yrange`), which GtkSourceView's
//!   current-line highlight (`gtksourceview.c`
//!   `gtk_source_view_paint_current_line_highlight` l. 2874–2897) and the
//!   gutter's per-line cells (`gtksourcegutterlines.c` l. 196–206) both use,
//!   so the highlight and the line-number cell grow with it.
//! - A `GtkSourceMap` on the same buffer gets the **same absolute** space: a
//!   buffer tag reaches every view. The map's own `bottom-margin`, by
//!   contrast, follows the view's through a binding that divides it by 4.35
//!   (`gtksourcemap.c` `scale_margin` l. 838–846, bound at l. 884–891).
//! - The print compositor ignores the tag (it honours only colours, style,
//!   weight, underline and strikethrough, `gtksourceprintcompositor.c`
//!   `get_iter_attrs` l. 2177–2290): a 5,000 px tag prints the same pages.
//!
//! The cost of setting the view's `bottom-margin` on the dense 710 fixture
//! (160 lines, validated), wrap on and off, is a recorded magnitude: the
//! setter invalidates every line (A37), so the view revalidates the whole
//! document.
//!
//! LushText's post-open settle (`bound-post-open-settle`, D6.2) therefore
//! sets a dense document's margin once before its install and records later
//! resize-driven changes against this axiom.

use std::time::Duration;

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::probes::post_open::{
    DENSE_LINE_CHARS, DENSE_LINES, DenseView, FrameCounter, REFERENCE_WINDOW, WatchedDispatch,
    dense_text, printed_pages, watch_until_quiet,
};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The overscroll the tag and the margin carry, px.
pub const OVERSCROLL_PX: i32 = 400;
/// The tag height the print comparison uses, px.
pub const PRINT_OVERSCROLL_PX: i32 = 5_000;
/// The text whose last paragraph is empty.
pub const ENDS_IN_NEWLINE: &str = "alpha\nbeta\n";
/// The text whose last paragraph is not empty.
pub const ENDS_IN_TEXT: &str = "alpha\nbeta";
/// Seed of the dense fixture.
pub const SEED: u64 = 58;

const QUIET: Duration = Duration::from_millis(250);
const BUDGET: Duration = Duration::from_secs(20);

/// A presented source view with line numbers, current-line highlight and a
/// map beside it.
pub struct OverscrollView {
    /// The view.
    pub dense: DenseView,
    /// The map.
    pub map: sourceview5::Map,
    /// Its frames.
    pub frames: FrameCounter,
    _shown: Presented,
}

impl OverscrollView {
    fn new(recorder: &mut Recorder, wrap: gtk4::WrapMode, text: &str) -> Result<Self, Stop> {
        let dense = DenseView::new(wrap);
        dense.view.set_show_line_numbers(true);
        dense.view.set_highlight_current_line(true);
        dense.buffer.set_text(text);
        dense.buffer.place_cursor(&dense.buffer.end_iter());
        let map = sourceview5::Map::new();
        map.set_view(&dense.view);
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        container.append(&dense.scrolled);
        container.append(&map);
        let shown = Presented::checked_sized(
            recorder,
            &container,
            REFERENCE_WINDOW,
            "control: the view was presented",
        )?;
        let frames = recorder.require(
            FrameCounter::watch(&dense.view),
            "control: the view has a frame clock",
        )?;
        let fixture = Self {
            dense,
            map,
            frames,
            _shown: shown,
        };
        let _ = fixture.quiet(|| 0.0);
        Ok(fixture)
    }

    fn quiet(&self, read: impl FnMut() -> f64) -> Vec<WatchedDispatch> {
        let watched = watch_until_quiet(Some(&self.frames), QUIET, BUDGET, read);
        settle(Duration::from_millis(20));
        watched
    }

    fn line_yrange(&self, line: i32) -> (i32, i32) {
        let iter = self
            .dense
            .buffer
            .iter_at_line(line)
            .unwrap_or_else(|| self.dense.buffer.end_iter());
        self.dense.view.line_yrange(&iter)
    }

    /// The map's content height: the bottom of its last line.
    fn map_upper(&self) -> f64 {
        let (y, height) = self.map.line_yrange(&self.dense.buffer.end_iter());
        f64::from(y + height)
    }

    fn overscroll_tag(&self, pixels: i32) -> gtk4::TextTag {
        let tag = gtk4::TextTag::builder().pixels_below_lines(pixels).build();
        self.dense.buffer.tag_table().add(&tag);
        tag
    }

    fn tag_line(&self, tag: &gtk4::TextTag, line: i32) {
        let buffer = &self.dense.buffer;
        let start = buffer
            .iter_at_line(line)
            .unwrap_or_else(|| buffer.end_iter());
        let end = buffer
            .iter_at_line(line + 1)
            .unwrap_or_else(|| buffer.end_iter());
        buffer.apply_tag(tag, &start, &end);
        let _ = self.quiet(|| self.dense.uppers().1);
    }
}

/// The cost of a margin change: frameless dispatches of 1 ms or more, the
/// longest of them, their total, and the longest frame, µs.
type MarginCost = (usize, u128, u128, u128);

/// Before and after: (line yrange, view upper, map upper).
type Shape = ((i32, i32), f64, f64);

fn shape(fixture: &OverscrollView, line: i32) -> Shape {
    (
        fixture.line_yrange(line),
        fixture.dense.uppers().1,
        fixture.map_upper(),
    )
}

fn describe(before: Shape, after: Shape) -> String {
    format!(
        "line y {} h {} -> y {} h {}; view upper {} -> {}; map content {} -> {}",
        before.0.0, before.0.1, after.0.0, after.0.1, before.1, after.1, before.2, after.2
    )
}

/// The cost of setting `bottom-margin` on the validated dense fixture:
/// (count of frameless iterations of 1 ms or more, longest, total, longest
/// frame), µs, and the map's bottom margin after.
fn margin_cost(recorder: &mut Recorder, wrap: gtk4::WrapMode) -> Result<(MarginCost, i32), Stop> {
    let fixture = OverscrollView::new(
        recorder,
        wrap,
        &dense_text(DENSE_LINES, DENSE_LINE_CHARS, SEED),
    )?;
    fixture
        .dense
        .buffer
        .place_cursor(&fixture.dense.buffer.start_iter());
    let _ = fixture.quiet(|| fixture.dense.uppers().1);
    fixture.dense.view.set_bottom_margin(OVERSCROLL_PX);
    fixture.dense.view.queue_allocate();
    let watched = fixture.quiet(|| fixture.dense.uppers().1);
    let heavy: Vec<u128> = watched
        .iter()
        .filter(|w| w.dispatch.frames == 0 && w.dispatch.micros >= 1_000)
        .map(|w| w.dispatch.micros)
        .collect();
    let longest_frame = watched
        .iter()
        .filter(|w| w.dispatch.frames > 0)
        .map(|w| w.dispatch.micros)
        .max()
        .unwrap_or_default();
    Ok((
        (
            heavy.len(),
            heavy.iter().copied().max().unwrap_or_default(),
            heavy.iter().sum(),
            longest_frame,
        ),
        fixture.map.bottom_margin(),
    ))
}

/// Probe A73.
#[must_use]
pub fn probe_a73() -> Observation {
    Recorder::run(AxiomId::new(73), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);

        // Empty last paragraph: the tag on it, then on the paragraph before.
        let empty = OverscrollView::new(recorder, gtk4::WrapMode::Word, ENDS_IN_NEWLINE)?;
        let before = shape(&empty, 2);
        let tag = empty.overscroll_tag(OVERSCROLL_PX);
        empty.tag_line(&tag, 2);
        let on_empty = shape(&empty, 2);
        let previous_before = empty.line_yrange(1);
        empty.tag_line(&tag, 1);
        let on_previous = shape(&empty, 2);
        let previous_after = empty.line_yrange(1);
        recorder.measure("tag_on_empty_last", describe(before, on_empty));
        recorder.measure("tag_on_paragraph_before", describe(before, on_previous));
        recorder.measure(
            "paragraph_before_height",
            format!("{} -> {}", previous_before.1, previous_after.1),
        );
        drop(empty);

        // Non-empty last paragraph.
        let text = OverscrollView::new(recorder, gtk4::WrapMode::Word, ENDS_IN_TEXT)?;
        let before_text = shape(&text, 1);
        let tag = text.overscroll_tag(OVERSCROLL_PX);
        text.tag_line(&tag, 1);
        let after_text = shape(&text, 1);
        let cursor_line = text
            .dense
            .buffer
            .iter_at_offset(text.dense.buffer.cursor_position())
            .line();
        recorder.measure("tag_on_text_last", describe(before_text, after_text));
        recorder.measure("cursor_line", cursor_line);
        drop(text);

        // Printing.
        let plain = sourceview5::Buffer::new(None);
        plain.set_text(ENDS_IN_TEXT);
        let tagged = sourceview5::Buffer::new(None);
        tagged.set_text(ENDS_IN_TEXT);
        let print_tag = gtk4::TextTag::builder()
            .pixels_below_lines(PRINT_OVERSCROLL_PX)
            .build();
        tagged.tag_table().add(&print_tag);
        let start = tagged.iter_at_line(1).unwrap_or_else(|| tagged.end_iter());
        tagged.apply_tag(&print_tag, &start, &tagged.end_iter());
        let plain_pages = recorder.require(printed_pages(&plain), "control: the PDF export ran")?;
        let tagged_pages =
            recorder.require(printed_pages(&tagged), "control: the PDF export ran")?;
        recorder.measure(
            "printed_pages",
            format!("plain {plain_pages}, {PRINT_OVERSCROLL_PX} px tag {tagged_pages}"),
        );

        // The cost of the margin on the dense fixture.
        let (on, map_margin) = margin_cost(recorder, gtk4::WrapMode::Word)?;
        let (off, _) = margin_cost(recorder, gtk4::WrapMode::None)?;
        for (key, (count, longest, total, frame)) in [
            ("bottom_margin_wrap_on", on),
            ("bottom_margin_wrap_off", off),
        ] {
            recorder.measure(
                key,
                format!(
                    "{count} frameless dispatches of 1 ms or more, longest {longest} us, total \
                     {total} us; longest frame {frame} us"
                ),
            );
        }
        recorder.measure("map_bottom_margin", map_margin);

        let line_height = before.0.1;
        recorder.control(
            line_height > 0 && plain_pages > 0,
            "control: the lines have a height and the PDF export paginated",
        )?;
        recorder.axiom(
            on_empty.0 == before.0 && (on_empty.1 - before.1).abs() < 1.0,
            "a pixels-below-lines tag on the empty last paragraph changes nothing",
        )?;
        recorder.axiom(
            on_previous.0.0 >= before.0.0 + OVERSCROLL_PX && on_previous.0.1 == before.0.1,
            "on the paragraph before it, the space appears above the last line",
        )?;
        recorder.axiom(
            after_text.0.1 >= before_text.0.1 + OVERSCROLL_PX,
            "on a non-empty last paragraph the space is part of that line's y-range",
        )?;
        recorder.axiom(
            (after_text.2 - before_text.2 - f64::from(OVERSCROLL_PX)).abs() < 1.0,
            "a map of the buffer gets the same absolute space",
        )?;
        recorder.axiom(
            map_margin * 4 <= OVERSCROLL_PX && map_margin > 0,
            "the map's bottom-margin follows the view's, scaled down",
        )?;
        recorder.axiom(
            plain_pages == tagged_pages,
            "the print compositor ignores the tag",
        )
    })
}

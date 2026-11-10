// SPDX-License-Identifier: MIT OR Apache-2.0

//! A106: where a `GtkSourceMap` draws its slider, and what the text views'
//! margins do to it.
//!
//! **GtkSourceView >= 5.22** places the slider from the two vertical
//! adjustments and the two views' *content heights*, which include each
//! view's top and bottom margins (`get_slider_position`:
//! `y = visible_begin / view_height × map_height − map_value`, the heights
//! being each `GtkTextView`'s measured natural height). The slider therefore
//! covers the map's drawing of the view's visible text only while the map's
//! margins are the view's scaled by the map's text scale. A map given the
//! view's bottom margin **unscaled** (an end-of-document overscroll margin of
//! hundreds of pixels copied 1:1) draws its slider well below the text the
//! view shows.
//!
//! **GtkSourceView < 5.22** placed the slider from the end iter's line
//! location in each view and the visible rectangles, so margins below the
//! last line did not move it: the copied and the scaled margin both cover the
//! visible text.
//!
//! Control: with no margins the slider covers the visible text on both.
//!
//! LushText copied the editor's overscroll bottom margin onto its minimap's
//! map; from GtkSourceView 5.22 it scales it instead
//! (`ui/editor_page/minimap/projection_execution.rs`).

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::observation::Stop;
use crate::probes::a99::MapFixture;
use crate::probes::a103::slider;
use crate::probes::text::record_text_toolkit;
use crate::session::{Presented, settle};
use crate::version::catalogued_bound;
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines of the fixture.
pub const LINES: usize = 2_000;
/// The view's bottom margin, an end-of-document overscroll.
pub const VIEW_BOTTOM_MARGIN: i32 = 600;
/// How far the slider's top may sit from the map's drawing of the view's
/// first visible line and still cover it (font rounding of the scaled map).
pub const ALIGNED_PX: i32 = 3;
/// How far it must sit to count as misplaced.
pub const MISPLACED_PX: i32 = 20;

/// The map's bottom margin in one round.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapMargin {
    /// No margin on either view.
    None,
    /// The view's bottom margin copied unscaled.
    Copied,
    /// The view's bottom margin scaled by the map's text scale.
    Scaled,
}

/// A view's document height: the bottom of its end iter's line, in buffer
/// coordinates (margins excluded).
fn document_height(view: &impl IsA<gtk4::TextView>) -> i32 {
    let view = view.as_ref();
    let rect = view.iter_location(&view.buffer().end_iter());
    rect.y() + rect.height()
}

/// The map bottom margin that keeps the map's content height the view's
/// scaled by the map's text scale (`map_doc / view_doc`), rounded.
#[must_use]
pub fn scaled_bottom_margin(fixture: &MapFixture) -> i32 {
    let view_doc = i64::from(document_height(&fixture.view));
    let map_doc = i64::from(document_height(&fixture.map));
    if view_doc <= 0 {
        return 0;
    }
    let view_margins = i64::from(fixture.view.top_margin() + fixture.view.bottom_margin());
    let scaled = (view_margins * map_doc + view_doc / 2) / view_doc;
    i32::try_from(scaled - i64::from(fixture.map.top_margin())).map_or(0, |margin| margin.max(0))
}

/// Set the margins for `margin`, scroll the view to the middle of its range,
/// let frames draw, and return the slider's top minus the map's drawing of
/// the view's first visible line, in map widget pixels.
#[must_use]
pub fn slider_offset(fixture: &MapFixture, margin: MapMargin) -> Option<i32> {
    let view_bottom = match margin {
        MapMargin::None => 0,
        MapMargin::Copied | MapMargin::Scaled => VIEW_BOTTOM_MARGIN,
    };
    fixture.view.set_bottom_margin(view_bottom);
    fixture.map.set_bottom_margin(0);
    settle(LAYOUT_SETTLE);
    let map_bottom = match margin {
        MapMargin::None => 0,
        MapMargin::Copied => view_bottom,
        MapMargin::Scaled => scaled_bottom_margin(fixture),
    };
    fixture.map.set_bottom_margin(map_bottom);
    settle(LAYOUT_SETTLE);
    let vadjustment = fixture.view.vadjustment()?;
    let middle = (vadjustment.upper() - vadjustment.page_size()) / 2.0;
    vadjustment.set_value(middle.floor());
    settle(LAYOUT_SETTLE);
    settle(LAYOUT_SETTLE);
    let visible = fixture.view.visible_rect();
    let (top_iter, _) = fixture.view.line_at_y(visible.y());
    let in_map = fixture.map.iter_location(&top_iter);
    let (_, line_y) =
        fixture
            .map
            .buffer_to_window_coords(gtk4::TextWindowType::Widget, in_map.x(), in_map.y());
    let bounds = slider(&fixture.map)?.compute_bounds(&fixture.map)?;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a slider coordinate inside a map a few hundred pixels tall"
    )]
    let slider_y = bounds.y().round() as i32;
    Some(slider_y - line_y)
}

/// One fresh, presented fixture measured with `margin`.
fn presented_round(recorder: &mut Recorder, margin: MapMargin) -> Result<i32, Stop> {
    let fixture = MapFixture::new(LINES);
    let _shown = Presented::checked(
        recorder,
        &fixture.root,
        "control: the view and its map were presented",
    )?;
    settle(LAYOUT_SETTLE);
    let offset = slider_offset(&fixture, margin);
    recorder.control(
        offset.is_some(),
        "control: the map has an allocated slider and the view a visible line",
    )?;
    Ok(offset.unwrap_or_default())
}

/// Probe A106.
#[must_use]
pub fn probe_a106() -> Observation {
    Recorder::run(AxiomId::new(106), |recorder| {
        record_text_toolkit(recorder);
        let none = presented_round(recorder, MapMargin::None)?;
        let copied = presented_round(recorder, MapMargin::Copied)?;
        let scaled = presented_round(recorder, MapMargin::Scaled)?;
        recorder.measure("view_bottom_margin", VIEW_BOTTOM_MARGIN);
        recorder.measure("slider_offset_no_margins", none);
        recorder.measure("slider_offset_copied_margin", copied);
        recorder.measure("slider_offset_scaled_margin", scaled);
        recorder.control(
            none.abs() <= ALIGNED_PX,
            "control: with no margins the slider covers the map's drawing of the visible text",
        )?;
        if catalogued_bound(recorder, AxiomId::new(106))?.reached() {
            recorder.axiom(
                copied.abs() >= MISPLACED_PX && scaled.abs() <= ALIGNED_PX,
                "A106 (GtkSourceView >= 5.22): the slider follows the views' content \
                 heights, margins included, so an unscaled copied bottom margin misplaces it \
                 and a scaled one keeps it on the visible text",
            )
        } else {
            recorder.axiom(
                copied.abs() <= ALIGNED_PX && scaled.abs() <= ALIGNED_PX,
                "A106 (GtkSourceView < 5.22): the slider follows the end iter's line, so a \
                 bottom margin, copied or scaled, does not move it off the visible text",
            )
        }
    })
}

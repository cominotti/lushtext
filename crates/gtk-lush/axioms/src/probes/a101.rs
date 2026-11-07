// SPDX-License-Identifier: MIT OR Apache-2.0

//! A101: a `GtkDrawingArea` overlaid on a `GtkSourceMap` (a `GtkOverlay`
//! sibling) is **not redrawn** when the map scrolls. Scrolling the editor
//! moves the map's own vertical adjustment and the map repaints, but the
//! overlay's draw function is not called again: GTK reuses its cached render
//! node until the drawing area queues a draw itself.
//!
//! So an overlay that paints marks aligned with the map's lines goes stale on
//! every scroll unless it listens to the map adjustment's `value-changed`
//! and queues its own draw. LushText's minimap marker strip does that
//! (`bin-minimap-markers-per-row` D7).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::a99::MapFixture;
use crate::probes::text::record_text_toolkit;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// Lines of the fixture, many more than the map shows.
pub const LINES: usize = 20_000;

/// A [`MapFixture`] whose map carries an overlaid drawing area counting its
/// draws.
#[derive(Clone, Debug)]
pub struct OverlaidMap {
    /// The view, its map and the buffer.
    pub fixture: MapFixture,
    /// The overlay strip.
    pub strip: gtk4::DrawingArea,
    /// Calls of the strip's draw function.
    pub draws: Rc<Cell<u32>>,
}

impl OverlaidMap {
    /// Build the fixture; the map moves into an overlay with the strip.
    #[must_use]
    pub fn new() -> Self {
        let fixture = MapFixture::new(LINES);
        fixture.root.remove(&fixture.map);
        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&fixture.map));
        let strip = gtk4::DrawingArea::new();
        strip.set_width_request(8);
        strip.set_halign(gtk4::Align::End);
        strip.set_can_target(false);
        let draws = Rc::new(Cell::new(0u32));
        {
            let draws = draws.clone();
            strip.set_draw_func(move |_, cr, _, _| {
                draws.set(draws.get() + 1);
                cr.set_source_rgba(1.0, 0.5, 0.0, 0.9);
                cr.rectangle(0.0, 10.0, 8.0, 2.0);
                let _ = cr.fill();
            });
        }
        overlay.add_overlay(&strip);
        fixture.root.append(&overlay);
        Self {
            fixture,
            strip,
            draws,
        }
    }

    /// Scroll the editor to `fraction` of its range.
    pub fn scroll_editor_to(&self, fraction: f64) {
        if let Some(adjustment) = self.fixture.view.vadjustment() {
            let range = adjustment.upper() - adjustment.page_size();
            adjustment.set_value(adjustment.lower() + range * fraction);
        }
    }

    /// The map's vertical scroll value.
    #[must_use]
    pub fn map_value(&self) -> f64 {
        self.fixture
            .map
            .vadjustment()
            .map_or(0.0, |adjustment| adjustment.value())
    }
}

impl Default for OverlaidMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Probe A101.
#[must_use]
pub fn probe_a101() -> Observation {
    Recorder::run(AxiomId::new(101), |recorder| {
        record_text_toolkit(recorder);
        let overlaid = OverlaidMap::new();
        let shown = Presented::checked(recorder, &overlaid.fixture.root, "the map is presented")?;
        settle(Duration::from_millis(300));
        let first_draws = overlaid.draws.get();
        let before = overlaid.map_value();
        overlaid.draws.set(0);
        for step in 1..=5 {
            overlaid.scroll_editor_to(f64::from(step) / 10.0);
            settle(Duration::from_millis(120));
        }
        let after = overlaid.map_value();
        let draws_on_scroll = overlaid.draws.get();
        overlaid.strip.queue_draw();
        settle(Duration::from_millis(120));
        let draws_after_queue = overlaid.draws.get() - draws_on_scroll;
        drop(shown);
        recorder.measure("draws_at_present", first_draws);
        recorder.measure("map_value_before", before);
        recorder.measure("map_value_after", after);
        recorder.measure("draws_while_map_scrolled", draws_on_scroll);
        recorder.measure("draws_after_queue_draw", draws_after_queue);
        recorder.control(first_draws >= 1, "control: the strip drew when presented")?;
        recorder.control(
            after > before + 1.0,
            "control: the map scrolled with the editor",
        )?;
        recorder.control(
            draws_after_queue >= 1,
            "control: a queued draw redraws the strip",
        )?;
        recorder.axiom(
            draws_on_scroll == 0,
            "an overlay sibling is not redrawn when the source map scrolls",
        )
    })
}

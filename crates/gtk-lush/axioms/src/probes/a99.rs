// SPDX-License-Identifier: MIT OR Apache-2.0

//! A99: `gtk_text_view_get_line_at_y` on a presented `GtkSourceMap` is
//! **monotone** in `y`: sweeping every pixel row of the laid-out document
//! top to bottom never returns an earlier line. It **skips zero-height
//! lines**: lines hidden by an `invisible` tag are never returned, so they
//! fall between the starts of two rows. Its cost is **sublinear** in the line
//! count: one call near the end of a 262,144-line map costs about what one
//! near the end of a 1,024-line map costs.
//!
//! LushText's minimap strip builds a row table with one call per strip pixel
//! row (`bin-minimap-markers-per-row` D2); a group of rows showing the same
//! line, hidden lines included, gets one marker probe.

use std::time::Duration;

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::a98::numbered_lines;
use crate::probes::text::{invisible_tag, record_text_toolkit, time_us};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// Lines of the map the sweep runs over.
pub const SWEEP_LINES: usize = 600;
/// The hidden line range of the sweep map, `[start, end)`.
pub const HIDDEN: (i32, i32) = (200, 260);
/// Line counts the cost is compared at.
pub const COST_LINES: [usize; 2] = [1 << 10, 1 << 18];
/// Calls timed at each line count.
const TIMED_CALLS: u32 = 256;

/// A view and the source map bound to it, the map inside a scrolled window
/// beside the view, as an editor hosts them.
#[derive(Clone, Debug)]
pub struct MapFixture {
    /// The box to present.
    pub root: gtk4::Box,
    /// The source view.
    pub view: sourceview5::View,
    /// The map projecting it.
    pub map: sourceview5::Map,
    /// The buffer.
    pub buffer: sourceview5::Buffer,
}

impl MapFixture {
    /// A view of `lines` numbered lines and its map.
    #[must_use]
    pub fn new(lines: usize) -> Self {
        let buffer = sourceview5::Buffer::new(None);
        buffer.set_text(&numbered_lines(lines));
        let view = sourceview5::View::with_buffer(&buffer);
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&view)
            .hexpand(true)
            .vexpand(true)
            .build();
        let map = sourceview5::Map::new();
        map.set_view(&view);
        map.set_width_request(120);
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        root.append(&scrolled);
        root.append(&map);
        Self {
            root,
            view,
            map,
            buffer,
        }
    }

    /// Hide `[start, end)` with an `invisible` tag.
    pub fn hide_lines(&self, (start, end): (i32, i32)) {
        let tag = invisible_tag(self.buffer.upcast_ref());
        if let (Some(start), Some(end)) = (
            self.buffer.iter_at_line(start),
            self.buffer.iter_at_line(end),
        ) {
            self.buffer.apply_tag(&tag, &start, &end);
        }
    }

    /// The map's laid-out document height, from the end iter.
    #[must_use]
    pub fn map_height(&self) -> i32 {
        let (y, height) = self.map.line_yrange(&self.buffer.end_iter());
        y + height
    }
}

/// The line `line_at_y` returns at every pixel row of the map's document.
#[must_use]
pub fn sweep(fixture: &MapFixture) -> Vec<i32> {
    (0..fixture.map_height())
        .map(|y| fixture.map.line_at_y(y).0.line())
        .collect()
}

/// Microseconds per call near the end of a `lines`-line map.
fn cost_per_call_ns(lines: usize) -> Option<u128> {
    let fixture = MapFixture::new(lines);
    let shown = Presented::detached(&fixture.root);
    settle(Duration::from_millis(300));
    let height = fixture.map_height();
    if height <= 0 {
        return None;
    }
    let ((), elapsed) = time_us(|| {
        for index in 0..TIMED_CALLS {
            let y = height - 1 - i32::try_from(index).unwrap_or(0);
            let _ = fixture.map.line_at_y(y);
        }
    });
    drop(shown);
    Some(elapsed * 1_000 / u128::from(TIMED_CALLS))
}

/// Probe A99.
#[must_use]
pub fn probe_a99() -> Observation {
    Recorder::run(AxiomId::new(99), |recorder| {
        record_text_toolkit(recorder);
        let fixture = MapFixture::new(SWEEP_LINES);
        fixture.hide_lines(HIDDEN);
        let shown = Presented::checked(recorder, &fixture.root, "the map is presented")?;
        settle(Duration::from_millis(300));
        let lines = sweep(&fixture);
        drop(shown);
        let monotone = lines
            .array_windows()
            .all(|[earlier, later]| earlier <= later);
        let returned_hidden = lines
            .iter()
            .filter(|line| (HIDDEN.0..HIDDEN.1).contains(line))
            .count();
        let distinct = {
            let mut distinct = lines.clone();
            distinct.dedup();
            distinct.len()
        };
        recorder.measure("rows_swept", lines.len());
        recorder.measure("distinct_lines", distinct);
        recorder.measure("hidden_lines_returned", returned_hidden);
        let small = recorder.require(cost_per_call_ns(COST_LINES[0]), "the small map laid out")?;
        let large = recorder.require(cost_per_call_ns(COST_LINES[1]), "the large map laid out")?;
        recorder.measure("ns_per_call_1024_lines", small);
        recorder.measure("ns_per_call_262144_lines", large);
        let visible_lines = SWEEP_LINES - usize::try_from(HIDDEN.1 - HIDDEN.0).unwrap_or(0);
        recorder.control(
            distinct >= visible_lines / 2,
            "control: the sweep reached most visible lines",
        )?;
        recorder.axiom(monotone, "line_at_y is monotone in y")?;
        recorder.axiom(
            returned_hidden == 0,
            "line_at_y never returns a hidden line",
        )?;
        recorder.axiom(
            large < small.saturating_mul(16).max(20_000),
            "line_at_y near the end costs about the same at 262,144 lines as at 1,024",
        )
    })
}

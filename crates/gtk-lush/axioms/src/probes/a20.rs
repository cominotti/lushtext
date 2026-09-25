// SPDX-License-Identifier: MIT OR Apache-2.0

//! A20: where a `GtkListView` anchors after a host moves its value depends on
//! whether the list had already been allocated at that value when it last saw
//! `value-changed`.
//!
//! A host `set_value` delivers its `value-changed` before the list has been
//! allocated at the new value. The list then keeps its scroll anchor far from
//! the view, with an alignment outside `[0, 1]`, so a later page change
//! re-derives the value along a steep line (A7's slope). When the host
//! re-emits `value-changed` once after the list has been allocated at that
//! value, the list re-anchors on the row at the view's top edge with an
//! alignment inside `[0, 1]`, and the same page change moves the value by no
//! more than the page change itself.
//!
//! The probe moves the value to the top of a far row in two fresh fixtures,
//! shrinks the page once in each, and reads the implied alignment
//! `-(Δvalue / Δpage)` of `value = anchor − align × page`.

use gtk4::prelude::*;

use super::{LAYOUT_SETTLE, row_stride, same_value};
use crate::fixtures::{HostedList, LIST_HEIGHT, ROWS, realized_rows};
use crate::observation::{Recorder, Stop};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The rows whose top the host moves the value to, from the top of the list.
pub const TARGET_ROWS: [u32; 2] = [40, 150];
/// The page height the list shrinks to after the move.
pub const SHRUNK_PAGE: i32 = 200;

/// Re-emit `value-changed` on `adjustment` without changing its value, as a
/// host re-announcing the value it already published.
pub fn reannounce_value(adjustment: &gtk4::Adjustment) {
    adjustment.emit_by_name::<()>("value-changed", &[]);
}

/// One move-then-shrink run.
struct Run {
    moved_value: f64,
    shrunk_value: f64,
    mapped_in_view: usize,
}

impl Run {
    /// The alignment `value = anchor − align × page` implies for the shrink.
    fn align(&self) -> f64 {
        let page_change = f64::from(SHRUNK_PAGE - LIST_HEIGHT);
        -(self.shrunk_value - self.moved_value) / page_change
    }

    /// The anchor position `value + align × page` implies, before the shrink.
    fn anchor_position(&self) -> f64 {
        self.moved_value + self.align() * f64::from(LIST_HEIGHT)
    }
}

/// Present a fresh list, move its value to the top of `row` (re-announcing it
/// once the list has been allocated there when `reannounce`), then shrink the
/// page and read the value back.
fn run(recorder: &mut Recorder, row: u32, reannounce: bool) -> Result<Run, Stop> {
    let hosted = HostedList::new();
    let _shown = Presented::checked(
        recorder,
        &hosted.host,
        "control: the fixture window realizes",
    )?;
    let target = f64::from(row) * row_stride(&hosted.adjustment, ROWS);
    hosted.adjustment.set_value(target);
    settle(LAYOUT_SETTLE);
    if reannounce {
        reannounce_value(&hosted.adjustment);
        settle(LAYOUT_SETTLE);
    }
    let moved_value = hosted.adjustment.value();
    let height = f64::from(hosted.list.height());
    let mapped_in_view = realized_rows(&hosted.list)
        .iter()
        .filter(|widget| widget.is_mapped())
        .filter_map(|widget| widget.compute_bounds(&hosted.list))
        .filter(|bounds| {
            let top = f64::from(bounds.y());
            top + f64::from(bounds.height()) > 0.0 && top < height
        })
        .count();
    recorder.control(
        same_value(moved_value, target) && mapped_in_view > 0,
        "control: the list holds the moved value and maps rows in view there",
    )?;
    hosted.host.set_child_height(SHRUNK_PAGE);
    settle(LAYOUT_SETTLE);
    Ok(Run {
        moved_value,
        shrunk_value: hosted.adjustment.value(),
        mapped_in_view,
    })
}

/// The measurement keys of one run.
type RunKeys = [&'static str; 5];

const KEYS: [[RunKeys; 2]; 2] = [
    [
        [
            "row_40_moved_value",
            "row_40_mapped_in_view",
            "row_40_shrunk_value",
            "row_40_align",
            "row_40_anchor_position",
        ],
        [
            "row_150_moved_value",
            "row_150_mapped_in_view",
            "row_150_shrunk_value",
            "row_150_align",
            "row_150_anchor_position",
        ],
    ],
    [
        [
            "row_40_reannounced_moved_value",
            "row_40_reannounced_mapped_in_view",
            "row_40_reannounced_shrunk_value",
            "row_40_reannounced_align",
            "row_40_reannounced_anchor_position",
        ],
        [
            "row_150_reannounced_moved_value",
            "row_150_reannounced_mapped_in_view",
            "row_150_reannounced_shrunk_value",
            "row_150_reannounced_align",
            "row_150_reannounced_anchor_position",
        ],
    ],
];

fn record(recorder: &mut Recorder, run: &Run, keys: &'static RunKeys) {
    recorder.measure(keys[0], run.moved_value);
    recorder.measure(keys[1], run.mapped_in_view);
    recorder.measure(keys[2], run.shrunk_value);
    recorder.measure(keys[3], run.align());
    recorder.measure(keys[4], run.anchor_position());
}

/// Probe A20. See the module documentation.
#[must_use]
pub fn probe_a20() -> Observation {
    Recorder::run(AxiomId::new(20), |recorder| {
        let mut stale = Vec::new();
        let mut reannounced = Vec::new();
        for (index, row) in TARGET_ROWS.into_iter().enumerate() {
            let moved = run(recorder, row, false)?;
            record(recorder, &moved, &KEYS[0][index]);
            stale.push(moved);
            let moved = run(recorder, row, true)?;
            record(recorder, &moved, &KEYS[1][index]);
            reannounced.push(moved);
        }
        recorder.axiom(
            stale.iter().all(|run| run.align().abs() > 1.0),
            "A20: after a host set_value the list's anchor alignment lies outside [0, 1]",
        )?;
        recorder.axiom(
            reannounced
                .iter()
                .all(|run| (0.0..=1.0).contains(&run.align())),
            "A20: re-announced after the list was allocated there, the alignment lies in [0, 1]",
        )
    })
}

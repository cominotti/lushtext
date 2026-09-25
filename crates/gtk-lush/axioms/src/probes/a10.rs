// SPDX-License-Identifier: MIT OR Apache-2.0

//! A10: `mapped` does not mean "drawn on screen". A `GtkListView` maps rows
//! whose bounds lie wholly outside its own allocation: one row past the
//! bottom edge at rest, and one row past **each** edge when its value falls
//! on a row boundary — a host `set_value` to a row top, `scroll_to`, or a
//! focus `scroll_to` — so it maps 11 rows of which 9 are in view. At a value
//! between row boundaries the same list maps 10 rows and all of them
//! intersect. A test that counts mapped rows, or asserts that some row is
//! mapped, therefore overcounts what the user sees; a rendered-row assertion
//! must intersect each mapped row's bounds with the viewport it cares about.
//!
//! The probe also records, without asserting it, the one part of the old
//! statement that holds: a focused row scrolled out of view stays realized
//! but unmapped.

use gtk4::prelude::*;

use super::{LAYOUT_SETTLE, SCROLL_SETTLE, row_stride};
use crate::fixtures::{HostedList, ROWS, RowPlacement, realized_rows, row_index, row_placement};
use crate::observation::{Recorder, Stop};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// A value between row boundaries (about 29.4 rows down).
pub const UNALIGNED_VALUE: f64 = 1_000.0;
/// The row whose top the probe scrolls the list's own adjustment to.
pub const ALIGNED_ROW: u32 = 30;
/// The row the probe asks the list to `scroll_to`.
pub const SCROLL_TO_ROW: u32 = 200;
/// The row the probe asks the list to `scroll_to` with focus.
pub const FOCUS_ROW: u32 = 150;
/// How far the probe then scrolls the focused row out of view.
pub const SCROLL_AWAY: f64 = 500.0;

/// What the list maps in one state.
struct MappedRows {
    /// Rows the list has mapped.
    mapped: usize,
    /// Mapped rows whose bounds intersect `[0, height)` of the list.
    intersecting: usize,
    /// Mapped rows wholly outside the list's allocation, as `index@top`.
    outside: Vec<String>,
    /// How many of `outside` lie above the allocation and how many below.
    outside_above: usize,
    outside_below: usize,
}

impl MappedRows {
    /// Read the mapped rows of `list` against its own allocation.
    fn read(list: &gtk4::ListView) -> Self {
        let mut read = Self {
            mapped: 0,
            intersecting: 0,
            outside: Vec::new(),
            outside_above: 0,
            outside_below: 0,
        };
        for row in realized_rows(list).iter().filter(|row| row.is_mapped()) {
            read.mapped += 1;
            let (top, above) = match row_placement(row, list) {
                None => continue,
                Some(RowPlacement::InView) => {
                    read.intersecting += 1;
                    continue;
                }
                Some(RowPlacement::Above(top)) => (top, true),
                Some(RowPlacement::Below(top)) => (top, false),
            };
            let index = row_index(row).map_or_else(|| "?".to_owned(), |index| index.to_string());
            read.outside.push(format!("{index}@{top}"));
            if above {
                read.outside_above += 1;
            } else {
                read.outside_below += 1;
            }
        }
        read
    }

    fn record(&self, recorder: &mut Recorder, state: &'static StateKeys) {
        recorder.measure(state[0], self.mapped);
        recorder.measure(state[1], self.intersecting);
        recorder.measure(state[2], self.outside.join(" "));
        recorder.measure(state[3], self.outside_above);
        recorder.measure(state[4], self.outside_below);
    }
}

/// The measurement keys of one state.
type StateKeys = [&'static str; 5];

const RESTING: StateKeys = [
    "resting_mapped_rows",
    "resting_intersecting_rows",
    "resting_mapped_outside",
    "resting_outside_above",
    "resting_outside_below",
];
const UNALIGNED: StateKeys = [
    "unaligned_mapped_rows",
    "unaligned_intersecting_rows",
    "unaligned_mapped_outside",
    "unaligned_outside_above",
    "unaligned_outside_below",
];
const ALIGNED: StateKeys = [
    "aligned_mapped_rows",
    "aligned_intersecting_rows",
    "aligned_mapped_outside",
    "aligned_outside_above",
    "aligned_outside_below",
];
const SCROLLED_TO: StateKeys = [
    "scroll_to_mapped_rows",
    "scroll_to_intersecting_rows",
    "scroll_to_mapped_outside",
    "scroll_to_outside_above",
    "scroll_to_outside_below",
];
const FOCUSED: StateKeys = [
    "focus_mapped_rows",
    "focus_intersecting_rows",
    "focus_mapped_outside",
    "focus_outside_above",
    "focus_outside_below",
];

/// Settle `delay`, then read and record the list's mapped rows under `keys`.
fn read_state(
    recorder: &mut Recorder,
    hosted: &HostedList,
    delay: std::time::Duration,
    value_key: &'static str,
    keys: &'static StateKeys,
) -> MappedRows {
    settle(delay);
    recorder.measure(value_key, hosted.adjustment.value());
    let read = MappedRows::read(&hosted.list);
    read.record(recorder, keys);
    read
}

/// Probe A10. See the module documentation.
#[must_use]
pub fn probe_a10() -> Observation {
    Recorder::run(AxiomId::new(10), |recorder| {
        let hosted = HostedList::new();
        let _shown = Presented::checked(
            recorder,
            &hosted.host,
            "control: the fixture window realizes",
        )?;
        let stride = row_stride(&hosted.adjustment, ROWS);
        recorder.measure("row_stride", stride);
        recorder.measure("list_height", hosted.list.height());
        recorder.control(
            stride > 0.0 && hosted.list.height() > 0,
            "control: the list is allocated and has a row stride",
        )?;

        let resting = read_state(recorder, &hosted, LAYOUT_SETTLE, "resting_value", &RESTING);
        rows_in_view(
            recorder,
            &resting,
            "control: at rest the list maps rows in view",
        )?;

        hosted.adjustment.set_value(UNALIGNED_VALUE);
        let unaligned = read_state(
            recorder,
            &hosted,
            LAYOUT_SETTLE,
            "unaligned_value",
            &UNALIGNED,
        );
        rows_in_view(
            recorder,
            &unaligned,
            "control: unaligned, the list maps rows in view",
        )?;

        hosted.adjustment.set_value(f64::from(ALIGNED_ROW) * stride);
        let aligned = read_state(recorder, &hosted, LAYOUT_SETTLE, "aligned_value", &ALIGNED);
        rows_in_view(
            recorder,
            &aligned,
            "control: aligned, the list maps rows in view",
        )?;

        hosted
            .list
            .scroll_to(SCROLL_TO_ROW, gtk4::ListScrollFlags::NONE, None);
        let scrolled_to = read_state(
            recorder,
            &hosted,
            SCROLL_SETTLE,
            "scroll_to_value",
            &SCROLLED_TO,
        );
        rows_in_view(
            recorder,
            &scrolled_to,
            "control: after scroll_to the list maps rows in view",
        )?;

        hosted
            .list
            .scroll_to(FOCUS_ROW, gtk4::ListScrollFlags::FOCUS, None);
        let focused = read_state(recorder, &hosted, SCROLL_SETTLE, "focus_value", &FOCUSED);
        rows_in_view(
            recorder,
            &focused,
            "control: after a focus scroll the list maps rows in view",
        )?;

        // Recorded, not asserted: which rows GTK keeps realized is tracker
        // policy no design relies on.
        hosted
            .adjustment
            .set_value(hosted.adjustment.value() + SCROLL_AWAY);
        settle(LAYOUT_SETTLE);
        let focus_row = realized_rows(&hosted.list)
            .into_iter()
            .find(|row| row_index(row) == Some(FOCUS_ROW));
        recorder.measure("away_focus_row_realized", focus_row.is_some());
        recorder.measure(
            "away_focus_row_mapped",
            focus_row.is_some_and(|row| row.is_mapped()),
        );

        recorder.axiom(
            resting.outside_below >= 1,
            "A10: at rest the list maps a row wholly below its allocation",
        )?;
        for (read, check) in [
            (
                &aligned,
                "A10: at a row-aligned value the list maps a row wholly outside each edge",
            ),
            (
                &scrolled_to,
                "A10: after scroll_to the list maps a row wholly outside each edge",
            ),
            (
                &focused,
                "A10: after a focus scroll the list maps a row wholly outside each edge",
            ),
        ] {
            recorder.axiom(read.outside_above >= 1 && read.outside_below >= 1, check)?;
        }
        Ok(())
    })
}

/// A control step: the list maps some rows that are in view at all.
fn rows_in_view(
    recorder: &mut Recorder,
    read: &MappedRows,
    check: &'static str,
) -> Result<(), Stop> {
    recorder.control(read.intersecting > 0, check)
}

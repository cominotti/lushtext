// SPDX-License-Identifier: MIT OR Apache-2.0

//! A1: a `GtkListView` realizes at most about `GTK_LIST_VIEW_MAX_LIST_ITEMS`
//! (200) row widgets — the cap plus a few tracker extras, 205 measured on GTK
//! 4.22.5 both at rest and in a viewport that needs 294 rows — however tall
//! its viewport, and the realized range follows its **own** vadjustment. A list handed its whole content as viewport
//! therefore draws blank space after about the two-hundredth row, which is why
//! `ViewportSliceBin` hands the list only the band the outer scroller shows.
//!
//! The probe also records, without asserting it, a lag GTK 4.22.5 shows: the
//! **first** host `set_value` to a position past the realized rows is not
//! followed — the list keeps its old anchor, re-derives the value within a
//! pixel of the request, and maps no row in the viewport — until the next
//! `value-changed`, after which the realized range follows. No design relies
//! on that lag, so it is a measurement for review rather than an axiom.

use gtk4::prelude::*;

use super::{LAYOUT_SETTLE, row_stride};
use crate::fixtures::{HostedList, LIST_HEIGHT, ROWS, realized_rows, row_index, row_index_range};
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// `GTK_LIST_VIEW_MAX_LIST_ITEMS` in `gtk/gtklistview.c`.
pub(crate) const GTK_LIST_VIEW_MAX_LIST_ITEMS: u32 = 200;
/// The rows GTK's item trackers may keep beyond the cap: measured, not
/// derived. GTK 4.22.5 realizes 205 rows at most (0–204 at the top of the
/// list), so a change here is an axiom change to review, not to absorb.
pub(crate) const TRACKER_EXTRA_ROWS: u32 = 5;
/// A viewport tall enough that it would need well over the cap to fill.
pub const TALL_VIEWPORT: i32 = 10_000;
/// The row the probe scrolls its own adjustment to for the second half.
pub const SCROLLED_ROW: u32 = 300;

/// How many of `rows` are mapped (drawn).
fn mapped_rows(rows: &[gtk4::Widget]) -> usize {
    rows.iter().filter(|row| row.is_mapped()).count()
}

/// Probe A1. See the module documentation.
#[must_use]
pub fn probe_a01() -> Observation {
    Recorder::run(AxiomId::new(1), |recorder| {
        let hosted = HostedList::new();
        let _shown = Presented::checked(
            recorder,
            &hosted.host,
            "control: the fixture window realizes",
        )?;
        let stride = row_stride(&hosted.adjustment, ROWS);
        recorder.measure("row_stride", stride);
        let resting = realized_rows(&hosted.list);
        recorder.measure("resting_realized_rows", resting.len());
        recorder.control(
            !resting.is_empty() && row_index_range(&resting).is_some_and(|(low, _)| low == 0),
            "control: a resting list realizes its first rows",
        )?;

        hosted.host.set_child_height(TALL_VIEWPORT);
        settle(LAYOUT_SETTLE);
        let rows_in_view = hosted.adjustment.page_size() / stride;
        let cap = GTK_LIST_VIEW_MAX_LIST_ITEMS + TRACKER_EXTRA_ROWS;
        recorder.measure("tall_page", hosted.adjustment.page_size());
        recorder.measure("tall_rows_in_view", rows_in_view);
        recorder.control(
            rows_in_view > f64::from(cap),
            "control: the tall viewport needs more rows than the cap to fill",
        )?;
        let tall = realized_rows(&hosted.list);
        recorder.measure("tall_realized_rows", tall.len());
        if let Some((low, high)) = row_index_range(&tall) {
            recorder.measure("tall_first_row", low);
            recorder.measure("tall_last_row", high);
        }
        recorder.axiom(
            u32::try_from(tall.len()).is_ok_and(|count| count <= cap),
            "A1: the list realizes at most the cap plus its tracker extras however tall its viewport",
        )?;

        hosted.host.set_child_height(LIST_HEIGHT);
        settle(LAYOUT_SETTLE);
        let target = f64::from(SCROLLED_ROW) * stride;
        hosted.adjustment.set_value(target);
        settle(LAYOUT_SETTLE);
        let first = realized_rows(&hosted.list);
        recorder.measure("first_far_move_value", hosted.adjustment.value());
        recorder.measure("first_far_move_mapped_rows", mapped_rows(&first));
        if let Some((low, high)) = row_index_range(&first) {
            recorder.measure("first_far_move_first_row", low);
            recorder.measure("first_far_move_last_row", high);
        }

        hosted.adjustment.set_value(target + 1.0);
        settle(LAYOUT_SETTLE);
        let scrolled = realized_rows(&hosted.list);
        let range = row_index_range(&scrolled);
        let mapped = mapped_rows(&scrolled);
        recorder.measure("scrolled_value", hosted.adjustment.value());
        // Not checked against the cap: rows kept for focus (row 0 here) stay
        // realized, unmapped, beside the visible range (recorded by A10).
        recorder.measure("scrolled_realized_rows", scrolled.len());
        recorder.measure("scrolled_mapped_rows", mapped);
        if let Some((low, high)) = range {
            recorder.measure("scrolled_first_row", low);
            recorder.measure("scrolled_last_row", high);
        }
        recorder.axiom(
            mapped > 0
                && scrolled
                    .iter()
                    .filter(|row| row.is_mapped())
                    .filter_map(row_index)
                    .any(|index| index == SCROLLED_ROW),
            "A1: the realized and mapped rows follow the list's own vadjustment",
        )
    })
}

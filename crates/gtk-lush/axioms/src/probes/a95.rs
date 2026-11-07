// SPDX-License-Identifier: MIT OR Apache-2.0

//! A95: on the A93 list, `GtkSingleSelection::set_selected(p)` plus
//! `GtkListView::scroll_to(p, …)` for a row far down realizes only around
//! `p`: its `get_item` calls stay bounded (the same 212 at 16,383 and
//! 131,071). Its time does grow with the row count, about as fast (1.3 ms
//! and 10.3 ms on GTK 4.22.5): the list view's height estimate over its
//! unrealized rows is not free, so the probe bounds the calls and records
//! the time.
//!
//! LushText's notes browser restores the selection and reaches the last row
//! (`End`, `win.select-notes-browser-row`) this way at any row count
//! (`bound-notes-browser-by-budget` D2); its widget test bounds that dispatch.

use std::time::{Duration, Instant};

use super::lazy_sections::{CountingSectionModel, sectioned_list};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// The row counts; the target is each one's last row.
pub const SIZES: [u32; 2] = [16 * 1024, 128 * 1024];
/// The most `get_item` calls one select-and-scroll may make.
pub const ITEM_CALL_CEILING: u64 = 1_024;

/// Select and scroll to the last of `rows`: time and `get_item` calls.
#[must_use]
pub fn time_select_far(rows: u32) -> (Duration, u64) {
    let model = CountingSectionModel::new(rows, 4_096);
    let (scroller, list, selection) = sectioned_list(&model);
    let _shown = Presented::detached(&scroller);
    flush_events();
    model.reset_calls();
    let target = rows - 1;
    let started = Instant::now();
    selection.set_selected(target);
    list.scroll_to(target, gtk4::ListScrollFlags::FOCUS, None);
    flush_events();
    (started.elapsed(), model.calls().0)
}

/// Probe A95. See the module documentation.
#[must_use]
pub fn probe_a95() -> Observation {
    Recorder::run(AxiomId::new(95), |recorder| {
        let [near, far] = SIZES;
        let (near_cost, near_items) = time_select_far(near);
        let (far_cost, far_items) = time_select_far(far);
        recorder.measure("near_us", near_cost.as_micros());
        recorder.measure("far_us", far_cost.as_micros());
        recorder.measure("near_item_calls", near_items);
        recorder.measure("far_item_calls", far_items);
        let growth = far_cost.as_secs_f64() / near_cost.as_secs_f64().max(1e-9);
        recorder.measure("growth", format!("{growth:.2}"));
        recorder.control(
            near_items > 0,
            "control: the list realized rows at the target",
        )?;
        recorder.axiom(
            near_items <= ITEM_CALL_CEILING && far_items <= ITEM_CALL_CEILING,
            "A95: selecting and scrolling to a far row realizes only around it",
        )
    })
}

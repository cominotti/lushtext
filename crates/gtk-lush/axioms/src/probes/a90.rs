// SPDX-License-Identifier: MIT OR Apache-2.0

//! A90: the dispatch that first shows a `GtkListView` inside a
//! `GtkScrolledWindow` that was invisible (the scrolled window turned
//! visible inside a presented window) costs in proportion to the rows the
//! list realizes, which A1 bounds, and not to the model's rows: revealing a
//! list over 16,384 rows costs about the same as over 256.
//!
//! LushText's workspace search panel (`bound-lists-by-budget-not-count` D4)
//! reveals its results list in the turn its first matches arrive; this bounds
//! what that reveal costs whatever the model holds.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use super::a87::{BOUNDED_GROWTH, label_list, numbered_store};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events, wait_until};
use crate::{AxiomId, Observation};

/// The model sizes compared.
pub const SIZES: [u32; 2] = [256, 16 * 1024];
/// Repetitions per size; the fastest is kept.
const REPETITIONS: u32 = 3;

/// The fastest reveal of a list over `rows`: the time from making its
/// scrolled window visible until the list is mapped, through the frames
/// that takes. Returns it with the realized row count.
#[must_use]
pub fn time_first_reveal(rows: u32) -> (Duration, usize) {
    let mut fastest = Duration::MAX;
    let mut realized = 0;
    for _ in 0..REPETITIONS {
        let store = numbered_store(rows);
        let list = label_list(&store);
        let scroller = gtk4::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .visible(false)
            .build();
        let host = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        host.append(&gtk4::Label::new(Some("header")));
        host.append(&scroller);
        let _shown = Presented::detached(&host);
        flush_events();
        let started = Instant::now();
        scroller.set_visible(true);
        let mapped = wait_until(Duration::from_secs(5), || list.is_mapped());
        let took = started.elapsed();
        if mapped {
            fastest = fastest.min(took);
        }
        realized = crate::fixtures::realized_rows(&list).len();
    }
    (fastest, realized)
}

/// Probe A90. See the module documentation.
#[must_use]
pub fn probe_a90() -> Observation {
    Recorder::run(AxiomId::new(90), |recorder| {
        let [small, large] = SIZES;
        let (small_cost, small_rows) = time_first_reveal(small);
        let (large_cost, large_rows) = time_first_reveal(large);
        recorder.measure("small_reveal_us", small_cost.as_micros());
        recorder.measure("large_reveal_us", large_cost.as_micros());
        recorder.measure("small_realized_rows", small_rows);
        recorder.measure("large_realized_rows", large_rows);
        recorder.control(
            small_cost < Duration::MAX && large_cost < Duration::MAX,
            "control: the list maps after its scrolled window turns visible",
        )?;
        let growth = large_cost.as_secs_f64() / small_cost.as_secs_f64().max(1e-9);
        recorder.measure("growth", format!("{growth:.2}"));
        recorder.axiom(
            large_rows <= 205 + 8,
            "A90: the revealed list realizes a bounded number of rows (A1)",
        )?;
        recorder.axiom(
            growth < BOUNDED_GROWTH,
            "A90: revealing a list costs O(realized rows), not O(model rows)",
        )
    })
}

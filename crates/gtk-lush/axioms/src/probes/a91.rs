// SPDX-License-Identifier: MIT OR Apache-2.0

//! A91: `TreeListRow::set_expanded(false)` on a row expanded over a large
//! child store removes every child row from the flattened `GtkTreeListModel`
//! in **one synchronous call whose cost grows with the children**, unlike
//! expanding it (A89) or appending a batch to it (A88). Nothing can slice
//! it: the row, the store and the list view hold no partial state between
//! "expanded" and "collapsed".
//!
//! LushText's workspace sidebar lists a directory of any size since
//! `bound-lists-by-budget-not-count`, so collapsing a directory of tens of
//! thousands of entries is one dispatch over `B_SETTLE_US`; the design
//! records it as a measured toolkit-floor exception citing this axiom.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use super::a87::label_list;
use super::a88::ExpandedTree;
use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// The child-store sizes compared (8 times the rows).
pub const SIZES: [u32; 2] = [16 * 1024, 128 * 1024];
/// The least growth, large over small, that the axiom asserts: a quarter of
/// the size ratio. A cost bounded like A88's or A89's (a batch, a position)
/// cannot reach it; a cost that follows the children does.
pub const LINEAR_FLOOR: f64 = 2.0;
/// Collapses timed per size; the fastest is kept.
const REPETITIONS: u32 = 3;

/// The fastest `set_expanded(false)` of a row expanded over `rows` children,
/// with the tree shown by a presented list, and the flattened item count
/// after the last collapse.
#[must_use]
pub fn time_collapse(rows: u32) -> (Duration, u32) {
    let fixture = ExpandedTree::new(rows);
    let list = label_list(&fixture.tree);
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    let _shown = Presented::detached(&scroller);
    let mut fastest = Duration::MAX;
    let Some(row) = fixture.tree.row(0) else {
        return (fastest, fixture.tree.n_items());
    };
    for _ in 0..REPETITIONS {
        if !row.is_expanded() {
            row.set_expanded(true);
            flush_events();
        }
        let started = Instant::now();
        row.set_expanded(false);
        fastest = fastest.min(started.elapsed());
        flush_events();
    }
    (fastest, fixture.tree.n_items())
}

/// Probe A91. See the module documentation.
#[must_use]
pub fn probe_a91() -> Observation {
    Recorder::run(AxiomId::new(91), |recorder| {
        let [small, large] = SIZES;
        let (small_cost, small_items) = time_collapse(small);
        let (large_cost, large_items) = time_collapse(large);
        recorder.control(
            small_items == 1 && large_items == 1,
            "control: each collapse removed every child row",
        )?;
        let growth = large_cost.as_secs_f64() / small_cost.as_secs_f64().max(1e-9);
        recorder.measure("small_collapse_us", small_cost.as_micros());
        recorder.measure("large_collapse_us", large_cost.as_micros());
        recorder.measure("growth", format!("{growth:.2}"));
        recorder.axiom(
            growth >= LINEAR_FLOOR,
            "A91: collapsing a row costs in proportion to its children, in one call",
        )
    })
}

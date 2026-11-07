// SPDX-License-Identifier: MIT OR Apache-2.0

//! A89 (extends A22): `TreeListRow::set_expanded(true)` on the **last** row
//! of a large flattened model costs a few tens of microseconds and grows
//! sub-linearly in the row's position: the last of 16,384 rows costs well
//! under 8 times the last of 1,024 (12 µs against 57 µs on GTK 4.22.5).
//!
//! LushText's workspace search panel (`bound-lists-by-budget-not-count` D4)
//! appends new file rows at the tail of its results model and expands them in
//! one pass per turn, by position, without walking the flattened model.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use super::a87::{BOUNDED_GROWTH, label_list};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// The root-row counts compared.
pub const SIZES: [u32; 2] = [1_024, 16 * 1024];
/// The children each root row's create function returns.
pub const CHILDREN: u32 = 4;
/// Tail rows expanded per size; the fastest single expansion is kept.
const TAIL_ROWS: u32 = 8;

/// A tree of `rows` collapsed root rows, each with [`CHILDREN`] children.
#[must_use]
pub fn collapsed_tree(rows: u32) -> gtk4::TreeListModel {
    let root = gtk4::gio::ListStore::new::<gtk4::StringObject>();
    let items: Vec<gtk4::StringObject> = (0..rows)
        .map(|index| gtk4::StringObject::new(&format!("file {index}")))
        .collect();
    root.splice(0, 0, &items);
    gtk4::TreeListModel::new(root, false, false, |item| {
        let name = item.downcast_ref::<gtk4::StringObject>()?.string();
        if !name.starts_with("file ") {
            return None;
        }
        let store = gtk4::gio::ListStore::new::<gtk4::StringObject>();
        for index in 0..CHILDREN {
            store.append(&gtk4::StringObject::new(&format!("{name}/match {index}")));
        }
        Some(store.upcast())
    })
}

/// The fastest `set_expanded(true)` of one of the last [`TAIL_ROWS`] rows of
/// a presented tree of `rows`, and the flattened item count after.
#[must_use]
pub fn time_tail_expansion(rows: u32) -> (Duration, u32) {
    let tree = collapsed_tree(rows);
    let list = label_list(&tree);
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    let _shown = Presented::detached(&scroller);
    let mut fastest = Duration::MAX;
    // Expand from the last row backwards, so each expanded row is still the
    // tail of the collapsed rows and its position is `n_items - 1`.
    for _ in 0..TAIL_ROWS {
        let last = tree.n_items() - 1;
        let Some(row) = tree.row(last) else {
            break;
        };
        let started = Instant::now();
        row.set_expanded(true);
        fastest = fastest.min(started.elapsed());
        flush_events();
        // Collapse it again so the next tail is a collapsed file row.
        row.set_expanded(false);
        flush_events();
    }
    (fastest, tree.n_items())
}

/// Probe A89. See the module documentation.
#[must_use]
pub fn probe_a89() -> Observation {
    Recorder::run(AxiomId::new(89), |recorder| {
        let [small, large] = SIZES;
        let (small_cost, small_items) = time_tail_expansion(small);
        let (large_cost, large_items) = time_tail_expansion(large);
        recorder.control(
            small_items == small && large_items == large,
            "control: each expansion was collapsed again",
        )?;
        let growth = large_cost.as_secs_f64() / small_cost.as_secs_f64().max(1e-9);
        recorder.measure("small_expand_us", small_cost.as_micros());
        recorder.measure("large_expand_us", large_cost.as_micros());
        recorder.measure("growth", format!("{growth:.2}"));
        recorder.axiom(
            growth < BOUNDED_GROWTH,
            "A89: expanding the last row grows sub-linearly in its position",
        )
    })
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! A88: one `splice` appending a batch at the end of an expanded row's child
//! `GListStore`, under a `GtkTreeListModel` (`passthrough = false`) shown by
//! a presented `GtkListView`, costs a small fraction of a list turn and grows
//! sub-linearly in the rows already in the store: 16 times the rows costs
//! well under 8 times as much (56 µs against 303 µs measured on GTK 4.22.5,
//! after 16,384 and 262,144 rows). The design's fallback, a growing custom
//! `GListModel`, is therefore not needed.
//!
//! LushText's workspace sidebar (`ui/sidebar/workspace_section/scan_execution.rs`,
//! `bound-lists-by-budget-not-count` D6) loads a directory of any size by
//! appending 256-row batches to its child store within a time slice; if the
//! splice grew with the store, a large directory could not fit the slice.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use super::a87::{BOUNDED_GROWTH, label_list, numbered_store};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// The child-store sizes compared.
pub const SIZES: [u32; 2] = [16 * 1024, 256 * 1024];
/// The batch appended at the end.
pub const BATCH: u32 = 256;
/// Batches timed per size; the fastest is kept.
const REPETITIONS: u32 = 5;

/// A tree whose one root row is expanded over a child store of `rows`.
#[derive(Clone, Debug)]
pub struct ExpandedTree {
    /// The flattened model.
    pub tree: gtk4::TreeListModel,
    /// The expanded row's child store.
    pub children: gtk4::gio::ListStore,
}

impl ExpandedTree {
    /// Build the tree and expand its root row.
    #[must_use]
    pub fn new(rows: u32) -> Self {
        let root = gtk4::gio::ListStore::new::<gtk4::StringObject>();
        root.append(&gtk4::StringObject::new("directory"));
        let children = numbered_store(rows);
        let child_model = children.clone();
        let tree = gtk4::TreeListModel::new(root, false, false, move |item| {
            let name = item.downcast_ref::<gtk4::StringObject>()?.string();
            (name == "directory").then(|| child_model.clone().upcast())
        });
        if let Some(row) = tree.row(0) {
            row.set_expanded(true);
        }
        Self { tree, children }
    }

    /// Splice one batch at the end of the child store, timed.
    #[must_use]
    pub fn append_batch(&self) -> Duration {
        let start = self.children.n_items();
        let items: Vec<gtk4::StringObject> = (0..BATCH)
            .map(|index| gtk4::StringObject::new(&format!("batch {}", start + index)))
            .collect();
        let started = Instant::now();
        self.children.splice(start, 0, &items);
        started.elapsed()
    }
}

/// The fastest end-of-store batch splice into a presented tree of `rows`.
#[must_use]
pub fn time_end_batch(rows: u32) -> (Duration, u32) {
    let fixture = ExpandedTree::new(rows);
    let list = label_list(&fixture.tree);
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    let _shown = Presented::detached(&scroller);
    let mut fastest = Duration::MAX;
    for _ in 0..REPETITIONS {
        fastest = fastest.min(fixture.append_batch());
        flush_events();
    }
    (fastest, fixture.tree.n_items())
}

/// Probe A88. See the module documentation.
#[must_use]
pub fn probe_a88() -> Observation {
    Recorder::run(AxiomId::new(88), |recorder| {
        let [small, large] = SIZES;
        let (small_cost, small_items) = time_end_batch(small);
        let (large_cost, large_items) = time_end_batch(large);
        recorder.measure("small_flattened_items", small_items);
        recorder.measure("large_flattened_items", large_items);
        recorder.control(
            small_items == 1 + small + REPETITIONS * BATCH
                && large_items == 1 + large + REPETITIONS * BATCH,
            "control: every batch entered the flattened model",
        )?;
        let growth = large_cost.as_secs_f64() / small_cost.as_secs_f64().max(1e-9);
        recorder.measure("small_batch_us", small_cost.as_micros());
        recorder.measure("large_batch_us", large_cost.as_micros());
        recorder.measure("growth", format!("{growth:.2}"));
        recorder.axiom(
            growth < BOUNDED_GROWTH,
            "A88: an end-of-store batch splice grows sub-linearly in the store",
        )
    })
}

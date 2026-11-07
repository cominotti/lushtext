// SPDX-License-Identifier: MIT OR Apache-2.0

//! A92: `AdwSidebar` in sidebar mode builds **one row widget per item**,
//! off-screen items included: it binds a `GtkListBox` to its flattened item
//! model with a create-row function (libadwaita 1.9.3 `src/adw-sidebar.c`,
//! `gtk_list_box_bind_model` at line 1973, `create_row` at 1334), and
//! `GtkListBox` does not recycle rows. Repopulating it (`remove_all`, then a
//! section of N items) therefore costs in proportion to N.
//!
//! LushText's notes browser listed every match in an `AdwSidebar` until
//! `bound-notes-browser-by-budget`, which is why it capped the list at 512
//! rows and why a 512-row publish took 30–39 ms; it now uses a virtualized
//! `GtkListView` (A93–A95).

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// The item counts compared (32 times the rows).
pub const SIZES: [u32; 2] = [512, 16 * 1024];
/// The least growth, large over small, the axiom asserts: a quarter of the
/// size ratio, which a cost bounded by the visible rows cannot reach.
pub const LINEAR_FLOOR: f64 = 8.0;

/// Repopulate a presented sidebar with `rows` items; return the time of
/// `remove_all` plus the append and the first flush, and the `GtkListBoxRow`
/// widgets that exist afterwards.
#[must_use]
pub fn time_repopulate(rows: u32) -> (Duration, usize) {
    let sidebar = libadwaita::Sidebar::new();
    sidebar.set_mode(libadwaita::SidebarMode::Sidebar);
    let first = libadwaita::SidebarSection::new();
    first.append(libadwaita::SidebarItem::new("seed"));
    sidebar.append(first);
    let _shown = Presented::detached(&sidebar);
    let started = Instant::now();
    sidebar.remove_all();
    let section = libadwaita::SidebarSection::new();
    section.set_title(Some("Rows"));
    for index in 0..rows {
        section.append(libadwaita::SidebarItem::new(&format!("row {index}")));
    }
    sidebar.append(section);
    flush_events();
    let elapsed = started.elapsed();
    (elapsed, count_list_box_rows(sidebar.upcast_ref()))
}

fn count_list_box_rows(root: &gtk4::Widget) -> usize {
    let mut count = 0;
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        if widget.is::<gtk4::ListBoxRow>() {
            count += 1;
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            stack.push(next.clone());
            child = next.next_sibling();
        }
    }
    count
}

/// Probe A92. See the module documentation.
#[must_use]
pub fn probe_a92() -> Observation {
    Recorder::run(AxiomId::new(92), |recorder| {
        let [small, large] = SIZES;
        let (small_cost, small_rows) = time_repopulate(small);
        let (large_cost, large_rows) = time_repopulate(large);
        recorder.measure("small_repopulate_us", small_cost.as_micros());
        recorder.measure("large_repopulate_us", large_cost.as_micros());
        recorder.measure("small_rows", small_rows);
        recorder.measure("large_rows", large_rows);
        let growth = large_cost.as_secs_f64() / small_cost.as_secs_f64().max(1e-9);
        recorder.measure("growth", format!("{growth:.2}"));
        recorder.control(small_rows > 0, "control: the sidebar built its rows")?;
        recorder.axiom(
            small_rows == small as usize && large_rows == large as usize,
            "A92: AdwSidebar builds one row widget per item, off-screen ones included",
        )?;
        recorder.axiom(
            growth >= LINEAR_FLOOR,
            "A92: repopulating an AdwSidebar costs in proportion to its items",
        )
    })
}

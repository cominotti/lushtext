// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A19: a `GtkViewport` emits `notify::page-size` only after it
//! has allocated its child. Scroll to the end, then grow the viewport: the
//! readout shows the clamp's `value-changed` before the child's allocation,
//! and both notifications after it, all in one frame. "Queue from the next
//! page-size notify" followed by a resize shows that allocation not being
//! served in its frame.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::ViewportOrder;
use gtk_lush_axioms::fixtures::a19::{
    GROWN_HEIGHT, RESTING_HEIGHT, SHRUNK_HEIGHT, VIEWPORT_WIDTHS,
};
use gtk4::prelude::*;

/// How many of the latest logged events the readout shows.
const SHOWN_EVENTS: usize = 8;

fn build(ui: &support::SampleUi) {
    let fixture = ViewportOrder::new();
    ui.set_fixture(&fixture.host);
    let watching = fixture.clone();
    fixture.probe.connect_map(move |_| {
        let _ = watching.watch_layout_ends();
    });
    let scrolling = fixture.clone();
    ui.add_control("Scroll to the end", move || scrolling.scroll_to_end());
    for (label, width, height) in [
        ("Rest", VIEWPORT_WIDTHS[0], RESTING_HEIGHT),
        ("Grow", VIEWPORT_WIDTHS[1], GROWN_HEIGHT),
        ("Shrink", VIEWPORT_WIDTHS[2], SHRUNK_HEIGHT),
    ] {
        let resizing = fixture.clone();
        ui.add_control(&format!("{label} ({width} × {height})"), move || {
            resizing.set_viewport_size(width, height);
        });
    }
    let queueing = fixture.clone();
    ui.add_control("Queue from the next page-size notify", move || {
        queueing.queue_allocate_from_next_page_notify();
    });
    ui.set_readout(move || {
        let log = fixture.log();
        let latest = log
            .iter()
            .skip(log.len().saturating_sub(SHOWN_EVENTS))
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | ");
        format!(
            "child allocations {}   last queue {:?}\n{latest}",
            fixture.probe.allocations(),
            fixture.queued_from_notify()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(19), build)
}

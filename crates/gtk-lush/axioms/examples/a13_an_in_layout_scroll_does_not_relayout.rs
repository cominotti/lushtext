// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A13: moving the outer adjustment from inside layout does not
//! reliably schedule a relayout. The host re-allocates on every outer move, as
//! a slice bin re-slices; a move made from an idle re-allocates it, while the
//! same move made inside its own allocation leaves the count unchanged.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::ReslicingHost;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = ReslicingHost::new();
    ui.set_fixture(&fixture.scroller);
    let outer = fixture.scroller.vadjustment();
    ui.add_control("Move outer to 250 from an idle", move || {
        outer.set_value(250.0);
    });
    let in_layout = fixture.clone();
    ui.add_control("Move outer to 500 inside the next allocation", move || {
        drop(in_layout.move_outer_inside_next_allocation(500.0));
    });
    let outer = fixture.scroller.vadjustment();
    ui.add_control("Back to the top", move || {
        outer.set_value(0.0);
    });
    let outer = fixture.scroller.vadjustment();
    let host = fixture.host;
    ui.set_readout(move || {
        format!(
            "outer value {}   host allocations {}",
            outer.value(),
            host.allocations()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(13), build)
}

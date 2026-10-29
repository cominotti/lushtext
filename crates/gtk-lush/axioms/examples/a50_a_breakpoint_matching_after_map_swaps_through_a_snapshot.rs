// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A50: a bin with a `max-width: 600sp` breakpoint whose setter
//! writes the label. Resize the bin across the condition, or add a second
//! matching breakpoint after map, and watch the child-visible flag drop for
//! one frame while the bin draws a snapshot of the old state.

mod support;

use std::cell::Cell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::BreakpointFixture;
use gtk_lush_axioms::fixtures::a50::{CONDITION_SP, MATCHING_WIDTH, WIDE_WIDTH};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = BreakpointFixture::new(WIDE_WIDTH);
    if fixture
        .add_max_width_breakpoint(CONDITION_SP, "applied")
        .is_none()
    {
        return;
    }
    ui.set_fixture(&fixture.host);
    for width in [WIDE_WIDTH, MATCHING_WIDTH] {
        let host = fixture.host.clone();
        ui.add_control(&format!("Width {width} px"), move || {
            host.set_child_width(width);
        });
    }
    let adder = fixture.clone();
    ui.add_control("Add a matching breakpoint", move || {
        let _ = adder.add_max_width_breakpoint(CONDITION_SP + 400, "added after map");
    });
    let hidden_frames = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&hidden_frames);
    let content = fixture.content.clone();
    let _tick = fixture.host.add_tick_callback(move |_, _| {
        if !content.is_child_visible() {
            counter.set(counter.get() + 1);
        }
        gtk4::glib::ControlFlow::Continue
    });
    let label = fixture.label;
    ui.set_readout(move || {
        format!(
            "label: {}   frames drawn from the snapshot so far: {}",
            label.text(),
            hidden_frames.get()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(50), build)
}

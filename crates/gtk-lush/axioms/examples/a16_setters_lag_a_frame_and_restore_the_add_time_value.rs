// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A16: a `max-width: 600sp` breakpoint whose setter writes the
//! label. Resize the bin across the edge, and write the label yourself while
//! the breakpoint is applied: the next unapply restores the text the label had
//! when the setter was added, not your write.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::BreakpointFixture;
use gtk_lush_axioms::fixtures::a16::{
    APPLIED_LABEL, CONDITION_SP, NARROW_WIDTH, WIDE_WIDTH, WRITE_BEFORE_ADDING_THE_SETTER,
    WRITE_WHILE_APPLIED,
};
use libadwaita::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = BreakpointFixture::new(WIDE_WIDTH);
    fixture.label.set_text(WRITE_BEFORE_ADDING_THE_SETTER);
    let Some(breakpoint) = fixture.add_max_width_breakpoint(CONDITION_SP, APPLIED_LABEL) else {
        return;
    };
    ui.set_fixture(&fixture.host);
    for width in [NARROW_WIDTH, WIDE_WIDTH] {
        let host = fixture.host.clone();
        ui.add_control(&format!("Width {width} px"), move || {
            host.set_child_width(width);
        });
    }
    let label = fixture.label.clone();
    ui.add_control("Write the label", move || {
        label.set_text(WRITE_WHILE_APPLIED);
    });
    let bin = fixture.bin.clone();
    let label = fixture.label;
    ui.set_readout(move || {
        let applied = bin.current_breakpoint().as_ref() == Some(&breakpoint);
        format!(
            "bin width {} px   applied: {applied}   label: {}",
            bin.width(),
            label.text()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(16), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A18: one bin carries a `max-width: 900sp` breakpoint added
//! first and a `max-width: 500sp` breakpoint added second, each writing its
//! name into the label. Below both edges both match, and only the last one
//! added applies. (The probe also measures the window-minimum half of the
//! axiom on three plain windows; that half needs no interaction.)

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::BreakpointFixture;
use gtk_lush_axioms::fixtures::a18::{BELOW_BOTH_WIDTH, BETWEEN_WIDTH, NARROW_SP, WIDE_SP};
use libadwaita::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = BreakpointFixture::new(BETWEEN_WIDTH);
    let (Some(wide), Some(narrow)) = (
        fixture.add_max_width_breakpoint(WIDE_SP, "wide"),
        fixture.add_max_width_breakpoint(NARROW_SP, "narrow"),
    ) else {
        return;
    };
    ui.set_fixture(&fixture.host);
    for width in [BETWEEN_WIDTH, BELOW_BOTH_WIDTH] {
        let host = fixture.host.clone();
        ui.add_control(&format!("Width {width} px"), move || {
            host.set_child_width(width);
        });
    }
    let bin = fixture.bin.clone();
    let label = fixture.label;
    ui.set_readout(move || {
        let current = bin.current_breakpoint();
        let name = if current.as_ref() == Some(&wide) {
            "wide (900sp, added first)"
        } else if current.as_ref() == Some(&narrow) {
            "narrow (500sp, added last)"
        } else {
            "none"
        };
        format!(
            "bin width {} px   current breakpoint: {name}   label: {}",
            bin.width(),
            label.text()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(18), build)
}

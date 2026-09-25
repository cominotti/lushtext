// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A14: a `max-width: 601sp` breakpoint on an `AdwBreakpointBin`
//! that the host allocates at an exact width. Step the width across the edge
//! and change the text scale: the breakpoint matches up to and including
//! floor(601 × scale) px — 601, 751, 901 at 1.0, 1.25, 1.5.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a14::{APPLIED_LABEL, CONDITION_SP, RESCALE_WIDTH, TEXT_SCALES};
use gtk_lush_axioms::fixtures::{BASE_XFT_DPI, BreakpointFixture, set_text_scale};
use libadwaita::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = BreakpointFixture::new(RESCALE_WIDTH);
    let Some(breakpoint) = fixture.add_max_width_breakpoint(CONDITION_SP, APPLIED_LABEL) else {
        return;
    };
    ui.set_fixture(&fixture.host);
    for scale in TEXT_SCALES {
        ui.add_control(&format!("Text scale {scale}‰"), move || {
            set_text_scale(scale);
        });
    }
    for scale in TEXT_SCALES {
        let edge = CONDITION_SP * scale / 1000;
        for width in [edge, edge + 1] {
            let host = fixture.host.clone();
            ui.add_control(&format!("Width {width} px"), move || {
                host.set_child_width(width);
            });
        }
    }
    ui.set_readout(move || {
        let dpi = gtk4::Settings::default().map_or(-1, |settings| settings.gtk_xft_dpi());
        let applied = fixture.is_current(&breakpoint);
        format!(
            "gtk-xft-dpi {dpi} (base {BASE_XFT_DPI})   bin width {} px   max-width: \
             {CONDITION_SP}sp applied: {applied}",
            fixture.bin.width()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(14), build)
}

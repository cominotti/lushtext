// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A2: a list outside a scroller reports its whole content as its
//! minimum height. The readout measures the list directly; its minimum stays
//! the full content height whatever height the host allocates it.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{HostedList, LIST_HEIGHT};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    for (label, height) in [("Allocate 150 px", 150), ("Allocate 300 px", LIST_HEIGHT)] {
        let host = hosted.host.clone();
        ui.add_control(label, move || {
            host.set_child_height(height);
        });
    }
    let list = hosted.list.clone();
    let adjustment = hosted.adjustment.clone();
    let host = hosted.host;
    ui.set_readout(move || {
        // Measure only once the list is on screen, for the width it has.
        if !list.is_mapped() || list.width() <= 0 {
            return "measuring once the list is shown…".to_owned();
        }
        let (minimum, natural, _, _) = list.measure(gtk4::Orientation::Vertical, list.width());
        format!(
            "allocated {} px   content (upper) {}   measured minimum {minimum}   natural {natural}",
            host.child_height(),
            adjustment.upper()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(2), build)
}

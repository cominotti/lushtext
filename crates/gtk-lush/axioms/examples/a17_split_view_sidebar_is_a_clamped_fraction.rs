// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A17: an `AdwOverlaySplitView` whose sidebar is its fraction of
//! the width, clamped to its sp minimum and maximum. Change the sizing and the
//! text scale, and toggle `show-sidebar` and `collapsed`: the sidebar moves,
//! the window does not.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a17::{SCALED_TEXT, SIDEBAR_REQUEST, WINDOW_WIDTH};
use gtk_lush_axioms::fixtures::{SplitViewFixture, set_text_scale};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = SplitViewFixture::new();
    ui.set_fixture(&fixture.split);
    for (label, fraction, min, max) in [
        ("0.25 in [180, 280] sp (default)", 0.25, 180.0, 280.0),
        ("0.25 in [300, 400] sp", 0.25, 300.0, 400.0),
        ("0.5 in [100, 200] sp", 0.5, 100.0, 200.0),
    ] {
        let fixture = fixture.clone();
        ui.add_control(label, move || fixture.size_sidebar(fraction, min, max));
    }
    for scale in [1_000, SCALED_TEXT] {
        ui.add_control(&format!("Text scale {scale}‰"), move || {
            set_text_scale(scale);
        });
    }
    let sidebar = fixture.sidebar.clone();
    ui.add_control(&format!("Sidebar asks {SIDEBAR_REQUEST} px"), move || {
        let request = if sidebar.width_request() == SIDEBAR_REQUEST {
            -1
        } else {
            SIDEBAR_REQUEST
        };
        sidebar.set_width_request(request);
    });
    let split = fixture.split.clone();
    ui.add_control("Toggle show-sidebar", move || {
        split.set_show_sidebar(!split.shows_sidebar());
    });
    let split = fixture.split.clone();
    ui.add_control("Toggle collapsed", move || {
        split.set_collapsed(!split.is_collapsed());
    });
    let split = fixture.split.clone();
    let sidebar = fixture.sidebar.clone();
    let content = fixture.content;
    ui.set_readout(move || {
        let window = split
            .root()
            .map_or(0, |root| root.upcast_ref::<gtk4::Widget>().width());
        format!(
            "split {} px (the probe uses {WINDOW_WIDTH})   sidebar {} px   content {} px   \
             window {window} px   shown {}   collapsed {}",
            split.width(),
            sidebar.width(),
            content.width(),
            split.shows_sidebar(),
            split.is_collapsed()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(17), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A101: a view and its source map, with an orange strip overlaid
//! on the map's right edge. Scroll the view: the map follows, and the strip's
//! draw count does not move. "Queue a draw" redraws it once.

mod support;

use gtk4::prelude::*;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a101::OverlaidMap;

fn build(ui: &support::SampleUi) {
    let overlaid = OverlaidMap::new();
    ui.set_fixture(&overlaid.fixture.root);
    {
        let strip = overlaid.strip.clone();
        ui.add_control("Queue a draw", move || strip.queue_draw());
    }
    {
        let overlaid = overlaid.clone();
        ui.add_control("Scroll to the middle", move || {
            overlaid.scroll_editor_to(0.5);
        });
    }
    ui.set_readout(move || {
        format!(
            "strip draws: {}\nmap scroll value: {:.0}",
            overlaid.draws.get(),
            overlaid.map_value()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(101), build)
}

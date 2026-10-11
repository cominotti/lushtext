// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A32 (a defect axiom): detaching a `GtkSourceMap` from its view
//! while its update tick is pending logs GTK `CRITICAL`s. "Scroll, then
//! detach" does both in one main-loop turn; watch the terminal for the
//! criticals (this sample does not capture them). "Reattach" restores the
//! map.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a32::MappedView;
use sourceview5::prelude::*;

fn build(ui: &support::SampleUi) {
    let Some(fixture) = MappedView::new() else {
        ui.set_readout(|| "GtkSourceView has no html language".to_owned());
        return;
    };
    ui.set_fixture(&fixture.container);
    {
        let fixture = fixture.clone();
        ui.add_control("Scroll the view", move || fixture.scroll());
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Scroll, then detach the map", move || {
            fixture.scroll();
            fixture.detach();
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Reattach the map", move || {
            fixture.map.set_view(&fixture.source.view);
        });
    }
    ui.set_readout(move || format!("map attached: {}", fixture.map.view().is_some()));
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(32), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A113: a tree whose rows are built like LushText's. Click an
//! arrow (GTK toggles the row), press `+`/`-` with the row focused (nothing)
//! and after Tab has moved the focus onto the expander (the row expands or
//! collapses), and
//! turn the release claim on to see a click on the arrow toggle nothing. The
//! readout shows where the click gesture lives and the claims taken.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a113::{
    CLASS_ACTIONS, click_gestures, disclosure_fixture, expander_icon,
};

fn build(ui: &support::SampleUi) {
    let fixture = disclosure_fixture();
    ui.set_fixture(&fixture.column);
    {
        let fixture = fixture.clone();
        ui.add_control("Release claim on / off", move || {
            fixture.claim_enabled.set(!fixture.claim_enabled.get());
        });
    }
    ui.set_readout(move || {
        let gestures = fixture.expander_at(0).map(|expander| {
            (
                click_gestures(&expander),
                expander_icon(&expander).map(|icon| click_gestures(&icon)),
            )
        });
        format!(
            "claim {}, claims {}, activations {}; click gestures (expander, icon) {gestures:?}; \
             class actions {CLASS_ACTIONS:?}",
            fixture.claim_enabled.get(),
            fixture.claims.get(),
            fixture.activations.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(113), build)
}

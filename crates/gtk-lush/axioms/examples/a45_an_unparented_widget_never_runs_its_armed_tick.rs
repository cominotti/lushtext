// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A45: a widget unparented while its tick callback is armed never
//! runs it, while an armed sibling kept in the tree does. "Arm both, then
//! unparent one" does both in one turn; the readout counts each widget's
//! ticks.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a45::TickPair;

fn build(ui: &support::SampleUi) {
    let pair = TickPair::new();
    ui.set_fixture(&pair.container);
    {
        let pair = pair.clone();
        ui.add_control("Arm both, then unparent one", move || {
            pair.arm();
            pair.unparent_retired();
        });
    }
    ui.set_readout(move || {
        format!(
            "kept ticks {}   unparented ticks {}",
            pair.kept_ticks.get(),
            pair.retired_ticks.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(45), build)
}

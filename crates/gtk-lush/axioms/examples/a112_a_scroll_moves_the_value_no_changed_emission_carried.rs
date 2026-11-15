// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A112: a text view's vertical adjustment, logging each
//! `changed` and `value-changed` with the value it read. "Scroll to line
//! 1,000" calls `scroll_to_mark` (value-changed to a value no changed
//! carried); resizing the window reconfigures the adjustment (changed first,
//! already carrying the value its value-changed then reports).

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a112::{EmissionLog, document_view, scroll_to_target};

fn build(ui: &support::SampleUi) {
    let fixture = document_view();
    ui.set_fixture(&fixture.scrolled);
    let log = EmissionLog::attach(&fixture.scrolled.vadjustment());
    ui.add_control("Scroll to line 1,000", move || scroll_to_target(&fixture));
    ui.set_readout(move || {
        let emissions = log.emissions.borrow();
        let tail = emissions.len().saturating_sub(4);
        format!("last emissions {:?}", &emissions[tail..])
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(112), build)
}

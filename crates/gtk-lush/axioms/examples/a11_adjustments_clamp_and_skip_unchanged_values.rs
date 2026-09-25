// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A11: `configure` and `set_value` clamp into
//! `[lower, upper − page]`, and a `set_value` that leaves the value unchanged
//! emits nothing. The scrollbar shows the adjustment; the buttons ask for
//! values outside its range and count the `value-changed` emissions.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::count_value_changes;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let adjustment = gtk4::Adjustment::new(0.0, 0.0, 300.0, 1.0, 100.0, 100.0);
    let scrollbar = gtk4::Scrollbar::new(gtk4::Orientation::Horizontal, Some(&adjustment));
    scrollbar.set_valign(gtk4::Align::Center);
    ui.set_fixture(&scrollbar);
    let emissions = count_value_changes(&adjustment);
    for (label, value) in [
        ("set_value(-5)", -5.0),
        ("set_value(150)", 150.0),
        ("set_value(1000)", 1_000.0),
        ("set_value(5000)", 5_000.0),
    ] {
        let adjustment = adjustment.clone();
        ui.add_control(label, move || {
            adjustment.set_value(value);
        });
    }
    let target = adjustment.clone();
    ui.add_control("configure(500, 0, 300, page 100)", move || {
        target.configure(500.0, 0.0, 300.0, 1.0, 100.0, 100.0);
    });
    ui.set_readout(move || {
        format!(
            "value {}   range [{}, {}]   value-changed ×{}",
            adjustment.value(),
            adjustment.lower(),
            adjustment.upper() - adjustment.page_size(),
            emissions.get()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(11), build)
}

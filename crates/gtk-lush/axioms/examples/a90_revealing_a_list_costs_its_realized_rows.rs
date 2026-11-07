// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A90: reveal a list view whose scrolled window was invisible,
//! over models of two sizes. The cost follows the realized rows, not the
//! model.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a90::{SIZES, time_first_reveal};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Reveals list views over {SIZES:?} rows in a separate window."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Measure", move || {
            let costs: Vec<String> = SIZES
                .iter()
                .map(|rows| {
                    let (cost, realized) = time_first_reveal(*rows);
                    format!("{rows} rows: {} us, {realized} realized", cost.as_micros())
                })
                .collect();
            *readout.borrow_mut() = costs.join("\n");
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(90), build)
}

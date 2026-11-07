// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A88: splice a 256-row batch at the end of an expanded row's
//! child store under a presented tree list, at two store sizes. The cost
//! follows the batch, not the store.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a88::{BATCH, SIZES, time_end_batch};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Splices {BATCH} rows at the end of child stores of {SIZES:?} rows."
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
                    let (cost, items) = time_end_batch(*rows);
                    format!("{rows} rows: {} us ({items} flattened)", cost.as_micros())
                })
                .collect();
            *readout.borrow_mut() = costs.join("\n");
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(88), build)
}

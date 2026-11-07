// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A91: collapse a row expanded over child stores of two sizes,
//! under a presented tree list. The cost follows the children removed.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a91::{SIZES, time_collapse};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Collapses a row expanded over child stores of {SIZES:?} rows."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Measure", move || {
            let costs: Vec<String> = SIZES
                .iter()
                .map(|rows| format!("{rows} children: {} us", time_collapse(*rows).0.as_micros()))
                .collect();
            *readout.borrow_mut() = costs.join("\n");
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(91), build)
}

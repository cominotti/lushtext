// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A92: repopulate a presented `AdwSidebar` with 512 and 16,384
//! items and count the row widgets it built; every item gets one.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a92::{SIZES, time_repopulate};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Repopulates an AdwSidebar with {SIZES:?} items and counts its rows."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Measure", move || {
            let text = SIZES
                .iter()
                .map(|rows| {
                    let (cost, built) = time_repopulate(*rows);
                    format!("{rows} items: {built} rows in {} us", cost.as_micros())
                })
                .collect::<Vec<_>>()
                .join("\n");
            *readout.borrow_mut() = text;
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(92), build)
}

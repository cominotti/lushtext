// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A95: select and scroll to the last row of 16,384 and 131,072,
//! and count the model calls each makes.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a95::{SIZES, time_select_far};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Selects and scrolls to the last of {SIZES:?} rows."
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
                    let (cost, items) = time_select_far(*rows);
                    format!("{rows} rows: {} us, {items} get_item", cost.as_micros())
                })
                .collect::<Vec<_>>()
                .join("\n");
            *readout.borrow_mut() = text;
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(95), build)
}

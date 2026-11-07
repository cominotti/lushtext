// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A89: expand the last row of a flattened tree of two sizes. The
//! cost follows the children added, not the row's position.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a89::{CHILDREN, SIZES, time_tail_expansion};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Expands the tail rows ({CHILDREN} children each) of trees of {SIZES:?} rows."
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
                    format!(
                        "{rows} rows: {} us",
                        time_tail_expansion(*rows).0.as_micros()
                    )
                })
                .collect();
            *readout.borrow_mut() = costs.join("\n");
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(89), build)
}

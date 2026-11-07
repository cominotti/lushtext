// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A87: time 256 single appends, and one 256-item splice, to a
//! store under a list view at two store sizes, with the list unparented,
//! under an invisible scrolled window, and presented. No state grows with
//! the store; on the presented list one splice costs a fraction of the
//! single appends.

mod support;

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a87::{
    APPENDS, ListState, SIZES, time_one_splice, time_single_appends,
};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Times {APPENDS} single appends at {SIZES:?} rows in each list state."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Measure", move || {
            let mut text = String::new();
            for state in [
                ListState::Unparented,
                ListState::HiddenInPresentedWindow,
                ListState::Presented,
            ] {
                let costs: Vec<u128> = SIZES
                    .iter()
                    .map(|rows| time_single_appends(*rows, state).as_micros())
                    .collect();
                let splices: Vec<u128> = SIZES
                    .iter()
                    .map(|rows| time_one_splice(*rows, state).as_micros())
                    .collect();
                let _ = writeln!(
                    text,
                    "{state:?}: appends {costs:?} us, one splice {splices:?} us"
                );
            }
            *readout.borrow_mut() = text;
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(87), build)
}

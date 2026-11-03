// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A81: three busy one-shot idles at one priority run in one
//! main-loop iteration, one step lower waits for the next, and one of the same
//! priority added during the dispatch runs later. Press the button and read
//! which callbacks each iteration ran.

mod support;

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a81::{BUSY, trace_co_dispatch};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Arms three idles of {} ms each at G_PRIORITY_DEFAULT_IDLE, one at \
         G_PRIORITY_DEFAULT_IDLE + 1, and one more at the same priority from the first callback.",
        BUSY.as_millis()
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Arm and iterate", move || {
            let mut text = String::new();
            for (index, iteration) in trace_co_dispatch().iterations.iter().enumerate() {
                let _ = writeln!(
                    text,
                    "iteration {index}: {:?} in {} us",
                    iteration.ran, iteration.micros
                );
            }
            *readout.borrow_mut() = text;
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(81), build)
}

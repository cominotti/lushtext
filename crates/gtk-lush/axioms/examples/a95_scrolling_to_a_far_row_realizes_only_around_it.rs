// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A95: select and scroll to the last row of 16,384 and 131,072
//! (and of 262,144 and 1,048,576 when slow probes are included), and count the
//! model calls each makes.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::fixtures::a95::{SIZES, SLOW_SIZES, time_select_far};
use gtk_lush_axioms::{AxiomId, include_slow_probes};

fn sizes() -> Vec<u32> {
    let mut sizes = SIZES.to_vec();
    if include_slow_probes() {
        sizes.extend(SLOW_SIZES);
    }
    sizes
}

fn build(ui: &support::SampleUi) {
    let sizes = sizes();
    let note = gtk4::Label::new(Some(&format!(
        "Selects and scrolls to the last of {sizes:?} rows."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Measure", move || {
            let text = sizes
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

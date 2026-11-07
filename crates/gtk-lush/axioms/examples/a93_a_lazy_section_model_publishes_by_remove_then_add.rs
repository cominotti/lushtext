// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A93: publish 512 and 131,072 rows into a presented, sectioned
//! list over a lazy model, in one emission and as remove-then-add, and count
//! the model calls each publish makes.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a93::{Publish, SIZES, fresh_items_keep_working, time_publish};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Publishes {SIZES:?} rows into a lazy sectioned list and counts get_item calls."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Measure", move || {
            let mut lines = Vec::new();
            for publish in [Publish::OneStep, Publish::RemoveThenAdd] {
                for rows in SIZES {
                    let (cost, items, sections) = time_publish(rows, publish);
                    lines.push(format!(
                        "{publish:?} {rows} rows: {} us, {items} get_item, {sections} get_section",
                        cost.as_micros()
                    ));
                }
            }
            lines.push(format!(
                "fresh items accepted: {}",
                fresh_items_keep_working(SIZES[1])
            ));
            *readout.borrow_mut() = lines.join("\n");
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(93), build)
}

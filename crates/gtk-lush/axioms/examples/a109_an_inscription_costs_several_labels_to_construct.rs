// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A109: construct the realized band's worth of `GtkLabel`s and
//! of `GtkInscription`s and compare the time.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a109::{WIDGETS, construct_us, inscription, label};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Construct constructs {WIDGETS} labels and {WIDGETS} inscriptions (median of seven \
         rounds each)."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press Construct")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Construct", move || {
            let labels = construct_us(label);
            let inscriptions = construct_us(inscription);
            *readout.borrow_mut() =
                format!("{WIDGETS} labels: {labels} us\n{WIDGETS} inscriptions: {inscriptions} us");
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(109), build)
}

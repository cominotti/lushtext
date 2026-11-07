// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A94: a sectioned list's headers are not focusable, and focus
//! moves from a section's last row to the next section's first row.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a94::observe_headers;

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(
        "Builds a sectioned list, then moves focus down across a section boundary.",
    ));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Observe", move || {
            let (headers, focusable, role, focused) = observe_headers();
            *readout.borrow_mut() = format!(
                "{headers} headers, {focusable} focusable, role {role}, focus after Down: {}",
                focused.unwrap_or_default()
            );
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(94), build)
}

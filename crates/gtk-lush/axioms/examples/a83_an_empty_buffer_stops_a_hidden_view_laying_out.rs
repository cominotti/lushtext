// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A83: load a document into a hidden tab page, give its view an
//! empty buffer while it validates, then give the document back, and read how
//! long a G_PRIORITY_DEFAULT_IDLE source waited each time.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a82::{describe, load_and_trace, trace_lower_delivery};
use gtk_lush_axioms::fixtures::a83::DetachFixture;

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(
        "Opens a probe window with an AdwTabView, loads a document into the hidden page, \
         detaches its buffer, then attaches it again.",
    ));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Load, detach, reattach", move || {
            let text = DetachFixture::standalone().map_or_else(
                || "the fixture window never realized".to_owned(),
                |fixture| {
                    let loaded = load_and_trace(&fixture.document.buffer);
                    fixture.detach();
                    let detached = trace_lower_delivery();
                    fixture.attach();
                    let reattached = trace_lower_delivery();
                    format!(
                        "loaded hidden: {}\ndetached: {}\nreattached: {}",
                        describe(loaded),
                        describe(detached),
                        describe(reattached)
                    )
                },
            );
            *readout.borrow_mut() = text;
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(83), build)
}

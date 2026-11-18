// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A117: announcing through a `GtkTestATContext` kills the
//! process. The fixture is a label; the two controls run the announcement in
//! a child of this sample, under `GTK_A11Y=test` and under `GTK_A11Y=none`,
//! so the crash never takes the sample window with it. The readout shows
//! this window's own AT context and each child's outcome (the `test` child is
//! killed by signal 11; the `none` child returns and exits 0). `--check` runs
//! the probe.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::{AxiomId, announce_in_child};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some("A labelled fixture"));
    ui.set_fixture(&label);
    let outcomes = Rc::new(RefCell::new(Vec::<String>::new()));
    for backend in ["test", "none"] {
        let outcomes = Rc::clone(&outcomes);
        ui.add_control(
            &format!("Announce in a GTK_A11Y={backend} child"),
            move || {
                let outcome = announce_in_child(backend).map_or_else(
                    |error| format!("did not start ({error})"),
                    |outcome| outcome.summary(),
                );
                outcomes
                    .borrow_mut()
                    .push(format!("GTK_A11Y={backend}: {outcome}"));
            },
        );
    }
    ui.set_readout(move || {
        let own = if std::env::var("GTK_A11Y").as_deref() == Ok("none") {
            "(no context)".to_owned()
        } else {
            label.at_context().type_().name().to_owned()
        };
        let children = outcomes.borrow();
        let last = if children.is_empty() {
            "no child run yet".to_owned()
        } else {
            children.join("; ")
        };
        format!(
            "GTK_A11Y={}; this window's AT context: {own}; {last}",
            std::env::var("GTK_A11Y").unwrap_or_else(|_| "(unset)".to_owned()),
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(117), build)
}

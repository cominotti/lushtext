// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A80: while a `GtkTextView` has lines left to validate, a
//! `G_PRIORITY_DEFAULT_IDLE` source waits for the whole buffer, while a source
//! one step above the validation idle runs at once. Load the document into the
//! view on the hidden page (or the shown one) and read when each source ran.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a80::{DeliveryFixture, DeliveryTrace, Host, LINES};

fn describe(trace: &DeliveryTrace) -> String {
    let line = |name: &str, delivery: Option<gtk_lush_axioms::fixtures::a80::Delivery>| {
        delivery.map_or_else(
            || format!("{name}: never"),
            |d| {
                format!(
                    "{name}: iteration {}, {} ms, validated y {}",
                    d.iteration,
                    d.micros / 1000,
                    d.validated_y
                )
            },
        )
    };
    format!(
        "{}\n{}\nfinal y {}",
        line("G_PRIORITY_DEFAULT_IDLE", trace.default_idle),
        line("one above validation", trace.above_validate),
        trace.final_y
    )
}

fn build(ui: &support::SampleUi) {
    // Each run opens its own probe window (the view on screen, or on a hidden
    // stack page) and closes it when the trace ends.
    let note = gtk4::Label::new(Some(
        "Each button opens a probe window, loads the document and reports when a \
         G_PRIORITY_DEFAULT_IDLE source and a source one step above the validation idle ran.",
    ));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press a button")));
    for host in [Host::HiddenPage, Host::Presented] {
        let readout = Rc::clone(&readout);
        ui.add_control(
            &format!("Load {LINES} lines into the {} view", host.name()),
            move || {
                let text = DeliveryFixture::standalone(host).map_or_else(
                    || "the fixture window never realized".to_owned(),
                    |fixture| describe(&fixture.trace(true)),
                );
                *readout.borrow_mut() = text;
            },
        );
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(80), build)
}

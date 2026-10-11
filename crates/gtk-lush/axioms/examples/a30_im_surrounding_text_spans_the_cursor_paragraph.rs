// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A30: an input method that asks for surrounding text gets the
//! cursor's whole paragraph. The view uses the crate's probe input method
//! (selected for this view only, through its `im-module` property); click in
//! the long second line, use the arrow keys, type or scroll, and the readout
//! lists each surrounding-text request: what armed it, how many characters
//! the view handed over, and how long that took.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a30::{describe, probe_im_view};
use gtk_lush_axioms::fixtures::take_im_requests;
use std::cell::RefCell;
use std::rc::Rc;

fn build(ui: &support::SampleUi) {
    let Some(fixture) = probe_im_view() else {
        ui.set_readout(|| "the probe input method could not be registered".to_owned());
        return;
    };
    ui.set_fixture(&fixture.scrolled);
    let log = Rc::new(RefCell::new(Vec::new()));
    {
        let log = log.clone();
        ui.add_control("Clear the request log", move || {
            log.borrow_mut().clear();
            let _ = take_im_requests();
        });
    }
    ui.set_readout(move || {
        let mut entries = log.borrow_mut();
        entries.extend(take_im_requests());
        let start = entries.len().saturating_sub(8);
        format!(
            "last requests (trigger:chars:time): {}",
            describe(&entries[start..])
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(30), build)
}

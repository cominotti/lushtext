// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A46: the first toplevel this process realizes paid a one-time
//! cost — the sample's own window, realized before it showed. Realize more
//! windows and compare: a later realize does not pay it again.

mod support;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{fixture_window, timed_realize};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let explanation = gtk4::Label::builder()
        .label(
            "This window was the process's first toplevel, so its realize paid the one-time \
             cost. Each button press builds and realizes another window and shows how long \
             its realize took.",
        )
        .wrap(true)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    ui.set_fixture(&explanation);
    let timings = Rc::new(RefCell::new(Vec::<i64>::new()));
    let windows = Rc::new(RefCell::new(Vec::<libadwaita::Window>::new()));
    let recorder = Rc::clone(&timings);
    let keeper = Rc::clone(&windows);
    ui.add_control("Realize another toplevel", move || {
        let window = fixture_window("A46 later toplevel");
        recorder.borrow_mut().push(timed_realize(&window));
        window.present();
        keeper.borrow_mut().push(window);
    });
    ui.set_readout(move || {
        let timings = timings.borrow();
        if timings.is_empty() {
            "no later toplevel realized yet".to_owned()
        } else {
            format!(
                "later realizes (µs): {}",
                timings
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(46), build)
}

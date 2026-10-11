// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A29: the GtkSourceView context engine analyses a line in one
//! call and gives up after 2000 ms, disabling highlighting for the whole
//! buffer. It also highlights text hidden by an invisible tag. Run one
//! highlighting attempt at a chosen line length (the window freezes for the
//! call's duration, which is the point) and read what happened.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a29::{FIRST_LENGTH, highlight_attempt};

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some(
        "Each button highlights a fresh one-line minified-markup buffer in one call.",
    ));
    label.set_wrap(true);
    ui.set_fixture(&label);
    let readout = Rc::new(RefCell::new(String::from("press a button")));
    for multiple in [1usize, 2, 4, 8, 16, 32] {
        let readout = readout.clone();
        let chars = FIRST_LENGTH * multiple;
        ui.add_control(&format!("Highlight {chars} chars"), move || {
            let text = match highlight_attempt(chars) {
                Some(attempt) => format!(
                    "{} chars: {} µs, cut-off {}, later line tags [{}]\n{}",
                    attempt.chars,
                    attempt.micros,
                    attempt.cut_off,
                    attempt.later_line_tags.join(", "),
                    attempt.messages.join("\n")
                ),
                None => "GtkSourceView has no html language".to_owned(),
            };
            readout.replace(text);
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(29), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A107: one unwrapped line of 10,000 characters. "Visual step"
//! moves the cursor one position to the right the way the Right key does,
//! "Logical step" the cheap way; the readout shows where each landed and how
//! long it took. On this line the visual step takes about a second.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a107::{LONG_CHARS, START_OFFSET, line_view, step_right};

/// The last step: its button, where the cursor landed, and how long it took.
type LastStep = Option<(&'static str, i32, u128)>;

fn build(ui: &support::SampleUi) {
    let fixture = line_view(LONG_CHARS);
    ui.set_fixture(&fixture.scrolled);
    let last: Rc<Cell<LastStep>> = Rc::new(Cell::new(None));
    for (label, step) in [
        ("Visual step", gtk4::MovementStep::VisualPositions),
        ("Logical step", gtk4::MovementStep::LogicalPositions),
    ] {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control(label, move || {
            let (landed, micros) = step_right(&fixture, step, START_OFFSET, START_OFFSET);
            last.set(Some((label, landed, micros)));
        });
    }
    ui.set_readout(move || match last.get() {
        Some((label, landed, micros)) => format!("{label}: landed at {landed} in {micros} us"),
        None => "press a button".to_owned(),
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(107), build)
}

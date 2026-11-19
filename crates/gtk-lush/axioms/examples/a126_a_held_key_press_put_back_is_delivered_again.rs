// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A126: a list of rows under a capture-phase key controller.
//! "Hold keys" makes the controller keep each key press and stop it; click a
//! row and press Down a few times, and nothing moves. "Put back" puts the
//! held presses back on the display's queue: the readout shows each press
//! arriving again, in order, before a low-priority idle, and the focus and
//! selection move as the presses ask.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a126::{Mark, hold_fixture};

fn build(ui: &support::SampleUi) {
    let fixture = hold_fixture();
    ui.set_fixture(&fixture.column);
    {
        let fixture = fixture.clone();
        ui.add_control("Hold keys", move || {
            fixture.marks.borrow_mut().clear();
            fixture.holding.set(true);
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Put back", move || fixture.put_back());
    }
    ui.set_readout(move || {
        let marks: Vec<String> = fixture
            .marks
            .borrow()
            .iter()
            .map(|mark| match mark {
                Mark::Seen(Some(index)) => format!("press {index}"),
                Mark::Seen(None) => "other press".to_owned(),
                Mark::LowIdle => "low idle".to_owned(),
            })
            .collect();
        format!(
            "holding {}; held {}; after the put: {}; focus {:?}; selected {}",
            fixture.holding.get(),
            fixture.held.borrow().len(),
            marks.join(" → "),
            fixture.focused_row(),
            fixture.selection.selected()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(126), build)
}

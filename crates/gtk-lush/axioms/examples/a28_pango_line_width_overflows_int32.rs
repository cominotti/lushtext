// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A28: Pango measures a line in `int32` units of 1/1024 px. The
//! readout predicts, from the monospace advance, the line length at which a
//! line's width overflows, and shows the x positions just below and past it:
//! past the overflow they stop growing and wrap negative. The view shows a
//! line just past the overflow; scroll to its end.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::TextViewFixture;
use gtk_lush_axioms::fixtures::a28::{PAST_OVERFLOW_CHARS, predicted_overflow_length, x_positions};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::None);
    ui.set_fixture(&fixture.scrolled);
    let readout = Rc::new(RefCell::new(String::from("press a button")));
    {
        let (fixture, readout) = (fixture.clone(), readout.clone());
        ui.add_control("Measure around the overflow length", move || {
            let (advance, overflow) = predicted_overflow_length(&fixture.view);
            let indexes = [overflow - 1_000, overflow - 1, overflow, overflow + 1];
            let (positions, micros) =
                x_positions(&fixture.view, overflow + PAST_OVERFLOW_CHARS, &indexes);
            readout.replace(format!(
                "advance {advance} units/char   predicted overflow at {overflow} chars   \
                 layout {micros} µs\nx (units) at {indexes:?}: {positions:?}"
            ));
        });
    }
    {
        ui.add_control("Load a line just past the overflow", move || {
            let (_, overflow) = predicted_overflow_length(&fixture.view);
            fixture
                .buffer
                .set_text(&"a".repeat(overflow + PAST_OVERFLOW_CHARS));
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(28), build)
}

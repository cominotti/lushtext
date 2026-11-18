// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A122: a text view with an `extend-selection` handler that
//! answers FALSE. Double-click a word: the handler passes the emission on and
//! GtkTextView selects the word. "Answer TRUE" makes the handler claim the
//! emission instead, and a double click no longer selects the word; "Answer
//! FALSE" passes it on again.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a122::{connect_answer, document_view};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = document_view();
    ui.set_fixture(&fixture.scrolled);
    let answer = Rc::new(Cell::new(false));
    let (_handler, calls) = connect_answer(&fixture.view, Rc::clone(&answer), false);
    ui.add_control("Answer TRUE", {
        let answer = Rc::clone(&answer);
        move || answer.set(true)
    });
    ui.add_control("Answer FALSE", {
        let answer = Rc::clone(&answer);
        move || answer.set(false)
    });
    ui.set_readout(move || {
        let selection = fixture
            .buffer
            .selection_bounds()
            .map(|(start, end)| fixture.buffer.text(&start, &end, false).to_string())
            .unwrap_or_default();
        format!(
            "handler answers {}, {} emissions seen, selection {selection:?}",
            if answer.get() { "TRUE" } else { "FALSE" },
            calls.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(122), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A26: word boundaries are computed over the whole paragraph,
//! hidden text included. The view shows a head whose last word continues into
//! a hidden run; moving to the word's end lands inside the hidden text. The
//! cost button times one word movement in a long paragraph: first call,
//! repeat, and after an edit on another line.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a26::{
    HEAD, spanning_word_buffer, spanning_word_start, word_movement_costs,
};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let buffer = spanning_word_buffer();
    let view = gtk4::TextView::with_buffer(&buffer);
    view.set_monospace(true);
    ui.set_fixture(&view);
    let costs = Rc::new(Cell::new((0u128, 0u128, 0u128)));
    {
        let buffer = buffer.clone();
        ui.add_control("Move to the spanning word's end", move || {
            let mut iter = buffer.iter_at_offset(spanning_word_start());
            iter.forward_word_end();
            buffer.place_cursor(&iter);
        });
    }
    {
        let costs = costs.clone();
        ui.add_control("Time word movement in a long paragraph", move || {
            costs.set(word_movement_costs());
        });
    }
    ui.set_readout(move || {
        let cursor = buffer.iter_at_mark(&buffer.get_insert()).offset();
        let (first, repeat, after_edit) = costs.get();
        format!(
            "head ends at offset {}   cursor at {cursor}\nword end: first {first} µs   \
             repeat {repeat} µs   after an edit elsewhere {after_edit} µs",
            HEAD.len()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(26), build)
}

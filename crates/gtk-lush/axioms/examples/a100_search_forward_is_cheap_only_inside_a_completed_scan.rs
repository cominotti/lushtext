// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A100: a 262,144-line buffer whose last line holds the only
//! "needle". The readout shows the occurrence count (-1 while the scan runs)
//! and how long one `forward` from the start takes now: milliseconds before
//! the scan ends, microseconds after. "Edit" inserts a character and sends
//! the count back to -1 until the rescan ends.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a100::{forward_from_start_us, needle_search, sparse_buffer};

fn build(ui: &support::SampleUi) {
    let buffer = sparse_buffer(gtk_lush_axioms::fixtures::a100::SPARSE_LINES);
    let context = Rc::new(RefCell::new(needle_search(&buffer)));
    let view = sourceview5::View::with_buffer(&buffer);
    let scrolled = gtk4::ScrolledWindow::builder().child(&view).build();
    ui.set_fixture(&scrolled);
    {
        let (buffer, context) = (buffer.clone(), context.clone());
        ui.add_control("New search (unscanned)", move || {
            *context.borrow_mut() = needle_search(&buffer);
        });
    }
    {
        ui.add_control("Edit near the top", move || {
            if let Some(mut at) = buffer.iter_at_line(10) {
                buffer.insert(&mut at, "x");
            }
        });
    }
    ui.set_readout(move || {
        let context = context.borrow();
        let count = context.occurrences_count();
        let (line, us) = forward_from_start_us(&context);
        format!("occurrences-count: {count}\nforward from start: line {line:?} in {us} µs")
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(100), build)
}

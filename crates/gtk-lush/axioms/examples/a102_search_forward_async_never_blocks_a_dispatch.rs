// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A102: a 262,144-line buffer whose last line holds the only
//! "needle". Each button starts a fresh, unscanned search and walks it with
//! `forward_async`, from the start or from a hundred lines before the match;
//! the readout shows the line found, the wall time and the longest dispatch.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a100::{SPARSE_LINES, needle_search, sparse_buffer};
use gtk_lush_axioms::fixtures::a102::{AsyncWalk, walk_async};

fn build(ui: &support::SampleUi) {
    let buffer = sparse_buffer(SPARSE_LINES);
    let view = sourceview5::View::with_buffer(&buffer);
    let scrolled = gtk4::ScrolledWindow::builder().child(&view).build();
    ui.set_fixture(&scrolled);
    let last: Rc<Cell<AsyncWalk>> = Rc::new(Cell::new(AsyncWalk::default()));
    let last_line = i32::try_from(SPARSE_LINES - 1).unwrap_or(i32::MAX);
    {
        let (buffer, last) = (buffer.clone(), last.clone());
        ui.add_control("Walk from the start", move || {
            last.set(walk_async(&needle_search(&buffer), 0));
        });
    }
    {
        let last = last.clone();
        ui.add_control("Walk from near the match", move || {
            last.set(walk_async(&needle_search(&buffer), last_line - 100));
        });
    }
    ui.set_readout(move || {
        let walk = last.get();
        format!(
            "line {:?}\nwall time {} µs\nlongest dispatch {} µs",
            walk.line, walk.elapsed_us, walk.worst_dispatch_us
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(102), build)
}

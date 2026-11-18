// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A124: a directory row expanded over 256 entries with 400 rows
//! after it. "Remove 32 from an idle" removes the first 32 children from a
//! one-shot high-priority idle, outside any frame; the readout shows the row
//! widgets created and bound inside the splice, how many entering rows were
//! mapped when it returned, and the setups and binds the next frame ran (0).

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a124::{
    AFTER_ROWS, IdleRemoval, SMALL_CHILDREN, idle_removal, tree_fixture,
};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = tree_fixture(SMALL_CHILDREN, AFTER_ROWS);
    ui.set_fixture(&fixture.column);
    let last: Rc<RefCell<Option<IdleRemoval>>> = Rc::default();
    {
        let fixture = fixture.clone();
        let last = Rc::clone(&last);
        ui.add_control("Remove 32 from an idle", move || {
            last.replace(idle_removal(&fixture));
        });
    }
    ui.set_readout(move || match last.borrow().as_ref() {
        None => format!("children {}", fixture.children.n_items()),
        Some(removal) => format!(
            "children {}; in splice (setups, binds) {:?}; entering {} mapped {}; next frame {:?}",
            fixture.children.n_items(),
            removal.in_call,
            removal.entering,
            removal.entering_mapped,
            removal.next_frame_counts
        ),
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(124), build)
}

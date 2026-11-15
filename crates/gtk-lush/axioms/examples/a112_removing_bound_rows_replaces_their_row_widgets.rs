// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A112: a directory row expanded over 256 entries with 400 rows
//! after it. "Remove 16 bound" removes the first 16 children (their row
//! widgets are destroyed, and as many created for the rows moving into the
//! band); "Remove unbound after" removes the children after the band (no row
//! widget changes). The readout counts the row widgets created and destroyed.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a112::{AFTER_ROWS, SMALL_CHILDREN, tree_fixture};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = tree_fixture(SMALL_CHILDREN, AFTER_ROWS);
    ui.set_fixture(&fixture.column);
    {
        let fixture = fixture.clone();
        ui.add_control("Remove 16 bound", move || {
            let _ = fixture.remove_children(0, 16.min(fixture.children.n_items()));
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Remove unbound after", move || {
            if let Some(&high) = fixture.bound_children().last() {
                let len = fixture.children.n_items();
                if high + 1 < len {
                    let _ = fixture.remove_children(high + 1, len - high - 1);
                }
            }
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Reset counters", move || fixture.counters.reset());
    }
    ui.set_readout(move || {
        format!(
            "children {}, bound {}; created {}, destroyed {}",
            fixture.children.n_items(),
            fixture.bound().len(),
            fixture.counters.setups.get(),
            fixture.counters.teardowns.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(112), build)
}

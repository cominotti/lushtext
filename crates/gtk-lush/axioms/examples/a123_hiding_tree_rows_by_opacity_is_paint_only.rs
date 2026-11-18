// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A123: a directory row expanded over 256 entries with 400 rows
//! after it. "Hide the subtree" sets opacity 0, `can-target` false and the
//! accessible `hidden` state on the row widgets of the realized subtree rows;
//! "Clear" undoes it. The readout shows the band (first position, bound
//! rows, the list's allocation) and the row measures, setups and teardowns
//! since the last action: hiding and clearing move none of them.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a123::{hide_fixture, read_band, reset_counts};

fn build(ui: &support::SampleUi) {
    let fixture = hide_fixture();
    ui.set_fixture(&fixture.column);
    let hidden = Rc::new(Cell::new(0u32));
    {
        let fixture = fixture.clone();
        let hidden = Rc::clone(&hidden);
        ui.add_control("Hide the subtree", move || {
            reset_counts(&fixture);
            hidden.set(fixture.hide_subtree().0);
        });
    }
    {
        let fixture = fixture.clone();
        let hidden = Rc::clone(&hidden);
        ui.add_control("Clear", move || {
            reset_counts(&fixture);
            let _ = fixture.clear_subtree();
            hidden.set(0);
        });
    }
    ui.set_readout(move || {
        let band = read_band(&fixture);
        format!(
            "hidden {}; first {} bound {} allocation {:?}; measures {} setups {} teardowns {}",
            hidden.get(),
            band.first,
            band.bound,
            band.allocation,
            band.measures,
            band.setups,
            band.teardowns
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(123), build)
}

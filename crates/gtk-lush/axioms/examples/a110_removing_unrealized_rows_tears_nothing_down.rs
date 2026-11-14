// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A110: a 16,384-row list scrolled to its middle. "Hide" unmaps
//! it; "Trim outside" removes the rows GTK never realized after and before
//! the band; "Remove a unit" removes rows from the end. The readout counts
//! the row widgets created and destroyed.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a110::{BAND_MODEL_ROWS, MIDDLE, RowText, UNIT, row_list};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = row_list(BAND_MODEL_ROWS, RowText::Label);
    ui.set_fixture(&fixture.column);
    {
        let fixture = fixture.clone();
        ui.add_control("Scroll to the middle", move || {
            fixture
                .list
                .scroll_to(MIDDLE, gtk4::ListScrollFlags::NONE, None);
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Hide / show", move || {
            fixture.scroller.set_visible(!fixture.scroller.is_visible());
            fixture.counters.reset();
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Trim outside", move || {
            if let Some((low, high)) = fixture.bound_span() {
                fixture.remove_tail(high + 1);
                fixture.remove_head(low);
            }
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Remove a unit", move || {
            let len = fixture.model.n_items();
            fixture.remove_tail(len.saturating_sub(UNIT));
        });
    }
    ui.set_readout(move || {
        format!(
            "rows {}, realized {}, mapped {}; since hide: created {}, destroyed {}",
            fixture.model.n_items(),
            fixture.realized_rows(),
            fixture.list.is_mapped(),
            fixture.counters.setups.get(),
            fixture.counters.teardowns.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(110), build)
}

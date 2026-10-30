// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A63: a text view's drag-and-drop controllers. Drop a string on
//! it (as GTK's drop dispatch does), toggle `editable`, remove the drop
//! target: a non-editable view refuses the drop, and once removed there is no
//! drop target. Dragging text from another application onto the view shows
//! the same.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a63::{DOCUMENT, dnd_census, dnd_view, drop_text};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = dnd_view();
    ui.set_fixture(&fixture.scrolled);
    let last = Rc::new(Cell::new(None));
    {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control("Drop a string at the start", move || {
            last.set(
                dnd_census(&fixture.view)
                    .drop_targets
                    .first()
                    .map(|target| drop_text(target, 5.0, 5.0)),
            );
        });
    }
    {
        let view = fixture.view.clone();
        ui.add_control("Toggle editable", move || {
            view.set_editable(!view.is_editable());
        });
    }
    {
        let view = fixture.view.clone();
        ui.add_control("Remove the drop target", move || {
            for target in dnd_census(&view).drop_targets {
                view.remove_controller(&target);
            }
        });
    }
    {
        let buffer = fixture.buffer.clone();
        ui.add_control("Reset the text", move || buffer.set_text(DOCUMENT));
    }
    ui.set_readout(move || {
        let census = dnd_census(&fixture.view);
        format!(
            "editable {}   drop targets {}   drop-motion {}   drag sources {}\nlast drop answered {:?}",
            fixture.view.is_editable(),
            census.drop_targets.len(),
            census.drop_motion,
            census.drag_sources,
            last.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(63), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A24: text tagged `invisible` is left out of the layout. Load a
//! visible head followed by a long tail, hide the tail, show it again, or load
//! the head alone: the readout shows that a hidden tail adds no height and
//! does not move the head, and how long each tag change took.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a24::{head_geometry, load, tail_bounds};
use gtk_lush_axioms::fixtures::{TextViewFixture, invisible_tag, time_us};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    let tag = invisible_tag(&fixture.buffer);
    ui.set_fixture(&fixture.scrolled);
    let last = Rc::new(Cell::new(("nothing yet", 0u128)));
    {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control("Load the head alone", move || {
            let _: u128 = load(&fixture, None);
            last.set(("loaded the head alone", 0));
        });
    }
    {
        let (fixture, tag, last) = (fixture.clone(), tag.clone(), last.clone());
        ui.add_control("Load head + tail, hide the tail", move || {
            last.set(("hid the tail", load(&fixture, Some(&tag))));
        });
    }
    {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control("Show the tail", move || {
            let (start, end) = tail_bounds(&fixture);
            let ((), micros) = time_us(|| {
                fixture.buffer.remove_tag(&tag, &start, &end);
                let _ = head_geometry(&fixture);
            });
            last.set(("showed the tail", micros));
        });
    }
    ui.set_readout(move || {
        let geometry = head_geometry(&fixture);
        let (what, micros) = last.get();
        format!(
            "paragraph height {}   head end (x, y, w, h) {:?}\nlast: {what} ({micros} µs)",
            geometry.paragraph_height, geometry.head_end
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(24), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A23: a `GtkTextView` lays a paragraph out as one
//! `PangoLayout`. Load a single paragraph of 2k, 20k or 200k characters,
//! wrapped or not: the readout shows that the paragraph's display height is
//! its whole wrap count even though only its top is on screen, and how long
//! its first layout took.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::TextViewFixture;
use gtk_lush_axioms::fixtures::a23::{LENGTHS, first_layout_us, paragraph_geometry};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    ui.set_fixture(&fixture.scrolled);
    let last_layout = Rc::new(Cell::new((0usize, 0u128)));
    for chars in LENGTHS {
        let (fixture, last_layout) = (fixture.clone(), last_layout.clone());
        ui.add_control(&format!("Load {}k characters", chars / 1_000), move || {
            last_layout.set((chars, first_layout_us(&fixture, chars)));
        });
    }
    let toggler = fixture.clone();
    ui.add_control("Toggle wrapping", move || {
        let next = if toggler.view.wrap_mode() == gtk4::WrapMode::None {
            gtk4::WrapMode::WordChar
        } else {
            gtk4::WrapMode::None
        };
        toggler.view.set_wrap_mode(next);
    });
    ui.set_readout(move || {
        let geometry = paragraph_geometry(&fixture);
        let (chars, micros) = last_layout.get();
        format!(
            "wrap {:?}   viewport height {}   paragraph height {}   last char bottom {}   \
             line height {}\nlast load: {chars} chars, first layout {micros} µs",
            fixture.view.wrap_mode(),
            fixture.scrolled.height(),
            geometry.height,
            geometry.last_char_bottom,
            geometry.line_height,
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(23), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A75: a wrapping text view of many long paragraphs with a left
//! gutter. Widen or narrow the gutter and watch the last (offscreen)
//! paragraph's position move once the view has laid every paragraph out
//! again; the readout shows how long that revalidation took.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::TextViewFixture;
use gtk_lush_axioms::fixtures::a75::{
    NARROW_GUTTER, WIDE_GUTTER, last_paragraph_extent, set_gutter_width, settle_validated, text,
};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    set_gutter_width(&fixture.view, NARROW_GUTTER);
    fixture.buffer.set_text(&text());
    ui.set_fixture(&fixture.scrolled);
    let last = Rc::new(Cell::new(0u128));
    for (label, width) in [
        ("Narrow gutter", NARROW_GUTTER),
        ("Wide gutter", WIDE_GUTTER),
    ] {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control(label, move || {
            set_gutter_width(&fixture.view, width);
            let (_, took) = settle_validated(&fixture);
            last.set(took.as_millis());
        });
    }
    ui.set_readout(move || {
        let (y, height) = last_paragraph_extent(&fixture);
        format!(
            "last paragraph at y {y}, height {height}\nlast revalidation {} ms",
            last.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(75), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A79: a last line holding a short visible head and a hidden run
//! of 1, 2 or 4 MiB, with or without a delimiter after it. The readout shows
//! how long mapping a point below the text to an iterator takes: growing with
//! the hidden bytes on an undelimited last line, flat with a delimiter.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a79::{below_layout_lookup_us, install};
use gtk_lush_axioms::fixtures::{TextViewFixture, invisible_tag};

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::None);
    let tag = invisible_tag(&fixture.buffer);
    ui.set_fixture(&fixture.scrolled);
    let last = Rc::new(Cell::new(0u128));
    for (label, mib, delimited) in [
        ("4 MiB hidden, last line", 4, false),
        ("4 MiB hidden, delimited", 4, true),
    ] {
        let (fixture, tag, last) = (fixture.clone(), tag.clone(), last.clone());
        ui.add_control(label, move || {
            install(&fixture, &tag, mib, delimited);
            last.set(below_layout_lookup_us(&fixture));
        });
    }
    ui.set_readout(move || format!("lookup below the text: {} µs", last.get()));
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(79), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A65: insert a 1 or 4 MiB tail hidden after a short head with
//! one insert_with_tags: the paragraph stays as tall (wrapping) and as wide
//! (not wrapping) as the head alone. The readout shows the insert's and the
//! first validation's time.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a65::{head, insert_hidden_tail, paragraph_extent, tail};
use gtk_lush_axioms::fixtures::{TextViewFixture, invisible_tag};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    let tag = invisible_tag(&fixture.buffer);
    fixture.buffer.set_text(&head());
    ui.set_fixture(&fixture.scrolled);
    let last = Rc::new(Cell::new((0usize, 0u128, 0u128)));
    for mib in [1usize, 4] {
        let (fixture, tag, last) = (fixture.clone(), tag.clone(), last.clone());
        ui.add_control(&format!("Head + {mib} MiB hidden tail"), move || {
            fixture.buffer.set_text(&head());
            let (insert, validate) = insert_hidden_tail(&fixture, &tag, &tail(mib));
            last.set((mib, insert, validate));
        });
    }
    {
        let buffer = fixture.buffer.clone();
        ui.add_control("Head alone", move || buffer.set_text(&head()));
    }
    {
        let view = fixture.view.clone();
        ui.add_control("Toggle wrapping", move || {
            view.set_wrap_mode(if view.wrap_mode() == gtk4::WrapMode::None {
                gtk4::WrapMode::WordChar
            } else {
                gtk4::WrapMode::None
            });
        });
    }
    ui.set_readout(move || {
        let (height, width) = paragraph_extent(&fixture);
        let (mib, insert, validate) = last.get();
        format!(
            "characters {}   paragraph height {height}   layout width {width:.0}\n\
             last: {mib} MiB, insert {insert} µs, first validation {validate} µs",
            fixture.buffer.char_count()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(65), build)
}

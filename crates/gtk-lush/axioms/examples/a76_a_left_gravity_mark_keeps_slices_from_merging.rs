// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A76: append 64 tagged 64 KiB slices to a buffer, with and
//! without a left-gravity mark after each slice, and delete them again range
//! by range. The readout shows the per-append cost at both ends of the run:
//! flat with marks, growing without (the segment merge of A36).

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a76::{
    append_tagged_slices, delete_ranges_forwards, ends, slice_text,
};
use gtk_lush_axioms::fixtures::{TextViewFixture, invisible_tag};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    let tag = invisible_tag(&fixture.buffer);
    ui.set_fixture(&fixture.scrolled);
    let last = Rc::new(RefCell::new(String::from("no run yet")));
    for (label, separate) in [("Append with marks", true), ("Append without marks", false)] {
        let (buffer, tag, last) = (fixture.buffer.clone(), tag.clone(), last.clone());
        ui.add_control(label, move || {
            let (costs, marks) = append_tagged_slices(&buffer, &tag, &slice_text(), separate);
            let deletions = if marks.is_empty() {
                String::from("-")
            } else {
                format!("{:?}", ends(&delete_ranges_forwards(&buffer, &marks)))
            };
            *last.borrow_mut() = format!(
                "{label}: append first/last µs {:?}, delete first/last µs {deletions}",
                ends(&costs)
            );
        });
    }
    ui.set_readout(move || {
        format!(
            "characters {}\n{}",
            fixture.buffer.char_count(),
            last.borrow()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(76), build)
}

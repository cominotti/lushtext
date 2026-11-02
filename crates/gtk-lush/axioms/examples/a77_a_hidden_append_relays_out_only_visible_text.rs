// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A77: grow a 16 MiB hidden tail in 64 slices, with the 10,000
//! character head in place, with only its first character in place, or
//! inserted after the tail. The readout shows the view's re-layout and the
//! x = 0 lookup GtkSourceView's snapshot makes, at both ends of the run.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a76::ends;
use gtk_lush_axioms::fixtures::a77::{HEAD_CHARS, head, slice_text, sliced_install};
use gtk_lush_axioms::fixtures::{TextViewFixture, invisible_tag};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    let tag = invisible_tag(&fixture.buffer);
    ui.set_fixture(&fixture.scrolled);
    let last = Rc::new(RefCell::new(String::from("no run yet")));
    for (label, prefix) in [
        ("Head first", HEAD_CHARS),
        ("First character first", 1),
        ("Tail first", 0),
    ] {
        let (fixture, tag, last) = (fixture.clone(), tag.clone(), last.clone());
        ui.add_control(label, move || {
            let measured = sliced_install(&fixture, &tag, &head(), &slice_text(), prefix);
            *last.borrow_mut() = format!(
                "{label}: re-layout per append first/last µs {:?}, x=0 lookup first/last µs \
                 {:?}, rest of head {} µs",
                ends(&measured.relayout_us),
                ends(&measured.lookup_us),
                measured.head_us
            );
        });
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
        format!(
            "characters {}\n{}",
            fixture.buffer.char_count(),
            last.borrow()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(77), build)
}

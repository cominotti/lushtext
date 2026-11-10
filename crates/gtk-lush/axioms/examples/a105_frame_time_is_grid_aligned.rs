// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A105: a drawing area whose frames are stamped when they begin.
//! "Request a redraw" queues one and shows when it was requested, when its
//! frame began, the frame time and the presentation. On GTK 4.24 the frame
//! time can precede the request; before 4.24 it never does.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a105::{DrawnRequest, StampedArea};

fn build(ui: &support::SampleUi) {
    let stamped = Rc::new(StampedArea::new());
    ui.set_fixture(&stamped.area);
    let last: Rc<Cell<Option<DrawnRequest>>> = Rc::new(Cell::new(None));
    let hooked = Rc::new(Cell::new(false));
    {
        let last = last.clone();
        ui.add_control("Request a redraw", move || {
            if !hooked.replace(true) {
                stamped.hook();
            }
            last.set(stamped.request());
        });
    }
    ui.set_readout(move || match last.get() {
        Some(drawn) => format!(
            "frame began {} µs after the request\nframe time {} µs from the request\npresented {} µs after it",
            drawn.began_us - drawn.requested_us,
            drawn.frame_time_us - drawn.requested_us,
            drawn.presentation_us - drawn.requested_us,
        ),
        None => "press the button".to_owned(),
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(105), build)
}

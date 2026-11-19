// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A125: a ticking drawing area. "Arm after paint" arms a one-shot
//! high-priority idle in the next frame's after-paint; it works 4 ms and
//! re-arms once. The readout shows the last events: the frames' before-paint
//! and after-paint counters and each source with the counter it read, in the
//! order they ran.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a125::{Event, clock_fixture};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = clock_fixture();
    ui.set_fixture(&fixture.column);
    {
        let fixture = fixture.clone();
        ui.add_control("Arm after paint", move || {
            fixture.events.borrow_mut().clear();
            if let Some(clock) = fixture.area.frame_clock() {
                let recorder = fixture.clone();
                let handler = std::rc::Rc::new(std::cell::Cell::new(None));
                let slot = std::rc::Rc::clone(&handler);
                let id = clock.connect_after_paint(move |clock| {
                    recorder.events.borrow_mut().push((
                        Event::AfterPaint(clock.frame_counter()),
                        std::time::Instant::now(),
                    ));
                    recorder.arm(1, 0, std::time::Duration::from_millis(4), true);
                    if let Some(id) = slot.take() {
                        clock.disconnect(id);
                    }
                });
                handler.set(Some(id));
            }
        });
    }
    ui.set_readout(move || {
        let events = fixture.events.borrow();
        let shown: Vec<String> = events
            .iter()
            .take(6)
            .map(|(event, _)| format!("{event:?}"))
            .collect();
        shown.join(" → ")
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(125), build)
}

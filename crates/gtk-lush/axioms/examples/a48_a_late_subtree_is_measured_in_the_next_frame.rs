// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A48: in a mapped window, making a counting widget visible, or
//! appending a new one, measures nothing at the call; the readout shows the
//! measure arriving in a later frame, with the frame counter at the call.

mod support;

use std::cell::Cell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::CountingWidget;
use gtk_lush_axioms::fixtures::a47::COUNTING_WIDTH;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    let late = CountingWidget::new(COUNTING_WIDTH);
    late.set_visible(false);
    content.append(&late);
    ui.set_fixture(&content);
    let at_call = Rc::new(Cell::new((0u32, -1i64)));
    let record = Rc::clone(&at_call);
    let target = late.clone();
    ui.add_control("Make it visible", move || {
        target.set_visible(true);
        let frame = target
            .frame_clock()
            .map_or(-1, |clock| clock.frame_counter());
        record.set((target.measures(), frame));
    });
    let parent = content;
    ui.add_control("Append another", move || {
        parent.append(&CountingWidget::new(COUNTING_WIDTH));
    });
    ui.set_readout(move || {
        let (measures, frame) = at_call.get();
        let now = late.frame_clock().map_or(-1, |clock| clock.frame_counter());
        format!(
            "at the call: measures {measures} (frame {frame})   now: measures {} (frame {now})",
            late.measures()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(48), build)
}

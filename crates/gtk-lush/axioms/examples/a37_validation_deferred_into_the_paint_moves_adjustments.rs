// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A37: typing, widening a tag, or widening the view's left margin
//! (with a redraw of the view: the margin setter queues none) inside a frame
//! (a tick callback, as GTK handles a key press inside the frame) makes
//! `GtkTextView` flush its validation from its snapshot and change the scroll
//! adjustments there; the readout counts those changes, and the terminal may
//! show `Trying to snapshot GtkGizmo … without a current allocation`. "Queue
//! allocation before paint" installs the workaround, after which the same
//! changes count nothing inside the paint. Typing with the keyboard shows the
//! same.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a37::{FRAMES, FrameChange, LongLineView, PaintPhaseChanges};

fn build(ui: &support::SampleUi) {
    let fixture = LongLineView::new();
    ui.set_fixture(&fixture.container);
    let counter: Rc<RefCell<Option<PaintPhaseChanges>>> = Rc::new(RefCell::new(None));
    for (label, change) in [
        ("Type in frames", FrameChange::Type),
        ("Widen a tag in frames", FrameChange::Tag),
        ("Widen the left margin in frames", FrameChange::Margin),
    ] {
        let fixture = fixture.clone();
        let counter = Rc::clone(&counter);
        ui.add_control(label, move || {
            if counter.borrow().is_none() {
                counter.replace(PaintPhaseChanges::watch(&fixture));
            }
            let _done = fixture.change_in_frames(change, FRAMES);
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Queue allocation before paint", move || {
            let _installed = fixture.queue_allocation_before_paint();
        });
    }
    ui.set_readout(move || {
        let (upper, page) = fixture.horizontal_extent();
        let in_paint = counter.borrow().as_ref().map_or_else(
            || "not counting yet".to_owned(),
            |changes| changes.count().to_string(),
        );
        format!("upper {upper:.0}   page {page:.0}   adjustment changes inside paint: {in_paint}")
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(37), build)
}

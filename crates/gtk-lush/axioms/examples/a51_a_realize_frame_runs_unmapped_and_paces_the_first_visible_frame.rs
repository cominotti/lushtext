// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A51: compose a window by realizing it in one idle and
//! presenting it in a later one, or both in one idle, and read every
//! `after-paint` of its frame clock: whether the window was mapped, the frame
//! counter, the frame time and the presentation time. The split order shows an
//! unmapped, never-presented frame and a first visible frame about one
//! `FRAME_INTERVAL` later; the merged order shows none.

mod support;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a51::{Composed, FRAME_INTERVAL_US, Order};

fn build(ui: &support::SampleUi) {
    let explanation = gtk4::Label::builder()
        .label(format!(
            "Each button composes a new window. 'Split' realizes it in one idle and presents \
             it in a later one; 'Merged' does both in one idle. The readout lists the last \
             window's frames; an unmapped frame paces the next one {FRAME_INTERVAL_US} µs later."
        ))
        .wrap(true)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    ui.set_fixture(&explanation);
    let last = Rc::new(RefCell::new(None::<Composed>));
    for (label, order) in [
        ("Split: realize, then present", Order::Split),
        ("Merged: realize and present", Order::Merged),
    ] {
        let slot = Rc::clone(&last);
        ui.add_control(label, move || {
            slot.replace(Some(Composed::queue(order, "A51 sample window")));
        });
    }
    ui.set_readout(move || {
        let last = last.borrow();
        let Some(composed) = last.as_ref() else {
            return "no window composed yet".to_owned();
        };
        let paints = composed.paints();
        let first_time = paints.first().map_or(0, |paint| paint.frame_time_us);
        paints
            .iter()
            .take(4)
            .map(|paint| {
                format!(
                    "#{} {} +{} µs presented {}",
                    paint.counter,
                    if paint.mapped { "mapped" } else { "UNMAPPED" },
                    paint.frame_time_us - first_time,
                    composed.presentation_us(paint.counter) != 0
                )
            })
            .collect::<Vec<_>>()
            .join("   ")
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(51), build)
}

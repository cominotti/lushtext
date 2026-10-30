// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A59: a protected-configuration text view (not editable, no
//! cursor). Click, middle-click, right-click and drag in it with and without
//! the capture-phase claim: without it the view's own gestures move the
//! insert mark, select and open the context menu; with it nothing changes.
//! `--check` drives the presses through the headless compositor.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a59::{
    CaptureClaim, mark_state, observe_stock_gestures, popover_visible, protected_view,
};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = protected_view();
    let log = observe_stock_gestures(&fixture.view);
    ui.set_fixture(&fixture.scrolled);
    let claim: Rc<RefCell<Option<CaptureClaim>>> = Rc::new(RefCell::new(None));
    {
        let (fixture, claim) = (fixture.clone(), claim.clone());
        ui.add_control("Toggle the capture claim", move || {
            let current = claim.borrow_mut().take();
            match current {
                Some(installed) => installed.remove(&fixture.view),
                None => *claim.borrow_mut() = Some(CaptureClaim::install(&fixture.view)),
            }
        });
    }
    {
        let (fixture, log) = (fixture.clone(), log.clone());
        ui.add_control("Reset marks and counts", move || {
            fixture.buffer.place_cursor(&fixture.buffer.start_iter());
            if let Some(log) = &log {
                log.clear();
            }
        });
    }
    ui.set_readout(move || {
        let state = mark_state(&fixture);
        let (pressed, drags) = log.as_ref().map_or((Vec::new(), 0), |log| {
            (log.pressed.borrow().clone(), log.drag_updates.get())
        });
        format!(
            "claim installed {}   insert {}   selection bound {}   menu open {}\n\
             view's click gesture pressed (buttons) {pressed:?}   drag updates {drags}",
            claim.borrow().is_some(),
            state.insert,
            state.selection_bound,
            popover_visible(&fixture.view)
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(59), build)
}

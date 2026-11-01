// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A64: a non-editable source view. Press Ctrl+A, F7,
//! Ctrl+Shift+A (on the 42) or Shift+F10 in it, or emit change-case and
//! join-lines, with and without the guards: without them the buffer, the
//! marks or cursor-visible change although the view is not editable.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a64::{
    NUMBER_OFFSET, menu_key_claim, preempt_signals, reset, source_view, view_state,
};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let (scrolled, view, _buffer) = source_view();
    view.set_editable(false);
    ui.set_fixture(&scrolled);
    let guards = Rc::new(RefCell::new(None));
    {
        let (view, guards) = (view.clone(), guards.clone());
        ui.add_control("Toggle the guards", move || {
            let current = guards.borrow_mut().take();
            if let Some((handlers, claim)) = current {
                for handler in handlers {
                    view.disconnect(handler);
                }
                view.remove_controller(&claim);
            } else {
                let claim = menu_key_claim();
                view.add_controller(claim.clone());
                *guards.borrow_mut() = Some((preempt_signals(&view), claim));
            }
        });
    }
    {
        let view = view.clone();
        ui.add_control("Reset (cursor in 42)", move || {
            reset(&view, NUMBER_OFFSET, NUMBER_OFFSET);
            view.grab_focus();
        });
    }
    {
        let view = view.clone();
        ui.add_control("Emit change-case (upper) on 'value'", move || {
            reset(&view, 0, 5);
            view.emit_by_name::<()>("change-case", &[&sourceview5::ChangeCaseType::Upper]);
        });
    }
    {
        let view = view.clone();
        ui.add_control("Emit join-lines", move || {
            reset(&view, 0, 40);
            view.emit_by_name::<()>("join-lines", &[]);
        });
    }
    ui.set_readout(move || {
        let state = view_state(&view);
        format!(
            "guards {}   editable {}   insert {}   bound {}   cursor-visible {}\n{}",
            guards.borrow().is_some(),
            view.is_editable(),
            state.insert,
            state.bound,
            state.cursor_visible,
            state.text.trim_end()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(64), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A60: a text view using the probe input method beside a
//! button. Toggle `editable`, move focus between the button and the view and
//! type: the readout shows whether the view's key controller carries an
//! input-method context and what the input method was asked for.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a60::{focus_pair, key_controller_has_im};
use gtk_lush_axioms::fixtures::take_im_requests;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let Some((content, button, fixture)) = focus_pair() else {
        ui.set_readout(|| "the probe input method could not be registered".to_owned());
        return;
    };
    ui.set_fixture(&content);
    {
        let view = fixture.view.clone();
        ui.add_control("Toggle editable", move || {
            view.set_editable(!view.is_editable());
        });
    }
    ui.add_control("Focus the button", move || {
        button.grab_focus();
    });
    {
        let view = fixture.view.clone();
        ui.add_control("Focus the view", move || {
            view.grab_focus();
        });
    }
    let seen = Rc::new(RefCell::new(Vec::new()));
    ui.set_readout(move || {
        seen.borrow_mut().extend(
            take_im_requests()
                .into_iter()
                .map(|request| request.trigger),
        );
        let mut seen = seen.borrow_mut();
        let excess = seen.len().saturating_sub(12);
        seen.drain(..excess);
        format!(
            "editable {}   key controller has an input method {:?}\nlast input-method requests: {}",
            fixture.view.is_editable(),
            key_controller_has_im(&fixture.view),
            seen.join(", ")
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(60), build)
}

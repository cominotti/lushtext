// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A61: a cursor-less view with caret browsing and the
//! move-cursor intercept switchable. With caret browsing on, Page Down moves
//! the insert mark; with the intercept installed it only scrolls.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a34::{caret_and_scroll, cursorless_view, page_down};
use gtk_lush_axioms::fixtures::a61::{connect_intercept, marks};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = cursorless_view();
    ui.set_fixture(&fixture.scrolled);
    let intercept = Rc::new(RefCell::new(None));
    {
        let (fixture, intercept) = (fixture.clone(), intercept.clone());
        ui.add_control("Toggle the intercept", move || {
            let current = intercept.borrow_mut().take();
            match current {
                Some((handler, _)) => fixture.view.disconnect(handler),
                None => *intercept.borrow_mut() = Some(connect_intercept(&fixture)),
            }
        });
    }
    ui.add_control("Toggle caret browsing", || {
        if let Some(settings) = gtk4::Settings::default() {
            settings.set_gtk_keynav_use_caret(!settings.is_gtk_keynav_use_caret());
        }
    });
    {
        let fixture = fixture.clone();
        ui.add_control("Page Down (move-cursor)", move || page_down(&fixture));
    }
    ui.set_readout(move || {
        let (_, scroll) = caret_and_scroll(&fixture);
        let (insert, bound) = marks(&fixture);
        let browsing =
            gtk4::Settings::default().is_some_and(|settings| settings.is_gtk_keynav_use_caret());
        format!(
            "intercept {}   caret browsing {browsing}   insert {insert}   selection bound {bound}   \
             scroll {scroll:.0}",
            intercept.borrow().is_some()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(61), build)
}

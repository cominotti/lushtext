// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A133: a tab strip that starts with two tabs. Add tabs until it
//! overflows and watch `is-overflowing`; attach the three-button end widget
//! and see the unpinned tabs keep overflowing and the bar keep its height.
//! "Re-announce selection" notifies the unchanged `selected-page`: scroll the
//! strip away from the selected tab first and watch it scroll back.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a133::{end_group, fixture, overflows, record_notifies};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let strip = Rc::new(fixture());
    ui.set_fixture(&strip.root);
    let notifies = record_notifies(&strip);
    ui.add_control("Add tab", {
        let strip = Rc::clone(&strip);
        move || {
            let _ = strip.append_tab();
        }
    });
    let group: Rc<RefCell<Option<gtk4::Box>>> = Rc::new(RefCell::new(None));
    ui.add_control("Toggle end widget", {
        let strip = Rc::clone(&strip);
        let group = Rc::clone(&group);
        move || {
            let attached = group.borrow_mut().take();
            if attached.is_some() {
                strip.bar.set_end_action_widget(None::<&gtk4::Widget>);
            } else {
                let widget = end_group();
                strip.bar.set_end_action_widget(Some(&widget));
                group.replace(Some(widget));
            }
        }
    });
    ui.add_control("Re-announce selection", {
        let strip = Rc::clone(&strip);
        move || strip.tab_view.notify("selected-page")
    });
    ui.set_readout(move || {
        let unpinned = strip
            .unpinned()
            .is_some_and(|scroller| overflows(&scroller.hadjustment()));
        format!(
            "{} tabs, is-overflowing {}, unpinned overflows {unpinned}, bar height {}, {} notifies",
            strip.tab_view.n_pages(),
            strip.bar.is_overflowing(),
            strip.bar.height(),
            notifies.borrow().len()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(133), build)
}

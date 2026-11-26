// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A132: a tab strip of many tabs. "Forward one page" writes the
//! unpinned scroller's adjustment from outside the bar, as LushText's scroll
//! button does; the tabs move by the page increment and stay there. "Select
//! last tab" starts the strip's own scroll animation; "Right-to-left" flips
//! the bar's direction.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a132::{describe_adjustment, fixture, write_scroll_value};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let strip = std::rc::Rc::new(fixture());
    ui.set_fixture(&strip.root);
    ui.add_control("Forward one page", {
        let strip = std::rc::Rc::clone(&strip);
        move || {
            if let Some(scroller) = strip.unpinned() {
                let adjustment = scroller.hadjustment();
                let target = (adjustment.value() + adjustment.page_increment())
                    .min(adjustment.upper() - adjustment.page_size());
                write_scroll_value(&strip, target);
            }
        }
    });
    ui.add_control("Select last tab", {
        let strip = std::rc::Rc::clone(&strip);
        move || {
            if let Some(page) = strip.pages().last() {
                strip.tab_view.set_selected_page(page);
            }
        }
    });
    ui.add_control("Right-to-left", {
        move || {
            let next = if gtk4::Widget::default_direction() == gtk4::TextDirection::Rtl {
                gtk4::TextDirection::Ltr
            } else {
                gtk4::TextDirection::Rtl
            };
            gtk4::Widget::set_default_direction(next);
        }
    });
    ui.set_readout(move || {
        strip.unpinned().map_or_else(
            || "no unpinned scroller found".to_owned(),
            |scroller| describe_adjustment(&scroller.hadjustment()),
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(132), build)
}

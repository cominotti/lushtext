// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A115: an expanded directory row. "Collapsing class" adds the
//! class the application rule targets, and its arrow then draws collapsed
//! while the row stays expanded; "Direction" switches LTR and RTL.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a115::{COLLAPSING_CLASS, COLLAPSING_CSS, tree_fixture};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    if let Some(display) = gtk4::gdk::Display::default() {
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(COLLAPSING_CSS);
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let fixture = tree_fixture(4, 0);
    ui.set_fixture(&fixture.column);
    {
        let list = fixture.list.clone();
        ui.add_control("Collapsing class", move || {
            if let Some(expander) = list
                .first_child()
                .and_then(|row| row.first_child())
                .and_downcast::<gtk4::TreeExpander>()
            {
                if expander.has_css_class(COLLAPSING_CLASS) {
                    expander.remove_css_class(COLLAPSING_CLASS);
                } else {
                    expander.add_css_class(COLLAPSING_CLASS);
                }
            }
        });
    }
    {
        let list = fixture.list.clone();
        ui.add_control("Direction", move || {
            let next = if list.direction() == gtk4::TextDirection::Rtl {
                gtk4::TextDirection::Ltr
            } else {
                gtk4::TextDirection::Rtl
            };
            list.set_direction(next);
        });
    }
    let tree = fixture.tree;
    ui.set_readout(move || {
        let expanded = tree
            .item(0)
            .and_downcast::<gtk4::TreeListRow>()
            .is_some_and(|row| row.is_expanded());
        format!("row expanded {expanded}")
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(115), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A113: a text view with a 6 px top margin. "Scroll to line one"
//! asks GTK for the aligned scroll to the first line (`scroll_to_iter`,
//! `use_align`, `yalign` 0): the value comes to rest at the top margin, and
//! the margin scrolls out of sight. "Rest at lower" sets the value to
//! `lower`, which shows it again.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a113::{document_view, scroll_to_first_line};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = document_view();
    ui.set_fixture(&fixture.scrolled);
    let adjustment = fixture.scrolled.vadjustment();
    ui.add_control("Scroll to line one", {
        let fixture = fixture.clone();
        move || scroll_to_first_line(&fixture, false)
    });
    ui.add_control("Rest at lower", {
        let adjustment = adjustment.clone();
        move || adjustment.set_value(adjustment.lower())
    });
    ui.set_readout(move || {
        format!(
            "value {:.1}, lower {:.1}, top margin {}",
            adjustment.value(),
            adjustment.lower(),
            fixture.view.top_margin()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(113), build)
}

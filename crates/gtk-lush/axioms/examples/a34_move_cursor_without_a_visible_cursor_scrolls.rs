// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A34: with the cursor invisible, Page Down (`move-cursor`)
//! scrolls the view and leaves the insert mark alone; with caret browsing on
//! (`gtk-keynav-use-caret`, F7 in many apps) it moves the insert mark.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a34::{caret_and_scroll, cursorless_view, page_down};

fn build(ui: &support::SampleUi) {
    let fixture = cursorless_view();
    ui.set_fixture(&fixture.scrolled);
    {
        let fixture = fixture.clone();
        ui.add_control("Page Down (move-cursor)", move || page_down(&fixture));
    }
    ui.add_control("Toggle caret browsing", || {
        if let Some(settings) = gtk4::Settings::default() {
            settings.set_gtk_keynav_use_caret(!settings.is_gtk_keynav_use_caret());
        }
    });
    ui.set_readout(move || {
        let (insert, value) = caret_and_scroll(&fixture);
        let browsing =
            gtk4::Settings::default().is_some_and(|settings| settings.is_gtk_keynav_use_caret());
        format!("insert offset {insert}   scroll value {value:.0}   caret browsing {browsing}")
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(34), build)
}

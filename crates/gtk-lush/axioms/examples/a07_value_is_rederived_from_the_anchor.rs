// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A7: a `GtkListView` re-derives its value from its scroll anchor
//! when its page changes. Move the value (as a host does, or with
//! `scroll_to`), then change the page: the value moves although nobody wrote
//! it, on a straight line in the page height.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{HostedList, ROWS};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let adjustment = hosted.adjustment.clone();
    ui.add_control("Host value → row 40", move || {
        let stride = adjustment.upper() / f64::from(ROWS);
        adjustment.set_value(40.0 * stride);
    });
    let list = hosted.list.clone();
    ui.add_control("scroll_to row 200", move || {
        list.scroll_to(200, gtk4::ListScrollFlags::NONE, None);
    });
    for (label, page) in [("Page 300", 300), ("Page 200", 200), ("Page 150", 150)] {
        let host = hosted.host.clone();
        ui.add_control(label, move || {
            host.set_child_height(page);
        });
    }
    let adjustment = hosted.adjustment;
    ui.set_readout(move || {
        format!(
            "value {}   page {}   upper {}",
            adjustment.value(),
            adjustment.page_size(),
            adjustment.upper()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(7), build)
}

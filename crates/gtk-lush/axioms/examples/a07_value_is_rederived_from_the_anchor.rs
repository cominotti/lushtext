// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A7: a `GtkListView` re-derives its value from its scroll anchor
//! when its page changes. Move the value (as a host does, or with
//! `scroll_to`), then change the page: the value moves although nobody wrote
//! it, on a straight line in the page height.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a07::{BOTTOM_ANCHORED_ROW, HOST_VALUE_ROW, PAGE_SWEEP};
use gtk_lush_axioms::fixtures::{HostedList, LIST_HEIGHT, ROWS, row_stride};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let adjustment = hosted.adjustment.clone();
    ui.add_control(&format!("Host value → row {HOST_VALUE_ROW}"), move || {
        adjustment.set_value(f64::from(HOST_VALUE_ROW) * row_stride(&adjustment, ROWS));
    });
    let list = hosted.list.clone();
    ui.add_control(&format!("scroll_to row {BOTTOM_ANCHORED_ROW}"), move || {
        list.scroll_to(BOTTOM_ANCHORED_ROW, gtk4::ListScrollFlags::NONE, None);
    });
    for page in std::iter::once(LIST_HEIGHT).chain(PAGE_SWEEP) {
        let host = hosted.host.clone();
        ui.add_control(&format!("Page {page}"), move || {
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

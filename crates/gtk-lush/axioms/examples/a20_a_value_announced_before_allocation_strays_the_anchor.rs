// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A20: after a host `set_value`, a `GtkListView` keeps its scroll
//! anchor far from the view, so shrinking the page moves the value steeply.
//! Re-announcing the value once the list has been allocated there re-anchors
//! it at the view's edge, and the same shrink moves the value by no more than
//! the page change. Reset, jump, optionally re-announce, then shrink.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a20::{SHRUNK_PAGE, TARGET_ROWS, reannounce_value};
use gtk_lush_axioms::fixtures::{HostedList, LIST_HEIGHT, ROWS, row_stride};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let host = hosted.host.clone();
    let adjustment = hosted.adjustment.clone();
    ui.add_control(&format!("Reset (value 0, page {LIST_HEIGHT})"), move || {
        host.set_child_height(LIST_HEIGHT);
        adjustment.set_value(0.0);
    });
    for row in TARGET_ROWS {
        let adjustment = hosted.adjustment.clone();
        ui.add_control(&format!("Host value → row {row}"), move || {
            adjustment.set_value(f64::from(row) * row_stride(&adjustment, ROWS));
        });
    }
    let adjustment = hosted.adjustment.clone();
    ui.add_control("Re-announce the value", move || {
        reannounce_value(&adjustment);
    });
    let host = hosted.host.clone();
    ui.add_control(&format!("Shrink to page {SHRUNK_PAGE}"), move || {
        host.set_child_height(SHRUNK_PAGE);
    });
    let adjustment = hosted.adjustment;
    ui.set_readout(move || {
        format!(
            "value {}   page {}",
            adjustment.value(),
            adjustment.page_size()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(20), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A1: a `GtkListView` realizes about 200 rows (205 measured) per
//! visible range, however tall its viewport. Give it a 10 000 px viewport and
//! the realized rows stop near row 204, leaving the rest of the viewport
//! blank. The "Jump" controls also show the lag the probe records: the first
//! host jump past the realized rows maps nothing until a second move.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{HostedList, LIST_HEIGHT, realized_rows, row_index_range};
use gtk4::prelude::*;

const TALL_VIEWPORT: i32 = 10_000;
const FAR_ROW: f64 = 300.0;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&hosted.host)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build();
    ui.set_fixture(&scroller);
    let host = hosted.host.clone();
    ui.add_control("Tall viewport (10 000 px)", move || {
        host.set_child_height(TALL_VIEWPORT);
        host.set_size_request(-1, TALL_VIEWPORT);
    });
    let host = hosted.host.clone();
    ui.add_control("Resting viewport (300 px)", move || {
        host.set_child_height(LIST_HEIGHT);
        host.set_size_request(-1, -1);
    });
    let adjustment = hosted.adjustment.clone();
    ui.add_control("Jump to row 300", move || {
        let stride = adjustment.upper() / f64::from(gtk_lush_axioms::fixtures::ROWS);
        adjustment.set_value(FAR_ROW * stride);
    });
    let adjustment = hosted.adjustment.clone();
    ui.add_control("Nudge 1 px", move || {
        adjustment.set_value(adjustment.value() + 1.0);
    });
    let list = hosted.list.clone();
    let adjustment = hosted.adjustment;
    ui.set_readout(move || {
        let rows = realized_rows(&list);
        let mapped = rows.iter().filter(|row| row.is_mapped()).count();
        format!(
            "realized {} rows {:?}   mapped {mapped}   value {}   page {}",
            rows.len(),
            row_index_range(&rows),
            adjustment.value(),
            adjustment.page_size()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(1), build)
}

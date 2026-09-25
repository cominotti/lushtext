// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A10: `mapped` does not mean drawn on screen. A 300 px list of
//! 34 px rows maps a row wholly below its allocation at rest, and a row
//! wholly outside each edge when its value falls on a row boundary; at a
//! value between boundaries every mapped row is in view. The readout lists
//! the mapped rows whose bounds miss the list's allocation, as `row@top`.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a10::{
    ALIGNED_ROW, FOCUS_ROW, SCROLL_AWAY, SCROLL_TO_ROW, UNALIGNED_VALUE,
};
use gtk_lush_axioms::fixtures::{
    HostedList, ROWS, RowPlacement, realized_rows, row_index, row_placement, row_stride,
};
use gtk4::prelude::*;

/// Mapped rows, those intersecting the list's allocation, and the rest.
fn describe(list: &gtk4::ListView) -> String {
    let mut mapped = 0;
    let mut outside = Vec::new();
    for row in realized_rows(list).iter().filter(|row| row.is_mapped()) {
        mapped += 1;
        if let Some(RowPlacement::Above(top) | RowPlacement::Below(top)) = row_placement(row, list)
        {
            let index = row_index(row).map_or_else(|| "?".to_owned(), |i| i.to_string());
            outside.push(format!("{index}@{top}"));
        }
    }
    format!(
        "mapped {mapped}   in view {}   mapped outside [{}]",
        mapped - outside.len(),
        outside.join(" ")
    )
}

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let adjustment = hosted.adjustment.clone();
    ui.add_control("Rest (value 0)", move || adjustment.set_value(0.0));
    let adjustment = hosted.adjustment.clone();
    ui.add_control(&format!("Value {UNALIGNED_VALUE}"), move || {
        adjustment.set_value(UNALIGNED_VALUE);
    });
    let adjustment = hosted.adjustment.clone();
    ui.add_control(&format!("Top of row {ALIGNED_ROW}"), move || {
        adjustment.set_value(f64::from(ALIGNED_ROW) * row_stride(&adjustment, ROWS));
    });
    let list = hosted.list.clone();
    ui.add_control(&format!("scroll_to row {SCROLL_TO_ROW}"), move || {
        list.scroll_to(SCROLL_TO_ROW, gtk4::ListScrollFlags::NONE, None);
    });
    let list = hosted.list.clone();
    ui.add_control(&format!("Focus row {FOCUS_ROW}"), move || {
        list.scroll_to(FOCUS_ROW, gtk4::ListScrollFlags::FOCUS, None);
    });
    let adjustment = hosted.adjustment.clone();
    ui.add_control(&format!("Scroll {SCROLL_AWAY} px away"), move || {
        adjustment.set_value(adjustment.value() + SCROLL_AWAY);
    });
    let list = hosted.list;
    let adjustment = hosted.adjustment;
    ui.set_readout(move || format!("value {}   {}", adjustment.value(), describe(&list)));
}

fn main() -> ExitCode {
    support::run(AxiomId::new(10), build)
}

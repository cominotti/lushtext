// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A9: a `GtkListBase` drops a pending `scroll_to` on any
//! `value-changed` of its adjustment. `scroll_to` row 200 alone lands on the
//! row; nudging the adjustment 10 px before the request applies leaves the
//! list at the nudge.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::HostedList;
use gtk_lush_axioms::fixtures::a09::{NUDGE, TARGET_ROW};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let list = hosted.list.clone();
    ui.add_control(&format!("scroll_to row {TARGET_ROW}"), move || {
        list.scroll_to(TARGET_ROW, gtk4::ListScrollFlags::NONE, None);
    });
    let list = hosted.list.clone();
    let adjustment = hosted.adjustment.clone();
    ui.add_control(
        &format!("scroll_to row {TARGET_ROW}, nudged {NUDGE} px before it applies"),
        move || {
            list.scroll_to(TARGET_ROW, gtk4::ListScrollFlags::NONE, None);
            adjustment.set_value(adjustment.value() + NUDGE);
        },
    );
    let adjustment = hosted.adjustment.clone();
    ui.add_control("Back to the top", move || {
        adjustment.set_value(0.0);
    });
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
    support::run(AxiomId::new(9), build)
}

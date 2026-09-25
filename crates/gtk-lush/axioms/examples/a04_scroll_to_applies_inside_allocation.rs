// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A4: a `GtkListView` applies `scroll_to` and focus scrolling
//! inside its own allocation. The readout counts `value-changed` emissions
//! and how many fired while the host was allocating the list: always all.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a04::{FAR_ROW, FOCUS_ROW};
use gtk_lush_axioms::fixtures::{EmissionSites, HostedList};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let sites = EmissionSites::watch(&hosted.adjustment, &hosted.host);
    let list = hosted.list.clone();
    ui.add_control(&format!("scroll_to row {FAR_ROW}"), move || {
        list.scroll_to(FAR_ROW, gtk4::ListScrollFlags::NONE, None);
    });
    let list = hosted.list.clone();
    ui.add_control(&format!("Focus row {FOCUS_ROW}"), move || {
        list.scroll_to(FOCUS_ROW, gtk4::ListScrollFlags::FOCUS, None);
    });
    let adjustment = hosted.adjustment;
    ui.set_readout(move || {
        format!(
            "value {}   value-changed ×{}, inside the list's allocation ×{}",
            adjustment.value(),
            sites.total(),
            sites.inside_allocation()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(4), build)
}

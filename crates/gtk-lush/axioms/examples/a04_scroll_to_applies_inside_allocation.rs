// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A4: a `GtkListView` applies `scroll_to` and focus scrolling
//! inside its own allocation. The readout counts `value-changed` emissions
//! and how many fired while the host was allocating the list: always all.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{EmissionSites, HostedList};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let sites = EmissionSites::watch(&hosted.adjustment, &hosted.host);
    let list = hosted.list.clone();
    ui.add_control("scroll_to row 200", move || {
        list.scroll_to(200, gtk4::ListScrollFlags::NONE, None);
    });
    let list = hosted.list.clone();
    ui.add_control("Focus row 20", move || {
        list.scroll_to(20, gtk4::ListScrollFlags::FOCUS, None);
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

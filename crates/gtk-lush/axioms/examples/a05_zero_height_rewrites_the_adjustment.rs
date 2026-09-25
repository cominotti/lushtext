// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A5: a zero-height allocation makes a `GtkListView` rewrite the
//! adjustment its host owns. Publish a value, then allocate zero height: the
//! value jumps and `value-changed` fires although nothing asked to scroll.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a05::PUBLISHED_VALUE;
use gtk_lush_axioms::fixtures::{HostedList, LIST_HEIGHT, count_value_changes};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let emissions = count_value_changes(&hosted.adjustment);
    let adjustment = hosted.adjustment.clone();
    ui.add_control(&format!("Publish value {PUBLISHED_VALUE}"), move || {
        adjustment.set_value(PUBLISHED_VALUE);
    });
    let host = hosted.host.clone();
    ui.add_control("Allocate zero height", move || {
        host.set_child_height(0);
    });
    let host = hosted.host.clone();
    ui.add_control(&format!("Restore {LIST_HEIGHT} px"), move || {
        host.set_child_height(LIST_HEIGHT);
    });
    let adjustment = hosted.adjustment.clone();
    let host = hosted.host;
    ui.set_readout(move || {
        format!(
            "allocated {} px   value {}   page {}   upper {}   value-changed ×{}",
            host.child_height(),
            adjustment.value(),
            adjustment.page_size(),
            adjustment.upper(),
            emissions.get()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(5), build)
}

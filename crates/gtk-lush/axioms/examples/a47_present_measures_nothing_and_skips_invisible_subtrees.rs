// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A47: two counting widgets, one visible and one invisible.
//! Presenting the window measured neither; the first frame's layout measured
//! the visible one, and the invisible one has never been measured or mapped.
//! Show it, and watch it be measured in the next frame.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::CountingWidget;
use gtk_lush_axioms::fixtures::a47::COUNTING_WIDTH;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let visible = CountingWidget::new(COUNTING_WIDTH);
    let invisible = CountingWidget::new(COUNTING_WIDTH);
    invisible.set_visible(false);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    content.append(&visible);
    content.append(&invisible);
    ui.set_fixture(&content);
    let target = invisible.clone();
    ui.add_control("Show the invisible widget", move || {
        target.set_visible(true);
    });
    let target = invisible.clone();
    ui.add_control("Hide it again", move || target.set_visible(false));
    ui.set_readout(move || {
        format!(
            "visible: measures {} allocations {} mapped {}   invisible: measures {} allocations {} mapped {}",
            visible.measures(),
            visible.allocations(),
            visible.is_mapped(),
            invisible.measures(),
            invisible.allocations(),
            invisible.is_mapped(),
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(47), build)
}

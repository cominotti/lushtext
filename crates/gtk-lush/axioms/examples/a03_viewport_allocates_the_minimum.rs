// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A3: a `GtkViewport` allocates a non-scrollable child its
//! minimum in the scroll direction. The host asks for 2000 px minimum and
//! 5000 px natural; switch the viewport's policy to see which one it gets.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::ViewportedHost;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = ViewportedHost::new();
    ui.set_fixture(&fixture.scroller);
    for (label, policy) in [
        ("Policy: minimum (default)", gtk4::ScrollablePolicy::Minimum),
        ("Policy: natural", gtk4::ScrollablePolicy::Natural),
    ] {
        let viewport = fixture.viewport.clone();
        ui.add_control(label, move || {
            if let Some(viewport) = viewport.as_ref() {
                viewport.set_vscroll_policy(policy);
            }
        });
    }
    let host = fixture.host.clone();
    let scroller = fixture.scroller.clone();
    let viewport = fixture.viewport;
    ui.set_readout(move || {
        format!(
            "policy {:?}   host allocated {} px   scroller range {}",
            viewport
                .as_ref()
                .map(gtk4::prelude::ScrollableExt::vscroll_policy),
            host.height(),
            scroller.vadjustment().upper()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(3), build)
}

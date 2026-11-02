// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A52: two views of one buffer; "Unparent the right view" removes
//! it from the window but keeps it alive, and "Grow the paragraph" appends
//! text. The readout shows the paragraph height each view's layout records:
//! the unparented view's grows too, because it still lays the buffer out.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a52::{APPENDED_CHARS, SharedBufferViews};

fn build(ui: &support::SampleUi) {
    let views = SharedBufferViews::new();
    ui.set_fixture(&views.container);
    {
        let views = views.clone();
        ui.add_control("Unparent the right view", move || views.unparent_retired());
    }
    {
        let views = views.clone();
        ui.add_control("Grow the paragraph", move || {
            views.grow_paragraph(APPENDED_CHARS);
        });
    }
    ui.set_readout(move || {
        format!(
            "kept paragraph height {}   unparented paragraph height {}",
            SharedBufferViews::paragraph_height(&views.kept.view),
            SharedBufferViews::paragraph_height(&views.retired.view)
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(52), build)
}

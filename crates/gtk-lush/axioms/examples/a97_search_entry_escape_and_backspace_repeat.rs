// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A97: a search entry with a capture-phase key logger. Press
//! Escape with and without text, tap Backspace, and hold it on the empty
//! entry: the log shows the stop-search count, the text each press saw, and
//! the held key's repeated presses before its one release.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a97::{Seen, entry_fixture};

fn build(ui: &support::SampleUi) {
    let fixture = entry_fixture();
    ui.set_fixture(&fixture.entry);
    {
        let seen = fixture.seen.clone();
        ui.add_control("Clear the log", move || seen.borrow_mut().clear());
    }
    let seen = fixture.seen.clone();
    let stops = fixture.stops;
    ui.set_readout(move || {
        let events = seen
            .borrow()
            .iter()
            .map(|event| match event {
                Seen::Press(key, text) => format!("press {key:?} on {text:?}"),
                Seen::Release(key) => format!("release {key:?}"),
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("stop-search: {}\n{events}", stops.borrow())
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(97), build)
}

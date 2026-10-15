// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A38: every `set_*` through the keyfile backend replaces the
//! settings file on the calling thread, an unchanged value included. Write a
//! new value or the same value again and watch the replacement count and the
//! time the call took.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{INTEGER_KEY, KeyfileSettingsFixture};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let Some(fixture) = KeyfileSettingsFixture::new("a38-sample", 0) else {
        ui.set_fixture(&gtk4::Label::new(Some(
            "The GTK 4 file-chooser schema is not installed.",
        )));
        return;
    };
    let watch = fixture.watch_writes();
    let settings = fixture.settings();
    let label = gtk4::Label::new(Some(&fixture.path().display().to_string()));
    label.set_wrap(true);
    ui.set_fixture(&label);
    let last_us = Rc::new(Cell::new(0u128));
    let value = Rc::new(Cell::new(100i32));
    for (title, change) in [("Write a new value", true), ("Write the same value", false)] {
        let (settings, last_us, value) = (settings.clone(), last_us.clone(), value.clone());
        ui.add_control(title, move || {
            if change {
                value.set(value.get() + 1);
            }
            let started = Instant::now();
            let _ = settings.set_int(INTEGER_KEY, value.get());
            last_us.set(started.elapsed().as_micros());
        });
    }
    // The readout owns the fixture, so its directory lives as long as the
    // window and is removed with it.
    ui.set_readout(move || {
        let _keep = &fixture;
        format!(
            "value {}   file replacements {}   last set_* took {} us",
            value.get(),
            watch
                .as_ref()
                .map_or(0, gtk_lush_axioms::fixtures::WriteWatch::count),
            last_us.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(38), build)
}

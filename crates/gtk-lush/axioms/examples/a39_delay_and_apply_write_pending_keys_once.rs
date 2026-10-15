// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A39: a delayed `GSettings` keeps its writes in memory until
//! `apply()`, which writes them all in one replacement of the settings file.
//! Toggle a few keys on the delayed instance, compare what it and a plain
//! instance read, then apply.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{BOOLEAN_KEYS, KeyfileSettingsFixture};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let Some(fixture) = KeyfileSettingsFixture::new("a39-sample", 0) else {
        ui.set_fixture(&gtk4::Label::new(Some(
            "The GTK 4 file-chooser schema is not installed.",
        )));
        return;
    };
    let watch = fixture.watch_writes();
    let delayed = fixture.settings();
    delayed.delay();
    let plain = fixture.settings();
    ui.set_fixture(&gtk4::Label::new(Some(
        "A delayed and a plain GSettings instance on one private keyfile backend",
    )));
    for key in BOOLEAN_KEYS {
        let delayed = delayed.clone();
        ui.add_control(&format!("Toggle {key}"), move || {
            let _ = delayed.set_boolean(key, !delayed.boolean(key));
        });
    }
    {
        let delayed = delayed.clone();
        ui.add_control("Apply", move || delayed.apply());
    }
    ui.set_readout(move || {
        let _keep = &fixture;
        let read = |settings: &gtk4::gio::Settings| {
            BOOLEAN_KEYS
                .iter()
                .map(|key| if settings.boolean(key) { "1" } else { "0" })
                .collect::<String>()
        };
        format!(
            "delayed reads {}   plain reads {}   pending {}   file replacements {}",
            read(&delayed),
            read(&plain),
            delayed.has_unapplied(),
            watch
                .as_ref()
                .map_or(0, gtk_lush_axioms::fixtures::WriteWatch::count)
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(39), build)
}

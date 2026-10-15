// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A40: a worker thread writes the settings file through a backend
//! object of its own; the window's backend keeps answering the old value until
//! its file monitor reloads the file. Write from the worker and watch the value
//! the window reads and the `changed` count.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{INTEGER_KEY, KeyfileSettingsFixture, write_on_a_worker};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let Some(fixture) = KeyfileSettingsFixture::new("a40-sample", 0) else {
        ui.set_fixture(&gtk4::Label::new(Some(
            "The GTK 4 file-chooser schema is not installed.",
        )));
        return;
    };
    let reader = fixture.settings();
    ui.set_fixture(&gtk4::Label::new(Some(
        "The window's keyfile backend, and a worker's own backend over the same file",
    )));
    let changes = Rc::new(Cell::new(0u32));
    {
        let changes = changes.clone();
        reader.connect_changed(Some(INTEGER_KEY), move |_, _| {
            changes.set(changes.get() + 1);
        });
    }
    let value = Rc::new(Cell::new(4_000i32));
    let read_right_after = Rc::new(Cell::new(reader.int(INTEGER_KEY)));
    {
        let (path, value, reader, read_right_after) = (
            fixture.path().to_path_buf(),
            value.clone(),
            reader.clone(),
            read_right_after.clone(),
        );
        ui.add_control("Write on a worker's own backend", move || {
            value.set(value.get() + 1);
            let _ = write_on_a_worker(path.clone(), value.get());
            read_right_after.set(reader.int(INTEGER_KEY));
        });
    }
    ui.set_readout(move || {
        let _keep = &fixture;
        format!(
            "worker wrote {}   window read right after {}   window reads now {}   changed {}",
            value.get(),
            read_right_after.get(),
            reader.int(INTEGER_KEY),
            changes.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(40), build)
}

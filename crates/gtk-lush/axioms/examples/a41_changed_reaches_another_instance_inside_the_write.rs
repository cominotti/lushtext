// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A41: where `changed` arrives. A plain write reaches another
//! instance inside `set_*`; a write on a delayed instance reaches only that
//! instance, and the others when it is applied. Write either way, apply, and
//! read the emission log.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{INTEGER_KEY, KeyfileSettingsFixture};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let Some(fixture) = KeyfileSettingsFixture::new("a41-sample") else {
        ui.set_fixture(&gtk4::Label::new(Some(
            "The GTK 4 file-chooser schema is not installed.",
        )));
        return;
    };
    let writer = fixture.settings();
    let peer = fixture.settings();
    let delayed = fixture.settings();
    delayed.delay();
    ui.set_fixture(&gtk4::Label::new(Some(
        "A writer, a peer and a delayed instance on one private keyfile backend",
    )));
    let phase = Rc::new(Cell::new("idle"));
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    for (name, settings) in [("peer", &peer), ("delayed", &delayed)] {
        let (phase, log) = (phase.clone(), log.clone());
        settings.connect_changed(Some(INTEGER_KEY), move |settings, _| {
            log.borrow_mut().push(format!(
                "{name} {} {}",
                settings.int(INTEGER_KEY),
                phase.get()
            ));
        });
    }
    let _ = (peer.int(INTEGER_KEY), delayed.int(INTEGER_KEY));
    let value = Rc::new(Cell::new(500i32));
    {
        let (phase, value) = (phase.clone(), value.clone());
        ui.add_control("Plain write", move || {
            value.set(value.get() + 1);
            phase.set("inside set");
            let _ = writer.set_int(INTEGER_KEY, value.get());
            phase.set("idle");
        });
    }
    {
        let (delayed, phase, value) = (delayed.clone(), phase.clone(), value);
        ui.add_control("Delayed write", move || {
            value.set(value.get() + 1);
            phase.set("inside set");
            let _ = delayed.set_int(INTEGER_KEY, value.get());
            phase.set("idle");
        });
    }
    {
        ui.add_control("Apply", move || {
            phase.set("inside apply");
            delayed.apply();
            phase.set("idle");
        });
    }
    ui.set_readout(move || {
        let _keep = &fixture;
        let log = log.borrow();
        let tail: Vec<&String> = log.iter().rev().take(4).collect();
        format!("changed emissions {}: {tail:?}", log.len())
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(41), build)
}

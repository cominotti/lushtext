// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A44: clearing the language inside the `insert-text`
//! after-handler leaves the inserted text untagged. Toggle whether the handler
//! clears the language, insert a paragraph, and read the tags on it once the
//! main loop has drained.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a44::{DowngradingView, INSERTED_CHARS};

fn build(ui: &support::SampleUi) {
    let Some(fixture) = DowngradingView::new(true) else {
        ui.set_readout(|| "GtkSourceView has no html language".to_owned());
        return;
    };
    ui.set_fixture(&fixture.source.scrolled);
    let last_start = Rc::new(Cell::new(None::<i32>));
    {
        let fixture = fixture.clone();
        ui.add_control("Toggle clearing in the handler", move || {
            let flag = &fixture.clear_on_insert;
            flag.set(!flag.get());
        });
    }
    {
        let (fixture, last_start) = (fixture.clone(), last_start.clone());
        ui.add_control("Insert a paragraph", move || {
            last_start.set(Some(fixture.insert_paragraph()));
        });
    }
    ui.set_readout(move || {
        let tags = last_start
            .get()
            .map(|start| fixture.tags_from(start).join(", "))
            .unwrap_or_default();
        format!(
            "handler clears the language: {}   inserted {INSERTED_CHARS} chars, tags: [{tags}]",
            fixture.clear_on_insert.get()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(44), build)
}

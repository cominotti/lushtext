// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A67: a language parses its definitions at its first attach and
//! shares the parse through the language object. Attach each language with a
//! fresh private manager and read the first, shared and post-release attach
//! costs; build the language catalog on a worker and read what it answered.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a67::{LANGUAGES, SNIPPET, attach_costs, worker_catalog};
use gtk4::prelude::*;
use sourceview5::prelude::*;

fn build(ui: &support::SampleUi) {
    let view = sourceview5::View::new();
    if let Some(language) = sourceview5::LanguageManager::new().language("html")
        && let Some(buffer) = view.buffer().downcast_ref::<sourceview5::Buffer>()
    {
        buffer.set_language(Some(&language));
        buffer.set_text(SNIPPET);
    }
    view.set_monospace(true);
    ui.set_fixture(&view);
    let lines = Rc::new(RefCell::new(Vec::<String>::new()));
    {
        let lines = lines.clone();
        ui.add_control("Attach each language with a fresh manager", move || {
            for id in LANGUAGES {
                if let Some(cost) = attach_costs(id) {
                    lines.borrow_mut().push(format!(
                        "{id}: first {} us, shared {} us, after release {} us",
                        cost.first_us, cost.shared_us, cost.released_us
                    ));
                }
            }
        });
    }
    {
        let lines = lines.clone();
        ui.add_control("Build the catalog on a worker", move || {
            if let Some(catalog) = worker_catalog() {
                lines.borrow_mut().push(format!(
                    "worker: {} languages in {} us, index.html -> {:?}",
                    catalog.ids.len(),
                    catalog.list_us,
                    catalog.guess
                ));
            }
        });
    }
    ui.set_readout(move || lines.borrow().join("\n"));
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(67), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A129: each Rust-`regex` construct "Open and Replace in Editor"
//! passes through, compiled by GtkSourceView's GRegex search over one line,
//! with the first match it finds beside the one the Rust engine finds.
//! "Add an invalid pattern" shows the compile error GRegex reports for `[`,
//! the probe's control.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a129::{CONSTRUCTS, DOCUMENT, first_match};

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some(DOCUMENT));
    ui.set_fixture(&label);
    let invalid = Rc::new(Cell::new(false));
    ui.add_control("Add an invalid pattern", {
        let invalid = Rc::clone(&invalid);
        move || invalid.set(true)
    });
    ui.set_readout(move || {
        let mut lines: Vec<String> = CONSTRUCTS
            .iter()
            .map(|(name, pattern, expected)| {
                format!(
                    "{name}: {pattern} -> {:?} (Rust: {expected})",
                    first_match(pattern)
                )
            })
            .collect();
        if invalid.get() {
            lines.push(format!("invalid: [ -> {:?}", first_match("[")));
        }
        lines.join("\n")
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(129), build)
}

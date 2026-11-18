// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A127: a regex Replace All in a GtkSourceView buffer. Each
//! button runs `replace_all` with one replacement on a fresh copy of the
//! text, so `\2-\1`, `\g<first>!`, `\g<1>0` and `$1` show how GRegex
//! expands references: `$1` stays literal.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a127::{DOCUMENT, EXPANSIONS, PATTERN, replace_all_with};

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some(DOCUMENT));
    label.set_selectable(true);
    ui.set_fixture(&label);
    let last = Rc::new(RefCell::new(String::from("none yet")));
    for (replacement, _) in EXPANSIONS {
        let label = label.clone();
        let last = Rc::clone(&last);
        ui.add_control(&format!("Replace with {replacement}"), move || {
            let result = replace_all_with(replacement);
            let text = match &result {
                Ok((_, text)) => text.clone(),
                Err(error) => error.clone(),
            };
            label.set_text(&text);
            *last.borrow_mut() = format!("{replacement} -> {result:?}");
        });
    }
    ui.set_readout(move || {
        format!(
            "pattern {PATTERN} over {DOCUMENT:?}\nlast: {}",
            last.borrow()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(127), build)
}

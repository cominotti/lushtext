// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A42: text inserted whole into a `GtkTextBuffer` gets Pango's
//! paragraphs as lines — after `\n`, `\r`, `\r\n` and U+2029, never after
//! U+2028 or U+0085 — while an edit splits only its own text and a deletion
//! joins only the lines whose starts it removes. Pick a fresh case or an edit
//! case; the readout shows the buffer's lines.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a42::{
    CASES, EDIT_CASES, edited_buffer, line_texts, pango_paragraph_starts,
};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let view = gtk4::TextView::new();
    view.set_monospace(true);
    ui.set_fixture(&view);
    let note = Rc::new(RefCell::new(String::new()));
    for (label, text) in CASES {
        let (view, note) = (view.clone(), note.clone());
        ui.add_control(label, move || {
            let buffer = gtk4::TextBuffer::new(None);
            buffer.set_text(text);
            view.set_buffer(Some(&buffer));
            *note.borrow_mut() = format!(
                "{label} (fresh): pango paragraphs at {:?}",
                pango_paragraph_starts(text)
            );
        });
    }
    for (label, initial, edit, expected) in EDIT_CASES {
        let (view, note) = (view.clone(), note.clone());
        ui.add_control(label, move || {
            view.set_buffer(Some(&edited_buffer(initial, *edit)));
            *note.borrow_mut() = format!("{label} (edited): expected {expected:?}");
        });
    }
    ui.set_readout(move || {
        let lines: Vec<String> = line_texts(&view.buffer())
            .into_iter()
            .map(|(text, chars)| format!("{text:?}:{chars}"))
            .collect();
        format!("{}   buffer lines {}", note.borrow(), lines.join(" "))
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(42), build)
}

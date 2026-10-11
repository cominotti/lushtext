// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A27: applying or removing a tag emits no `changed`, leaves the
//! buffer unmodified, and records no undo step. Hide and show the text's tail
//! with an invisible tag, then type a character: only the typing counts.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{ParagraphKind, invisible_tag, paragraph};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let buffer = gtk4::TextBuffer::new(None);
    buffer.set_text(&paragraph(ParagraphKind::Ascii, 2_000, 27));
    buffer.set_modified(false);
    let tag = invisible_tag(&buffer);
    let view = gtk4::TextView::with_buffer(&buffer);
    view.set_wrap_mode(gtk4::WrapMode::WordChar);
    ui.set_fixture(&view);
    let changes = Rc::new(Cell::new(0u32));
    let counter = changes.clone();
    buffer.connect_changed(move |_| counter.set(counter.get() + 1));
    for (label, hide) in [("Hide the tail", true), ("Show the tail", false)] {
        let (buffer, tag) = (buffer.clone(), tag.clone());
        ui.add_control(label, move || {
            let (start, end) = (buffer.iter_at_offset(100), buffer.end_iter());
            if hide {
                buffer.apply_tag(&tag, &start, &end);
            } else {
                buffer.remove_tag(&tag, &start, &end);
            }
        });
    }
    {
        let buffer = buffer.clone();
        ui.add_control("Type one character", move || {
            buffer.insert_at_cursor("x");
        });
    }
    ui.set_readout(move || {
        format!(
            "changed emissions {}   modified {}   can undo {}",
            changes.get(),
            buffer.is_modified(),
            buffer.can_undo()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(27), build)
}

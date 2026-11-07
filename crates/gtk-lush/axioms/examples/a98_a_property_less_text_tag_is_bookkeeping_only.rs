// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A98: a property-less tag over some lines of a view. Tag a
//! line, insert lines above it, type, and undo: the readout lists the tagged
//! lines and whether the buffer can undo. The view never relayouts or
//! changes how the text looks, and undo only ever reverts text.

mod support;

use gtk4::prelude::*;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a98::{bookkeeping_tag, numbered_lines, tag_line, tagged_lines};

fn build(ui: &support::SampleUi) {
    let buffer = sourceview5::Buffer::new(None);
    buffer.begin_irreversible_action();
    buffer.set_text(&numbered_lines(40));
    buffer.end_irreversible_action();
    let tag = bookkeeping_tag(&buffer, "a98-sample");
    let view = sourceview5::View::with_buffer(&buffer);
    let scrolled = gtk4::ScrolledWindow::builder().child(&view).build();
    ui.set_fixture(&scrolled);
    {
        let (buffer, tag) = (buffer.clone(), tag.clone());
        ui.add_control("Tag the caret's line", move || {
            let line = buffer.iter_at_mark(&buffer.get_insert()).line();
            tag_line(&buffer, &tag, line);
        });
    }
    {
        let buffer = buffer.clone();
        ui.add_control("Insert two lines at the top", move || {
            buffer.insert(&mut buffer.start_iter(), "new\nnew\n");
        });
    }
    {
        let buffer = buffer.clone();
        ui.add_control("Undo", move || {
            buffer.undo();
        });
    }
    ui.set_readout(move || {
        format!(
            "tagged lines: {:?}\ncan undo: {}",
            tagged_lines(&buffer, &tag),
            buffer.can_undo()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(98), build)
}

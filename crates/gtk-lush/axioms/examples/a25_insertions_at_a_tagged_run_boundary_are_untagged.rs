// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A25: text inserted exactly at a tagged (bold) run's start or end
//! is untagged; text inserted strictly inside it is tagged. Each button
//! rebuilds the buffer and inserts at one site; bold shows which side of the
//! toggle the new text landed on.

mod support;

use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a25::{INSERTED, Site, tagged_buffer, tagged_inserted_chars};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let view = gtk4::TextView::new();
    view.set_monospace(true);
    ui.set_fixture(&view);
    let (buffer, _) = tagged_buffer();
    view.set_buffer(Some(&buffer));
    let view = Rc::new(view);
    for (label, site) in [
        ("Insert at the run's start", Site::Start),
        ("Insert inside the run", Site::Interior),
        ("Insert at the run's end", Site::End),
    ] {
        let view = view.clone();
        ui.add_control(label, move || {
            let (buffer, _) = tagged_buffer();
            let mut at = buffer.iter_at_offset(site.offset());
            buffer.insert(&mut at, INSERTED);
            view.set_buffer(Some(&buffer));
        });
    }
    ui.set_readout(move || {
        format!(
            "tagged inserted characters: start {}   inside {}   end {} (of {})",
            tagged_inserted_chars(Site::Start),
            tagged_inserted_chars(Site::Interior),
            tagged_inserted_chars(Site::End),
            INSERTED.len()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(25), build)
}

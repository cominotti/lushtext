// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A33: `GtkSourceSearchContext` skips hidden text by default
//! (`visible-only` is true). With `visible-only` false, the plain-text search
//! finds the matches inside hidden text and the regex search still does not.
//! The view shows the visible part of a line whose tail (three more
//! "needle"s) is hidden.

mod support;

use gtk4::prelude::*;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a33::{forward_matches, mixed_buffer, search_with};

fn build(ui: &support::SampleUi) {
    let buffer = mixed_buffer();
    let view = sourceview5::View::with_buffer(&buffer);
    ui.set_fixture(&view);
    {
        let buffer = buffer.clone();
        ui.add_control("Reveal the hidden tail", move || {
            let (start, end) = buffer.bounds();
            buffer.remove_all_tags(&start, &end);
        });
    }
    ui.set_readout(move || {
        format!(
            "matches for \"needle\" (5 in the line, 3 of them hidden)\n\
             default visible-only: plain-text {}   regex {}\n\
             visible-only false:   plain-text {}   regex {}",
            forward_matches(&search_with(&buffer, false, None)),
            forward_matches(&search_with(&buffer, true, None)),
            forward_matches(&search_with(&buffer, false, Some(false))),
            forward_matches(&search_with(&buffer, true, Some(false))),
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(33), build)
}

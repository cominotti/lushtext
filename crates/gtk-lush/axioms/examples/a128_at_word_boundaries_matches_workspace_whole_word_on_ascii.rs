// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A128: the occurrences of "foo" that GtkSourceView finds with
//! at-word-boundaries on, plain-text and regex, beside the ones workspace
//! search's whole-word rule finds, on an ASCII-identifier line and on a
//! non-ASCII one. "Show one line" switches the shown fixture between the
//! two.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a128::{
    ASCII_DOCUMENT, NEEDLE, UNICODE_DOCUMENT, UNICODE_NEEDLE, ascii_whole_word_offsets,
    unicode_whole_word_offsets, word_boundary_offsets,
};

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some(&format!("{ASCII_DOCUMENT}\n{UNICODE_DOCUMENT}")));
    ui.set_fixture(&label);
    ui.add_control("Show one line", move || {
        let next = if label.text() == ASCII_DOCUMENT {
            UNICODE_DOCUMENT
        } else {
            ASCII_DOCUMENT
        };
        label.set_text(next);
    });
    ui.set_readout(move || {
        format!(
            "ASCII, \"{NEEDLE}\": workspace {:?}, plain {:?}, regex {:?}\n\
             non-ASCII, \"{UNICODE_NEEDLE}\": workspace {:?}, plain {:?}, regex {:?}",
            ascii_whole_word_offsets(ASCII_DOCUMENT, NEEDLE),
            word_boundary_offsets(ASCII_DOCUMENT, NEEDLE, false),
            word_boundary_offsets(ASCII_DOCUMENT, NEEDLE, true),
            unicode_whole_word_offsets(UNICODE_DOCUMENT, UNICODE_NEEDLE),
            word_boundary_offsets(UNICODE_DOCUMENT, UNICODE_NEEDLE, false),
            word_boundary_offsets(UNICODE_DOCUMENT, UNICODE_NEEDLE, true),
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(128), build)
}

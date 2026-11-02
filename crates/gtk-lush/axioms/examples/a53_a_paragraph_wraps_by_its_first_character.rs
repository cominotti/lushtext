// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A53: two long paragraphs in a word-wrapping view. "Tag the first
//! character" puts a wrap-mode-none tag on the first paragraph's first
//! character only, which unwraps that whole paragraph; "Type before it"
//! inserts an untagged character in front, and the paragraph wraps again.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a53::FirstCharacterWrap;

fn build(ui: &support::SampleUi) {
    let fixture = FirstCharacterWrap::new();
    ui.set_fixture(&fixture.fixture.scrolled);
    {
        let fixture = fixture.clone();
        ui.add_control("Tag the first character", move || {
            fixture.tag_first_character();
        });
    }
    {
        let fixture = fixture.clone();
        ui.add_control("Type before it", move || {
            fixture.insert_at_paragraph_start();
        });
    }
    ui.set_readout(move || {
        format!(
            "first paragraph height {}   second paragraph height {}",
            fixture.paragraph_height(0),
            fixture.paragraph_height(1)
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(53), build)
}

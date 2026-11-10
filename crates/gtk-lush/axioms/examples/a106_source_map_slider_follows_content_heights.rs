// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A106: a view and its source map. Each button gives the view an
//! end-of-document bottom margin, gives the map none, the same margin copied,
//! or the margin scaled by the map's text scale, scrolls the view to the middle
//! and shows how far the slider's top sits from the map's drawing of the
//! view's first visible line. On GtkSourceView 5.22 the copied margin moves
//! the slider off the visible text; before 5.22 it does not.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a106::{LINES, MapFixture, MapMargin, slider_offset};

/// The last round: the margin and the slider's offset from the text, if any.
type LastRound = Option<(MapMargin, Option<i32>)>;

fn build(ui: &support::SampleUi) {
    let fixture = Rc::new(MapFixture::new(LINES));
    ui.set_fixture(&fixture.root);
    let last: Rc<Cell<LastRound>> = Rc::new(Cell::new(None));
    for (label, margin) in [
        ("No margins", MapMargin::None),
        ("Copy the bottom margin", MapMargin::Copied),
        ("Scale the bottom margin", MapMargin::Scaled),
    ] {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control(label, move || {
            last.set(Some((margin, slider_offset(&fixture, margin))));
        });
    }
    ui.set_readout(move || match last.get() {
        Some((margin, Some(offset))) => format!("{margin:?}: slider {offset:+} px from the text"),
        Some((margin, None)) => format!("{margin:?}: no slider yet"),
        None => "press a button".to_owned(),
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(106), build)
}

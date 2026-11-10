// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A103: a view and its source map, with a rule that moves the
//! slider's margins while the map carries a class. Each button resizes the
//! map, resizes its slider, or toggles that class, lets frames draw, and shows
//! how many "Trying to snapshot GtkSourceMapSlider … without a current
//! allocation" warnings they logged. On GtkSourceView 5.22 the slider resize
//! and the class toggle log; before 5.22 nothing does.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a103::{
    LINES, MapFixture, Queued, install_margin_rule, queue_and_count,
};

fn build(ui: &support::SampleUi) {
    let fixture = Rc::new(MapFixture::new(LINES));
    let _provider = install_margin_rule(&fixture.map.display());
    ui.set_fixture(&fixture.root);
    let last: Rc<Cell<Option<(Queued, usize)>>> = Rc::new(Cell::new(None));
    for (label, queued) in [
        ("Resize the map", Queued::Map),
        ("Resize the slider", Queued::Slider),
        ("Toggle the slider margin class", Queued::SliderMarginClass),
    ] {
        let (fixture, last) = (fixture.clone(), last.clone());
        ui.add_control(label, move || {
            last.set(Some((queued, queue_and_count(&fixture, queued))));
        });
    }
    ui.set_readout(move || match last.get() {
        Some((queued, warnings)) => format!("{queued:?}: {warnings} slider warning(s)"),
        None => "press a button".to_owned(),
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(103), build)
}

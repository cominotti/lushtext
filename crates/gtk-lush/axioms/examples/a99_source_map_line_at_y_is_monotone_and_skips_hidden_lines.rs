// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A99: a view with a source map, lines 200-259 hidden. The
//! readout sweeps `line_at_y` over every pixel row of the map's document and
//! reports whether it stayed monotone and whether a hidden line came back.

mod support;

use gtk4::prelude::*;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a99::{HIDDEN, MapFixture, SWEEP_LINES, sweep};

fn build(ui: &support::SampleUi) {
    let fixture = MapFixture::new(SWEEP_LINES);
    fixture.hide_lines(HIDDEN);
    ui.set_fixture(&fixture.root);
    {
        let buffer = fixture.buffer.clone();
        ui.add_control("Reveal the hidden lines", move || {
            let (start, end) = buffer.bounds();
            buffer.remove_all_tags(&start, &end);
        });
    }
    ui.set_readout(move || {
        let lines = sweep(&fixture);
        let monotone = lines
            .array_windows()
            .all(|[earlier, later]| earlier <= later);
        let hidden = lines
            .iter()
            .filter(|line| (HIDDEN.0..HIDDEN.1).contains(line))
            .count();
        format!(
            "rows swept: {}\nmonotone: {monotone}\nhidden lines returned: {hidden}",
            lines.len()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(99), build)
}

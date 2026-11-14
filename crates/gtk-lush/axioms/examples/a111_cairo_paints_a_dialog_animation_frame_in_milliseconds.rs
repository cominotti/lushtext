// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A111: a list inside an `AdwDialog`. "Open dialog" presents it
//! with its animation; run the sample with `GSK_RENDERER=cairo` and with the
//! default renderer to compare how smoothly it animates. `--check` runs the
//! probe, which times the animation frame's node under both renderers.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a111::{BAND_MODEL_ROWS, FEW_ROWS, RowText, row_list};
use libadwaita::prelude::AdwDialogExt;

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(
        "Open a dialog holding a list of 16 or 16,384 rows; the open and close animations \
         draw the dialog scaled and faded.",
    ));
    note.set_wrap(true);
    ui.set_fixture(&note);
    for rows in [FEW_ROWS, BAND_MODEL_ROWS] {
        let anchor = note.clone();
        ui.add_control(&format!("Open dialog ({rows} rows)"), move || {
            let fixture = row_list(rows, RowText::Label);
            let dialog = libadwaita::Dialog::builder()
                .content_width(640)
                .content_height(560)
                .child(&fixture.column)
                .build();
            dialog.present(Some(&anchor));
        });
    }
    ui.set_readout(|| {
        format!(
            "renderer: {}",
            std::env::var("GSK_RENDERER").unwrap_or_else(|_| "default".to_owned())
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(111), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A108: retext every realized row of a 16-row and a 16,384-row
//! list built from `GtkLabel`s and from a text-independent measure, and read
//! the next frame's layout and paint.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a108::{BAND_MODEL_ROWS, FEW_ROWS, RowText, retext_frame};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(
        "Measure presents each list in its own window, changes the text of every realized \
         row five times, and reports the median frame.",
    ));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press Measure")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Measure", move || {
            let mut lines = Vec::new();
            for kind in [RowText::Label, RowText::MeasureFree] {
                for rows in [FEW_ROWS, BAND_MODEL_ROWS] {
                    let (frame, realized) = retext_frame(kind, rows);
                    lines.push(format!(
                        "{kind:?}, {rows} rows ({realized} realized): layout {} us, paint {} us, \
                         frame {} us",
                        frame.layout_us, frame.paint_us, frame.total_us
                    ));
                }
            }
            *readout.borrow_mut() = lines.join("\n");
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(108), build)
}

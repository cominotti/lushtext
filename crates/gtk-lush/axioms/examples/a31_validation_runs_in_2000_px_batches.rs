// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A31: after the on-screen part, a `GtkTextView` lays out the rest
//! of its buffer in idle batches of 2000 px. Load a document of short lines
//! and watch the scrollbar grow: the readout lists how `upper` grew, batch by
//! batch, and the rows each batch covered.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::TextViewFixture;
use gtk_lush_axioms::fixtures::a31::{record_upper_changes, short_lines};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let fixture = TextViewFixture::new(gtk4::WrapMode::None);
    ui.set_fixture(&fixture.scrolled);
    let uppers = Rc::new(RefCell::new(Vec::<f64>::new()));
    record_upper_changes(&fixture.scrolled.vadjustment(), &uppers);
    {
        let uppers = uppers.clone();
        ui.add_control("Load 6000 short lines", move || {
            uppers.borrow_mut().clear();
            fixture.buffer.set_text(&short_lines());
        });
    }
    ui.set_readout(move || {
        let values = uppers.borrow();
        let steps: Vec<String> = values
            .array_windows::<2>()
            .map(|[before, after]| format!("{:.0}", after - before))
            .take(12)
            .collect();
        format!(
            "upper changes {}   final upper {:.0}\nfirst steps (px): {}",
            values.len(),
            values.last().copied().unwrap_or_default(),
            steps.join(", ")
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(31), build)
}

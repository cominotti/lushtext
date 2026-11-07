// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A96: a natural-height lazy list over 512 and 131,072 items,
//! presented and republished; count the model calls each makes.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a96::{MAX_CONTENT_HEIGHT, SIZES, run};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Presents a lazy list in a natural-height scroller (max {MAX_CONTENT_HEIGHT} px) over \
         {SIZES:?} items and republishes it."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Measure", move || {
            let text = SIZES
                .iter()
                .map(|items| {
                    let figures = run(*items);
                    format!(
                        "{items} items: present {} get_item; publish {} us, {} get_item; \
                         height {} px",
                        figures.present_items,
                        figures.publish.as_micros(),
                        figures.publish_items,
                        figures.scroller_height
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            *readout.borrow_mut() = text;
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(96), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A66: time one 64 Ki-character chunk read from a two-million
//! character single-line buffer at each depth, by character offset and by
//! line and byte index, and the two whole-paragraph walks.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a66::{
    DEPTHS, PARAGRAPH_CHARS, read_by_line_index, read_by_offset, single_segment_buffer,
    walk_by_line_index, walk_by_mark,
};
use gtk_lush_axioms::fixtures::{ParagraphKind, paragraph};

fn micros(work: impl FnOnce()) -> u128 {
    let started = Instant::now();
    work();
    started.elapsed().as_micros()
}

fn build(ui: &support::SampleUi) {
    let text = paragraph(ParagraphKind::Ascii, PARAGRAPH_CHARS, 66);
    let report = Rc::new(RefCell::new("press a button".to_owned()));
    ui.set_fixture(&gtk4::Label::new(Some(
        "no view: the buffer is not displayed",
    )));
    {
        let (text, report) = (text.clone(), report.clone());
        ui.add_control("Time chunk reads", move || {
            let buffer = single_segment_buffer(&text);
            let rows: Vec<String> = DEPTHS
                .iter()
                .map(|&depth| {
                    let offset = micros(|| drop(read_by_offset(&buffer, depth)));
                    let index = micros(|| drop(read_by_line_index(&buffer, depth)));
                    format!(
                        "{}k: offset {offset} µs, byte index {index} µs",
                        depth / 1024
                    )
                })
                .collect();
            *report.borrow_mut() = rows.join("\n");
        });
    }
    {
        let report = report.clone();
        ui.add_control("Time both walks", move || {
            let mark_total: u128 = walk_by_mark(&single_segment_buffer(&text)).1.iter().sum();
            let index_total: u128 = walk_by_line_index(&single_segment_buffer(&text))
                .1
                .iter()
                .sum();
            *report.borrow_mut() =
                format!("mark walk {mark_total} µs   byte-index walk {index_total} µs");
        });
    }
    ui.set_readout(move || report.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(66), build)
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A70: after an insert below the viewport, the view lays out only
//! the inserted lines, in its validation idle. Append dense lines below a
//! validated document and watch the content extent grow.
//!
//! `--derive-v-slice` (headless only, like `--check`) bisects the largest
//! insert whose validation dispatch fits half of `B_SETTLE_US`, in both wrap
//! modes, with seven repetitions per point, and prints the curve and the
//! result. It is a timing measurement: run it one at a time, and record the
//! load it printed. `--editor-like` dresses the view like LushText's editor
//! (its font CSS, line numbers, current-line highlight); `--with-map` adds a
//! `GtkSourceMap` of the buffer, like LushText's minimap.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::fixtures::a70::{
    DERIVATION_CAP_LINES, DERIVATION_REPETITIONS, ValidationFixture, ViewStyle, bisect_lines,
};
use gtk_lush_axioms::fixtures::post_open::{
    DENSE_LINE_CHARS, DenseView, HALF_SETTLE_US, dense_text,
};
use gtk_lush_axioms::{AxiomId, init_toolkit};
use gtk4::prelude::*;

/// The line counts the derivation prints the full curve at.
const CURVE: [usize; 9] = [1, 2, 4, 8, 16, 32, 64, 128, 256];

fn build(ui: &support::SampleUi) {
    let dense = DenseView::new(gtk4::WrapMode::None);
    dense.buffer.set_text(&dense_text(80, DENSE_LINE_CHARS, 55));
    ui.set_fixture(&dense.scrolled);
    for lines in [8usize, 64] {
        let dense = dense.clone();
        ui.add_control(&format!("Append {lines} dense lines"), move || {
            dense.append(&dense_text(lines, DENSE_LINE_CHARS, 56));
        });
    }
    {
        let dense = dense.clone();
        ui.add_control("Toggle wrapping", move || {
            let wrap = if dense.view.wrap_mode() == gtk4::WrapMode::None {
                gtk4::WrapMode::Word
            } else {
                gtk4::WrapMode::None
            };
            dense.view.set_wrap_mode(wrap);
        });
    }
    ui.set_readout(move || {
        let (width, height) = dense.uppers();
        format!(
            "lines {}   content {width:.0} x {height:.0} px",
            dense.buffer.line_count()
        )
    });
}

fn derive() -> ExitCode {
    if std::env::var("GTK_LUSH_AXIOMS_HEADLESS").as_deref() != Ok("1") {
        eprintln!("refusing --derive-v-slice outside a private headless session");
        return ExitCode::from(77);
    }
    if let Err(error) = init_toolkit() {
        eprintln!("GTK did not initialize: {error}");
        return ExitCode::from(77);
    }
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("load before: {}", load.trim());
    let style = if std::env::args().any(|argument| argument == "--with-map") {
        ViewStyle::EditorWithMap
    } else if std::env::args().any(|argument| argument == "--editor-like") {
        ViewStyle::Editor
    } else {
        ViewStyle::Bare
    };
    println!("view style: {style:?}");
    let mut derived = Vec::new();
    for wrap in [gtk4::WrapMode::None, gtk4::WrapMode::Word] {
        let Some(fixture) = ValidationFixture::standalone_styled(wrap, style) else {
            eprintln!("the fixture did not realize");
            return ExitCode::from(2);
        };
        let curve: Vec<String> = CURVE
            .iter()
            .map(|&lines| {
                let mut sorted = fixture.validation_samples_us(lines, DERIVATION_REPETITIONS);
                sorted.sort_unstable();
                format!(
                    "{}:{}/{}",
                    lines * DENSE_LINE_CHARS,
                    sorted.get(sorted.len() / 2).copied().unwrap_or_default(),
                    sorted.last().copied().unwrap_or_default()
                )
            })
            .collect();
        println!("curve {wrap:?} (chars:median/max us): {}", curve.join(" "));
        let mut tried = Vec::new();
        let lines = bisect_lines(
            &fixture,
            DERIVATION_CAP_LINES,
            DERIVATION_REPETITIONS,
            &mut tried,
        );
        let tried: Vec<String> = tried
            .iter()
            .map(|(lines, micros)| format!("{}:{micros}", lines * DENSE_LINE_CHARS))
            .collect();
        println!(
            "bisection {wrap:?} (chars:max over reps us, all must fit): {}",
            tried.join(" ")
        );
        println!(
            "largest fitting {wrap:?}: {lines} lines = {} chars (budget {HALF_SETTLE_US} us{})",
            lines * DENSE_LINE_CHARS,
            if lines == DERIVATION_CAP_LINES {
                ", cap reached"
            } else {
                ""
            }
        );
        derived.push(lines * DENSE_LINE_CHARS);
    }
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("load after: {}", load.trim());
    println!(
        "V_SLICE = {} chars",
        derived.iter().copied().min().unwrap_or_default()
    );
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    if std::env::args().any(|argument| argument == "--derive-v-slice") {
        derive()
    } else {
        support::run(AxiomId::new(70), build)
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A54: the context engine analyses in fixed idle slices at
//! G_PRIORITY_LOW. Run the analysis of 1 MiB of short-line HTML and read each
//! dispatch, then the order of the engine's slices and three armed idles.
//!
//! `--derive-s-bg-highlight` (headless only, like `--check`) bisects, per
//! language (html, markdown, rust, json), the largest synthetic short-line
//! document whose highlighting keeps every main-loop dispatch after the
//! language attach within `B_SETTLE_US` in an editor-like presented view, with
//! seven repetitions per point; a point passes only if every repetition's
//! worst dispatch fits. It is a timing measurement: run it one at a time, and
//! record the load it printed.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use std::process::ExitCode;

use gtk_lush_axioms::fixtures::a54::{
    B_SETTLE_US, DERIVATION_BRACKET, DERIVATION_REPETITIONS, HighlightFixture, all_fit,
    bisect_bytes, engine_dispatches, priority_trace,
};
use gtk_lush_axioms::{AxiomId, init_toolkit};

/// The languages the derivation runs for.
const LANGUAGES: [&str; 4] = ["html", "markdown", "rust", "json"];

fn build(ui: &support::SampleUi) {
    let label = gtk4::Label::new(Some("No fixture view: the probe uses a bare buffer."));
    ui.set_fixture(&label);
    let lines = Rc::new(RefCell::new(Vec::<String>::new()));
    {
        let lines = lines.clone();
        ui.add_control("Analyse 1 MiB of HTML", move || {
            if let Some((dispatches, chars)) = engine_dispatches() {
                let durations: Vec<String> =
                    dispatches.iter().map(|d| d.micros.to_string()).collect();
                lines.borrow_mut().push(format!(
                    "{chars} chars in {} dispatches (us): {}",
                    dispatches.len(),
                    durations.join(", ")
                ));
            }
        });
    }
    {
        let lines = lines.clone();
        ui.add_control("Trace priorities", move || {
            if let Some(trace) = priority_trace() {
                lines
                    .borrow_mut()
                    .push(format!("trace: {}", trace.iter().collect::<String>()));
            }
        });
    }
    ui.set_readout(move || lines.borrow().join("\n"));
}

fn describe(worst: &[u128]) -> String {
    let values: Vec<String> = worst.iter().map(ToString::to_string).collect();
    format!(
        "{} [{}]",
        if all_fit(worst) { "pass" } else { "fail" },
        values.join(",")
    )
}

fn derive() -> ExitCode {
    if std::env::var("GTK_LUSH_AXIOMS_HEADLESS").as_deref() != Ok("1") {
        eprintln!("refusing --derive-s-bg-highlight outside a private headless session");
        return ExitCode::from(77);
    }
    if let Err(error) = init_toolkit() {
        eprintln!("GTK did not initialize: {error}");
        return ExitCode::from(77);
    }
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("load before: {}", load.trim());
    println!(
        "rule: a size passes only if all {DERIVATION_REPETITIONS} repetitions keep their worst \
         post-attach dispatch <= {B_SETTLE_US} us; bracket {} .. {} bytes",
        DERIVATION_BRACKET.0, DERIVATION_BRACKET.1
    );
    let Some(fixture) = HighlightFixture::standalone() else {
        eprintln!("the fixture did not realize");
        return ExitCode::from(2);
    };
    for language in LANGUAGES {
        let mut tried = Vec::new();
        let bytes = bisect_bytes(&fixture, language, DERIVATION_REPETITIONS, &mut tried);
        let mut curve = tried.clone();
        curve.sort_by_key(|(size, _)| *size);
        for (size, worst) in &curve {
            println!("{language} {size} B: {}", describe(worst));
        }
        println!(
            "S_BG_HIGHLIGHT({language}) = {bytes} B{}",
            if bytes == DERIVATION_BRACKET.1 {
                " (upper bracket reached)"
            } else {
                ""
            }
        );
    }
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("load after: {}", load.trim());
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    if std::env::args().any(|argument| argument == "--derive-s-bg-highlight") {
        derive()
    } else {
        support::run(AxiomId::new(54), build)
    }
}

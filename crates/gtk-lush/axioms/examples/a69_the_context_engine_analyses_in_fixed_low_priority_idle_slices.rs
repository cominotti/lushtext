// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A69: the context engine analyses in fixed idle slices at
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

use gtk_lush_axioms::fixtures::a69::{
    B_INTERACT_US, B_SETTLE_US, ClassifiedDispatch, DERIVATION_BRACKET, DERIVATION_REPETITIONS,
    DispatchKind, HighlightFixture, KeyCost, all_fit, bisect_bytes, bisect_typing_bytes,
    engine_dispatches, over_budget, priority_trace, worst,
};
use gtk_lush_axioms::fixtures::post_open::short_document;
use gtk_lush_axioms::{AxiomId, SampleFlags, init_toolkit};
use gtk4::prelude::*;

/// The languages the derivation runs for unless `--languages=a,b,…` names
/// others.
const LANGUAGES: [&str; 4] = ["html", "markdown", "rust", "json"];
/// Repetitions of each attribution case.
const ATTRIBUTION_REPETITIONS: usize = 3;
/// Page downs after the analysis.
const PAGE_DOWNS: u32 = 10;
/// Keystrokes typed at each site.
const KEYSTROKES: u32 = 3;
/// The chunk sizes option (d) inserts with, bytes.
const CHUNKS: [usize; 5] = [4 * 1024, 8 * 1024, 16 * 1024, 32 * 1024, 55_380];
/// When a simulated keystroke arrives after the attach, ms.
const KEY_OFFSETS_MS: [u64; 6] = [1, 5, 15, 30, 45, 60];
/// Paragraphs left visible when the rest is hidden before the attach.
const VISIBLE_LINES: i32 = 100;
/// Keystrokes per typing point.
const TYPING_KEYS: u32 = 7;

/// The value of `--name=value` on the command line.
fn argument(name: &str) -> Option<String> {
    let prefix = format!("--{name}=");
    std::env::args().find_map(|argument| argument.strip_prefix(&prefix).map(str::to_owned))
}

/// `language:bytes` pairs from `--name=html:1000,rust:2000`.
fn sized_languages(name: &str) -> Vec<(String, usize)> {
    argument(name)
        .unwrap_or_default()
        .split(',')
        .filter_map(|pair| {
            let (language, bytes) = pair.split_once(':')?;
            Some((language.to_owned(), bytes.parse().ok()?))
        })
        .collect()
}

/// Per kind: count, worst µs, and how many exceed the budget.
fn summary(classified: &[ClassifiedDispatch]) -> String {
    let kinds = [
        DispatchKind::FirstUpdate,
        DispatchKind::Worker,
        DispatchKind::Frame,
        DispatchKind::Other,
    ];
    let parts: Vec<String> = kinds
        .iter()
        .map(|kind| {
            let of_kind: Vec<u128> = classified
                .iter()
                .filter(|dispatch| dispatch.kind == *kind)
                .map(|dispatch| dispatch.micros)
                .collect();
            format!(
                "{kind} n={} worst={} over={}",
                of_kind.len(),
                of_kind.iter().max().copied().unwrap_or_default(),
                of_kind
                    .iter()
                    .filter(|micros| **micros > B_SETTLE_US)
                    .count()
            )
        })
        .collect();
    format!(
        "worst {} us; {}; over budget [{}]",
        worst(classified),
        parts.join("; "),
        over_budget(classified, B_SETTLE_US).join(" ")
    )
}

fn headless_toolkit() -> Result<(), ExitCode> {
    if std::env::var("GTK_LUSH_AXIOMS_HEADLESS").as_deref() != Ok("1") {
        eprintln!("refusing a measurement mode outside a private headless session");
        return Err(ExitCode::from(77));
    }
    if let Err(error) = init_toolkit() {
        eprintln!("GTK did not initialize: {error}");
        return Err(ExitCode::from(77));
    }
    Ok(())
}

fn load() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn attribute() -> ExitCode {
    if let Err(code) = headless_toolkit() {
        return code;
    }
    println!("load before: {}", load());
    let Some(fixture) = HighlightFixture::standalone() else {
        return ExitCode::from(2);
    };
    for (language_id, bytes) in sized_languages("attribute-s-bg-highlight") {
        let (Some(language), Some(text)) = (
            sourceview5::LanguageManager::default().language(&language_id),
            short_document(&language_id, bytes, 54),
        ) else {
            println!("{language_id}: no language or generator");
            continue;
        };
        for repetition in 0..ATTRIBUTION_REPETITIONS {
            let attach = fixture.classified_attach(&language, &text, true);
            println!(
                "{language_id} {bytes} B attach #{repetition}: {}",
                summary(&attach)
            );
        }
        let pages = fixture.classified_page_downs(PAGE_DOWNS);
        println!(
            "{language_id} {bytes} B {PAGE_DOWNS} page downs: {}",
            summary(&pages)
        );
        let middle = fixture.buffer().char_count() / 2;
        let typed_middle = fixture.classified_typing(Some(middle), KEYSTROKES);
        println!(
            "{language_id} {bytes} B {KEYSTROKES} keys in the middle: {}",
            summary(&typed_middle)
        );
        let typed_end = fixture.classified_typing(None, KEYSTROKES);
        println!(
            "{language_id} {bytes} B {KEYSTROKES} keys at the end: {}",
            summary(&typed_end)
        );
    }
    println!("load after: {}", load());
    ExitCode::SUCCESS
}

fn describe_keys(costs: &[KeyCost]) -> String {
    let parts: Vec<String> = costs
        .iter()
        .map(|cost| format!("{}/{}", cost.worst, cost.total))
        .collect();
    let pass = !costs.is_empty() && costs.iter().all(|cost| cost.worst <= B_INTERACT_US);
    format!(
        "{} [{}]",
        if pass { "pass" } else { "fail" },
        parts.join(",")
    )
}

fn typing() -> ExitCode {
    if let Err(code) = headless_toolkit() {
        return code;
    }
    println!("load before: {}", load());
    println!(
        "rule: a size passes only if each of {TYPING_KEYS} keystrokes in the middle keeps every \
         dispatch it causes <= {B_INTERACT_US} us; entries are worst/total us per key"
    );
    let Some(fixture) = HighlightFixture::standalone() else {
        return ExitCode::from(2);
    };
    for (language_id, bytes) in sized_languages("typing-check") {
        let (Some(language), Some(text)) = (
            sourceview5::LanguageManager::default().language(&language_id),
            short_document(&language_id, bytes, 54),
        ) else {
            continue;
        };
        let costs = fixture.mid_typing_costs(&language, &text, TYPING_KEYS);
        println!(
            "{language_id} {bytes} B mid typing: {}",
            describe_keys(&costs)
        );
    }
    for language in argument("typing-s-bg-highlight")
        .unwrap_or_default()
        .split(',')
        .filter(|language| !language.is_empty())
    {
        let mut tried = Vec::new();
        let bytes = bisect_typing_bytes(&fixture, language, TYPING_KEYS, &mut tried);
        tried.sort_by_key(|(size, _)| *size);
        for (size, costs) in &tried {
            println!("{language} {size} B: {}", describe_keys(costs));
        }
        println!("TYPING_LIMIT({language}) = {bytes} B");
    }
    println!("load after: {}", load());
    ExitCode::SUCCESS
}

fn options() -> ExitCode {
    if let Err(code) = headless_toolkit() {
        return code;
    }
    println!("load before: {}", load());
    let Some(fixture) = HighlightFixture::standalone() else {
        return ExitCode::from(2);
    };
    for (language_id, bytes) in sized_languages("options-s-bg-highlight") {
        let (Some(language), Some(text)) = (
            sourceview5::LanguageManager::default().language(&language_id),
            short_document(&language_id, bytes, 54),
        ) else {
            println!("{language_id}: no language or generator");
            continue;
        };
        let label = format!("{language_id} {bytes} B");
        println!(
            "{label} baseline: {}",
            summary(&fixture.classified_attach(&language, &text, true))
        );
        let (analysis, toggle, after) = fixture.classified_delayed_enable(&language, &text);
        println!(
            "{label} (b) attach with highlighting off: {}",
            summary(&analysis)
        );
        println!(
            "{label} (b) set_highlight_syntax(true) call: {toggle} us; after it: {}",
            summary(&after)
        );
        println!(
            "{label} (c) no style scheme: {}",
            summary(&fixture.classified_attach_without_scheme(&language, &text))
        );
        for chunk in CHUNKS {
            println!(
                "{label} (d) attach to the empty buffer, insert in {chunk} B chunks: {}",
                summary(&fixture.classified_chunked(&language, &text, chunk))
            );
        }
        let pages = fixture.classified_page_downs(PAGE_DOWNS);
        println!(
            "{label} (d) then {PAGE_DOWNS} page downs: {}",
            summary(&pages)
        );
        println!(
            "{label} (e) all but the first {VISIBLE_LINES} lines hidden at the attach: {}",
            summary(&fixture.classified_attach_hidden(&language, &text, VISIBLE_LINES))
        );
        for offset in KEY_OFFSETS_MS {
            let (lateness, after) = fixture.keystroke_during_attach(&language, &text, offset);
            println!(
                "{label} (f) key {offset} ms after the attach: waited {lateness} us, longest \
                 dispatch after it {after} us"
            );
        }
    }
    println!("load after: {}", load());
    ExitCode::SUCCESS
}

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
    let named = argument("languages");
    let languages: Vec<&str> = named
        .as_deref()
        .map_or_else(|| LANGUAGES.to_vec(), |list| list.split(',').collect());
    for language in languages {
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

/// The measurement modes and their options (`support::guard`).
const FLAGS: SampleFlags = SampleFlags {
    modes: &[
        "--derive-s-bg-highlight",
        "--attribute-s-bg-highlight",
        "--options-s-bg-highlight",
        "--typing-s-bg-highlight",
        "--typing-check",
    ],
    options: &["--languages"],
};
fn main() -> ExitCode {
    if let Some(code) = support::guard(&FLAGS) {
        return code;
    }
    if std::env::args().any(|argument| argument == "--derive-s-bg-highlight") {
        derive()
    } else if argument("attribute-s-bg-highlight").is_some() {
        attribute()
    } else if argument("options-s-bg-highlight").is_some() {
        options()
    } else if argument("typing-s-bg-highlight").is_some() || argument("typing-check").is_some() {
        typing()
    } else {
        support::run(AxiomId::new(69), build)
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A85: type into a highlighted buffer and read, per edit, how
//! long the context engine's first update took (from an idle at
//! `G_PRIORITY_HIGH_IDLE` queued just before the edit), how much it reported
//! re-analysed, and whether its `G_PRIORITY_LOW` worker ran; with a downgrade
//! limit, a `G_PRIORITY_LOW - 1` idle removes the language when the first
//! update took longer, before the worker runs.
//!
//! Measurement modes (headless only):
//!
//! `--edit-cost=LANG --text-file=PATH [--positions=N] [--downgrade-us=D]`
//! installs the document into an editor-like presented view with its
//! language attached (C6b's paced install) and types one ordinary key a
//! third of the way into `N` lines spread over the document (default 24),
//! printing each key's first-update time, reported span, worker emissions
//! and worst iteration, then the maxima. With `--downgrade-us`, a key whose
//! first update took longer removes the language and the run stops there.
//!
//! `--flip-cost=LANG --text-file=PATH [--repetitions=R] [--downgrade-us=D]`
//! types each of the language's flipping keys (A84's `flipping_keys`) at the
//! start and at the middle line, character by character, then seven ordinary
//! keys, the closer, and seven more, each edit observed as above, and prints
//! the worst flipping and ordinary iteration, the worst echo (insert to the
//! next painted frame), and whether the language was removed.

mod support;

use std::process::ExitCode;

use gtk4::prelude::TextBufferExt as _;
use sourceview5::prelude::BufferExt as _;

use gtk_lush_axioms::fixtures::a69::HighlightFixture;
use gtk_lush_axioms::fixtures::a84::{CHUNK_BYTES, flipping_keys, paced_chunked_install};
use gtk_lush_axioms::fixtures::a85::{EditCost, Pacing, observe_edit_framed, paced_edit};
use gtk_lush_axioms::{AxiomId, SampleFlags, init_toolkit};

fn argument(name: &str) -> Option<String> {
    let prefix = format!("--{name}=");
    std::env::args().find_map(|argument| argument.strip_prefix(&prefix).map(str::to_owned))
}

fn load() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Everything both modes need: the presented fixture, the language and the
/// document. `Err` is the exit code.
fn prepare(
    language_id: &str,
) -> Result<(HighlightFixture, sourceview5::Language, String), ExitCode> {
    if std::env::var("GTK_LUSH_AXIOMS_HEADLESS").as_deref() != Ok("1") {
        eprintln!("refusing a measurement mode outside a private headless session");
        return Err(ExitCode::from(77));
    }
    if let Err(error) = init_toolkit() {
        eprintln!("GTK did not initialize: {error}");
        return Err(ExitCode::from(77));
    }
    let Some(language) = sourceview5::LanguageManager::default().language(language_id) else {
        eprintln!("{language_id}: no language");
        return Err(ExitCode::from(2));
    };
    let Some(text) = argument("text-file").and_then(|path| std::fs::read_to_string(path).ok())
    else {
        eprintln!("--text-file=PATH is required and must be UTF-8");
        return Err(ExitCode::from(2));
    };
    let Some(fixture) = HighlightFixture::standalone() else {
        return Err(ExitCode::from(2));
    };
    Ok((fixture, language, text))
}

fn edit(fixture: &HighlightFixture, offset: i32, text: &str, limit: Option<u128>) -> EditCost {
    let count = || fixture.frames_painted();
    if std::env::args().any(|argument| argument == "--paced") {
        let mut pacing = Pacing::DEFAULT;
        if let Some(target) = argument("target-us").and_then(|value| value.parse().ok()) {
            pacing.target_us = target;
        }
        if let Some(budget) = argument("budget-us").and_then(|value| value.parse().ok()) {
            pacing.budget_us = budget;
        }
        let paced = paced_edit(fixture.buffer(), offset, text, pacing, Some(&count));
        if std::env::var_os("A85_VERBOSE").is_some() {
            println!("  paced: {paced:?}");
        }
        return EditCost {
            first_update_us: paced.step_max_us,
            first_update_span: paced.analysed,
            worker_emissions: paced.worker_emissions,
            downgraded: paced.downgraded,
            worst_us: paced.worst_us,
            echo_us: paced.echo_us,
            ..EditCost::default()
        };
    }
    observe_edit_framed(fixture.buffer(), offset, text, limit, Some(&count))
}

fn edit_cost(language_id: &str) -> ExitCode {
    let (fixture, language, text) = match prepare(language_id) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };
    let positions: i32 = argument("positions")
        .and_then(|value| value.parse().ok())
        .unwrap_or(24);
    let limit: Option<u128> = argument("downgrade-us").and_then(|value| value.parse().ok());
    println!("load before: {}", load());
    let _ = paced_chunked_install(fixture.buffer(), &language, &text, CHUNK_BYTES);
    let buffer = fixture.buffer().clone();
    let lines = buffer.line_count();
    let mut worst = EditCost::default();
    let mut downgraded_at = None;
    for index in 0..positions.max(1) {
        let line = (lines - 1) * index / positions.max(1);
        let Some(mut iter) = buffer.iter_at_line(line) else {
            continue;
        };
        let column = iter.chars_in_line() / 3;
        let _ = iter.forward_chars(column);
        let cost = edit(&fixture, iter.offset(), "x", limit);
        println!(
            "{language_id} line {line}: first update {} us span {} worker {} worst {} us echo {} us{}",
            cost.first_update_us,
            cost.first_update_span,
            cost.worker_emissions,
            cost.worst_us,
            cost.echo_us,
            if cost.downgraded { " DOWNGRADED" } else { "" }
        );
        worst.first_update_us = worst.first_update_us.max(cost.first_update_us);
        worst.first_update_span = worst.first_update_span.max(cost.first_update_span);
        worst.worker_emissions = worst.worker_emissions.max(cost.worker_emissions);
        worst.worst_us = worst.worst_us.max(cost.worst_us);
        worst.echo_us = worst.echo_us.max(cost.echo_us);
        if cost.downgraded {
            downgraded_at = Some(line);
            break;
        }
    }
    println!(
        "EDIT_COST({language_id}, {} B) first update max {} us, span max {}, worker max {}, worst {} us, echo {} us, downgraded at {downgraded_at:?}",
        text.len(),
        worst.first_update_us,
        worst.first_update_span,
        worst.worker_emissions,
        worst.worst_us,
        worst.echo_us
    );
    println!("load after: {}", load());
    ExitCode::SUCCESS
}

/// Type `keys` one character at a time at `offset`; returns the worst
/// iteration and echo, whether the language went, and the next offset.
fn type_keys(
    fixture: &HighlightFixture,
    mut offset: i32,
    keys: &str,
    limit: Option<u128>,
) -> (u128, u128, u128, bool, i32) {
    let (mut worst, mut echo, mut first) = (0, 0, 0);
    let mut downgraded = false;
    for key in keys.chars() {
        let mut text = [0u8; 4];
        let cost = edit(fixture, offset, key.encode_utf8(&mut text), limit);
        worst = worst.max(cost.worst_us);
        echo = echo.max(cost.echo_us);
        first = first.max(cost.first_update_us);
        downgraded |= cost.downgraded;
        offset += 1;
    }
    (worst, echo, first, downgraded, offset)
}

fn flip_cost(language_id: &str) -> ExitCode {
    let (fixture, language, text) = match prepare(language_id) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };
    let repetitions: usize = argument("repetitions")
        .and_then(|value| value.parse().ok())
        .unwrap_or(2);
    let limit: Option<u128> = argument("downgrade-us").and_then(|value| value.parse().ok());
    println!("load before: {}", load());
    let (mut flip_max, mut ordinary_max, mut echo_max) = (0u128, 0u128, 0u128);
    for (opener, closer) in flipping_keys(language_id) {
        for position in ["start", "middle"] {
            for repetition in 0..repetitions {
                let _ = paced_chunked_install(fixture.buffer(), &language, &text, CHUNK_BYTES);
                let buffer = fixture.buffer();
                let offset = if position == "start" {
                    0
                } else {
                    buffer
                        .iter_at_line(buffer.line_count() / 2)
                        .map_or(0, |iter| iter.offset())
                };
                let (open, open_echo, open_first, d1, at) =
                    type_keys(&fixture, offset, opener, limit);
                let (inside, inside_echo, inside_first, d2, at) =
                    type_keys(&fixture, at, "xxxxxxx", limit);
                let (close, close_echo, close_first, d3, at) =
                    type_keys(&fixture, at, closer, limit);
                let (after, after_echo, after_first, d4, _) =
                    type_keys(&fixture, at, "xxxxxxx", limit);
                let flip = open.max(close);
                let ordinary = inside.max(after);
                let echo = open_echo.max(inside_echo).max(close_echo).max(after_echo);
                flip_max = flip_max.max(flip);
                ordinary_max = ordinary_max.max(ordinary);
                echo_max = echo_max.max(echo);
                println!(
                    "{language_id} {} B {opener:?} at {position} rep {repetition}: flip worst {flip} us \
                     (open {open} first {open_first}, close {close} first {close_first}), ordinary worst \
                     {ordinary} us (inside {inside} first {inside_first}, after {after} first {after_first}), \
                     echo max {echo} us, downgraded {}{}{}{}, language {}",
                    text.len(),
                    u8::from(d1),
                    u8::from(d2),
                    u8::from(d3),
                    u8::from(d4),
                    buffer.language().is_some()
                );
            }
        }
    }
    println!(
        "FLIP_COST({language_id}, {} B) flip {flip_max} us, ordinary {ordinary_max} us, echo {echo_max} us",
        text.len()
    );
    println!("load after: {}", load());
    ExitCode::SUCCESS
}

/// The measurement modes and their options (`support::guard`).
const FLAGS: SampleFlags = SampleFlags {
    modes: &["--edit-cost", "--flip-cost"],
    options: &[
        "--text-file",
        "--positions",
        "--downgrade-us",
        "--repetitions",
        "--paced",
        "--target-us",
        "--budget-us",
    ],
};

fn build(ui: &support::SampleUi) {
    use gtk_lush_axioms::fixtures::a85::{
        LARGE_BYTES, analysed_buffer, c_without_closers, observe_edit,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    let note = gtk4::Label::new(Some(&format!(
        "Types /* at the start of {LARGE_BYTES} bytes of C with no closer: once letting the \
         worker run, once removing the language from a G_PRIORITY_LOW - 1 idle."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Type a flipping key", move || {
            let text = c_without_closers(LARGE_BYTES);
            *readout.borrow_mut() = match (analysed_buffer("c", &text), analysed_buffer("c", &text))
            {
                (Some(kept), Some(cut)) => {
                    let kept = observe_edit(&kept, 0, "/*", None);
                    let cut = observe_edit(&cut, 0, "/*", Some(0));
                    format!(
                        "kept: first update {} us, worker emissions {}, worst {} us; removed: \
                         worker emissions {}, emissions after the idle {}, worst {} us",
                        kept.first_update_us,
                        kept.worker_emissions,
                        kept.worst_us,
                        cut.worker_emissions,
                        cut.emissions_after_decision,
                        cut.worst_us
                    )
                }
                _ => "no c language".to_owned(),
            };
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> ExitCode {
    if let Some(code) = support::guard(&FLAGS) {
        return code;
    }
    if let Some(language) = argument("edit-cost") {
        return edit_cost(&language);
    }
    if let Some(language) = argument("flip-cost") {
        return flip_cost(&language);
    }
    support::run(AxiomId::new(85), build)
}

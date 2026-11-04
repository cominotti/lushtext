// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A84: install short-line HTML chunk by chunk into a buffer whose
//! language is attached, and read, per chunk, whether a
//! `G_PRIORITY_DEFAULT_IDLE` and a `G_PRIORITY_LOW + 1` idle found the
//! largest `highlight-updated` end at the buffer's end.
//!
//! Measurement mode (headless only):
//! `--chunked-install=markdown:1048576,rust:1048576[,…] [--chunk=8192]
//! [--repetitions=3]` installs each synthetic short-line document into an
//! editor-like presented view with LushText's pacing
//! (`chunked-highlight-attach`) and prints the chunks, the low-priority
//! waits, the worst iteration, iterations over `B_SETTLE_US`, and the
//! install time.
//!
//! `--typing-limit=markdown,rust[,…] [--repetitions=3]` installs each
//! language's document that way at powers of two from 64 KiB to 1 MiB and
//! types seven keys at the document's start and seven in its middle: the
//! largest size at which every dispatch after every key stays within
//! `B_INTERACT_US` in every repetition is the language's typing limit with
//! the language attached (`chunked-highlight-attach`). Keys are also typed
//! inside six lines near the top, a third of the way across, where the
//! lane's click lands. `--from-bytes=N` starts at size N; `--text-file=PATH`
//! types into that document (for example a lane `short-language` fixture)
//! instead of the generated one, at its own size only.

mod support;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk4::prelude::TextBufferExt as _;

use gtk_lush_axioms::fixtures::a69::HighlightFixture;
use gtk_lush_axioms::fixtures::a84::{
    CHUNK_BYTES, DOCUMENT_BYTES, chunked_install, paced_chunked_install,
};
use gtk_lush_axioms::fixtures::post_open::{short_document, short_html};
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

fn measure(spec: &str) -> ExitCode {
    if std::env::var("GTK_LUSH_AXIOMS_HEADLESS").as_deref() != Ok("1") {
        eprintln!("refusing a measurement mode outside a private headless session");
        return ExitCode::from(77);
    }
    if let Err(error) = init_toolkit() {
        eprintln!("GTK did not initialize: {error}");
        return ExitCode::from(77);
    }
    let chunk = argument("chunk")
        .and_then(|value| value.parse().ok())
        .unwrap_or(CHUNK_BYTES);
    let repetitions: usize = argument("repetitions")
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    println!("load before: {}", load());
    let Some(fixture) = HighlightFixture::standalone() else {
        return ExitCode::from(2);
    };
    for pair in spec.split(',') {
        let Some((language_id, bytes)) = pair.split_once(':') else {
            continue;
        };
        let Ok(bytes) = bytes.parse::<usize>() else {
            continue;
        };
        let (Some(language), Some(text)) = (
            sourceview5::LanguageManager::default().language(language_id),
            short_document(language_id, bytes, 84),
        ) else {
            println!("{language_id}: no language or generator");
            continue;
        };
        for repetition in 0..repetitions {
            let run = paced_chunked_install(fixture.buffer(), &language, &text, chunk);
            println!(
                "{language_id} {bytes} B chunk {chunk} rep {repetition}: chunks={} low_waits={} \
                 worst={} us over_settle={} install={} ms attach={} ms",
                run.chunks,
                run.low_waits,
                run.worst_us,
                run.over_settle,
                run.install_ms,
                run.attach_ms
            );
        }
    }
    println!("load after: {}", load());
    ExitCode::SUCCESS
}

/// Typing limit sizes, bytes: powers of two from 64 KiB to 1 MiB.
const TYPING_SIZES: [usize; 5] = [1 << 16, 1 << 17, 1 << 18, 1 << 19, 1 << 20];
/// Keys typed at each position.
const TYPING_KEYS: u32 = 7;
/// The lines, near the top, typed into a third of the way across.
const CLICK_LINES: [i32; 6] = [3, 7, 11, 17, 23, 29];
/// The column typed at on those lines.
const CLICK_COLUMN: i32 = 40;

fn typing_limit(spec: &str) -> ExitCode {
    use gtk_lush_axioms::fixtures::a69::{B_INTERACT_US, worst};
    if std::env::var("GTK_LUSH_AXIOMS_HEADLESS").as_deref() != Ok("1") {
        eprintln!("refusing a measurement mode outside a private headless session");
        return ExitCode::from(77);
    }
    if let Err(error) = init_toolkit() {
        eprintln!("GTK did not initialize: {error}");
        return ExitCode::from(77);
    }
    let repetitions: usize = argument("repetitions")
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    println!("load before: {}", load());
    let Some(fixture) = HighlightFixture::standalone() else {
        return ExitCode::from(2);
    };
    for language_id in spec.split(',').filter(|id| !id.is_empty()) {
        let Some(language) = sourceview5::LanguageManager::default().language(language_id) else {
            println!("{language_id}: no language");
            continue;
        };
        let mut limit = 0;
        let from: usize = argument("from-bytes")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        // With a document given, only its own size is tried.
        let one_file = argument("text-file").is_some();
        for bytes in TYPING_SIZES
            .into_iter()
            .filter(|bytes| *bytes >= from)
            .take(if one_file { 1 } else { TYPING_SIZES.len() })
        {
            // `--text-file=<path>` types into that document instead (the
            // lane's own fixture), at its size.
            let text = match argument("text-file") {
                Some(path) => std::fs::read_to_string(path).ok(),
                None => short_document(language_id, bytes, 84),
            };
            let Some(text) = text else {
                break;
            };
            let mut passes = true;
            for repetition in 0..repetitions {
                let _ = paced_chunked_install(fixture.buffer(), &language, &text, CHUNK_BYTES);
                let middle = fixture.buffer().char_count() / 2;
                let start = worst(&fixture.classified_typing(Some(0), TYPING_KEYS));
                let mid = worst(&fixture.classified_typing(Some(middle), TYPING_KEYS));
                // Where the lane's click lands: a third of the way across a
                // line in the first screen, as often inside a token.
                let mut lines = Vec::new();
                for line in CLICK_LINES {
                    let mut iter = fixture
                        .buffer()
                        .iter_at_line(line)
                        .unwrap_or_else(|| fixture.buffer().start_iter());
                    let _ = iter.forward_chars(CLICK_COLUMN);
                    if iter.line() != line {
                        iter.backward_char();
                    }
                    lines.push(worst(
                        &fixture.classified_typing(Some(iter.offset()), TYPING_KEYS),
                    ));
                }
                let in_lines = lines.iter().copied().max().unwrap_or_default();
                println!(
                    "{language_id} {bytes} B rep {repetition}: start worst {start} us, middle worst {mid} us, in-line worst {in_lines} us {lines:?}"
                );
                passes &=
                    start <= B_INTERACT_US && mid <= B_INTERACT_US && in_lines <= B_INTERACT_US;
            }
            if !passes {
                break;
            }
            limit = bytes;
        }
        println!("TYPING_LIMIT({language_id}) = {limit} B");
    }
    println!("load after: {}", load());
    ExitCode::SUCCESS
}

/// `--flip-limit`: the largest power of two at which, at the document's
/// start and at the start of its middle line, each flipping key keeps every
/// dispatch within `B_SETTLE_US`, and ordinary keys typed in the flipped
/// document and after the closer keep every dispatch within `B_INTERACT_US`
/// (reviews M1 and F1).
fn flip_limit(spec: &str) -> ExitCode {
    use gtk_lush_axioms::fixtures::a69::{B_INTERACT_US, B_SETTLE_US};
    use gtk_lush_axioms::fixtures::a84::{flipping_keys, type_and_watch};
    if std::env::var("GTK_LUSH_AXIOMS_HEADLESS").as_deref() != Ok("1") {
        eprintln!("refusing a measurement mode outside a private headless session");
        return ExitCode::from(77);
    }
    if let Err(error) = init_toolkit() {
        eprintln!("GTK did not initialize: {error}");
        return ExitCode::from(77);
    }
    let repetitions: usize = argument("repetitions")
        .and_then(|value| value.parse().ok())
        .unwrap_or(2);
    let from: usize = argument("from-bytes")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let one_file = argument("text-file").is_some();
    println!("load before: {}", load());
    println!(
        "rule: at the start and at the middle line, each opener typed key by key until quiet \
         (every iteration <= {B_SETTLE_US} us), then 7 ordinary keys (<= {B_INTERACT_US} us), \
         then the closer (<= {B_SETTLE_US} us), then 7 ordinary keys (<= {B_INTERACT_US} us), \
         in every repetition; echo = insert to next painted frame"
    );
    let Some(fixture) = HighlightFixture::standalone() else {
        return ExitCode::from(2);
    };
    for language_id in spec.split(',').filter(|id| !id.is_empty()) {
        let keys = flipping_keys(language_id);
        let Some(language) = sourceview5::LanguageManager::default().language(language_id) else {
            println!("{language_id}: no language");
            continue;
        };
        if keys.is_empty() {
            println!("{language_id}: no flipping keys");
            continue;
        }
        let mut limit = 0;
        for bytes in TYPING_SIZES
            .into_iter()
            .filter(|bytes| *bytes >= from)
            .take(if one_file { 1 } else { TYPING_SIZES.len() })
        {
            let text = match argument("text-file") {
                Some(path) => std::fs::read_to_string(path).ok(),
                None => short_document(language_id, bytes, 84),
            };
            let Some(text) = text else {
                break;
            };
            let mut passes = true;
            for (opener, closer) in keys {
                for position in ["start", "middle"] {
                    for repetition in 0..repetitions {
                        let _ =
                            paced_chunked_install(fixture.buffer(), &language, &text, CHUNK_BYTES);
                        let buffer = fixture.buffer();
                        let offset = if position == "start" {
                            0
                        } else {
                            buffer
                                .iter_at_line(buffer.line_count() / 2)
                                .map_or(0, |iter| iter.offset())
                        };
                        let open = type_and_watch(&fixture, offset, opener, 0);
                        let after = offset + i32::try_from(opener.chars().count()).unwrap_or(0);
                        let flipped = type_and_watch(&fixture, after, "", 7);
                        let after = after + 7;
                        let close = type_and_watch(&fixture, after, closer, 0);
                        let after = after + i32::try_from(closer.chars().count()).unwrap_or(0);
                        let back = type_and_watch(&fixture, after, "", 7);
                        let flip_worst = open.worst_us.max(close.worst_us);
                        let ordinary_worst = flipped.worst_us.max(back.worst_us);
                        println!(
                            "{language_id} {} B {opener:?} at {position} rep {repetition}: flip worst {flip_worst} us \
                             (open {} quiet {} ms, close {} quiet {} ms), ordinary worst {ordinary_worst} us \
                             (flipped {}, after {}), echo max {} us",
                            text.len(),
                            open.worst_us,
                            open.quiet_ms,
                            close.worst_us,
                            close.quiet_ms,
                            flipped.worst_us,
                            back.worst_us,
                            open.echo_max_us
                                .max(close.echo_max_us)
                                .max(flipped.echo_max_us)
                                .max(back.echo_max_us),
                        );
                        passes &= flip_worst <= B_SETTLE_US && ordinary_worst <= B_INTERACT_US;
                    }
                }
            }
            if !passes {
                break;
            }
            limit = bytes;
        }
        println!("FLIP_LIMIT({language_id}) = {limit} B");
    }
    println!("load after: {}", load());
    ExitCode::SUCCESS
}

/// The measurement modes and their options (`support::guard`).
const FLAGS: SampleFlags = SampleFlags {
    modes: &["--chunked-install", "--typing-limit", "--flip-limit"],
    options: &["--chunk", "--repetitions", "--from-bytes", "--text-file"],
};

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Installs {DOCUMENT_BYTES} bytes of short-line HTML in chunks of at most {CHUNK_BYTES} \
         bytes into an attached buffer with no view."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Install chunk by chunk", move || {
            let text = short_html(DOCUMENT_BYTES, 84);
            *readout.borrow_mut() = chunked_install("html", &text, CHUNK_BYTES).map_or_else(
                || "no html language".to_owned(),
                |chunks| {
                    let by_default = chunks.iter().filter(|c| c.at_end_by_default_idle).count();
                    let by_low = chunks.iter().filter(|c| c.at_end_by_low_idle).count();
                    let workers: u32 = chunks.iter().map(|c| c.worker_dispatches).sum();
                    let worst = chunks.iter().map(|c| c.worst_us).max().unwrap_or_default();
                    format!(
                        "{} chunks: analysed by the default idle {by_default}, by the low + 1 \
                         idle {by_low}; worker dispatches {workers}; worst iteration {worst} us",
                        chunks.len()
                    )
                },
            );
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> ExitCode {
    if let Some(code) = support::guard(&FLAGS) {
        return code;
    }
    if let Some(spec) = argument("flip-limit") {
        return flip_limit(&spec);
    }
    if let Some(spec) = argument("chunked-install") {
        return measure(&spec);
    }
    if let Some(spec) = argument("typing-limit") {
        return typing_limit(&spec);
    }
    support::run(AxiomId::new(84), build)
}

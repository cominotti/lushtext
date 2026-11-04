// SPDX-License-Identifier: MIT OR Apache-2.0

//! A84: a GtkSourceView buffer's **`highlight-updated` end offsets tell how
//! far its context engine has analysed**, so text can be installed into a
//! language-attached buffer chunk by chunk, each chunk analysed before the
//! next one goes in.
//!
//! After an insert at the end of a buffer whose language is attached (or an
//! attach to a buffer that already holds text), the engine analyses from the
//! invalidated point forwards in its first update (`G_PRIORITY_HIGH_IDLE`)
//! and, if that 10 ms slice does not reach the end, in its `G_PRIORITY_LOW`
//! worker (A69). Each analysed range is refreshed and reported by one
//! `highlight-updated (start, end)`; the **largest `end` reported since the
//! insert reaches the end of the last line's text** ([`analysis_end`]) exactly
//! when the analysis has reached the end of the buffer, and a `G_PRIORITY_LOW + 1` idle queued
//! after the insert (which waits for the worker, A69) always finds it there.
//! The same holds for the first attach to a buffer holding one chunk.
//!
//! What C6 measured, and this probe records as magnitudes, never as a
//! verdict: for paragraph-aligned chunks of at most 8 KiB the **first update
//! alone** analyses the chunk (a `G_PRIORITY_DEFAULT_IDLE` idle queued after
//! the insert already finds the analysis at the end, and no worker slice
//! runs); larger chunks need the worker. Whether a chunk fits the first
//! update's 10 ms slice depends on the machine and its load, which is why
//! LushText's chunked install checks the reported coverage before each next
//! chunk instead of trusting the chunk size (`chunked-highlight-attach`, C6b).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gtk4::glib;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::a68::line_slices;
use crate::probes::post_open::{MAX_ITERATIONS, short_html};
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// The chunk LushText installs a highlighted document in, bytes.
pub const CHUNK_BYTES: usize = 8 * 1024;
/// Bytes of short-line HTML the probe installs chunk by chunk.
pub const DOCUMENT_BYTES: usize = 256 * 1024;
/// A chunk far larger than the first update's slice can analyse, for the
/// control that the coverage check is not vacuous.
pub const LARGE_CHUNK_BYTES: usize = 256 * 1024;
/// Seed of the probe document.
pub const SEED: u64 = 84;

/// The largest `highlight-updated` end offset since the last reset.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    /// The largest end offset reported since the last [`Coverage::reset`].
    pub covered_to: Rc<Cell<i32>>,
    /// Emissions since the last reset.
    pub emissions: Rc<Cell<u32>>,
    /// Emissions whose end lay past the buffer's character count.
    pub beyond_end: Rc<Cell<u32>>,
}

impl Coverage {
    /// Start watching `buffer`.
    #[must_use]
    pub fn watch(buffer: &sourceview5::Buffer) -> Self {
        let coverage = Self::default();
        let (covered, emissions, beyond) = (
            Rc::clone(&coverage.covered_to),
            Rc::clone(&coverage.emissions),
            Rc::clone(&coverage.beyond_end),
        );
        buffer.connect_highlight_updated(move |buffer, _, end| {
            emissions.set(emissions.get() + 1);
            if end.offset() > buffer.char_count() {
                beyond.set(beyond.get() + 1);
            }
            covered.set(covered.get().max(end.offset()));
        });
        coverage
    }

    /// Forget what was reported (before an insert).
    pub fn reset(&self) {
        self.covered_to.set(0);
        self.emissions.set(0);
    }

    /// Whether the analysis has reported the end of `buffer`
    /// (`analysis_end`).
    #[must_use]
    pub fn at_end(&self, buffer: &sourceview5::Buffer) -> bool {
        self.covered_to.get() >= analysis_end(buffer)
    }
}

/// The offset the engine's last `highlight-updated` reports once it has
/// analysed the whole buffer: the end of the last line's text. When the text
/// ends with a paragraph delimiter, the empty last line has no text, and the
/// report ends where that delimiter starts (measured: one character before
/// the character count for `\n`).
#[must_use]
pub fn analysis_end(buffer: &sourceview5::Buffer) -> i32 {
    let end = buffer.end_iter();
    if end.is_start() || !end.starts_line() {
        return end.offset();
    }
    let mut iter = end;
    iter.backward_line();
    if !iter.ends_line() {
        iter.forward_to_line_end();
    }
    iter.offset()
}

/// What one chunk's insert observed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChunkObservation {
    /// Bytes in the chunk.
    pub bytes: usize,
    /// Whether a `G_PRIORITY_DEFAULT_IDLE` idle queued after the insert found
    /// the analysis at the buffer's end.
    pub at_end_by_default_idle: bool,
    /// Whether a `G_PRIORITY_LOW + 1` idle queued after the insert found it
    /// there.
    pub at_end_by_low_idle: bool,
    /// Engine dispatches (iterations with `highlight-updated`) after the
    /// first one, before the `G_PRIORITY_LOW + 1` idle: worker slices.
    pub worker_dispatches: u32,
    /// The longest iteration until the `G_PRIORITY_LOW + 1` idle, µs.
    pub worst_us: u128,
    /// The largest reported end, and the buffer's characters, at that idle.
    pub covered_to: i32,
    /// The buffer's characters then.
    pub chars: i32,
    /// Emissions since the change.
    pub emissions: u32,
}

/// Insert `text` at the end of `buffer` and drive the default context until a
/// `G_PRIORITY_LOW + 1` idle queued after the insert dispatches, observing
/// coverage at a `G_PRIORITY_DEFAULT_IDLE` idle and at that one.
#[must_use]
pub fn observe_insert(
    buffer: &sourceview5::Buffer,
    coverage: &Coverage,
    text: &str,
) -> ChunkObservation {
    coverage.reset();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, text);
    observe_until_low_idle(buffer, coverage, text.len())
}

/// Drive the default context until a `G_PRIORITY_LOW + 1` idle armed now
/// dispatches (coverage must have been reset before the change it follows).
#[must_use]
pub fn observe_until_low_idle(
    buffer: &sourceview5::Buffer,
    coverage: &Coverage,
    bytes: usize,
) -> ChunkObservation {
    let default_seen: Rc<Cell<Option<bool>>> = Rc::default();
    let low_seen: Rc<Cell<Option<bool>>> = Rc::default();
    {
        let (buffer, coverage, seen) = (buffer.clone(), coverage.clone(), Rc::clone(&default_seen));
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            seen.set(Some(coverage.at_end(&buffer)));
            glib::ControlFlow::Break
        });
    }
    {
        let (buffer, coverage, seen) = (buffer.clone(), coverage.clone(), Rc::clone(&low_seen));
        let lower = glib::Priority::from(glib::ffi::G_PRIORITY_LOW + 1);
        glib::idle_add_local_full(lower, move || {
            seen.set(Some(coverage.at_end(&buffer)));
            glib::ControlFlow::Break
        });
    }
    let context = glib::MainContext::default();
    let mut engine_dispatches = 0u32;
    let mut worst = 0u128;
    for _ in 0..MAX_ITERATIONS {
        if low_seen.get().is_some() {
            break;
        }
        let before = coverage.emissions.get();
        let started = Instant::now();
        if !context.iteration(false) {
            continue;
        }
        worst = worst.max(started.elapsed().as_micros());
        if coverage.emissions.get() != before {
            engine_dispatches += 1;
        }
    }
    ChunkObservation {
        bytes,
        at_end_by_default_idle: default_seen.get().unwrap_or(false),
        at_end_by_low_idle: low_seen.get().unwrap_or(false),
        worker_dispatches: engine_dispatches.saturating_sub(1),
        worst_us: worst,
        covered_to: coverage.covered_to.get(),
        chars: buffer.char_count(),
        emissions: coverage.emissions.get(),
    }
}

/// A highlighted buffer with no view and `language_id` attached while it is
/// empty, its first update drained. `None` without that language.
#[must_use]
pub fn attached_empty_buffer(language_id: &str) -> Option<sourceview5::Buffer> {
    let language = sourceview5::LanguageManager::default().language(language_id)?;
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_highlight_syntax(true);
    buffer.set_language(Some(&language));
    flush_events();
    Some(buffer)
}

/// Install `text` chunk by chunk into an attached, empty buffer, observing
/// every chunk.
#[must_use]
pub fn chunked_install(
    language_id: &str,
    text: &str,
    chunk: usize,
) -> Option<Vec<ChunkObservation>> {
    let buffer = attached_empty_buffer(language_id)?;
    let coverage = Coverage::watch(&buffer);
    let observed = line_slices(text, chunk)
        .into_iter()
        .map(|slice| observe_insert(&buffer, &coverage, slice))
        .collect();
    drop(buffer);
    flush_events();
    Some(observed)
}

/// Attach `language_id` to a highlighted buffer already holding `text`
/// (one chunk) and observe until a `G_PRIORITY_LOW + 1` idle.
#[must_use]
pub fn attach_to_one_chunk(language_id: &str, text: &str) -> Option<ChunkObservation> {
    let language = sourceview5::LanguageManager::default().language(language_id)?;
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_highlight_syntax(true);
    buffer.set_text(text);
    flush_events();
    let coverage = Coverage::watch(&buffer);
    buffer.set_language(Some(&language));
    let observed = observe_until_low_idle(&buffer, &coverage, text.len());
    drop(buffer);
    flush_events();
    Some(observed)
}

/// Probe A84.
#[must_use]
pub fn probe_a84() -> Observation {
    Recorder::run(AxiomId::new(84), |recorder| {
        let text = short_html(DOCUMENT_BYTES, SEED);
        let chunks = chunked_install("html", &text, CHUNK_BYTES);
        recorder.control(chunks.is_some(), "control: an html language is installed")?;
        let chunks = chunks.unwrap_or_default();
        recorder.control(
            chunks.len() >= 16 && chunks.iter().all(|chunk| chunk.bytes <= CHUNK_BYTES),
            "control: the document went in as at least 16 chunks of at most 8 KiB",
        )?;
        let by_default = chunks.iter().filter(|c| c.at_end_by_default_idle).count();
        let workers: u32 = chunks.iter().map(|c| c.worker_dispatches).sum();
        let worst = chunks.iter().map(|c| c.worst_us).max().unwrap_or_default();
        recorder.measure("chunks", chunks.len());
        recorder.measure("analysed_by_default_idle", by_default);
        recorder.measure("worker_dispatches", workers);
        recorder.measure("worst_iteration_us", worst);
        if let (Some(first), Some(last)) = (chunks.first(), chunks.last()) {
            recorder.measure(
                "first_chunk",
                format!(
                    "{}/{} in {} emissions",
                    first.covered_to, first.chars, first.emissions
                ),
            );
            recorder.measure(
                "last_chunk",
                format!(
                    "{}/{} in {} emissions",
                    last.covered_to, last.chars, last.emissions
                ),
            );
        }
        recorder.axiom(
            chunks.iter().all(|chunk| chunk.at_end_by_low_idle),
            "after each chunk's insert, a G_PRIORITY_LOW + 1 idle finds the largest \
             highlight-updated end at the buffer's end",
        )?;

        let first = text.get(..CHUNK_BYTES).map(|head| {
            let cut = head.rfind('\n').map_or(head.len(), |newline| newline + 1);
            &text[..cut]
        });
        let attach = first.and_then(|first| attach_to_one_chunk("html", first));
        recorder.control(attach.is_some(), "control: the attach to one chunk ran")?;
        let attach = attach.unwrap_or_default();
        recorder.measure(
            "attach_one_chunk_by_default_idle",
            attach.at_end_by_default_idle,
        );
        recorder.measure(
            "attach_one_chunk_worker_dispatches",
            attach.worker_dispatches,
        );
        recorder.axiom(
            attach.at_end_by_low_idle,
            "after an attach to a buffer holding one chunk, a G_PRIORITY_LOW + 1 idle finds \
             the largest highlight-updated end at the buffer's end",
        )?;

        let crlf = text.replace('\n', "\r\n");
        let crlf_chunks = chunked_install("html", &crlf, CHUNK_BYTES).unwrap_or_default();
        let unterminated = text.trim_end_matches('\n');
        let open_chunks = chunked_install("html", unterminated, CHUNK_BYTES).unwrap_or_default();
        if let (Some(c), Some(o)) = (crlf_chunks.last(), open_chunks.last()) {
            recorder.measure("crlf_last_chunk", format!("{}/{}", c.covered_to, c.chars));
            recorder.measure(
                "unterminated_last_chunk",
                format!("{}/{}", o.covered_to, o.chars),
            );
        }
        recorder.axiom(
            !crlf_chunks.is_empty()
                && crlf_chunks.iter().all(|chunk| chunk.at_end_by_low_idle)
                && !open_chunks.is_empty()
                && open_chunks.iter().all(|chunk| chunk.at_end_by_low_idle),
            "the same holds for CRLF text and for text whose last paragraph has no delimiter",
        )?;

        let large = chunked_install(
            "html",
            &short_html(LARGE_CHUNK_BYTES * 2, SEED),
            LARGE_CHUNK_BYTES,
        )
        .unwrap_or_default();
        recorder.measure(
            "large_chunks_by_default_idle",
            large.iter().filter(|c| c.at_end_by_default_idle).count(),
        );
        recorder.measure(
            "large_chunks_worker_dispatches",
            large.iter().map(|c| c.worker_dispatches).sum::<u32>(),
        );
        recorder.control(
            large.iter().any(|chunk| chunk.worker_dispatches > 0),
            "control: a 256 KiB chunk needs the worker, so the coverage check is not vacuous",
        )?;
        recorder.axiom(
            large
                .iter()
                .all(|chunk| !chunk.at_end_by_default_idle || chunk.worker_dispatches == 0)
                && large.iter().all(|chunk| chunk.at_end_by_low_idle),
            "a chunk the worker still analyses is not yet reported at the end by a \
             G_PRIORITY_DEFAULT_IDLE idle, and is by the G_PRIORITY_LOW + 1 idle",
        )?;
        flush_events();
        Ok(())
    })
}

/// What one paced chunked install into a presented view measured.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PacedInstallRun {
    /// Chunks inserted.
    pub chunks: usize,
    /// Steps that found the analysis behind and waited at
    /// `G_PRIORITY_LOW + 1` once.
    pub low_waits: usize,
    /// The longest main-loop iteration from the attach until the view and
    /// the engine were quiet, µs (the attach call itself excluded).
    pub worst_us: u128,
    /// Iterations over `B_SETTLE_US`.
    pub over_settle: usize,
    /// Milliseconds from the first insert to the last chunk analysed.
    pub install_ms: u128,
    /// Milliseconds the attach call took (the A67 parse).
    pub attach_ms: u128,
}

/// LushText's pacing, reproduced: insert one chunk, queue the next step at
/// `G_PRIORITY_DEFAULT_IDLE`; a step that finds the analysis behind waits
/// once at `G_PRIORITY_LOW + 1` (after any worker slice, A69) and then
/// inserts. The first chunk goes in before the attach, as LushText's install
/// does, so the first frame shows text.
#[must_use]
pub fn paced_chunked_install(
    buffer: &sourceview5::Buffer,
    language: &sourceview5::Language,
    text: &str,
    chunk: usize,
) -> PacedInstallRun {
    use std::cell::RefCell;
    use std::time::Duration;
    buffer.set_language(None::<&sourceview5::Language>);
    buffer.set_highlight_syntax(true);
    buffer.set_text("");
    flush_events();
    let slices: Rc<Vec<String>> = Rc::new(
        line_slices(text, chunk)
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let Some(first) = slices.first() else {
        return PacedInstallRun::default();
    };
    buffer.insert(&mut buffer.end_iter(), first);
    flush_events();
    let coverage = Coverage::watch(buffer);
    let started = Instant::now();
    buffer.set_language(Some(language));
    let attach_ms = started.elapsed().as_millis();
    let next = Rc::new(Cell::new(1usize));
    let waits = Rc::new(Cell::new(0usize));
    let done_at: Rc<Cell<Option<Instant>>> = Rc::default();
    type Step = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;
    let step: Step = Rc::default();
    {
        let (buffer, coverage, slices, next, waits, done_at, weak) = (
            buffer.clone(),
            coverage,
            Rc::clone(&slices),
            Rc::clone(&next),
            Rc::clone(&waits),
            Rc::clone(&done_at),
            Rc::downgrade(&step),
        );
        *step.borrow_mut() = Some(Box::new(move |waited: bool| {
            let reschedule = |priority: i32, waited: bool| {
                let weak = weak.clone();
                glib::idle_add_local_full(glib::Priority::from(priority), move || {
                    if let Some(step) = weak.upgrade()
                        && let Some(step) = step.borrow().as_ref()
                    {
                        step(waited);
                    }
                    glib::ControlFlow::Break
                });
            };
            if !coverage.at_end(&buffer) && !waited {
                waits.set(waits.get() + 1);
                reschedule(glib::ffi::G_PRIORITY_LOW + 1, true);
                return;
            }
            let index = next.get();
            let Some(slice) = slices.get(index) else {
                done_at.set(Some(Instant::now()));
                return;
            };
            next.set(index + 1);
            coverage.reset();
            buffer.insert(&mut buffer.end_iter(), slice);
            reschedule(glib::ffi::G_PRIORITY_DEFAULT_IDLE, false);
        }));
    }
    {
        let weak = Rc::downgrade(&step);
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            if let Some(step) = weak.upgrade()
                && let Some(step) = step.borrow().as_ref()
            {
                step(false);
            }
            glib::ControlFlow::Break
        });
    }
    let mut worst = 0u128;
    let mut over = 0usize;
    let mut last_activity = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(600);
    while Instant::now() < deadline
        && (done_at.get().is_none() || last_activity.elapsed() < Duration::from_millis(300))
    {
        let Some(dispatch) = crate::probes::post_open::timed_iteration(None) else {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        };
        worst = worst.max(dispatch.micros);
        if dispatch.micros > crate::probes::a69::B_SETTLE_US {
            over += 1;
        }
        if dispatch.micros >= 1_000 {
            last_activity = Instant::now();
        }
    }
    step.borrow_mut().take();
    PacedInstallRun {
        chunks: slices.len(),
        low_waits: waits.get(),
        worst_us: worst,
        over_settle: over,
        install_ms: done_at
            .get()
            .map_or(0, |done| done.duration_since(started).as_millis()),
        attach_ms,
    }
}

/// The keys that flip the syntax context of everything after them, per
/// language, each an opener and the closer typed right after it
/// (`chunked-highlight-attach` reviews M1 and F1). Each is typed one
/// character at a time.
#[must_use]
pub fn flipping_keys(language_id: &str) -> &'static [(&'static str, &'static str)] {
    match language_id {
        "rust" | "sh" | "yaml" => &[("\"", "\"")],
        "python3" | "python" => &[("\"\"\"", "\"\"\"")],
        "markdown" => &[("```\n", "```\n"), ("<!--", "-->")],
        "c" | "js" | "typescript" | "css" => &[("/*", "*/")],
        "xml" => &[("<!--", "-->")],
        _ => &[],
    }
}

/// What typing one key sequence cost.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TypedRun {
    /// The longest main-loop iteration from the first key until quiet, µs.
    pub worst_us: u128,
    /// Iterations over `B_SETTLE_US`.
    pub over_settle: usize,
    /// Milliseconds from the first key until quiet.
    pub quiet_ms: u128,
    /// The longest time from a key's insert to the next painted frame, µs:
    /// for the sequence's own keys and for ordinary keys typed while the
    /// analysis it caused runs.
    pub echo_max_us: u128,
}

/// Type `keys` one character at a time at `offset` of `fixture`'s buffer,
/// and while the analysis runs type `ordinary` plain keys one reference
/// frame apart right after them; time every iteration until quiet.
#[must_use]
pub fn type_and_watch(
    fixture: &crate::probes::a69::HighlightFixture,
    offset: i32,
    keys: &str,
    ordinary: u32,
) -> TypedRun {
    use std::time::Duration;
    let buffer = fixture.buffer();
    let context = glib::MainContext::default();
    let quiet = Duration::from_millis(300);
    let started = Instant::now();
    let mut run = TypedRun::default();
    let mut cursor = offset;
    let mut last_activity = Instant::now();
    let mut pending: Vec<char> = keys.chars().collect();
    pending.reverse();
    let mut plain_left = ordinary;
    let mut echo_from: Option<(Instant, u32)> = None;
    let mut last_key = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        let typing_due = last_key.elapsed() >= Duration::from_millis(16) && echo_from.is_none();
        if typing_due
            && let Some(key) = pending.pop().or_else(|| {
                (plain_left > 0).then(|| {
                    plain_left -= 1;
                    'x'
                })
            })
        {
            let mut iter = buffer.iter_at_offset(cursor);
            let mut text = [0u8; 4];
            buffer.insert(&mut iter, key.encode_utf8(&mut text));
            cursor += 1;
            last_key = Instant::now();
            echo_from = Some((Instant::now(), fixture.frames_painted()));
            last_activity = Instant::now();
            continue;
        }
        if pending.is_empty()
            && plain_left == 0
            && echo_from.is_none()
            && last_activity.elapsed() >= quiet
        {
            break;
        }
        let iteration = Instant::now();
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_micros(200));
            continue;
        }
        let micros = iteration.elapsed().as_micros();
        run.worst_us = run.worst_us.max(micros);
        if micros > crate::probes::a69::B_SETTLE_US {
            run.over_settle += 1;
        }
        if micros >= 1_000 {
            last_activity = Instant::now();
        }
        if let Some((since, _)) = echo_from
            && since.elapsed() >= Duration::from_secs(2)
        {
            run.echo_max_us = run.echo_max_us.max(since.elapsed().as_micros());
            echo_from = None;
        }
        if let Some((since, frames)) = echo_from
            && fixture.frames_painted() != frames
        {
            run.echo_max_us = run.echo_max_us.max(since.elapsed().as_micros());
            echo_from = None;
        }
    }
    run.quiet_ms = started
        .elapsed()
        .as_millis()
        .saturating_sub(quiet.as_millis());
    run
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! A68: with a language attached, `set_highlight_syntax(true)` refreshes the
//! **whole buffer inside the call** (`gtksourcecontextengine.c`
//! `enable_highlight` l. 2275–2297 adds the whole buffer to the refresh
//! region and calls `refresh_range`, which re-applies the context classes and
//! emits `highlight-updated` for the whole range, l. 1210–1235), so its cost
//! grows with the buffer. Inserting text into a buffer whose language is
//! already attached **only invalidates**: no `highlight-updated` is emitted
//! inside an insert call; the analysis runs afterwards, from the engine's
//! idles.
//!
//! Recorded magnitudes, never verdicts: the cost of the toggle at 64 kB, 1 MB
//! and 5 MB of short-line HTML (after the engine has analysed the text with
//! highlighting off), the cost of attaching the language to an empty buffer,
//! of each 256 KiB insert after it, and the count and longest of the highlight
//! dispatches those inserts schedule, both when the loop drains after every
//! insert and when the inserts run back to back.
//!
//! LushText's post-open settle (`bound-post-open-settle`, D3) chooses its
//! publish order from this: an install that keeps the engine attached never
//! pays the document-sized toggle at publish.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::post_open::{MAX_ITERATIONS, TimedDispatch, short_html, timed_drain};
use crate::probes::text::time_us;
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// The document sizes the toggle cost is recorded at, in bytes.
pub const TOGGLE_SIZES: [usize; 3] = [64 * 1024, 1 << 20, 5 << 20];
/// Bytes per insert slice (LushText's `REPLACEMENT_INSERT_SLICE_BYTES`).
pub const INSERT_SLICE: usize = 256 * 1024;
/// Bytes the sliced inserts add in total.
pub const INSERTED_TOTAL: usize = 1 << 20;
/// Seed of the probe documents.
pub const SEED: u64 = 53;

/// Counts `highlight-updated` emissions on a buffer and remembers the end
/// offset of the last one.
#[derive(Clone, Debug, Default)]
pub struct HighlightUpdates {
    /// Emissions so far.
    pub count: Rc<Cell<u32>>,
    /// End offset of the last emission.
    pub last_end: Rc<Cell<i32>>,
}

impl HighlightUpdates {
    /// Start counting on `buffer`.
    #[must_use]
    pub fn watch(buffer: &sourceview5::Buffer) -> Self {
        let updates = Self::default();
        let (count, last_end) = (Rc::clone(&updates.count), Rc::clone(&updates.last_end));
        buffer.connect_highlight_updated(move |_, _, end| {
            count.set(count.get() + 1);
            last_end.set(end.offset());
        });
        updates
    }
}

/// An html buffer with highlighting set to `highlight`, or `None` without an
/// html language.
#[must_use]
pub fn html_buffer(highlight: bool) -> Option<sourceview5::Buffer> {
    let language = sourceview5::LanguageManager::default().language("html")?;
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_highlight_syntax(highlight);
    buffer.set_language(Some(&language));
    Some(buffer)
}

/// What turning highlighting on over one document observed.
#[derive(Clone, Copy, Debug)]
pub struct ToggleRun {
    /// Bytes in the document.
    pub bytes: usize,
    /// Characters in the buffer.
    pub chars: i32,
    /// Dispatches the analysis with highlighting off took.
    pub analysis_dispatches: usize,
    /// Microseconds `set_highlight_syntax(true)` took.
    pub toggle_us: u128,
    /// `highlight-updated` emissions inside the call.
    pub emissions_inside: u32,
    /// End offset of the last emission inside the call.
    pub emitted_end: i32,
}

/// Load `bytes` of short-line HTML with highlighting off, let the engine
/// analyse it, then turn highlighting on.
#[must_use]
pub fn toggle_run(bytes: usize) -> Option<ToggleRun> {
    let buffer = html_buffer(false)?;
    buffer.set_text(&short_html(bytes, SEED));
    let analysis_dispatches = timed_drain(MAX_ITERATIONS, None).len();
    let updates = HighlightUpdates::watch(&buffer);
    let ((), toggle_us) = time_us(|| buffer.set_highlight_syntax(true));
    let run = ToggleRun {
        bytes,
        chars: buffer.char_count(),
        analysis_dispatches,
        toggle_us,
        emissions_inside: updates.count.get(),
        emitted_end: updates.last_end.get(),
    };
    drop(buffer);
    flush_events();
    Some(run)
}

/// The paragraph-aligned slices of `text`, each at most `slice` bytes unless
/// one line is longer.
#[must_use]
pub fn line_slices(text: &str, slice: usize) -> Vec<&str> {
    let mut slices = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let cut = if rest.len() <= slice {
            rest.len()
        } else {
            rest[..slice]
                .rfind('\n')
                .map_or(slice, |newline| newline + 1)
        };
        let (head, tail) = rest.split_at(cut);
        slices.push(head);
        rest = tail;
    }
    slices
}

/// What sliced inserts into a language-attached buffer observed.
#[derive(Clone, Debug)]
pub struct InsertRun {
    /// Microseconds attaching the language to the empty buffer took.
    pub attach_us: u128,
    /// Microseconds each insert took.
    pub insert_us: Vec<u128>,
    /// `highlight-updated` emissions inside the insert calls.
    pub emissions_inside: u32,
    /// The dispatches after each insert (one entry when back to back).
    pub drains: Vec<Vec<TimedDispatch>>,
    /// `highlight-updated` emissions during the drains.
    pub emissions_after: u32,
}

/// Attach html to an empty highlighted buffer, then insert
/// [`INSERTED_TOTAL`] bytes in [`INSERT_SLICE`] slices, draining the main
/// loop after each insert or only after the last.
#[must_use]
pub fn insert_run(drain_each: bool) -> Option<InsertRun> {
    let language = sourceview5::LanguageManager::default().language("html")?;
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_highlight_syntax(true);
    let ((), attach_us) = time_us(|| buffer.set_language(Some(&language)));
    flush_events();
    let updates = HighlightUpdates::watch(&buffer);
    let text = short_html(INSERTED_TOTAL, SEED);
    let mut run = InsertRun {
        attach_us,
        insert_us: Vec::new(),
        emissions_inside: 0,
        drains: Vec::new(),
        emissions_after: 0,
    };
    for slice in line_slices(&text, INSERT_SLICE) {
        let before = updates.count.get();
        let ((), micros) = time_us(|| {
            let mut end = buffer.end_iter();
            buffer.insert(&mut end, slice);
        });
        run.emissions_inside += updates.count.get() - before;
        run.insert_us.push(micros);
        if drain_each {
            let before = updates.count.get();
            run.drains.push(timed_drain(MAX_ITERATIONS, None));
            run.emissions_after += updates.count.get() - before;
        }
    }
    if !drain_each {
        let before = updates.count.get();
        run.drains.push(timed_drain(MAX_ITERATIONS, None));
        run.emissions_after += updates.count.get() - before;
    }
    drop(buffer);
    flush_events();
    Some(run)
}

fn describe_drains(drains: &[Vec<TimedDispatch>]) -> String {
    super::join_entries(drains.iter().map(|drain| {
        let longest = drain.iter().map(|d| d.micros).max().unwrap_or_default();
        format!("{} dispatches, longest {longest} us", drain.len())
    }))
}

/// Probe A68.
#[must_use]
pub fn probe_a68() -> Observation {
    Recorder::run(AxiomId::new(68), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let mut toggles = Vec::new();
        for bytes in TOGGLE_SIZES {
            toggles.push(recorder.require(
                toggle_run(bytes),
                "control: GtkSourceView has an html language",
            )?);
        }
        recorder.measure(
            "toggle_on",
            super::join_entries(toggles.iter().map(|run| {
                format!(
                    "{} B: {} us ({} emissions to offset {} of {}; analysis took {} dispatches)",
                    run.bytes,
                    run.toggle_us,
                    run.emissions_inside,
                    run.emitted_end,
                    run.chars,
                    run.analysis_dispatches
                )
            })),
        );
        recorder.axiom(
            toggles
                .iter()
                .all(|run| run.emissions_inside >= 1 && run.emitted_end >= run.chars - 1),
            "turning highlighting on refreshes the whole buffer inside the call",
        )?;

        let paced = recorder.require(insert_run(true), "control: html language")?;
        let batched = recorder.require(insert_run(false), "control: html language")?;
        recorder.measure("attach_empty_us", paced.attach_us);
        recorder.measure("insert_us", super::join_entries(&paced.insert_us));
        recorder.measure("drained_after_each", describe_drains(&paced.drains));
        recorder.measure(
            "back_to_back_insert_us",
            super::join_entries(&batched.insert_us),
        );
        recorder.measure("drained_after_last", describe_drains(&batched.drains));
        recorder.measure(
            "emissions_inside_inserts",
            paced.emissions_inside + batched.emissions_inside,
        );
        recorder.measure(
            "emissions_after_inserts",
            paced.emissions_after + batched.emissions_after,
        );
        recorder.control(
            paced.insert_us.len() >= 4 && paced.emissions_after > 0 && batched.emissions_after > 0,
            "control: the sliced inserts were analysed by the engine afterwards",
        )?;
        recorder.axiom(
            paced.emissions_inside == 0 && batched.emissions_inside == 0,
            "an insert into a language-attached buffer only invalidates: nothing is highlighted \
             inside the insert call",
        )
    })
}

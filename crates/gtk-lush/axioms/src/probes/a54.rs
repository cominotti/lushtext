// SPDX-License-Identifier: MIT OR Apache-2.0

//! A54: GtkSourceView's context engine analyses a buffer in **main-loop
//! dispatches of a fixed slice** the application cannot configure
//! (GtkSourceView 5.20 `gtksourcecontextengine.c` l. 46–67): one
//! **first update** at `G_PRIORITY_HIGH_IDLE` (`FIRST_UPDATE_PRIORITY`) with a
//! **10 ms** slice (`FIRST_UPDATE_TIME_SLICE`), then, while anything is left,
//! an **incremental worker** at `G_PRIORITY_LOW`
//! (`INCREMENTAL_UPDATE_PRIORITY`) with a **30 ms** slice
//! (`INCREMENTAL_UPDATE_TIME_SLICE`). The worker is one `g_idle_add_full`
//! source that returns `G_SOURCE_CONTINUE` until everything is analysed
//! (`idle_worker` l. 2331–2347, `install_idle_worker` l. 2382–2390), so it
//! is **ready again at once** after every slice: there is no timeout and no
//! gap between slices beyond the main loop's own turn. The slice bounds only
//! the analysis (`update_syntax` checks its timer per line, l. 5513); the
//! context-class refresh and `highlight-updated` emission of the analysed
//! range follow inside the same dispatch (`refresh_range`, l. 5554–5558), so
//! a dispatch can run past the slice.
//!
//! Structurally, on a 1 MiB short-line HTML buffer with no view: every
//! slice but the last leaves the default main context ready; a
//! `G_PRIORITY_DEFAULT_IDLE` idle armed after the first update dispatches
//! before the worker's first slice; a second `G_PRIORITY_LOW` idle armed then
//! dispatches **between every two worker slices** (equal priority: GLib
//! dispatches every ready source of the top priority in one iteration); and a
//! `G_PRIORITY_LOW + 1` idle waits until the analysis is complete.
//!
//! Recorded magnitudes, never verdicts: each dispatch's duration and the
//! offset it analysed to, and the gap between dispatches.
//!
//! The sample's `--derive-s-bg-highlight` mode measures, per language, the
//! largest synthetic short-line document whose highlighting keeps every
//! dispatch after the language attach within LushText's `B_SETTLE_US`
//! (`HighlightFixture`, `bisect_bytes`); the attach call itself is the A52
//! parse and is not timed.
//!
//! LushText's post-open settle (`bound-post-open-settle`) cannot shorten the
//! slices (D4 gates background highlighting by total size instead), and its
//! settle sentinel cannot rely on the worker outranking a `G_PRIORITY_LOW`
//! idle: they are equal, so a low-priority sentinel dispatches between worker
//! slices (review R25).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gtk4::glib;
use gtk4::glib::translate::IntoGlib;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::a53::{HighlightUpdates, html_buffer};
use crate::probes::post_open::{MAX_ITERATIONS, short_html};
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// Bytes of short-line HTML in the probe document.
pub const DOCUMENT_BYTES: usize = 1 << 20;
/// `FIRST_UPDATE_TIME_SLICE`, ms.
pub const FIRST_UPDATE_SLICE_MS: u32 = 10;
/// `INCREMENTAL_UPDATE_TIME_SLICE`, ms.
pub const INCREMENTAL_SLICE_MS: u32 = 30;
/// Seed of the probe document.
pub const SEED: u64 = 54;

/// One engine dispatch: an iteration in which `highlight-updated` fired.
#[derive(Clone, Copy, Debug)]
pub struct EngineDispatch {
    /// Microseconds the iteration took.
    pub micros: u128,
    /// The offset the analysis reached.
    pub analysed_to: i32,
    /// Whether the default main context was ready right after it.
    pub ready_after: bool,
    /// Microseconds from the end of the previous engine dispatch to the start
    /// of this one (0 for the first).
    pub gap_us: u128,
}

/// Load the probe document into a highlighted html buffer with no view and
/// time every engine dispatch until nothing is ready.
#[must_use]
pub fn engine_dispatches() -> Option<(Vec<EngineDispatch>, i32)> {
    let buffer = html_buffer(true)?;
    let updates = HighlightUpdates::watch(&buffer);
    buffer.set_text(&short_html(DOCUMENT_BYTES, SEED));
    let context = glib::MainContext::default();
    let mut dispatches = Vec::new();
    let mut previous_end: Option<Instant> = None;
    for _ in 0..MAX_ITERATIONS {
        let before = updates.count.get();
        let started = Instant::now();
        if !context.iteration(false) {
            break;
        }
        let ended = Instant::now();
        if updates.count.get() != before {
            dispatches.push(EngineDispatch {
                micros: (ended - started).as_micros(),
                analysed_to: updates.last_end.get(),
                ready_after: context.pending(),
                gap_us: previous_end.map_or(0, |end| (started - end).as_micros()),
            });
            previous_end = Some(Instant::now());
        }
    }
    let chars = buffer.char_count();
    drop(buffer);
    flush_events();
    Some((dispatches, chars))
}

/// Run the analysis again with three idles armed after the first update —
/// `G_PRIORITY_DEFAULT_IDLE` (`D`), a continuing `G_PRIORITY_LOW` sentinel
/// (`L`, stops once a turn passed with no engine dispatch) and
/// `G_PRIORITY_LOW + 1` (`Z`) — and return the order of events, `H` for each
/// engine dispatch.
#[must_use]
pub fn priority_trace() -> Option<Vec<char>> {
    let buffer = html_buffer(true)?;
    let updates = HighlightUpdates::watch(&buffer);
    let trace = Rc::new(RefCell::new(Vec::new()));
    {
        let trace = Rc::clone(&trace);
        buffer.connect_highlight_updated(move |_, _, _| trace.borrow_mut().push('H'));
    }
    buffer.set_text(&short_html(DOCUMENT_BYTES, SEED));
    let context = glib::MainContext::default();
    // The first update (G_PRIORITY_HIGH_IDLE) runs alone.
    while updates.count.get() == 0 && context.iteration(false) {}
    {
        let trace = Rc::clone(&trace);
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            trace.borrow_mut().push('D');
            glib::ControlFlow::Break
        });
    }
    {
        let trace = Rc::clone(&trace);
        let count = Rc::clone(&updates.count);
        let seen = Cell::new(count.get());
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            trace.borrow_mut().push('L');
            if count.get() == seen.get() {
                glib::ControlFlow::Break
            } else {
                seen.set(count.get());
                glib::ControlFlow::Continue
            }
        });
    }
    {
        let trace = Rc::clone(&trace);
        glib::idle_add_local_full(
            glib::Priority::from(glib::Priority::LOW.into_glib() + 1),
            move || {
                trace.borrow_mut().push('Z');
                glib::ControlFlow::Break
            },
        );
    }
    let mut iterations = 0;
    while context.iteration(false) && iterations < MAX_ITERATIONS {
        iterations += 1;
    }
    drop(buffer);
    flush_events();
    Some(trace.take())
}

/// Whether, in `trace`, a `L` lies between every two consecutive `H` after
/// the first `H` (the first update).
#[must_use]
pub fn low_between_every_slice(trace: &[char]) -> bool {
    let after_first: Vec<char> = trace
        .iter()
        .copied()
        .skip_while(|event| *event != 'H')
        .skip(1)
        .filter(|event| matches!(event, 'H' | 'L'))
        .collect();
    after_first
        .array_windows::<2>()
        .all(|[left, right]| !(*left == 'H' && *right == 'H'))
}

/// Probe A54.
#[must_use]
pub fn probe_a54() -> Observation {
    Recorder::run(AxiomId::new(54), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        recorder.measure(
            "slices_cited_ms",
            format!(
                "first update {FIRST_UPDATE_SLICE_MS} at G_PRIORITY_HIGH_IDLE, incremental \
                 {INCREMENTAL_SLICE_MS} at G_PRIORITY_LOW"
            ),
        );
        let (dispatches, chars) = recorder.require(
            engine_dispatches(),
            "control: GtkSourceView has an html language",
        )?;
        recorder.measure("document_chars", chars);
        recorder.measure("engine_dispatches", dispatches.len());
        recorder.measure(
            "dispatch_us",
            super::join_entries(dispatches.iter().map(|d| d.micros)),
        );
        recorder.measure(
            "analysed_to",
            super::join_entries(dispatches.iter().map(|d| d.analysed_to)),
        );
        recorder.measure(
            "max_gap_us",
            dispatches
                .iter()
                .map(|d| d.gap_us)
                .max()
                .unwrap_or_default(),
        );
        recorder.control(
            dispatches.len() >= 3
                && dispatches
                    .last()
                    .is_some_and(|d| d.analysed_to >= chars - 1),
            "control: the analysis needed the first update and at least two worker slices, and \
             reached the end",
        )?;
        let (last, earlier) = dispatches
            .split_last()
            .map_or((None, &[][..]), |(l, e)| (Some(l), e));
        recorder.axiom(
            earlier.iter().all(|d| d.ready_after) && last.is_some_and(|d| !d.ready_after),
            "the engine reschedules through an idle: every slice but the last leaves the default \
             main context ready at once",
        )?;

        let trace = recorder.require(priority_trace(), "control: html language")?;
        let text: String = trace.iter().collect();
        recorder.measure("priority_trace", &text);
        let first_h = trace.iter().position(|e| *e == 'H');
        let second_h = trace
            .iter()
            .enumerate()
            .filter(|(_, e)| **e == 'H')
            .nth(1)
            .map(|(index, _)| index);
        let last_h = trace.iter().rposition(|e| *e == 'H');
        let d = trace.iter().position(|e| *e == 'D');
        let z = trace.iter().position(|e| *e == 'Z');
        recorder.control(
            first_h.is_some() && second_h.is_some() && d.is_some() && z.is_some(),
            "control: the trace holds the first update, a worker slice and both one-shot idles",
        )?;
        recorder.axiom(
            d < second_h,
            "a G_PRIORITY_DEFAULT_IDLE idle armed during the analysis dispatches before the \
             next worker slice",
        )?;
        recorder.axiom(
            low_between_every_slice(&trace),
            "a G_PRIORITY_LOW idle armed during the analysis dispatches between every two worker \
             slices",
        )?;
        recorder.axiom(
            z > last_h,
            "a G_PRIORITY_LOW + 1 idle waits until the analysis is complete",
        )
    })
}

/// The budget every post-attach dispatch must fit for a size to pass the
/// `S_BG_HIGHLIGHT` derivation: LushText's `B_SETTLE_US` (16,000 µs, owned by
/// `crates/lushtext-core/src/model/responsiveness_budget.rs`).
pub const B_SETTLE_US: u128 = 16_000;
/// The smallest and largest document sizes the derivation brackets, bytes.
pub const DERIVATION_BRACKET: (usize, usize) = (4 * 1024, 2 << 20);
/// Repetitions per derivation point; a point passes only if all of them do.
pub const DERIVATION_REPETITIONS: usize = 7;
/// The bisection stops when the failing size is within this many percent of
/// the passing one.
pub const DERIVATION_PRECISION_PERCENT: usize = 5;

/// A presented, editor-like source view (monospace, word wrap, line
/// numbers) at the reference size, for the `S_BG_HIGHLIGHT` derivation.
pub struct HighlightFixture {
    view: crate::probes::post_open::DenseView,
    frames: crate::probes::post_open::FrameCounter,
    _shown: crate::session::Presented,
}

impl HighlightFixture {
    /// Present the fixture. `None` when it never realized.
    #[must_use]
    pub fn standalone() -> Option<Self> {
        let mut recorder = Recorder::detached();
        let view = crate::probes::post_open::DenseView::new(gtk4::WrapMode::Word);
        view.view.set_show_line_numbers(true);
        view.buffer.set_highlight_syntax(true);
        let shown = crate::session::Presented::checked_sized(
            &mut recorder,
            &view.scrolled,
            crate::probes::post_open::REFERENCE_WINDOW,
            "control: the view was presented",
        )
        .ok()?;
        let frames = crate::probes::post_open::FrameCounter::watch(&view.view)?;
        Some(Self {
            view,
            frames,
            _shown: shown,
        })
    }

    /// Load `text` with no language and let the view settle; then attach
    /// `language` (the attach call itself is not timed: it is the A52
    /// parse) and time every main-loop iteration until the engine and the
    /// view are idle. Returns the longest iteration, µs, and how many engine
    /// dispatches the analysis took.
    #[must_use]
    pub fn worst_dispatch_after_attach(
        &self,
        language: &sourceview5::Language,
        text: &str,
    ) -> (u128, u32) {
        use crate::probes::post_open::watch_until_quiet;
        use std::time::Duration;
        let buffer = &self.view.buffer;
        buffer.set_language(None::<&sourceview5::Language>);
        buffer.set_text(text);
        buffer.place_cursor(&buffer.start_iter());
        let _ = watch_until_quiet(
            Some(&self.frames),
            Duration::from_millis(200),
            Duration::from_secs(60),
            || 0.0,
        );
        let updates = HighlightUpdates::watch(buffer);
        buffer.set_language(Some(language));
        let count = std::rc::Rc::clone(&updates.count);
        let watched = watch_until_quiet(
            Some(&self.frames),
            Duration::from_millis(300),
            Duration::from_secs(120),
            move || f64::from(count.get()),
        );
        let worst = watched
            .iter()
            .map(|watched| watched.dispatch.micros)
            .max()
            .unwrap_or_default();
        (worst, updates.count.get())
    }

    /// The worst dispatch of each of `repetitions` runs at `bytes`.
    #[must_use]
    pub fn repetitions(&self, language_id: &str, bytes: usize, repetitions: usize) -> Vec<u128> {
        let (Some(language), Some(text)) = (
            sourceview5::LanguageManager::default().language(language_id),
            crate::probes::post_open::short_document(language_id, bytes, SEED),
        ) else {
            return Vec::new();
        };
        (0..repetitions)
            .map(|_| self.worst_dispatch_after_attach(&language, &text).0)
            .collect()
    }
}

/// Whether every repetition's worst dispatch fits [`B_SETTLE_US`].
#[must_use]
pub fn all_fit(worst: &[u128]) -> bool {
    !worst.is_empty() && worst.iter().all(|micros| *micros <= B_SETTLE_US)
}

/// Bisect the largest size in [`DERIVATION_BRACKET`] at which every one of
/// `repetitions` runs keeps every post-attach dispatch within
/// [`B_SETTLE_US`]: powers of two first, then halving the bracket until the
/// failing size is within [`DERIVATION_PRECISION_PERCENT`] of the passing
/// one. Every point tried is appended to `tried` with its worst dispatches.
/// Returns the largest passing size (0 when the smallest fails; the upper
/// bracket when it passes).
#[must_use]
pub fn bisect_bytes(
    fixture: &HighlightFixture,
    language_id: &str,
    repetitions: usize,
    tried: &mut Vec<(usize, Vec<u128>)>,
) -> usize {
    let mut fits = |bytes: usize| {
        let worst = fixture.repetitions(language_id, bytes, repetitions);
        let passes = all_fit(&worst);
        tried.push((bytes, worst));
        passes
    };
    let (smallest, largest) = DERIVATION_BRACKET;
    if !fits(smallest) {
        return 0;
    }
    let mut low = smallest;
    let mut high = None;
    while low < largest {
        let next = (low * 2).min(largest);
        if fits(next) {
            low = next;
        } else {
            high = Some(next);
            break;
        }
    }
    let Some(mut high) = high else {
        return low;
    };
    while (high - low) * 100 > low * DERIVATION_PRECISION_PERCENT {
        let middle = low + (high - low) / 2;
        if fits(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

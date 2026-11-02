// SPDX-License-Identifier: MIT OR Apache-2.0

//! A69: GtkSourceView's context engine analyses a buffer in **main-loop
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
//! (`HighlightFixture`, `bisect_bytes`); the attach call itself is the A67
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
use crate::probes::a68::{HighlightUpdates, html_buffer};
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

/// Probe A69.
#[must_use]
pub fn probe_a69() -> Observation {
    Recorder::run(AxiomId::new(69), |recorder| {
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
    /// `language` (the attach call itself is not timed: it is the A67
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

/// What ran in one main-loop iteration the attribution timed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchKind {
    /// The engine's first-update idle (the first `highlight-updated` after an
    /// attach, an edit or a view request).
    FirstUpdate,
    /// A `G_PRIORITY_LOW` worker slice.
    Worker,
    /// A frame (with or without engine work in the same iteration).
    Frame,
    /// Anything else: GTK validation, the fixture's own call.
    Other,
}

impl std::fmt::Display for DispatchKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::FirstUpdate => "first-update",
            Self::Worker => "worker",
            Self::Frame => "frame",
            Self::Other => "other",
        })
    }
}

/// One timed, classified iteration.
#[derive(Clone, Copy, Debug)]
pub struct ClassifiedDispatch {
    /// What ran.
    pub kind: DispatchKind,
    /// Microseconds it took.
    pub micros: u128,
}

/// The dispatches of `classified` over `budget`, as `kind:µs` entries.
#[must_use]
pub fn over_budget(classified: &[ClassifiedDispatch], budget: u128) -> Vec<String> {
    classified
        .iter()
        .filter(|dispatch| dispatch.micros > budget)
        .map(|dispatch| format!("{}:{}", dispatch.kind, dispatch.micros))
        .collect()
}

/// The longest of `classified`, µs.
#[must_use]
pub fn worst(classified: &[ClassifiedDispatch]) -> u128 {
    classified
        .iter()
        .map(|d| d.micros)
        .max()
        .unwrap_or_default()
}

impl HighlightFixture {
    /// The fixture's buffer.
    #[must_use]
    pub fn buffer(&self) -> &sourceview5::Buffer {
        &self.view.buffer
    }

    /// The fixture's view.
    #[must_use]
    pub fn source_view(&self) -> &sourceview5::View {
        &self.view.view
    }

    /// Drive the main loop until 300 ms pass with no engine dispatch and no
    /// iteration of 1 ms or more, classifying every iteration. The first
    /// engine dispatch after `first_update_armed` is set (an attach, an edit)
    /// counts as the first update; `updates` counts `highlight-updated`.
    #[must_use]
    pub fn classified_until_quiet(
        &self,
        updates: &HighlightUpdates,
        mut first_update_armed: bool,
    ) -> Vec<ClassifiedDispatch> {
        use std::time::Duration;
        let quiet = Duration::from_millis(300);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut last_activity = Instant::now();
        let mut classified = Vec::new();
        while Instant::now() < deadline && last_activity.elapsed() < quiet {
            let before = updates.count.get();
            let Some(dispatch) = crate::probes::post_open::timed_iteration(Some(&self.frames))
            else {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            };
            let engine = updates.count.get() != before;
            let kind = if dispatch.frames > 0 {
                DispatchKind::Frame
            } else if engine && first_update_armed {
                DispatchKind::FirstUpdate
            } else if engine {
                DispatchKind::Worker
            } else {
                DispatchKind::Other
            };
            if engine {
                first_update_armed = false;
            }
            if engine || dispatch.micros >= 1_000 {
                last_activity = Instant::now();
            }
            classified.push(ClassifiedDispatch {
                kind,
                micros: dispatch.micros,
            });
        }
        classified
    }

    /// Load `text` with no language, settle, then attach `language` (not
    /// timed) with `highlight` and classify every iteration until idle.
    #[must_use]
    pub fn classified_attach(
        &self,
        language: &sourceview5::Language,
        text: &str,
        highlight: bool,
    ) -> Vec<ClassifiedDispatch> {
        let buffer = &self.view.buffer;
        buffer.set_language(None::<&sourceview5::Language>);
        buffer.set_highlight_syntax(highlight);
        buffer.set_text(text);
        buffer.place_cursor(&buffer.start_iter());
        self.scroll_to(0.0);
        let updates = HighlightUpdates::watch(buffer);
        let _ = self.classified_until_quiet(&updates, false);
        buffer.set_language(Some(language));
        self.classified_until_quiet(&updates, true)
    }

    /// Scroll the view to `value` and settle.
    pub fn scroll_to(&self, value: f64) {
        self.view.scrolled.vadjustment().set_value(value);
    }

    /// After the analysis, page down `pages` times from the top, classifying
    /// the iterations after each page.
    #[must_use]
    pub fn classified_page_downs(&self, pages: u32) -> Vec<ClassifiedDispatch> {
        let updates = HighlightUpdates::watch(&self.view.buffer);
        self.scroll_to(0.0);
        let _ = self.classified_until_quiet(&updates, false);
        let adjustment = self.view.scrolled.vadjustment();
        let mut all = Vec::new();
        for _ in 0..pages {
            adjustment.set_value(adjustment.value() + adjustment.page_size());
            all.extend(self.classified_until_quiet(&updates, true));
        }
        all
    }

    /// Type `keys` characters at `offset` (`None`: the end), each followed by
    /// the iterations it causes; the insert call itself is timed as
    /// [`DispatchKind::Other`].
    #[must_use]
    pub fn classified_typing(&self, offset: Option<i32>, keys: u32) -> Vec<ClassifiedDispatch> {
        let buffer = &self.view.buffer;
        let updates = HighlightUpdates::watch(buffer);
        let iter = offset.map_or_else(|| buffer.end_iter(), |offset| buffer.iter_at_offset(offset));
        buffer.place_cursor(&iter);
        self.view.view.scroll_mark_onscreen(&buffer.get_insert());
        let _ = self.classified_until_quiet(&updates, false);
        let mut all = Vec::new();
        for _ in 0..keys {
            let started = Instant::now();
            buffer.insert_interactive_at_cursor("x", true);
            self.view.view.scroll_mark_onscreen(&buffer.get_insert());
            all.push(ClassifiedDispatch {
                kind: DispatchKind::Other,
                micros: started.elapsed().as_micros(),
            });
            all.extend(self.classified_until_quiet(&updates, true));
        }
        all
    }

    /// Option (d): attach `language` to the empty buffer, then insert `text`
    /// in paragraph-aligned chunks of at most `chunk` bytes, letting the main
    /// loop go quiet after each. Returns every timed step, the inserts
    /// included.
    #[must_use]
    pub fn classified_chunked(
        &self,
        language: &sourceview5::Language,
        text: &str,
        chunk: usize,
    ) -> Vec<ClassifiedDispatch> {
        let buffer = &self.view.buffer;
        buffer.set_language(None::<&sourceview5::Language>);
        buffer.set_highlight_syntax(true);
        buffer.set_text("");
        let updates = HighlightUpdates::watch(buffer);
        let _ = self.classified_until_quiet(&updates, false);
        buffer.set_language(Some(language));
        let mut all = self.classified_until_quiet(&updates, true);
        for slice in crate::probes::a68::line_slices(text, chunk) {
            let started = Instant::now();
            let mut end = buffer.end_iter();
            buffer.insert(&mut end, slice);
            all.push(ClassifiedDispatch {
                kind: DispatchKind::Other,
                micros: started.elapsed().as_micros(),
            });
            all.extend(self.classified_until_quiet(&updates, true));
        }
        all
    }

    /// Option (c): the same attach with no style scheme on the buffer.
    #[must_use]
    pub fn classified_attach_without_scheme(
        &self,
        language: &sourceview5::Language,
        text: &str,
    ) -> Vec<ClassifiedDispatch> {
        let buffer = &self.view.buffer;
        let scheme = buffer.style_scheme();
        buffer.set_style_scheme(None::<&sourceview5::StyleScheme>);
        let classified = self.classified_attach(language, text, true);
        buffer.set_style_scheme(scheme.as_ref());
        classified
    }

    /// Option (b): attach with highlighting off, let the analysis run, then
    /// turn highlighting on. Returns (the analysis's dispatches, the toggle
    /// call's µs, the dispatches after the toggle).
    #[must_use]
    pub fn classified_delayed_enable(
        &self,
        language: &sourceview5::Language,
        text: &str,
    ) -> (Vec<ClassifiedDispatch>, u128, Vec<ClassifiedDispatch>) {
        let analysis = self.classified_attach(language, text, false);
        let updates = HighlightUpdates::watch(&self.view.buffer);
        let started = Instant::now();
        self.view.buffer.set_highlight_syntax(true);
        let toggle = started.elapsed().as_micros();
        let after = self.classified_until_quiet(&updates, true);
        (analysis, toggle, after)
    }
}

/// LushText's `B_INTERACT_US` (8,000 µs, owned by
/// `crates/lushtext-core/src/model/responsiveness_budget.rs`): the budget each
/// dispatch after a keystroke must fit for the typing derivation.
pub const B_INTERACT_US: u128 = 8_000;

/// What one keystroke cost: the longest dispatch it caused (the insert call
/// included) and their sum, µs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyCost {
    /// The longest dispatch after the key.
    pub worst: u128,
    /// All the dispatches after the key, summed.
    pub total: u128,
}

impl HighlightFixture {
    /// Load `text`, attach `language` and let the analysis finish (not
    /// measured); then type `keys` characters in the middle of the document,
    /// one at a time, returning each key's cost.
    #[must_use]
    pub fn mid_typing_costs(
        &self,
        language: &sourceview5::Language,
        text: &str,
        keys: u32,
    ) -> Vec<KeyCost> {
        let _ = self.classified_attach(language, text, true);
        let middle = self.view.buffer.char_count() / 2;
        (0..keys)
            .map(|_| {
                let key = self.classified_typing(Some(middle), 1);
                KeyCost {
                    worst: worst(&key),
                    total: key.iter().map(|dispatch| dispatch.micros).sum(),
                }
            })
            .collect()
    }

    /// Attach with every paragraph after the first `visible_lines` hidden by
    /// an `invisible` tag (removed afterwards), classifying the dispatches
    /// until idle.
    #[must_use]
    pub fn classified_attach_hidden(
        &self,
        language: &sourceview5::Language,
        text: &str,
        visible_lines: i32,
    ) -> Vec<ClassifiedDispatch> {
        let buffer = &self.view.buffer;
        buffer.set_language(None::<&sourceview5::Language>);
        buffer.set_highlight_syntax(true);
        buffer.set_text(text);
        let tag = crate::probes::text::invisible_tag(buffer.upcast_ref());
        let start = buffer
            .iter_at_line(visible_lines)
            .unwrap_or_else(|| buffer.end_iter());
        buffer.apply_tag(&tag, &start, &buffer.end_iter());
        let updates = HighlightUpdates::watch(buffer);
        let _ = self.classified_until_quiet(&updates, false);
        buffer.set_language(Some(language));
        let classified = self.classified_until_quiet(&updates, true);
        buffer.remove_all_tags(&buffer.start_iter(), &buffer.end_iter());
        buffer.tag_table().remove(&tag);
        classified
    }

    /// Attach `language`, and `offset_ms` later deliver a simulated keystroke
    /// from a `G_PRIORITY_DEFAULT` timeout (the priority of input events) at
    /// the document's middle. Returns the timeout's lateness (how long the
    /// key waited behind a dispatch), µs, and the longest dispatch after the
    /// key.
    #[must_use]
    pub fn keystroke_during_attach(
        &self,
        language: &sourceview5::Language,
        text: &str,
        offset_ms: u64,
    ) -> (u128, u128) {
        use std::time::Duration;
        let buffer = &self.view.buffer;
        buffer.set_language(None::<&sourceview5::Language>);
        buffer.set_highlight_syntax(true);
        buffer.set_text(text);
        let updates = HighlightUpdates::watch(buffer);
        let _ = self.classified_until_quiet(&updates, false);
        let lateness = std::rc::Rc::new(Cell::new(None::<u128>));
        buffer.set_language(Some(language));
        let due = Instant::now() + Duration::from_millis(offset_ms);
        {
            let (lateness, buffer) = (std::rc::Rc::clone(&lateness), buffer.clone());
            glib::timeout_add_local_once(Duration::from_millis(offset_ms), move || {
                lateness.set(Some(
                    Instant::now().saturating_duration_since(due).as_micros(),
                ));
                let middle = buffer.iter_at_offset(buffer.char_count() / 2);
                buffer.place_cursor(&middle);
                buffer.insert_interactive_at_cursor("x", true);
            });
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        while lateness.get().is_none() && Instant::now() < deadline {
            if crate::probes::post_open::timed_iteration(Some(&self.frames)).is_none() {
                std::thread::sleep(Duration::from_micros(200));
            }
        }
        let after = self.classified_until_quiet(&updates, true);
        (lateness.get().unwrap_or(u128::MAX), worst(&after))
    }
}

/// Bisect, like [`bisect_bytes`], the largest size at which every one of
/// `repetitions` keystrokes in the middle of an analysed document keeps every
/// dispatch it causes within [`B_INTERACT_US`].
#[must_use]
pub fn bisect_typing_bytes(
    fixture: &HighlightFixture,
    language_id: &str,
    repetitions: u32,
    tried: &mut Vec<(usize, Vec<KeyCost>)>,
) -> usize {
    let Some(language) = sourceview5::LanguageManager::default().language(language_id) else {
        return 0;
    };
    let mut fits = |bytes: usize| {
        let Some(text) = crate::probes::post_open::short_document(language_id, bytes, SEED) else {
            return false;
        };
        let costs = fixture.mid_typing_costs(&language, &text, repetitions);
        let passes = !costs.is_empty() && costs.iter().all(|cost| cost.worst <= B_INTERACT_US);
        tried.push((bytes, costs));
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

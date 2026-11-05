// SPDX-License-Identifier: MIT OR Apache-2.0

//! A85: after an **edit** in a GtkSourceView buffer whose language is
//! attached, the context engine's **first update** reports its re-analysis in
//! one `highlight-updated` emitted from the `G_PRIORITY_HIGH_IDLE` source it
//! installed during the edit; an idle queued at that same priority **before**
//! the edit (a `insert-text` / `delete-range` handler that runs before the
//! buffer's class handler) dispatches immediately before it, in the same
//! iteration, and an idle queued at **`G_PRIORITY_LOW - 1`** after the edit
//! dispatches after it and **before the first `G_PRIORITY_LOW` worker slice**
//! (A69). Removing the language from that idle removes the pending worker:
//! no `highlight-updated` follows.
//!
//! The re-analysis stops where the new analysis merges with the previous one
//! (`update_syntax`, `gtksourcecontextengine.c` l. 5480–5505), at the end of
//! the buffer, or when the 10 ms slice runs out, and only in the last case is
//! the worker installed. The first update's duration, timed from the idle
//! before it to its emission, therefore tells an edit whose re-analysis fits
//! the slice from one that needs the worker, before the worker runs.
//!
//! What it does not say, and the sample measures as magnitudes only: how far
//! a given edit re-analyses (a key that opens a comment with no closer after
//! it re-analyses the rest of the document; an ordinary key in some
//! languages re-analyses to the next blank line), and how long that takes on
//! a given machine.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gtk4::glib;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::post_open::MAX_ITERATIONS;
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// The priority of the decision idle: after the first update, GTK's
/// validation and the frame, before the engine's worker (A69).
#[must_use]
pub fn decision_priority() -> glib::Priority {
    glib::Priority::from(glib::ffi::G_PRIORITY_LOW - 1)
}

/// One `highlight-updated` emission.
#[derive(Clone, Copy, Debug)]
pub struct Emission {
    /// Priority of the source that was dispatching (`None`: none, the
    /// emission ran outside a dispatch).
    pub priority: Option<i32>,
    /// The reported range, character offsets.
    pub start: i32,
    /// The reported range's end.
    pub end: i32,
    /// When it was emitted.
    pub at: Instant,
}

/// Every `highlight-updated` of a buffer, with the dispatching priority.
#[derive(Clone, Debug, Default)]
pub struct EmissionLog {
    /// The emissions, in order.
    pub emissions: Rc<RefCell<Vec<Emission>>>,
}

impl EmissionLog {
    /// Watch `buffer`.
    #[must_use]
    pub fn watch(buffer: &sourceview5::Buffer) -> (Self, glib::SignalHandlerId) {
        let log = Self::default();
        let emissions = Rc::clone(&log.emissions);
        let id = buffer.connect_highlight_updated(move |_, start, end| {
            let priority = glib::main_current_source().map(|source| source.priority());
            emissions.borrow_mut().push(Emission {
                priority,
                start: start.offset(),
                end: end.offset(),
                at: Instant::now(),
            });
        });
        (log, id)
    }
}

/// What the decision idle saw: when it ran, whether it removed the language,
/// the first update's duration and reported span.
type Decision = (Instant, bool, u128, i32);

/// What one edit cost, observed from its edit to quiet.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EditCost {
    /// Whether the idle queued before the edit dispatched before the first
    /// update's emission.
    pub before_idle_first: bool,
    /// Whether the first emission after the edit came from a
    /// `G_PRIORITY_HIGH_IDLE` source.
    pub first_from_high_idle: bool,
    /// Whether the decision idle ran after the first update's emission.
    pub decision_after_first: bool,
    /// Microseconds from the idle before the edit to the first update's
    /// emission: the first update's duration.
    pub first_update_us: u128,
    /// Characters the first update reported re-analysed.
    pub first_update_span: i32,
    /// Emissions from a `G_PRIORITY_LOW` source: worker slices.
    pub worker_emissions: u32,
    /// Emissions after the decision idle.
    pub emissions_after_decision: u32,
    /// Whether the decision idle removed the language.
    pub downgraded: bool,
    /// The longest main-loop iteration from the edit until quiet, µs.
    pub worst_us: u128,
    /// Microseconds from the insert to the next painted frame (0 without a
    /// frame counter).
    pub echo_us: u128,
}

/// Insert `text` at `offset` of `buffer` (language attached, analysed), with
/// an idle at `G_PRIORITY_HIGH_IDLE` queued just before the insert and a
/// decision idle at `G_PRIORITY_LOW - 1` queued just after it; the decision
/// idle removes the language when `downgrade_over_us` is set and the first
/// update took longer. Drives the default context until 300 ms pass with
/// no emission and no iteration of 1 ms or more.
#[must_use]
pub fn observe_edit(
    buffer: &sourceview5::Buffer,
    offset: i32,
    text: &str,
    downgrade_over_us: Option<u128>,
) -> EditCost {
    observe_edit_framed(buffer, offset, text, downgrade_over_us, None)
}

/// [`observe_edit`] with `frames`, a painted-frame counter, for the echo.
#[must_use]
pub fn observe_edit_framed(
    buffer: &sourceview5::Buffer,
    offset: i32,
    text: &str,
    downgrade_over_us: Option<u128>,
    frames: Option<&dyn Fn() -> u32>,
) -> EditCost {
    use std::time::Duration;
    let (log, id) = EmissionLog::watch(buffer);
    let before_at: Rc<Cell<Option<Instant>>> = Rc::default();
    let decision: Rc<Cell<Option<Decision>>> = Rc::default();
    {
        let before_at = Rc::clone(&before_at);
        glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
            before_at.set(Some(Instant::now()));
            glib::ControlFlow::Break
        });
    }
    let mut iter = buffer.iter_at_offset(offset);
    let inserted_at = Instant::now();
    let frames_before = frames.map(|count| count());
    buffer.insert(&mut iter, text);
    let mut echo_us = 0u128;
    {
        let (buffer, log, before_at, decision) = (
            buffer.clone(),
            log.clone(),
            Rc::clone(&before_at),
            Rc::clone(&decision),
        );
        glib::idle_add_local_full(decision_priority(), move || {
            let now = Instant::now();
            let first = log
                .emissions
                .borrow()
                .iter()
                .find(|emission| emission.priority == Some(glib::ffi::G_PRIORITY_HIGH_IDLE))
                .copied();
            let (cost, span) = match (before_at.get(), first) {
                (Some(before), Some(first)) if first.at >= before => (
                    first.at.duration_since(before).as_micros(),
                    first.end - first.start,
                ),
                _ => (0, 0),
            };
            let remove = downgrade_over_us.is_some_and(|limit| cost > limit);
            if remove {
                buffer.set_language(None::<&sourceview5::Language>);
            }
            decision.set(Some((now, remove, cost, span)));
            glib::ControlFlow::Break
        });
    }
    let context = glib::MainContext::default();
    let quiet = Duration::from_millis(300);
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut last_activity = Instant::now();
    let mut worst = 0u128;
    let mut iterations = 0usize;
    while Instant::now() < deadline
        && iterations < MAX_ITERATIONS
        && (decision.get().is_none() || last_activity.elapsed() < quiet)
    {
        let emitted = log.emissions.borrow().len();
        let started = Instant::now();
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_micros(200));
            continue;
        }
        iterations += 1;
        let micros = started.elapsed().as_micros();
        worst = worst.max(micros);
        if micros >= 1_000 || log.emissions.borrow().len() != emitted {
            last_activity = Instant::now();
        }
        if echo_us == 0
            && let (Some(count), Some(before)) = (frames, frames_before)
            && count() != before
        {
            echo_us = inserted_at.elapsed().as_micros();
        }
    }
    buffer.disconnect(id);
    let emissions = log.emissions.borrow();
    let before = before_at.get();
    let first = emissions.first().copied();
    let (decided_at, downgraded, first_update_us, first_update_span) = decision
        .get()
        .unwrap_or_else(|| (Instant::now(), false, 0, 0));
    EditCost {
        before_idle_first: matches!((before, first), (Some(b), Some(f)) if b <= f.at),
        first_from_high_idle: first
            .is_some_and(|f| f.priority == Some(glib::ffi::G_PRIORITY_HIGH_IDLE)),
        decision_after_first: first.is_some_and(|f| f.at <= decided_at),
        first_update_us,
        first_update_span,
        worker_emissions: u32::try_from(
            emissions
                .iter()
                .filter(|e| e.priority == Some(glib::ffi::G_PRIORITY_LOW))
                .count(),
        )
        .unwrap_or(u32::MAX),
        emissions_after_decision: u32::try_from(
            emissions.iter().filter(|e| e.at > decided_at).count(),
        )
        .unwrap_or(u32::MAX),
        downgraded,
        worst_us: worst,
        echo_us,
    }
}

/// `bytes` of short C lines with `//` comments only: no `*/` anywhere, so a
/// `/*` typed at the start turns everything after it into a comment.
#[must_use]
pub fn c_without_closers(bytes: usize) -> String {
    let mut text = String::with_capacity(bytes + 128);
    let mut n = 0u32;
    while text.len() < bytes {
        n += 1;
        text.push_str(&format!(
            "static int value_{n}(int x) {{ return x + {}; }} // note {n}\n",
            n % 977
        ));
    }
    text
}

/// A highlighted buffer with no view holding `text` with `language_id`
/// attached and analysed. `None` without that language.
#[must_use]
pub fn analysed_buffer(language_id: &str, text: &str) -> Option<sourceview5::Buffer> {
    let language = sourceview5::LanguageManager::default().language(language_id)?;
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_highlight_syntax(true);
    buffer.set_text(text);
    buffer.set_language(Some(&language));
    // Drain the attach's analysis (worker included).
    let done: Rc<Cell<bool>> = Rc::default();
    {
        let done = Rc::clone(&done);
        glib::idle_add_local_full(
            glib::Priority::from(glib::ffi::G_PRIORITY_LOW + 1),
            move || {
                done.set(true);
                glib::ControlFlow::Break
            },
        );
    }
    let context = glib::MainContext::default();
    for _ in 0..MAX_ITERATIONS {
        if done.get() {
            break;
        }
        context.iteration(true);
    }
    flush_events();
    Some(buffer)
}

/// The priority of a paced re-analysis step: one above the first update's
/// `G_PRIORITY_HIGH_IDLE`, so every step dispatches before it.
#[must_use]
pub fn step_priority() -> glib::Priority {
    glib::Priority::from(glib::ffi::G_PRIORITY_HIGH_IDLE - 1)
}

/// What one paced re-analysis of an edit did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PacedEdit {
    /// Steps run (`ensure_highlight` calls).
    pub steps: u32,
    /// The longest step, µs.
    pub step_max_us: u128,
    /// Microseconds spent in steps.
    pub spent_us: u128,
    /// Characters the steps reported re-analysed.
    pub analysed: i32,
    /// Whether a step's report ended before its bound: the analysis merged.
    pub merged: bool,
    /// Whether the budget ran out and the language was removed.
    pub downgraded: bool,
    /// Emissions from the first update's `G_PRIORITY_HIGH_IDLE` source.
    pub first_update_emissions: u32,
    /// Emissions from a `G_PRIORITY_LOW` source: worker slices.
    pub worker_emissions: u32,
    /// The longest main-loop iteration from the edit until quiet, µs.
    pub worst_us: u128,
    /// Microseconds from the insert to the next painted frame (0 without a
    /// frame counter).
    pub echo_us: u128,
}

/// The pacing rule the sample and probe drive: `ensure_highlight` on an
/// empty range at the end bound analyses from the invalid point to the line
/// after the bound and tags nothing; the step size adapts towards
/// `target_us`; once `budget_us` is spent without the analysis merging, the
/// language is removed.
#[derive(Clone, Copy, Debug)]
pub struct Pacing {
    /// Target duration of one step, µs.
    pub target_us: u128,
    /// The re-analysis budget of one edit, µs.
    pub budget_us: u128,
    /// The first step's size, characters.
    pub first_step_chars: i32,
}

impl Pacing {
    /// The rule LushText's measurements started from.
    pub const DEFAULT: Self = Self {
        target_us: 2_000,
        budget_us: 10_000,
        first_step_chars: 256,
    };
}

/// The bound one step analyses to: `from + chars`, moved to the start of the
/// next line (`update_syntax` analyses whole lines), clamped to the end.
fn step_bound(buffer: &sourceview5::Buffer, from: i32, chars: i32) -> gtk4::TextIter {
    let mut iter = buffer.iter_at_offset(from.saturating_add(chars.max(1)));
    if !iter.starts_line() {
        iter.forward_line();
    }
    iter
}

/// Insert `text` at `offset` and re-analyse the edit in paced steps from an
/// idle at [`step_priority`], driving the default context until quiet.
#[must_use]
pub fn paced_edit(
    buffer: &sourceview5::Buffer,
    offset: i32,
    text: &str,
    pacing: Pacing,
    frames: Option<&dyn Fn() -> u32>,
) -> PacedEdit {
    use std::time::Duration;
    let (log, id) = EmissionLog::watch(buffer);
    let result: Rc<RefCell<PacedEdit>> = Rc::default();
    let finished: Rc<Cell<bool>> = Rc::default();
    let mut iter = buffer.iter_at_offset(offset);
    let line_start = {
        let mut start = iter;
        start.set_line_offset(0);
        start.offset()
    };
    let inserted_at = Instant::now();
    let frames_before = frames.map(|count| count());
    buffer.insert(&mut iter, text);
    {
        let (buffer, log, result, finished) = (
            buffer.clone(),
            log.clone(),
            Rc::clone(&result),
            Rc::clone(&finished),
        );
        let from = Cell::new(line_start);
        let chars = Cell::new(pacing.first_step_chars);
        glib::idle_add_local_full(step_priority(), move || {
            let bound = step_bound(&buffer, from.get(), chars.get());
            let before = log.emissions.borrow().len();
            let started = Instant::now();
            buffer.ensure_highlight(&bound, &bound);
            let took = started.elapsed().as_micros();
            let report = log.emissions.borrow().get(before).copied();
            let mut paced = result.borrow_mut();
            paced.steps += 1;
            if std::env::var_os("A85_STEPS").is_some() {
                println!(
                    "    step {} chars {} took {took} us report {:?}",
                    paced.steps,
                    chars.get(),
                    report.map(|r| (r.start, r.end, bound.offset()))
                );
            }
            paced.step_max_us = paced.step_max_us.max(took);
            paced.spent_us += took;
            let Some(report) = report else {
                paced.merged = true;
                finished.set(true);
                return glib::ControlFlow::Break;
            };
            paced.analysed += report.end - report.start;
            let at_end = bound.is_end();
            if report.end + 1 < bound.offset() || at_end {
                paced.merged = true;
                finished.set(true);
                return glib::ControlFlow::Break;
            }
            if paced.spent_us > pacing.budget_us {
                buffer.set_language(None::<&sourceview5::Language>);
                paced.downgraded = true;
                finished.set(true);
                return glib::ControlFlow::Break;
            }
            from.set(report.end);
            let analysed = (report.end - report.start).max(1);
            let scaled = i128::from(analysed) * i128::try_from(pacing.target_us).unwrap_or(0)
                / i128::try_from(took.max(1)).unwrap_or(1);
            let grown = i128::from(chars.get()) * 2;
            chars.set(i32::try_from(scaled.clamp(128, grown.max(128))).unwrap_or(1 << 20));
            glib::ControlFlow::Continue
        });
    }
    let context = glib::MainContext::default();
    let quiet = Duration::from_millis(300);
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut last_activity = Instant::now();
    let mut worst = 0u128;
    let mut echo_us = 0u128;
    let mut iterations = 0usize;
    while Instant::now() < deadline
        && iterations < MAX_ITERATIONS
        && (!finished.get() || last_activity.elapsed() < quiet)
    {
        let emitted = log.emissions.borrow().len();
        let started = Instant::now();
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_micros(200));
            continue;
        }
        iterations += 1;
        let micros = started.elapsed().as_micros();
        worst = worst.max(micros);
        if micros >= 1_000 || log.emissions.borrow().len() != emitted {
            last_activity = Instant::now();
        }
        if echo_us == 0
            && let (Some(count), Some(before)) = (frames, frames_before)
            && count() != before
        {
            echo_us = inserted_at.elapsed().as_micros();
        }
    }
    buffer.disconnect(id);
    let emissions = log.emissions.borrow();
    let mut paced = *result.borrow();
    paced.first_update_emissions = u32::try_from(
        emissions
            .iter()
            .filter(|e| e.priority == Some(glib::ffi::G_PRIORITY_HIGH_IDLE))
            .count(),
    )
    .unwrap_or(u32::MAX);
    paced.worker_emissions = u32::try_from(
        emissions
            .iter()
            .filter(|e| e.priority == Some(glib::ffi::G_PRIORITY_LOW))
            .count(),
    )
    .unwrap_or(u32::MAX);
    paced.worst_us = worst;
    paced.echo_us = echo_us;
    paced
}

/// Bytes of the small document of the probe.
pub const SMALL_BYTES: usize = 4 * 1024;
/// Bytes of the document whose flip needs the worker.
pub const LARGE_BYTES: usize = 512 * 1024;

/// Probe A85.
#[must_use]
pub fn probe_a85() -> Observation {
    Recorder::run(AxiomId::new(85), |recorder| {
        let small = recorder.require(
            analysed_buffer("c", &c_without_closers(SMALL_BYTES)),
            "control: a c language is installed",
        )?;
        let middle = small.char_count() / 2;
        let key = observe_edit(&small, middle, "x", None);
        recorder.measure("small_key_first_update_us", key.first_update_us);
        recorder.measure("small_key_span", key.first_update_span);
        recorder.measure("small_key_worker_emissions", key.worker_emissions);
        recorder.axiom(
            key.before_idle_first && key.first_from_high_idle && key.decision_after_first,
            "an idle at G_PRIORITY_HIGH_IDLE queued before an edit runs before the first \
             update, whose highlight-updated comes from a G_PRIORITY_HIGH_IDLE source, and a \
             G_PRIORITY_LOW - 1 idle queued after the edit runs after it",
        )?;
        recorder.axiom(
            key.worker_emissions == 0 && key.first_update_span > 0,
            "an ordinary key in a small document is re-analysed by the first update alone",
        )?;
        drop(small);

        let text = c_without_closers(LARGE_BYTES);
        let control = recorder.require(
            analysed_buffer("c", &text),
            "control: the large document was analysed",
        )?;
        let flip = observe_edit(&control, 0, "/*", None);
        recorder.measure("flip_first_update_us", flip.first_update_us);
        recorder.measure("flip_span", flip.first_update_span);
        recorder.measure("flip_worker_emissions", flip.worker_emissions);
        recorder.measure("flip_worst_us", flip.worst_us);
        recorder.control(
            flip.worker_emissions > 0,
            "control: a /* typed at the start of 512 KiB of c with no closer needs the worker",
        )?;
        drop(control);
        flush_events();

        let downgraded = recorder.require(
            analysed_buffer("c", &text),
            "control: the large document was analysed again",
        )?;
        let cut = observe_edit(&downgraded, 0, "/*", Some(0));
        recorder.measure("downgraded_first_update_us", cut.first_update_us);
        recorder.measure("downgraded_worst_us", cut.worst_us);
        recorder.axiom(
            cut.downgraded
                && cut.worker_emissions == 0
                && cut.emissions_after_decision == 0
                && downgraded.language().is_none(),
            "removing the language from that G_PRIORITY_LOW - 1 idle removes the pending \
             worker: no highlight-updated follows",
        )?;
        let mut expected = String::from("/*");
        expected.push_str(&text);
        recorder.axiom(
            downgraded
                .text(&downgraded.start_iter(), &downgraded.end_iter(), true)
                .as_str()
                == expected,
            "and changes no text",
        )?;
        drop(downgraded);
        flush_events();

        // Paced: `ensure_highlight` on an empty range at a bound, from an
        // idle above the first update.
        let small = recorder.require(
            analysed_buffer("c", &c_without_closers(SMALL_BYTES)),
            "control: the small document was analysed again",
        )?;
        let middle = small.char_count() / 2;
        let paced = paced_edit(&small, middle, "x", Pacing::DEFAULT, None);
        recorder.measure("paced_small_steps", paced.steps);
        recorder.measure("paced_small_step_max_us", paced.step_max_us);
        recorder.axiom(
            paced.merged
                && paced.first_update_emissions == 0
                && paced.worker_emissions == 0
                && paced.analysed > 0,
            "ensure_highlight on an empty range at a bound, from a G_PRIORITY_HIGH_IDLE - 1 \
             idle, re-analyses the edit and reports it; once the report ends before the \
             bound, the first update reports nothing and no worker runs",
        )?;
        drop(small);

        let whole = recorder.require(
            analysed_buffer("c", &text),
            "control: the large document was analysed a third time",
        )?;
        let unbounded = Pacing {
            budget_us: u128::MAX,
            ..Pacing::DEFAULT
        };
        let walked = paced_edit(&whole, 0, "/*", unbounded, None);
        recorder.measure("paced_flip_steps", walked.steps);
        recorder.measure("paced_flip_step_max_us", walked.step_max_us);
        recorder.measure("paced_flip_spent_us", walked.spent_us);
        recorder.control(
            walked.steps > 1,
            "control: the flip's re-analysis took more than one step",
        )?;
        recorder.axiom(
            walked.merged && walked.first_update_emissions == 0 && walked.worker_emissions == 0,
            "steps whose reports reach their bound walk the analysis forward to the end, \
             and the first update and the worker then find nothing",
        )?;
        drop(whole);
        flush_events();

        let cut = recorder.require(
            analysed_buffer("c", &text),
            "control: the large document was analysed a fourth time",
        )?;
        let no_allowance = Pacing {
            budget_us: 0,
            ..Pacing::DEFAULT
        };
        let stopped = paced_edit(&cut, 0, "/*", no_allowance, None);
        recorder.axiom(
            stopped.downgraded
                && stopped.first_update_emissions == 0
                && stopped.worker_emissions == 0
                && cut.language().is_none(),
            "removing the language between steps, before the first update ran, removes \
             the first update and the worker",
        )?;
        drop(cut);
        flush_events();
        Ok(())
    })
}

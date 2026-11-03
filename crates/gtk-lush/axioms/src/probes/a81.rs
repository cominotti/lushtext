// SPDX-License-Identifier: MIT OR Apache-2.0

//! A81: GLib's main loop **co-dispatches every ready source of the top
//! priority in one iteration** (`g_main_context_check` collects all ready
//! sources whose priority equals the highest ready priority, and
//! `g_main_context_dispatch` runs them back to back before the context polls
//! again). So the main-loop stall of one iteration is the **sum** of the
//! dispatches of the equal-priority sources that were ready together, while a
//! ready source of a numerically **higher** (less urgent) priority waits for a
//! later iteration. A source of the same priority that becomes ready during
//! that iteration's dispatch (added by one of the callbacks) is not part of
//! it: it runs in the next iteration.
//!
//! This is the general form of what A69 records for the context engine's
//! worker and a `G_PRIORITY_LOW` idle, and of what `spawn-completion-priority`
//! met when the first session-restore turn shared one
//! `G_PRIORITY_DEFAULT_IDLE` iteration with a composition stage: several
//! validating text views (A80), each with its own incremental validation idle
//! at the same priority, lay out one batch **each** per iteration.
//!
//! Recorded magnitudes, never verdicts: which callbacks each iteration ran
//! and how long the iteration took.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;

use crate::observation::Recorder;
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// The priority the co-dispatched sources share.
pub const SHARED_PRIORITY: i32 = glib::ffi::G_PRIORITY_DEFAULT_IDLE;
/// Equal-priority sources armed together.
pub const SOURCES: usize = 3;
/// The labels of the co-dispatched sources.
pub const LABELS: [&str; SOURCES] = ["s0", "s1", "s2"];
/// How long each co-dispatched callback keeps the main thread busy.
pub const BUSY: Duration = Duration::from_millis(4);

/// What one iteration of the default context ran.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Iteration {
    /// The callbacks that dispatched in it, by label, in order.
    pub ran: Vec<&'static str>,
    /// Microseconds the iteration took.
    pub micros: u128,
}

/// The iterations one arming took, until everything armed had run.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CoDispatchTrace {
    /// Each iteration that dispatched anything, in order.
    pub iterations: Vec<Iteration>,
}

impl CoDispatchTrace {
    /// The index of the iteration that ran `label`.
    #[must_use]
    pub fn iteration_of(&self, label: &str) -> Option<usize> {
        self.iterations
            .iter()
            .position(|iteration| iteration.ran.contains(&label))
    }
}

fn busy() {
    let started = Instant::now();
    while started.elapsed() < BUSY {
        std::hint::spin_loop();
    }
}

/// Arm `SOURCES` busy one-shot idles at `SHARED_PRIORITY` (labels `s0`,
/// `s1`, …; the first also arms `late`, one more at the same priority, from
/// its callback) and one at `SHARED_PRIORITY + 1` (`lower`), then iterate the
/// default context one iteration at a time until all ran.
#[must_use]
pub fn trace_co_dispatch() -> CoDispatchTrace {
    flush_events();
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    for (index, label) in LABELS.into_iter().enumerate() {
        let log = Rc::clone(&log);
        glib::idle_add_local_full(glib::Priority::from(SHARED_PRIORITY), move || {
            busy();
            log.borrow_mut().push(label);
            if index == 0 {
                let log = Rc::clone(&log);
                glib::idle_add_local_full(glib::Priority::from(SHARED_PRIORITY), move || {
                    log.borrow_mut().push("late");
                    glib::ControlFlow::Break
                });
            }
            glib::ControlFlow::Break
        });
    }
    {
        let log = Rc::clone(&log);
        glib::idle_add_local_full(glib::Priority::from(SHARED_PRIORITY + 1), move || {
            log.borrow_mut().push("lower");
            glib::ControlFlow::Break
        });
    }
    let context = glib::MainContext::default();
    let mut trace = CoDispatchTrace::default();
    for _ in 0..64 {
        let started = Instant::now();
        context.iteration(false);
        let micros = started.elapsed().as_micros();
        let ran = std::mem::take(&mut *log.borrow_mut());
        if !ran.is_empty() {
            let done = ran.contains(&"lower") || trace.iteration_of("lower").is_some();
            trace.iterations.push(Iteration { ran, micros });
            if done && trace.iteration_of("late").is_some() {
                break;
            }
        }
    }
    trace
}

/// Probe A81.
#[must_use]
pub fn probe_a81() -> Observation {
    Recorder::run(AxiomId::new(81), |recorder| {
        let trace = trace_co_dispatch();
        for (index, iteration) in trace.iterations.iter().enumerate() {
            recorder.measure(
                "iteration",
                format!(
                    "{index}: ran {:?} in {} us",
                    iteration.ran, iteration.micros
                ),
            );
        }
        let first = trace.iteration_of("s0");
        recorder.control(first.is_some(), "control: the armed sources dispatched")?;
        let same = LABELS
            .iter()
            .all(|label| trace.iteration_of(label) == first);
        recorder.axiom(
            same,
            "every ready source of the top priority dispatches in one main-loop iteration",
        )?;
        let busy_micros = u128::from(u32::try_from(SOURCES).unwrap_or(u32::MAX)) * BUSY.as_micros();
        recorder.axiom(
            first.is_some_and(|index| trace.iterations[index].micros >= busy_micros),
            "that iteration lasts at least the sum of the co-dispatched callbacks",
        )?;
        recorder.axiom(
            trace
                .iteration_of("lower")
                .zip(first)
                .is_some_and(|(lower, first)| lower > first),
            "a ready source one step lower in priority waits for a later iteration",
        )?;
        recorder.axiom(
            trace
                .iteration_of("late")
                .zip(first)
                .is_some_and(|(late, first)| late > first),
            "a source of the same priority added during the dispatch runs in a later iteration",
        )?;
        flush_events();
        Ok(())
    })
}

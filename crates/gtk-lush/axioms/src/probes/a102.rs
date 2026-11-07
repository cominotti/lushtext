// SPDX-License-Identifier: MIT OR Apache-2.0

//! A102: `GtkSourceSearchContext::forward_async` on a context that has not
//! scanned **never blocks a dispatch long**: it scans the unscanned text in
//! batches across main-loop turns and calls back once it reaches the match.
//! A walk to a match 262,143 lines away keeps every dispatch short (where the
//! synchronous `forward` spends one 70-80 ms dispatch, A100); one started a
//! hundred lines before the match calls back within a few turns.
//!
//! **But a pending request does not survive a query change**: when the
//! search text changes while it scans, GtkSourceView drops the request
//! without ever calling back, and GIO logs `GTask ... finalized without ever
//! returning` as a CRITICAL once the task is freed.
//!
//! Recorded beside it, as values only: the wall time to the callback from the
//! buffer start and from a hundred lines before the match, and the context's
//! own background scan (the time until `occurrences-count` reads a count when
//! nothing else runs).
//!
//! LushText's minimap therefore does **not** use it for interim search
//! markers, which a keystroke in the search entry can supersede at any time
//! (`bin-minimap-markers-per-row` D5); it searches a private copy of the
//! band's text instead.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::a100::{SPARSE_LINES, needle_search, sparse_buffer};
use crate::probes::text::{LogCapture, record_text_toolkit};
use crate::{AxiomId, Observation};

/// The longest dispatch the probe accepts during an asynchronous walk:
/// `B_INTERACT_US` (8 ms), LushText's interaction budget.
pub const DISPATCH_BUDGET: Duration = Duration::from_millis(8);
/// How long the probe waits for one walk.
const WALK_BUDGET: Duration = Duration::from_secs(60);

/// One asynchronous walk: the line it found, its wall time, and its longest
/// dispatch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AsyncWalk {
    /// The match line, `None` when nothing was found or the walk timed out.
    pub line: Option<i32>,
    /// Microseconds from the call to the callback.
    pub elapsed_us: u128,
    /// The longest main-loop dispatch while it ran, in microseconds.
    pub worst_dispatch_us: u128,
}

/// Start `forward_async` on `context` from `from_line` and drive the main
/// loop until it calls back, timing every dispatch.
#[must_use]
pub fn walk_async(context: &sourceview5::SearchContext, from_line: i32) -> AsyncWalk {
    let buffer = context.buffer();
    let from = buffer
        .iter_at_line(from_line)
        .unwrap_or_else(|| buffer.start_iter());
    let found: Rc<Cell<Option<Option<i32>>>> = Rc::new(Cell::new(None));
    let start = Instant::now();
    {
        let found = found.clone();
        context.forward_async(&from, None::<&gtk4::gio::Cancellable>, move |result| {
            found.set(Some(result.ok().map(|(start, _, _)| start.line())));
        });
    }
    let mut worst = Duration::ZERO;
    let main = gtk4::glib::MainContext::default();
    while found.get().is_none() && start.elapsed() < WALK_BUDGET {
        let turn = Instant::now();
        if main.iteration(false) {
            worst = worst.max(turn.elapsed());
        } else {
            std::thread::sleep(Duration::from_micros(200));
        }
    }
    AsyncWalk {
        line: found.get().flatten(),
        elapsed_us: start.elapsed().as_micros(),
        worst_dispatch_us: worst.as_micros(),
    }
}

/// Microseconds until a fresh context's own scan reads a count, driving the
/// main loop and nothing else.
fn background_scan_us(context: &sourceview5::SearchContext) -> u128 {
    let start = Instant::now();
    let main = gtk4::glib::MainContext::default();
    while context.occurrences_count() < 0 && start.elapsed() < WALK_BUDGET {
        if !main.iteration(false) {
            std::thread::sleep(Duration::from_micros(200));
        }
    }
    start.elapsed().as_micros()
}

/// Start a far `forward_async`, change the query before it calls back, free
/// the context, and report whether the callback ran and the CRITICALs GIO
/// logged.
fn superseded_request() -> (bool, usize) {
    let capture = LogCapture::start();
    let called = Rc::new(Cell::new(false));
    {
        let buffer = sparse_buffer(SPARSE_LINES);
        let context = needle_search(&buffer);
        {
            let called = called.clone();
            context.forward_async(
                &buffer.start_iter(),
                None::<&gtk4::gio::Cancellable>,
                move |_| {
                    called.set(true);
                },
            );
        }
        let main = gtk4::glib::MainContext::default();
        main.iteration(false);
        context.settings().set_search_text(Some("other"));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !called.get() && Instant::now() < deadline {
            if !main.iteration(false) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    let main = gtk4::glib::MainContext::default();
    while main.iteration(false) {}
    let criticals = capture
        .messages()
        .iter()
        .filter(|message| message.level == "critical")
        .count();
    (called.get(), criticals)
}

/// Probe A102.
#[must_use]
pub fn probe_a102() -> Observation {
    Recorder::run(AxiomId::new(102), |recorder| {
        record_text_toolkit(recorder);
        let last = i32::try_from(SPARSE_LINES - 1).unwrap_or(i32::MAX);

        let far_buffer = sparse_buffer(SPARSE_LINES);
        let far = walk_async(&needle_search(&far_buffer), 0);
        recorder.measure("far_walk_line", format!("{:?}", far.line));
        recorder.measure("far_walk_us", far.elapsed_us);
        recorder.measure("far_walk_worst_dispatch_us", far.worst_dispatch_us);

        let near_buffer = sparse_buffer(SPARSE_LINES);
        let near = walk_async(&needle_search(&near_buffer), last - 100);
        recorder.measure("near_walk_line", format!("{:?}", near.line));
        recorder.measure("near_walk_us", near.elapsed_us);
        recorder.measure("near_walk_worst_dispatch_us", near.worst_dispatch_us);

        let scan_buffer = sparse_buffer(SPARSE_LINES);
        let scan_us = background_scan_us(&needle_search(&scan_buffer));
        recorder.measure("background_scan_us", scan_us);

        let (superseded_called_back, superseded_criticals) = superseded_request();
        recorder.measure("superseded_request_called_back", superseded_called_back);
        recorder.measure("superseded_request_criticals", superseded_criticals);

        recorder.control(
            far.line == Some(last) && near.line == Some(last),
            "control: both walks found the match on the last line",
        )?;
        recorder.axiom(
            far.worst_dispatch_us < DISPATCH_BUDGET.as_micros()
                && near.worst_dispatch_us < DISPATCH_BUDGET.as_micros(),
            "forward_async never blocks a dispatch past 8 ms on an unscanned document",
        )?;
        recorder.axiom(
            near.elapsed_us < far.elapsed_us,
            "a walk that starts near the match calls back sooner than one from the start",
        )?;
        recorder.axiom(
            !superseded_called_back && superseded_criticals >= 1,
            "a pending forward_async superseded by a query change never calls back, and GIO logs a CRITICAL",
        )
    })
}

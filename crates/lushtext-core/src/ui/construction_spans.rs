// SPDX-License-Identifier: GPL-3.0-or-later

//! Startup and window-construction span recording.
//!
//! **Cross-cutting instrumentation with no workflow role and no `WFR-*` row**,
//! like `ui::responsiveness_probe`. It reads `g_get_monotonic_time()` around
//! application startup and each window's construction and keeps the result as
//! plain data (`model::construction_spans`): process-wide for startup, on each
//! window for its construction. The shell-geometry evidence
//! (`ui::window::geometry::evidence`) and the automation snapshot project it;
//! the measurement build of the responsiveness lane also writes every span to
//! its evidence stream (`responsiveness_probe::record_span`, an empty function
//! in shipped builds).
//!
//! The construction span of a window runs from the start of
//! `LushtextWindow::new` until control returns to the main loop, observed by a
//! `G_PRIORITY_HIGH` idle queued at the start of construction: GLib dispatches
//! it on the first iteration after the dispatch that built the window, before
//! any default-priority work. The first-frame span ends at the first
//! `after-paint` of the window's frame clock.

use std::cell::Cell;

use crate::model::construction_spans::{StartupSpans, span_duration_us};
use crate::ui::responsiveness_probe;

thread_local! {
    static STARTUP: Cell<StartupSpans> = const { Cell::new(StartupSpans {
        toolkit_us: None,
        lushtext_us: None,
    }) };
}

/// The monotonic clock every span uses, in microseconds.
#[must_use]
pub(crate) fn now_us() -> i64 {
    glib::monotonic_time()
}

/// Record application startup: `parent_startup` ran from `toolkit_start_us`
/// to `toolkit_end_us`, and LushText's own startup work from there until
/// `lushtext_end_us`.
pub(crate) fn record_startup(toolkit_start_us: i64, toolkit_end_us: i64, lushtext_end_us: i64) {
    STARTUP.with(|startup| {
        startup.set(StartupSpans {
            toolkit_us: Some(span_duration_us(toolkit_start_us, toolkit_end_us)),
            lushtext_us: Some(span_duration_us(toolkit_end_us, lushtext_end_us)),
        });
    });
    responsiveness_probe::record_span("startup_toolkit", toolkit_start_us, toolkit_end_us);
    responsiveness_probe::record_span("startup_lushtext", toolkit_end_us, lushtext_end_us);
}

/// The process's startup spans; both are `None` until startup ran.
#[must_use]
pub(crate) fn startup_spans() -> StartupSpans {
    STARTUP.with(Cell::get)
}

/// Forward one span to the measurement build's evidence stream.
pub(crate) fn emit(name: &'static str, start_us: i64, end_us: i64) {
    responsiveness_probe::record_span(name, start_us, end_us);
}

/// The largest main-loop dispatch measured so far, when a watchdog runs.
#[must_use]
pub(crate) fn largest_dispatch_so_far_us() -> Option<u64> {
    responsiveness_probe::largest_dispatch_so_far_us()
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: test policy — this workflow's `test-utils` timing overrides.
//!
//! Presented widget tests do not reliably advance `AdwTimedAnimation` frame
//! clocks (`widget-wiring.md`, "Timed animations"), so a test can make a
//! scroll step land at once, and can lengthen the hold-to-repeat delay so a
//! hold stays pending while it drives a cancellation. These are
//! **configuration** seams: they change timing and report nothing, and the
//! module compiles only under `test-utils`, so no override storage exists in
//! a production build.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static IMMEDIATE_STEPS: AtomicBool = AtomicBool::new(false);
static REPEAT_DELAY_MS: AtomicU32 = AtomicU32::new(0);
static REPEAT_INTERVAL_MS: AtomicU32 = AtomicU32::new(0);

/// Timing a widget test asks the tab-strip scroll controls to use.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TabOverflowTestTiming {
    /// Land each scroll step at once instead of animating it.
    pub immediate_steps: bool,
    /// The hold-to-repeat delay in milliseconds; 0 keeps the production one.
    pub repeat_delay_ms: u32,
    /// The repeat interval in milliseconds; 0 keeps the production one.
    pub repeat_interval_ms: u32,
}

/// Apply `timing` to every later step and hold in this process.
pub fn set_tab_overflow_test_timing(timing: TabOverflowTestTiming) {
    IMMEDIATE_STEPS.store(timing.immediate_steps, Ordering::Release);
    REPEAT_DELAY_MS.store(timing.repeat_delay_ms, Ordering::Release);
    REPEAT_INTERVAL_MS.store(timing.repeat_interval_ms, Ordering::Release);
}

/// Whether a step lands at once.
pub(super) fn immediate_steps() -> bool {
    IMMEDIATE_STEPS.load(Ordering::Acquire)
}

/// The overriding repeat delay, if a test set one.
pub(super) fn repeat_delay_ms() -> Option<u32> {
    let delay = REPEAT_DELAY_MS.load(Ordering::Acquire);
    (delay > 0).then_some(delay)
}

/// The overriding repeat interval, if a test set one.
pub(super) fn repeat_interval_ms() -> Option<u32> {
    let interval = REPEAT_INTERVAL_MS.load(Ordering::Acquire);
    (interval > 0).then_some(interval)
}

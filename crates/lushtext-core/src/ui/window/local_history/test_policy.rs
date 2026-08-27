// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "test-utils")]

//! The local-history workflow's single test-only timing and limit value.
//!
//! One value rather than several independent module-level statics, and the whole
//! module is behind `#[cfg(feature = "test-utils")]`, so **a production build
//! compiles no override storage at all**. The public setter names are unchanged,
//! because they are what widget tests call.
//!
//! The one override that deliberately stays elsewhere is the preview *read*
//! delay: it belongs to `services/local_history_service.rs`, which owns the
//! behavior it changes. The `editor_io` precedent settles that — an override
//! lives with the code it overrides, not with the workflow that observes it.

use std::sync::atomic::{AtomicU64, Ordering};

/// Baseline attempts to fail before production persistence runs.
static BASELINE_FAILURES: AtomicU64 = AtomicU64::new(0);
/// Worker delay applied to baseline persistence, in milliseconds.
static BASELINE_DELAY_MS: AtomicU64 = AtomicU64::new(0);
/// Delay between scheduled preview install slices, in milliseconds.
static PREVIEW_INSTALL_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Fail the next `count` baseline attempts before production persistence runs.
pub fn set_local_history_baseline_failures_for_test(count: u64) {
    BASELINE_FAILURES.store(count, Ordering::Release);
}

/// Delay baseline persistence for deterministic ownership-generation tests.
pub fn set_local_history_baseline_delay_for_test(delay_ms: u64) {
    BASELINE_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Delay successive preview slices without blocking GTK for deterministic tests.
pub fn set_local_history_preview_install_delay_for_test(delay_ms: u64) {
    PREVIEW_INSTALL_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// The configured baseline persistence delay.
#[must_use]
pub(crate) fn baseline_delay_ms() -> u64 {
    BASELINE_DELAY_MS.load(Ordering::Acquire)
}

/// Consume one configured baseline failure, if any remain.
#[must_use]
pub(crate) fn take_baseline_failure() -> bool {
    BASELINE_FAILURES
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
            remaining.checked_sub(1)
        })
        .is_ok()
}

/// The configured preview install slice delay.
#[must_use]
pub(super) fn preview_install_delay_ms() -> u64 {
    PREVIEW_INSTALL_DELAY_MS.load(Ordering::Acquire)
}

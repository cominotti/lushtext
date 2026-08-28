// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: test policy — this workflow's single `test-utils` override value.
//!
//! One knob: an artificial delay inside the lossy-encoding analysis worker, so a
//! widget test can hold the analysis open and prove that a stale completion is
//! refused. It is a **configuration** seam, not an inspection one — it changes
//! timing rather than reporting state — and the module compiles only under
//! `test-utils`, so no override storage exists in a production build.

use std::sync::atomic::{AtomicU64, Ordering};

static LOSSY_ENCODING_ANALYSIS_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial lossy-encoding analysis delay for window tests.
pub fn set_lossy_encoding_analysis_delay_for_test(delay_ms: u64) {
    LOSSY_ENCODING_ANALYSIS_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Sleep the configured analysis delay. Runs on the analysis worker, never GTK.
pub(super) fn delay_lossy_encoding_analysis() {
    let delay_ms = LOSSY_ENCODING_ANALYSIS_DELAY_MS.load(Ordering::Acquire);
    if delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
}

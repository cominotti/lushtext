// SPDX-License-Identifier: GPL-3.0-or-later

//! The notes and bookmarks workflow's single test-policy value.
//!
//! Everything a test may override about this workflow lives in one place, and the
//! whole module is behind `#[cfg(feature = "test-utils")]` so a production build
//! compiles no override storage at all. Adding a second module-level static for
//! the next overridable knob is the regression this module exists to prevent —
//! before this consolidation the two knobs below lived as separate statics in
//! `notes/mod.rs` and `notes/bookmarks.rs`.
//!
//! **What is deliberately not here.** `services/palette/notes.rs` owns the
//! note-source delay and the browser-query delay, and
//! `services/bookmark_excerpt.rs` owns the excerpt read path, because the service
//! owns the behavior those change. Mirroring them into a second value in `ui/`
//! would fork one policy across two workflows, and the note source is **shared**
//! with migrated `WFR-COMMAND-PALETTE`; slots 3a and 3b recorded the same
//! decision for `services/editor_io.rs`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::policy::{self, NOTES_BROWSER_SOURCE_ENTRY_LIMIT};

/// Test-only overrides for one process's notes and bookmarks workflow.
struct NotesTestPolicy {
    /// Narrows the browser's admitted source-entry ceiling so a focused
    /// truncation test does not have to create ten thousand sidecars.
    browser_source_entry_limit: AtomicUsize,
    /// Delays the closed-file bookmark excerpt worker so a widget test can
    /// observe a stale completion being refused.
    bookmark_excerpt_preview_delay_ms: AtomicU64,
}

impl NotesTestPolicy {
    const fn new() -> Self {
        Self {
            browser_source_entry_limit: AtomicUsize::new(NOTES_BROWSER_SOURCE_ENTRY_LIMIT),
            bookmark_excerpt_preview_delay_ms: AtomicU64::new(0),
        }
    }
}

static POLICY: NotesTestPolicy = NotesTestPolicy::new();

/// Override the browser source-entry policy for focused truncation tests.
pub fn set_notes_browser_source_entry_limit_for_test(limit: usize) {
    POLICY
        .browser_source_entry_limit
        .store(limit, Ordering::Release);
}

/// Delay closed-file bookmark excerpt loads so supersession can be observed.
pub fn set_bookmark_excerpt_preview_delay_for_test(delay_ms: u64) {
    POLICY
        .bookmark_excerpt_preview_delay_ms
        .store(delay_ms, Ordering::Release);
}

/// Return the effective browser source policy for this process.
pub(super) fn notes_browser_source_limits() -> crate::services::palette::NoteSourceLimits {
    policy::notes_browser_source_limits_for_entries(
        POLICY.browser_source_entry_limit.load(Ordering::Acquire),
    )
}

/// Sleep for the armed excerpt-preview delay, if a test set one.
pub(super) fn delay_bookmark_excerpt_preview() {
    let delay_ms = POLICY
        .bookmark_excerpt_preview_delay_ms
        .load(Ordering::Acquire);
    if delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
}

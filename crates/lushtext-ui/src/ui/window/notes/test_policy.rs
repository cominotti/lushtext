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

/// Test-only overrides for one process's notes and bookmarks workflow.
struct NotesTestPolicy {
    /// Narrows the browser's searchable-text ceiling so a focused byte-cut
    /// test does not have to write 64 MiB of notes (`0` means unset).
    browser_source_text_bytes: AtomicUsize,
    /// Delays the closed-file bookmark excerpt worker so a widget test can
    /// observe a stale completion being refused.
    bookmark_excerpt_preview_delay_ms: AtomicU64,
    /// Caps the units one open-editor capture turn takes, beside its clock,
    /// so a test sees several turns however fast the host is (`0` = unset).
    capture_units_per_turn: AtomicUsize,
    /// One-shot probe capturing which thread drops an installed folded note body.
    next_note_body_disposal_probe: crate::ui::plain_disposal::DisposalProbeSlot,
}

impl NotesTestPolicy {
    const fn new() -> Self {
        Self {
            browser_source_text_bytes: AtomicUsize::new(0),
            bookmark_excerpt_preview_delay_ms: AtomicU64::new(0),
            capture_units_per_turn: AtomicUsize::new(0),
            next_note_body_disposal_probe: crate::ui::plain_disposal::DisposalProbeSlot::new(),
        }
    }
}

static POLICY: NotesTestPolicy = NotesTestPolicy::new();

/// Arm a one-shot probe reporting which thread drops the next folded note body.
pub fn set_next_note_body_disposal_probe_for_test(
    sender: std::sync::mpsc::Sender<std::thread::ThreadId>,
) {
    POLICY.next_note_body_disposal_probe.set(sender);
}

/// Attach the armed disposal probe to one guarded note body, if set.
pub(super) fn attach_note_body_disposal_probe(
    owner: crate::ui::plain_disposal::DisposalOwned<String>,
) -> crate::ui::plain_disposal::DisposalOwned<String> {
    POLICY.next_note_body_disposal_probe.attach(owner)
}

/// Narrow the browser's searchable-text budget (`text_bytes`) and cap the
/// units of one open-editor capture turn (`capture_units_per_turn`) for
/// focused tests; `0` restores production for either.
pub fn set_notes_browser_budgets_for_test(text_bytes: usize, capture_units_per_turn: usize) {
    POLICY
        .browser_source_text_bytes
        .store(text_bytes, Ordering::Release);
    POLICY
        .capture_units_per_turn
        .store(capture_units_per_turn, Ordering::Release);
}

/// Delay closed-file bookmark excerpt loads so supersession can be observed.
pub fn set_bookmark_excerpt_preview_delay_for_test(delay_ms: u64) {
    POLICY
        .bookmark_excerpt_preview_delay_ms
        .store(delay_ms, Ordering::Release);
}

/// The test's narrowed searchable-text budget, if one is set.
pub(super) fn notes_browser_source_text_bytes() -> Option<usize> {
    let bytes = POLICY.browser_source_text_bytes.load(Ordering::Acquire);
    (bytes > 0).then_some(bytes)
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

/// Whether a test cap ends the capture turn after `units` units.
pub(super) fn capture_turn_capped(units: usize) -> bool {
    let cap = POLICY.capture_units_per_turn.load(Ordering::Acquire);
    cap > 0 && units >= cap
}

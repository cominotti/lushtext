// SPDX-License-Identifier: GPL-3.0-or-later

//! The workspace tree workflow's single test-policy value.
//!
//! Everything a test may override about this workflow lives in one place, and the
//! whole module is behind `#[cfg(feature = "test-utils")]` so a production build
//! compiles no override storage at all. Adding a second module-level static — or
//! a second test-only *field on a production state struct*, which is the form
//! this workflow historically used and which no `static` grep finds — is the
//! regression this module exists to prevent.
//!
//! **What is deliberately not here.** `services/workspace_manager.rs` and
//! `services/workspace_watch.rs` own their own overrides, because the services
//! own the behavior those change and share it with the external-file-monitor
//! capability; slots 3a and 3b recorded the same decision for
//! `services/editor_io.rs`.

use std::sync::atomic::{AtomicU64, Ordering};

/// Test-only overrides for one process's workspace tree workflow.
struct WorkspaceTreeTestPolicy {
    /// Delays the guarded placeholder-cleanup worker so a test can interpose a
    /// competing rename between the cancel and the unlink.
    placeholder_cleanup_delay_ms: AtomicU64,
    /// Delays the guarded rename worker so a test can retarget the section's
    /// live context cell before the completion runs.
    rename_worker_delay_ms: AtomicU64,
}

impl WorkspaceTreeTestPolicy {
    const fn new() -> Self {
        Self {
            placeholder_cleanup_delay_ms: AtomicU64::new(0),
            rename_worker_delay_ms: AtomicU64::new(0),
        }
    }
}

static POLICY: WorkspaceTreeTestPolicy = WorkspaceTreeTestPolicy::new();

/// Delay placeholder cleanup so a competing rename can be interposed.
///
/// **One counted, justified actuation-adjacent configuration seam.** The
/// placeholder-cleanup ordering hazard is only reachable when another writer
/// takes the placeholder's name *between* the cancel and the unlink. Without a
/// delay the cleanup worker always wins that race in a headless test, so the
/// regression test for a confirmed data-destruction defect could not distinguish
/// the fixed code from the broken code — it passed either way, which is worse
/// than having no test. Every other property of the cleanup (the write guard,
/// the inode recheck, the conservative empty-only directory removal) is exercised
/// through the production path unchanged.
pub fn set_workspace_placeholder_cleanup_delay_for_test(delay_ms: u64) {
    POLICY
        .placeholder_cleanup_delay_ms
        .store(delay_ms, Ordering::Release);
}

/// Delay the rename worker so the section's context cell can be retargeted.
///
/// **A second counted, justified configuration seam, for the same reason as the
/// first.** The rename completion's wrong-row and stale-watch-target hazards are
/// only reachable when the live context cell changes *while the worker runs*. The
/// worker finishes faster than a headless test can retarget the cell, so without
/// this the regression test passed against the broken code as well as the fixed
/// code — a test that cannot fail is not coverage. Both of these seams exist
/// because the defects they prove destroy or silently strand the user's own
/// documents.
pub fn set_workspace_rename_worker_delay_for_test(delay_ms: u64) {
    POLICY
        .rename_worker_delay_ms
        .store(delay_ms, Ordering::Release);
}

/// Sleep for the armed rename-worker delay, if a test set one.
pub(super) fn delay_rename_worker() {
    let delay_ms = POLICY.rename_worker_delay_ms.load(Ordering::Acquire);
    if delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
}

/// Sleep for the armed placeholder-cleanup delay, if a test set one.
pub(super) fn delay_placeholder_cleanup() {
    let delay_ms = POLICY.placeholder_cleanup_delay_ms.load(Ordering::Acquire);
    if delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
}

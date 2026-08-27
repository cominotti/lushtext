// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "test-utils")]

//! The draft workflow's single test-only timing and limit value.
//!
//! This was the slot's largest configuration population: ten setters and nine
//! delay/fail hooks spread across module-level statics. They are one value here,
//! and the whole module is behind `#[cfg(feature = "test-utils")]`, so **a
//! production build compiles no override storage at all**. The public setter
//! names are unchanged, because they are what widget tests call.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use super::policy::FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS;
use crate::services::draft_service;

/// Every test-only timing, limit, and fault override this workflow owns.
///
/// Atomics rather than a `RefCell`: the delay and failure hooks are read from
/// **worker threads**, which is the whole reason they exist.
struct DraftTestPolicy {
    first_dirty_autosave_delay_ms: AtomicU64,
    automatic_draft_limit_bytes: AtomicU64,
    restore_delay_ms: AtomicU64,
    body_delay_ms: AtomicU64,
    manifest_delay_ms: AtomicU64,
    manifest_completion_delay_ms: AtomicU64,
    delete_delay_ms: AtomicU64,
    orphan_cleanup_start_delay_ms: AtomicU64,
    orphan_cleanup_followup_delay_ms: AtomicU64,
    orphan_cleanup_worker_delay_ms: AtomicU64,
    fail_next_body: AtomicBool,
    fail_next_manifest: AtomicBool,
    fail_next_delete: AtomicBool,
    next_body_disposal_probe: crate::ui::plain_disposal::DisposalProbeSlot,
}

static POLICY: DraftTestPolicy = DraftTestPolicy {
    first_dirty_autosave_delay_ms: AtomicU64::new(FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS),
    automatic_draft_limit_bytes: AtomicU64::new(draft_service::MAX_AUTOMATIC_DRAFT_BYTES),
    restore_delay_ms: AtomicU64::new(0),
    body_delay_ms: AtomicU64::new(0),
    manifest_delay_ms: AtomicU64::new(0),
    manifest_completion_delay_ms: AtomicU64::new(0),
    delete_delay_ms: AtomicU64::new(0),
    orphan_cleanup_start_delay_ms: AtomicU64::new(2_000),
    orphan_cleanup_followup_delay_ms: AtomicU64::new(30_000),
    orphan_cleanup_worker_delay_ms: AtomicU64::new(0),
    fail_next_body: AtomicBool::new(false),
    fail_next_manifest: AtomicBool::new(false),
    fail_next_delete: AtomicBool::new(false),
    next_body_disposal_probe: crate::ui::plain_disposal::DisposalProbeSlot::new(),
};

/// Configure the first-dirty autosave debounce for timing-sensitive widget tests.
pub fn set_first_dirty_autosave_delay_for_test(delay_ms: u64) {
    POLICY
        .first_dirty_autosave_delay_ms
        .store(delay_ms, Ordering::Release);
}

/// Configure the automatic draft limit for focused widget tests.
pub fn set_automatic_draft_limit_for_test(max_bytes: u64) {
    POLICY
        .automatic_draft_limit_bytes
        .store(max_bytes, Ordering::Release);
}

/// Delay every asynchronous draft read for deterministic freshness tests.
pub fn set_draft_restore_delay_for_test(delay_ms: u64) {
    POLICY.restore_delay_ms.store(delay_ms, Ordering::Release);
}

/// Backwards-compatible name for existing aggregate-budget tests.
pub fn set_lazy_draft_read_delay_for_test(delay_ms: u64) {
    set_draft_restore_delay_for_test(delay_ms);
}

/// Delay body, manifest, and delete stages independently for ordered race tests.
pub fn set_draft_mutation_delays_for_test(body_ms: u64, manifest_ms: u64, delete_ms: u64) {
    POLICY.body_delay_ms.store(body_ms, Ordering::Release);
    POLICY
        .manifest_delay_ms
        .store(manifest_ms, Ordering::Release);
    POLICY.delete_delay_ms.store(delete_ms, Ordering::Release);
}

/// Delay worker return after a manifest upsert is already durable.
pub fn set_draft_manifest_completion_delay_for_test(delay_ms: u64) {
    POLICY
        .manifest_completion_delay_ms
        .store(delay_ms, Ordering::Release);
}

/// Inject one failure at each selected production-routed mutation stage.
pub fn fail_next_draft_mutations_for_test(body: bool, manifest: bool, delete: bool) {
    POLICY.fail_next_body.store(body, Ordering::Release);
    POLICY.fail_next_manifest.store(manifest, Ordering::Release);
    POLICY.fail_next_delete.store(delete, Ordering::Release);
}

/// Configure orphan-cleanup timer and worker delays for deterministic widget tests.
pub fn set_orphan_cleanup_delays_for_test(start_ms: u64, followup_ms: u64, worker_ms: u64) {
    POLICY
        .orphan_cleanup_start_delay_ms
        .store(start_ms, Ordering::Release);
    POLICY
        .orphan_cleanup_followup_delay_ms
        .store(followup_ms, Ordering::Release);
    POLICY
        .orphan_cleanup_worker_delay_ms
        .store(worker_ms, Ordering::Release);
}

/// Observe the worker thread that finally retires the next restored draft body.
pub fn set_next_draft_body_disposal_probe_for_test(sender: Sender<std::thread::ThreadId>) {
    POLICY.next_body_disposal_probe.set(sender);
}

pub(super) fn first_dirty_autosave_delay_ms() -> u64 {
    POLICY.first_dirty_autosave_delay_ms.load(Ordering::Acquire)
}

pub(super) fn automatic_draft_limit_bytes() -> u64 {
    POLICY.automatic_draft_limit_bytes.load(Ordering::Acquire)
}

pub(super) fn restore_delay_ms() -> u64 {
    POLICY.restore_delay_ms.load(Ordering::Acquire)
}

pub(super) fn body_delay_ms() -> u64 {
    POLICY.body_delay_ms.load(Ordering::Acquire)
}

pub(super) fn manifest_delay_ms() -> u64 {
    POLICY.manifest_delay_ms.load(Ordering::Acquire)
}

pub(super) fn manifest_completion_delay_ms() -> u64 {
    POLICY.manifest_completion_delay_ms.load(Ordering::Acquire)
}

pub(super) fn delete_delay_ms() -> u64 {
    POLICY.delete_delay_ms.load(Ordering::Acquire)
}

pub(super) fn orphan_cleanup_start_delay_ms() -> u64 {
    POLICY.orphan_cleanup_start_delay_ms.load(Ordering::Acquire)
}

pub(super) fn orphan_cleanup_followup_delay_ms() -> u64 {
    POLICY
        .orphan_cleanup_followup_delay_ms
        .load(Ordering::Acquire)
}

pub(super) fn orphan_cleanup_worker_delay_ms() -> u64 {
    POLICY
        .orphan_cleanup_worker_delay_ms
        .load(Ordering::Acquire)
}

pub(super) fn take_body_failure() -> bool {
    POLICY.fail_next_body.swap(false, Ordering::AcqRel)
}

pub(super) fn take_manifest_failure() -> bool {
    POLICY.fail_next_manifest.swap(false, Ordering::AcqRel)
}

pub(super) fn take_delete_failure() -> bool {
    POLICY.fail_next_delete.swap(false, Ordering::AcqRel)
}

pub(super) fn attach_body_disposal_probe(
    owner: crate::ui::plain_disposal::DisposalOwned<String>,
) -> crate::ui::plain_disposal::DisposalOwned<String> {
    POLICY.next_body_disposal_probe.attach(owner)
}

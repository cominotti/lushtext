// SPDX-License-Identifier: GPL-3.0-or-later

//! `admission` role for the workspace tree workflow's directory-scan stage order.
//!
//! # Role
//!
//! This is the **admission** coordination role: the gate that decides whether a new
//! child-directory scan may start against a bounded process-wide budget, reserving a
//! permit up front and releasing it on completion or rejection, plus the retry that
//! re-arms a refused scan. Reserve-then-settle, never fire-and-forget.
//!
//! Dissolved out of the pre-convention `tree_loading.rs`, which was not one
//! coordination job: it mixed this admission gate with the scan worker, child-store
//! materialization, and the reorder drag shield (which moved to
//! `reorder_execution.rs`, the role that owns that stage order and its accessibility
//! parity).
//!
//! # Scope of the budget, stated honestly
//!
//! `WORKSPACE_SCAN_TASK_LIMIT` and its two counters are **process-global**, shared by
//! every workspace section in every window — not per-section. Anything projecting
//! these counters as evidence must name that scope rather than implying the numbers
//! belong to one section or one window.

use super::LushtextWorkspaceSection;
use super::scan_execution;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Process-wide child/emptiness tasks allowed to retain admitted scan payloads.
pub(super) const WORKSPACE_SCAN_TASK_LIMIT: usize = 4;
/// Compact admission retries share one frame-paced source per section.
const WORKSPACE_SCAN_ADMISSION_RETRY: Duration = Duration::from_millis(16);

static ACTIVE_WORKSPACE_SCAN_TASKS: AtomicUsize = AtomicUsize::new(0);
static WORKSPACE_SCAN_TASK_HIGH_WATER: AtomicUsize = AtomicUsize::new(0);

pub(super) struct WorkspaceScanPermit;

impl Drop for WorkspaceScanPermit {
    fn drop(&mut self) {
        ACTIVE_WORKSPACE_SCAN_TASKS.fetch_sub(1, Ordering::Release);
    }
}

pub(super) fn try_acquire_workspace_scan_permit() -> Option<WorkspaceScanPermit> {
    let admitted = ACTIVE_WORKSPACE_SCAN_TASKS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
            (active < WORKSPACE_SCAN_TASK_LIMIT).then_some(active + 1)
        })
        .ok()?;
    WORKSPACE_SCAN_TASK_HIGH_WATER.fetch_max(admitted + 1, Ordering::AcqRel);
    Some(WorkspaceScanPermit)
}

pub(super) fn arm_workspace_scan_admission_retry(section: &LushtextWorkspaceSection) {
    if section
        .imp()
        .workspace_scan_admission_source
        .borrow()
        .is_some()
    {
        return;
    }
    let section_weak = section.downgrade();
    let source = glib::timeout_add_local(WORKSPACE_SCAN_ADMISSION_RETRY, move || {
        let Some(section) = section_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        retry_workspace_scan_admission(&section)
    });
    section
        .imp()
        .workspace_scan_admission_source
        .replace(Some(source));
}

fn retry_workspace_scan_admission(section: &LushtextWorkspaceSection) -> glib::ControlFlow {
    for _ in 0..WORKSPACE_SCAN_TASK_LIMIT {
        let child_key = section
            .imp()
            .child_admission_scans
            .borrow()
            .keys()
            .next()
            .copied();
        let child = child_key.and_then(|key| {
            section
                .imp()
                .child_admission_scans
                .borrow_mut()
                .remove(&key)
                .map(|request| (key, request))
        });
        if let Some((store_key, request)) = child {
            scan_execution::start_child_scan(section, store_key, request);
        } else if !super::folder_execution::retry_one_folder_empty_admission(section) {
            break;
        }
        if ACTIVE_WORKSPACE_SCAN_TASKS.load(Ordering::Acquire) >= WORKSPACE_SCAN_TASK_LIMIT {
            break;
        }
    }

    let waiting = !section.imp().child_admission_scans.borrow().is_empty()
        || !section.imp().folder_empty_admission.borrow().is_empty();
    if waiting {
        glib::ControlFlow::Continue
    } else {
        section.imp().workspace_scan_admission_source.take();
        glib::ControlFlow::Break
    }
}

/// Currently admitted scan tasks, process-wide.
///
/// Ungated on purpose: the evidence surface needs this in a production build, and an
/// admission counter is ordinary observable state rather than test-only state.
pub(super) fn active_scan_tasks() -> usize {
    ACTIVE_WORKSPACE_SCAN_TASKS.load(Ordering::Acquire)
}

/// High-water mark of admitted scan tasks, process-wide.
pub(super) fn scan_task_high_water() -> usize {
    WORKSPACE_SCAN_TASK_HIGH_WATER.load(Ordering::Acquire)
}

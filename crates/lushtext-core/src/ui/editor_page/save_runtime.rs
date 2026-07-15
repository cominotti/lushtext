// SPDX-License-Identifier: GPL-3.0-or-later

//! Process-wide byte admission for editor save payloads.
//!
//! Queued requests retain only weak widget ownership, scalar freshness, a
//! destination path, and the completion callback. Complete document text is
//! captured only after the save-specific policy grants shared capacity.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::model::editor_memory::EDITOR_MEMORY_UPPER_BUDGET_BYTES;
#[cfg(feature = "test-utils")]
use crate::model::save_admission::SaveAdmissionSnapshot;
use crate::model::save_admission::{
    ExternalTransientPressure, SaveAdmissionPolicy, SaveAdmissionPriority, SaveAdmissionRequest,
    conservative_save_payload_weight,
};
use crate::services::editor_io::EditorSaveError;
use crate::ui::window::LushtextWindow;
use gtk4::prelude::*;

use super::LushtextEditorPage;
use super::load_runtime;
use super::load_save::SaveCallback;

thread_local! {
    static COORDINATOR: RefCell<SaveAdmissionCoordinator> =
        RefCell::new(SaveAdmissionCoordinator::default());
}

struct QueuedSave {
    editor: glib::WeakRef<LushtextEditorPage>,
    editor_id: usize,
    generation: u64,
    path: PathBuf,
    cancel_pending_load: bool,
    required_modified: bool,
    close_session_identity: Option<u64>,
    allow_lossy: bool,
    callback: Option<SaveCallback>,
}

#[derive(Default)]
struct SaveAdmissionCoordinator {
    policy: SaveAdmissionPolicy,
    requests: BTreeMap<u64, QueuedSave>,
    next_request_id: u64,
    next_sequence: u64,
    drain_scheduled: bool,
}

/// Exact-once ownership of one admitted save payload charge.
///
/// The permit is `Send` and has no GTK state, so snapshot cancellation, worker
/// failure/panic, stale completion, and successful write consumption all
/// converge on the same main-context release path.
pub(super) struct SavePayloadPermit {
    request_id: Option<u64>,
    weight: u64,
}

impl Drop for SavePayloadPermit {
    fn drop(&mut self) {
        let Some(request_id) = self.request_id.take() else {
            return;
        };
        debug_assert!(self.weight > 0, "admitted saves carry a byte charge");
        glib::idle_add_once(move || release_on_main(request_id));
    }
}

pub(super) struct SaveSubmission {
    pub path: PathBuf,
    pub cancel_pending_load: bool,
    pub priority: SaveAdmissionPriority,
    pub close_session_identity: Option<u64>,
    pub allow_lossy: bool,
    pub callback: SaveCallback,
}

pub(super) fn submit(editor: &LushtextEditorPage, generation: u64, submission: SaveSubmission) {
    let editor_id = editor.as_ptr() as usize;
    let required_modified = editor.is_modified();
    let weight = conservative_save_payload_weight(editor.estimated_live_buffer_bytes());
    let destination_identity = destination_identity(&submission.path);

    COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.next_request_id = coordinator.next_request_id.wrapping_add(1);
        coordinator.next_sequence = coordinator.next_sequence.wrapping_add(1);
        let request_id = coordinator.next_request_id;
        coordinator.policy.queue(SaveAdmissionRequest {
            request_id,
            owner_id: u64::try_from(editor_id).unwrap_or(u64::MAX),
            save_generation: generation,
            destination_identity,
            close_session_identity: submission.close_session_identity,
            sequence: coordinator.next_sequence,
            weight,
            priority: submission.priority,
        });
        coordinator.requests.insert(
            request_id,
            QueuedSave {
                editor: editor.downgrade(),
                editor_id,
                generation,
                path: submission.path,
                cancel_pending_load: submission.cancel_pending_load,
                required_modified,
                close_session_identity: submission.close_session_identity,
                allow_lossy: submission.allow_lossy,
                callback: Some(submission.callback),
            },
        );
    });
    schedule_drain();
    load_runtime::schedule_drain_for_external_change();
}

pub(super) fn cancel_for_editor(editor: &LushtextEditorPage) {
    let editor_id = editor.as_ptr() as usize;
    let cancelled = COORDINATOR.with_borrow_mut(|coordinator| {
        let stale = coordinator
            .requests
            .iter()
            .filter_map(|(request_id, request)| {
                (request.editor_id == editor_id).then_some(*request_id)
            })
            .collect::<Vec<_>>();
        let mut cancelled = Vec::with_capacity(stale.len());
        for request_id in stale {
            coordinator.policy.cancel_queued(request_id);
            if let Some(request) = coordinator.requests.remove(&request_id) {
                cancelled.push(request);
            }
        }
        cancelled
    });
    for mut request in cancelled {
        if let Some(callback) = request.callback.take() {
            callback(Err(EditorSaveError::SnapshotCancelled));
        }
    }
    schedule_drain();
    load_runtime::schedule_drain_for_external_change();
}

fn schedule_drain() {
    let should_schedule = COORDINATOR.with_borrow_mut(|coordinator| {
        if coordinator.drain_scheduled {
            false
        } else {
            coordinator.drain_scheduled = true;
            true
        }
    });
    if should_schedule {
        glib::idle_add_local_once(drain);
    }
}

pub(super) fn schedule_drain_for_external_change() {
    schedule_drain();
}

fn drain() {
    let (load_weight, load_exclusive) = load_runtime::active_pressure();
    let (cancelled, dispatches) = COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.drain_scheduled = false;

        let stale = coordinator
            .requests
            .iter()
            .filter_map(|(request_id, request)| {
                let current = request.editor.upgrade().is_some_and(|editor| {
                    editor.queued_save_is_current(
                        request.generation,
                        &request.path,
                        request.cancel_pending_load,
                        request.required_modified,
                        request.close_session_identity,
                    )
                });
                (!current).then_some(*request_id)
            })
            .collect::<Vec<_>>();
        let mut cancelled = Vec::with_capacity(stale.len());
        for request_id in stale {
            coordinator.policy.cancel_queued(request_id);
            if let Some(request) = coordinator.requests.remove(&request_id) {
                cancelled.push(request);
            }
        }

        for (request_id, request) in &coordinator.requests {
            if let Some(editor) = request.editor.upgrade() {
                coordinator.policy.refresh_queued(
                    *request_id,
                    request.generation,
                    destination_identity(&request.path),
                    conservative_save_payload_weight(editor.estimated_live_buffer_bytes()),
                );
            }
        }

        let protected_over_budget =
            protected_residency_bytes(&coordinator.requests) > EDITOR_MEMORY_UPPER_BUDGET_BYTES;
        let pressure = ExternalTransientPressure {
            active_weight: load_weight,
            exclusive_active: load_exclusive,
            protected_residency_over_budget: protected_over_budget,
        };
        let mut dispatches = Vec::new();
        while let Some(grant) = coordinator.policy.admit_next(pressure) {
            let Some(request) = coordinator.requests.remove(&grant.request_id) else {
                let _ = coordinator.policy.release(grant.request_id);
                continue;
            };
            dispatches.push((
                request,
                SavePayloadPermit {
                    request_id: Some(grant.request_id),
                    weight: grant.weight,
                },
            ));
        }
        (cancelled, dispatches)
    });

    let cancelled_any = !cancelled.is_empty();
    for mut request in cancelled {
        if let Some(editor) = request.editor.upgrade() {
            editor.finish_queued_save_without_admission(request.generation);
        }
        if let Some(callback) = request.callback.take() {
            callback(Err(EditorSaveError::SnapshotCancelled));
        }
    }

    let admitted_any = !dispatches.is_empty();
    for (mut request, permit) in dispatches {
        let Some(callback) = request.callback.take() else {
            continue;
        };
        let Some(editor) = request.editor.upgrade() else {
            callback(Err(EditorSaveError::SnapshotCancelled));
            continue;
        };
        editor.begin_admitted_save(
            request.generation,
            request.path,
            request.cancel_pending_load,
            request.required_modified,
            request.close_session_identity,
            request.allow_lossy,
            permit,
            callback,
        );
    }
    if cancelled_any || admitted_any {
        // Removing a stale close save can be the event that makes queued loads
        // eligible again, even when this drain admits no replacement save.
        load_runtime::schedule_drain_for_external_change();
    }
}

fn release_on_main(request_id: u64) {
    let released =
        COORDINATOR.with_borrow_mut(|coordinator| coordinator.policy.release(request_id));
    if released {
        schedule_drain();
        load_runtime::schedule_drain_for_external_change();
    }
}

fn protected_residency_bytes(requests: &BTreeMap<u64, QueuedSave>) -> u64 {
    let application = requests.values().find_map(|request| {
        request
            .editor
            .upgrade()
            .and_then(|editor| editor.root())
            .and_then(|root| root.downcast::<LushtextWindow>().ok())
            .and_then(|window| window.application())
    });
    if let Some(application) = application {
        return application
            .windows()
            .into_iter()
            .filter_map(|window| window.downcast::<LushtextWindow>().ok())
            .fold(0u64, |total, window| {
                total.saturating_add(window.protected_editor_residency_bytes())
            });
    }

    requests.values().fold(0u64, |total, request| {
        total.saturating_add(
            request
                .editor
                .upgrade()
                .map_or(0, |editor| editor.estimated_live_buffer_bytes()),
        )
    })
}

fn destination_identity(path: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn active_pressure() -> (u64, bool) {
    COORDINATOR.with_borrow(|coordinator| {
        let snapshot = coordinator.policy.snapshot();
        (snapshot.active_weight, snapshot.exclusive_active)
    })
}

pub(super) fn close_work_pending_or_active() -> bool {
    COORDINATOR.with_borrow(|coordinator| {
        let snapshot = coordinator.policy.snapshot();
        snapshot.queued_close_count > 0 || snapshot.active_close_count > 0
    })
}

#[cfg(feature = "test-utils")]
#[must_use]
pub(super) fn snapshot_for_test() -> SaveAdmissionSnapshot {
    COORDINATOR.with_borrow(|coordinator| coordinator.policy.snapshot())
}

#[cfg(feature = "test-utils")]
pub(super) fn reset_for_test() {
    COORDINATOR.with_borrow_mut(|coordinator| *coordinator = SaveAdmissionCoordinator::default());
}

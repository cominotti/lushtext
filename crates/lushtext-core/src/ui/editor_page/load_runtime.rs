// SPDX-License-Identifier: GPL-3.0-or-later

//! Process-wide transient file-load admission for editor pages.
//!
//! The queue retains only weak widget ownership and compact metadata plans.
//! Document-sized read/decode work enters the generic worker executor only
//! after the plain-Rust byte policy grants capacity.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
#[cfg(feature = "test-utils")]
use std::sync::mpsc::Sender;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

use crate::model::editor_memory::EDITOR_MEMORY_UPPER_BUDGET_BYTES;
use crate::model::encoding::DocumentEncoding;
#[cfg(feature = "test-utils")]
use crate::model::file_load::FileLoadAdmissionSnapshot;
use crate::model::file_load::{
    FileLoadAdmissionPolicy, FileLoadAdmissionRequest, FileLoadPriority,
    decoded_body_reservation_weight,
};
use crate::services::editor_io::{self, EditorLoadError, FileLoadPlan, LoadMetadata};
use crate::ui::window::LushtextWindow;

use super::save;
use super::{EditorLoadState, LushtextEditorPage};

thread_local! {
    static COORDINATOR: RefCell<FileLoadCoordinator> = RefCell::new(FileLoadCoordinator::default());
}

#[cfg(feature = "test-utils")]
static NEXT_LOAD_BODY_DISPOSAL_PROBE: crate::ui::plain_disposal::DisposalProbeSlot =
    crate::ui::plain_disposal::DisposalProbeSlot::new();
#[cfg(feature = "test-utils")]
static NEXT_LOAD_DISPOSAL_RESERVATION_WEIGHT: AtomicU64 = AtomicU64::new(0);

/// Observe the worker thread that finally retires the next decoded load body.
#[cfg(feature = "test-utils")]
pub fn set_next_load_body_disposal_probe_for_test(sender: Sender<std::thread::ThreadId>) {
    NEXT_LOAD_BODY_DISPOSAL_PROBE.set(sender);
}

/// Override one conservative reservation without allocating a giant GTK fixture.
#[cfg(feature = "test-utils")]
pub fn set_next_load_disposal_reservation_weight_for_test(weight: u64) {
    NEXT_LOAD_DISPOSAL_RESERVATION_WEIGHT.store(weight, AtomicOrdering::Release);
}

fn attach_load_body_disposal_probe(
    owner: crate::ui::plain_disposal::DisposalOwned<String>,
) -> crate::ui::plain_disposal::DisposalOwned<String> {
    #[cfg(feature = "test-utils")]
    let owner = NEXT_LOAD_BODY_DISPOSAL_PROBE.attach(owner);
    owner
}

struct QueuedLoad {
    editor: glib::WeakRef<LushtextEditorPage>,
    editor_id: usize,
    generation: u64,
    plan: FileLoadPlan,
    reopen_as: Option<DocumentEncoding>,
    cancel: Arc<AtomicBool>,
    error_state: EditorLoadState,
    sequence: u64,
}

#[derive(Default)]
struct FileLoadCoordinator {
    policy: FileLoadAdmissionPolicy,
    requests: BTreeMap<u64, QueuedLoad>,
    next_request_id: u64,
    next_sequence: u64,
    drain_scheduled: bool,
    disposal_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
}

/// RAII ownership for one admitted payload charge.
///
/// This value is `Send` and deliberately contains no GTK state, so worker
/// panic, stale result, page disposal, installation cancellation, and normal
/// finalization all converge on the same exact-once release path.
pub(super) struct TransientLoadPermit {
    request_id: Option<u64>,
    weight: u64,
}

impl TransientLoadPermit {
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub(super) fn weight(&self) -> u64 {
        self.weight
    }
}

impl Drop for TransientLoadPermit {
    fn drop(&mut self) {
        let Some(request_id) = self.request_id.take() else {
            return;
        };
        debug_assert!(
            self.weight > 0,
            "admitted file-load permits carry a byte charge"
        );
        // Drop may run on a worker during unwinding. A Send-only idle handoff
        // keeps the thread-local coordinator on GTK's default main context.
        glib::idle_add_once(move || release_on_main(request_id));
    }
}

pub(super) struct GuardedLoadResult {
    pub(super) metadata: LoadMetadata,
    pub(super) content: crate::ui::plain_disposal::DisposalOwned<String>,
}

struct AdmittedLoadOutcome {
    permit: TransientLoadPermit,
    result: Result<GuardedLoadResult, EditorLoadError>,
}

pub(super) fn submit(
    editor: &LushtextEditorPage,
    generation: u64,
    plan: FileLoadPlan,
    reopen_as: Option<DocumentEncoding>,
    cancel: Arc<AtomicBool>,
    error_state: EditorLoadState,
) {
    let editor_id = editor.as_ptr() as usize;
    COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.next_request_id = coordinator.next_request_id.wrapping_add(1);
        coordinator.next_sequence = coordinator.next_sequence.wrapping_add(1);
        let request_id = coordinator.next_request_id;
        let sequence = coordinator.next_sequence;
        coordinator.policy.queue(FileLoadAdmissionRequest {
            request_id,
            owner_id: u64::try_from(editor_id).unwrap_or(u64::MAX),
            sequence,
            weight: plan.transient_weight,
            priority: current_priority(editor),
        });
        coordinator.requests.insert(
            request_id,
            QueuedLoad {
                editor: editor.downgrade(),
                editor_id,
                generation,
                plan,
                reopen_as,
                cancel,
                error_state,
                sequence,
            },
        );
    });
    schedule_drain();
}

pub(super) fn cancel_for_editor(editor: &LushtextEditorPage) {
    let editor_id = editor.as_ptr() as usize;
    COORDINATOR.with_borrow_mut(|coordinator| {
        let stale = coordinator
            .requests
            .iter()
            .filter_map(|(request_id, request)| {
                (request.editor_id == editor_id).then_some(*request_id)
            })
            .collect::<Vec<_>>();
        for request_id in stale {
            coordinator.requests.remove(&request_id);
            coordinator.policy.cancel_queued(request_id);
        }
    });
    schedule_drain();
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

fn drain() {
    let (save_weight, save_exclusive) = save::admission::active_pressure();
    let close_save_pending = save::admission::close_work_pending_or_active();
    let (dispatches, disposal_blocked_epoch) = COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.drain_scheduled = false;
        let stale = coordinator
            .requests
            .iter()
            .filter_map(|(request_id, request)| {
                let current = request.editor.upgrade().is_some_and(|editor| {
                    editor.load_request_is_current(request.generation, &request.cancel)
                });
                (!current).then_some(*request_id)
            })
            .collect::<Vec<_>>();
        for request_id in stale {
            coordinator.requests.remove(&request_id);
            coordinator.policy.cancel_queued(request_id);
        }

        for (request_id, request) in &coordinator.requests {
            if let Some(editor) = request.editor.upgrade() {
                coordinator
                    .policy
                    .update_priority(*request_id, current_priority(&editor));
            }
        }

        let protected_over_budget =
            protected_residency_bytes(&coordinator.requests) > EDITOR_MEMORY_UPPER_BUDGET_BYTES;
        let mut dispatches = Vec::new();
        let mut disposal_blocked_epoch = None;
        if !close_save_pending {
            while let Some(grant) = coordinator.policy.admit_next_with_external(
                protected_over_budget,
                save_weight,
                save_exclusive,
            ) {
                let Some(request) = coordinator.requests.get(&grant.request_id) else {
                    let _ = coordinator.policy.release(grant.request_id);
                    continue;
                };
                let reservation_weight =
                    decoded_body_reservation_weight(request.plan.facts.byte_size);
                #[cfg(feature = "test-utils")]
                let reservation_weight = {
                    let override_weight =
                        NEXT_LOAD_DISPOSAL_RESERVATION_WEIGHT.swap(0, AtomicOrdering::AcqRel);
                    if override_weight > 0 {
                        override_weight
                    } else {
                        reservation_weight
                    }
                };
                let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
                let Some(reservation) =
                    crate::ui::plain_disposal::try_reserve_file_load_for_gtk(reservation_weight)
                else {
                    let priority = request
                        .editor
                        .upgrade()
                        .map_or(FileLoadPriority::Normal, |editor| current_priority(&editor));
                    let queued = FileLoadAdmissionRequest {
                        request_id: grant.request_id,
                        owner_id: u64::try_from(request.editor_id).unwrap_or(u64::MAX),
                        sequence: request.sequence,
                        weight: request.plan.transient_weight,
                        priority,
                    };
                    let _ = coordinator.policy.release(grant.request_id);
                    coordinator.policy.queue(queued);
                    disposal_blocked_epoch = Some(observed_epoch);
                    break;
                };
                let Some(request) = coordinator.requests.remove(&grant.request_id) else {
                    let _ = coordinator.policy.release(grant.request_id);
                    drop(reservation);
                    continue;
                };
                dispatches.push((
                    request,
                    TransientLoadPermit {
                        request_id: Some(grant.request_id),
                        weight: grant.weight,
                    },
                    reservation,
                ));
            }
        }
        (dispatches, disposal_blocked_epoch)
    });

    for (request, permit, reservation) in dispatches {
        dispatch(request, permit, reservation);
    }
    COORDINATOR.with_borrow(|coordinator| {
        if let Some(observed_epoch) = disposal_blocked_epoch {
            coordinator
                .disposal_wakeup
                .arm(observed_epoch, schedule_drain);
        } else {
            coordinator.disposal_wakeup.cancel();
        }
    });
    save::admission::schedule_drain_for_external_change();
}

fn dispatch(
    request: QueuedLoad,
    permit: TransientLoadPermit,
    mut reservation: crate::ui::plain_disposal::DisposalReservation,
) {
    let editor_weak = request.editor.clone();
    let generation = request.generation;
    let error_state = request.error_state;
    let cancel = Arc::clone(&request.cancel);
    let reopen_as = request.reopen_as;
    let plan = request.plan;
    spawn_blocking_then(
        editor_weak,
        move || {
            let result =
                editor_io::load_planned_text_file(plan, &cancel, reopen_as).map(|loaded| {
                    let editor_io::LoadResult { metadata, content } = loaded;
                    reservation.shrink_to(u64::try_from(content.capacity()).unwrap_or(u64::MAX));
                    GuardedLoadResult {
                        metadata,
                        content: attach_load_body_disposal_probe(reservation.own(content)),
                    }
                });
            AdmittedLoadOutcome { permit, result }
        },
        move |editor_weak, outcome| {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            editor.accept_admitted_load_outcome(
                generation,
                outcome.result,
                error_state,
                outcome.permit,
            );
        },
    );
}

fn release_on_main(request_id: u64) {
    let released =
        COORDINATOR.with_borrow_mut(|coordinator| coordinator.policy.release(request_id));
    if released {
        schedule_drain();
        save::admission::schedule_drain_for_external_change();
    }
}

pub(super) fn schedule_drain_for_external_change() {
    schedule_drain();
}

pub(super) fn active_pressure() -> (u64, bool) {
    COORDINATOR.with_borrow(|coordinator| {
        let snapshot = coordinator.policy.snapshot();
        (snapshot.active_weight, snapshot.exclusive_active)
    })
}

fn current_priority(editor: &LushtextEditorPage) -> FileLoadPriority {
    if editor
        .root()
        .and_then(|root| root.downcast::<LushtextWindow>().ok())
        .is_some_and(|window| window.is_selected_editor(editor))
    {
        FileLoadPriority::Active
    } else {
        FileLoadPriority::Normal
    }
}

fn protected_residency_bytes(requests: &BTreeMap<u64, QueuedLoad>) -> u64 {
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
        let bytes = request
            .editor
            .upgrade()
            .map_or(0, |editor| editor.estimated_live_buffer_bytes());
        total.saturating_add(bytes)
    })
}

#[cfg(feature = "test-utils")]
#[must_use]
pub(super) fn snapshot_for_test() -> FileLoadAdmissionSnapshot {
    COORDINATOR.with_borrow(|coordinator| coordinator.policy.snapshot())
}

#[cfg(feature = "test-utils")]
#[must_use]
pub(super) fn disposal_wakeup_armed_for_test() -> bool {
    COORDINATOR.with_borrow(|coordinator| coordinator.disposal_wakeup.is_armed())
}

#[cfg(feature = "test-utils")]
pub(super) fn reset_for_test() {
    COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.disposal_wakeup.cancel();
        *coordinator = FileLoadCoordinator::default();
    });
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the document-save workflow's admission job.
//!
//! Admission is *reserve then settle*. A save request is queued as compact
//! scalars — weak editor ownership, a generation, a destination path, and the
//! completion callback — and only once
//! [`policy::SaveAdmissionPolicy`](super::policy::SaveAdmissionPolicy) grants
//! shared byte capacity does the workflow capture document-sized text. That
//! ordering is the whole point: it is what stops several editors from each
//! holding a full copy of their buffer while waiting for a worker.
//!
//! This module owns the process-wide coordinator, the queue stage that publishes
//! save ownership, the idle drain that retires stale requests and admits fresh
//! ones, and the exactly-once release of an admitted charge. It does not decide
//! *whether* a request is still current — that is
//! [`policy::queued_save_is_current`](super::policy::queued_save_is_current),
//! validated here against one [`QueuedSaveTicket`] rather than clause by clause.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::model::editor_memory::EDITOR_MEMORY_UPPER_BUDGET_BYTES;
use crate::services::editor_io::EditorSaveError;
use crate::ui::window::LushtextWindow;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::policy::{
    ExternalTransientPressure, QueuedSaveFacts, QueuedSaveTicket, SaveAdmissionPolicy,
    SaveAdmissionPriority, SaveAdmissionRequest, SaveAdmissionSnapshot,
    conservative_save_payload_weight, queued_save_is_current, save_may_preempt_pending_load,
};
use super::{SaveCallback, execution};
use crate::ui::editor_page::load;
use crate::ui::editor_page::{EditorLoadState, LushtextEditorPage};

thread_local! {
    static COORDINATOR: RefCell<SaveAdmissionCoordinator> =
        RefCell::new(SaveAdmissionCoordinator::default());
}

/// One queued request: the ticket it was queued under, plus who to tell.
struct QueuedSave {
    editor: glib::WeakRef<LushtextEditorPage>,
    editor_id: usize,
    ticket: QueuedSaveTicket,
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
pub(crate) struct SavePayloadPermit {
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

/// Queue one save request, publishing save ownership before yielding.
///
/// This is the workflow's stage 1. It refuses the request outright when the
/// editor cannot honour it, then advances the save generation and marks the
/// editor saving *before* the idle drain runs, so a duplicate save and an
/// already-planned memory eviction both revalidate this page as protected.
pub(crate) fn queue_save_request(
    editor: &LushtextEditorPage,
    path: PathBuf,
    explicit_destination: bool,
    priority: SaveAdmissionPriority,
    close_session_identity: Option<u64>,
    callback: SaveCallback,
) {
    let may_preempt_pending_load = save_may_preempt_pending_load(explicit_destination);
    if editor.imp().load_state.get() == EditorLoadState::Loading {
        if may_preempt_pending_load {
            // This is the first of two `cancel_load()` calls, and they are not
            // duplication: they sit on opposite sides of the refusal gates
            // below and observe different state. This one runs *before* the
            // `installation_incomplete` gate on purpose — cancelling a live
            // chunked install is what sets that flag, because the cancelled
            // path deliberately empties the buffer, so the gate then refuses
            // this save instead of queueing a write of content that is about to
            // be cleared. `cancel_load` also advances the load generation on
            // every call, which `SaveCompletionTicket` captures, so merging the
            // two calls changes observable values as well as ordering.
            editor.cancel_load();
        } else {
            callback(Err(EditorSaveError::LoadInProgress));
            return;
        }
    }
    if editor.imp().load.installation_incomplete.get() {
        callback(Err(EditorSaveError::IncompleteLoadInstallation));
        return;
    }
    if editor.buffer_replacement_in_progress() {
        callback(Err(EditorSaveError::LoadInProgress));
        return;
    }
    if editor.imp().save.inflight.get() {
        callback(Err(EditorSaveError::SaveInProgress));
        return;
    }

    if may_preempt_pending_load {
        // The second call, *after* the gates. It covers the paths where
        // `load_state` was not `Loading` — most importantly during final
        // projection, where it withdraws a reload a file-loaded callback queued
        // reentrantly and returns without advancing the generation.
        editor.cancel_load();
    }
    // Publish queued ownership before yielding so duplicate saves and an
    // already-planned eviction pass revalidate this page as protected.
    let generation = editor.imp().save.generation.get().wrapping_add(1);
    editor.imp().save.generation.set(generation);
    editor.imp().save.inflight.set(true);
    editor.notify_memory_policy_changed();

    // Consent belongs to this generation: cancellation must discard it
    // instead of allowing unrelated later content to save lossily.
    let allow_lossy = editor.take_lossy_save_once();

    submit(
        editor,
        QueuedSaveTicket {
            save_generation: generation,
            path,
            explicit_destination,
            required_modified: editor.is_modified(),
            close_session_identity,
        },
        priority,
        allow_lossy,
        callback,
    );
}

fn submit(
    editor: &LushtextEditorPage,
    ticket: QueuedSaveTicket,
    priority: SaveAdmissionPriority,
    allow_lossy: bool,
    callback: SaveCallback,
) {
    let editor_id = editor.as_ptr() as usize;
    let weight = conservative_save_payload_weight(editor.estimated_live_buffer_bytes());
    let destination_identity = destination_identity(&ticket.path);

    COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.next_request_id = coordinator.next_request_id.wrapping_add(1);
        coordinator.next_sequence = coordinator.next_sequence.wrapping_add(1);
        let request_id = coordinator.next_request_id;
        coordinator.policy.queue(SaveAdmissionRequest {
            request_id,
            owner_id: u64::try_from(editor_id).unwrap_or(u64::MAX),
            save_generation: ticket.save_generation,
            destination_identity,
            close_session_identity: ticket.close_session_identity,
            sequence: coordinator.next_sequence,
            weight,
            priority,
        });
        coordinator.requests.insert(
            request_id,
            QueuedSave {
                editor: editor.downgrade(),
                editor_id,
                ticket,
                allow_lossy,
                callback: Some(callback),
            },
        );
    });
    schedule_drain();
    load::admission::schedule_drain_for_external_change();
}

/// Observe the live editor state one queued ticket must still match.
///
/// Captured against the ticket, so the close-session clause already answers this
/// ticket's question and is `true` when the ticket names no close session.
fn queued_save_facts(editor: &LushtextEditorPage, ticket: &QueuedSaveTicket) -> QueuedSaveFacts {
    QueuedSaveFacts {
        saving: editor.is_saving(),
        save_generation: editor.imp().save.generation.get(),
        modified: editor.is_modified(),
        current_path: editor.file_path(),
        close_session_current: ticket
            .close_session_identity
            .is_none_or(|identity| close_save_session_is_current(editor, identity)),
    }
}

/// Whether the window still owns the close session a save was queued under.
pub(super) fn close_save_session_is_current(editor: &LushtextEditorPage, identity: u64) -> bool {
    editor
        .root()
        .and_then(|root| root.downcast::<LushtextWindow>().ok())
        .is_some_and(|window| window.close_save_session_is_current(identity))
}

/// Whether one queued save still describes the editor it was queued against.
pub(super) fn queued_save_ticket_is_current(
    editor: &LushtextEditorPage,
    ticket: &QueuedSaveTicket,
) -> bool {
    queued_save_is_current(ticket, &queued_save_facts(editor, ticket))
}

/// Take every queued request matching `doomed` out of the queue and the policy.
///
/// Both callers that retire requests — editor teardown and the drain's stale
/// pass — need the same three steps in the same order, and only differ in which
/// requests they consider retired.
fn take_queued(
    coordinator: &mut SaveAdmissionCoordinator,
    doomed: impl Fn(&QueuedSave) -> bool,
) -> Vec<QueuedSave> {
    let request_ids = coordinator
        .requests
        .iter()
        .filter_map(|(request_id, request)| doomed(request).then_some(*request_id))
        .collect::<Vec<_>>();
    let mut taken = Vec::with_capacity(request_ids.len());
    for request_id in request_ids {
        coordinator.policy.cancel_queued(request_id);
        if let Some(request) = coordinator.requests.remove(&request_id) {
            taken.push(request);
        }
    }
    taken
}

/// Drop every queued request owned by one editor, telling each caller.
pub(crate) fn cancel_for_editor(editor: &LushtextEditorPage) {
    let editor_id = editor.as_ptr() as usize;
    let cancelled = COORDINATOR.with_borrow_mut(|coordinator| {
        take_queued(coordinator, |request| request.editor_id == editor_id)
    });
    for mut request in cancelled {
        if let Some(callback) = request.callback.take() {
            callback(Err(EditorSaveError::SnapshotCancelled));
        }
    }
    schedule_drain();
    load::admission::schedule_drain_for_external_change();
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

/// Re-run admission because another lane's pressure changed.
pub(crate) fn schedule_drain_for_external_change() {
    schedule_drain();
}

/// Retire stale queued requests, then admit as many fresh ones as fit.
///
/// This is where control resumes after the queue stage yielded: the idle source
/// scheduled by [`schedule_drain`] calls this, and every admitted request
/// continues into [`execution::begin_admitted_save`].
fn drain() {
    let (load_weight, load_exclusive) = load::admission::active_pressure();
    let (cancelled, dispatches) = COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.drain_scheduled = false;

        let cancelled = take_queued(coordinator, |request| {
            !request
                .editor
                .upgrade()
                .is_some_and(|editor| queued_save_ticket_is_current(&editor, &request.ticket))
        });

        for (request_id, request) in &coordinator.requests {
            if let Some(editor) = request.editor.upgrade() {
                coordinator.policy.refresh_queued(
                    *request_id,
                    request.ticket.save_generation,
                    destination_identity(&request.ticket.path),
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
            finish_queued_save_without_admission(&editor, request.ticket.save_generation);
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
        execution::begin_admitted_save(
            &editor,
            request.ticket,
            request.allow_lossy,
            permit,
            callback,
        );
    }
    if cancelled_any || admitted_any {
        // Removing a stale close save can be the event that makes queued loads
        // eligible again, even when this drain admits no replacement save.
        load::admission::schedule_drain_for_external_change();
    }
}

/// Release save ownership for a request that never reached admission.
pub(super) fn finish_queued_save_without_admission(
    editor: &LushtextEditorPage,
    save_generation: u64,
) {
    if editor.imp().save.generation.get() != save_generation {
        return;
    }
    editor.imp().save.inflight.set(false);
    editor.notify_memory_policy_changed();
    editor.refresh_accessibility_metadata();
}

fn release_on_main(request_id: u64) {
    let released =
        COORDINATOR.with_borrow_mut(|coordinator| coordinator.policy.release(request_id));
    if released {
        schedule_drain();
        load::admission::schedule_drain_for_external_change();
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

/// Byte weight and exclusivity this lane currently imposes on other lanes.
pub(crate) fn active_pressure() -> (u64, bool) {
    COORDINATOR.with_borrow(|coordinator| {
        let snapshot = coordinator.policy.snapshot();
        (snapshot.active_weight, snapshot.exclusive_active)
    })
}

/// Whether any close-gating save is queued or running.
pub(crate) fn close_work_pending_or_active() -> bool {
    COORDINATOR.with_borrow(|coordinator| {
        let snapshot = coordinator.policy.snapshot();
        snapshot.queued_close_count > 0 || snapshot.active_close_count > 0
    })
}

/// Process-wide scalar admission accounting, for the evidence surface.
pub(super) fn admission_snapshot() -> SaveAdmissionSnapshot {
    COORDINATOR.with_borrow(|coordinator| coordinator.policy.snapshot())
}

/// Whether an idle drain is already armed, for the evidence surface.
pub(super) fn drain_pending() -> bool {
    COORDINATOR.with_borrow(|coordinator| coordinator.drain_scheduled)
}

#[cfg(feature = "test-utils")]
impl LushtextEditorPage {
    /// Reset process-wide save admission state between isolated widget cases.
    ///
    /// Preserved actuation seam: shared admission accounting outlives any one
    /// editor and no user path empties it, so isolated cases need a way to start
    /// from a known-clean lane.
    pub fn reset_transient_save_admission_for_test(&self) {
        COORDINATOR
            .with_borrow_mut(|coordinator| *coordinator = SaveAdmissionCoordinator::default());
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the document-load workflow's admission job.
//!
//! Admission is *reserve then settle*. Nothing document-sized is read until a
//! shared byte budget has granted capacity for it, so several tabs opening large
//! files at once cannot each hold a full decoded copy while waiting for a
//! worker. This module owns everything that happens before decoded bytes exist:
//!
//! - the **entry stage**, which rotates the load generation and the cancellation
//!   token, publishes provisional identity, and either dispatches planning or
//!   parks the request behind bounded cleanup;
//! - the **planning probe**, a compact worker read that produces the
//!   [`FileLoadPlan`] whose `transient_weight` is the number the budget is spent
//!   against — it copies no document text;
//! - the **process-wide coordinator**, its queue, its idle drain, and the
//!   disposal-capacity wakeup that re-arms the drain when the disposal lane
//!   frees room;
//! - **exactly-once release** of one admitted charge, through
//!   [`TransientLoadPermit`]'s `Drop`.
//!
//! It does not decide *whether* a request is still current: that is
//! [`policy::load_request_is_current`](super::policy::load_request_is_current),
//! validated here against one [`LoadRequestTicket`] rather than clause by
//! clause.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::model::editor_memory::EDITOR_MEMORY_UPPER_BUDGET_BYTES;
use crate::model::encoding::DocumentEncoding;
use crate::model::file_load::{
    FileLoadAdmissionPolicy, FileLoadAdmissionRequest, FileLoadAdmissionSnapshot, FileLoadPriority,
    decoded_body_reservation_weight,
};
use crate::services::editor_io::{self, EditorLoadError, FileLoadPlan, LoadMetadata};
use crate::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use crate::ui::window::LushtextWindow;

use super::policy::{self, AbortDisposition, LoadOutcome, LoadRequestTicket};
use super::{evidence, execution, retirement, save};

thread_local! {
    static COORDINATOR: RefCell<FileLoadCoordinator> = RefCell::new(FileLoadCoordinator::default());
}

/// Latest load request held while a prior partial buffer is cleared in slices.
///
/// Only the newest intent survives: an earlier parked request's planning owner
/// is released as soon as a later one replaces it, so no caller waits forever
/// for a terminal that will never come.
pub(crate) struct PendingFileLoad {
    path: PathBuf,
    reopen_as: Option<DocumentEncoding>,
    planning_terminal: Option<Box<dyn FnOnce()>>,
}

impl PendingFileLoad {
    /// Release the parked request's background planning owner.
    pub(super) fn finish_planning(mut self) {
        if let Some(callback) = self.planning_terminal.take() {
            callback();
        }
    }
}

struct QueuedLoad {
    editor: glib::WeakRef<LushtextEditorPage>,
    editor_id: usize,
    ticket: LoadRequestTicket,
    plan: FileLoadPlan,
    reopen_as: Option<DocumentEncoding>,
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

impl FileLoadCoordinator {
    /// Drop every queued request the predicate selects, freeing its queue slot.
    ///
    /// The ids are collected before removal because the predicate reads the
    /// queue this then mutates.
    fn retire_queued_where(&mut self, retire: impl Fn(&QueuedLoad) -> bool) {
        let retired = self
            .requests
            .iter()
            .filter_map(|(request_id, request)| retire(request).then_some(*request_id))
            .collect::<Vec<_>>();
        for request_id in retired {
            self.requests.remove(&request_id);
            self.policy.cancel_queued(request_id);
        }
    }
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
    /// The byte charge this permit holds, as the evidence surface reports it.
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

/// One decoded load result whose body is owned by the disposal lane.
pub(super) struct GuardedLoadResult {
    pub(super) metadata: LoadMetadata,
    pub(super) content: crate::ui::plain_disposal::DisposalOwned<String>,
}

struct AdmittedLoadOutcome {
    permit: TransientLoadPermit,
    result: Result<GuardedLoadResult, EditorLoadError>,
}

impl LoadRequestTicket {
    /// Whether this dispatched request still describes its editor's live state.
    ///
    /// The ticket carries dispatch-time expectation; the editor supplies the
    /// live generation and token. No `*Facts` companion is needed because every
    /// clause the completion compares is live editor state.
    pub(super) fn is_current(&self, editor: &LushtextEditorPage) -> bool {
        policy::load_request_is_current(
            self,
            editor.imp().load_tracking.generation.get(),
            &editor.imp().load_tracking.cancel_token.borrow(),
        )
    }
}

/// Stage 1: accept one load request for this editor.
///
/// Three ways out, in order. A request arriving during final projection or
/// during a live installation is **parked** as the single latest intent and the
/// current work is asked to stop; bounded cleanup restarts it. Otherwise the
/// editor's load identity is rotated — a fresh generation and a fresh
/// cancellation token, captured together as one [`LoadRequestTicket`] — and the
/// compact planning probe is dispatched.
pub(crate) fn begin_load_request(
    editor: &LushtextEditorPage,
    path: &Path,
    reopen_as: Option<DocumentEncoding>,
    planning_terminal: Option<Box<dyn FnOnce()>>,
) {
    if editor.imp().load.finalizing.get() {
        park_pending_request(editor, path, reopen_as, planning_terminal);
        retirement::cancel_noninstall_load_resources(editor);
        return;
    }
    let installation = editor.imp().load.installation.borrow().clone();
    if let Some(session) = installation {
        park_pending_request(editor, path, reopen_as, planning_terminal);
        editor
            .imp()
            .load_tracking
            .cancel_token
            .borrow()
            .store(true, Ordering::Release);
        cancel_for_editor(editor);
        retirement::abort_installation(&session, AbortDisposition::Cancel);
        return;
    }

    let file_path = path.to_path_buf();
    let error_state = policy::load_failure_state(editor.imp().load_state.get());
    retirement::cancel_noninstall_load_resources(editor);
    editor
        .imp()
        .load
        .planning_terminal_callback
        .replace(planning_terminal);
    editor.imp().file_path.replace(Some(file_path.clone()));
    editor.imp().canonical_file_path.borrow_mut().take();
    editor.imp().file_size.set(None);
    editor.imp().load_state.set(EditorLoadState::Loading);
    editor.imp().latest_load_failed.set(false);
    editor.notify_memory_policy_changed();
    editor.refresh_accessibility_metadata();
    editor.stop_file_monitor();

    let ticket = rotate_load_identity(editor);
    evidence::record_load_started(editor);

    let planning_ticket = ticket.clone();
    let cancel_for_plan = Arc::clone(&ticket.cancel_token);
    spawn_blocking_then(
        editor.downgrade(),
        move || editor_io::plan_text_file(&file_path, &cancel_for_plan),
        move |editor_weak, result| {
            // This early return cannot drop a stored planning terminal, for two
            // independent reasons, and both must stay true. First,
            // `finish_load_planning` below is outside the `if`/`else`, so even a
            // stale ticket releases the terminal — moving it inside either arm
            // would be a regression. Second, `upgrade()` fails only after
            // finalization, and GObject runs `dispose()` strictly before
            // `finalize()`; `LushtextEditorPage::dispose` calls
            // `dispose_load_resources`, which takes and calls the stored
            // callback and finishes any parked request's planning owner. So by
            // the time this arm can be reached, the slot is already empty.
            //
            // The stake is the session-restore sequencer: it counts exactly
            // these releases to decide when to open the next document, so a
            // dropped terminal would stall restore. It could never
            // over-admit — that is the property `release_permit` protects, and a
            // missing release can only under-admit.
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if planning_ticket.is_current(&editor) {
                let load_generation = planning_ticket.load_generation;
                match result {
                    Ok(plan) => submit(&editor, planning_ticket, plan, reopen_as, error_state),
                    Err(error) => {
                        execution::apply_load_result_if_current(
                            &editor,
                            load_generation,
                            Err(error),
                            error_state,
                        );
                    }
                }
            } else {
                refuse_stale_completion(&editor);
            }
            finish_load_planning(&editor);
        },
    );
}

/// Park the newest request behind work that must finish first.
fn park_pending_request(
    editor: &LushtextEditorPage,
    path: &Path,
    reopen_as: Option<DocumentEncoding>,
    planning_terminal: Option<Box<dyn FnOnce()>>,
) {
    if let Some(replaced) = editor
        .imp()
        .load
        .pending_load
        .replace(Some(PendingFileLoad {
            path: path.to_path_buf(),
            reopen_as,
            planning_terminal,
        }))
    {
        replaced.finish_planning();
    }
}

/// Take the parked request, if the workflow left one behind.
pub(super) fn take_pending_request(editor: &LushtextEditorPage) -> Option<PendingFileLoad> {
    editor.imp().load.pending_load.take()
}

/// Drop the parked request, releasing its background planning owner.
///
/// Every retirement path takes this route rather than dropping the parked
/// request silently: whoever is waiting on that terminal — the session-restore
/// sequencer counts exactly these releases — would otherwise wait forever.
pub(super) fn discard_pending_request(editor: &LushtextEditorPage) {
    if let Some(pending) = take_pending_request(editor) {
        pending.finish_planning();
    }
}

/// Restart a parked request now that bounded cleanup has finished.
pub(super) fn resume_pending_request(editor: &LushtextEditorPage, pending: PendingFileLoad) {
    begin_load_request(
        editor,
        &pending.path,
        pending.reopen_as,
        pending.planning_terminal,
    );
}

/// Advance this editor's load identity and capture it as one ticket.
fn rotate_load_identity(editor: &LushtextEditorPage) -> LoadRequestTicket {
    evidence::record_retired_request_cancellation(
        editor,
        editor
            .imp()
            .load_tracking
            .cancel_token
            .borrow()
            .load(Ordering::Acquire),
    );
    let cancel = Arc::new(AtomicBool::new(false));
    editor
        .imp()
        .load_tracking
        .cancel_token
        .replace(Arc::clone(&cancel));
    let load_generation = editor.imp().load_tracking.generation.get().wrapping_add(1);
    editor.imp().load_tracking.generation.set(load_generation);
    LoadRequestTicket::new(load_generation, cancel)
}

/// Advance the load identity without starting a new request.
///
/// Cancellation and disposal both use this so a worker already in flight can
/// never publish against the identity it was dispatched under.
pub(super) fn retire_load_identity(editor: &LushtextEditorPage) {
    editor
        .imp()
        .load_tracking
        .generation
        .set(editor.imp().load_tracking.generation.get().wrapping_add(1));
}

/// Release the background planning owner, whichever path reached the terminal.
pub(super) fn finish_load_planning(editor: &LushtextEditorPage) {
    if let Some(callback) = editor.imp().load.planning_terminal_callback.take() {
        callback();
    }
}

/// Stage 3: queue one planned request under the shared byte budget.
fn submit(
    editor: &LushtextEditorPage,
    ticket: LoadRequestTicket,
    plan: FileLoadPlan,
    reopen_as: Option<DocumentEncoding>,
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
                ticket,
                plan,
                reopen_as,
                error_state,
                sequence,
            },
        );
    });
    schedule_drain();
}

/// Drop every queued request this editor owns.
pub(super) fn cancel_for_editor(editor: &LushtextEditorPage) {
    let editor_id = editor.as_ptr() as usize;
    COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.retire_queued_where(|request| request.editor_id == editor_id);
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

/// Stage 4: retire stale requests, then admit as many as the budget allows.
fn drain() {
    let (save_weight, save_exclusive) = save::admission::active_pressure();
    let close_save_pending = save::admission::close_work_pending_or_active();
    let (dispatches, disposal_blocked_epoch) = COORDINATOR.with_borrow_mut(|coordinator| {
        coordinator.drain_scheduled = false;
        coordinator.retire_queued_where(|request| {
            !request
                .editor
                .upgrade()
                .is_some_and(|editor| request.ticket.is_current(&editor))
        });

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
                let reservation_weight = reservation_weight_for(request.plan.facts.byte_size);
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

/// The conservative disposal reservation one planned body is charged.
fn reservation_weight_for(source_bytes: u64) -> u64 {
    #[cfg(feature = "test-utils")]
    if let Some(override_weight) = super::test_policy::take_disposal_reservation_weight_override() {
        return override_weight;
    }
    decoded_body_reservation_weight(source_bytes)
}

/// Stage 5: read and decode the admitted request on a worker.
fn dispatch(
    request: QueuedLoad,
    permit: TransientLoadPermit,
    mut reservation: crate::ui::plain_disposal::DisposalReservation,
) {
    let editor_weak = request.editor.clone();
    let ticket = request.ticket;
    let error_state = request.error_state;
    let cancel = Arc::clone(&ticket.cancel_token);
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
                        content: attach_body_disposal_probe(reservation.own(content)),
                    }
                });
            AdmittedLoadOutcome { permit, result }
        },
        move |editor_weak, outcome| {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            execution::accept_admitted_load_outcome(
                &editor,
                &ticket,
                outcome.result,
                error_state,
                outcome.permit,
            );
        },
    );
}

fn attach_body_disposal_probe(
    owner: crate::ui::plain_disposal::DisposalOwned<String>,
) -> crate::ui::plain_disposal::DisposalOwned<String> {
    #[cfg(feature = "test-utils")]
    let owner = super::test_policy::attach_body_disposal_probe(owner);
    owner
}

fn release_on_main(request_id: u64) {
    let released =
        COORDINATOR.with_borrow_mut(|coordinator| coordinator.policy.release(request_id));
    if released {
        schedule_drain();
        save::admission::schedule_drain_for_external_change();
    }
}

/// Re-arm the drain because another lane's pressure changed.
pub(crate) fn schedule_drain_for_external_change() {
    schedule_drain();
}

/// Byte weight and exclusivity this lane currently holds, for the save lane.
pub(crate) fn active_pressure() -> (u64, bool) {
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

/// Process-wide scalar accounting for this lane, read by the evidence surface.
pub(super) fn admission_snapshot() -> FileLoadAdmissionSnapshot {
    COORDINATOR.with_borrow(|coordinator| coordinator.policy.snapshot())
}

/// Whether an idle drain is already armed.
pub(super) fn drain_pending() -> bool {
    COORDINATOR.with_borrow(|coordinator| coordinator.drain_scheduled)
}

/// Whether this lane is polling for the disposal lane to free capacity.
pub(super) fn disposal_wakeup_armed() -> bool {
    COORDINATOR.with_borrow(|coordinator| coordinator.disposal_wakeup.is_armed())
}

impl LushtextEditorPage {
    /// Reset process-wide admission state between isolated widget cases.
    ///
    /// Actuation seam, preserved: the coordinator is process-wide, so a test
    /// case cannot otherwise start from a known lane state.
    #[cfg(feature = "test-utils")]
    pub fn reset_transient_load_admission_for_test(&self) {
        COORDINATOR.with_borrow_mut(|coordinator| {
            coordinator.disposal_wakeup.cancel();
            *coordinator = FileLoadCoordinator::default();
        });
    }
}

/// Record that a completion was refused because its editor moved on.
pub(super) fn refuse_stale_completion(editor: &LushtextEditorPage) {
    evidence::record_outcome(editor, LoadOutcome::RefusedAsStale);
}

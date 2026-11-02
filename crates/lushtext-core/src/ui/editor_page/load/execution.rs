// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the document-load workflow's execution job.
//!
//! Execution is the admitted half — accept, install, publish. It begins only
//! once [`admission`](super::admission) has granted byte capacity and a worker
//! has produced decoded text, and it is the only place this workflow writes into
//! the live `GtkTextBuffer`.
//!
//! ## Installation is one state machine with four phases
//!
//! [`ChunkedLoadInstall`] is the whole of it. A slice is scheduled on a 1 ms
//! timeout, runs one phase, and either schedules the next slice or reaches a
//! terminal:
//!
//! | Phase | What one slice does | Next |
//! | --- | --- | --- |
//! | `ClearingExisting` | delete up to one clear budget of the previous buffer | itself until empty, then `Installing` |
//! | `Installing` | insert up to one install budget of decoded text | itself until the payload ends, then `Finalizing` |
//! | `ClearingCancelled` | delete what a cancelled install left behind | itself until empty, then terminal — owned by [`retirement`](super::retirement) |
//! | `Finalizing` | no slice runs; the final projection owns the main thread | terminal |
//!
//! Every slice boundary — insert and delete alike — ends on a **paragraph**
//! boundary. That is a performance contract with a user-visible failure mode,
//! not refactorable detail; see [`super::policy`] for why, and note that the
//! insert side reads its boundary from
//! [`next_install_boundary`](crate::model::file_load::next_install_boundary) in
//! `model/`.
//!
//! ## Small payloads take the direct path
//!
//! Under the shared synchronous-install threshold, and with a small existing
//! buffer, the swap happens in one turn: no session, no timeout, no slice
//! counter. Both paths converge on [`complete_loaded_installation`], so the
//! published result cannot differ between them.

use crate::ui::editor_page::edit_lock::EditLock;
use crate::ui::editor_page::protected_view::execution::ProtectedInstallSource;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::model::buffer_replacement::{SliceBudget, next_slice_boundary};
use crate::model::document_scale::DocumentScale;
use crate::model::encoding::FileHealthFindingKind;
use crate::services::editor_io::{self, EditorLoadError};
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::ui::editor_page::imp::EditorCallback;
use crate::ui::editor_page::{
    EditorLoadState, InlineAlertLifetime, LushtextEditorPage, PendingWarningAction,
};
use crate::ui::responsiveness_probe::PostOpenSpan;

use super::admission::{self, GuardedLoadResult, TransientLoadPermit};
use super::policy::{
    self, AbortDisposition, InstallSliceAction, LoadInstallPhase, LoadOutcome, LoadRequestTicket,
};
use super::{evidence, retirement};

/// Temporary view flags captured while a load makes the editor read-only.
///
/// The save workflow keeps its own equivalent in `save/execution.rs`. They are
/// deliberately not shared: each workflow owns the flags it suspended, so
/// neither has to reach into the other's state.
pub(super) struct ViewInteractivityState {
    cursor_visible: bool,
}

/// Projection and view flags restored after one complete or cancelled install.
pub(super) struct LoadInstallationState {
    lock: EditLock,
    view: ViewInteractivityState,
    minimap_tracking_suspended: bool,
    history_capture_suppressed: bool,
    projection_suspended: bool,
}

/// One generation-bound GTK installation retaining decoded text and admission.
pub(crate) struct ChunkedLoadInstall {
    pub(super) editor: glib::WeakRef<LushtextEditorPage>,
    pub(super) buffer: sourceview5::Buffer,
    pub(super) end_mark: Option<gtk4::TextMark>,
    pub(super) loaded: Option<GuardedLoadResult>,
    pub(super) byte_offset: usize,
    pub(super) generation: u64,
    pub(super) source_id: Option<glib::SourceId>,
    /// The two slice budgets; a dense document also bounds visible
    /// characters (`bound-post-open-settle`, D6.1).
    pub(super) budget: SliceBudget,
    pub(super) permit: Option<TransientLoadPermit>,
    /// Taken once, by the terminal that gives the suspension back.
    pub(super) restore: Option<LoadInstallationState>,
    pub(super) phase: LoadInstallPhase,
    pub(super) terminal: bool,
    pub(super) slice_count: u64,
    /// The index of the paragraph the next slice starts, so a protected
    /// body's fold plan applies at insertion (`ui::protected_fold`).
    pub(super) next_paragraph: u64,
}

impl ChunkedLoadInstall {
    /// The admitted payload charge this installation still retains.
    pub(super) fn retained_weight(&self) -> Option<u64> {
        self.permit.as_ref().map(TransientLoadPermit::weight)
    }

    /// Which phase this installation is running.
    pub(super) fn phase(&self) -> LoadInstallPhase {
        self.phase
    }
}

/// The priority a paced slice waits at: `G_PRIORITY_DEFAULT_IDLE` (200), below
/// GtkTextView's incremental validation idle (`GTK_TEXT_VIEW_PRIORITY_VALIDATE`,
/// 125) and above the settle sentinel (`G_PRIORITY_LOW`). Shared with the paced
/// whole-buffer replacement.
pub(crate) const PACED_SLICE_PRIORITY: glib::Priority = glib::Priority::DEFAULT_IDLE;

/// Schedule the next slice of a paced (dense) installation from an idle
/// below `GTK_TEXT_VIEW_PRIORITY_VALIDATE` (review R14, ledger A70):
/// GtkTextView's incremental validation idle lays out the previous slice's
/// new lines first, so each validation dispatch holds one slice's lines, and
/// the slices follow each other at main-loop speed (pacing by painted frames
/// took 16.7 s for an 11 MiB document against main's 1.9 s). A slice waits
/// for the frame that shows the latest key first (`input_quiet::paced_idle`).
fn schedule_paced_insert_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (session_for_step, session_for_store) = (Rc::clone(session), Rc::downgrade(session));
    crate::ui::input_quiet::paced_idle(
        PACED_SLICE_PRIORITY,
        move || run_install_slice(&session_for_step),
        move |source_id| {
            if let Some(session) = session_for_store.upgrade() {
                session.borrow_mut().source_id = Some(source_id);
            }
        },
    );
}

/// Schedule the next bounded slice of one installation.
pub(super) fn schedule_install_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let session_for_source = Rc::clone(session);
    let source_id = glib::timeout_add_local_once(Duration::from_millis(1), move || {
        run_install_slice(&session_for_source);
    });
    session.borrow_mut().source_id = Some(source_id);
}

fn run_install_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    crate::ui::responsiveness_probe::span("load_install_slice", || {
        run_install_slice_now(session);
    });
}

fn run_install_slice_now(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (editor, generation, phase, terminal) = {
        let mut state = session.borrow_mut();
        state.source_id.take();
        (
            state.editor.upgrade(),
            state.generation,
            state.phase,
            state.terminal,
        )
    };
    let installation_current = editor.as_ref().is_some_and(|editor| {
        policy::installation_is_current(
            editor.imp().load_tracking.generation.get(),
            generation,
            editor
                .imp()
                .load_tracking
                .cancel_token
                .borrow()
                .load(std::sync::atomic::Ordering::Acquire),
        )
    });

    match policy::install_slice_action(terminal, editor.is_some(), phase, installation_current) {
        // A terminal session and the finalizing phase both mean "no slice may
        // run": finalization owns the main thread until it publishes.
        InstallSliceAction::Ignore | InstallSliceAction::RunPhase(LoadInstallPhase::Finalizing) => {
        }
        InstallSliceAction::AbortDisposed => {
            retirement::abort_installation(session, AbortDisposition::Dispose);
        }
        InstallSliceAction::AbortCancelled => {
            retirement::abort_installation(session, AbortDisposition::Cancel);
        }
        InstallSliceAction::RunPhase(LoadInstallPhase::ClearingExisting) => {
            run_existing_clear_slice(session);
        }
        InstallSliceAction::RunPhase(LoadInstallPhase::Installing) => {
            let protected = session
                .borrow()
                .loaded
                .as_ref()
                .is_some_and(|loaded| loaded.metadata.profile.fold_plan.is_some());
            if protected && !crate::ui::protected_fold::tail_insert_admitted() {
                // Another document inserted a hidden tail in this iteration.
                schedule_install_slice(session);
                return;
            }
            run_text_insert_slice(session);
        }
        InstallSliceAction::RunPhase(LoadInstallPhase::ClearingCancelled) => {
            retirement::run_cancelled_clear_slice(session);
        }
    }
}

/// Delete one bounded slice of the buffer, ending on a paragraph boundary.
///
/// Returns whether the buffer is now empty.
pub(super) fn delete_buffer_slice(buffer: &sourceview5::Buffer) -> bool {
    let remaining = buffer.char_count();
    if remaining <= 0 {
        return true;
    }
    let mut start = buffer.start_iter();
    let mut end = start;
    let _ = end.forward_chars(policy::clear_slice_char_count(remaining));
    // GTK text layout validates whole paragraphs, so a deletion that stops
    // inside a line would re-lay-out the shrinking remainder on every turn.
    // Extending to the next line start deletes each paragraph exactly once.
    if policy::clear_slice_extends_to_paragraph_end(end.is_end(), end.starts_line()) {
        let _ = end.forward_line();
    }
    crate::ui::editor_page::edit_lock::with_own_edit(|| buffer.delete(&mut start, &mut end));
    buffer.char_count() == 0
}

fn run_existing_clear_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let buffer = session.borrow().buffer.clone();
    let cleared = delete_buffer_slice(&buffer);
    if session.borrow().phase != LoadInstallPhase::ClearingExisting {
        return;
    }
    if cleared {
        let mark = buffer.create_mark(None, &buffer.end_iter(), false);
        let mut state = session.borrow_mut();
        if state.phase != LoadInstallPhase::ClearingExisting || state.terminal {
            buffer.delete_mark(&mark);
            return;
        }
        state.end_mark = Some(mark);
        state.phase = LoadInstallPhase::Installing;
        let editor = state.editor.upgrade();
        let budget_is_paced = state.budget == SliceBudget::PACED;
        drop(state);
        // The old text is gone and nothing new is in: formatting applied now
        // invalidates no line (review R17).
        if let Some(editor) = editor {
            apply_pending_formatting(&editor);
            if budget_is_paced {
                editor.set_overscroll_before_install();
            }
        }
    }
    schedule_install_slice(session);
}

fn run_text_insert_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (buffer, mark, start, paragraph, loaded) = {
        let mut state = session.borrow_mut();
        let Some(mark) = state.end_mark.clone() else {
            drop(state);
            retirement::abort_installation(session, AbortDisposition::Cancel);
            return;
        };
        let Some(loaded) = state.loaded.take() else {
            drop(state);
            retirement::abort_installation(session, AbortDisposition::Cancel);
            return;
        };
        (
            state.buffer.clone(),
            mark,
            state.byte_offset,
            state.next_paragraph,
            loaded,
        )
    };
    let budget = session.borrow().budget;
    let end = next_slice_boundary(&loaded.content, start, budget);
    let content_len = loaded.content.len();
    let mut iter = buffer.iter_at_mark(&mark);
    // GtkTextBuffer emits signals synchronously. Keep the session unborrowed so
    // a reentrant cancellation can move it to bounded cleanup without panicking.
    let next_paragraph = {
        let _span = PostOpenSpan::begin("post_open_install_slice");
        crate::ui::editor_page::edit_lock::with_own_edit(|| {
            if let Some(plan) = loaded.metadata.profile.fold_plan.as_deref() {
                crate::ui::protected_fold::insert_folded(
                    buffer.upcast_ref(),
                    &mut iter,
                    &loaded.content[start..end],
                    paragraph,
                    plan,
                )
            } else {
                buffer.insert(&mut iter, &loaded.content[start..end]);
                paragraph
            }
        })
    };

    let reached_end = {
        let mut state = session.borrow_mut();
        if state.terminal || state.phase != LoadInstallPhase::Installing {
            drop(state);
            drop(loaded);
            return;
        }
        state.byte_offset = end;
        state.next_paragraph = next_paragraph;
        state.slice_count = state.slice_count.saturating_add(1);
        state.loaded = Some(loaded);
        end == content_len
    };

    if reached_end {
        finish_chunked_install(session);
    } else if budget == SliceBudget::PACED {
        schedule_paced_insert_slice(session);
    } else {
        schedule_install_slice(session);
    }
}

fn finish_chunked_install(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (editor, buffer, mark, source, loaded, restore, permit, slice_count, generation) = {
        let mut state = session.borrow_mut();
        if state.terminal {
            return;
        }
        state.phase = LoadInstallPhase::Finalizing;
        (
            state.editor.upgrade(),
            state.buffer.clone(),
            state.end_mark.take(),
            state.source_id.take(),
            state.loaded.take(),
            state.restore.take(),
            state.permit.take(),
            state.slice_count,
            state.generation,
        )
    };
    if let Some(source) = source {
        source.remove();
    }
    if let Some(editor) = editor.as_ref() {
        editor.imp().load.finalizing.set(true);
        editor.imp().load.dispose_during_finalization.set(false);
    }
    if let Some(mark) = mark {
        buffer.delete_mark(&mark);
    }
    let Some(editor) = editor else {
        drop(loaded);
        drop(permit);
        return;
    };
    let Some(restore) = restore else {
        // Another terminal already gave the suspension back.
        conclude_installation(&editor, session, permit);
        drop(loaded);
        return;
    };
    editor.imp().load.installation_slice_count.set(slice_count);
    let disposed_during_finalization = editor.imp().load.dispose_during_finalization.get();
    if disposed_during_finalization || editor.imp().load_tracking.generation.get() != generation {
        buffer.end_irreversible_action();
        // The suspension this installation captured must be given back when a
        // **live** editor publishes nothing. Leaving it would strand the tab
        // non-editable with local-history capture and minimap edit tracking
        // still suppressed — and worse, a superseding load would then capture
        // those already-suspended values as its own "previous" state and
        // faithfully restore them to suspended when it finished, making the
        // condition permanent for the session.
        //
        // A **disposed** editor is deliberately excluded, and the asymmetry is
        // the point: restoration reaches `source_view()` and `refresh_minimap()`,
        // both of which read panicking `TemplateChild` accessors that GTK4 has
        // already cleared in `dispose()`. There is nothing left to strand on a
        // widget that is going away, so giving state back to it can only turn a
        // teardown into a crash.
        if !disposed_during_finalization {
            restore_load_installation_state(&editor, restore);
        }
        conclude_installation(&editor, session, permit);
        drop(loaded);
        return;
    }
    let Some(loaded) = loaded else {
        // Same contract on the payload-less exit, plus the irreversible-action
        // block this arm previously left open, which would have kept undo
        // disabled for the tab.
        buffer.end_irreversible_action();
        restore_load_installation_state(&editor, restore);
        conclude_installation(&editor, session, permit);
        return;
    };
    // Retain admission through every final projection and callback. Dropping
    // it afterward schedules one coalesced queue drain.
    complete_loaded_installation(&editor, loaded, restore);
    conclude_installation(&editor, session, permit);
}

/// Retire one finalized installation: terminal, ownership, then finalization.
///
/// Reached by all three of `finish_chunked_install`'s exits — published,
/// payload-less, and superseded — so none of them can drop the order.
fn conclude_installation(
    editor: &LushtextEditorPage,
    session: &Rc<RefCell<ChunkedLoadInstall>>,
    permit: Option<TransientLoadPermit>,
) {
    session.borrow_mut().terminal = true;
    // A terminal that did not publish the load (cancelled, superseded) leaves
    // the buffer without the bytes the last decode fact described; the
    // publishing terminal sets the fact again in `complete_loaded_installation_now`.
    if !editor.is_protected_view() || editor.imp().load.installation_incomplete.get() {
        editor.imp().document_metadata.decode_inexact.set(false);
    }
    clear_installation_owner(editor, session);
    finish_load_finalization(editor, permit);
}

/// Release this editor's ownership of one installation session.
pub(super) fn clear_installation_owner(
    editor: &LushtextEditorPage,
    session: &Rc<RefCell<ChunkedLoadInstall>>,
) {
    let is_current = editor
        .imp()
        .load
        .installation
        .borrow()
        .as_ref()
        .is_some_and(|current| Rc::ptr_eq(current, session));
    if is_current {
        editor.imp().load.installation.take();
    }
}

/// Stage 6: accept one admitted worker outcome, or refuse it as stale.
pub(super) fn accept_admitted_load_outcome(
    editor: &LushtextEditorPage,
    ticket: &LoadRequestTicket,
    result: Result<GuardedLoadResult, EditorLoadError>,
    error_state: EditorLoadState,
    permit: TransientLoadPermit,
) {
    if !ticket.is_current(editor) {
        admission::refuse_stale_completion(editor);
        return;
    }
    match result {
        Ok(loaded) => install_guarded_load(editor, ticket.load_generation, loaded, Some(permit)),
        Err(EditorLoadError::Cancelled) => {}
        Err(error) => publish_load_error(editor, &error, error_state),
    }
}

/// Apply one background load result only if it still belongs to the newest load.
///
/// Background reads can finish after a later request or a cancellation. Keeping
/// the freshness check inside this helper makes the stale-result guard easy to
/// exercise without a timing race.
pub(super) fn apply_load_result_if_current(
    editor: &LushtextEditorPage,
    load_generation: u64,
    result: Result<editor_io::LoadResult, EditorLoadError>,
    error_state: EditorLoadState,
) -> bool {
    let ticket = LoadRequestTicket::new(
        load_generation,
        editor.imp().load_tracking.cancel_token.borrow().clone(),
    );
    if !ticket.is_current(editor) {
        admission::refuse_stale_completion(editor);
        return false;
    }
    match result {
        Ok(editor_io::LoadResult { metadata, content }) => {
            let weight = u64::try_from(content.capacity()).unwrap_or(u64::MAX);
            let Some(mut reservation) = crate::ui::plain_disposal::try_reserve_for_gtk(weight)
            else {
                return false;
            };
            reservation.shrink_to(weight);
            install_guarded_load(
                editor,
                load_generation,
                GuardedLoadResult {
                    metadata,
                    content: reservation.own(content),
                },
                None,
            );
        }
        Err(EditorLoadError::Cancelled) => {}
        Err(error) => publish_load_error(editor, &error, error_state),
    }
    true
}

fn install_guarded_load(
    editor: &LushtextEditorPage,
    load_generation: u64,
    loaded: GuardedLoadResult,
    permit: Option<TransientLoadPermit>,
) {
    crate::ui::responsiveness_probe::span("load_install_begin", || {
        install_guarded_load_now(editor, load_generation, loaded, permit);
    });
}

fn install_guarded_load_now(
    editor: &LushtextEditorPage,
    load_generation: u64,
    loaded: GuardedLoadResult,
    permit: Option<TransientLoadPermit>,
) {
    let summary = loaded.metadata.summary();
    let chunked = policy::requires_chunked_install(
        loaded.content.len(),
        editor.buffer().char_count(),
        policy::batch_exceeds_slice(summary),
    );
    if !chunked
        && loaded.metadata.profile.fold_plan.is_some()
        && !crate::ui::protected_fold::tail_insert_admitted()
    {
        // A direct install inserts its tails now: wait for the next
        // iteration when another document already inserted one in this one.
        defer_direct_protected_install(editor, load_generation, loaded, permit);
        return;
    }
    evidence::record_install_started(editor, chunked);
    if chunked {
        start_chunked_install(editor, load_generation, loaded, permit);
    } else {
        install_loaded_direct(editor, loaded, permit);
    }
}

fn publish_load_error(
    editor: &LushtextEditorPage,
    error: &EditorLoadError,
    error_state: EditorLoadState,
) {
    tracing::error!("{error}");
    let error_text = error.to_string();
    editor.imp().load_state.set(error_state);
    editor.imp().latest_load_failed.set(true);
    // No bytes were loaded, so no decode fact about them stands.
    editor.imp().document_metadata.decode_inexact.set(false);
    evidence::record_outcome(editor, LoadOutcome::Failed);
    editor.notify_memory_policy_changed();
    editor.emit_inline_notification(
        InlineActionNotification {
            style: InlineNotificationStyle::Error,
            title: "Could Not Open File".to_string(),
            body: error_text.clone(),
            primary_button: Some("_Retry".to_string()),
            secondary_button: None,
        },
        InlineAlertLifetime::SupersededByLoad,
    );
    editor.refresh_accessibility_metadata();
    if error_state == EditorLoadState::Loaded {
        editor.start_file_monitor();
    }
    // No installation began before a worker error, so no guard is held here
    // and a listener sees the buffer and language as they now stand.
    editor.fire_content_republished();
    if let Some(callback) = editor.imp().load.load_failed_callback.take() {
        callback(error_text);
    }
}

pub(super) fn install_loaded_direct(
    editor: &LushtextEditorPage,
    loaded: GuardedLoadResult,
    permit: Option<TransientLoadPermit>,
) {
    editor.imp().load.installation_slice_count.set(0);
    let plan = highlight_plan_for(editor, &loaded);
    let restore = begin_load_installation(editor, false, plan);
    // The direct path leaves the minimap projection attached, so a body the
    // scale keeps from the map or the context engine is gated before the
    // first byte enters the buffer.
    editor.prepare_for_incoming_profile(&loaded.metadata.profile);
    editor.begin_protected_install(
        &loaded.metadata.profile,
        ProtectedInstallSource::Load {
            eviction_reload: editor.imp().residency.evicted.get(),
        },
    );
    let buffer = editor.buffer();
    // A direct install's old text is small by the same threshold, so its
    // formatting goes on before the one `set_text` (D6.2).
    apply_pending_formatting(editor);
    editor.imp().load.finalizing.set(true);
    editor.imp().load.dispose_during_finalization.set(false);
    buffer.begin_irreversible_action();
    {
        let _span = PostOpenSpan::begin("post_open_install_direct");
        crate::ui::editor_page::edit_lock::with_own_edit(|| {
            match loaded.metadata.profile.fold_plan.as_deref() {
                // A protected body: every folded tail goes in hidden, in this same
                // dispatch as its head (design D1).
                Some(plan) => {
                    buffer.set_text("");
                    let mut start = buffer.start_iter();
                    crate::ui::protected_fold::insert_folded(
                        buffer.upcast_ref(),
                        &mut start,
                        &loaded.content,
                        0,
                        plan,
                    );
                }
                None => buffer.set_text(&loaded.content),
            }
        });
    }
    if editor.imp().load.dispose_during_finalization.get() {
        buffer.end_irreversible_action();
        finish_load_finalization(editor, permit);
        return;
    }
    complete_loaded_installation(editor, loaded, restore);
    finish_load_finalization(editor, permit);
}

fn start_chunked_install(
    editor: &LushtextEditorPage,
    generation: u64,
    loaded: GuardedLoadResult,
    permit: Option<TransientLoadPermit>,
) {
    let plan = highlight_plan_for(editor, &loaded);
    let restore = begin_load_installation(editor, true, plan);
    editor.prepare_for_incoming_profile(&loaded.metadata.profile);
    let loaded_summary = loaded.metadata.summary();
    editor.begin_protected_install(
        &loaded.metadata.profile,
        ProtectedInstallSource::Load {
            eviction_reload: editor.imp().residency.evicted.get(),
        },
    );
    let buffer = editor.buffer();
    buffer.begin_irreversible_action();
    let session = Rc::new(RefCell::new(ChunkedLoadInstall {
        editor: editor.downgrade(),
        buffer,
        end_mark: None,
        loaded: Some(loaded),
        byte_offset: 0,
        generation,
        source_id: None,
        budget: SliceBudget::for_profile(loaded_summary),
        permit,
        restore: Some(restore),
        phase: LoadInstallPhase::ClearingExisting,
        terminal: false,
        slice_count: 0,
        next_paragraph: 0,
    }));
    editor.imp().load.installation_slice_count.set(0);
    editor
        .imp()
        .load
        .installation
        .replace(Some(Rc::clone(&session)));
    schedule_install_slice(&session);
}

/// Apply the EditorConfig overrides the planning worker resolved, if any,
/// before the install's first insert (`bound-post-open-settle`, D6.2).
fn apply_pending_formatting(editor: &LushtextEditorPage) {
    let pending = editor.imp().load.pending_formatting.take();
    editor
        .imp()
        .load
        .formatting_applied_before_insert
        .set(pending.is_some());
    if let Some(overrides) = pending {
        editor.apply_editorconfig_overrides(overrides);
    }
}

/// The highlight order for one install (`policy::highlight_attach_plan`).
fn highlight_plan_for(
    editor: &LushtextEditorPage,
    loaded: &GuardedLoadResult,
) -> policy::HighlightAttachPlan {
    policy::highlight_attach_plan(
        editor
            .scale_for_incoming_profile(&loaded.metadata.profile)
            .syntax_enabled,
    )
}

/// Suspend the projections and view flags one installation must not trigger.
fn begin_load_installation(
    editor: &LushtextEditorPage,
    suspend_minimap_projection: bool,
    highlight: policy::HighlightAttachPlan,
) -> LoadInstallationState {
    editor.invalidate_minimap_analysis_content();
    let imp = editor.imp();
    let view = editor.source_view();
    let restore = LoadInstallationState {
        // The buffer-level lock, not `set_editable(false)`: restoring
        // `editable` would invalidate every line of the installed document
        // (edit_lock.rs).
        lock: editor.lock_edits(),
        view: ViewInteractivityState {
            cursor_visible: view.is_cursor_visible(),
        },
        minimap_tracking_suspended: imp.minimap.tracking_suspended.replace(true),
        history_capture_suppressed: editor.suspend_local_history_capture(),
        projection_suspended: imp.load.projection_suspended.replace(true),
    };
    if suspend_minimap_projection {
        editor.suspend_minimap_projection();
    }
    if editor.search_bar().search_context().is_some() {
        editor.search_bar().detach();
    }
    // The old text's modified-line marks would collapse onto the first line
    // as it is cleared, and GtkSourceView sorts every mark of a line in each
    // frame's gutter and background paint: up to 2,000 marks made each frame
    // of a dense replacement cost hundreds of milliseconds. The new text gets
    // its own marks from its workflow's terminal.
    editor.clear_modified_line_marks();
    view.set_cursor_visible(false);
    if highlight == policy::HighlightAttachPlan::OffUntilPublish {
        editor.buffer().set_highlight_syntax(false);
    }
    editor.refresh_accessibility_metadata();
    restore
}

/// Re-run a direct protected install on a later main-loop iteration, while its
/// load is still the editor's current one.
fn defer_direct_protected_install(
    editor: &LushtextEditorPage,
    load_generation: u64,
    loaded: GuardedLoadResult,
    permit: Option<TransientLoadPermit>,
) {
    let editor = editor.downgrade();
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        let Some(editor) = editor.upgrade() else {
            return;
        };
        let current = policy::installation_is_current(
            editor.imp().load_tracking.generation.get(),
            load_generation,
            editor
                .imp()
                .load_tracking
                .cancel_token
                .borrow()
                .load(std::sync::atomic::Ordering::Acquire),
        );
        if current {
            install_guarded_load(&editor, load_generation, loaded, permit);
        } else {
            admission::refuse_stale_completion(&editor);
        }
    });
}

/// Stage 7: publish the installed content as this tab's loaded document.
fn complete_loaded_installation(
    editor: &LushtextEditorPage,
    loaded: GuardedLoadResult,
    restore: LoadInstallationState,
) {
    crate::ui::responsiveness_probe::span("load_complete", || {
        complete_loaded_installation_now(editor, loaded, restore);
    });
}

fn complete_loaded_installation_now(
    editor: &LushtextEditorPage,
    loaded: GuardedLoadResult,
    restore: LoadInstallationState,
) {
    let _span = PostOpenSpan::begin("post_open_publish");
    let GuardedLoadResult {
        metadata: loaded,
        content,
    } = loaded;
    let summary = loaded.summary();
    let editor_io::LoadMetadata {
        size,
        profile: _,
        canonical_path,
        mtime,
        encoding_state,
        has_bom,
        file_health,
        decode_round_trips,
    } = loaded;
    let buffer = editor.buffer();
    if DocumentScale::for_profile(summary).undo_enabled {
        buffer.end_irreversible_action();
    }
    buffer.set_modified(false);
    buffer.place_cursor(&buffer.start_iter());

    editor.imp().file_size.set(Some(size));
    // A load starts a new document session from the worker profile; an
    // eviction reload keeps the session floor (design Open Question 3).
    let eviction_reload = editor.imp().residency.evicted.get();
    editor.imp().canonical_file_path.replace(canonical_path);
    editor.imp().latest_load_failed.set(false);
    editor.imp().load.installation_incomplete.set(false);
    editor.imp().residency.evicted.set(false);
    editor.set_document_encoding_state(encoding_state);
    editor.set_has_bom(has_bom);
    editor
        .imp()
        .document_metadata
        .decode_inexact
        .set(decode_round_trips == Some(false));
    editor.set_file_health(file_health);
    // The scale gates syntax, bracket matching, invisibles, search
    // highlighting and the minimap before a highlight idle can run.
    editor.begin_document_scale_session(summary, eviction_reload);
    editor.clear_modified_line_marks();
    editor.apply_restore_position();
    editor.imp().monitor.last_known_mtime.set(mtime);
    let outliving_alert_shown = editor.settle_inline_notification_after_load();
    // A protected document keeps its installed body as the text source
    // (design D4) instead of a local-history baseline.
    let content = editor.retain_protected_source(content);
    editor.seed_local_history_from_guarded_loaded_content(content);
    restore_load_installation_state(editor, restore);
    editor.imp().load_state.set(EditorLoadState::Loaded);
    evidence::record_outcome(editor, LoadOutcome::Loaded);
    editor.notify_memory_policy_changed();
    editor.start_file_monitor();
    // A recovery alert that outlives the load keeps the editor's one alert
    // slot: it says where unsaved edits were kept, and the line-ending warning
    // returns on a later load once that alert is gone.
    if !outliving_alert_shown
        && editor
            .file_health()
            .iter()
            .any(|finding| finding.kind == FileHealthFindingKind::MixedLineEndings)
    {
        editor.emit_inline_notification_with_warning_action(
            InlineActionNotification {
                style: InlineNotificationStyle::Warning,
                title: "Mixed Line Endings Detected".to_string(),
                body: format!(
                    "This document opened with mixed line endings. Normalize future saves to {}.",
                    editor.save_line_ending().label()
                ),
                primary_button: Some("_Normalize…".to_string()),
                secondary_button: None,
            },
            PendingWarningAction::NormalizeLineEndings,
            InlineAlertLifetime::SupersededByLoad,
        );
    }
    editor.refresh_minimap();
    editor.refresh_accessibility_metadata();
    // The guard was restored above and the state is `Loaded`, so projections
    // that stood down during installation (the Markdown preview) re-render now.
    editor.fire_content_republished();
    editor.imp().load.load_failed_callback.borrow_mut().take();
    if let Some(callback) = editor.imp().load.load_completed_callback.take() {
        callback();
    }
    invoke_reentrant_callbacks(&editor.imp().load.file_loaded_callbacks);
    // Settle is read after the publish: its sentinel must follow it.
    editor.record_settle_mark();
}

/// Invoke every listener in `slot`, tolerating re-entrant registration.
///
/// Snapshots the list and drops the borrow before invoking, so a listener that
/// re-enters its `connect_*` registration (a `borrow_mut`) cannot panic the GTK
/// thread. Listeners registered during invocation land in the temporarily-empty
/// live vec and are appended after the originals, preserving invocation order.
pub(super) fn invoke_reentrant_callbacks(slot: &RefCell<Vec<EditorCallback>>) {
    let callbacks = std::mem::take(&mut *slot.borrow_mut());
    for callback in &callbacks {
        callback();
    }
    let mut slot = slot.borrow_mut();
    let newly_registered = std::mem::replace(&mut *slot, callbacks);
    slot.extend(newly_registered);
}

/// Restore the projections and view flags one installation suspended.
pub(super) fn restore_load_installation_state(
    editor: &LushtextEditorPage,
    restore: LoadInstallationState,
) {
    editor
        .imp()
        .load
        .projection_suspended
        .set(restore.projection_suspended);
    editor.set_local_history_capture_suppressed(restore.history_capture_suppressed);
    editor.set_minimap_tracking_suspended(restore.minimap_tracking_suspended);
    editor.unlock_edits(restore.lock);
    editor
        .source_view()
        .set_cursor_visible(restore.view.cursor_visible);
    editor.reassert_protected_view_properties();
    if editor.is_search_visible() && editor.search_bar().search_context().is_none() {
        editor.attach_search_context();
    }
    editor.refresh_minimap();
}

/// Release finalization ownership and start only the newest deferred reload.
///
/// Every parked request leaves here with its background planning owner either
/// carried into the restart or released. Dropping that owner silently would
/// strand whoever is waiting on the terminal — the session-restore sequencer
/// counts exactly these releases to decide when to open the next document.
pub(super) fn finish_load_finalization(
    editor: &LushtextEditorPage,
    permit: Option<TransientLoadPermit>,
) {
    editor.imp().load.finalizing.set(false);
    let disposed = editor.imp().load.dispose_during_finalization.replace(false);
    let pending = admission::take_pending_request(editor);
    drop(permit);
    match pending {
        Some(pending) if !disposed => admission::resume_pending_request(editor, pending),
        Some(pending) => pending.finish_planning(),
        None => {}
    }
}

/// Actuation seams that drive steps reachable only through a worker completion.
///
/// These are preserved, not retired: the stale-generation gate and the
/// post-install size policy are only otherwise reachable by winning a race with
/// a background read, or by loading tens of megabytes through `GtkTextBuffer`
/// just to cross a threshold. The convention's deferred actuation category
/// covers exactly this shape.
#[cfg(feature = "test-utils")]
impl LushtextEditorPage {
    /// Apply a synthetic load result through the production stale-generation gate.
    #[must_use]
    pub fn apply_load_result_for_test(
        &self,
        load_generation: u64,
        result: Result<editor_io::LoadResult, EditorLoadError>,
    ) -> bool {
        self.refresh_load_token_for_test(load_generation);
        apply_load_result_if_current(self, load_generation, result, EditorLoadState::Failed)
    }

    /// Apply a synthetic reload failure that preserves the prior loaded buffer.
    #[must_use]
    pub fn apply_reload_error_for_test(
        &self,
        load_generation: u64,
        error: EditorLoadError,
    ) -> bool {
        self.refresh_load_token_for_test(load_generation);
        apply_load_result_if_current(self, load_generation, Err(error), EditorLoadState::Loaded)
    }

    /// Clear a cancellation the test's own setup left on the current request.
    fn refresh_load_token_for_test(&self, load_generation: u64) {
        if self.imp().load_tracking.generation.get() == load_generation {
            self.imp()
                .load_tracking
                .cancel_token
                .replace(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                    false,
                )));
        }
    }

    /// Apply the same post-load size policy without blocking on a giant fixture.
    ///
    /// # Panics
    ///
    /// Panics when the test process has already exhausted the guarded disposal
    /// capacity required to own the synthetic body.
    pub fn apply_loaded_content_for_test(&self, content: &str, reported_size: u64) {
        // The worker profiles the body; this test path does the same off the
        // load pipeline, as the analysis pass would.
        let profile = crate::model::paragraph_boundary::DocumentProfile::of_text(content);
        let content = content.to_string();
        let weight = u64::try_from(content.capacity()).unwrap_or(u64::MAX);
        let reservation = crate::ui::plain_disposal::try_reserve_for_gtk(weight)
            .expect("test load body should acquire disposal capacity");
        evidence::record_install_started(self, false);
        install_loaded_direct(
            self,
            GuardedLoadResult {
                metadata: editor_io::LoadMetadata {
                    size: reported_size,
                    profile,
                    canonical_path: self.canonical_file_path(),
                    mtime: self.imp().monitor.last_known_mtime.get(),
                    encoding_state: self.document_encoding_state(),
                    has_bom: self.has_bom(),
                    file_health: self.file_health(),
                    decode_round_trips: Some(!self.decode_is_inexact()),
                },
                content: reservation.own(content),
            },
            None,
        );
    }
}

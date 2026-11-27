// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination for one bounded whole-buffer replacement session.
//!
//! This module owns everything the pure policy beside it deliberately does not:
//! GTK source lifetime, the projection/editability guard, body ownership across
//! the worker boundary, the scheduled clear and install turns, supersession, and
//! exactly-once terminal cleanup. Every decision it makes about *what* to do
//! next is delegated to [`super::policy`]; what remains here is *how*.

use crate::ui::editor_page::edit_lock::EditLock;
use crate::ui::editor_page::protected_view::execution::ProtectedInstallSource;
use crate::ui::editor_page::protected_view::policy::WholeContentInstall;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::protected_fold::FoldedInstall;
use lushtext_model::document::buffer_replacement::{
    BufferReplacementMode, BufferReplacementPlan, SliceBudget, next_clear_char_count,
    next_slice_boundary,
};
use lushtext_model::document::paragraph_boundary::DocumentProfile;

use super::admission::{
    BufferReplacementRequest, FreshnessCheck, ReplacementBody, ReplacementSlot, TerminalCallback,
};
use super::evidence::{self, BufferReplacementTerminalDiagnostic};
use super::policy::{
    BufferClearState, BufferReplacementCancelReason, BufferReplacementMetrics,
    BufferReplacementOutcome, BufferReplacementTicket, CancelDisposition, ClearProgress,
    MutationProgress, ReplacementPhase, after_clear_slice, cancel_disposition,
    guard_restores_on_terminal, insertion_is_complete, turn_may_run,
};
// The Protected View install mapping and the test-only actuation seams name
// the workflow family; every caller supplies its own variant in its ticket.
use super::policy::BufferReplacementWorkflow;

/// Delay between scheduled turns. One millisecond, not an idle source: an idle
/// source can starve behind higher-priority work while the buffer is still
/// half-mutated.
const SLICE_TURN_DELAY: Duration = Duration::from_millis(1);

#[derive(Clone, Copy)]
enum ResourceRelease {
    Released,
    NotHeld,
}

impl ResourceRelease {
    const fn as_bool(self) -> bool {
        matches!(self, Self::Released)
    }
}

fn trace_terminal(
    outcome: &BufferReplacementOutcome,
    source_release: ResourceRelease,
    guard_release: ResourceRelease,
) {
    let (ticket, cancel_reason, metrics) = match outcome {
        BufferReplacementOutcome::Complete {
            ticket, metrics, ..
        } => (*ticket, None, *metrics),
        BufferReplacementOutcome::Cancelled {
            ticket,
            reason,
            metrics,
        } => (*ticket, Some(*reason), *metrics),
    };
    tracing::debug!(
        workflow = ?ticket.workflow,
        generation = ticket.generation.get(),
        cancel_reason = ?cancel_reason,
        slice_count = metrics.slice_count,
        cleared_characters = metrics.cleared_characters,
        inserted_bytes = metrics.inserted_bytes,
        peak_retained_bodies = metrics.peak_retained_bodies,
        source_released = source_release.as_bool(),
        guard_released = guard_release.as_bool(),
        "buffer replacement reached terminal cleanup"
    );
}

/// The editor projections one replacement suspends, captured for exact restore.
struct ReplacementGuard {
    lock: EditLock,
    cursor_visible: bool,
    highlight_syntax: bool,
    minimap_tracking_suspended: bool,
    history_capture_suppressed: bool,
    projection_suspended: bool,
    monitor_active: bool,
}

/// Install parameters and chunking state shared across clearing and installing steps.
struct InstallPayload {
    profile: Option<DocumentProfile>,
    budget: SliceBudget,
    fold_plan: Option<std::sync::Arc<lushtext_model::document::protected_fold::FoldPlan>>,
    folded: Option<FoldedInstall>,
    highlight: Option<crate::ui::editor_page::language_attach::HighlightChunks>,
}

/// Active step of a live replacement session before terminal cleanup.
enum ActiveStep {
    Clearing {
        body: Option<ReplacementBody>,
        payload: InstallPayload,
        mutation_started: bool,
        language_off_pending: bool,
    },
    Installing {
        body: Option<ReplacementBody>,
        payload: InstallPayload,
        byte_offset: usize,
        inserted_turns: u64,
    },
    ClearingCancelled {
        reason: BufferReplacementCancelReason,
    },
}

/// Algebraic lifecycle phase for one live replacement session.
#[derive(Default)]
enum SessionPhase {
    Active(Box<ActiveSession>),
    #[default]
    Terminal,
}

struct ActiveSession {
    step: ActiveStep,
    guard: Option<ReplacementGuard>,
    callback: Option<TerminalCallback>,
    source_id: Option<glib::SourceId>,
}

/// One live replacement's GTK-owned state.
pub(crate) struct BufferReplacementSession {
    editor: glib::WeakRef<LushtextEditorPage>,
    buffer: sourceview5::Buffer,
    pub(super) ticket: BufferReplacementTicket,
    is_current: FreshnessCheck,
    pub(super) metrics: BufferReplacementMetrics,
    phase: SessionPhase,
}

impl BufferReplacementSession {
    pub(super) fn phase_kind(&self) -> ReplacementPhase {
        match &self.phase {
            SessionPhase::Active(active) => match &active.step {
                ActiveStep::Clearing { .. } => ReplacementPhase::Clearing,
                ActiveStep::Installing { .. } => ReplacementPhase::Installing,
                ActiveStep::ClearingCancelled { .. } => ReplacementPhase::ClearingCancelled,
            },
            SessionPhase::Terminal => ReplacementPhase::Finalizing,
        }
    }

    pub(super) fn mutation_started(&self) -> bool {
        match &self.phase {
            SessionPhase::Active(active) => match &active.step {
                ActiveStep::Clearing {
                    mutation_started, ..
                } => *mutation_started,
                ActiveStep::Installing { .. } | ActiveStep::ClearingCancelled { .. } => true,
            },
            SessionPhase::Terminal => true,
        }
    }
}

/// The Protected View install kind of a replacement workflow: the
/// whole-content installs of a document body. Save formatting never runs on a
/// protected buffer (design D9) and eviction only clears.
fn protected_install_kind(workflow: BufferReplacementWorkflow) -> Option<WholeContentInstall> {
    match workflow {
        BufferReplacementWorkflow::DraftRecovery => Some(WholeContentInstall::DraftRecovery),
        BufferReplacementWorkflow::LocalHistoryRestore => {
            Some(WholeContentInstall::LocalHistoryRestore)
        }
        BufferReplacementWorkflow::LocalHistoryUndo => Some(WholeContentInstall::UndoRestore),
        BufferReplacementWorkflow::MemoryEviction | BufferReplacementWorkflow::SaveFormatting => {
            None
        }
        #[cfg(feature = "test-utils")]
        BufferReplacementWorkflow::Test => None,
    }
}

// --- stage 2: begin ---------------------------------------------------------

pub(super) fn begin(editor: &LushtextEditorPage, request: BufferReplacementRequest) {
    // A dense body is sliced at any size, each slice bounded by visible
    // characters too (`bound-post-open-settle`, D6.1).
    let budget = request
        .profile
        .as_ref()
        .map_or(SliceBudget::BYTES_ONLY, |profile| {
            SliceBudget::for_profile(profile.summary())
        });
    let fold_plan = request
        .profile
        .as_ref()
        .and_then(|profile| profile.fold_plan.clone());
    // A highlighted body above its language's attach-after-install threshold
    // goes in chunk by chunk with its language attached
    // (`chunked-highlight-attach`): attaching it to the whole body at the
    // terminal would analyse it in 30 ms worker slices (A69).
    let highlight = request.profile.as_ref().and_then(|profile| {
        lushtext_model::document::highlight_chunking::highlight_install_order_with(
            editor.syntax_language(),
            profile.summary().total_bytes,
            editor.scale_for_incoming_profile(profile).syntax_enabled,
            lushtext_model::document::document_scale::markdown_chunked_cap(),
        )
        .is_chunked()
        .then(|| crate::ui::editor_page::language_attach::HighlightChunks::new(budget))
    });
    let plan = BufferReplacementPlan::for_text(
        editor.buffer().char_count(),
        request.body.len(),
        budget == SliceBudget::PACED
            || highlight.is_some()
            || super::policy::protected_install_is_sliced(fold_plan.as_deref()),
    );
    let budget = if highlight.is_some() {
        budget.with_highlight_cut()
    } else {
        budget
    };
    // A chunked body keeps the old text's highlighting until the first clear
    // turn (review S3): a session cancelled before it mutates anything then
    // leaves the language and its analysis exactly as they were.
    let guard = begin_guard(editor, highlight.is_some());
    // A sliced clear deletes from the start, and an attached language would
    // re-analyse the remainder after every slice: the first clear turn takes
    // it off (removing one costs nothing, A27).
    let language_off_pending =
        highlight.is_some() || (plan.clear_only && plan.mode == BufferReplacementMode::Sliced);
    if let Some(profile) = &request.profile {
        editor.prepare_for_incoming_profile(profile);
        if let Some(kind) = protected_install_kind(request.ticket.workflow) {
            editor.begin_protected_install(profile, ProtectedInstallSource::Replacement(kind));
        }
    }
    let buffer = editor.buffer();
    buffer.begin_irreversible_action();
    let folded = fold_plan
        .clone()
        .map(|plan| FoldedInstall::new(plan, buffer.upcast_ref(), true));
    let session = Rc::new(RefCell::new(BufferReplacementSession {
        editor: editor.downgrade(),
        buffer,
        ticket: request.ticket,
        is_current: request.is_current,
        metrics: BufferReplacementMetrics::for_one_retained_body(),
        phase: SessionPhase::Active(Box::new(ActiveSession {
            step: ActiveStep::Clearing {
                body: Some(request.body),
                payload: InstallPayload {
                    profile: request.profile,
                    budget,
                    fold_plan,
                    folded,
                    highlight,
                },
                mutation_started: false,
                language_off_pending,
            },
            guard: Some(guard),
            callback: Some(request.callback),
            source_id: None,
        })),
    }));
    *editor.imp().replacement.slot.borrow_mut() = ReplacementSlot::Running {
        active: Rc::clone(&session),
        pending: None,
    };
    let is_highlighted = matches!(
        &session.borrow().phase,
        SessionPhase::Active(active)
            if matches!(&active.step, ActiveStep::Clearing { payload, .. } if payload.highlight.is_some())
    );
    editor.set_replacement_chunked_owner(is_highlighted);
    editor.imp().replacement.slice_count.set(0);
    editor
        .imp()
        .replacement
        .protected_install
        .set(crate::ui::protected_fold::FoldedInstallMetrics::default());
    editor.imp().replacement.protected_clear_turns.set(0);
    match plan.mode {
        BufferReplacementMode::Direct => run_direct(&session),
        BufferReplacementMode::Sliced => schedule_turn(&session),
    }
}

fn begin_guard(editor: &LushtextEditorPage, keep_highlight: bool) -> ReplacementGuard {
    let imp = editor.imp();
    let view = editor.source_view();
    let buffer = editor.buffer();
    let guard = ReplacementGuard {
        // The buffer-level lock, not `set_editable(false)`: restoring
        // `editable` would invalidate every line of the replaced document
        // (edit_lock.rs).
        lock: editor.lock_edits(),
        cursor_visible: view.is_cursor_visible(),
        highlight_syntax: buffer.is_highlight_syntax(),
        minimap_tracking_suspended: imp.minimap.tracking_suspended.replace(true),
        history_capture_suppressed: editor.suspend_local_history_capture(),
        projection_suspended: imp.replacement.projection_suspended.replace(true),
        monitor_active: imp.monitor.file_monitor.borrow().is_some(),
    };
    editor.stop_file_monitor();
    editor.suspend_minimap_projection();
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
    if !keep_highlight {
        buffer.set_highlight_syntax(false);
    }
    editor.refresh_accessibility_metadata();
    guard
}

fn restore_guard(
    editor: &LushtextEditorPage,
    guard: ReplacementGuard,
    release: lushtext_model::document::held_input::ContentDisposition,
) {
    let imp = editor.imp();
    imp.replacement
        .projection_suspended
        .set(guard.projection_suspended);
    editor.set_local_history_capture_suppressed(guard.history_capture_suppressed);
    editor.set_minimap_tracking_suspended(guard.minimap_tracking_suspended);
    editor.unlock_edits(guard.lock, release);
    editor
        .source_view()
        .set_cursor_visible(guard.cursor_visible);
    editor.reassert_protected_view_properties();
    let syntax_enabled = editor.document_scale().syntax_enabled;
    {
        let _span =
            crate::ui::responsiveness_probe::PostOpenSpan::begin("post_open_highlight_attach");
        editor
            .buffer()
            .set_highlight_syntax(guard.highlight_syntax && syntax_enabled);
    }
    if editor.buffer().language().is_none() {
        // A cancelled install of a gated body cleared the language up front;
        // if the session still allows syntax, the path's language comes back.
        editor.request_language_attach();
    }
    if editor.is_search_visible() && editor.search_bar().search_context().is_none() {
        editor.attach_search_context();
    }
    if guard.monitor_active
        && editor.load_state() == crate::ui::editor_page::EditorLoadState::Loaded
        && !editor.is_evicted()
    {
        editor.start_file_monitor();
    }
    editor.refresh_minimap();
    editor.refresh_accessibility_metadata();
}

// --- stages 3-5: the bounded turns ------------------------------------------

fn schedule_turn(session: &Rc<RefCell<BufferReplacementSession>>) {
    let session_for_source = Rc::clone(session);
    let source_id = glib::timeout_add_local_once(SLICE_TURN_DELAY, move || {
        run_turn(&session_for_source);
    });
    let mut state = session.borrow_mut();
    match &mut state.phase {
        SessionPhase::Active(active) => active.source_id = Some(source_id),
        SessionPhase::Terminal => {
            drop(state);
            source_id.remove();
        }
    }
}

fn run_turn(session: &Rc<RefCell<BufferReplacementSession>>) {
    crate::ui::responsiveness_probe::span("replacement_turn", || run_turn_now(session));
}

fn run_turn_now(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (editor, phase, current, wait_for_tail_admission) = {
        let mut state = session.borrow_mut();
        let SessionPhase::Active(active) = &mut state.phase else {
            return;
        };
        active.source_id.take();
        let phase = match active.step {
            ActiveStep::Clearing { .. } => ReplacementPhase::Clearing,
            ActiveStep::Installing { .. } => ReplacementPhase::Installing,
            ActiveStep::ClearingCancelled { .. } => ReplacementPhase::ClearingCancelled,
        };
        let wait_for_tail_admission = matches!(
            &active.step,
            ActiveStep::Installing { payload, .. } if payload.fold_plan.is_some()
        );
        let editor = state.editor.upgrade();
        let current = editor
            .as_ref()
            .is_some_and(|editor| (state.is_current)(editor));
        #[cfg(feature = "test-utils")]
        let current = if let Some(editor) = editor.as_ref()
            && editor
                .imp()
                .replacement
                .stale_after_slices
                .get()
                .is_some_and(|limit| state.metrics.slice_count >= limit)
        {
            editor.imp().replacement.stale_after_slices.set(None);
            false
        } else {
            current
        };
        (editor, phase, current, wait_for_tail_admission)
    };
    let Some(_editor) = editor else {
        cancel_session(session, BufferReplacementCancelReason::Disposed, true);
        return;
    };
    if !turn_may_run(phase, current) {
        cancel_session(session, BufferReplacementCancelReason::Stale, false);
        return;
    }

    match phase {
        ReplacementPhase::Clearing => run_clear_turn(session),
        ReplacementPhase::Installing
            if wait_for_tail_admission && !crate::ui::protected_fold::tail_insert_admitted() =>
        {
            // Another document inserted a hidden tail in this iteration.
            schedule_turn(session);
        }
        ReplacementPhase::Installing => {
            if replacement_chunk_may_go_in(session) {
                run_install_turn(session);
            }
        }
        ReplacementPhase::ClearingCancelled => run_cancelled_clear_turn(session),
        ReplacementPhase::Finalizing => {}
    }
}

/// Delete one bounded slice: the last recorded ranges of a protected tail
/// while the editor's boundary ledger holds any (C3, design D4), then a slice
/// ending on a paragraph boundary. Returns whether the buffer is now empty
/// and the characters (or, for a ledger turn, bytes) deleted.
fn delete_one_slice(
    editor: Option<&LushtextEditorPage>,
    buffer: &sourceview5::Buffer,
    count: i32,
) -> (bool, u64) {
    if let Some(editor) = editor {
        let ledger = &editor.imp().protected_boundaries;
        let turn = crate::ui::editor_page::edit_lock::with_own_edit(|| {
            ledger.clear_turn(buffer.upcast_ref())
        });
        if let Some(turn) = turn {
            editor.imp().replacement.protected_clear_turns.set(
                editor
                    .imp()
                    .replacement
                    .protected_clear_turns
                    .get()
                    .saturating_add(1),
            );
            return (
                buffer.char_count() == 0 && ledger.is_empty(),
                turn.deleted_bytes,
            );
        }
    }
    if count == 0 {
        return (true, 0);
    }
    let mut start = buffer.start_iter();
    let mut end = start;
    let _ = end.forward_chars(count);
    // GTK text layout validates whole paragraphs, so a deletion that stops
    // inside a line would re-lay-out the shrinking remainder on every turn.
    // Extending to the next line start deletes each paragraph exactly once.
    if !end.is_end() && !end.starts_line() {
        let _ = end.forward_line();
    }
    let deleted = u64::try_from(end.offset()).unwrap_or(0);
    crate::ui::editor_page::edit_lock::with_own_edit(|| buffer.delete(&mut start, &mut end));
    (buffer.char_count() == 0, deleted)
}

/// Take the language off before the first deleted slice (review S3): a
/// chunked body also turns highlighting on for its attach and keeps it on at
/// the terminal.
fn take_language_off_before_clearing(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (editor, chunked) = {
        let mut state = session.borrow_mut();
        let editor = state.editor.upgrade();
        let SessionPhase::Active(active) = &mut state.phase else {
            return;
        };
        let ActiveStep::Clearing {
            payload,
            language_off_pending,
            ..
        } = &mut active.step
        else {
            return;
        };
        if !std::mem::take(language_off_pending) {
            return;
        }
        let chunked = payload.highlight.is_some();
        if chunked && let Some(guard) = active.guard.as_mut() {
            guard.highlight_syntax = true;
        }
        (editor, chunked)
    };
    let Some(editor) = editor else {
        return;
    };
    if chunked {
        editor.begin_chunked_highlight();
    } else {
        editor.cancel_language_attach();
        if editor.buffer().language().is_some() {
            editor.buffer().set_language(None::<&sourceview5::Language>);
        }
    }
}

fn run_clear_turn(session: &Rc<RefCell<BufferReplacementSession>>) {
    take_language_off_before_clearing(session);
    let (editor, buffer, count) = {
        let mut state = session.borrow_mut();
        let count = next_clear_char_count(state.buffer.char_count());
        if let SessionPhase::Active(active) = &mut state.phase
            && let ActiveStep::Clearing {
                mutation_started, ..
            } = &mut active.step
            && count > 0
        {
            // GtkTextBuffer emits `changed` synchronously. Establish ownership
            // before the call so a reentrant superseding request takes the
            // partial-mutation cleanup path.
            *mutation_started = true;
        }
        (state.editor.upgrade(), state.buffer.clone(), count)
    };
    let (cleared, deleted) = delete_one_slice(editor.as_ref(), &buffer, count);
    let progress = {
        let mut state = session.borrow_mut();
        let body_is_empty = match &state.phase {
            SessionPhase::Active(active) => match &active.step {
                ActiveStep::Clearing { body, .. } => body.as_deref().is_none_or(str::is_empty),
                _ => return,
            },
            SessionPhase::Terminal => return,
        };
        state.metrics.record_cleared_slice(deleted);
        let clear_state = if cleared {
            BufferClearState::Emptied
        } else {
            BufferClearState::Remaining
        };
        let progress = after_clear_slice(clear_state, body_is_empty);
        if progress == ClearProgress::BeginInstalling {
            let SessionPhase::Active(active) = &mut state.phase else {
                return;
            };
            let ActiveStep::Clearing { body, payload, .. } = std::mem::replace(
                &mut active.step,
                ActiveStep::ClearingCancelled {
                    reason: BufferReplacementCancelReason::Stale,
                },
            ) else {
                return;
            };
            active.step = ActiveStep::Installing {
                body,
                payload,
                byte_offset: 0,
                inserted_turns: 0,
            };
        }
        progress
    };
    match progress {
        ClearProgress::ContinueClearing | ClearProgress::BeginInstalling => schedule_turn(session),
        ClearProgress::Finish => finish_session(session, None),
    }
}

fn run_install_turn(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (editor, buffer, start, body, mut folded, budget, highlighted) = {
        let mut state = session.borrow_mut();
        let editor = state.editor.upgrade();
        let buffer = state.buffer.clone();
        let SessionPhase::Active(active) = &mut state.phase else {
            return;
        };
        let ActiveStep::Installing {
            body: body_slot,
            payload,
            byte_offset,
            ..
        } = &mut active.step
        else {
            return;
        };
        if payload.folded.is_some() && editor.is_none() {
            // A protected body goes in folded only, through the editor's
            // boundary ledger: with the editor gone, end the session rather
            // than insert its hidden tails as plain visible text.
            drop(state);
            cancel_session(session, BufferReplacementCancelReason::Disposed, true);
            return;
        }
        let Some(body) = body_slot.take() else {
            drop(state);
            cancel_session(session, BufferReplacementCancelReason::Stale, false);
            return;
        };
        (
            editor,
            buffer,
            *byte_offset,
            body,
            payload.folded.take(),
            payload.budget,
            payload.highlight.is_some(),
        )
    };
    if highlighted && let Some(editor) = editor.as_ref() {
        editor.reset_highlight_coverage();
    }
    let body_len = body.len();
    let end = crate::ui::editor_page::edit_lock::with_own_edit(|| {
        if let (Some(install), Some(editor)) = (folded.as_mut(), editor.as_ref()) {
            let append = || buffer.end_iter();
            let end = install.step(&append, &body, budget, &editor.imp().protected_boundaries);
            editor
                .imp()
                .replacement
                .protected_install
                .set(install.metrics());
            end.ok()
        } else {
            let end = next_slice_boundary(&body, start, budget);
            buffer.insert(&mut buffer.end_iter(), &body[start..end]);
            Some(end)
        }
    });
    let Some(end) = end else {
        // The sliced protected install broke (`FoldedInstallBroken`): cancel
        // the session, which clears the partial buffer and hands the body back
        // to its caller, rather than finish with a document that is not it.
        {
            let mut state = session.borrow_mut();
            if let SessionPhase::Active(active) = &mut state.phase
                && let ActiveStep::Installing {
                    body: body_slot,
                    payload,
                    ..
                } = &mut active.step
            {
                payload.folded = folded;
                *body_slot = Some(body);
            } else {
                drop(state);
                body.return_on_cancel();
                return;
            }
        }
        cancel_session(session, BufferReplacementCancelReason::Stale, false);
        return;
    };
    let protected = folded.is_some();

    {
        let mut state = session.borrow_mut();
        // A terminal session's metrics were already copied out and reported, so
        // this turn must not append to them; a merely superseded one is still
        // live and its insertion is real work that boundedness evidence owes.
        if matches!(state.phase, SessionPhase::Terminal) {
            drop(state);
            body.return_on_cancel();
            return;
        }
        state.metrics.record_inserted_slice(end);
        let SessionPhase::Active(active) = &mut state.phase else {
            drop(state);
            body.return_on_cancel();
            return;
        };
        let ActiveStep::Installing {
            body: body_slot,
            payload,
            byte_offset,
            inserted_turns,
        } = &mut active.step
        else {
            drop(state);
            body.return_on_cancel();
            return;
        };
        payload.folded = folded;
        *byte_offset = end;
        *inserted_turns = inserted_turns.saturating_add(1);
        *body_slot = Some(body);
    }
    if insertion_is_complete(end, body_len) {
        finish_session(session, None);
    } else {
        use crate::ui::editor_page::load::policy::{
            InstallPacingFacts, InstallSlicePacing, install_slice_pacing,
        };
        let laid_out = editor
            .as_ref()
            .is_some_and(LushtextEditorPage::install_follows_layout);
        let paced = budget == SliceBudget::PACED || highlighted;
        match install_slice_pacing(InstallPacingFacts {
            dense: paced,
            laid_out,
            protected,
        }) {
            InstallSlicePacing::BehindValidation => {
                schedule_paced_turn(session, crate::ui::editor_page::load::PACED_SLICE_PRIORITY);
            }
            InstallSlicePacing::BehindVisibleWork => {
                schedule_paced_turn(
                    session,
                    crate::ui::editor_page::load::background_slice_priority(),
                );
            }
            InstallSlicePacing::Timeout => schedule_turn(session),
        }
    }
}

/// The chunked highlighted install's gate before an insertion turn
/// (`model::highlight_chunking::chunk_gate`, shared with the load through
/// `LushtextEditorPage::decide_highlight_chunk`): `true` when the turn may
/// insert now; otherwise the session is parked until the attach resolves or
/// queued once at `G_PRIORITY_LOW + 1` behind the engine's worker.
fn replacement_chunk_may_go_in(session: &Rc<RefCell<BufferReplacementSession>>) -> bool {
    use crate::ui::editor_page::language_attach::{ChunkDecision, analysis_wait_priority};
    let Some(editor) = session.borrow().editor.upgrade() else {
        return true;
    };
    // Decide outside the borrow (review N1).
    let (Some(mut highlight), inserted) = ({
        let state = session.borrow();
        match &state.phase {
            SessionPhase::Active(active) => match &active.step {
                ActiveStep::Installing {
                    payload,
                    inserted_turns,
                    ..
                } => (payload.highlight, *inserted_turns),
                _ => (None, 0),
            },
            SessionPhase::Terminal => (None, 0),
        }
    }) else {
        return true;
    };
    let decision = editor.decide_highlight_chunk(&mut highlight, inserted);
    if let SessionPhase::Active(active) = &mut session.borrow_mut().phase
        && let ActiveStep::Installing { payload, .. } = &mut active.step
        && let Some(slot) = payload.highlight.as_mut()
    {
        *slot = highlight;
    }
    match decision {
        ChunkDecision::Insert => true,
        ChunkDecision::Unhighlighted(base) => {
            if let SessionPhase::Active(active) = &mut session.borrow_mut().phase
                && let ActiveStep::Installing { payload, .. } = &mut active.step
            {
                payload.budget = base;
                payload.highlight = None;
            }
            editor.set_replacement_chunked_owner(false);
            true
        }
        ChunkDecision::AwaitAttach => {
            let weak = Rc::downgrade(session);
            editor.when_language_attach_resolves(move || {
                if let Some(session) = weak.upgrade() {
                    resume_parked_turn(
                        &session,
                        crate::ui::editor_page::load::PACED_SLICE_PRIORITY,
                    );
                }
            });
            false
        }
        ChunkDecision::AwaitAnalysis => {
            resume_parked_turn(session, analysis_wait_priority());
            false
        }
    }
}

/// Queue a parked chunked session's next turn, unless it ended or was
/// rescheduled meanwhile.
fn resume_parked_turn(session: &Rc<RefCell<BufferReplacementSession>>, priority: glib::Priority) {
    let resumable = matches!(
        &session.borrow().phase,
        SessionPhase::Active(active)
            if active.source_id.is_none()
                && matches!(active.step, ActiveStep::Installing { .. })
    );
    if resumable {
        schedule_paced_turn(session, priority);
    }
}

/// Schedule a dense body's next insertion turn from an idle below
/// `GTK_TEXT_VIEW_PRIORITY_VALIDATE`, so GtkTextView's validation idle lays
/// out the previous slice first and each validation holds one slice's lines
/// (`bound-post-open-settle`, review R14, ledger A70). A turn waits for the
/// frame that shows the latest key first (`input_quiet::paced_idle`).
///
/// A background tab's view lays nothing out (`defer-background-tab-layout`),
/// so its turns wait at `load::background_slice_priority`, behind the visible
/// tab's (`load::policy::install_slice_pacing`).
fn schedule_paced_turn(session: &Rc<RefCell<BufferReplacementSession>>, priority: glib::Priority) {
    let (session_for_step, session_for_store) = (Rc::clone(session), Rc::downgrade(session));
    crate::ui::editor_page::load::queue_paced_slice(
        priority,
        move || run_turn(&session_for_step),
        move |source_id| {
            if let Some(session) = session_for_store.upgrade() {
                let mut state = session.borrow_mut();
                match &mut state.phase {
                    SessionPhase::Active(active) => active.source_id = Some(source_id),
                    SessionPhase::Terminal => {
                        drop(state);
                        source_id.remove();
                    }
                }
            }
        },
    );
}

fn run_direct(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (editor, buffer, body, fold_plan, current) = {
        let mut state = session.borrow_mut();
        let editor = state.editor.upgrade();
        let buffer = state.buffer.clone();
        let current = editor
            .as_ref()
            .is_some_and(|editor| (state.is_current)(editor));
        let SessionPhase::Active(active) = &mut state.phase else {
            return;
        };
        let ActiveStep::Clearing { body, payload, .. } = &mut active.step else {
            return;
        };
        (
            editor,
            buffer,
            body.take().unwrap_or_default(),
            payload.fold_plan.clone(),
            current,
        )
    };
    let restore_body = |body: ReplacementBody| {
        if let SessionPhase::Active(active) = &mut session.borrow_mut().phase
            && let ActiveStep::Clearing { body: slot, .. } = &mut active.step
        {
            *slot = Some(body);
        } else {
            body.return_on_cancel();
        }
    };
    if editor.is_none() {
        restore_body(body);
        cancel_session(session, BufferReplacementCancelReason::Disposed, true);
        return;
    }
    if !current {
        restore_body(body);
        cancel_session(session, BufferReplacementCancelReason::Stale, false);
        return;
    }
    let old_chars = buffer.char_count();
    if let SessionPhase::Active(active) = &mut session.borrow_mut().phase
        && let ActiveStep::Clearing {
            mutation_started, ..
        } = &mut active.step
    {
        *mutation_started = true;
    }
    crate::ui::editor_page::edit_lock::with_own_edit(|| match fold_plan.as_deref() {
        // A protected body: every folded tail goes in hidden, in this same
        // dispatch as its head (design D1).
        Some(plan) => {
            buffer.set_text("");
            let mut start = buffer.start_iter();
            crate::ui::protected_fold::insert_folded_document(
                buffer.upcast_ref(),
                &mut start,
                &body,
                plan,
            );
        }
        None => buffer.set_text(&body),
    });
    if let Some(editor) = editor.as_ref() {
        // The old text is gone, so the marks a sliced protected install left
        // are collapsed and go without merging anything (C3).
        editor
            .imp()
            .protected_boundaries
            .forget(buffer.upcast_ref());
    }
    let returned = {
        let mut state_ref = session.borrow_mut();
        let state = &mut *state_ref;
        match &mut state.phase {
            SessionPhase::Active(active) => match &mut active.step {
                ActiveStep::Clearing { body: slot, .. } => {
                    state.metrics.record_direct_replacement(
                        u64::try_from(old_chars.max(0)).unwrap_or(0),
                        body.len(),
                    );
                    *slot = Some(body);
                    None
                }
                _ => Some(body),
            },
            SessionPhase::Terminal => Some(body),
        }
    };
    if let Some(body) = returned {
        body.return_on_cancel();
        return;
    }
    finish_session(session, None);
}

// --- stage 6: cancellation --------------------------------------------------

pub(super) fn cancel_session(
    session: &Rc<RefCell<BufferReplacementSession>>,
    reason: BufferReplacementCancelReason,
    disposing: bool,
) {
    let (editor, source, mutation_started, body, highlighted, clear_partial) = {
        let mut state = session.borrow_mut();
        let editor = state.editor.upgrade();
        let SessionPhase::Active(active) = &mut state.phase else {
            return;
        };
        let source = active.source_id.take();
        let (body, highlighted, mutation_started) = match &mut active.step {
            ActiveStep::Clearing {
                body,
                payload,
                mutation_started,
                ..
            } => (
                body.take(),
                payload.highlight.take().is_some(),
                *mutation_started,
            ),
            ActiveStep::Installing { body, payload, .. } => {
                (body.take(), payload.highlight.take().is_some(), true)
            }
            ActiveStep::ClearingCancelled { .. } => (None, false, true),
        };
        let mutation = if mutation_started {
            MutationProgress::Started
        } else {
            MutationProgress::NotStarted
        };
        let clear_partial =
            cancel_disposition(disposing, mutation) == CancelDisposition::ClearPartialBuffer;
        if clear_partial {
            active.step = ActiveStep::ClearingCancelled { reason };
        }
        (
            editor,
            source,
            mutation_started,
            body,
            highlighted,
            clear_partial,
        )
    };
    if let Some(body) = body {
        body.return_on_cancel();
    }
    if let Some(source) = source {
        source.remove();
    }
    if highlighted && let Some(editor) = editor {
        editor.set_replacement_chunked_owner(false);
        // Only partial chunks need the language off before their clear: a
        // session that mutated nothing leaves the old text's language as
        // it was (review S3).
        if mutation_started {
            editor.begin_chunked_highlight();
        }
    }
    if clear_partial {
        schedule_turn(session);
    } else {
        finish_session(session, Some(reason));
    }
}

fn run_cancelled_clear_turn(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (editor, buffer) = {
        let state = session.borrow();
        (state.editor.upgrade(), state.buffer.clone())
    };
    let count = next_clear_char_count(buffer.char_count());
    let (cleared, deleted) = delete_one_slice(editor.as_ref(), &buffer, count);
    let finish_reason = {
        let mut state = session.borrow_mut();
        let SessionPhase::Active(active) = &state.phase else {
            return;
        };
        let ActiveStep::ClearingCancelled { reason } = &active.step else {
            return;
        };
        let reason = *reason;
        state.metrics.record_cleared_slice(deleted);
        cleared.then_some(reason)
    };
    if let Some(reason) = finish_reason {
        finish_session(session, Some(reason));
    } else {
        schedule_turn(session);
    }
}

// --- stage 7: terminal ------------------------------------------------------

fn finish_session(
    session: &Rc<RefCell<BufferReplacementSession>>,
    cancellation: Option<BufferReplacementCancelReason>,
) {
    let (editor, buffer, ticket, metrics, old_phase) = {
        let mut state = session.borrow_mut();
        let old_phase = std::mem::take(&mut state.phase);
        if matches!(old_phase, SessionPhase::Terminal) {
            return;
        }
        (
            state.editor.upgrade(),
            state.buffer.clone(),
            state.ticket,
            state.metrics,
            old_phase,
        )
    };
    let SessionPhase::Active(active) = old_phase else {
        return;
    };
    let ActiveSession {
        step,
        guard,
        callback,
        source_id: source,
    } = *active;
    let (body, profile, mutated) = match step {
        ActiveStep::Clearing {
            body,
            payload,
            mutation_started,
            ..
        } => (body, payload.profile, mutation_started),
        ActiveStep::Installing { body, payload, .. } => (body, payload.profile, true),
        ActiveStep::ClearingCancelled { .. } => (None, None, true),
    };
    if let Some(source) = source {
        source.remove();
    }
    buffer.end_irreversible_action();
    // What the buffer holds now, for text typed meanwhile (`held_input.rs`):
    // a whole new body, the untouched old one, or a cleared one.
    let release = match (cancellation, mutated) {
        (None, _) => lushtext_model::document::held_input::ContentDisposition::WholeContent,
        (Some(_), true) => lushtext_model::document::held_input::ContentDisposition::Unpublished,
        (Some(_), false) => lushtext_model::document::held_input::ContentDisposition::Unchanged,
    };
    let outcome = match cancellation {
        None => {
            let body = body
                .map_or_default(|body| match &editor {
                    Some(editor) => body.retain_for_protected_view(editor),
                    None => body,
                })
                .into_completed_body();
            #[cfg(not(feature = "test-utils"))]
            drop(body);
            BufferReplacementOutcome::Complete {
                ticket,
                #[cfg(feature = "test-utils")]
                body,
                metrics,
            }
        }
        Some(reason) => {
            drop(body);
            BufferReplacementOutcome::Cancelled {
                ticket,
                reason,
                metrics,
            }
        }
    };
    let guard_release = if guard.is_some() {
        ResourceRelease::Released
    } else {
        ResourceRelease::NotHeld
    };
    trace_terminal(&outcome, ResourceRelease::Released, guard_release);
    let Some(editor) = editor else {
        if let Some(callback) = callback {
            callback(outcome);
        }
        return;
    };
    editor.set_replacement_chunked_owner(false);
    evidence::record_terminal(
        &editor,
        BufferReplacementTerminalDiagnostic {
            ticket,
            cancel_reason: cancellation,
            metrics,
            source_released: true,
            guard_released: guard.is_some(),
        },
    );
    if cancellation.is_none()
        && let Some(profile) = &profile
    {
        // The installed body's profile joins the session before the guard
        // restores highlighting, so a gated feature is never switched back on.
        editor.join_installed_profile(profile);
    }
    match cancellation {
        None => editor.imp().replacement.left_partial.set(false),
        Some(reason) if mutated && reason != BufferReplacementCancelReason::Disposed => {
            // The cancelled clear emptied the buffer: the emptiness is the
            // workflow's, not an edit of the user's, so the tab is not
            // modified, and a save may not write it over the file.
            buffer.set_modified(false);
            editor.imp().replacement.left_partial.set(true);
        }
        Some(_) => {}
    }
    if guard_restores_on_terminal(cancellation)
        && let Some(guard) = guard
    {
        restore_guard(&editor, guard, release);
        // The buffer is renderable again. A cancelled session with a parked
        // superseding request stays quiet: that request fires at its own
        // terminal, so rendering the cleared buffer now would be wasted.
        let superseded = cancellation.is_some() && editor.imp().replacement.has_pending();
        if !superseded {
            editor.fire_content_republished();
        }
    }
    if let Some(callback) = callback {
        callback(outcome);
    }
    editor
        .imp()
        .replacement
        .slice_count
        .set(metrics.slice_count);
    release_owner_and_start_pending(&editor, session);
}

fn release_owner_and_start_pending(
    editor: &LushtextEditorPage,
    session: &Rc<RefCell<BufferReplacementSession>>,
) {
    let pending = {
        let mut slot = editor.imp().replacement.slot.borrow_mut();
        let is_current = match &*slot {
            ReplacementSlot::Running { active, .. } => Rc::ptr_eq(active, session),
            ReplacementSlot::Idle => false,
        };
        if !is_current {
            return;
        }
        match std::mem::take(&mut *slot) {
            ReplacementSlot::Running { pending, .. } => pending,
            ReplacementSlot::Idle => None,
        }
    };
    if let Some(pending) = pending {
        begin(editor, *pending);
    }
}

// --- test-only actuation seams ---------------------------------------------
//
// Each drives a step reachable only through a caller workflow or a resumed
// slice turn, which is the programme-level deferred seam category. Counted, not
// grown.

#[cfg(feature = "test-utils")]
use super::policy::BufferReplacementTestOutcome;

#[cfg(feature = "test-utils")]
impl LushtextEditorPage {
    /// Drive one replacement through the production admission path.
    pub fn replace_buffer_for_test(
        &self,
        body: String,
        generation: u64,
        current: Rc<std::cell::Cell<bool>>,
        outcomes: Rc<RefCell<Vec<BufferReplacementTestOutcome>>>,
    ) {
        self.replace_buffer_bounded(BufferReplacementRequest::new(
            BufferReplacementTicket {
                workflow: BufferReplacementWorkflow::Test,
                generation: lushtext_model::values::BufferReplacementGeneration::new(generation),
            },
            body,
            move |_| current.get(),
            move |outcome| outcomes.borrow_mut().push(outcome.into_test_outcome()),
        ));
    }

    /// Drive one replacement that hands its uninstalled body back on cancel.
    pub fn replace_buffer_returning_cancelled_body_for_test(
        &self,
        body: String,
        generation: u64,
        current: Rc<std::cell::Cell<bool>>,
        outcomes: Rc<RefCell<Vec<BufferReplacementTestOutcome>>>,
        cancelled_bodies: Rc<RefCell<Vec<String>>>,
        profile_body: bool,
    ) {
        // `profile_body` carries the body's paragraph profile, as every
        // production body-installing replacement does (draft recovery, local
        // history, mirror-back), so the chunked highlighted install applies.
        let profile = profile_body
            .then(|| lushtext_model::document::paragraph_boundary::DocumentProfile::of_text(&body));
        let request = BufferReplacementRequest::new_returning_body_on_cancel(
            BufferReplacementTicket {
                workflow: BufferReplacementWorkflow::Test,
                generation: lushtext_model::values::BufferReplacementGeneration::new(generation),
            },
            body,
            move |_| current.get(),
            move |outcome| outcomes.borrow_mut().push(outcome.into_test_outcome()),
            move |body| cancelled_bodies.borrow_mut().push(body),
        );
        self.replace_buffer_bounded(match profile {
            Some(profile) => request.with_profile(profile),
            None => request,
        });
    }

    /// Exercise the same teardown path window disposal uses.
    pub fn dispose_buffer_replacement_for_test(&self) {
        self.cancel_buffer_replacement_for_dispose();
    }

    /// Make the caller's freshness check fail after `slices` completed turns.
    pub fn make_buffer_replacement_stale_after_slices_for_test(&self, slices: u64) {
        self.imp().replacement.stale_after_slices.set(Some(slices));
    }
}

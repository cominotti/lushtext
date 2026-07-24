// SPDX-License-Identifier: GPL-3.0-or-later

//! One editor-owned bounded whole-buffer mutation session.
//!
//! Callers retain workflow semantics in freshness and terminal callbacks. This
//! module owns GTK source lifetime, projection suppression, body ownership,
//! bounded clear/insert turns, supersession, and exact terminal cleanup.

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::model::buffer_replacement::{
    BufferReplacementMode, BufferReplacementPlan, next_clear_char_count, next_replacement_boundary,
};

use super::LushtextEditorPage;

type FreshnessCheck = Box<dyn Fn(&LushtextEditorPage) -> bool>;
type TerminalCallback = Box<dyn FnOnce(BufferReplacementOutcome)>;
/// Callback that receives a guarded (disposal-owned) source body back.
type GuardedBodyCallback = Box<dyn FnOnce(crate::ui::plain_disposal::DisposalOwned<String>)>;
/// Callback that receives a plain source body back (test-only cancel probe).
#[cfg(feature = "test-utils")]
type PlainBodyCallback = Box<dyn FnOnce(String)>;

/// The uninstalled source body paired with the workflow callbacks matched to
/// its kind.
///
/// Keeping each callback *inside* the body variant it belongs to makes a
/// guarded cancellation callback structurally impossible to attach to a plain
/// body (and a plain callback impossible to attach to a guarded body): the only
/// constructors that accept a guarded callback also demand a guarded body. As a
/// result terminal teardown matches every legal pairing exhaustively with no
/// runtime panic arm for an unrepresentable mismatch.
enum ReplacementBody {
    Plain {
        body: String,
        /// Test-only probe returning the uninstalled plain body on cancel.
        #[cfg(feature = "test-utils")]
        on_cancel: Option<PlainBodyCallback>,
    },
    Guarded {
        body: crate::ui::plain_disposal::DisposalOwned<String>,
        /// Returns the uninstalled guarded body on cancel/supersede, keeping its
        /// disposal reservation intact.
        on_cancel: Option<GuardedBodyCallback>,
        /// Preserves the installed guarded source for an accepted workflow cache.
        on_complete: Option<GuardedBodyCallback>,
    },
}

impl Default for ReplacementBody {
    fn default() -> Self {
        Self::Plain {
            body: String::new(),
            #[cfg(feature = "test-utils")]
            on_cancel: None,
        }
    }
}

impl Deref for ReplacementBody {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Plain { body, .. } => body,
            Self::Guarded { body, .. } => body,
        }
    }
}

impl ReplacementBody {
    /// Return this uninstalled body to its workflow owner on cancel/supersede.
    ///
    /// A guarded completion callback is intentionally dropped uncalled here; a
    /// guarded body with no cancel callback disposes itself through
    /// `DisposalOwned`.
    fn return_on_cancel(self) {
        match self {
            #[cfg(feature = "test-utils")]
            Self::Plain { body, on_cancel } => {
                if let Some(callback) = on_cancel {
                    callback(body);
                }
            }
            #[cfg(not(feature = "test-utils"))]
            Self::Plain { body } => drop(body),
            Self::Guarded {
                body,
                on_cancel,
                on_complete: _,
            } => match on_cancel {
                Some(callback) => callback(body),
                None => drop(body),
            },
        }
    }

    /// Finalize an accepted replacement: route a guarded body to its completion
    /// callback and yield the retained plain body for terminal evidence.
    fn into_completed_body(self) -> String {
        match self {
            #[cfg(feature = "test-utils")]
            Self::Plain { body, on_cancel: _ } => body,
            #[cfg(not(feature = "test-utils"))]
            Self::Plain { body } => body,
            Self::Guarded {
                body,
                on_cancel: _,
                on_complete,
            } => {
                match on_complete {
                    Some(callback) => callback(body),
                    None => drop(body),
                }
                String::new()
            }
        }
    }
}

/// Workflow family that owns one replacement ticket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferReplacementWorkflow {
    MemoryEviction,
    DraftRecovery,
    LocalHistoryRestore,
    LocalHistoryUndo,
    SaveFormatting,
    #[cfg(feature = "test-utils")]
    Test,
}

/// Caller-owned freshness identity carried through every scheduled turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferReplacementTicket {
    pub workflow: BufferReplacementWorkflow,
    pub generation: u64,
}

/// Why one replacement stopped without publishing successful terminal state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferReplacementCancelReason {
    Stale,
    Superseded,
    Disposed,
}

/// Scalar boundedness and cleanup evidence for one replacement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BufferReplacementMetrics {
    pub slice_count: u64,
    pub cleared_characters: u64,
    pub inserted_bytes: usize,
    pub peak_retained_bodies: usize,
}

/// Exact terminal result delivered once to the workflow owner.
#[derive(Debug)]
pub enum BufferReplacementOutcome {
    Complete {
        ticket: BufferReplacementTicket,
        #[cfg(feature = "test-utils")]
        body: String,
        metrics: BufferReplacementMetrics,
    },
    Cancelled {
        ticket: BufferReplacementTicket,
        reason: BufferReplacementCancelReason,
        metrics: BufferReplacementMetrics,
    },
}

impl BufferReplacementOutcome {
    fn trace_terminal(&self, source_released: bool, guard_released: bool) {
        let (ticket, cancel_reason, metrics) = match self {
            Self::Complete {
                ticket, metrics, ..
            } => (*ticket, None, *metrics),
            Self::Cancelled {
                ticket,
                reason,
                metrics,
            } => (*ticket, Some(*reason), *metrics),
        };
        tracing::debug!(
            workflow = ?ticket.workflow,
            generation = ticket.generation,
            cancel_reason = ?cancel_reason,
            slice_count = metrics.slice_count,
            cleared_characters = metrics.cleared_characters,
            inserted_bytes = metrics.inserted_bytes,
            peak_retained_bodies = metrics.peak_retained_bodies,
            source_released,
            guard_released,
            "buffer replacement reached terminal cleanup"
        );
    }
}

/// One complete replacement request; pending ownership never contains GTK objects.
///
/// The body kind and its cancellation/completion callbacks are paired by
/// construction: every constructor that accepts a guarded callback also demands
/// a guarded body, so a guarded callback cannot reach a plain body (or the
/// reverse). There is no builder that could later cross those kinds.
pub struct BufferReplacementRequest {
    ticket: BufferReplacementTicket,
    body: ReplacementBody,
    is_current: FreshnessCheck,
    callback: TerminalCallback,
}

impl BufferReplacementRequest {
    /// Plain-body replacement with no body-return callback (e.g. eviction).
    pub fn new(
        ticket: BufferReplacementTicket,
        body: String,
        is_current: impl Fn(&LushtextEditorPage) -> bool + 'static,
        callback: impl FnOnce(BufferReplacementOutcome) + 'static,
    ) -> Self {
        Self {
            ticket,
            body: ReplacementBody::Plain {
                body,
                #[cfg(feature = "test-utils")]
                on_cancel: None,
            },
            is_current: Box::new(is_current),
            callback: Box::new(callback),
        }
    }

    /// Plain-body replacement that returns the uninstalled body when cancelled.
    #[cfg(feature = "test-utils")]
    pub fn new_returning_body_on_cancel(
        ticket: BufferReplacementTicket,
        body: String,
        is_current: impl Fn(&LushtextEditorPage) -> bool + 'static,
        callback: impl FnOnce(BufferReplacementOutcome) + 'static,
        on_cancel: impl FnOnce(String) + 'static,
    ) -> Self {
        Self {
            ticket,
            body: ReplacementBody::Plain {
                body,
                on_cancel: Some(Box::new(on_cancel)),
            },
            is_current: Box::new(is_current),
            callback: Box::new(callback),
        }
    }

    /// Guarded-body replacement that returns the uninstalled guarded body on
    /// cancel without releasing its disposal reservation.
    pub(crate) fn new_guarded_returning_body_on_cancel(
        ticket: BufferReplacementTicket,
        body: crate::ui::plain_disposal::DisposalOwned<String>,
        is_current: impl Fn(&LushtextEditorPage) -> bool + 'static,
        callback: impl FnOnce(BufferReplacementOutcome) + 'static,
        on_cancel: impl FnOnce(crate::ui::plain_disposal::DisposalOwned<String>) + 'static,
    ) -> Self {
        Self {
            ticket,
            body: ReplacementBody::Guarded {
                body,
                on_cancel: Some(Box::new(on_cancel)),
                on_complete: None,
            },
            is_current: Box::new(is_current),
            callback: Box::new(callback),
        }
    }

    /// Guarded-body replacement that preserves the installed guarded source for
    /// an accepted workflow cache when the replacement completes.
    pub(crate) fn new_guarded_returning_body_on_complete(
        ticket: BufferReplacementTicket,
        body: crate::ui::plain_disposal::DisposalOwned<String>,
        is_current: impl Fn(&LushtextEditorPage) -> bool + 'static,
        callback: impl FnOnce(BufferReplacementOutcome) + 'static,
        on_complete: impl FnOnce(crate::ui::plain_disposal::DisposalOwned<String>) + 'static,
    ) -> Self {
        Self {
            ticket,
            body: ReplacementBody::Guarded {
                body,
                on_cancel: None,
                on_complete: Some(Box::new(on_complete)),
            },
            is_current: Box::new(is_current),
            callback: Box::new(callback),
        }
    }
}

#[derive(Clone, Copy)]
struct ReplacementGuard {
    editable: bool,
    cursor_visible: bool,
    highlight_syntax: bool,
    minimap_tracking_suspended: bool,
    history_capture_suppressed: bool,
    projection_suspended: bool,
    monitor_active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplacementPhase {
    Clearing,
    Installing,
    ClearingCancelled,
    Finalizing,
}

pub(crate) struct BufferReplacementSession {
    editor: glib::WeakRef<LushtextEditorPage>,
    buffer: sourceview5::Buffer,
    ticket: BufferReplacementTicket,
    body: Option<ReplacementBody>,
    byte_offset: usize,
    is_current: FreshnessCheck,
    callback: Option<TerminalCallback>,
    source_id: Option<glib::SourceId>,
    guard: Option<ReplacementGuard>,
    phase: ReplacementPhase,
    cancel_reason: Option<BufferReplacementCancelReason>,
    mutation_started: bool,
    terminal: bool,
    metrics: BufferReplacementMetrics,
}

/// Editor-owned active/latest state for all whole-buffer replacement workflows.
#[derive(Default)]
pub struct BufferReplacementState {
    pub(crate) active: RefCell<Option<Rc<RefCell<BufferReplacementSession>>>>,
    pending: RefCell<Option<BufferReplacementRequest>>,
    pub projection_suspended: std::cell::Cell<bool>,
    pub slice_count: std::cell::Cell<u64>,
    #[cfg(feature = "test-utils")]
    stale_after_slices: std::cell::Cell<Option<u64>>,
    #[cfg(feature = "test-utils")]
    last_terminal: RefCell<Option<BufferReplacementTerminalDiagnostic>>,
}

/// Stable terminal projection used by the headless editor replacement harness.
#[cfg(feature = "test-utils")]
#[derive(Debug, Eq, PartialEq)]
pub struct BufferReplacementTestOutcome {
    pub ticket: BufferReplacementTicket,
    pub body: Option<String>,
    pub cancel_reason: Option<BufferReplacementCancelReason>,
    pub metrics: BufferReplacementMetrics,
}

/// Content-free terminal evidence retained by one editor for workflow tests.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferReplacementTerminalDiagnostic {
    pub ticket: BufferReplacementTicket,
    pub cancel_reason: Option<BufferReplacementCancelReason>,
    pub metrics: BufferReplacementMetrics,
    pub source_released: bool,
    pub guard_released: bool,
}

fn schedule_slice(session: &Rc<RefCell<BufferReplacementSession>>) {
    let session_for_source = Rc::clone(session);
    let source_id = glib::timeout_add_local_once(Duration::from_millis(1), move || {
        run_slice(&session_for_source);
    });
    session.borrow_mut().source_id = Some(source_id);
}

fn run_slice(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (editor, phase, current) = {
        let mut state = session.borrow_mut();
        state.source_id.take();
        if state.terminal {
            return;
        }
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
        (editor, state.phase, current)
    };
    let Some(_editor) = editor else {
        cancel_session(session, BufferReplacementCancelReason::Disposed, true);
        return;
    };
    if phase != ReplacementPhase::ClearingCancelled && !current {
        cancel_session(session, BufferReplacementCancelReason::Stale, false);
        return;
    }

    match phase {
        ReplacementPhase::Clearing => run_clear_slice(session),
        ReplacementPhase::Installing => run_insert_slice(session),
        ReplacementPhase::ClearingCancelled => run_cancelled_clear_slice(session),
        ReplacementPhase::Finalizing => {}
    }
}

fn delete_one_slice(buffer: &sourceview5::Buffer, count: i32) -> (bool, u64) {
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
    buffer.delete(&mut start, &mut end);
    (buffer.char_count() == 0, deleted)
}

fn run_clear_slice(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (buffer, count) = {
        let mut state = session.borrow_mut();
        let count = next_clear_char_count(state.buffer.char_count());
        if count > 0 {
            // GtkTextBuffer emits `changed` synchronously. Establish ownership
            // before the call so a reentrant superseding request takes the
            // partial-mutation cleanup path.
            state.mutation_started = true;
        }
        (state.buffer.clone(), count)
    };
    let (cleared, deleted) = delete_one_slice(&buffer, count);
    {
        let mut state = session.borrow_mut();
        if state.terminal {
            return;
        }
        state.metrics.cleared_characters = state.metrics.cleared_characters.saturating_add(deleted);
        state.metrics.slice_count = state.metrics.slice_count.saturating_add(1);
        if state.phase != ReplacementPhase::Clearing {
            return;
        }
    }
    if !cleared {
        schedule_slice(session);
        return;
    }
    if session.borrow().body.as_deref().is_none_or(str::is_empty) {
        finish_session(session, None);
        return;
    }
    session.borrow_mut().phase = ReplacementPhase::Installing;
    schedule_slice(session);
}

fn run_insert_slice(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (buffer, start, body) = {
        let mut state = session.borrow_mut();
        let Some(body) = state.body.take() else {
            drop(state);
            cancel_session(session, BufferReplacementCancelReason::Stale, false);
            return;
        };
        state.mutation_started = true;
        (state.buffer.clone(), state.byte_offset, body)
    };
    let end = next_replacement_boundary(&body, start);
    let body_len = body.len();
    let mut iter = buffer.end_iter();
    buffer.insert(&mut iter, &body[start..end]);

    let current_phase = {
        let mut state = session.borrow_mut();
        if state.terminal {
            drop(state);
            body.return_on_cancel();
            return;
        }
        state.metrics.inserted_bytes = state.metrics.inserted_bytes.max(end);
        state.metrics.slice_count = state.metrics.slice_count.saturating_add(1);
        if state.phase != ReplacementPhase::Installing {
            drop(state);
            body.return_on_cancel();
            return;
        }
        state.byte_offset = end;
        state.body = Some(body);
        state.phase
    };
    if current_phase != ReplacementPhase::Installing {
        return;
    }
    if end == body_len {
        finish_session(session, None);
    } else {
        schedule_slice(session);
    }
}

fn run_direct(session: &Rc<RefCell<BufferReplacementSession>>) {
    let (editor, buffer, body, current) = {
        let mut state = session.borrow_mut();
        let editor = state.editor.upgrade();
        let current = editor
            .as_ref()
            .is_some_and(|editor| (state.is_current)(editor));
        let body = state.body.take().unwrap_or_default();
        (editor, state.buffer.clone(), body, current)
    };
    if editor.is_none() {
        session.borrow_mut().body = Some(body);
        cancel_session(session, BufferReplacementCancelReason::Disposed, true);
        return;
    }
    if !current {
        session.borrow_mut().body = Some(body);
        cancel_session(session, BufferReplacementCancelReason::Stale, false);
        return;
    }
    let old_chars = buffer.char_count();
    session.borrow_mut().mutation_started = true;
    buffer.set_text(&body);
    let mut state = session.borrow_mut();
    if state.terminal || state.phase != ReplacementPhase::Clearing {
        drop(state);
        body.return_on_cancel();
        return;
    }
    state.mutation_started = true;
    state.metrics.cleared_characters = u64::try_from(old_chars.max(0)).unwrap_or(0);
    state.metrics.inserted_bytes = body.len();
    state.body = Some(body);
    drop(state);
    finish_session(session, None);
}

fn cancel_session(
    session: &Rc<RefCell<BufferReplacementSession>>,
    reason: BufferReplacementCancelReason,
    disposing: bool,
) {
    let (source, mutation_started, body) = {
        let mut state = session.borrow_mut();
        if state.terminal || state.phase == ReplacementPhase::Finalizing {
            return;
        }
        let source = state.source_id.take();
        state.cancel_reason = Some(reason);
        let body = state.body.take();
        (source, state.mutation_started, body)
    };
    if let Some(body) = body {
        body.return_on_cancel();
    }
    if let Some(source) = source {
        source.remove();
    }
    if disposing || !mutation_started {
        finish_session(session, Some(reason));
        return;
    }
    session.borrow_mut().phase = ReplacementPhase::ClearingCancelled;
    schedule_slice(session);
}

fn run_cancelled_clear_slice(session: &Rc<RefCell<BufferReplacementSession>>) {
    let buffer = session.borrow().buffer.clone();
    let count = next_clear_char_count(buffer.char_count());
    let (cleared, deleted) = delete_one_slice(&buffer, count);
    {
        let mut state = session.borrow_mut();
        if state.terminal || state.phase != ReplacementPhase::ClearingCancelled {
            return;
        }
        state.metrics.cleared_characters = state.metrics.cleared_characters.saturating_add(deleted);
        state.metrics.slice_count = state.metrics.slice_count.saturating_add(1);
    }
    if cleared {
        let reason = session
            .borrow()
            .cancel_reason
            .unwrap_or(BufferReplacementCancelReason::Stale);
        finish_session(session, Some(reason));
    } else {
        schedule_slice(session);
    }
}

fn finish_session(
    session: &Rc<RefCell<BufferReplacementSession>>,
    cancellation: Option<BufferReplacementCancelReason>,
) {
    let (editor, buffer, source, guard, ticket, body, callback, metrics) = {
        let mut state = session.borrow_mut();
        if state.terminal {
            return;
        }
        state.terminal = true;
        state.phase = ReplacementPhase::Finalizing;
        (
            state.editor.upgrade(),
            state.buffer.clone(),
            state.source_id.take(),
            state.guard.take(),
            state.ticket,
            state.body.take(),
            state.callback.take(),
            state.metrics,
        )
    };
    if let Some(source) = source {
        source.remove();
    }
    buffer.end_irreversible_action();
    let outcome = if let Some(reason) = cancellation {
        drop(body);
        BufferReplacementOutcome::Cancelled {
            ticket,
            reason,
            metrics,
        }
    } else {
        let body = body.unwrap_or_default().into_completed_body();
        #[cfg(not(feature = "test-utils"))]
        drop(body);
        BufferReplacementOutcome::Complete {
            ticket,
            #[cfg(feature = "test-utils")]
            body,
            metrics,
        }
    };
    outcome.trace_terminal(true, guard.is_some());
    let Some(editor) = editor else {
        if let Some(callback) = callback {
            callback(outcome);
        }
        return;
    };
    #[cfg(feature = "test-utils")]
    editor
        .imp()
        .replacement
        .last_terminal
        .replace(Some(BufferReplacementTerminalDiagnostic {
            ticket,
            cancel_reason: cancellation,
            metrics,
            source_released: true,
            guard_released: guard.is_some(),
        }));
    if cancellation != Some(BufferReplacementCancelReason::Disposed)
        && let Some(guard) = guard
    {
        restore_guard(&editor, guard);
    }
    if let Some(callback) = callback {
        callback(outcome);
    }
    editor
        .imp()
        .replacement
        .slice_count
        .set(metrics.slice_count);
    clear_owner_and_start_pending(&editor, session);
}

fn begin_guard(editor: &LushtextEditorPage) -> ReplacementGuard {
    let imp = editor.imp();
    let view = editor.source_view();
    let buffer = editor.buffer();
    let guard = ReplacementGuard {
        editable: view.is_editable(),
        cursor_visible: view.is_cursor_visible(),
        highlight_syntax: buffer.is_highlight_syntax(),
        minimap_tracking_suspended: imp.minimap.tracking_suspended.replace(true),
        history_capture_suppressed: imp.local_history.automatic_capture_suppressed.replace(true),
        projection_suspended: imp.replacement.projection_suspended.replace(true),
        monitor_active: imp.monitor.file_monitor.borrow().is_some(),
    };
    editor.stop_file_monitor();
    editor.suspend_minimap_projection();
    if editor.search_bar().search_context().is_some() {
        editor.search_bar().detach();
    }
    view.set_editable(false);
    view.set_cursor_visible(false);
    buffer.set_highlight_syntax(false);
    editor.refresh_accessibility_metadata();
    guard
}

fn restore_guard(editor: &LushtextEditorPage, guard: ReplacementGuard) {
    let imp = editor.imp();
    imp.replacement
        .projection_suspended
        .set(guard.projection_suspended);
    imp.local_history
        .automatic_capture_suppressed
        .set(guard.history_capture_suppressed);
    editor.set_minimap_tracking_suspended(guard.minimap_tracking_suspended);
    editor.source_view().set_editable(guard.editable);
    editor
        .source_view()
        .set_cursor_visible(guard.cursor_visible);
    editor.buffer().set_highlight_syntax(guard.highlight_syntax);
    if editor.is_search_visible() && editor.search_bar().search_context().is_none() {
        editor
            .search_bar()
            .attach(&editor.buffer(), editor.source_view());
    }
    if guard.monitor_active
        && editor.load_state() == super::EditorLoadState::Loaded
        && !editor.is_evicted()
    {
        editor.start_file_monitor();
    }
    editor.refresh_minimap();
    editor.refresh_accessibility_metadata();
}

fn clear_owner_and_start_pending(
    editor: &LushtextEditorPage,
    session: &Rc<RefCell<BufferReplacementSession>>,
) {
    let is_current = editor
        .imp()
        .replacement
        .active
        .borrow()
        .as_ref()
        .is_some_and(|current| Rc::ptr_eq(current, session));
    if !is_current {
        return;
    }
    editor.imp().replacement.active.take();
    if let Some(pending) = editor.imp().replacement.pending.take() {
        editor.start_buffer_replacement(pending);
    }
}

impl LushtextEditorPage {
    /// Start or supersede one whole-buffer replacement.
    pub(crate) fn replace_buffer_bounded(&self, request: BufferReplacementRequest) {
        let active = { self.imp().replacement.active.borrow().clone() };
        if let Some(active) = active {
            cancel_session(&active, BufferReplacementCancelReason::Superseded, false);
            if self.imp().replacement.active.borrow().is_none() {
                self.start_buffer_replacement(request);
                return;
            }
            if let Some(replaced) = self.imp().replacement.pending.replace(Some(request)) {
                replaced.body.return_on_cancel();
                (replaced.callback)(BufferReplacementOutcome::Cancelled {
                    ticket: replaced.ticket,
                    reason: BufferReplacementCancelReason::Superseded,
                    metrics: BufferReplacementMetrics::default(),
                });
            }
            return;
        }
        self.start_buffer_replacement(request);
    }

    fn start_buffer_replacement(&self, request: BufferReplacementRequest) {
        let plan = BufferReplacementPlan::for_sizes(self.buffer().char_count(), request.body.len());
        let guard = begin_guard(self);
        let buffer = self.buffer();
        buffer.begin_irreversible_action();
        let session = Rc::new(RefCell::new(BufferReplacementSession {
            editor: self.downgrade(),
            buffer,
            ticket: request.ticket,
            body: Some(request.body),
            byte_offset: 0,
            is_current: request.is_current,
            callback: Some(request.callback),
            source_id: None,
            guard: Some(guard),
            phase: ReplacementPhase::Clearing,
            cancel_reason: None,
            mutation_started: false,
            terminal: false,
            metrics: BufferReplacementMetrics {
                peak_retained_bodies: 1,
                ..BufferReplacementMetrics::default()
            },
        }));
        self.imp()
            .replacement
            .active
            .replace(Some(Rc::clone(&session)));
        self.imp().replacement.slice_count.set(0);
        match plan.mode {
            BufferReplacementMode::Direct => run_direct(&session),
            BufferReplacementMode::Sliced => schedule_slice(&session),
        }
    }

    pub(crate) fn cancel_buffer_replacement_for_dispose(&self) {
        if let Some(pending) = self.imp().replacement.pending.take() {
            pending.body.return_on_cancel();
            (pending.callback)(BufferReplacementOutcome::Cancelled {
                ticket: pending.ticket,
                reason: BufferReplacementCancelReason::Disposed,
                metrics: BufferReplacementMetrics::default(),
            });
        }
        let active = { self.imp().replacement.active.borrow().clone() };
        if let Some(active) = active {
            cancel_session(&active, BufferReplacementCancelReason::Disposed, true);
        }
    }

    #[must_use]
    pub(crate) fn buffer_replacement_in_progress(&self) -> bool {
        self.imp().replacement.active.borrow().is_some()
    }

    #[must_use]
    pub(crate) fn buffer_replacement_projection_suspended(&self) -> bool {
        self.imp().replacement.projection_suspended.get()
    }

    #[cfg(feature = "test-utils")]
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
                generation,
            },
            body,
            move |_| current.get(),
            move |outcome| {
                let outcome = match outcome {
                    BufferReplacementOutcome::Complete {
                        ticket,
                        body,
                        metrics,
                    } => BufferReplacementTestOutcome {
                        ticket,
                        body: Some(body),
                        cancel_reason: None,
                        metrics,
                    },
                    BufferReplacementOutcome::Cancelled {
                        ticket,
                        reason,
                        metrics,
                    } => BufferReplacementTestOutcome {
                        ticket,
                        body: None,
                        cancel_reason: Some(reason),
                        metrics,
                    },
                };
                outcomes.borrow_mut().push(outcome);
            },
        ));
    }

    #[cfg(feature = "test-utils")]
    pub fn replace_buffer_returning_cancelled_body_for_test(
        &self,
        body: String,
        generation: u64,
        current: Rc<std::cell::Cell<bool>>,
        outcomes: Rc<RefCell<Vec<BufferReplacementTestOutcome>>>,
        cancelled_bodies: Rc<RefCell<Vec<String>>>,
    ) {
        self.replace_buffer_bounded(BufferReplacementRequest::new_returning_body_on_cancel(
            BufferReplacementTicket {
                workflow: BufferReplacementWorkflow::Test,
                generation,
            },
            body,
            move |_| current.get(),
            move |outcome| {
                let outcome = match outcome {
                    BufferReplacementOutcome::Complete {
                        ticket,
                        body,
                        metrics,
                    } => BufferReplacementTestOutcome {
                        ticket,
                        body: Some(body),
                        cancel_reason: None,
                        metrics,
                    },
                    BufferReplacementOutcome::Cancelled {
                        ticket,
                        reason,
                        metrics,
                    } => BufferReplacementTestOutcome {
                        ticket,
                        body: None,
                        cancel_reason: Some(reason),
                        metrics,
                    },
                };
                outcomes.borrow_mut().push(outcome);
            },
            move |body| cancelled_bodies.borrow_mut().push(body),
        ));
    }

    #[cfg(feature = "test-utils")]
    pub fn dispose_buffer_replacement_for_test(&self) {
        self.cancel_buffer_replacement_for_dispose();
    }

    #[cfg(feature = "test-utils")]
    pub fn make_buffer_replacement_stale_after_slices_for_test(&self, slices: u64) {
        self.imp().replacement.stale_after_slices.set(Some(slices));
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn buffer_replacement_in_progress_for_test(&self) -> bool {
        self.buffer_replacement_in_progress()
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn buffer_replacement_projection_suspended_for_test(&self) -> bool {
        self.buffer_replacement_projection_suspended()
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn buffer_replacement_slice_count_for_test(&self) -> u64 {
        self.imp().replacement.active.borrow().as_ref().map_or_else(
            || self.imp().replacement.slice_count.get(),
            |session| session.borrow().metrics.slice_count,
        )
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn buffer_replacement_terminal_diagnostic_for_test(
        &self,
    ) -> Option<BufferReplacementTerminalDiagnostic> {
        *self.imp().replacement.last_terminal.borrow()
    }
}

#[cfg(test)]
mod pairing_tests {
    //! Correct-by-construction proof for body-kind / callback-kind pairing.
    //!
    //! The illegal pairings — a guarded cancellation/completion callback with a
    //! plain body, or a plain callback with a guarded body — are
    //! *unconstructible*: every constructor that accepts a guarded callback
    //! (`new_guarded_returning_body_on_{cancel,complete}`) also requires a
    //! `DisposalOwned<String>` body, and the only plain constructors (`new`,
    //! `new_returning_body_on_cancel`) accept no guarded callback. No
    //! `BufferReplacementRequest` value can therefore pair a guarded callback
    //! with a plain body, which is why [`ReplacementBody::return_on_cancel`] and
    //! [`ReplacementBody::into_completed_body`] match every legal pairing
    //! exhaustively with no runtime panic arm. These tests exercise the plain
    //! routing (guarded routing needs the GTK disposal lane and is covered by
    //! the replacement widget tests).

    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[cfg(feature = "test-utils")]
    #[test]
    fn plain_body_returns_through_its_matched_cancel_callback() {
        let returned = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&returned);
        let body = ReplacementBody::Plain {
            body: "abc".to_string(),
            on_cancel: Some(Box::new(move |body| *sink.borrow_mut() = Some(body))),
        };
        body.return_on_cancel();
        assert_eq!(returned.borrow().as_deref(), Some("abc"));
    }

    #[test]
    fn plain_body_completion_yields_the_retained_body() {
        let body = ReplacementBody::Plain {
            body: "done".to_string(),
            #[cfg(feature = "test-utils")]
            on_cancel: None,
        };
        assert_eq!(body.into_completed_body(), "done");
    }

    #[test]
    fn default_body_is_an_empty_plain_placeholder() {
        // `Default`/`mem::take`/`unwrap_or_default` placeholder values must stay
        // representable without weakening the pairing guarantee.
        let body = ReplacementBody::default();
        assert!(body.is_empty());
        assert_eq!(body.into_completed_body(), "");
    }
}

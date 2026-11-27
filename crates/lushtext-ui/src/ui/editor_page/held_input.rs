// SPDX-License-Identifier: GPL-3.0-or-later

//! Held typing (`keep-keystrokes-during-install`): text the user types while a
//! workflow owns the document's buffer is kept and inserted afterwards,
//! instead of being refused and lost. **The user's text is never lost**: held
//! text either goes into the document or, when the document cannot take it,
//! into a new Untitled tab, and a close never drops it.
//!
//! **Cross-cutting, no role**, like `edit_lock.rs`, whose handlers call it:
//! the document load, the whole-buffer replacement and the save share it. Its
//! decisions are the pure `model::held_input`.
//!
//! - **Capture.** While the editor *holds input* (a workflow holds the edit
//!   lock, a load is in flight, or held text waits for its replay), an
//!   insertion typed at the caret inside a user action that deleted nothing is
//!   appended to the held text and its emission stopped. Backspace removes the
//!   last held character, and an input method's `delete-surrounding` inside
//!   the held text edits it (`surrounding_text.rs`). One user action is held
//!   whole or not at all: a paste that overflows the cap mid-action is
//!   truncated back to where the action began. Every other user edit is
//!   refused, as the lock refuses it, and reported by a rate-limited warning.
//!   The view's drop targets are off for the whole held window, so a MOVE
//!   drag's source never deletes text the buffer did not take.
//! - **Settle.** A dispatch one above `gtk_lush_tasks::completion_priority()`
//!   (above GtkTextView's line validation, ledger A70, and never in the same
//!   iteration as a task completion, A81), queued at the first held character,
//!   whenever the last lock holder releases, and whenever the editor's state
//!   changes while nothing owns the buffer (a load's terminal), decides with
//!   `settle_verdict`: wait (a workflow still owns the buffer, a load is in
//!   flight, a draft restore is pending, or an input-method composition is in
//!   progress, reset after `PREEDIT_DEADLINE_MS`: re-armed on a short
//!   timeout), replay, or rescue.
//! - **Replay.** One `insert_interactive` inside one user action: one undo
//!   action, like typing, an ordinary edit for the modified flag, drafts, the
//!   document-scale watch and every other handler. It goes at the caret the
//!   published document is shown with, or, when every release left the
//!   content as it was (a save), where the first character was held.
//! - **Rescue.** A document that cannot take the text (view-only, evicted, a
//!   failed or cancelled load, a buffer a workflow left partial) gets none of
//!   it: the text opens in a new Untitled tab, marked draft-dirty, through the
//!   Preserved Drafts "Open" path, and a notice says where it went.
//! - **Close.** `flush_held_text_for_close` settles held text synchronously
//!   when no workflow owns the buffer, so the Save Changes dialog sees a
//!   modified document; while one does, the close is refused.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::protected_view::interaction_execution::DropTargetHold;
use super::{EditorLoadState, LushtextEditorPage};
use crate::ui::signal_connect;
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;
use lushtext_model::document::held_input::{
    BufferWorkflowOwnership, CloseDisposition, ContentDisposition, HeldEpisode, HeldText,
    HeldTextPresence, HoldOutcome, InsertDisposition, InsertFacts, NoDocumentCause, RemovalFacts,
    ReplayAnchor, SettleFacts, SettleVerdict, close_disposition, insert_disposition,
    removal_disposition, settle_verdict,
};

/// How long a waiting settle dispatch sleeps before it decides again: 16 ms
/// (kind: polling interval, a power of two in milliseconds), about two
/// 120 Hz frames. Only what has no hook waits on it: a composition's end and a
/// draft restore that resolves without installing anything. A load's
/// terminal, a replacement's and a save's queue the dispatch directly.
const SETTLE_RETRY_INTERVAL_MS: u64 = 16;

/// The settle dispatch's priority: one above
/// `gtk_lush_tasks::completion_priority()` (123, against the completions'
/// 124), so two above GtkTextView's line validation (`GDK_PRIORITY_REDRAW + 5`,
/// ledger A70): the replay is not held back behind a document's validation
/// batches.
///
/// Not the completion priority itself: GLib dispatches every ready source of
/// the top priority in one iteration (A81), so at 124 a replay and a task
/// completion or load/save hand-off ready together would run back to back, and
/// their bounds add past `B_SETTLE`: a replay that enters Protected View took
/// up to 13.6 ms (design D8), a completion up to 4.8 ms
/// (`spawn-completion-priority`). At 123 the replay's iteration dispatches
/// nothing else of the application's, and the completion runs in the next one.
fn settle_priority() -> glib::Priority {
    use glib::translate::IntoGlib;
    let completion: i32 = gtk_lush_tasks::completion_priority().into_glib();
    glib::Priority::from(completion - 1)
}

/// How long a composition may hold the replay back before the input method
/// is reset: 1,024 ms (kind: deadline, a power of two in milliseconds). A
/// composition the user is in the middle of ends well within it; one an input
/// method never ends must not keep the text out of the document.
const PREEDIT_DEADLINE_MS: i64 = 1_024;

/// The status notices held typing publishes, one rate-limit slot each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeldNotice {
    /// The first held character of an episode (information).
    Holding,
    /// A user edit the held window cannot keep was refused.
    EditNotApplied,
    /// An insertion did not fit the held buffer whole.
    Overflow,
}

impl HeldNotice {
    const fn slot(self) -> usize {
        match self {
            Self::Holding => 0,
            Self::EditNotApplied => 1,
            Self::Overflow => 2,
        }
    }
}

/// What tests and automation can observe of held typing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldInputEvidence {
    /// Characters held now.
    pub held_chars: u64,
    /// Insertions held since the editor was created.
    pub holds: u64,
    /// Replays run.
    pub replays: u64,
    /// Characters inserted by replays.
    pub replayed_chars: u64,
    /// Episodes rescued into a new tab because the document was view-only.
    pub rescued_view_only: u64,
    /// Episodes rescued into a new tab because the buffer held no whole
    /// document (or the replay's insert was refused).
    pub rescued_no_document: u64,
    /// Characters rescued into new tabs.
    pub rescued_chars: u64,
    /// User edits refused while held (deletions, undo, redo, misplaced or
    /// tainted inserts).
    pub refused_edits: u64,
    /// User actions refused because they did not fit.
    pub overflows: u64,
    /// Input-method compositions reset at their deadline.
    pub preedit_resets: u64,
    /// Held text settled synchronously by a close.
    pub close_flushes: u64,
    /// Status notices requested (published when the editor is in a window).
    pub notices: u64,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum UserActionDisposition {
    #[default]
    Clean,
    Tainted,
    Overflowed,
}

#[derive(Clone, Copy, Default)]
enum UserActionPhase {
    #[default]
    Idle,
    Active {
        depth: u32,
        start_len: usize,
        disposition: UserActionDisposition,
    },
}

#[derive(Default)]
enum HeldEpisodePhase {
    #[default]
    Idle,
    Holding {
        text: HeldText,
        episode: HeldEpisode,
        first_hold: gtk4::TextMark,
        preedit_since: Option<i64>,
    },
}

#[derive(Default)]
enum HeldSettleSchedule {
    #[default]
    Unscheduled,
    RetryTimeout(glib::SourceId),
    ImmediateIdle(glib::SourceId),
}

impl HeldSettleSchedule {
    fn take_source(&mut self) -> Option<glib::SourceId> {
        match std::mem::take(self) {
            Self::Unscheduled => None,
            Self::RetryTimeout(id) | Self::ImmediateIdle(id) => Some(id),
        }
    }
}

/// One editor's held-typing state.
#[derive(Default)]
pub struct HeldInputState {
    episode: RefCell<HeldEpisodePhase>,
    user_action: Cell<UserActionPhase>,
    settle: RefCell<HeldSettleSchedule>,
    /// The view's drop targets are held for the held window.
    drop_targets_held: Cell<bool>,
    notice_times: Cell<[Option<i64>; 3]>,
    evidence: Cell<HeldInputEvidence>,
}

impl LushtextEditorPage {
    fn held_state(&self) -> &HeldInputState {
        &self.imp().held_input
    }

    /// Whether text the user types now is held rather than inserted: a
    /// workflow holds the edit lock, a load is in flight, or held text waits
    /// for its replay.
    #[must_use]
    pub fn holds_typed_input(&self) -> bool {
        self.edits_locked() || self.load_state() == EditorLoadState::Loading || self.has_held_text()
    }

    /// Whether held text waits for its replay.
    #[must_use]
    pub fn has_held_text(&self) -> bool {
        matches!(
            &*self.held_state().episode.borrow(),
            HeldEpisodePhase::Holding { text, .. } if !text.is_empty()
        )
    }

    /// Characters held now.
    #[must_use]
    pub fn held_char_count(&self) -> u64 {
        match &*self.held_state().episode.borrow() {
            HeldEpisodePhase::Idle => 0,
            HeldEpisodePhase::Holding { text, .. } => text.char_count(),
        }
    }

    /// Whether closing this editor would lose the user's text: the document
    /// is modified, or typed text is held for it. Every close collector asks
    /// this, never `is_modified` alone.
    #[must_use]
    pub fn has_unsaved_changes(&self) -> bool {
        self.is_modified() || self.has_held_text()
    }

    /// What held typing has done, for tests.
    #[must_use]
    pub fn held_input_evidence(&self) -> HeldInputEvidence {
        let mut evidence = self.held_state().evidence.get();
        evidence.held_chars = self.held_char_count();
        evidence
    }

    fn update_held_evidence(&self, update: impl FnOnce(&mut HeldInputEvidence)) {
        let state = self.held_state();
        let mut evidence = state.evidence.get();
        update(&mut evidence);
        state.evidence.set(evidence);
    }

    /// The held text, for the input method's surrounding text.
    pub(super) fn held_text_snapshot(&self) -> String {
        match &*self.held_state().episode.borrow() {
            HeldEpisodePhase::Idle => String::new(),
            HeldEpisodePhase::Holding { text, .. } => text.as_str().to_owned(),
        }
    }

    /// Apply an input method's `delete-surrounding` to the held text when it
    /// falls inside it. Returns whether it did.
    pub(super) fn delete_surrounding_in_held_text(&self, offset: i32, n_chars: i32) -> bool {
        if !self.has_held_text() || super::edit_lock::own_edit_in_progress() {
            return false;
        }
        let (applied, empty) = {
            let mut phase = self.held_state().episode.borrow_mut();
            let HeldEpisodePhase::Holding { text, .. } = &mut *phase else {
                return false;
            };
            let applied = text.delete_surrounding(offset, n_chars);
            (applied, text.is_empty())
        };
        if applied && empty {
            self.end_held_episode();
        }
        applied
    }

    /// Whether a workflow owns the buffer: the edit lock is held, a load is
    /// in flight, or a replacement or install still suspends the buffer's
    /// projections.
    fn workflow_owns_buffer(&self) -> bool {
        self.edits_locked()
            || self.load_state() == EditorLoadState::Loading
            || self.buffer_replacement_in_progress()
            || self.load_projection_suspended()
    }

    /// Track the buffer's user action (connected from `setup_edit_lock`,
    /// before any other buffer handler) and Backspace on held text.
    pub(super) fn setup_held_input_tracking(&self) {
        let buffer = self.buffer();
        let editor = self.downgrade();
        buffer.connect_begin_user_action(move |_| {
            if let Some(editor) = editor.upgrade() {
                let state = editor.held_state();
                let next = match state.user_action.get() {
                    UserActionPhase::Idle => {
                        let start_len = match &*state.episode.borrow() {
                            HeldEpisodePhase::Idle => 0,
                            HeldEpisodePhase::Holding { text, .. } => text.len_bytes(),
                        };
                        UserActionPhase::Active {
                            depth: 1,
                            start_len,
                            disposition: UserActionDisposition::Clean,
                        }
                    }
                    UserActionPhase::Active {
                        depth,
                        start_len,
                        disposition,
                    } => UserActionPhase::Active {
                        depth: depth.saturating_add(1),
                        start_len,
                        disposition,
                    },
                };
                state.user_action.set(next);
            }
        });
        let editor = self.downgrade();
        buffer.connect_end_user_action(move |_| {
            if let Some(editor) = editor.upgrade() {
                let state = editor.held_state();
                let next = match state.user_action.get() {
                    UserActionPhase::Active {
                        depth,
                        start_len,
                        disposition,
                    } if depth > 1 => UserActionPhase::Active {
                        depth: depth - 1,
                        start_len,
                        disposition,
                    },
                    UserActionPhase::Active { .. } | UserActionPhase::Idle => UserActionPhase::Idle,
                };
                state.user_action.set(next);
            }
        });
        let view = self.source_view();
        // GtkTextView's `backspace` keybinding signal runs its class handler
        // last, so this one sees Backspace first.
        let editor = self.downgrade();
        signal_connect::connect_void(view, "backspace", false, move |values| {
            if let Some(editor) = editor.upgrade()
                && editor.backspace_held_text()
                && let Some(view) = values
                    .first()
                    .and_then(|value| value.get::<gtk4::TextView>().ok())
            {
                view.stop_signal_emission_by_name("backspace");
            }
        });
    }

    /// Decide one `insert-text` emission. Returns whether to stop it.
    pub(super) fn decide_insert(
        &self,
        buffer: &gtk4::TextBuffer,
        location: &gtk4::TextIter,
        text: &str,
    ) -> bool {
        let state = self.held_state();
        let (user_action, tainted) = match state.user_action.get() {
            UserActionPhase::Idle => (false, false),
            UserActionPhase::Active { disposition, .. } => {
                (true, disposition == UserActionDisposition::Tainted)
            }
        };
        let held = self.holds_typed_input();
        let facts = InsertFacts {
            own_edit: super::edit_lock::own_edit_in_progress(),
            locked: self.edits_locked(),
            held,
            user_action,
            at_caret: user_action
                && held
                && location.offset() == buffer.iter_at_mark(&buffer.get_insert()).offset(),
            tainted,
        };
        match insert_disposition(facts) {
            InsertDisposition::Pass => false,
            InsertDisposition::Hold => {
                self.hold_typed_text(buffer, text);
                true
            }
            InsertDisposition::Refuse { notify } => {
                if notify {
                    self.refuse_user_edit();
                }
                true
            }
        }
    }

    /// Decide one `delete-range`, `undo`, `redo`, `insert-paintable` or
    /// `insert-child-anchor` emission. `user_request`: undo and redo are a
    /// user's request whatever the user action. Returns whether to stop it.
    pub(super) fn decide_removal(&self, user_request: bool) -> bool {
        let state = self.held_state();
        let user_action = matches!(state.user_action.get(), UserActionPhase::Active { .. });
        let facts = RemovalFacts {
            own_edit: super::edit_lock::own_edit_in_progress(),
            locked: self.edits_locked(),
            held: self.holds_typed_input(),
            user_edit: user_request || user_action,
        };
        match removal_disposition(facts) {
            InsertDisposition::Pass => false,
            InsertDisposition::Hold => true,
            InsertDisposition::Refuse { notify } => {
                if let UserActionPhase::Active {
                    depth, start_len, ..
                } = state.user_action.get()
                {
                    // The rest of this user action is a different edit's
                    // (typing over a selection, Replace, moving lines).
                    state.user_action.set(UserActionPhase::Active {
                        depth,
                        start_len,
                        disposition: UserActionDisposition::Tainted,
                    });
                }
                if notify {
                    self.refuse_user_edit();
                }
                true
            }
        }
    }

    fn hold_typed_text(&self, buffer: &gtk4::TextBuffer, text: &str) {
        let state = self.held_state();
        if matches!(
            state.user_action.get(),
            UserActionPhase::Active {
                disposition: UserActionDisposition::Overflowed,
                ..
            }
        ) {
            // An earlier segment of this user action overflowed: the action is
            // refused whole, so none of its later segments is kept either.
            return;
        }
        let first = !self.has_held_text();
        let new_mark = if first {
            let caret = buffer.iter_at_mark(&buffer.get_insert());
            Some(buffer.create_mark(None, &caret, true))
        } else {
            None
        };
        let (outcome, unused_mark) = {
            let mut phase = state.episode.borrow_mut();
            let unused_mark = match (&mut *phase, new_mark) {
                (HeldEpisodePhase::Idle, Some(mark)) => {
                    *phase = HeldEpisodePhase::Holding {
                        text: HeldText::default(),
                        episode: HeldEpisode::new(),
                        first_hold: mark,
                        preedit_since: None,
                    };
                    None
                }
                (HeldEpisodePhase::Holding { first_hold, .. }, Some(mark)) => {
                    Some(std::mem::replace(first_hold, mark))
                }
                (_, None) => None,
            };
            let outcome = match &mut *phase {
                HeldEpisodePhase::Holding { text: held, .. } => held.hold(text),
                HeldEpisodePhase::Idle => HoldOutcome::Overflow,
            };
            (outcome, unused_mark)
        };
        if let Some(mark) = unused_mark {
            buffer.delete_mark(&mark);
        }
        match outcome {
            HoldOutcome::Held => {
                self.update_held_evidence(|e| e.holds = e.holds.saturating_add(1));
                if first {
                    let name = self.held_document_name();
                    self.publish_held_notice(
                        HeldNotice::Holding,
                        &format!("Typed text will appear when “{name}” is ready."),
                        MessageKind::Info,
                    );
                    self.refresh_accessibility_metadata();
                    self.queue_held_settle(false);
                }
            }
            HoldOutcome::Overflow => {
                // Paste atomicity: what this action already held goes too, so
                // a multi-segment paste is never partly held.
                let start = match state.user_action.get() {
                    UserActionPhase::Active {
                        depth, start_len, ..
                    } => {
                        state.user_action.set(UserActionPhase::Active {
                            depth,
                            start_len,
                            disposition: UserActionDisposition::Overflowed,
                        });
                        start_len
                    }
                    UserActionPhase::Idle => 0,
                };
                {
                    let mut phase = state.episode.borrow_mut();
                    if let HeldEpisodePhase::Holding { text: held, .. } = &mut *phase {
                        held.truncate_to(start);
                    }
                }
                self.update_held_evidence(|e| e.overflows = e.overflows.saturating_add(1));
                let name = self.held_document_name();
                self.publish_held_notice(
                    HeldNotice::Overflow,
                    &format!(
                        "Too much text arrived while “{name}” was busy, so it was not kept. \
                         The clipboard still has it: paste again when the document is ready."
                    ),
                    MessageKind::Warning,
                );
                if !self.has_held_text() {
                    self.end_held_episode();
                }
            }
        }
    }

    /// Backspace while text is held removes its last character. Returns
    /// whether it did (and the keybinding emission is to stop).
    fn backspace_held_text(&self) -> bool {
        if !self.source_view().is_editable() || super::edit_lock::own_edit_in_progress() {
            return false;
        }
        let (removed, empty) = {
            let mut phase = self.held_state().episode.borrow_mut();
            let HeldEpisodePhase::Holding { text, .. } = &mut *phase else {
                return false;
            };
            let removed = text.backspace();
            (removed, text.is_empty())
        };
        if removed && empty {
            self.end_held_episode();
        }
        removed
    }

    fn refuse_user_edit(&self) {
        self.update_held_evidence(|e| e.refused_edits = e.refused_edits.saturating_add(1));
        let text = if self.has_held_text() {
            "The document is busy: typed text is kept, but that edit was not applied.".to_owned()
        } else {
            let name = self.held_document_name();
            format!("“{name}” is busy: that edit was not applied.")
        };
        self.publish_held_notice(HeldNotice::EditNotApplied, &text, MessageKind::Warning);
    }

    /// A lock holder released: fold its content disposition into the episode
    /// and, when it was the last, queue the settle dispatch.
    pub(super) fn held_input_after_release(&self, release: ContentDisposition, last: bool) {
        let has_held = {
            let mut phase = self.held_state().episode.borrow_mut();
            match &mut *phase {
                HeldEpisodePhase::Holding { text, episode, .. } if !text.is_empty() => {
                    *episode = episode.after_release(release);
                    true
                }
                _ => false,
            }
        };
        if has_held && last {
            self.queue_held_settle(true);
        }
    }

    /// Keep the held window's side effects in step with the editor's state
    /// (from `refresh_accessibility_metadata`, which every load, replacement
    /// and save state change calls): the view's drop targets are off while a
    /// load is in flight or text is held, and held text whose buffer no
    /// workflow owns any more is settled at once rather than at the next
    /// retry, which is the hook in a load's terminals.
    pub(super) fn sync_held_input_window(&self) {
        let hold = self.load_state() == EditorLoadState::Loading || self.has_held_text();
        let state = self.held_state();
        if hold != state.drop_targets_held.get() {
            state.drop_targets_held.set(hold);
            if hold {
                self.hold_drop_targets(DropTargetHold::HeldInput);
            } else {
                self.release_drop_targets(DropTargetHold::HeldInput);
            }
        }
        if self.has_held_text() && !self.workflow_owns_buffer() {
            self.queue_held_settle(true);
        }
    }

    /// Queue the settle dispatch: `immediate` on an idle (replacing a pending
    /// retry timeout), otherwise a retry after the polling interval.
    fn queue_held_settle(&self, immediate: bool) {
        let state = self.held_state();
        // Taken into a local first: the `RefMut` must not live across `remove()`.
        let existing = std::mem::take(&mut *state.settle.borrow_mut());
        match existing {
            HeldSettleSchedule::ImmediateIdle(id) => {
                *state.settle.borrow_mut() = HeldSettleSchedule::ImmediateIdle(id);
                return;
            }
            HeldSettleSchedule::RetryTimeout(id) if !immediate => {
                *state.settle.borrow_mut() = HeldSettleSchedule::RetryTimeout(id);
                return;
            }
            HeldSettleSchedule::RetryTimeout(id) => id.remove(),
            HeldSettleSchedule::Unscheduled => {}
        }
        let editor = self.downgrade();
        let run = move || {
            if let Some(editor) = editor.upgrade() {
                *editor.held_state().settle.borrow_mut() = HeldSettleSchedule::Unscheduled;
                editor.run_held_settle();
            }
            glib::ControlFlow::Break
        };
        let priority = settle_priority();
        let scheduled = if immediate {
            HeldSettleSchedule::ImmediateIdle(glib::idle_add_local_full(priority, run))
        } else {
            HeldSettleSchedule::RetryTimeout(glib::timeout_add_local_full(
                Duration::from_millis(SETTLE_RETRY_INTERVAL_MS),
                priority,
                run,
            ))
        };
        *state.settle.borrow_mut() = scheduled;
    }

    /// Whether a composition holds the replay back. A composition older than
    /// `PREEDIT_DEADLINE_MS` is reset (which commits or drops its preedit, as
    /// the input method chooses; a commit is held like any typed text).
    fn preedit_holds_replay(&self, now: i64) -> bool {
        let state = self.held_state();
        let Some(im) = super::surrounding_text::view_im_context(self.source_view().upcast_ref())
        else {
            if let HeldEpisodePhase::Holding { preedit_since, .. } =
                &mut *state.episode.borrow_mut()
            {
                *preedit_since = None;
            }
            return false;
        };
        if im.preedit_string().0.is_empty() {
            if let HeldEpisodePhase::Holding { preedit_since, .. } =
                &mut *state.episode.borrow_mut()
            {
                *preedit_since = None;
            }
            return false;
        }
        let expired = {
            let mut phase = state.episode.borrow_mut();
            let HeldEpisodePhase::Holding { preedit_since, .. } = &mut *phase else {
                return false;
            };
            let since = *preedit_since.get_or_insert(now);
            now.saturating_sub(since) >= PREEDIT_DEADLINE_MS.saturating_mul(1_000)
        };
        if !expired {
            return true;
        }
        self.reset_held_preedit(&im);
        false
    }

    fn reset_held_preedit(&self, im: &gtk4::IMContext) {
        if let HeldEpisodePhase::Holding { preedit_since, .. } =
            &mut *self.held_state().episode.borrow_mut()
        {
            *preedit_since = None;
        }
        im.reset();
        self.update_held_evidence(|e| e.preedit_resets = e.preedit_resets.saturating_add(1));
    }

    fn run_held_settle(&self) {
        if !self.has_held_text() {
            return;
        }
        let preedit = self.preedit_holds_replay(glib::monotonic_time());
        if !self.settle_held_text(preedit) {
            self.queue_held_settle(false);
        }
    }

    /// Decide and act on the held text now. Returns false when it must wait.
    fn settle_held_text(&self, preedit: bool) -> bool {
        let broken = match &*self.held_state().episode.borrow() {
            HeldEpisodePhase::Holding { text, episode, .. } if !text.is_empty() => {
                episode.is_broken()
            }
            _ => return true,
        };
        let state = self.load_state();
        let facts = SettleFacts {
            locked: self.edits_locked()
                || self.buffer_replacement_in_progress()
                || self.load_projection_suspended(),
            loading: state == EditorLoadState::Loading,
            preedit,
            restore_pending: self.draft_restore_pending(),
            broken,
            load_failed: state == EditorLoadState::Failed,
            installation_incomplete: self.has_incomplete_load_installation(),
            evicted: self.is_evicted(),
            view_only: self.is_protected_view() || !self.source_view().is_editable(),
        };
        match settle_verdict(facts) {
            SettleVerdict::Wait => false,
            SettleVerdict::Replay => {
                self.replay_held_text();
                true
            }
            SettleVerdict::RescueViewOnly => self.rescue_held_text(None),
            SettleVerdict::RescueNoDocument(cause) => self.rescue_held_text(Some(cause)),
        }
    }

    /// Whether the window still has a draft restore queued or reading for
    /// this editor.
    fn draft_restore_pending(&self) -> bool {
        self.root()
            .and_downcast::<LushtextWindow>()
            .is_some_and(|window| window.draft_restore_pending_for(self))
    }

    /// Settle held text before a close looks at this editor (design D10):
    /// when no workflow owns the buffer, a composition is reset and the text
    /// is replayed (or rescued) now, so the close sees the document modified
    /// and the Save Changes dialog's Save writes the text. Returns `Busy`
    /// when a workflow still owns the buffer and the close must be refused.
    pub(crate) fn flush_held_text_for_close(&self) -> CloseDisposition {
        let ownership = if self.workflow_owns_buffer() || self.draft_restore_pending() {
            BufferWorkflowOwnership::Owned
        } else {
            BufferWorkflowOwnership::Idle
        };
        let held = if self.has_held_text() {
            HeldTextPresence::Present
        } else {
            HeldTextPresence::Empty
        };
        let disposition = close_disposition(held, ownership);
        if disposition == CloseDisposition::Flush {
            if let Some(im) =
                super::surrounding_text::view_im_context(self.source_view().upcast_ref())
                && !im.preedit_string().0.is_empty()
            {
                self.reset_held_preedit(&im);
            }
            if let Some(source) = self.held_state().settle.borrow_mut().take_source() {
                source.remove();
            }
            self.update_held_evidence(|e| e.close_flushes = e.close_flushes.saturating_add(1));
            if !self.settle_held_text(false) {
                // Only a rescue with no window to open a tab in waits; keep
                // the close refused rather than lose the text.
                self.queue_held_settle(false);
                return CloseDisposition::Busy;
            }
        }
        disposition
    }

    /// The notice a close refused because of held text publishes.
    pub(crate) fn held_text_close_refusal_text(&self) -> String {
        let name = self.held_document_name();
        format!("Typed text is waiting for “{name}”; close it when the document is ready.")
    }

    /// End the held episode: drop the first-hold mark, and let the drop
    /// targets and the accessible description follow.
    fn end_held_episode(&self) {
        if let HeldEpisodePhase::Holding { first_hold, .. } =
            std::mem::take(&mut *self.held_state().episode.borrow_mut())
        {
            self.buffer().delete_mark(&first_hold);
        }
        self.refresh_accessibility_metadata();
    }

    /// Insert the held text as one user action (one undo action, like typing).
    fn replay_held_text(&self) {
        let HeldEpisodePhase::Holding {
            mut text,
            episode,
            first_hold: mark,
            ..
        } = std::mem::take(&mut *self.held_state().episode.borrow_mut())
        else {
            return;
        };
        let text = text.take();
        let anchor = episode.anchor();
        let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
        let buffer = self.buffer();
        let view = self.source_view();
        let editable = view.is_editable();
        let inserted = crate::ui::responsiveness_probe::span("held_input_replay", || {
            buffer.begin_user_action();
            let inserted = match anchor {
                ReplayAnchor::FirstHold if !mark.is_deleted() => {
                    let mut at = buffer.iter_at_mark(&mark);
                    let inserted = buffer.insert_interactive(&mut at, &text, editable);
                    if inserted {
                        buffer.place_cursor(&at);
                    }
                    inserted
                }
                _ => buffer.insert_interactive_at_cursor(&text, editable),
            };
            buffer.end_user_action();
            inserted
        });
        buffer.delete_mark(&mark);
        if !inserted {
            // The buffer refused the insert: the text goes to a new tab, never
            // nowhere.
            let mut restored_text = HeldText::default();
            restored_text.hold(&text);
            let caret = buffer.iter_at_mark(&buffer.get_insert());
            let new_mark = buffer.create_mark(None, &caret, true);
            *self.held_state().episode.borrow_mut() = HeldEpisodePhase::Holding {
                text: restored_text,
                episode,
                first_hold: new_mark,
                preedit_since: None,
            };
            self.rescue_held_text(Some(NoDocumentCause::Replaced));
            return;
        }
        // A detached view shows the placeholder, whose marks are not the
        // document's (`view_attachment`).
        if view.has_focus() && self.view_shows_document() {
            view.scroll_mark_onscreen(&buffer.get_insert());
        }
        self.update_held_evidence(|e| {
            e.replays = e.replays.saturating_add(1);
            e.replayed_chars = e.replayed_chars.saturating_add(chars);
        });
        self.refresh_accessibility_metadata();
    }

    /// Open the held text in a new Untitled tab, marked draft-dirty, through
    /// the Preserved Drafts "Open" path, and say where it went. `cause`:
    /// `None` for a view-only document. Returns false (the text stays held)
    /// when the editor is in no window to open the tab in.
    fn rescue_held_text(&self, cause: Option<NoDocumentCause>) -> bool {
        let Some(window) = self.root().and_downcast::<LushtextWindow>() else {
            return false;
        };
        let HeldEpisodePhase::Holding {
            mut text,
            first_hold,
            ..
        } = std::mem::take(&mut *self.held_state().episode.borrow_mut())
        else {
            return true;
        };
        let text = text.take();
        let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
        self.buffer().delete_mark(&first_hold);
        self.refresh_accessibility_metadata();
        self.update_held_evidence(|e| {
            if cause.is_some() {
                e.rescued_no_document = e.rescued_no_document.saturating_add(1);
            } else {
                e.rescued_view_only = e.rescued_view_only.saturating_add(1);
            }
            e.rescued_chars = e.rescued_chars.saturating_add(chars);
            e.notices = e.notices.saturating_add(1);
        });
        let name = self.held_document_name();
        let typed = if chars == 1 {
            "1 typed character".to_owned()
        } else {
            format!("{chars} typed characters")
        };
        let why = match cause {
            None => "it is view-only because it contains very long lines",
            Some(NoDocumentCause::Evicted) => "it was unloaded from memory",
            Some(NoDocumentCause::NotLoaded) => "it did not finish loading",
            Some(NoDocumentCause::Replaced) => "its content was replaced before it was ready",
        };
        let profile = lushtext_model::document::paragraph_boundary::DocumentProfile::of_text(&text);
        window.open_set_aside_draft(text, profile);
        // Published after the tab opened, and never rate-limited: each rescue
        // is the one place the user learns where their text is.
        window.publish_status_message(
            &format!("{typed} could not go into “{name}” ({why}); they are in a new Untitled tab."),
            MessageKind::Warning,
        );
        true
    }

    fn held_document_name(&self) -> String {
        self.file_path()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .map_or_else(
                || "Untitled".to_owned(),
                |name| crate::ui::accessibility::bounded_announcement_text(&name, 64).into_owned(),
            )
    }

    /// Publish one held-typing status message, at most once per quiet
    /// interval per notice (Protected View's `notice_due`).
    fn publish_held_notice(&self, notice: HeldNotice, text: &str, kind: MessageKind) {
        let state = self.held_state();
        let now = glib::monotonic_time();
        let mut times = state.notice_times.get();
        let slot = notice.slot();
        if !super::protected_view::policy::notice_due(times[slot], now) {
            return;
        }
        times[slot] = Some(now);
        state.notice_times.set(times);
        self.update_held_evidence(|e| e.notices = e.notices.saturating_add(1));
        if let Some(window) = self.root().and_downcast::<LushtextWindow>() {
            window.publish_status_message(text, kind);
        }
    }
}

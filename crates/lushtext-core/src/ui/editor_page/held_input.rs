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
//! - **Settle.** A dispatch at `SETTLE_PRIORITY` (just above GtkTextView's
//!   line validation, ledger A70), queued at the first held character,
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
use crate::model::held_input::{
    CloseDisposition, ContentDisposition, HeldEpisode, HeldText, HoldOutcome, InsertDisposition,
    InsertFacts, NoDocumentCause, RemovalFacts, ReplayAnchor, SettleFacts, SettleVerdict,
    close_disposition, insert_disposition, removal_disposition, settle_verdict,
};
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;

/// How long a waiting settle dispatch sleeps before it decides again: 16 ms
/// (kind: polling interval, a power of two in milliseconds), about two
/// 120 Hz frames. Only what has no hook waits on it: a composition's end and a
/// draft restore that resolves without installing anything. A load's
/// terminal, a replacement's and a save's queue the dispatch directly.
const SETTLE_RETRY_INTERVAL_MS: u64 = 16;

/// The settle dispatch's priority: `GDK_PRIORITY_REDRAW + 4` (124), one above
/// GtkTextView's line validation (`GDK_PRIORITY_REDRAW + 5`, ledger A70), so
/// the replay is not held back behind a document's validation batches.
const SETTLE_PRIORITY: i32 = 124;

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

/// One editor's held-typing state.
#[derive(Default)]
pub struct HeldInputState {
    text: RefCell<HeldText>,
    episode: Cell<HeldEpisode>,
    /// Where the caret was when the episode's first character was held.
    first_hold: RefCell<Option<gtk4::TextMark>>,
    /// Depth of the buffer's user action (GTK emits only the outermost).
    user_action: Cell<u32>,
    /// The current user action had a deletion refused.
    tainted: Cell<bool>,
    /// Held bytes when the current user action began.
    action_start_len: Cell<usize>,
    /// The current user action overflowed: the rest of it is refused.
    action_overflowed: Cell<bool>,
    /// When a settle dispatch first saw the current composition.
    preedit_since: Cell<Option<i64>>,
    /// The view's drop targets are held for the held window.
    drop_targets_held: Cell<bool>,
    /// The queued settle dispatch, if any.
    settle_source: RefCell<Option<glib::SourceId>>,
    /// Whether that dispatch is the immediate one (not a retry timeout).
    settle_immediate: Cell<bool>,
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
        !self.held_state().text.borrow().is_empty()
    }

    /// Characters held now.
    #[must_use]
    pub fn held_char_count(&self) -> u64 {
        self.held_state().text.borrow().char_count()
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
        self.held_state().text.borrow().as_str().to_owned()
    }

    /// Apply an input method's `delete-surrounding` to the held text when it
    /// falls inside it. Returns whether it did.
    pub(super) fn delete_surrounding_in_held_text(&self, offset: i32, n_chars: i32) -> bool {
        if !self.has_held_text() || super::edit_lock::own_edit_in_progress() {
            return false;
        }
        let applied = self
            .held_state()
            .text
            .borrow_mut()
            .delete_surrounding(offset, n_chars);
        if applied && !self.has_held_text() {
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
                if state.user_action.get() == 0 {
                    state.tainted.set(false);
                    state.action_overflowed.set(false);
                    state.action_start_len.set(state.text.borrow().len_bytes());
                }
                state
                    .user_action
                    .set(state.user_action.get().saturating_add(1));
            }
        });
        let editor = self.downgrade();
        buffer.connect_end_user_action(move |_| {
            if let Some(editor) = editor.upgrade() {
                let state = editor.held_state();
                state
                    .user_action
                    .set(state.user_action.get().saturating_sub(1));
            }
        });
        let view = self.source_view();
        // GtkTextView's `backspace` keybinding signal runs its class handler
        // last, so this one sees Backspace first.
        let editor = self.downgrade();
        view.connect_local("backspace", false, move |values| {
            if let Some(editor) = editor.upgrade()
                && editor.backspace_held_text()
                && let Some(view) = values
                    .first()
                    .and_then(|value| value.get::<gtk4::TextView>().ok())
            {
                view.stop_signal_emission_by_name("backspace");
            }
            None
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
        let user_action = state.user_action.get() > 0;
        let held = self.holds_typed_input();
        let facts = InsertFacts {
            own_edit: super::edit_lock::own_edit_in_progress(),
            locked: self.edits_locked(),
            held,
            user_action,
            at_caret: user_action
                && held
                && location.offset() == buffer.iter_at_mark(&buffer.get_insert()).offset(),
            tainted: state.tainted.get(),
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
        let user_action = state.user_action.get() > 0;
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
                if user_action {
                    // The rest of this user action is a different edit's
                    // (typing over a selection, Replace, moving lines).
                    state.tainted.set(true);
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
        if state.action_overflowed.get() {
            // An earlier segment of this user action overflowed: the action is
            // refused whole, so none of its later segments is kept either.
            return;
        }
        let first = !self.has_held_text();
        if first {
            state.episode.set(HeldEpisode::new());
            let caret = buffer.iter_at_mark(&buffer.get_insert());
            let mark = buffer.create_mark(None, &caret, true);
            if let Some(old) = state.first_hold.replace(Some(mark)) {
                buffer.delete_mark(&old);
            }
        }
        let outcome = state.text.borrow_mut().hold(text);
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
                state.action_overflowed.set(true);
                let start = state.action_start_len.get();
                state.text.borrow_mut().truncate_to(start);
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
        let removed = self.held_state().text.borrow_mut().backspace();
        if removed && !self.has_held_text() {
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
        let state = self.held_state();
        if !self.has_held_text() {
            return;
        }
        state
            .episode
            .set(state.episode.get().after_release(release));
        if last {
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
        // Taken into a local first: the `RefMut` must not live into the block,
        // which borrows the cell again.
        let pending = state.settle_source.borrow_mut().take();
        if let Some(existing) = pending {
            if state.settle_immediate.get() || !immediate {
                state.settle_source.replace(Some(existing));
                return;
            }
            existing.remove();
        }
        let editor = self.downgrade();
        let run = move || {
            if let Some(editor) = editor.upgrade() {
                editor.held_state().settle_source.borrow_mut().take();
                editor.run_held_settle();
            }
            glib::ControlFlow::Break
        };
        let priority = glib::Priority::from(SETTLE_PRIORITY);
        let source = if immediate {
            glib::idle_add_local_full(priority, run)
        } else {
            glib::timeout_add_local_full(
                Duration::from_millis(SETTLE_RETRY_INTERVAL_MS),
                priority,
                run,
            )
        };
        state.settle_immediate.set(immediate);
        state.settle_source.replace(Some(source));
    }

    /// Whether a composition holds the replay back. A composition older than
    /// `PREEDIT_DEADLINE_MS` is reset (which commits or drops its preedit, as
    /// the input method chooses; a commit is held like any typed text).
    fn preedit_holds_replay(&self, now: i64) -> bool {
        let state = self.held_state();
        let Some(im) = super::surrounding_text::view_im_context(self.source_view().upcast_ref())
        else {
            state.preedit_since.set(None);
            return false;
        };
        if im.preedit_string().0.is_empty() {
            state.preedit_since.set(None);
            return false;
        }
        let since = *state.preedit_since.get().get_or_insert(now);
        state.preedit_since.set(Some(since));
        if now.saturating_sub(since) < PREEDIT_DEADLINE_MS.saturating_mul(1_000) {
            return true;
        }
        self.reset_held_preedit(&im);
        false
    }

    fn reset_held_preedit(&self, im: &gtk4::IMContext) {
        self.held_state().preedit_since.set(None);
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
        if !self.has_held_text() {
            return true;
        }
        let state = self.load_state();
        let facts = SettleFacts {
            locked: self.edits_locked()
                || self.buffer_replacement_in_progress()
                || self.load_projection_suspended(),
            loading: state == EditorLoadState::Loading,
            preedit,
            restore_pending: self.draft_restore_pending(),
            broken: self.held_state().episode.get().is_broken(),
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
        let owns = self.workflow_owns_buffer() || self.draft_restore_pending();
        let disposition = close_disposition(self.has_held_text(), owns);
        if disposition == CloseDisposition::Flush {
            if let Some(im) =
                super::surrounding_text::view_im_context(self.source_view().upcast_ref())
                && !im.preedit_string().0.is_empty()
            {
                self.reset_held_preedit(&im);
            }
            let pending = self.held_state().settle_source.borrow_mut().take();
            if let Some(source) = pending {
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
        if let Some(mark) = self.held_state().first_hold.take() {
            self.buffer().delete_mark(&mark);
        }
        self.refresh_accessibility_metadata();
    }

    /// Insert the held text as one user action (one undo action, like typing).
    fn replay_held_text(&self) {
        let text = self.held_state().text.borrow_mut().take();
        let anchor = self.held_state().episode.get().anchor();
        let mark = self.held_state().first_hold.take();
        let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
        let buffer = self.buffer();
        let view = self.source_view();
        let editable = view.is_editable();
        let inserted = crate::ui::responsiveness_probe::span("held_input_replay", || {
            buffer.begin_user_action();
            let inserted = match (anchor, mark.as_ref()) {
                (ReplayAnchor::FirstHold, Some(mark)) if !mark.is_deleted() => {
                    let mut at = buffer.iter_at_mark(mark);
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
        if let Some(mark) = mark {
            buffer.delete_mark(&mark);
        }
        if !inserted {
            // The buffer refused the insert: the text goes to a new tab, never
            // nowhere.
            self.held_state().text.borrow_mut().hold(&text);
            self.rescue_held_text(Some(NoDocumentCause::Replaced));
            return;
        }
        if view.has_focus() {
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
        let text = self.held_state().text.borrow_mut().take();
        let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
        self.end_held_episode();
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
        let profile = crate::model::paragraph_boundary::DocumentProfile::of_text(&text);
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

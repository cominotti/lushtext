// SPDX-License-Identifier: GPL-3.0-or-later

//! Held typing (`keep-keystrokes-during-install`): text the user types while a
//! workflow owns the document's buffer is kept and inserted afterwards,
//! instead of being refused and lost.
//!
//! **Cross-cutting, no role**, like `edit_lock.rs`, whose handlers call it:
//! the document load, the whole-buffer replacement and the save share it. Its
//! decisions are the pure `model::held_input`.
//!
//! - **Capture.** While the editor *holds input* (a workflow holds the edit
//!   lock, a load is in flight, or held text waits for its replay), an
//!   insertion typed at the caret inside a user action that deleted nothing is
//!   appended to the held text and its emission stopped. Backspace removes the
//!   last held character. Every other user edit is refused, as the lock
//!   refuses it, and reported by a rate-limited status warning.
//! - **Settle.** A dispatch queued at the first held character and whenever
//!   the last lock holder releases decides with `settle_verdict`: wait (a
//!   workflow still owns the buffer, a load is in flight, or an input-method
//!   composition is in progress: re-armed on a short timeout), replay, or
//!   discard with a notice.
//! - **Replay.** One `insert_interactive_at_cursor` inside one user action: one
//!   undo step, an ordinary edit for the modified flag, drafts, the
//!   document-scale watch and every other handler.
//! - **Which document.** Each lock release reports a `ContentDisposition`; an
//!   episode whose buffer was left partial or emptied, with no whole document
//!   published after it, discards instead of inserting into it.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::{EditorLoadState, LushtextEditorPage};
use crate::model::held_input::{
    ContentDisposition, HeldEpisode, HeldText, HoldOutcome, InsertDisposition, InsertFacts,
    RemovalFacts, SettleFacts, SettleVerdict, insert_disposition, removal_disposition,
    settle_verdict,
};
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;

/// How long a waiting settle dispatch sleeps before it decides again: 16 ms
/// (kind: polling interval, a power of two in milliseconds), about two
/// 120 Hz frames. A load's failure, cancellation or completion, a
/// replacement's end and a composition's end are seen within it without a hook
/// in each terminal.
const SETTLE_RETRY_INTERVAL_MS: u64 = 16;

/// The status notices held typing publishes, one rate-limit slot each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeldNotice {
    /// The first held character of an episode (information).
    Holding,
    /// A user edit the held window cannot keep was refused.
    EditNotApplied,
    /// An insertion did not fit the held buffer whole.
    Overflow,
    /// Held text was discarded: the buffer held no whole document.
    NotInserted,
}

impl HeldNotice {
    const fn slot(self) -> usize {
        match self {
            Self::Holding => 0,
            Self::EditNotApplied => 1,
            Self::Overflow => 2,
            Self::NotInserted => 3,
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
    /// Episodes discarded because the document was view-only.
    pub discarded_view_only: u64,
    /// Episodes discarded because the buffer held no whole document.
    pub discarded_no_document: u64,
    /// Characters discarded.
    pub discarded_chars: u64,
    /// User edits refused while held (deletions, undo, redo, misplaced or
    /// tainted inserts).
    pub refused_edits: u64,
    /// Insertions refused because they did not fit.
    pub overflows: u64,
    /// Status notices requested (published when the editor is in a window).
    pub notices: u64,
}

/// One editor's held-typing state.
#[derive(Default)]
pub struct HeldInputState {
    text: RefCell<HeldText>,
    episode: Cell<HeldEpisode>,
    /// Depth of the buffer's user action (GTK emits only the outermost).
    user_action: Cell<u32>,
    /// The current user action had a deletion refused.
    tainted: Cell<bool>,
    /// An input-method preedit is in progress.
    preedit: Cell<bool>,
    /// The queued settle dispatch, if any.
    settle_source: RefCell<Option<glib::SourceId>>,
    /// Whether that dispatch is the immediate one (not a retry timeout).
    settle_immediate: Cell<bool>,
    notice_times: Cell<[Option<i64>; 4]>,
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
        self.edits_locked()
            || self.load_state() == EditorLoadState::Loading
            || !self.held_state().text.borrow().is_empty()
    }

    /// What held typing has done, for tests.
    #[must_use]
    pub fn held_input_evidence(&self) -> HeldInputEvidence {
        let mut evidence = self.held_state().evidence.get();
        evidence.held_chars = self.held_state().text.borrow().char_count();
        evidence
    }

    fn update_held_evidence(&self, update: impl FnOnce(&mut HeldInputEvidence)) {
        let state = self.held_state();
        let mut evidence = state.evidence.get();
        update(&mut evidence);
        state.evidence.set(evidence);
    }

    /// Track the buffer's user action and the input method's preedit
    /// (connected from `setup_edit_lock`, before any other buffer handler).
    pub(super) fn setup_held_input_tracking(&self) {
        let buffer = self.buffer();
        let editor = self.downgrade();
        buffer.connect_begin_user_action(move |_| {
            if let Some(editor) = editor.upgrade() {
                let state = editor.held_state();
                if state.user_action.get() == 0 {
                    state.tainted.set(false);
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
        if let Some(im) = super::surrounding_text::view_im_context(view.upcast_ref()) {
            let editor = self.downgrade();
            im.connect_preedit_start(move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.held_state().preedit.set(true);
                }
            });
            let editor = self.downgrade();
            im.connect_preedit_end(move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.held_state().preedit.set(false);
                }
            });
        }
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
                self.hold_typed_text(text);
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

    fn hold_typed_text(&self, text: &str) {
        let first = self.held_state().text.borrow().is_empty();
        if first {
            self.held_state().episode.set(HeldEpisode::new());
        }
        let outcome = self.held_state().text.borrow_mut().hold(text);
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
                self.update_held_evidence(|e| e.overflows = e.overflows.saturating_add(1));
                self.publish_held_notice(
                    HeldNotice::Overflow,
                    "Too much text was typed while the document was busy; the rest was not kept.",
                    MessageKind::Warning,
                );
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
        if removed && self.held_state().text.borrow().is_empty() {
            self.refresh_accessibility_metadata();
        }
        removed
    }

    fn refuse_user_edit(&self) {
        self.update_held_evidence(|e| e.refused_edits = e.refused_edits.saturating_add(1));
        self.publish_held_notice(
            HeldNotice::EditNotApplied,
            "The document is busy: typed text is kept, but that edit was not applied.",
            MessageKind::Warning,
        );
    }

    /// A lock holder released: fold its content disposition into the episode
    /// and, when it was the last, queue the settle dispatch.
    pub(super) fn held_input_after_release(&self, release: ContentDisposition, last: bool) {
        let state = self.held_state();
        if state.text.borrow().is_empty() {
            return;
        }
        state
            .episode
            .set(state.episode.get().after_release(release));
        if last {
            self.queue_held_settle(true);
        }
    }

    /// Queue the settle dispatch: `immediate` on a default idle (replacing a
    /// pending retry timeout), otherwise a retry after the polling interval.
    fn queue_held_settle(&self, immediate: bool) {
        let state = self.held_state();
        if let Some(existing) = state.settle_source.borrow_mut().take() {
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
        };
        let source = if immediate {
            glib::idle_add_local_once(run)
        } else {
            glib::timeout_add_local_once(Duration::from_millis(SETTLE_RETRY_INTERVAL_MS), run)
        };
        state.settle_immediate.set(immediate);
        state.settle_source.replace(Some(source));
    }

    fn run_held_settle(&self) {
        if self.held_state().text.borrow().is_empty() {
            return;
        }
        let view = self.source_view();
        let state = self.load_state();
        let facts = SettleFacts {
            locked: self.edits_locked()
                || self.buffer_replacement_in_progress()
                || self.load_projection_suspended(),
            loading: state == EditorLoadState::Loading,
            preedit: self.held_state().preedit.get(),
            broken: self.held_state().episode.get().is_broken(),
            load_failed: state == EditorLoadState::Failed,
            installation_incomplete: self.has_incomplete_load_installation(),
            evicted: self.is_evicted(),
            view_only: self.is_protected_view() || !view.is_editable(),
        };
        match settle_verdict(facts) {
            SettleVerdict::Wait => self.queue_held_settle(false),
            SettleVerdict::Replay => self.replay_held_text(),
            SettleVerdict::DiscardViewOnly => {
                let chars = self.discard_held_text();
                self.update_held_evidence(|e| {
                    e.discarded_view_only = e.discarded_view_only.saturating_add(1);
                    e.discarded_chars = e.discarded_chars.saturating_add(chars);
                });
                self.update_held_evidence(|e| e.notices = e.notices.saturating_add(1));
                self.publish_protected_notice(
                    super::protected_view::policy::ProtectedNotice::HeldTextNotInserted,
                );
            }
            SettleVerdict::DiscardNoDocument => {
                let chars = self.discard_held_text();
                self.update_held_evidence(|e| {
                    e.discarded_no_document = e.discarded_no_document.saturating_add(1);
                    e.discarded_chars = e.discarded_chars.saturating_add(chars);
                });
                let name = self.held_document_name();
                let text = if chars == 1 {
                    format!("1 typed character was not inserted: “{name}” did not finish loading.")
                } else {
                    format!(
                        "{chars} typed characters were not inserted: “{name}” did not finish loading."
                    )
                };
                self.publish_held_notice(HeldNotice::NotInserted, &text, MessageKind::Warning);
            }
        }
    }

    /// Take the held text and end the episode; returns its character count.
    fn discard_held_text(&self) -> u64 {
        let mut held = self.held_state().text.borrow_mut();
        let chars = held.char_count();
        drop(held.take());
        drop(held);
        self.refresh_accessibility_metadata();
        chars
    }

    /// Insert the held text at the caret as one user action (one undo step).
    fn replay_held_text(&self) {
        let text = self.held_state().text.borrow_mut().take();
        let chars = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
        let buffer = self.buffer();
        let view = self.source_view();
        crate::ui::responsiveness_probe::span("held_input_replay", || {
            buffer.begin_user_action();
            let _ = buffer.insert_interactive_at_cursor(&text, view.is_editable());
            buffer.end_user_action();
        });
        if view.has_focus() {
            view.scroll_mark_onscreen(&buffer.get_insert());
        }
        self.update_held_evidence(|e| {
            e.replays = e.replays.saturating_add(1);
            e.replayed_chars = e.replayed_chars.saturating_add(chars);
        });
        self.refresh_accessibility_metadata();
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

    /// Whether held text waits for its replay (the accessible description).
    pub(super) fn has_held_text(&self) -> bool {
        !self.held_state().text.borrow().is_empty()
    }
}

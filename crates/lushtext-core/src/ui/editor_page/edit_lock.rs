// SPDX-License-Identifier: GPL-3.0-or-later

//! The buffer-level edit lock that keeps a document read-only while a
//! workflow owns its buffer (`bound-post-open-settle`).
//!
//! **Cross-cutting, no role**: the document-load installation
//! (`WFR-DOCUMENT-LOAD`), the whole-buffer replacement
//! (`WFR-BUFFER-REPLACEMENT`) and the save (`WFR-DOCUMENT-SAVE`) each hold the
//! lock while they own the buffer, and mark their own inserts and deletes.
//!
//! They used to make the view read-only with `GtkTextView::set_editable`. That
//! setter changes the view's default text style, which invalidates **every**
//! line (ledger A37). So restoring it at the terminal made GtkTextView lay out
//! the whole document a second time, in 2000 px batches of up to 18 ms on
//! dense wrapped lines, after a first dispatch of 42–45 ms (an 11 MiB restore,
//! measured 2026-10-31). The lock refuses edits at the buffer instead, which
//! changes no style:
//!
//! - `insert-text`, `insert-paintable`, `insert-child-anchor` and
//!   `delete-range` handlers (every signal that changes a buffer's content),
//!   connected when the editor is
//!   constructed, stop the emission unless the edit is the owning workflow's
//!   own (`with_own_edit`). Every route reaches the buffer through those
//!   signals: keys, input-method commits, paste and primary paste,
//!   drag-and-drop, undo and redo, and programmatic edits from any other
//!   workflow. A refused edit never reaches the default handler, so it
//!   changes no text, no modified flag, no undo history and no draft;
//! - `undo` and `redo` are refused at their own signals as well, so the undo
//!   history is never popped for an edit the lock then refuses;
//! - the view's drop targets are taken off while the lock is held, nested
//!   with Protected View's own detach (`DropTargetHold`). A drop the buffer
//!   refused would otherwise still be reported done: `GtkTextView`'s drop
//!   handler calls `gtk_text_buffer_insert_interactive`, which returns TRUE
//!   for a refused insert because the view is editable, so a MOVE drag's
//!   source deleted its text (the text vanished from both ends);
//! - the accessible state reports the view read-only while the lock is held
//!   (`accessibility.rs`), as `editable = false` did.
//!
//! The lock is a counter (a save's formatting mirror-back runs a replacement
//! inside the save). The own-edit mark is a GTK-thread scope around the owning
//! workflow's synchronous insert, delete or `set_text`, whose guard clears it
//! on every exit, unwinding included; nothing else edits a buffer inside it.

use std::cell::Cell;

use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::LushtextEditorPage;
use super::protected_view::interaction_execution::DropTargetHold;

/// One held edit lock, returned by [`LushtextEditorPage::lock_edits`] and
/// consumed by [`LushtextEditorPage::unlock_edits`].
///
/// Move-only on purpose: a workflow's restore state carries it, so restoring
/// that state twice, or never taking the lock it releases, does not compile.
#[must_use = "an edit lock is released only by passing it to unlock_edits"]
#[derive(Debug)]
pub(crate) struct EditLock {
    _held: (),
}

/// One editor's edit-lock state.
#[derive(Default)]
pub struct EditLockState {
    /// Workflows currently holding the lock.
    holders: Cell<u32>,
}

thread_local! {
    /// Depth of owning-workflow edits in progress on the GTK thread.
    static OWN_EDITS: Cell<u32> = const { Cell::new(0) };
}

/// Clears the own-edit mark when dropped, unwinding included.
struct OwnEdit;

impl Drop for OwnEdit {
    fn drop(&mut self) {
        OWN_EDITS.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Run the owning workflow's own buffer edit through the lock.
pub(crate) fn with_own_edit<R>(edit: impl FnOnce() -> R) -> R {
    OWN_EDITS.with(|depth| depth.set(depth.get().saturating_add(1)));
    let _guard = OwnEdit;
    edit()
}

impl LushtextEditorPage {
    /// Connect the refusing handlers (from `constructed`, before any other
    /// buffer handler, so a refused edit reaches none of them).
    pub(super) fn setup_edit_lock(&self) {
        let buffer = self.buffer();
        for signal in [
            "insert-text",
            "insert-paintable",
            "insert-child-anchor",
            "delete-range",
            "undo",
            "redo",
        ] {
            let editor = self.downgrade();
            buffer.connect_local(signal, false, move |values| {
                if let Some(editor) = editor.upgrade()
                    && editor.edit_refused()
                    && let Some(buffer) = values
                        .first()
                        .and_then(|value| value.get::<gtk4::TextBuffer>().ok())
                {
                    buffer.stop_signal_emission_by_name(signal);
                }
                None
            });
        }
    }

    /// Whether an edit arriving now is refused.
    fn edit_refused(&self) -> bool {
        self.imp().edit_lock.holders.get() > 0 && OWN_EDITS.with(Cell::get) == 0
    }

    /// Take the lock for a workflow that now owns the buffer. The first
    /// holder also takes the view's drop targets off.
    pub(crate) fn lock_edits(&self) -> EditLock {
        let state = &self.imp().edit_lock;
        let holders = state.holders.get();
        state.holders.set(holders.saturating_add(1));
        if holders == 0 {
            self.hold_drop_targets(DropTargetHold::EditLock);
        }
        self.refresh_accessibility_metadata();
        EditLock { _held: () }
    }

    /// Release a lock taken by [`Self::lock_edits`]. The last holder puts the
    /// drop targets back, unless Protected View still holds them.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "taking the move-only token by value is what releases it exactly once"
    )]
    pub(crate) fn unlock_edits(&self, lock: EditLock) {
        let EditLock { _held: () } = lock;
        let state = &self.imp().edit_lock;
        let holders = state.holders.get();
        debug_assert!(holders > 0, "unlock_edits without a held edit lock");
        state.holders.set(holders.saturating_sub(1));
        if holders == 1 {
            self.release_drop_targets(DropTargetHold::EditLock);
        }
        self.refresh_accessibility_metadata();
    }

    /// Whether the user can edit the document now: the view is editable and
    /// no workflow owns the buffer. What `editable = false` used to report
    /// while a load, replacement or save ran.
    #[must_use]
    pub fn accepts_user_edits(&self) -> bool {
        self.source_view().is_editable() && !self.edits_locked()
    }

    /// Whether some workflow holds the lock.
    #[must_use]
    pub fn edits_locked(&self) -> bool {
        self.imp().edit_lock.holders.get() > 0
    }
}

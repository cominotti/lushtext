// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`) — the Protected View session of one
//! editor (`long-line-protected-view` design D2, D3).
//!
//! The editor stores one [`ProtectedSession`] and only ever replaces it with
//! `ProtectedSession::on_event`'s result. This module enters the session from
//! C1's edit watch (task 3.3) and runs the one bounded entry undo (task 3.3a);
//! the install entries, the caret-free interaction and the clipboard routes
//! join it in the executor milestone (tasks 4.3–4.7a).

use std::cell::Cell;

use gtk4::prelude::*;

use super::policy::{ProtectedSession, SessionEvent};
use crate::model::protected_fold::{FoldThresholds, plan_folds};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::protected_fold::fold_tag;

/// The per-editor Protected View state.
#[derive(Debug)]
pub struct ProtectedViewRuntime {
    session: Cell<ProtectedSession>,
}

impl Default for ProtectedViewRuntime {
    fn default() -> Self {
        Self {
            session: Cell::new(ProtectedSession::EDITABLE),
        }
    }
}

impl ProtectedViewRuntime {
    fn advance(&self, event: SessionEvent) -> ProtectedSession {
        let next = self.session.get().on_event(event);
        self.session.set(next);
        next
    }
}

impl LushtextEditorPage {
    /// The editor's Protected View session.
    pub(crate) fn protected_session(&self) -> ProtectedSession {
        self.imp_protected_view().session.get()
    }

    /// Whether the editor is in Protected View.
    #[must_use]
    pub fn is_protected_view(&self) -> bool {
        self.protected_session().protected
    }

    /// Enter Protected View because an edit grew a paragraph over
    /// `L_PROTECT_EDIT` (design D2), inside the `insert-text` or
    /// `delete-range` after-handler emission of that edit.
    ///
    /// `lines` are the buffer lines C1's watch just measured (at most
    /// `2·R_batch`), with their lengths. The fold policy runs over each
    /// contiguous run of them, so both its stages apply in this emission: the
    /// length cap on the grown paragraph and the density cap on any window of
    /// the edited region over `V_BATCH` (the Kani-proved bound the
    /// responsiveness argument rests on). Tagging lays nothing out and records
    /// no undo (A27). The insert and selection-bound marks move to the start
    /// of the first folded paragraph (the paste or join left the insert mark at
    /// the end of the run, inside what is now a hidden tail), and the view
    /// stops being editable and shows no caret.
    pub(crate) fn enter_protected_view_from_edit(&self, lines: &[(u64, u64)]) {
        let buffer = self.buffer();
        let tag = fold_tag(buffer.upcast_ref());
        let mut first_folded = None;
        for run in contiguous_runs(lines) {
            let lengths: Vec<u64> = run.iter().map(|&(_, chars)| chars).collect();
            for range in plan_folds(&lengths, FoldThresholds::PRODUCTION) {
                let Some(&(line, _)) = usize::try_from(range.paragraph)
                    .ok()
                    .and_then(|index| run.get(index))
                else {
                    continue;
                };
                let (Ok(line), Ok(head)) = (i32::try_from(line), i32::try_from(range.head_chars))
                else {
                    continue;
                };
                if let Some(start) = buffer.iter_at_line_offset(line, head) {
                    let mut end = start;
                    if !end.ends_line() {
                        end.forward_to_line_end();
                    }
                    buffer.apply_tag(&tag, &start, &end);
                }
                first_folded.get_or_insert(line);
            }
        }
        if let Some(line) = first_folded
            && let Some(start) = buffer.iter_at_line(line)
        {
            buffer.place_cursor(&start);
        }
        let view = self.source_view();
        view.set_editable(false);
        view.set_cursor_visible(false);
        self.imp_protected_view().advance(SessionEvent::EditGrowth);
    }

    /// Whether the entry undo is available now (the alert's Undo and Ctrl+Z,
    /// design D2): the session entered by an edit and that edit is still the
    /// newest undo step. Protected View admits no other buffer change, so
    /// "the buffer can undo" is "the entry edit is the newest step".
    #[must_use]
    pub fn protected_entry_undo_available(&self) -> bool {
        self.protected_session()
            .entry_undo_available(self.buffer().can_undo())
    }

    /// Undo the edit that entered Protected View and leave it (design D2):
    /// the buffer's own undo of that step, the scale and watch from before the
    /// edit, the fold tags removed and the view editable again. Returns false,
    /// changing nothing, when the entry undo is not available.
    #[must_use = "false means nothing was undone"]
    pub fn undo_protected_entry(&self) -> bool {
        let buffer = self.buffer();
        let is_entry_step = buffer.can_undo();
        if !self.protected_session().entry_undo_available(is_entry_step) {
            return false;
        }
        buffer.undo();
        let tag = fold_tag(buffer.upcast_ref());
        let (start, end) = buffer.bounds();
        buffer.remove_tag(&tag, &start, &end);
        self.restore_document_scale_before_protected_entry();
        let view = self.source_view();
        view.set_editable(true);
        view.set_cursor_visible(true);
        self.imp_protected_view()
            .advance(SessionEvent::EntryUndo { is_entry_step });
        true
    }

    fn imp_protected_view(&self) -> &ProtectedViewRuntime {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        &self.imp().protected_view
    }
}

/// Split measured lines into runs of consecutive line numbers, in order.
fn contiguous_runs(lines: &[(u64, u64)]) -> impl Iterator<Item = &[(u64, u64)]> {
    lines.chunk_by(|a, b| b.0 == a.0.saturating_add(1))
}

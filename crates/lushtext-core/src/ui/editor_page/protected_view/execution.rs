// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`) — the Protected View session of one
//! editor (`long-line-protected-view` design D2, D3).
//!
//! The editor stores one [`ProtectedSession`] and only ever replaces it with
//! `ProtectedSession::on_event`'s result. This module enters the session at a
//! whole-content install and from C1's edit watch, keeps the session's text
//! source (the install body, or a bounded capture of the buffer after an edit
//! entry), and runs the one bounded entry undo.

use std::cell::{Cell, RefCell};

use gtk4::prelude::*;

use super::policy::{ProtectedSession, SessionEvent, WholeContentInstall};
use super::seams::{
    FoldSummary, ProtectedSourceState, ProtectedTextSource, ProtectedViewFacts,
    ProtectedViewReason, ProtectedViewTicket, protected_view_is_current,
};
use crate::model::document_scale::protects_by_length;
use crate::model::paragraph_boundary::DocumentProfile;
use crate::model::protected_fold::{FoldPlan, FoldRange, FoldThresholds, plan_folds};
use crate::ui::editor_page::{LushtextEditorPage, PendingWarningAction};
use crate::ui::plain_disposal::DisposalOwned;
use crate::ui::protected_fold::fold_tag;
use crate::ui::window::LushtextWindow;

/// The per-editor Protected View state.
#[derive(Debug)]
pub struct ProtectedViewRuntime {
    session: Cell<ProtectedSession>,
    /// A reload from disk just left Protected View: the next install terminal
    /// makes the view editable again, once.
    exit_pending: Cell<bool>,
    /// The session's immutable text source (design D4).
    source: RefCell<Option<ProtectedTextSource>>,
    /// What the session's fold hides (evidence and the automation snapshot).
    fold_summary: Cell<FoldSummary>,
    /// The session's fold plan, absolute paragraph numbers: what find and
    /// navigation read to tell a visible column from a hidden one.
    fold_plan: RefCell<Option<std::sync::Arc<FoldPlan>>>,
}

impl Default for ProtectedViewRuntime {
    fn default() -> Self {
        Self {
            session: Cell::new(ProtectedSession::EDITABLE),
            exit_pending: Cell::new(false),
            source: RefCell::new(None),
            fold_summary: Cell::new(FoldSummary::default()),
            fold_plan: RefCell::new(None),
        }
    }
}

/// Which whole-content install is starting, as the installers know it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedInstallSource {
    /// A load: a reload from disk unless it re-reads an evicted editor.
    Load { eviction_reload: bool },
    /// A bounded buffer replacement of the given kind.
    Replacement(WholeContentInstall),
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

    /// Before the first byte of a whole-content install (design D2, D3): the
    /// session advances from the body's worker profile, and a protected
    /// session makes the view non-editable and caret-free now, while the
    /// buffer still holds nothing of the body (setting `editable` invalidates
    /// every line, A37). A reload from disk re-derives: a body below
    /// `L_PROTECT_EDIT` leaves Protected View, applied at the install terminal
    /// by [`reassert_protected_view_properties`](Self::reassert_protected_view_properties).
    pub(crate) fn begin_protected_install(
        &self,
        profile: &DocumentProfile,
        source: ProtectedInstallSource,
    ) {
        let protected = protects_by_length(profile.longest_paragraph_chars);
        let event = match source {
            ProtectedInstallSource::Load {
                eviction_reload: true,
            } => SessionEvent::Installed {
                kind: WholeContentInstall::EvictionReload,
                protected,
            },
            ProtectedInstallSource::Load {
                eviction_reload: false,
            } => SessionEvent::ReloadFromDisk { protected },
            ProtectedInstallSource::Replacement(kind) => {
                SessionEvent::Installed { kind, protected }
            }
        };
        self.cancel_protected_source_capture();
        let runtime = self.imp_protected_view();
        let was_protected = runtime.session.get().protected;
        let next = runtime.advance(event);
        runtime
            .fold_summary
            .set(
                profile
                    .fold_plan
                    .as_deref()
                    .map_or_else(FoldSummary::default, |plan| FoldSummary {
                        folded_paragraphs: plan.folded_paragraphs(),
                        hidden_chars: plan.hidden_chars(),
                    }),
            );
        runtime.fold_plan.replace(profile.fold_plan.clone());
        if next.protected {
            let view = self.source_view();
            view.set_editable(false);
            view.set_cursor_visible(false);
            self.detach_protected_drop_targets();
            self.set_protected_primary_selection_off(true);
            self.set_protected_surfaces(true);
        } else if was_protected {
            runtime.exit_pending.set(true);
        }
    }

    /// At a whole-content install's terminal: keep the installed body as the
    /// session's text source when the session is protected (design D4), and
    /// hand it back otherwise. Every whole-content install replaces the
    /// previous source.
    pub(crate) fn retain_protected_source(
        &self,
        body: DisposalOwned<String>,
    ) -> Option<DisposalOwned<String>> {
        let runtime = self.imp_protected_view();
        if runtime.session.get().protected {
            runtime
                .source
                .replace(Some(ProtectedTextSource::retain(body)));
            // Results of a previous source do not survive it.
            self.restart_protected_find_for_new_source();
            self.refresh_line_inspector_for_new_source();
            None
        } else {
            runtime.source.replace(None);
            Some(body)
        }
    }

    /// Memory eviction cleared the buffer: the source goes with it (the
    /// eviction reload retains a fresh body, design D4). The session stays
    /// protected; an eviction reload is monotone.
    pub(crate) fn release_protected_source_for_eviction(&self) {
        let runtime = self.imp_protected_view();
        self.cancel_protected_source_capture();
        runtime.source.replace(None);
    }

    /// The session's text source, if it has one.
    pub(crate) fn protected_source(&self) -> Option<ProtectedTextSource> {
        self.imp_protected_view().source.borrow().clone()
    }

    /// A cancelled install clears the partial buffer it inserted: no protected
    /// body remains, so the session leaves Protected View (as a reload of a
    /// short file would) and the install's terminal makes the view editable
    /// again through [`reassert_protected_view_properties`](Self::reassert_protected_view_properties).
    pub(crate) fn abandon_protected_install(&self) {
        let runtime = self.imp_protected_view();
        if !runtime.session.get().protected {
            return;
        }
        self.cancel_protected_source_capture();
        runtime.source.replace(None);
        runtime.fold_summary.set(FoldSummary::default());
        runtime.fold_plan.replace(None);
        runtime.advance(SessionEvent::ReloadFromDisk { protected: false });
        runtime.exit_pending.set(true);
    }

    /// At an install terminal, after the installer restored the editable and
    /// caret state it saved: a protected session stays non-editable and
    /// caret-free, and a session a reload just left becomes editable again.
    pub(crate) fn reassert_protected_view_properties(&self) {
        let runtime = self.imp_protected_view();
        let view = self.source_view();
        if runtime.session.get().protected {
            view.set_editable(false);
            view.set_cursor_visible(false);
            self.request_protected_alert();
        } else if runtime.exit_pending.take() {
            view.set_editable(true);
            view.set_cursor_visible(true);
            self.restore_protected_drop_targets();
            self.set_protected_primary_selection_off(false);
            self.set_protected_surfaces(false);
        }
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
        let mut summary = FoldSummary::default();
        let mut applied = Vec::new();
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
                summary.folded_paragraphs = summary.folded_paragraphs.saturating_add(1);
                summary.hidden_chars = summary.hidden_chars.saturating_add(range.tail_chars);
                applied.push(FoldRange {
                    paragraph: u64::from(line.unsigned_abs()),
                    ..range
                });
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
        self.detach_protected_drop_targets();
        self.set_protected_primary_selection_off(true);
        // A search context already attached would keep scanning the grown
        // buffer from its idle (design D5).
        self.search_bar().detach();
        self.search_bar().set_replace_mode(false);
        self.set_protected_surfaces(true);
        self.request_protected_alert();
        let runtime = self.imp_protected_view();
        runtime.fold_summary.set(summary);
        runtime
            .fold_plan
            .replace(Some(std::sync::Arc::new(FoldPlan::from_ranges(applied))));
        let session = runtime.advance(SessionEvent::EditGrowth);
        runtime.source.replace(None);
        if let Some(ticket) = ProtectedViewTicket::new(self.downgrade(), session) {
            self.start_protected_source_capture(ticket);
        }
    }

    /// The ticket a Protected View stage dispatches under, while protected.
    pub(in crate::ui::editor_page) fn protected_view_ticket(&self) -> Option<ProtectedViewTicket> {
        ProtectedViewTicket::new(self.downgrade(), self.protected_session())
    }

    /// Whether a stage dispatched under `ticket` may still publish.
    pub(in crate::ui::editor_page) fn protected_view_current(
        &self,
        ticket: &ProtectedViewTicket,
    ) -> bool {
        protected_view_is_current(ticket, &self.protected_view_facts())
    }

    /// The editor's Protected View state, as a resumed stage checks it.
    pub(crate) fn protected_view_facts(&self) -> ProtectedViewFacts {
        let runtime = self.imp_protected_view();
        let session = runtime.session.get();
        let has_source = runtime.source.borrow().is_some();
        ProtectedViewFacts {
            session,
            reason: if session.entered_by_edit {
                ProtectedViewReason::EditGrowth
            } else {
                ProtectedViewReason::LongParagraph
            },
            folds: runtime.fold_summary.get(),
            source: match (has_source, session.entered_by_edit) {
                (true, false) => ProtectedSourceState::Retained,
                (true, true) => ProtectedSourceState::Captured,
                (false, true) => ProtectedSourceState::CapturePending,
                (false, false) => ProtectedSourceState::Released,
            },
        }
    }

    /// Keep an edit entry's captured body as the source, while the session is
    /// still protected at the entry's generation (`capture_execution`).
    pub(super) fn accept_captured_source(
        &self,
        ticket: &ProtectedViewTicket,
        body: DisposalOwned<String>,
    ) {
        if !self.protected_view_current(ticket) {
            return;
        }
        self.imp_protected_view()
            .source
            .replace(Some(ProtectedTextSource::retain(body)));
        self.notify_memory_policy_changed();
        self.restart_protected_find_for_new_source();
        self.refresh_line_inspector_for_new_source();
    }

    /// Whether an edit-entered session is still capturing its source.
    pub(crate) fn protected_source_capture_pending(&self) -> bool {
        let session = self.protected_session();
        session.protected && session.entered_by_edit && self.protected_source().is_none()
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
        self.restore_protected_drop_targets();
        self.set_protected_primary_selection_off(false);
        self.set_protected_surfaces(false);
        let runtime = self.imp_protected_view();
        runtime.fold_summary.set(FoldSummary::default());
        runtime.fold_plan.replace(None);
        runtime.source.replace(None);
        self.cancel_protected_source_capture();
        runtime.advance(SessionEvent::EntryUndo { is_entry_step });
        self.withdraw_protected_alert();
        true
    }

    /// Turn the Protected View surfaces on at entry or off at exit: the worker
    /// find routing and the fold gutter; at exit, the Line Inspector closes.
    fn set_protected_surfaces(&self, on: bool) {
        self.set_protected_find_routing(on);
        self.set_fold_gutter(on);
        if !on {
            self.close_line_inspector();
        }
        self.refresh_accessibility_metadata();
    }

    /// Take the Protected View alert down after the entry undo left the view.
    fn withdraw_protected_alert(&self) {
        if !matches!(
            self.pending_warning_action(),
            Some(PendingWarningAction::UndoProtectedEntry | PendingWarningAction::InspectLine)
        ) {
            return;
        }
        if let Some(window) = self.root().and_downcast::<LushtextWindow>() {
            window.dismiss_editor_notifications(self);
        } else {
            self.info_bar().render_notification(None);
        }
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

impl LushtextEditorPage {
    /// What the session's fold hides.
    pub(crate) fn protected_fold_summary(&self) -> FoldSummary {
        self.imp_protected_view().fold_summary.get()
    }
}

impl LushtextEditorPage {
    /// The session's fold plan, if it folds anything.
    pub(crate) fn protected_fold_plan(&self) -> Option<std::sync::Arc<FoldPlan>> {
        self.imp_protected_view().fold_plan.borrow().clone()
    }
}

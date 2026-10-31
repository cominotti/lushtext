// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: seam value objects — the Protected View workflow's ticket, facts and
//! freshness predicate (design D2), in the `*Ticket` + `*Facts` +
//! `*_is_current` shape.
//!
//! [`ProtectedViewTicket`] is built once when a protected session starts and
//! carried by every later stage that leaves the GTK dispatch (worker find,
//! inspector window reads, whole-line copy chunks, the edit-growth capture):
//! the editor's weak identity plus the protected-session generation.
//! [`ProtectedViewFacts`] is what the editor holds when such a stage resumes:
//! its live session, the entry reason, the fold summary and the source state.
//! [`protected_view_is_current`] validates the two as a unit, so a completion
//! that outlived an install, a reload, an exit or the editor itself publishes
//! nothing.
//!
//! The GTK half (the weak reference) stays in the ticket; the predicate and the
//! facts are plain values. Nothing here is Kani-checked: the session transition
//! that moves the generation is, in `policy.rs`.

use std::sync::Arc;

use super::policy::ProtectedSession;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::plain_disposal::DisposalOwned;

/// Why the editor is in Protected View.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedViewReason {
    /// A whole-content install whose body has a paragraph over
    /// `L_PROTECT_EDIT` (C1's `ReductionReason::LongParagraph`).
    #[cfg_attr(
        test,
        expect(
            dead_code,
            reason = "constructed by the Protected View executor (long-line-protected-view tasks 4.3-5.x), which lands after the pure policy"
        )
    )]
    LongParagraph,
    /// An edit grew a paragraph over `L_PROTECT_EDIT` (the only reason that
    /// may offer the entry undo).
    EditGrowth,
}

/// The fold plan's summary: what the alert, the automation snapshot and the
/// accessible description report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FoldSummary {
    /// Paragraphs with a hidden tail.
    pub(crate) folded_paragraphs: u64,
    /// Characters hidden across all tails.
    pub(crate) hidden_chars: u64,
}

/// Where the immutable text source stands (design D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedSourceState {
    /// The installed body, retained at the install's terminal.
    #[cfg_attr(
        test,
        expect(
            dead_code,
            reason = "constructed by the Protected View executor (long-line-protected-view tasks 4.3-5.x), which lands after the pure policy"
        )
    )]
    Retained,
    /// An edit-growth entry whose one capture has not run yet.
    CapturePending,
    /// An edit-growth entry whose capture completed.
    #[cfg_attr(
        test,
        expect(
            dead_code,
            reason = "constructed by the Protected View executor (long-line-protected-view tasks 4.3-5.x), which lands after the pure policy"
        )
    )]
    Captured,
    /// Released (close, eviction, reload, a replacing install).
    #[cfg_attr(
        test,
        expect(
            dead_code,
            reason = "constructed by the Protected View executor (long-line-protected-view tasks 4.3-5.x), which lands after the pure policy"
        )
    )]
    Released,
}

/// The editor's Protected View state when a stage resumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedViewFacts {
    pub(crate) session: ProtectedSession,
    pub(crate) reason: ProtectedViewReason,
    pub(crate) folds: FoldSummary,
    pub(crate) source: ProtectedSourceState,
}

/// The dispatch-time expectation of a Protected View stage.
#[derive(Clone, Debug)]
pub(crate) struct ProtectedViewTicket {
    editor: glib::WeakRef<LushtextEditorPage>,
    generation: u64,
}

impl ProtectedViewTicket {
    /// The ticket for the editor's current session, or `None` when the session
    /// is not protected (no Protected View stage may start then).
    #[must_use]
    pub(crate) fn new(
        editor: glib::WeakRef<LushtextEditorPage>,
        session: ProtectedSession,
    ) -> Option<Self> {
        session.protected.then_some(Self {
            editor,
            generation: session.generation,
        })
    }

    /// The editor, while it is alive.
    #[must_use]
    pub(crate) fn editor(&self) -> Option<LushtextEditorPage> {
        self.editor.upgrade()
    }

    /// The protected-session generation the stage was dispatched under.
    #[must_use]
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }
}

/// Whether a resumed stage may still publish: the editor is still in the very
/// protected session the ticket was dispatched under.
///
/// The generation advances on every install, reload, entry and exit
/// (`ProtectedSession::on_event`), so an equal generation on a protected
/// session means nothing replaced the content or left Protected View since
/// dispatch. Whether the editor itself is alive is the ticket's
/// [`ProtectedViewTicket::editor`], checked before the facts can be read.
#[must_use]
pub(crate) const fn protected_view_is_current(
    ticket: &ProtectedViewTicket,
    facts: &ProtectedViewFacts,
) -> bool {
    facts.session.protected && facts.session.generation == ticket.generation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::editor_page::protected_view::policy::SessionEvent;

    fn facts(session: ProtectedSession) -> ProtectedViewFacts {
        ProtectedViewFacts {
            session,
            reason: ProtectedViewReason::EditGrowth,
            folds: FoldSummary::default(),
            source: ProtectedSourceState::CapturePending,
        }
    }

    #[test]
    fn no_ticket_for_an_editable_session() {
        assert!(
            ProtectedViewTicket::new(glib::WeakRef::new(), ProtectedSession::EDITABLE).is_none()
        );
    }

    #[test]
    fn a_ticket_is_current_only_in_its_own_protected_session() {
        let entered = ProtectedSession::EDITABLE.on_event(SessionEvent::EditGrowth);
        let ticket = ProtectedViewTicket::new(glib::WeakRef::new(), entered).expect("protected");
        assert_eq!(ticket.generation(), entered.generation);
        assert!(ticket.editor().is_none());
        assert!(protected_view_is_current(&ticket, &facts(entered)));

        let reinstalled = entered.on_event(SessionEvent::ReloadFromDisk { protected: true });
        assert!(!protected_view_is_current(&ticket, &facts(reinstalled)));

        let exited = entered.on_event(SessionEvent::EntryUndo {
            is_entry_step: true,
        });
        assert!(!protected_view_is_current(&ticket, &facts(exited)));

        // A non-protected session at the ticket's generation is still stale.
        let forged = ProtectedSession {
            protected: false,
            ..entered
        };
        assert!(!protected_view_is_current(&ticket, &facts(forged)));
    }
}

// ---------------------------------------------------------------------------
// The text source (design D4, task 5.1)
// ---------------------------------------------------------------------------
//
// Find, the Line Inspector, whole-line copy and every save of a protected
// document read the source, never a GTK snapshot of the protected buffer (the
// chunked snapshot is quadratic inside one character segment, ledger A66). For
// a whole-content install it is the installed body, moved at the install's
// terminal instead of being disposed: no copy, and the body keeps its
// guaranteed hand-off to the disposal workers wherever its last reference
// drops. The buffer is immutable while protected, so the source equals
// `buffer.text(…, true)` for the life of the session. It crosses into the
// find and save workers, which is why it is a seam value object.

/// The immutable text of a protected document, shareable with workers.
#[derive(Clone)]
pub(crate) struct ProtectedTextSource {
    body: Arc<DisposalOwned<String>>,
}

impl ProtectedTextSource {
    /// Retain an installed body as the session's source.
    pub(crate) fn retain(body: DisposalOwned<String>) -> Self {
        Self {
            body: Arc::new(body.into_retained_current()),
        }
    }

    /// The whole document text.
    #[cfg_attr(
        test,
        expect(
            dead_code,
            reason = "read by the protected save and worker find (long-line-protected-view tasks 6.x, 7.1), which land after the source"
        )
    )]
    pub(crate) fn text(&self) -> &str {
        self.body.as_str()
    }

    /// Bytes the source retains, for the live-editor memory estimate.
    pub(crate) fn retained_bytes(&self) -> u64 {
        u64::try_from(self.body.len()).unwrap_or(u64::MAX)
    }
}

impl std::fmt::Debug for ProtectedTextSource {
    // The size only: document text never reaches a debug log.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProtectedTextSource")
            .field("bytes", &self.retained_bytes())
            .finish()
    }
}

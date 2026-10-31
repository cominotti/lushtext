// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence — the one observable surface of the Protected View workflow
//! (`long-line-protected-view`). Widget tests read it and the automation
//! snapshot projects from it (task 8.1). Reading it touches no toolkit state
//! and mutates nothing: every field comes from the editor's Protected View
//! runtime (the entry-undo availability reads the buffer's `can-undo`
//! property, which does no work).

use crate::ui::editor_page::LushtextEditorPage;

/// The observable Protected View state of one editor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProtectedViewEvidence {
    /// Whether the editor is in Protected View.
    pub protected: bool,
    /// Whether the session entered through an edit.
    pub entered_by_edit: bool,
    /// The protected-session generation.
    pub generation: u64,
    /// Bytes of the retained text source, when the session has one.
    pub source_bytes: Option<u64>,
    /// Whether the entry undo is offered now.
    pub entry_undo_available: bool,
    /// Paragraphs with a hidden tail.
    pub folded_paragraphs: u64,
    /// Characters hidden across every tail.
    pub hidden_chars: u64,
    /// Matches of the current protected find (at most `FIND_RESULT_CAP`).
    pub find_matches: usize,
    /// The current match's index, when there is one.
    pub find_current: Option<usize>,
    /// Whether a find request is running on its worker (holds the
    /// `editor-search` readiness blocker).
    pub find_pending: bool,
}

impl LushtextEditorPage {
    /// The Protected View evidence surface.
    #[must_use]
    pub fn protected_view_evidence(&self) -> ProtectedViewEvidence {
        let session = self.protected_session();
        ProtectedViewEvidence {
            protected: session.protected,
            entered_by_edit: session.entered_by_edit,
            generation: session.generation,
            source_bytes: self
                .protected_source()
                .map(|source| source.retained_bytes()),
            entry_undo_available: self.protected_entry_undo_available(),
            folded_paragraphs: self.protected_fold_summary().folded_paragraphs,
            hidden_chars: self.protected_fold_summary().hidden_chars,
            find_matches: self.protected_find_counts().0,
            find_current: self.protected_find_counts().1,
            find_pending: self.protected_find_pending(),
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence — the one observable surface of the Protected View workflow
//! (`long-line-protected-view`). Widget tests read it and the automation
//! snapshot projects from it (task 8.1). Reading it touches no toolkit state
//! and mutates nothing: every field comes from the editor's Protected View
//! runtime.

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
        }
    }
}

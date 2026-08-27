// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface: window-side note target resolution.
//!
//! This module carries **no role**. Every notes stage has to answer the same few
//! questions about the window before it can start — is there an open editor for
//! this path, does the active editor have a stable saved path, which workspace
//! folders are in the current shared scope, and which folder can a folder-note
//! action target. They are projections of window state onto the workflow's
//! vocabulary, not coordination of an ordered stage, so under
//! `gtk-adapter-module-boundaries` this is a called presentation surface: it owns
//! no pure policy and no evidence surface, and the decisions it reports come from
//! `policy.rs`. It is named in the `WFR-NOTES-BOOKMARKS` matrix row.

use std::path::{Path, PathBuf};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::model::workspace::WorkspaceScope;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;
use super::policy::{self, FolderNoteOpenTarget};

impl LushtextWindow {
    pub(super) fn open_editor_for_path(&self, path: &Path) -> Option<LushtextEditorPage> {
        let tab_view = &self.imp().tab_view;
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if editor.file_path().as_deref() == Some(path) {
                return Some(editor.clone());
            }
        }
        None
    }

    /// Return the active editor only when it has a stable saved file path.
    pub(super) fn require_saved_editor(
        &self,
        missing_path_message: &str,
    ) -> Option<LushtextEditorPage> {
        let Some(editor) = self.active_editor() else {
            self.publish_status_message(missing_path_message, MessageKind::Warning);
            return None;
        };
        if editor.file_path().is_some() {
            return Some(editor);
        }

        self.publish_status_message(missing_path_message, MessageKind::Warning);
        None
    }

    /// Collect current workspace folders for bookmark and note workflows.
    pub(super) fn workspace_folder_paths_for_notes(&self) -> Vec<PathBuf> {
        self.current_workspace_folder_paths()
    }

    /// Decide what `Open Folder Note...` can do in the current shared scope.
    pub(super) fn current_folder_note_open_target(&self) -> FolderNoteOpenTarget {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let WorkspaceScope::Workspace(workspace_id) = workspaces_file.current_scope() else {
            return FolderNoteOpenTarget::AggregateScope;
        };
        workspaces_file
            .workspaces
            .into_iter()
            .find(|workspace| workspace.id == workspace_id)
            .map_or(
                FolderNoteOpenTarget::WorkspaceMissing,
                policy::folder_note_target_for_workspace,
            )
    }

    /// Return whether the header menu can start a folder-note workflow immediately.
    pub(super) fn current_folder_note_action_available(&self) -> bool {
        policy::folder_note_action_available(&self.current_folder_note_open_target())
    }
}

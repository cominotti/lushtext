// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared workspace-scope state for the main window shell.
//!
//! The sidebar selector remains the user-facing control, but the window owns
//! the app-wide scope consumed by search, palette indexing, and note workflows.

use std::path::PathBuf;

use crate::model::workspace::WorkspaceScope;
use glib::subclass::prelude::ObjectSubclassIsExt;

use super::LushtextWindow;

impl LushtextWindow {
    /// Store a new shared workspace scope and refresh workspace-aware consumers.
    pub(super) fn set_workspace_scope(&self, scope: WorkspaceScope) {
        if *self.imp().workspace_scope.borrow() == scope {
            return;
        }
        *self.imp().workspace_scope.borrow_mut() = scope;
        self.refresh_workspace_scope_consumers();
    }

    /// Return the current shared workspace scope.
    #[must_use]
    pub(super) fn current_workspace_scope(&self) -> WorkspaceScope {
        self.imp().workspace_scope.borrow().clone()
    }

    /// Return the folder paths covered by the current shared scope.
    #[must_use]
    pub(super) fn current_workspace_folder_paths(&self) -> Vec<PathBuf> {
        let scope = self.current_workspace_scope();
        self.imp().sidebar.folder_paths_for_scope(&scope)
    }

    /// Refresh every window-level consumer that depends on the shared workspace scope.
    pub(super) fn refresh_workspace_scope_consumers(&self) {
        *self.imp().workspace_scope.borrow_mut() = self.imp().sidebar.current_scope();
        let folders = self.current_workspace_folder_paths();
        self.imp().search_panel.set_workspace_folders(folders);
        self.refresh_command_palette_sources();
        self.rebuild_file_index();
        self.refresh_notes_menu_state();
        self.refresh_preview();
    }
}

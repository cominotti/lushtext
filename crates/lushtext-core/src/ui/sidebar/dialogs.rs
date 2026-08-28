// SPDX-License-Identifier: GPL-3.0-or-later

//! Folder and workspace dialogs for the sidebar.
//!
//! This slice owns GTK dialog presentation and the follow-up workspace updates
//! that happen once the user confirms a sidebar action.
//!
//! # Role: called presentation surface — **not** one of the five roles
//!
//! Constructs the workflow's workspace and folder dialogs and reports their responses
//! back to the coordination roles. Dialog chrome, not coordination: the stages a
//! response triggers live in `list_execution` and `membership_execution`.
//!
//! It owns no `policy.rs` and no `evidence.rs`, and it keeps every behavior obligation
//! stated below and in the workflow's matrix row.

use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt};

use super::LushtextSidebar;
use crate::model::workspace::WorkspaceId;
use crate::ui::accessibility::{self, AnnouncementLane};

impl LushtextSidebar {
    /// Create a new empty workspace through a name-entry dialog.
    pub fn create_new_workspace(&self) {
        let Some(root) = self.root() else {
            return;
        };

        let dialog = libadwaita::AlertDialog::builder()
            .heading("New Workspace")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("create", "Create");
        dialog.set_response_appearance("create", libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        dialog.set_close_response("cancel");
        dialog.set_response_enabled("create", false);

        let entry = gtk4::Entry::new();
        entry.set_placeholder_text(Some("Workspace name"));
        entry.set_activates_default(true);
        dialog.set_extra_child(Some(&entry));

        let dialog_for_entry = dialog.clone();
        entry.connect_changed(move |entry| {
            dialog_for_entry.set_response_enabled("create", !entry.text().trim().is_empty());
        });

        let sidebar_weak = self.downgrade();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "create" {
                return;
            }
            let name = entry.text();
            let name = name.trim();
            if name.is_empty() {
                return;
            }
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_new_workspace_name(name);
            }
        });

        dialog.present(Some(&root));
    }

    /// Test helper for confirmed New Workspace name entry.
    #[cfg(feature = "test-utils")]
    pub fn enter_new_workspace_name_for_test(&self, name: &str) {
        self.handle_new_workspace_name(name);
    }

    /// Test helper for New Workspace cancellation.
    #[cfg(feature = "test-utils")]
    pub fn cancel_new_workspace_for_test(&self) {}

    /// Add another folder to an existing workspace by opening a folder dialog.
    pub(super) fn show_add_folder_dialog(&self, workspace_id: &WorkspaceId) {
        let Some(root) = self.root() else {
            return;
        };
        let Some(window) = root.downcast_ref::<gtk4::Window>() else {
            return;
        };

        let dialog = gtk4::FileDialog::builder()
            .title("Add Folder")
            .modal(true)
            .build();

        let sidebar_weak = self.downgrade();
        let workspace_id = workspace_id.clone();
        dialog.select_folder(Some(window), gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
                && let Some(sidebar) = sidebar_weak.upgrade()
            {
                sidebar.handle_add_folder_to_workspace(&workspace_id, &path);
            }
        });
    }

    /// Test helper for the existing-workspace Add Folder chooser.
    #[cfg(feature = "test-utils")]
    pub fn select_folder_for_workspace_for_test(
        &self,
        workspace_id: &WorkspaceId,
        path: &std::path::Path,
    ) {
        self.handle_add_folder_to_workspace(workspace_id, path);
    }

    /// Test helper for confirmed Remove from Workspace actions.
    #[cfg(feature = "test-utils")]
    pub fn remove_folder_from_workspace_for_test(
        &self,
        workspace_id: &WorkspaceId,
        folder_id: &crate::model::workspace::WorkspaceFolderId,
        path: &std::path::Path,
    ) {
        self.handle_remove_folder_from_workspace(workspace_id, folder_id, path);
    }

    /// Test helper for confirmed Rename Workspace actions.
    #[cfg(feature = "test-utils")]
    pub fn rename_workspace_for_test(&self, workspace_id: &WorkspaceId, new_name: &str) {
        self.handle_rename_workspace(workspace_id, new_name);
    }

    /// Test helper for confirmed Remove Workspace actions.
    #[cfg(feature = "test-utils")]
    pub fn remove_workspace_for_test(&self, workspace_id: &WorkspaceId) {
        self.handle_remove_workspace(workspace_id);
    }

    /// Show the rename workspace dialog.
    pub(super) fn show_rename_workspace_dialog(&self, workspace_id: &WorkspaceId) {
        let Some(root) = self.root() else {
            return;
        };
        let current_name = self.workspace_name_for_id(workspace_id);

        let dialog = libadwaita::AlertDialog::builder()
            .heading("Rename Workspace")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("rename", "Rename");
        dialog.set_response_appearance("rename", libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");

        let entry = gtk4::Entry::new();
        entry.set_text(&current_name);
        entry.set_activates_default(true);
        dialog.set_extra_child(Some(&entry));

        let sidebar_weak = self.downgrade();
        let workspace_id = workspace_id.clone();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "rename" {
                return;
            }
            let new_name = entry.text();
            let new_name = new_name.trim();
            if new_name.is_empty() {
                return;
            }

            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_rename_workspace(&workspace_id, new_name);
            }
        });

        dialog.present(Some(&root));
    }

    /// Show the remove workspace confirmation dialog.
    pub(super) fn show_unlist_workspace_dialog(&self, workspace_id: &WorkspaceId) {
        let Some(root) = self.root() else {
            return;
        };
        let current_name = self.workspace_name_for_id(workspace_id);

        let dialog = libadwaita::AlertDialog::builder()
            .heading(format!("Remove '{current_name}'?"))
            .body("The workspace will be removed from the sidebar. Files will not be deleted.")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("unlist", "Remove");
        dialog.set_response_appearance("unlist", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let sidebar_weak = self.downgrade();
        let workspace_id = workspace_id.clone();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "unlist" {
                return;
            }
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_remove_workspace(&workspace_id);
            }
        });

        accessibility::announce_with_lane(
            self,
            &format!("Remove {current_name}? The workspace will be removed from the sidebar."),
            AnnouncementLane::Alert,
        );
        dialog.present(Some(&root));
    }
}

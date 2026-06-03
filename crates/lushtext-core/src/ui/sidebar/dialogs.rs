// SPDX-License-Identifier: GPL-3.0-or-later

//! Folder and workspace dialogs for the sidebar.
//!
//! This slice owns GTK dialog presentation and the follow-up workspace updates
//! that happen once the user confirms a sidebar action.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt};
use std::path::Path;

use super::LushtextSidebar;
use crate::model::workspace::WorkspaceId;

impl LushtextSidebar {
    /// Create a new workspace by opening a folder dialog.
    pub fn create_new_workspace(&self) {
        let Some(root) = self.root() else {
            return;
        };
        let Some(window) = root.downcast_ref::<gtk4::Window>() else {
            return;
        };

        let dialog = gtk4::FileDialog::builder()
            .title("Open Folder")
            .modal(true)
            .build();

        let sidebar_weak = self.downgrade();
        dialog.select_folder(Some(window), gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
                && let Some(sidebar) = sidebar_weak.upgrade()
            {
                sidebar.handle_workspace_folder_selection(&path);
            }
        });
    }

    /// Complete the Add Workspace Folder chooser after GTK or a portal returns
    /// a folder path. Cancellation intentionally skips this helper so the
    /// workspace list is unchanged.
    fn handle_workspace_folder_selection(&self, path: &Path) {
        self.handle_new_workspace(path);
    }

    /// Test helper for the folder chooser's successful Add Workspace result.
    #[cfg(feature = "test-utils")]
    pub fn select_workspace_folder_for_test(&self, path: &std::path::Path) {
        self.handle_workspace_folder_selection(path);
    }

    /// Test helper for Add Workspace cancellation. This explicit no-op keeps
    /// cancellation coverage readable without exposing GTK dialog internals.
    #[cfg(feature = "test-utils")]
    pub fn cancel_workspace_folder_for_test(&self) {}

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
                sidebar
                    .imp()
                    .workspaces_file
                    .borrow_mut()
                    .rename_workspace(&workspace_id, new_name);
                sidebar.persist();
                sidebar.rebuild_sections_from_state();
                sidebar.notify_workspace_structure_changed();
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
                let scope_changed =
                    sidebar.imp().current_scope.borrow().workspace_id() == Some(&workspace_id);
                sidebar
                    .imp()
                    .workspaces_file
                    .borrow_mut()
                    .remove_workspace(&workspace_id);
                if scope_changed {
                    *sidebar.imp().current_scope.borrow_mut() =
                        sidebar.imp().workspaces_file.borrow().current_scope();
                }
                sidebar.persist();
                sidebar.rebuild_sections_from_state();
                sidebar.notify_workspace_structure_changed();
                if scope_changed {
                    sidebar.notify_workspace_scope_changed();
                }
            }
        });

        dialog.present(Some(&root));
    }
}

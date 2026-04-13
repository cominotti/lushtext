// SPDX-License-Identifier: GPL-3.0-or-later

//! Folder and workspace dialogs for the sidebar.
//!
//! This slice owns GTK dialog presentation and the follow-up workspace updates
//! that happen once the user confirms a sidebar action.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, AlertDialogExt};

use crate::model::workspace::{WorkspaceEntry, WorkspaceId};

use super::LushtextSidebar;

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
                sidebar.handle_new_workspace(&path);
            }
        });
    }

    /// Handle "Replace Workspace Root" or "Add Folder to Workspace".
    pub(super) fn handle_add_folder(&self, workspace_id: &WorkspaceId) {
        let Some(window) = self.parent_window() else {
            return;
        };

        let has_entries = self
            .imp()
            .workspaces_file
            .borrow()
            .workspaces
            .iter()
            .any(|workspace| workspace.id == *workspace_id && !workspace.entries.is_empty());

        let title = if has_entries {
            "Replace Workspace Root"
        } else {
            "Add Folder to Workspace"
        };

        let dialog = gtk4::FileDialog::builder().title(title).modal(true).build();

        let sidebar_weak = self.downgrade();
        let workspace_id = workspace_id.clone();
        dialog.select_folder(Some(&window), gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
                && let Some(sidebar) = sidebar_weak.upgrade()
            {
                let name = super::workspaces::folder_display_name(&path);

                sidebar.imp().workspaces_file.borrow_mut().replace_root(
                    &workspace_id,
                    WorkspaceEntry::Directory { path: path.clone() },
                    &name,
                );
                sidebar.persist();
                sidebar.rebuild_sections_from_state();
                sidebar.notify_workspace_changed();
            }
        });
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
                sidebar
                    .imp()
                    .workspaces_file
                    .borrow_mut()
                    .rename_workspace(&workspace_id, new_name);
                sidebar.persist();
                sidebar.rebuild_sections_from_state();
            }
        });

        dialog.present(Some(&root));
    }

    /// Show the unlist workspace confirmation dialog.
    pub(super) fn show_unlist_workspace_dialog(&self, workspace_id: &WorkspaceId) {
        let Some(root) = self.root() else {
            return;
        };
        let current_name = self.workspace_name_for_id(workspace_id);

        let dialog = libadwaita::AlertDialog::builder()
            .heading(format!("Unlist '{current_name}'?"))
            .body("The workspace will be removed from the sidebar. Files will not be deleted.")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("unlist", "Unlist");
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
                sidebar
                    .imp()
                    .workspaces_file
                    .borrow_mut()
                    .remove_workspace(&workspace_id);
                sidebar.persist();
                sidebar.rebuild_sections_from_state();
                sidebar.notify_workspace_changed();
            }
        });

        dialog.present(Some(&root));
    }

    fn parent_window(&self) -> Option<gtk4::Window> {
        self.root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok())
    }
}

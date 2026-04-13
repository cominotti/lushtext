// SPDX-License-Identifier: GPL-3.0-or-later

//! Callback forwarding from workspace sections back into the sidebar shell.
//!
//! The workspace sections speak in widget-local callbacks; this slice translates
//! those events into the sidebar's outward callback surface and workspace-level
//! handlers without repeating the same weak-upgrade glue inline.

use std::path::Path;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::ObjectExt;

use super::{LushtextSidebar, WorkspaceSection};

impl LushtextSidebar {
    /// Wire one section's callbacks into the sidebar shell.
    pub(super) fn wire_section_callbacks(&self, section: &WorkspaceSection) {
        let sidebar_weak = self.downgrade();
        section.connect_file_activated(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_file_activated(path);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_file_renamed(move |old, new| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_file_renamed(old, new);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_file_deleted(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_file_deleted(path);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_file_created(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_file_created(path);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_add_folder_requested(move |workspace_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_add_folder(workspace_id);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_rename_workspace_requested(move |workspace_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.show_rename_workspace_dialog(workspace_id);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_unlist_workspace_requested(move |workspace_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.show_unlist_workspace_dialog(workspace_id);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_folder_focused(move |workspace_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_folder_focused(workspace_id);
            }
        });
    }

    fn emit_file_activated(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().file_activated_callback.borrow() {
            callback(path);
        }
    }

    fn emit_file_renamed(&self, old_path: &Path, new_path: &Path) {
        if let Some(ref callback) = *self.imp().rename_callback.borrow() {
            callback(old_path, new_path);
        }
    }

    fn emit_file_deleted(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().delete_callback.borrow() {
            callback(path);
        }
    }

    fn emit_file_created(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().create_callback.borrow() {
            callback(path);
        }
    }
}

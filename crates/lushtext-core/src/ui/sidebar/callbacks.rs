// SPDX-License-Identifier: GPL-3.0-or-later

//! Callback forwarding from workspace sections back into the sidebar shell.
//!
//! The workspace sections speak in widget-local callbacks; this slice translates
//! those events into the sidebar's outward callback surface and workspace-level
//! handlers without repeating the same weak-upgrade glue inline.

use std::path::Path;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::ObjectExt;

use crate::services::notifications::NotificationSeverity;

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
        section.connect_local_history_requested(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_local_history_requested(path);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_document_note_requested(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_document_note_requested(path);
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
        section.connect_message(move |text, severity| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_message(text, severity);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_peek_promoted(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                // Peek promotion should behave exactly like normal row activation:
                // reuse the existing sidebar -> window open path so duplicate-tab
                // detection and editor focus remain centralized in the window shell.
                sidebar.emit_file_activated(path);
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

        let sidebar_weak = self.downgrade();
        section.connect_workspace_note_requested(move |workspace_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_workspace_note_requested(workspace_id);
            }
        });
    }

    fn emit_file_activated(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().file_activated_callback.borrow() {
            callback(path);
        }
    }

    fn emit_local_history_requested(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().local_history_callback.borrow() {
            callback(path);
        }
    }

    fn emit_document_note_requested(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().document_note_callback.borrow() {
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

    fn emit_message(&self, text: &str, severity: NotificationSeverity) {
        if let Some(ref callback) = *self.imp().message_callback.borrow() {
            callback(text, severity);
        }
    }

    fn emit_workspace_note_requested(&self, workspace_id: &crate::model::workspace::WorkspaceId) {
        if let Some(ref callback) = *self.imp().workspace_note_callback.borrow() {
            callback(workspace_id.clone());
        }
    }
}

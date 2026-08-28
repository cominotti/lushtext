// SPDX-License-Identifier: GPL-3.0-or-later

//! Callback forwarding from workspace sections back into the sidebar shell.
//!
//! The workspace sections speak in widget-local callbacks; this slice translates
//! those events into the sidebar's outward callback surface and workspace-level
//! handlers without repeating the same weak-upgrade glue inline.
//!
//! # Role: called presentation surface — **not** one of the five roles
//!
//! Projects the workflow onto the window's callback slots: it stores and forwards the
//! file and workspace callbacks the window registers. It holds no ordered stage of
//! its own.
//!
//! It owns no `policy.rs` and no `evidence.rs`, and it keeps every behavior obligation
//! stated below and in the workflow's matrix row.

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
        section.connect_add_folder_requested(move |workspace_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.show_add_folder_dialog(workspace_id);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_remove_folder_requested(move |workspace_id, folder_id, path| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_remove_folder_from_workspace(workspace_id, folder_id, path);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_folder_note_for_folder_requested(move |workspace_id, path| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_folder_note_for_folder_requested(workspace_id, path);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_reorder_folder_requested(move |workspace_id, folder_id, direction| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_reorder_folder_in_workspace(workspace_id, folder_id, direction);
            }
        });

        let sidebar_weak = self.downgrade();
        section.connect_reorder_folder_to_index_requested(
            move |workspace_id, folder_id, new_index| {
                if let Some(sidebar) = sidebar_weak.upgrade() {
                    sidebar.handle_reorder_folder_to_index_in_workspace(
                        workspace_id,
                        folder_id,
                        new_index,
                    );
                }
            },
        );

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
        section.connect_folder_note_requested(move |workspace_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.emit_folder_note_requested(workspace_id);
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
        // End the borrow before invoking so a callback that re-enters
        // registration cannot panic; restore unless a replacement was set.
        let callback = self.imp().rename_callback.borrow_mut().take();
        if let Some(callback) = callback {
            callback(old_path, new_path);
            self.imp()
                .rename_callback
                .borrow_mut()
                .get_or_insert(callback);
        }
    }

    fn emit_file_deleted(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().delete_callback.borrow() {
            callback(path);
        }
    }

    fn emit_file_created(&self, path: &Path) {
        let callback = self.imp().create_callback.borrow_mut().take();
        if let Some(callback) = callback {
            callback(path);
            self.imp()
                .create_callback
                .borrow_mut()
                .get_or_insert(callback);
        }
    }

    fn emit_message(&self, text: &str, severity: NotificationSeverity) {
        if let Some(ref callback) = *self.imp().message_callback.borrow() {
            callback(text, severity);
        }
    }

    fn emit_folder_note_requested(&self, workspace_id: &crate::model::workspace::WorkspaceId) {
        if let Some(ref callback) = *self.imp().folder_note_callback.borrow() {
            callback(workspace_id.clone());
        }
    }

    fn emit_folder_note_for_folder_requested(
        &self,
        workspace_id: &crate::model::workspace::WorkspaceId,
        path: &Path,
    ) {
        if let Some(ref callback) = *self.imp().folder_note_for_folder_callback.borrow() {
            callback(workspace_id.clone(), path.to_path_buf());
        }
    }
}

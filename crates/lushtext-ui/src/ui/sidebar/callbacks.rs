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

use gtk4::prelude::ObjectExt;

use crate::ui::signal_connect;
use lushtext_model::values::{SharedPath, SharedStr};
use lushtext_model::workspace::{WorkspaceId, WorkspaceScope};
use lushtext_services::coordination::notifications::NotificationSeverity;

use super::{LushtextSidebar, WorkspaceSection};

/// Typed outward events emitted by `LushtextSidebar` over its `"sidebar-event"`
/// GObject signal.
#[derive(Clone)]
pub(super) enum SidebarEvent {
    FileActivated(SharedPath),
    LocalHistoryRequested(SharedPath),
    DocumentNoteRequested(SharedPath),
    FileRenamed(SharedPath, SharedPath),
    FileDeleted(SharedPath),
    FileCreated(SharedPath),
    Message(SharedStr, NotificationSeverity),
    FolderNoteRequested(WorkspaceId),
    FolderNoteForFolderRequested(WorkspaceId, SharedPath),
    WorkspaceStructureChanged,
    WorkspaceScopeChanged(WorkspaceScope),
}

#[derive(Clone, glib::Boxed)]
#[boxed_type(name = "LushtextSidebarEvent")]
pub(super) struct BoxedSidebarEvent(pub(super) SidebarEvent);

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

    pub(super) fn emit_sidebar_event(&self, event: SidebarEvent) {
        signal_connect::emit_void(self, "sidebar-event", &[&BoxedSidebarEvent(event)]);
    }

    pub(super) fn connect_sidebar_event(&self, handler: Box<dyn Fn(&SidebarEvent) + 'static>) {
        signal_connect::connect_void(self, "sidebar-event", false, move |values| {
            if let Some(Ok(boxed)) = values.get(1).map(glib::Value::get::<&BoxedSidebarEvent>) {
                handler(&boxed.0);
            }
        });
    }

    fn emit_file_activated(&self, path: &Path) {
        self.emit_sidebar_event(SidebarEvent::FileActivated(SharedPath::from(path)));
    }

    fn emit_local_history_requested(&self, path: &Path) {
        self.emit_sidebar_event(SidebarEvent::LocalHistoryRequested(SharedPath::from(path)));
    }

    fn emit_document_note_requested(&self, path: &Path) {
        self.emit_sidebar_event(SidebarEvent::DocumentNoteRequested(SharedPath::from(path)));
    }

    fn emit_file_renamed(&self, old_path: &Path, new_path: &Path) {
        self.emit_sidebar_event(SidebarEvent::FileRenamed(
            SharedPath::from(old_path),
            SharedPath::from(new_path),
        ));
    }

    fn emit_file_deleted(&self, path: &Path) {
        self.emit_sidebar_event(SidebarEvent::FileDeleted(SharedPath::from(path)));
    }

    fn emit_file_created(&self, path: &Path) {
        self.emit_sidebar_event(SidebarEvent::FileCreated(SharedPath::from(path)));
    }

    pub(super) fn emit_message(&self, text: &str, severity: NotificationSeverity) {
        self.emit_sidebar_event(SidebarEvent::Message(SharedStr::from(text), severity));
    }

    fn emit_folder_note_requested(&self, workspace_id: &WorkspaceId) {
        self.emit_sidebar_event(SidebarEvent::FolderNoteRequested(workspace_id.clone()));
    }

    fn emit_folder_note_for_folder_requested(&self, workspace_id: &WorkspaceId, path: &Path) {
        self.emit_sidebar_event(SidebarEvent::FolderNoteForFolderRequested(
            workspace_id.clone(),
            SharedPath::from(path),
        ));
    }
}

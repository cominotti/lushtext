// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's **folder-membership** stage
//! order: add, remove, and reorder a workspace's folders.
//!
//! # Role, and why this is its own module
//!
//! Coordination, `execution`, qualified by the stage order it serves. This is the
//! **twelfth** stage order, which the inherited stage trace omitted even though
//! `Add Folder` was already recorded as one of the row's entry points. It earns its
//! own module rather than living in `list_execution` because it has two entry points
//! of its own (the workspace header dialog and the section-side row request), the
//! membership family's only off-GTK stage, its own self-restarting retry, and a
//! two-sided terminal.
//!
//! # Inversion to be aware of
//!
//! `handle_add_folder_to_workspace` resolves folder identity **off the GTK thread**,
//! so control resumes in `apply_add_folder_to_workspace` after the worker completes.
//! The mutation is applied and persistence requested only at that resumption point.

use std::path::{Path, PathBuf};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;

use crate::model::workspace::{
    WorkspaceConfig, WorkspaceFolderId, WorkspaceFolderMoveDirection, WorkspaceId,
};
use crate::services::notifications::NotificationSeverity;
use crate::services::workspace_manager;
use workspace_manager::{
    WorkspaceFolderAddError, WorkspaceFolderRemoveError, WorkspaceFolderReorderError,
};

use super::LushtextSidebar;

impl LushtextSidebar {
    /// Append one folder to an existing workspace and refresh dependent views.
    pub(super) fn handle_add_folder_to_workspace(&self, workspace_id: &WorkspaceId, path: &Path) {
        let workspace_id = workspace_id.clone();
        let folder_path = path.to_path_buf();
        let Some(existing_paths) = self
            .imp()
            .workspaces_file
            .borrow()
            .workspace(&workspace_id)
            .map(WorkspaceConfig::folder_paths)
        else {
            self.emit_add_folder_error(WorkspaceFolderAddError::WorkspaceNotFound);
            return;
        };

        spawn_blocking_then(
            self.clone(),
            {
                move || {
                    let folder_identity = workspace_manager::folder_identity(&folder_path);
                    let existing_identities = workspace_manager::folder_identities(&existing_paths);
                    (
                        workspace_id,
                        folder_path,
                        existing_paths,
                        folder_identity,
                        existing_identities,
                    )
                }
            },
            |sidebar,
             (
                workspace_id,
                folder_path,
                existing_paths,
                folder_identity,
                existing_identities,
            )| {
                sidebar.apply_add_folder_to_workspace(
                    &workspace_id,
                    &folder_path,
                    &existing_paths,
                    &folder_identity,
                    &existing_identities,
                );
            },
        );
    }

    fn apply_add_folder_to_workspace(
        &self,
        workspace_id: &WorkspaceId,
        folder_path: &Path,
        existing_paths: &[PathBuf],
        folder_identity: &workspace_manager::WorkspaceFolderIdentity,
        existing_identities: &[workspace_manager::WorkspaceFolderIdentity],
    ) {
        let result = {
            let mut workspaces = self.imp().workspaces_file.borrow_mut();
            workspace_manager::add_folder_to_workspace_with_identities(
                &mut workspaces,
                workspace_id,
                folder_path.to_path_buf(),
                existing_paths,
                folder_identity,
                existing_identities,
            )
        };

        match result {
            Ok(folder_id) => {
                if let Some(section) = self
                    .imp()
                    .sections
                    .borrow()
                    .iter()
                    .find(|section| section.workspace_id() == *workspace_id)
                {
                    section.add_workspace_folder(&folder_id, folder_path);
                } else {
                    self.rebuild_sections_from_state();
                }
                self.persist();
                self.notify_workspace_structure_changed();
            }
            Err(WorkspaceFolderAddError::StaleFolderSnapshot) => {
                self.handle_add_folder_to_workspace(workspace_id, folder_path);
            }
            Err(error) => self.emit_add_folder_error(error),
        }
    }

    /// Remove one folder membership from an existing workspace.
    pub(super) fn handle_remove_folder_from_workspace(
        &self,
        workspace_id: &WorkspaceId,
        folder_id: &WorkspaceFolderId,
        _folder_path: &Path,
    ) {
        let result = {
            let mut workspaces = self.imp().workspaces_file.borrow_mut();
            workspace_manager::remove_folder_from_workspace(
                &mut workspaces,
                workspace_id,
                folder_id,
            )
        };

        match result {
            Ok(removed_path) => {
                if let Some(section) = self
                    .imp()
                    .sections
                    .borrow()
                    .iter()
                    .find(|section| section.workspace_id() == *workspace_id)
                {
                    section.remove_workspace_folder(folder_id, &removed_path);
                } else {
                    self.rebuild_sections_from_state();
                }
                self.persist();
                self.notify_workspace_structure_changed();
                if let Some(ref callback) = *self.imp().message_callback.borrow() {
                    callback("Folder removed from workspace", NotificationSeverity::Info);
                }
            }
            Err(error) => self.emit_remove_folder_error(error),
        }
    }

    /// Reorder one folder membership inside an existing workspace.
    pub(super) fn handle_reorder_folder_in_workspace(
        &self,
        workspace_id: &WorkspaceId,
        folder_id: &WorkspaceFolderId,
        direction: WorkspaceFolderMoveDirection,
    ) {
        let (result, reordered_folders) = {
            let mut workspaces = self.imp().workspaces_file.borrow_mut();
            let result = workspace_manager::move_folder_in_workspace(
                &mut workspaces,
                workspace_id,
                folder_id,
                direction,
            );
            let reordered_folders = result.as_ref().ok().and_then(|()| {
                workspaces
                    .workspace(workspace_id)
                    .map(|workspace| workspace.folders.clone())
            });
            (result, reordered_folders)
        };

        self.finish_folder_reorder(workspace_id, result, reordered_folders);
    }

    /// Reorder one folder membership to a concrete post-drop index.
    pub(super) fn handle_reorder_folder_to_index_in_workspace(
        &self,
        workspace_id: &WorkspaceId,
        folder_id: &WorkspaceFolderId,
        new_index: usize,
    ) {
        let (result, reordered_folders) = {
            let mut workspaces = self.imp().workspaces_file.borrow_mut();
            let result = workspace_manager::reorder_folder_in_workspace(
                &mut workspaces,
                workspace_id,
                folder_id,
                new_index,
            );
            let reordered_folders = result.as_ref().ok().and_then(|()| {
                workspaces
                    .workspace(workspace_id)
                    .map(|workspace| workspace.folders.clone())
            });
            (result, reordered_folders)
        };

        self.finish_folder_reorder(workspace_id, result, reordered_folders);
    }

    fn finish_folder_reorder(
        &self,
        workspace_id: &WorkspaceId,
        result: std::result::Result<(), WorkspaceFolderReorderError>,
        reordered_folders: Option<Vec<crate::model::workspace::WorkspaceFolder>>,
    ) {
        match result {
            Ok(()) => {
                if let Some(section) = self
                    .imp()
                    .sections
                    .borrow()
                    .iter()
                    .find(|section| section.workspace_id() == *workspace_id)
                {
                    if let Some(folders) = reordered_folders {
                        section.load_workspace_folders(&folders);
                    } else {
                        self.rebuild_sections_from_state();
                    }
                } else {
                    self.rebuild_sections_from_state();
                }
                self.persist();
                self.notify_workspace_structure_changed();
                if let Some(ref callback) = *self.imp().message_callback.borrow() {
                    callback("Workspace folder order updated", NotificationSeverity::Info);
                }
            }
            Err(error) => self.emit_reorder_folder_error(error),
        }
    }

    fn emit_remove_folder_error(&self, error: WorkspaceFolderRemoveError) {
        let Some(ref callback) = *self.imp().message_callback.borrow() else {
            return;
        };
        match error {
            WorkspaceFolderRemoveError::FolderNotFound => {
                callback(
                    "Folder is no longer in this workspace",
                    NotificationSeverity::Warning,
                );
            }
            WorkspaceFolderRemoveError::WorkspaceNotFound => {
                callback(
                    "Workspace is no longer available",
                    NotificationSeverity::Warning,
                );
            }
        }
    }

    fn emit_reorder_folder_error(&self, error: WorkspaceFolderReorderError) {
        let Some(ref callback) = *self.imp().message_callback.borrow() else {
            return;
        };
        match error {
            WorkspaceFolderReorderError::AlreadyAtBoundary => {
                callback(
                    "Folder is already at that edge of the workspace",
                    NotificationSeverity::Info,
                );
            }
            WorkspaceFolderReorderError::FolderNotFound => {
                callback(
                    "Folder is no longer in this workspace",
                    NotificationSeverity::Warning,
                );
            }
            WorkspaceFolderReorderError::WorkspaceNotFound => {
                callback(
                    "Workspace is no longer available",
                    NotificationSeverity::Warning,
                );
            }
        }
    }

    fn emit_add_folder_error(&self, error: WorkspaceFolderAddError) {
        let Some(ref callback) = *self.imp().message_callback.borrow() else {
            return;
        };
        match error {
            WorkspaceFolderAddError::DuplicateFolder => {
                callback(
                    "Folder already belongs to this workspace",
                    NotificationSeverity::Warning,
                );
            }
            WorkspaceFolderAddError::WorkspaceNotFound => {
                callback(
                    "Workspace is no longer available",
                    NotificationSeverity::Warning,
                );
            }
            WorkspaceFolderAddError::StaleFolderSnapshot => {}
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace loading, persistence, and section orchestration for the sidebar.
//!
//! This slice keeps the non-dialog workspace lifecycle together: loading from
//! disk, building section widgets, persisting changes, and drill-down layout
//! coordination across sections.

use std::path::{Path, PathBuf};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::workspace::{
    WorkspaceConfig, WorkspaceFolderId, WorkspaceFolderMoveDirection, WorkspaceId, WorkspaceScope,
    WorkspacesFile,
};
use crate::services::notifications::NotificationSeverity;
use crate::services::{json_store, workspace_manager};
use crate::ui::accessibility;
use workspace_manager::{
    WorkspaceFolderAddError, WorkspaceFolderRemoveError, WorkspaceFolderReorderError,
};

use super::{LushtextSidebar, WorkspaceSection};

impl LushtextSidebar {
    /// Load workspaces from disk and build sections.
    pub fn load_workspaces(&self) {
        let data_dir = json_store::data_dir();
        spawn_blocking_then(
            self.clone(),
            move || workspace_manager::load_recovering(&data_dir),
            |sidebar, load| {
                for diagnostic in &load.diagnostics {
                    tracing::warn!("{}", diagnostic.summary());
                }
                if !load.diagnostics.is_empty()
                    && let Some(ref callback) = *sidebar.imp().message_callback.borrow()
                {
                    callback(
                        "Workspace state needed recovery; unsupported metadata was preserved",
                        NotificationSeverity::Warning,
                    );
                }
                let workspaces_file = load.value;
                sidebar.build_sections_from_file(workspaces_file);
                sidebar.notify_workspace_structure_changed();
                sidebar.notify_workspace_scope_changed();
            },
        );
    }

    /// Build workspace sections from a loaded `WorkspacesFile`.
    pub(super) fn build_sections_from_file(&self, workspaces_file: WorkspacesFile) {
        let imp = self.imp();

        let old_sections = imp.sections.borrow().clone();
        let collapsed_section_ids = old_sections
            .iter()
            .filter(|section| section.is_section_body_collapsed())
            .map(WorkspaceSection::workspace_id)
            .collect::<Vec<_>>();
        for section in &old_sections {
            section.stop_workspace_watch();
            imp.sections_box.remove(section);
        }
        imp.sections.borrow_mut().clear();

        for workspace in &workspaces_file.workspaces {
            let section = self.create_section(workspace);
            if collapsed_section_ids
                .iter()
                .any(|workspace_id| workspace_id == &workspace.id)
            {
                section.set_section_body_collapsed(true);
            }
            imp.sections_box.append(&section);
            imp.sections.borrow_mut().push(section);
        }

        let current_scope = workspaces_file.current_scope();
        *imp.workspaces_file.borrow_mut() = workspaces_file;
        *imp.current_scope.borrow_mut() = current_scope.clone();
        *imp.applied_workspace_filter.borrow_mut() = current_scope;
        imp.workspace_filter_animation_active.set(false);
        imp.workspace_list_revealer.set_reveal_child(true);
        self.refresh_workspace_filter_dropdown();
        self.apply_workspace_filter_visibility();
    }

    /// Refresh the top-row workspace selector from the current in-memory state.
    pub(super) fn refresh_workspace_filter_dropdown(&self) {
        let imp = self.imp();
        let workspaces = imp.workspaces_file.borrow();

        let mut options = Vec::with_capacity(workspaces.workspaces.len() + 1);
        let model = gtk4::StringList::new(&[]);
        model.append("All workspaces");
        options.push(WorkspaceScope::All);

        for workspace in &workspaces.workspaces {
            model.append(&workspace.name);
            options.push(WorkspaceScope::workspace(workspace.id.clone()));
        }

        let current_scope = imp.current_scope.borrow().clone();
        let selected_index = options
            .iter()
            .position(|candidate| *candidate == current_scope)
            .unwrap_or(0);

        drop(workspaces);

        imp.syncing_workspace_filter.set(true);
        imp.workspace_filter_dropdown.set_model(Some(&model));
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Workspace selector entries stay far below u32::MAX in practice"
        )]
        imp.workspace_filter_dropdown
            .set_selected(selected_index as u32);
        imp.syncing_workspace_filter.set(false);

        *imp.workspace_filter_options.borrow_mut() = options;

        let tooltip = if selected_index == 0 {
            "All workspaces".to_string()
        } else {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Workspace selector entries stay far below u32::MAX in practice"
            )]
            let selected_index = selected_index as u32;
            model
                .string(selected_index)
                .map_or_else(|| "All workspaces".to_string(), |label| label.to_string())
        };
        imp.workspace_filter_dropdown
            .set_tooltip_text(Some(&tooltip));
        accessibility::set_value_text(&*imp.workspace_filter_dropdown, &tooltip);
    }

    /// Show either every workspace section or only the selected one.
    pub(super) fn apply_workspace_filter_visibility(&self) {
        let current_scope = self.imp().current_scope.borrow().clone();
        for section in self.imp().sections.borrow().iter() {
            let section_visible = current_scope.includes_workspace(&section.workspace_id());
            section.set_visible(section_visible);
            if section_visible {
                section.sync_workspace_folder_reorder_handles();
                section.sync_file_row_states();
            }
        }
        *self.imp().applied_workspace_filter.borrow_mut() = current_scope;
    }

    /// Fade the scrollable workspace list out, swap the filter, then fade it back in.
    pub(super) fn animate_workspace_filter_change(&self) {
        let imp = self.imp();
        if imp.workspace_filter_animation_active.get()
            || imp.sections.borrow().is_empty()
            || imp.applied_workspace_filter.borrow().clone() == imp.current_scope.borrow().clone()
        {
            return;
        }

        imp.workspace_filter_animation_active.set(true);
        imp.workspace_list_revealer.set_reveal_child(false);

        let sidebar_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            let Some(sidebar) = sidebar_weak.upgrade() else {
                return;
            };
            let imp = sidebar.imp();
            if !imp.workspace_filter_animation_active.get()
                || imp.applied_workspace_filter.borrow().clone()
                    == imp.current_scope.borrow().clone()
            {
                return;
            }

            // The revealer normally applies the filter from its
            // child-revealed notification. This timeout is a safety net for
            // test/headless frame clocks where that notification may not fire.
            sidebar.apply_workspace_filter_visibility();
            imp.workspace_list_revealer.set_reveal_child(true);
            imp.workspace_filter_animation_active.set(false);
        });
    }

    /// Rebuild all sidebar sections from the current in-memory workspace file.
    pub(super) fn rebuild_sections_from_state(&self) {
        let current = self.imp().workspaces_file.borrow().clone();
        self.build_sections_from_file(current);
    }

    /// Create one workspace section, load its persisted folders, and wire callbacks.
    pub(super) fn create_section(&self, workspace: &WorkspaceConfig) -> WorkspaceSection {
        let section = WorkspaceSection::new(workspace.id.clone());
        section.set_workspace_name(&workspace.name);
        section.set_file_row_state_snapshot(std::rc::Rc::clone(
            &self.imp().file_row_state_snapshot.borrow(),
        ));
        section.load_workspace_folders(&workspace.folders);
        self.wire_section_callbacks(&section);
        section
    }

    /// Look up the display name of a workspace by ID from the persisted sidebar state.
    pub(super) fn workspace_name_for_id(&self, workspace_id: &WorkspaceId) -> String {
        self.imp()
            .workspaces_file
            .borrow()
            .workspaces
            .iter()
            .find(|workspace| workspace.id == *workspace_id)
            .map(|workspace| workspace.name.clone())
            .unwrap_or_default()
    }

    /// Handle drill-down focus on a folder: auto-collapse others and scroll into view.
    pub(super) fn handle_folder_focused(&self, focused_workspace_id: &WorkspaceId) {
        if self
            .imp()
            .settings
            .boolean(crate::config::keys::WORKSPACE_AUTO_COLLAPSE)
        {
            for section in self.imp().sections.borrow().iter() {
                if section.workspace_id() != *focused_workspace_id {
                    section.collapse_folders();
                }
            }
        }

        if let Some(section) = self
            .imp()
            .sections
            .borrow()
            .iter()
            .find(|section| section.workspace_id() == *focused_workspace_id)
            && let Some(point) = section.compute_point(
                &*self.imp().sections_box,
                &gtk4::graphene::Point::new(0.0, 0.0),
            )
        {
            let adjustment = self.imp().outer_scrolled_window.vadjustment();
            adjustment.set_value(f64::from(point.y()));
        }
    }

    /// Handle confirmed "New Workspace" name entry by creating an empty workspace.
    pub(super) fn handle_new_workspace_name(&self, name: &str) -> Option<WorkspaceId> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }

        let workspace_id = {
            let imp = self.imp();
            let mut workspaces = imp.workspaces_file.borrow_mut();
            let workspace_id = workspaces.add_empty_workspace(name);
            *imp.current_scope.borrow_mut() = workspaces.current_scope();
            workspace_id
        };
        self.persist();
        self.rebuild_sections_from_state();
        self.notify_workspace_structure_changed();
        self.notify_workspace_scope_changed();
        Some(workspace_id)
    }

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

    /// Rename an existing workspace through the shared persistence pipeline.
    pub(super) fn handle_rename_workspace(&self, workspace_id: &WorkspaceId, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }

        let renamed = self
            .imp()
            .workspaces_file
            .borrow_mut()
            .rename_workspace(workspace_id, new_name);
        if !renamed {
            return;
        }

        if let Some(section) = self
            .imp()
            .sections
            .borrow()
            .iter()
            .find(|section| section.workspace_id() == *workspace_id)
        {
            section.set_workspace_name(new_name);
        } else {
            self.rebuild_sections_from_state();
            self.persist();
            self.notify_workspace_structure_changed();
            return;
        }

        self.refresh_workspace_filter_dropdown();
        self.persist();
        self.notify_workspace_structure_changed();
    }

    /// Remove one workspace and persist the normalized scope that remains.
    pub(super) fn handle_remove_workspace(&self, workspace_id: &WorkspaceId) {
        let previous_scope = self.imp().current_scope.borrow().clone();
        let (removed, normalized_scope) = {
            let mut workspaces = self.imp().workspaces_file.borrow_mut();
            let previous_len = workspaces.workspaces.len();
            workspaces.remove_workspace(workspace_id);
            (
                workspaces.workspaces.len() != previous_len,
                workspaces.current_scope(),
            )
        };
        let scope_changed = previous_scope != normalized_scope;
        if !removed && !scope_changed {
            return;
        }
        *self.imp().current_scope.borrow_mut() = normalized_scope;

        if removed {
            let removed_section = {
                let mut sections = self.imp().sections.borrow_mut();
                sections
                    .iter()
                    .position(|section| section.workspace_id() == *workspace_id)
                    .map(|index| sections.remove(index))
            };
            if let Some(section) = removed_section {
                section.stop_workspace_watch();
                self.imp().sections_box.remove(&section);
            } else {
                self.rebuild_sections_from_state();
                self.persist();
                self.notify_workspace_structure_changed();
                if scope_changed {
                    self.notify_workspace_scope_changed();
                }
                return;
            }
        }

        self.refresh_workspace_filter_dropdown();
        self.apply_workspace_filter_visibility();
        self.persist();
        self.notify_workspace_structure_changed();
        if scope_changed {
            self.notify_workspace_scope_changed();
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

    /// Notify the window that workspace structure changed.
    pub(super) fn notify_workspace_structure_changed(&self) {
        if let Some(ref callback) = *self.imp().workspace_structure_changed_callback.borrow() {
            callback();
        }
    }

    /// Notify the window that the current workspace scope changed.
    pub(super) fn notify_workspace_scope_changed(&self) {
        if let Some(ref callback) = *self.imp().workspace_scope_changed_callback.borrow() {
            callback(self.imp().current_scope.borrow().clone());
        }
    }

    /// Apply a selector-driven scope change, persist it, and refresh visible sections.
    pub(super) fn change_scope_from_selector(&self, scope: WorkspaceScope) {
        let mut workspaces = self.imp().workspaces_file.borrow_mut();
        workspaces.set_current_scope(scope);
        let normalized_scope = workspaces.current_scope();
        drop(workspaces);

        if self.imp().current_scope.borrow().clone() == normalized_scope {
            return;
        }

        *self.imp().current_scope.borrow_mut() = normalized_scope;
        self.animate_workspace_filter_change();
        self.persist();
        self.notify_workspace_scope_changed();
    }

    /// Save the current workspace state to disk on a background thread.
    pub(super) fn persist(&self) {
        let imp = self.imp();
        imp.persist_dirty.set(true);
        if imp.persist_inflight.get() {
            return;
        }

        imp.persist_debounce.schedule(
            self,
            Duration::from_millis(super::PERSIST_DEBOUNCE_MS),
            move |sidebar, _| {
                let imp = sidebar.imp();
                if imp.persist_inflight.get() || !imp.persist_dirty.get() {
                    return;
                }

                let data_dir = json_store::data_dir();
                let workspaces_file = imp.workspaces_file.borrow().clone();
                imp.persist_inflight.set(true);
                imp.persist_dirty.set(false);

                spawn_blocking_then(
                    sidebar.clone(),
                    move || workspace_manager::save(&data_dir, &workspaces_file),
                    |sidebar, result| {
                        let imp = sidebar.imp();
                        imp.persist_inflight.set(false);
                        if let Err(error) = result {
                            tracing::error!("Failed to save workspaces: {error}");
                        }
                        if imp.persist_dirty.get() {
                            sidebar.persist();
                        }
                    },
                );
            },
        );
    }
}

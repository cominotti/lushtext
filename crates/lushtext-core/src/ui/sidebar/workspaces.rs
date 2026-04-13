// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace loading, persistence, and section orchestration for the sidebar.
//!
//! This slice keeps the non-dialog workspace lifecycle together: loading from
//! disk, building section widgets, persisting changes, and drill-down layout
//! coordination across sections.

use std::path::Path;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::workspace::{WorkspaceEntry, WorkspaceId, WorkspacesFile};
use crate::services::{async_task, json_store, workspace_manager};

use super::{LushtextSidebar, WorkspaceSection};

impl LushtextSidebar {
    /// Load workspaces from disk and build sections.
    pub fn load_workspaces(&self) {
        let data_dir = json_store::data_dir();
        async_task::spawn_blocking_then(
            self.clone(),
            move || workspace_manager::load(&data_dir).unwrap_or_default(),
            |sidebar, workspaces_file| {
                sidebar.build_sections_from_file(workspaces_file);
                sidebar.notify_workspace_changed();
            },
        );
    }

    /// Build workspace sections from a loaded `WorkspacesFile`.
    pub(super) fn build_sections_from_file(&self, workspaces_file: WorkspacesFile) {
        let imp = self.imp();

        let old_sections = imp.sections.borrow().clone();
        for section in &old_sections {
            imp.sections_box.remove(section);
        }
        imp.sections.borrow_mut().clear();

        for workspace in &workspaces_file.workspaces {
            let section =
                self.create_section(workspace.id.clone(), &workspace.name, &workspace.entries);
            imp.sections_box.append(&section);
            imp.sections.borrow_mut().push(section);
        }

        *imp.workspaces_file.borrow_mut() = workspaces_file;
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
        options.push(None);

        for workspace in &workspaces.workspaces {
            model.append(&workspace.name);
            options.push(Some(workspace.id.clone()));
        }

        let selected_filter = imp.selected_workspace_filter.borrow().clone();
        let selected_index = selected_filter
            .as_ref()
            .and_then(|selected_id| {
                options
                    .iter()
                    .position(|candidate| candidate.as_ref() == Some(selected_id))
            })
            .unwrap_or(0);

        drop(workspaces);

        imp.syncing_workspace_filter.set(true);
        imp.workspace_filter_dropdown.set_model(Some(&model));
        #[expect(clippy::cast_possible_truncation)] // selector length is far below u32::MAX
        imp.workspace_filter_dropdown
            .set_selected(selected_index as u32);
        imp.syncing_workspace_filter.set(false);

        *imp.workspace_filter_options.borrow_mut() = options.clone();
        *imp.selected_workspace_filter.borrow_mut() = options[selected_index].clone();

        let tooltip = if selected_index == 0 {
            "All workspaces".to_string()
        } else {
            #[expect(clippy::cast_possible_truncation)] // selector length is far below u32::MAX
            let selected_index = selected_index as u32;
            model
                .string(selected_index)
                .map_or_else(|| "All workspaces".to_string(), |label| label.to_string())
        };
        imp.workspace_filter_dropdown
            .set_tooltip_text(Some(&tooltip));
    }

    /// Show either every workspace section or only the selected one.
    pub(super) fn apply_workspace_filter_visibility(&self) {
        let selected_filter = self.imp().selected_workspace_filter.borrow().clone();
        for section in self.imp().sections.borrow().iter() {
            let visible = selected_filter
                .as_ref()
                .is_none_or(|workspace_id| section.workspace_id() == *workspace_id);
            section.set_visible(visible);
        }
        *self.imp().applied_workspace_filter.borrow_mut() = selected_filter;
    }

    /// Fade the scrollable workspace list out, swap the filter, then fade it back in.
    pub(super) fn animate_workspace_filter_change(&self) {
        let imp = self.imp();
        if imp.workspace_filter_animation_active.get()
            || imp.sections.borrow().is_empty()
            || imp.applied_workspace_filter.borrow().clone()
                == imp.selected_workspace_filter.borrow().clone()
        {
            return;
        }

        imp.workspace_filter_animation_active.set(true);
        imp.workspace_list_revealer.set_reveal_child(false);
    }

    /// Rebuild all sidebar sections from the current in-memory workspace file.
    pub(super) fn rebuild_sections_from_state(&self) {
        let current = self.imp().workspaces_file.borrow().clone();
        self.build_sections_from_file(current);
    }

    /// Create a single workspace section, load its roots, and wire callbacks.
    pub(super) fn create_section(
        &self,
        workspace_id: WorkspaceId,
        name: &str,
        roots: &[WorkspaceEntry],
    ) -> WorkspaceSection {
        let section = WorkspaceSection::new(workspace_id);
        section.set_workspace_name(name);

        if !roots.is_empty() {
            section.load_roots(roots);
        }

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
                    section.collapse_roots();
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

    /// Handle "New Workspace" creation after a folder is selected.
    pub(super) fn handle_new_workspace(&self, path: &Path) {
        let imp = self.imp();
        let name = folder_display_name(path);
        let root_entry = WorkspaceEntry::Directory {
            path: path.to_path_buf(),
        };

        {
            let mut workspaces = imp.workspaces_file.borrow_mut();
            let workspace_id = workspaces.add_workspace(&name);
            workspaces.add_entry(&workspace_id, root_entry.clone());
        }
        self.persist();
        self.rebuild_sections_from_state();
        self.notify_workspace_changed();
    }

    /// Notify the window that workspace structure changed.
    pub(super) fn notify_workspace_changed(&self) {
        if let Some(ref callback) = *self.imp().workspace_changed_callback.borrow() {
            callback();
        }
    }

    /// Save the current workspace state to disk on a background thread.
    pub(super) fn persist(&self) {
        let imp = self.imp();
        imp.persist_dirty.set(true);
        if imp.persist_inflight.get() {
            return;
        }

        let generation = imp.persist_generation.get().wrapping_add(1);
        imp.persist_generation.set(generation);

        let sidebar_weak = self.downgrade();
        glib::timeout_add_local_once(
            Duration::from_millis(super::PERSIST_DEBOUNCE_MS),
            move || {
                let Some(sidebar) = sidebar_weak.upgrade() else {
                    return;
                };
                let imp = sidebar.imp();
                if imp.persist_inflight.get()
                    || imp.persist_generation.get() != generation
                    || !imp.persist_dirty.get()
                {
                    return;
                }

                let data_dir = json_store::data_dir();
                let workspaces_file = imp.workspaces_file.borrow().clone();
                imp.persist_inflight.set(true);
                imp.persist_dirty.set(false);

                async_task::spawn_blocking_then(
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

/// Extract a display name from a path's last component.
pub(super) fn folder_display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || "New Workspace".to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

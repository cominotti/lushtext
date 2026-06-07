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

use crate::model::workspace::{WorkspaceId, WorkspaceScope, WorkspacesFile};
use crate::services::notifications::NotificationSeverity;
use crate::services::{async_task, json_store, workspace_manager};

use super::{LushtextSidebar, WorkspaceSection};

impl LushtextSidebar {
    /// Load workspaces from disk and build sections.
    pub fn load_workspaces(&self) {
        let data_dir = json_store::data_dir();
        async_task::spawn_blocking_then(
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
        for section in &old_sections {
            imp.sections_box.remove(section);
        }
        imp.sections.borrow_mut().clear();

        for workspace in &workspaces_file.workspaces {
            let section =
                self.create_section(workspace.id.clone(), &workspace.name, &workspace.root);
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
    }

    /// Show either every workspace section or only the selected one.
    pub(super) fn apply_workspace_filter_visibility(&self) {
        let current_scope = self.imp().current_scope.borrow().clone();
        for section in self.imp().sections.borrow().iter() {
            section.set_visible(current_scope.includes_workspace(&section.workspace_id()));
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

    /// Create a single workspace section, load its root, and wire callbacks.
    pub(super) fn create_section(
        &self,
        workspace_id: WorkspaceId,
        name: &str,
        root: &Path,
    ) -> WorkspaceSection {
        let section = WorkspaceSection::new(workspace_id);
        section.set_workspace_name(name);
        section.load_workspace_root(root);
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

        {
            let mut workspaces = imp.workspaces_file.borrow_mut();
            workspaces.add_workspace(&name, path.to_path_buf());
            *imp.current_scope.borrow_mut() = workspaces.current_scope();
        }
        self.persist();
        self.rebuild_sections_from_state();
        self.notify_workspace_structure_changed();
        self.notify_workspace_scope_changed();
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
        || "Workspace".to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

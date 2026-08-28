// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's **workspace-list** stage
//! orders: load, and add / rename / unlist.
//!
//! # Role
//!
//! Coordination, `execution`, qualified by the stage orders it serves. The workspace
//! list is the structure the file tree lives inside: adding or unlisting a workspace
//! creates and destroys the very sections `workspace_section/` coordinates, and
//! `load_workspaces` is the single entry point for both halves of this row. That
//! shared structure is why this row was **not** split into two census rows.
//!
//! # Inversion to be aware of
//!
//! `load_workspaces` captures the persistence request generation **before** dispatch
//! and refuses to adopt the loaded file when a mutation superseded it. Without that
//! guard, adopting a load would revert an in-memory workspace the user just created
//! while its write was still pending. Control resumes in the worker completion, not
//! at the call site.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::workspace::{WorkspaceConfig, WorkspaceId, WorkspacesFile};
use crate::services::notifications::NotificationSeverity;
use crate::services::{json_store, workspace_manager};

use crate::ui::sidebar::policy;

use super::{LushtextSidebar, WorkspaceSection};

impl LushtextSidebar {
    /// Apply one operation to the first section the workspace scope filter leaves
    /// visible, returning whether it was applied.
    ///
    /// Four keyboard and automation entry points wanted the same thing — "do this to
    /// whichever section the user can actually see" — and each had spelled out the same
    /// `sections.borrow().iter().find(is_visible)` walk in the facade. Naming it once
    /// here keeps section visibility a list-level concern and lets the facade delegate
    /// in one line per entry point.
    pub(super) fn with_first_visible_section<F>(&self, apply: F) -> bool
    where
        F: FnOnce(&WorkspaceSection) -> bool,
    {
        self.imp()
            .sections
            .borrow()
            .iter()
            .find(|section| section.is_visible())
            .is_some_and(apply)
    }

    /// Whether any section still has watcher lifecycle, mailbox, or refresh work.
    pub(super) fn any_section_blocks_refresh_readiness(&self) -> bool {
        self.imp()
            .sections
            .borrow()
            .iter()
            .any(WorkspaceSection::workspace_refresh_blocks_readiness)
    }

    /// Load workspaces from disk and build sections.
    pub fn load_workspaces(&self) {
        let data_dir = json_store::data_dir();
        // Capture the newest requested mutation *before* dispatching the load.
        // "New Workspace" is reachable from window present, so a user can create
        // a workspace while this load is in flight — and `build_sections_from_file`
        // unconditionally overwrites `workspaces_file`, which would discard that
        // workspace from memory while `persist()` has already scheduled it for
        // disk. The mismatch is what makes it data loss rather than a stale view.
        let requested_at_dispatch = self.imp().persistence.borrow().requested_generation();
        spawn_blocking_then(
            self.clone(),
            move || {
                let load = workspace_manager::load_recovering(&data_dir);
                // Delay *after* the read, not before it: M-4 is about adopting a
                // snapshot that has since gone stale, so the worker must carry the
                // pre-mutation state across the window a test interposes into.
                #[cfg(feature = "test-utils")]
                crate::ui::sidebar::test_policy::delay_load_worker();
                load
            },
            move |sidebar, load| {
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
                // A mutation arrived while the load was running: the in-memory state
                // holds something newer than what came off disk, and `persist()` has
                // already scheduled it. Neither side may simply win.
                //
                // **Discarding the load is not safe**, which is the trap the first
                // version of this guard fell into. `workspaces_file` starts *empty*,
                // and `start_persist_worker` snapshots it when the worker starts —
                // so if the mutation landed before the very first load, discarding the
                // load leaves memory holding only the new workspace, and the pending
                // write then commits that over every workspace on disk. Adopting the
                // load is not safe either: it reverts the mutation the user just made.
                //
                // So merge, with the loaded file as the base and the newer in-memory
                // workspaces winning on id collision.
                if sidebar.imp().persistence.borrow().requested_generation()
                    != requested_at_dispatch
                {
                    // Whether merging is safe depends on what "absent from memory"
                    // means, and exactly one bit distinguishes the two cases. The
                    // decision itself is pure policy, so it carries mutation coverage.
                    match policy::superseded_load_action(sidebar.imp().load_adopted.get()) {
                        policy::SupersededLoadAction::KeepMemory => {
                            tracing::info!(
                                "Skipping workspace load adoption: a mutation superseded \
                                 it and memory already holds the full list"
                            );
                        }
                        policy::SupersededLoadAction::MergeAndPersist => {
                            tracing::info!(
                                "Merging workspace load with a mutation that superseded it"
                            );
                            let in_memory = sidebar.imp().workspaces_file.borrow().clone();
                            sidebar.adopt_loaded_workspaces(
                                policy::merge_superseded_workspace_load(load.value, in_memory),
                            );
                            // The merged state differs from what the superseding write
                            // put on disk — that write carried only the in-memory side —
                            // so it must be persisted, not merely shown.
                            sidebar.persist();
                        }
                    }
                    sidebar.notify_workspace_structure_changed();
                    sidebar.notify_workspace_scope_changed();
                    return;
                }
                let workspaces_file = load.value;
                sidebar.adopt_loaded_workspaces(workspaces_file);
                sidebar.notify_workspace_structure_changed();
                sidebar.notify_workspace_scope_changed();
            },
        );
    }

    /// Adopt a completed workspace **load** into memory and rebuild the sections.
    ///
    /// The only path allowed to record that a load has been adopted. Sharing
    /// `build_sections_from_file` with every mutation is what made the M-4 guard inert:
    /// the bit `superseded_load_action` reads must mean "a load was adopted", not
    /// "sections were rebuilt from some file", and only this entry point can honestly
    /// claim the former.
    fn adopt_loaded_workspaces(&self, workspaces_file: WorkspacesFile) {
        self.build_sections_from_file(workspaces_file);
        // From here on, a workspace absent from memory means the user removed it — not
        // that nothing has been loaded. `merge_superseded_workspace_load` depends on it.
        self.imp().load_adopted.set(true);
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
        // Deliberately does **not** record load adoption: every mutation reaches here
        // through `rebuild_sections_from_state`, so recording it here would make the
        // M-4 guard read "sections were rebuilt" while its parameter names "a load was
        // adopted". Only `adopt_loaded_workspaces` may set that bit.
        *imp.current_scope.borrow_mut() = current_scope.clone();
        *imp.applied_workspace_filter.borrow_mut() = current_scope;
        imp.workspace_filter_animation_active.set(false);
        imp.workspace_list_revealer.set_reveal_child(true);
        self.refresh_workspace_filter_dropdown();
        self.apply_workspace_filter_visibility();
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

    /// Notify the window that workspace structure changed.
    pub(super) fn notify_workspace_structure_changed(&self) {
        if let Some(ref callback) = *self.imp().workspace_structure_changed_callback.borrow() {
            callback();
        }
    }
}

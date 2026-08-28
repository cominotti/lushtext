// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's **scope-filter** stage order.
//!
//! # Role
//!
//! Coordination, `execution`, qualified by the stage order it serves: the workspace
//! scope filter's dropdown projection, its visibility application, and the fade
//! sequence with its settle timer.
//!
//! # State this module owns for automation
//!
//! `workspace_filter_animation_active` lives here in spirit and on the subclass in
//! fact. It is the source of the `workspace-filter-animation` readiness blocker and
//! the `filter_animation_active` snapshot field, and its primary resumption point is
//! the revealer's `child-revealed` notification — the headless safety-net timer is a
//! fallback, not the main path.
//!
//! Do not confuse it with `workspace-sidebar-animation`, which is
//! `WFR-SHELL-LAYOUT`'s: that blocker follows the sidebar show/hide animation, not
//! this row.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::workspace::WorkspaceScope;
use crate::ui::accessibility;

use super::LushtextSidebar;

impl LushtextSidebar {
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

        imp.workspace_filter_settle_timer.arm(
            self,
            Duration::from_millis(300),
            move |sidebar, _token| {
                let imp = sidebar.imp();
                if !imp.workspace_filter_animation_active.get()
                    || imp.applied_workspace_filter.borrow().clone()
                        == imp.current_scope.borrow().clone()
                {
                    return;
                }

                // The revealer normally applies the filter from its
                // child-revealed notification. This timer is a safety net for
                // test/headless frame clocks where that notification may not
                // fire; a superseding re-arm drops the stale source instead of
                // relying on the boolean guard to neuter it.
                sidebar.apply_workspace_filter_visibility();
                imp.workspace_list_revealer.set_reveal_child(true);
                imp.workspace_filter_animation_active.set(false);
            },
        );
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
}

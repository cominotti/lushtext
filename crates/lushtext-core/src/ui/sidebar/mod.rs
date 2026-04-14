// SPDX-License-Identifier: GPL-3.0-or-later

//! Multi-workspace sidebar: fixed affordances, workspace sections, and persistence.
//!
//! The public widget facade stays here, while section callback forwarding,
//! workspace lifecycle flows, and folder/workspace dialogs live in dedicated
//! sibling modules to keep this driving adapter easier to navigate.

mod callbacks;
mod dialogs;
pub mod file_tree_item;
mod imp;
pub mod workspace_section;
mod workspaces;

use std::path::{Path, PathBuf};

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::model::workspace::WorkspaceEntry;
use crate::services::notifications::NotificationSeverity;

pub use file_tree_item::FileTreeItem;
pub use workspace_section::LushtextWorkspaceSection as WorkspaceSection;

/// Debounce interval for persisting workspace changes to disk (ms).
pub(super) const PERSIST_DEBOUNCE_MS: u64 = 150;

/// Supported fixed-width presets for the workspace sidebar footer controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceSidebarWidthPreset {
    Small,
    Comfy,
    Large,
}

impl WorkspaceSidebarWidthPreset {
    pub const DEFAULT: Self = Self::Comfy;

    #[must_use]
    pub fn fraction(self) -> f64 {
        match self {
            Self::Small => 0.2,
            Self::Comfy => 0.3,
            Self::Large => 0.4,
        }
    }

    #[must_use]
    pub fn from_fraction(fraction: f64) -> Self {
        let small_delta = (fraction - Self::Small.fraction()).abs();
        let comfy_delta = (fraction - Self::Comfy.fraction()).abs();
        let large_delta = (fraction - Self::Large.fraction()).abs();
        let min_delta = small_delta.min(comfy_delta.min(large_delta));

        if (comfy_delta - min_delta).abs() < f64::EPSILON {
            Self::Comfy
        } else if (small_delta - min_delta).abs() < f64::EPSILON {
            Self::Small
        } else {
            Self::Large
        }
    }
}

glib::wrapper! {
    pub struct LushtextSidebar(ObjectSubclass<imp::LushtextSidebar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextSidebar {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Remove a file/directory from the correct workspace section's model.
    pub fn remove_from_model(&self, target_path: &Path) {
        for section in self.imp().sections.borrow().iter() {
            if section.remove_from_model(target_path) {
                return;
            }
        }
    }

    pub fn connect_file_activated<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().file_activated_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_file_renamed<F: Fn(&Path, &Path) + 'static>(&self, f: F) {
        *self.imp().rename_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_file_deleted<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().delete_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_file_created<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().create_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_message<F: Fn(&str, NotificationSeverity) + 'static>(&self, f: F) {
        *self.imp().message_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_workspace_changed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().workspace_changed_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_width_preset_selected<F: Fn(WorkspaceSidebarWidthPreset) + 'static>(
        &self,
        f: F,
    ) {
        *self.imp().width_preset_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Sync the footer toggle state without re-emitting the window callback.
    pub fn set_width_preset(&self, preset: WorkspaceSidebarWidthPreset) {
        self.apply_width_preset_selection(preset, false);
    }

    /// Collect all directory root paths from all workspaces.
    #[must_use]
    pub fn workspace_roots(&self) -> Vec<PathBuf> {
        self.imp()
            .workspaces_file
            .borrow()
            .workspaces
            .iter()
            .flat_map(|workspace| workspace.entries.iter())
            .filter_map(|entry| match entry {
                WorkspaceEntry::Directory { path } => Some(path.clone()),
                WorkspaceEntry::File { .. } => None,
            })
            .collect()
    }

    /// Collect the currently selected workspace scope paths.
    ///
    /// Directories are returned as directory roots and file entries are returned
    /// as exact file paths, so window-level note workflows can stay aligned
    /// with the active workspace filter instead of inferring scope elsewhere.
    #[must_use]
    pub fn filtered_workspace_scope_paths(&self) -> Vec<PathBuf> {
        let selected_filter = self.imp().selected_workspace_filter.borrow().clone();
        self.imp()
            .workspaces_file
            .borrow()
            .workspaces
            .iter()
            .filter(|workspace| {
                selected_filter
                    .as_ref()
                    .is_none_or(|workspace_id| workspace.id == *workspace_id)
            })
            .flat_map(|workspace| workspace.entries.iter())
            .map(|entry| match entry {
                WorkspaceEntry::Directory { path } | WorkspaceEntry::File { path } => path.clone(),
            })
            .collect()
    }

    fn select_width_preset(&self, preset: WorkspaceSidebarWidthPreset) {
        self.apply_width_preset_selection(preset, true);
    }

    fn apply_width_preset_selection(&self, preset: WorkspaceSidebarWidthPreset, emit: bool) {
        let imp = self.imp();
        if imp.syncing_width_preset.get() {
            return;
        }

        imp.syncing_width_preset.set(true);
        imp.small_width_button
            .set_active(matches!(preset, WorkspaceSidebarWidthPreset::Small));
        imp.comfy_width_button
            .set_active(matches!(preset, WorkspaceSidebarWidthPreset::Comfy));
        imp.large_width_button
            .set_active(matches!(preset, WorkspaceSidebarWidthPreset::Large));
        imp.syncing_width_preset.set(false);

        if emit && let Some(ref callback) = *imp.width_preset_callback.borrow() {
            callback(preset);
        }
    }
}

impl Default for LushtextSidebar {
    fn default() -> Self {
        Self::new()
    }
}

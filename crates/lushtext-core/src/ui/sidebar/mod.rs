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

use crate::model::workspace::{WorkspaceId, WorkspaceScope, WorkspacesFile};
use crate::services::notifications::NotificationSeverity;

pub use file_tree_item::FileTreeItem;
pub use workspace_section::LushtextWorkspaceSection as WorkspaceSection;

/// Debounce interval for persisting workspace changes to disk (ms).
pub(super) const PERSIST_DEBOUNCE_MS: u64 = 150;

/// Supported named workspace sidebar presets used by Preferences and shell math.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceSidebarWidthPreset {
    Small,
    Comfy,
    Large,
}

impl WorkspaceSidebarWidthPreset {
    pub const DEFAULT: Self = Self::Comfy;
    pub const ALL: [Self; 3] = [Self::Small, Self::Comfy, Self::Large];

    /// Return the user-visible label for the preset picker.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Comfy => "Comfy",
            Self::Large => "Large",
        }
    }

    /// Return the stored preset hint fraction used to identify the selected preset.
    #[must_use]
    pub const fn fraction(self) -> f64 {
        match self {
            Self::Small => 0.2,
            Self::Comfy => 0.3,
            Self::Large => 0.4,
        }
    }

    /// Map an arbitrary stored fraction back onto the nearest supported preset.
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

    /// Convert the preset into a stable position for Adwaita combo rows.
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Small => 0,
            Self::Comfy => 1,
            Self::Large => 2,
        }
    }

    /// Convert a combo-row selection back into a workspace width preset.
    #[must_use]
    pub const fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Self::Small),
            1 => Some(Self::Comfy),
            2 => Some(Self::Large),
            _ => None,
        }
    }

    /// Lower bound for this preset once the sidebar is side-by-side on desktop widths.
    #[must_use]
    pub const fn min_width_sp(self) -> f64 {
        match self {
            Self::Small => 220.0,
            Self::Comfy => 280.0,
            Self::Large => 340.0,
        }
    }

    /// Upper bound that keeps the sidebar comfortable on wide and ultrawide windows.
    #[must_use]
    pub const fn max_width_sp(self) -> f64 {
        match self {
            Self::Small => 280.0,
            Self::Comfy => 360.0,
            Self::Large => 440.0,
        }
    }

    /// Convert the preset's hint fraction into a bounded visible width for the current window.
    #[must_use]
    pub fn clamped_width_sp(self, window_width: i32) -> f64 {
        (f64::from(window_width.max(1)) * self.fraction())
            .clamp(self.min_width_sp(), self.max_width_sp())
    }

    /// Return the effective split-view fraction after clamping this preset for the window width.
    #[must_use]
    pub fn effective_fraction(self, window_width: i32) -> f64 {
        (self.clamped_width_sp(window_width) / f64::from(window_width.max(1))).min(1.0)
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

    pub fn connect_local_history_requested<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().local_history_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_document_note_requested<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().document_note_callback.borrow_mut() = Some(Box::new(f));
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

    pub fn connect_workspace_note_requested<F: Fn(WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().workspace_note_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_workspace_structure_changed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().workspace_structure_changed_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store a callback invoked whenever the current workspace scope changes.
    pub fn connect_workspace_scope_changed<F: Fn(WorkspaceScope) + 'static>(&self, f: F) {
        *self.imp().workspace_scope_changed_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Return the current workspace scope mirrored by the sidebar shell.
    #[must_use]
    pub fn current_scope(&self) -> WorkspaceScope {
        self.imp().current_scope.borrow().clone()
    }

    /// Collect all persisted workspace roots regardless of the current scope.
    #[must_use]
    pub fn all_workspace_root_paths(&self) -> Vec<PathBuf> {
        self.imp()
            .workspaces_file
            .borrow()
            .all_workspace_root_paths()
    }

    /// Collect the workspace roots covered by one explicit scope.
    #[must_use]
    pub fn root_paths_for_scope(&self, scope: &WorkspaceScope) -> Vec<PathBuf> {
        self.imp()
            .workspaces_file
            .borrow()
            .root_paths_for_scope(scope)
    }

    /// Collect the current scope's workspace roots.
    #[must_use]
    pub fn current_scope_root_paths(&self) -> Vec<PathBuf> {
        let scope = self.current_scope();
        self.root_paths_for_scope(&scope)
    }

    /// Return a snapshot of the current persisted workspace state.
    #[must_use]
    pub fn workspaces_file(&self) -> WorkspacesFile {
        self.imp().workspaces_file.borrow().clone()
    }
}

impl Default for LushtextSidebar {
    fn default() -> Self {
        Self::new()
    }
}

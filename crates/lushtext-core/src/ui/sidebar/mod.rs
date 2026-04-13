// SPDX-License-Identifier: GPL-3.0-or-later

//! Multi-workspace sidebar: orchestrates workspace sections, persistence,
//! the fixed "New Workspace" affordance, and the fixed width-preset footer.

pub mod file_tree_item;
// Private implementation module (GObject pattern).
mod imp;
pub mod workspace_section;

use crate::model::workspace::{WorkspaceEntry, WorkspaceId, WorkspacesFile};
use crate::services::{async_task, json_store, workspace_manager};
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;
use workspace_section::LushtextWorkspaceSection;

// Re-export for window integration
pub use file_tree_item::FileTreeItem;
pub use workspace_section::LushtextWorkspaceSection as WorkspaceSection;

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

// glib::wrapper! generates the public wrapper type for this widget.
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

    /// Load workspaces from disk and build sections.
    /// Called once from window `constructed()`. Runs I/O on a background thread
    /// to avoid blocking the UI on slow filesystems (NFS, USB).
    pub fn load_workspaces(&self) {
        let data_dir = json_store::data_dir();
        crate::services::async_task::spawn_blocking_then(
            self.clone(),
            move || workspace_manager::load(&data_dir).unwrap_or_default(),
            |sidebar, workspaces_file| {
                sidebar.build_sections_from_file(workspaces_file);
                sidebar.notify_workspace_changed();
            },
        );
    }

    /// Create a new workspace by opening a folder dialog.
    /// Called from the fixed top button and from `win.open-folder`.
    pub fn create_new_workspace(&self) {
        let Some(root) = self.root() else { return };
        let Some(window) = root.downcast_ref::<gtk4::Window>() else {
            return;
        };

        let dialog = gtk4::FileDialog::builder()
            .title("Open Folder")
            .modal(true)
            .build();

        let sidebar_weak = self.downgrade();
        dialog.select_folder(Some(window), gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
                && let Some(sidebar) = sidebar_weak.upgrade()
            {
                sidebar.handle_new_workspace(path);
            }
        });
    }

    /// Remove a file/directory from the correct workspace section's model.
    pub fn remove_from_model(&self, target_path: &Path) {
        for section in self.imp().sections.borrow().iter() {
            if section.remove_from_model(target_path) {
                return;
            }
        }
    }

    // --- Callback registration (forwarded to window) ---

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
    /// Used by the window to build the command palette's file index.
    #[must_use]
    pub fn workspace_roots(&self) -> Vec<PathBuf> {
        use crate::model::workspace::WorkspaceEntry;
        self.imp()
            .workspaces_file
            .borrow()
            .workspaces
            .iter()
            .flat_map(|ws| ws.entries.iter())
            .filter_map(|entry| match entry {
                WorkspaceEntry::Directory { path } => Some(path.clone()),
                WorkspaceEntry::File { .. } => None,
            })
            .collect()
    }

    // --- Internal orchestration ---

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

        if emit && let Some(ref cb) = *imp.width_preset_callback.borrow() {
            cb(preset);
        }
    }

    /// Build workspace sections from a loaded WorkspacesFile.
    fn build_sections_from_file(&self, workspaces_file: WorkspacesFile) {
        let imp = self.imp();

        // Clear existing sections
        let old_sections = imp.sections.borrow().clone();
        for section in &old_sections {
            imp.sections_box.remove(section);
        }
        imp.sections.borrow_mut().clear();

        // Create a section for each workspace
        for ws_config in &workspaces_file.workspaces {
            let section = self.create_section(
                ws_config.id.clone(),
                &ws_config.name,
                &ws_config
                    .entries
                    .iter()
                    .map(|e| (e.path().to_path_buf(), e.is_dir()))
                    .collect::<Vec<_>>(),
            );
            imp.sections_box.append(&section);
            imp.sections.borrow_mut().push(section);
        }

        *imp.workspaces_file.borrow_mut() = workspaces_file;
    }

    /// Create a single workspace section, load its roots, and wire callbacks.
    fn create_section(
        &self,
        ws_id: WorkspaceId,
        name: &str,
        roots: &[(PathBuf, bool)],
    ) -> LushtextWorkspaceSection {
        let section = LushtextWorkspaceSection::new(ws_id);
        section.set_workspace_name(name);

        if !roots.is_empty() {
            section.load_roots(roots);
        }

        self.wire_section_callbacks(&section);
        section
    }

    /// Wire a section's callbacks to forward file operations to the sidebar's callbacks,
    /// and workspace operations (add-folder, rename, unlist) to the sidebar's handlers.
    fn wire_section_callbacks(&self, section: &LushtextWorkspaceSection) {
        // File activated → forward to window
        let sidebar_weak = self.downgrade();
        section.connect_file_activated(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade()
                && let Some(ref cb) = *sidebar.imp().file_activated_callback.borrow()
            {
                cb(path);
            }
        });

        // File renamed → forward to window
        let sidebar_weak = self.downgrade();
        section.connect_file_renamed(move |old, new| {
            if let Some(sidebar) = sidebar_weak.upgrade()
                && let Some(ref cb) = *sidebar.imp().rename_callback.borrow()
            {
                cb(old, new);
            }
        });

        // File deleted → forward to window
        let sidebar_weak = self.downgrade();
        section.connect_file_deleted(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade()
                && let Some(ref cb) = *sidebar.imp().delete_callback.borrow()
            {
                cb(path);
            }
        });

        // File created → forward to window
        let sidebar_weak = self.downgrade();
        section.connect_file_created(move |path| {
            if let Some(sidebar) = sidebar_weak.upgrade()
                && let Some(ref cb) = *sidebar.imp().create_callback.borrow()
            {
                cb(path);
            }
        });

        // Add folder to this workspace
        let sidebar_weak = self.downgrade();
        section.connect_add_folder_requested(move |ws_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_add_folder(ws_id);
            }
        });

        // Rename workspace
        let sidebar_weak = self.downgrade();
        section.connect_rename_workspace_requested(move |ws_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.show_rename_workspace_dialog(ws_id);
            }
        });

        // Unlist workspace
        let sidebar_weak = self.downgrade();
        section.connect_unlist_workspace_requested(move |ws_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.show_unlist_workspace_dialog(ws_id);
            }
        });

        // Folder focused (drill-down)
        let sidebar_weak = self.downgrade();
        section.connect_folder_focused(move |ws_id| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.handle_folder_focused(ws_id);
            }
        });
    }

    // --- Helpers ---

    /// Get the parent GtkWindow for presenting dialogs.
    fn parent_window(&self) -> Option<gtk4::Window> {
        self.root().and_then(|r| r.downcast::<gtk4::Window>().ok())
    }

    /// Look up the display name of a workspace section by ID.
    fn workspace_name_for_id(&self, ws_id: &WorkspaceId) -> String {
        let sections = self.imp().sections.borrow();
        sections
            .iter()
            .find(|s| s.workspace_id() == *ws_id)
            .map(workspace_section::LushtextWorkspaceSection::workspace_name)
            .unwrap_or_default()
    }

    /// Apply a function to the section matching the given workspace ID.
    fn with_section(&self, ws_id: &WorkspaceId, f: impl FnOnce(&LushtextWorkspaceSection)) {
        let sections = self.imp().sections.borrow();
        if let Some(section) = sections.iter().find(|s| s.workspace_id() == *ws_id) {
            f(section);
        }
    }

    // --- Workspace lifecycle ---

    /// Handle drill-down focus on a folder: auto-collapse others and scroll into view.
    fn handle_folder_focused(&self, focused_ws_id: &WorkspaceId) {
        if self
            .imp()
            .settings
            .boolean(crate::config::keys::WORKSPACE_AUTO_COLLAPSE)
        {
            for section in self.imp().sections.borrow().iter() {
                if section.workspace_id() != *focused_ws_id {
                    section.collapse_roots();
                }
            }
        }

        // Scroll the focused section to the top
        if let Some(section) = self
            .imp()
            .sections
            .borrow()
            .iter()
            .find(|s| s.workspace_id() == *focused_ws_id)
            && let Some(point) = section.compute_point(
                &*self.imp().sections_box,
                &gtk4::graphene::Point::new(0.0, 0.0),
            )
        {
            let adj = self.imp().outer_scrolled_window.vadjustment();
            adj.set_value(point.y() as f64);
        }
    }

    /// Handle "New Workspace" creation after a folder is selected.
    fn handle_new_workspace(&self, path: PathBuf) {
        let imp = self.imp();
        let name = folder_display_name(&path);

        let ws_id = {
            let mut wf = imp.workspaces_file.borrow_mut();
            let ws_id = wf.add_workspace(&name);
            wf.add_entry(&ws_id, WorkspaceEntry::Directory { path: path.clone() });
            ws_id
        };
        self.persist();

        let section = self.create_section(ws_id, &name, &[(path, true)]);
        imp.sections_box.append(&section);
        imp.sections.borrow_mut().push(section);
        self.notify_workspace_changed();
    }

    /// Handle "Replace Workspace Root" for an existing workspace.
    fn handle_add_folder(&self, ws_id: &WorkspaceId) {
        let Some(window) = self.parent_window() else {
            return;
        };

        let has_entries = self
            .imp()
            .workspaces_file
            .borrow()
            .workspaces
            .iter()
            .any(|w| w.id == *ws_id && !w.entries.is_empty());

        let title = if has_entries {
            "Replace Workspace Root"
        } else {
            "Add Folder to Workspace"
        };

        let dialog = gtk4::FileDialog::builder().title(title).modal(true).build();

        let sidebar_weak = self.downgrade();
        let ws_id = ws_id.clone();
        dialog.select_folder(Some(&window), gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
                && let Some(sidebar) = sidebar_weak.upgrade()
            {
                let name = folder_display_name(&path);

                sidebar.imp().workspaces_file.borrow_mut().replace_root(
                    &ws_id,
                    WorkspaceEntry::Directory { path: path.clone() },
                    &name,
                );
                sidebar.persist();

                sidebar.with_section(&ws_id, |section| {
                    section.load_roots(&[(path, true)]);
                    section.set_workspace_name(&name);
                });
                sidebar.notify_workspace_changed();
            }
        });
    }

    /// Show the rename workspace dialog.
    fn show_rename_workspace_dialog(&self, ws_id: &WorkspaceId) {
        let Some(root) = self.root() else { return };
        let current_name = self.workspace_name_for_id(ws_id);

        let dialog = libadwaita::AlertDialog::builder()
            .heading("Rename Workspace")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("rename", "Rename");
        dialog.set_response_appearance("rename", libadwaita::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");

        let entry = gtk4::Entry::new();
        entry.set_text(&current_name);
        entry.set_activates_default(true);
        dialog.set_extra_child(Some(&entry));

        let sidebar_weak = self.downgrade();
        let ws_id = ws_id.clone();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "rename" {
                return;
            }
            let new_name = entry.text();
            let new_name = new_name.trim();
            if new_name.is_empty() {
                return;
            }

            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar
                    .imp()
                    .workspaces_file
                    .borrow_mut()
                    .rename_workspace(&ws_id, new_name);
                sidebar.persist();
                sidebar.with_section(&ws_id, |section| {
                    section.set_workspace_name(new_name);
                });
            }
        });

        dialog.present(Some(&root));
    }

    /// Show the unlist workspace confirmation dialog.
    fn show_unlist_workspace_dialog(&self, ws_id: &WorkspaceId) {
        let Some(root) = self.root() else { return };
        let current_name = self.workspace_name_for_id(ws_id);

        let dialog = libadwaita::AlertDialog::builder()
            .heading(format!("Unlist '{current_name}'?"))
            .body("The workspace will be removed from the sidebar. Files will not be deleted.")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("unlist", "Unlist");
        dialog.set_response_appearance("unlist", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let sidebar_weak = self.downgrade();
        let ws_id = ws_id.clone();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "unlist" {
                return;
            }
            if let Some(sidebar) = sidebar_weak.upgrade() {
                let imp = sidebar.imp();
                imp.workspaces_file.borrow_mut().remove_workspace(&ws_id);
                sidebar.persist();

                let mut sections = imp.sections.borrow_mut();
                if let Some(idx) = sections.iter().position(|s| s.workspace_id() == ws_id) {
                    let section = sections.remove(idx);
                    imp.sections_box.remove(&section);
                }
                drop(sections);
                sidebar.notify_workspace_changed();
            }
        });

        dialog.present(Some(&root));
    }

    /// Notify the window that workspace structure changed (for file index rebuild).
    fn notify_workspace_changed(&self) {
        if let Some(ref cb) = *self.imp().workspace_changed_callback.borrow() {
            cb();
        }
    }

    /// Save the current workspace state to disk on a background thread.
    /// Fire-and-forget: workspace persistence is non-critical and the next
    /// mutation will overwrite the file anyway.
    fn persist(&self) {
        let imp = self.imp();
        imp.persist_dirty.set(true);
        if imp.persist_inflight.get() {
            return;
        }

        let generation = imp.persist_generation.get().wrapping_add(1);
        imp.persist_generation.set(generation);

        let sidebar_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(PERSIST_DEBOUNCE_MS), move || {
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
                    if let Err(e) = result {
                        tracing::error!("Failed to save workspaces: {}", e);
                    }
                    if imp.persist_dirty.get() {
                        sidebar.persist();
                    }
                },
            );
        });
    }
}

/// Extract a display name from a path's last component.
fn folder_display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || "New Workspace".to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl Default for LushtextSidebar {
    fn default() -> Self {
        Self::new()
    }
}

/// Debounce interval for persisting workspace changes to disk (ms).
/// 150ms coalesces rapid mutations (e.g., adding multiple folders)
/// into a single write while keeping perceived save latency low.
const PERSIST_DEBOUNCE_MS: u64 = 150;

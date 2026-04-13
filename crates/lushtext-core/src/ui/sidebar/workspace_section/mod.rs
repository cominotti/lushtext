// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-workspace section widget: header + file tree + context menus.

mod actions;
// Private implementation module (GObject pattern).
mod imp;
mod tree_index;
mod tree_loading;

use super::file_tree_item::FileTreeItem;
use crate::model::workspace::WorkspaceId;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use std::path::{Path, PathBuf};

// glib::wrapper! generates the public wrapper type for this widget.
glib::wrapper! {
    pub struct LushtextWorkspaceSection(ObjectSubclass<imp::LushtextWorkspaceSection>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextWorkspaceSection {
    #[must_use]
    pub fn new(workspace_id: WorkspaceId) -> Self {
        let obj: Self = Object::builder().build();
        *obj.imp().workspace_id.borrow_mut() = workspace_id;
        obj
    }

    pub fn set_workspace_name(&self, name: &str) {
        self.imp().header_label.set_label(name);
    }

    #[must_use]
    pub fn workspace_name(&self) -> String {
        self.imp().header_label.label().to_string()
    }

    #[must_use]
    pub fn workspace_id(&self) -> WorkspaceId {
        self.imp().workspace_id.borrow().clone()
    }

    /// Load root paths into the file tree. Builds the `TreeListModel`
    /// and child models asynchronously for responsive UI.
    pub fn load_roots(&self, roots: &[(PathBuf, bool)]) {
        *self.imp().original_roots.borrow_mut() = roots.to_vec();
        self.imp().drilldown_stack.borrow_mut().clear();
        self.imp().drilldown_header_box.set_visible(false);
        self._load_roots(roots, false);
    }

    fn _load_roots(&self, roots: &[(PathBuf, bool)], auto_expand: bool) {
        self.save_expanded_paths();
        tree_loading::clear_all_dir_state(self);
        self.reset_item_cache();
        let root_store = gio::ListStore::new::<FileTreeItem>();
        for (root, is_dir) in roots {
            let is_empty = if *is_dir {
                Some(crate::services::file_tree::is_dir_empty(root))
            } else {
                None
            };
            let item = FileTreeItem::new(root.clone(), *is_dir, is_empty);
            let index = root_store.n_items() as usize;
            root_store.append(&item);
            self.cache_root_item(root.clone(), index);
        }

        // GTK4 has no dedicated tree widget. Three pieces compose:
        // - TreeListModel: flattens hierarchical data into a list,
        //   tracking which nodes are expanded/collapsed
        // - ListView: renders the flat list with efficient item recycling
        // - TreeExpander: adds indentation and expand/collapse arrows
        //
        // autoexpand=false prevents unbounded recursive expansion.
        // passthrough=false wraps items in TreeListRow.
        let section_weak = self.downgrade();
        let tree_model = gtk4::TreeListModel::new(root_store.clone(), false, false, move |item| {
            let section = section_weak.upgrade()?;
            let fi = item.downcast_ref::<FileTreeItem>()?;
            if !fi.is_dir() {
                return None;
            }
            if fi.is_empty() == Some(true) {
                return None; // Folders known to be empty don't get child models, hiding the arrow natively.
            }
            fi.path().map(|p| {
                tree_loading::build_children_model(&section, &p).upcast::<gio::ListModel>()
            })
        });

        let selection = gtk4::SingleSelection::new(Some(tree_model.clone()));
        let imp = self.imp();
        imp.file_tree_view.set_model(Some(&selection));
        *imp.root_store.borrow_mut() = Some(root_store);
        *imp.tree_model.borrow_mut() = Some(tree_model.clone());
        self.update_button_state();

        if auto_expand {
            // Expand roots to save user from extra clicks, specially nice on drill-downs
            for i in 0..tree_model.n_items() {
                if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>() {
                    row.set_expanded(true);
                }
            }
        }
    }

    /// Add a single root path to an existing file tree.
    /// `is_dir` avoids a `stat(2)` call — callers already know the entry type.
    pub fn add_root(&self, path: &Path, is_dir: bool) {
        let has_store = !self.imp().original_roots.borrow().is_empty();
        if has_store {
            let already_exists = self
                .imp()
                .original_roots
                .borrow()
                .iter()
                .any(|(p, _)| p == path);
            if !already_exists {
                self.imp()
                    .original_roots
                    .borrow_mut()
                    .push((path.to_path_buf(), is_dir));
                // Only update the tree model if we are NOT drilled down
                if self.imp().drilldown_stack.borrow().is_empty() {
                    let store_ref = self.imp().root_store.borrow();
                    if let Some(root_store) = store_ref.as_ref() {
                        let is_empty = if is_dir {
                            Some(crate::services::file_tree::is_dir_empty(path))
                        } else {
                            None
                        };
                        let item = FileTreeItem::new(path.to_path_buf(), is_dir, is_empty);
                        let index = root_store.n_items() as usize;
                        root_store.append(&item);
                        self.cache_root_item(path.to_path_buf(), index);
                    }
                }
            }
        } else {
            self.load_roots(&[(path.to_path_buf(), is_dir)]);
        }
        self.update_button_state();
    }

    /// Returns true if this section has at least one root loaded.
    #[must_use]
    pub fn has_roots(&self) -> bool {
        !self.imp().original_roots.borrow().is_empty()
    }

    /// Focuses the workspace panel on a specific deep directory, allowing users
    /// to navigate past the horizontal clipping limit.
    pub fn focus_folder(&self, dir_path: &Path) {
        self.imp()
            .drilldown_stack
            .borrow_mut()
            .push(dir_path.to_path_buf());
        self.imp().drilldown_header_box.set_visible(true);

        let path_str = dir_path.to_string_lossy();
        self.imp().drilldown_path_label.set_label(&path_str);
        self.imp()
            .drilldown_path_label
            .set_tooltip_text(Some(&path_str));

        self._load_roots(&[(dir_path.to_path_buf(), true)], true);
        self.notify_folder_focused();
    }

    /// Navigates one level up the drill-down stack. Restores original roots if empty.
    pub fn navigate_back(&self) {
        let mut stack = self.imp().drilldown_stack.borrow_mut();
        let popped_path = stack.pop();
        if let Some(parent_path) = stack.last().cloned() {
            let path_str = parent_path.to_string_lossy();
            self.imp().drilldown_path_label.set_label(&path_str);
            self.imp()
                .drilldown_path_label
                .set_tooltip_text(Some(&path_str));
            if let Some(target) = popped_path {
                *self.imp().pending_selection.borrow_mut() = Some(target);
            }
            drop(stack);
            self._load_roots(&[(parent_path, true)], true);
        } else {
            if let Some(target) = popped_path {
                *self.imp().pending_selection.borrow_mut() = Some(target);
            }
            drop(stack);
            self.imp().drilldown_header_box.set_visible(false);
            let original = self.imp().original_roots.borrow().clone();
            self._load_roots(&original, true);
        }
    }

    /// Update the add-folder button icon and tooltip based on whether roots exist.
    /// Empty workspace: "Add Folder to Workspace" (folder-new-symbolic)
    /// Workspace with roots: "Replace Workspace Root" (folder-open-symbolic)
    fn update_button_state(&self) {
        let button = &self.imp().add_folder_button;
        if self.has_roots() {
            button.set_icon_name("folder-open-symbolic");
            button.set_tooltip_text(Some("Replace Workspace Root"));
        } else {
            button.set_icon_name("folder-new-symbolic");
            button.set_tooltip_text(Some("Add Folder to Workspace"));
        }
    }

    // --- Callback registration ---

    pub fn connect_file_activated<F: Fn(&Path) + 'static>(&self, f: F) {
        self.imp()
            .file_tree_view
            .connect_activate(move |list_view, position| {
                activate_file_at(list_view, position, &f);
            });
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

    pub fn connect_add_folder_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().add_folder_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_rename_workspace_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().rename_workspace_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_unlist_workspace_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().unlist_workspace_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_folder_focused<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().folder_focused_callback.borrow_mut() = Some(Box::new(f));
    }

    // --- Callback notification helpers (called from imp.rs closures) ---

    pub fn notify_add_folder_requested(&self) {
        let ws_id = self.workspace_id();
        if let Some(ref cb) = *self.imp().add_folder_callback.borrow() {
            cb(&ws_id);
        }
    }

    pub fn notify_rename_workspace_requested(&self) {
        let ws_id = self.workspace_id();
        if let Some(ref cb) = *self.imp().rename_workspace_callback.borrow() {
            cb(&ws_id);
        }
    }

    pub fn notify_unlist_workspace_requested(&self) {
        let ws_id = self.workspace_id();
        if let Some(ref cb) = *self.imp().unlist_workspace_callback.borrow() {
            cb(&ws_id);
        }
    }

    pub fn notify_folder_focused(&self) {
        let ws_id = self.workspace_id();
        if let Some(ref cb) = *self.imp().folder_focused_callback.borrow() {
            cb(&ws_id);
        }
    }

    /// Collapses the root directories of this workspace section.
    pub fn collapse_roots(&self) {
        if let Some(tree_model) = self.imp().tree_model.borrow().as_ref() {
            let mut roots = Vec::new();
            for i in 0..tree_model.n_items() {
                if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                    && row.depth() == 0
                    && row.is_expanded()
                {
                    roots.push(row);
                }
            }
            for row in roots {
                row.set_expanded(false);
            }
        }
    }

    /// Expands the root directories of this workspace section if they are not confirmed empty.
    pub fn expand_roots(&self) {
        if let Some(tree_model) = self.imp().tree_model.borrow().as_ref() {
            let mut roots = Vec::new();
            for i in 0..tree_model.n_items() {
                if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                    && row.depth() == 0
                    && !row.is_expanded()
                    && let Some(item) = row.item().and_downcast::<FileTreeItem>()
                    && item.is_empty() != Some(true)
                {
                    roots.push(row);
                }
            }
            for row in roots {
                row.set_expanded(true);
            }
        }
    }

    /// Toggles the expansion state of the root directories. If any root is collapsed, expands all.
    /// Otherwise, collapses all.
    pub fn toggle_roots(&self) {
        if let Some(tree_model) = self.imp().tree_model.borrow().as_ref() {
            let mut roots = Vec::new();
            let mut any_collapsed = false;

            for i in 0..tree_model.n_items() {
                if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                    && row.depth() == 0
                    && let Some(item) = row.item().and_downcast::<FileTreeItem>()
                    && item.is_dir()
                    && !item.is_placeholder()
                    && item.is_empty() != Some(true)
                {
                    roots.push(row.clone());
                    if !row.is_expanded() {
                        any_collapsed = true;
                    }
                }
            }

            for row in roots {
                row.set_expanded(any_collapsed);
            }
        }
    }

    /// Select and scroll to a path after its row exists in the flattened tree model.
    ///
    /// Tree expansion is asynchronous, so pending selections are fulfilled by the
    /// tree-loading helper after child batches land in the `TreeListModel`.
    pub(super) fn select_and_scroll_to(&self, target_path: &Path) {
        if let Some(tree_model) = self.imp().tree_model.borrow().as_ref() {
            for i in 0..tree_model.n_items() {
                if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                    && let Some(item) = row.item().and_downcast::<FileTreeItem>()
                    && item.path().as_deref() == Some(target_path)
                {
                    if let Some(selection) = self
                        .imp()
                        .file_tree_view
                        .model()
                        .and_downcast::<gtk4::SingleSelection>()
                    {
                        selection.set_selected(i);
                        self.imp()
                            .file_tree_view
                            .scroll_to(i, gtk4::ListScrollFlags::FOCUS, None);
                    }
                    self.imp().pending_selection.borrow_mut().take();
                    return;
                }
            }
        }
    }

}

/// Extract the file item at the given position and call the callback if it's a file.
fn activate_file_at(list_view: &gtk4::ListView, position: u32, callback: &dyn Fn(&Path)) {
    let Some(model) = list_view.model() else {
        return;
    };
    if let Some(item) = model.item(position)
        && let Some(tree_row) = item.downcast_ref::<gtk4::TreeListRow>()
        && let Some(file_item) = tree_row
            .item()
            .and_then(|i| i.downcast::<FileTreeItem>().ok())
    {
        if file_item.is_dir() && !file_item.is_placeholder() && file_item.is_empty() != Some(true) {
            tree_row.set_expanded(!tree_row.is_expanded());
        } else if !file_item.is_dir()
            && let Some(ref path) = file_item.path()
        {
            callback(path);
        }
    }
}

impl Default for LushtextWorkspaceSection {
    fn default() -> Self {
        Self::new(WorkspaceId::default())
    }
}

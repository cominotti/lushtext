// SPDX-License-Identifier: GPL-3.0-or-later

//! Root-tree loading, drill-down, and root expansion helpers for one workspace section.
//!
//! This slice keeps the tree-model and drill-down orchestration together so the
//! public facade can stay focused on the widget API and callback surface.

use std::path::Path;

use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{gio, glib};

use crate::model::workspace::WorkspaceEntry;
use crate::ui::sidebar::file_tree_item::FileTreeItem;

use super::LushtextWorkspaceSection;

impl LushtextWorkspaceSection {
    /// Load root paths into the file tree.
    pub fn load_roots(&self, roots: &[WorkspaceEntry]) {
        *self.imp().original_roots.borrow_mut() = roots.to_vec();
        self.imp().drilldown_stack.borrow_mut().clear();
        self.imp().drilldown_header_box.set_visible(false);
        self.load_root_model(roots, false);
    }

    pub(super) fn load_root_model(&self, roots: &[WorkspaceEntry], auto_expand: bool) {
        self.dismiss_peek_for_rebuild();
        self.save_expanded_paths();
        super::tree_loading::clear_all_dir_state(self);
        self.reset_item_cache();
        let root_store = gio::ListStore::new::<FileTreeItem>();
        for entry in roots {
            let root_path = entry.path().to_path_buf();
            let is_dir = entry.is_dir();
            let is_empty = if is_dir {
                Some(crate::services::file_tree::is_dir_empty(&root_path))
            } else {
                None
            };
            let item = FileTreeItem::new(root_path.clone(), is_dir, is_empty);
            let index = root_store.n_items() as usize;
            root_store.append(&item);
            self.cache_root_item(root_path, index);
        }

        let section_weak = self.downgrade();
        let tree_model = gtk4::TreeListModel::new(root_store.clone(), false, false, move |item| {
            let section = section_weak.upgrade()?;
            let file_item = item.downcast_ref::<FileTreeItem>()?;
            if !file_item.is_dir() || file_item.is_empty() == Some(true) {
                return None;
            }
            file_item.path().map(|path| {
                super::tree_loading::build_children_model(&section, &path)
                    .upcast::<gio::ListModel>()
            })
        });

        let selection = gtk4::SingleSelection::new(Some(tree_model.clone()));
        self.install_peek_selection_model(&selection);
        let imp = self.imp();
        imp.file_tree_view.set_model(Some(&selection));
        *imp.root_store.borrow_mut() = Some(root_store);
        *imp.tree_model.borrow_mut() = Some(tree_model.clone());
        self.update_button_state();
        self.restore_root_model_state(auto_expand);
        self.restart_workspace_watch();
    }

    /// Add a single root path to an existing file tree.
    pub fn add_root(&self, path: &Path, is_dir: bool) {
        let new_entry = if is_dir {
            WorkspaceEntry::Directory {
                path: path.to_path_buf(),
            }
        } else {
            WorkspaceEntry::File {
                path: path.to_path_buf(),
            }
        };
        let has_store = !self.imp().original_roots.borrow().is_empty();
        if has_store {
            let already_exists = self
                .imp()
                .original_roots
                .borrow()
                .iter()
                .any(|entry| entry.path() == path);
            if !already_exists {
                self.imp()
                    .original_roots
                    .borrow_mut()
                    .push(new_entry.clone());
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
            self.load_roots(&[new_entry]);
        }
        self.update_button_state();
    }

    /// Returns true if this section has at least one root loaded.
    #[must_use]
    pub fn has_roots(&self) -> bool {
        !self.imp().original_roots.borrow().is_empty()
    }

    /// Focus the workspace panel on a specific deep directory.
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

        self.load_root_model(
            &[WorkspaceEntry::Directory {
                path: dir_path.to_path_buf(),
            }],
            true,
        );
        self.notify_folder_focused();
    }

    /// Navigate one level up the drill-down stack.
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
            self.load_root_model(&[WorkspaceEntry::Directory { path: parent_path }], true);
        } else {
            if let Some(target) = popped_path {
                *self.imp().pending_selection.borrow_mut() = Some(target);
            }
            drop(stack);
            self.imp().drilldown_header_box.set_visible(false);
            let original = self.imp().original_roots.borrow().clone();
            self.load_root_model(&original, true);
        }
    }

    /// Collapses the root directories of this workspace section.
    pub fn collapse_roots(&self) {
        for row in self.expanded_root_rows() {
            row.set_expanded(false);
        }
        self.restart_workspace_watch();
    }

    /// Expands the root directories of this workspace section if they are not confirmed empty.
    pub fn expand_roots(&self) {
        for row in self.collapsed_non_empty_root_rows() {
            row.set_expanded(true);
        }
        self.restart_workspace_watch();
    }

    /// Toggle the expansion state of the root directories as one group.
    pub fn toggle_roots(&self) {
        let rows = self.toggleable_root_rows();
        let any_collapsed = rows.iter().any(|row| !row.is_expanded());
        for row in rows {
            row.set_expanded(any_collapsed);
        }
        self.restart_workspace_watch();
    }

    /// Select and scroll to a path after its row exists in the flattened tree model.
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

    /// Update the add-folder button icon and tooltip based on whether roots exist.
    fn update_button_state(&self) {
        let button = &self.imp().add_folder_button;
        self.imp().refresh_button.set_sensitive(self.has_roots());
        if self.has_roots() {
            button.set_icon_name("folder-open-symbolic");
            button.set_tooltip_text(Some("Replace Workspace Root"));
        } else {
            button.set_icon_name("folder-new-symbolic");
            button.set_tooltip_text(Some("Add Folder to Workspace"));
        }
    }

    fn root_rows(&self) -> Vec<gtk4::TreeListRow> {
        let Some(tree_model) = self.imp().tree_model.borrow().as_ref().cloned() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for i in 0..tree_model.n_items() {
            if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                && row.depth() == 0
            {
                rows.push(row);
            }
        }
        rows
    }

    fn expanded_root_rows(&self) -> Vec<gtk4::TreeListRow> {
        self.root_rows()
            .into_iter()
            .filter(gtk4::TreeListRow::is_expanded)
            .collect()
    }

    fn collapsed_non_empty_root_rows(&self) -> Vec<gtk4::TreeListRow> {
        self.root_rows()
            .into_iter()
            .filter(|row| !row.is_expanded())
            .filter(|row| {
                row.item()
                    .and_downcast::<FileTreeItem>()
                    .is_some_and(|item| item.is_empty() != Some(true))
            })
            .collect()
    }

    fn toggleable_root_rows(&self) -> Vec<gtk4::TreeListRow> {
        self.root_rows()
            .into_iter()
            .filter(|row| {
                row.item()
                    .and_downcast::<FileTreeItem>()
                    .is_some_and(|item| {
                        item.is_dir() && !item.is_placeholder() && item.is_empty() != Some(true)
                    })
            })
            .collect()
    }

    fn restore_root_model_state(&self, auto_expand: bool) {
        let section_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(section) = section_weak.upgrade() else {
                return;
            };

            for row in section.root_rows() {
                let should_expand = auto_expand
                    || row
                        .item()
                        .and_downcast::<FileTreeItem>()
                        .and_then(|item| item.path())
                        .is_some_and(|path| section.imp().expanded_paths.borrow().contains(&path));
                if should_expand {
                    row.set_expanded(true);
                }
            }

            let pending_selection = section.imp().pending_selection.borrow().clone();
            if let Some(target_path) = pending_selection {
                section.select_and_scroll_to(&target_path);
            }
        });
    }
}

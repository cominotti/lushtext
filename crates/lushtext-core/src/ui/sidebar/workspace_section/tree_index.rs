// SPDX-License-Identifier: GPL-3.0-or-later

//! Tree index and cache helpers for `LushtextWorkspaceSection`.
//!
//! This file keeps path/index bookkeeping separate from the widget-facing tree
//! behavior so rename/delete/drill-down flows do not need to interleave with
//! cache maintenance details.

use std::path::{Path, PathBuf};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};

use super::super::file_tree_item::FileTreeItem;
use super::imp::ItemLocation;
use super::{tree_loading, LushtextWorkspaceSection};

impl LushtextWorkspaceSection {
    pub(super) fn save_expanded_paths(&self) {
        if let Some(tree_model) = self.imp().tree_model.borrow().as_ref() {
            let mut expanded = self.imp().expanded_paths.borrow_mut();
            for i in 0..tree_model.n_items() {
                if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                    && row.is_expanded()
                    && let Some(item) = row.item().and_downcast::<FileTreeItem>()
                    && let Some(path) = item.path()
                {
                    expanded.insert(path);
                }
            }
        }
    }

    pub(super) fn reset_item_cache(&self) {
        self.imp().root_paths.borrow_mut().clear();
        self.imp().child_paths.borrow_mut().clear();
        self.imp().item_locations.borrow_mut().clear();
    }

    pub(super) fn cache_root_item(&self, path: PathBuf, index: usize) {
        let imp = self.imp();
        let mut root_paths = imp.root_paths.borrow_mut();
        let insert_at = index.min(root_paths.len());
        root_paths.insert(insert_at, path.clone());
        drop(root_paths);
        self.shift_cached_indices(None, insert_at, 1);
        imp.item_locations.borrow_mut().insert(
            path,
            ItemLocation {
                parent_dir: None,
                index: insert_at,
            },
        );
    }

    /// Cache a child row's parent directory and index for O(1) later lookup.
    pub(super) fn cache_child_item(&self, parent_dir: &Path, path: PathBuf, index: usize) {
        let imp = self.imp();
        let parent_key = parent_dir.to_path_buf();
        let mut child_paths = imp.child_paths.borrow_mut();
        let siblings = child_paths.entry(parent_key.clone()).or_default();
        let insert_at = index.min(siblings.len());
        siblings.insert(insert_at, path.clone());
        drop(child_paths);
        self.shift_cached_indices(Some(parent_dir), insert_at, 1);
        imp.item_locations.borrow_mut().insert(
            path,
            ItemLocation {
                parent_dir: Some(parent_key),
                index: insert_at,
            },
        );
    }

    pub(super) fn rename_cached_item(&self, old_path: &Path, new_path: &Path) {
        let Some(location) = self.imp().item_locations.borrow_mut().remove(old_path) else {
            return;
        };

        match location.parent_dir.as_deref() {
            None => {
                if let Some(cached) = self.imp().root_paths.borrow_mut().get_mut(location.index) {
                    *cached = new_path.to_path_buf();
                }
            }
            Some(parent_dir) => {
                if let Some(siblings) = self.imp().child_paths.borrow_mut().get_mut(parent_dir)
                    && let Some(cached) = siblings.get_mut(location.index)
                {
                    *cached = new_path.to_path_buf();
                }
            }
        }

        self.imp()
            .item_locations
            .borrow_mut()
            .insert(new_path.to_path_buf(), location);
    }

    pub(super) fn append_item_preserving_placeholder(
        &self,
        store: &gio::ListStore,
        parent_dir: &Path,
        item: &FileTreeItem,
    ) {
        let insert_pos = store
            .n_items()
            .checked_sub(1)
            .and_then(|idx| store.item(idx))
            .and_then(|obj| obj.downcast::<FileTreeItem>().ok())
            .filter(super::super::file_tree_item::FileTreeItem::is_placeholder)
            .map_or_else(|| store.n_items(), |_| store.n_items() - 1);

        if insert_pos == store.n_items() {
            store.append(item);
        } else {
            store.insert(insert_pos, item);
        }

        if let Some(path) = item.path() {
            self.cache_child_item(parent_dir, path, insert_pos as usize);
        }
    }

    /// Resolve the live `TreeListRow` for a directory path, reusing the weak cache when possible.
    pub(super) fn find_dir_row(&self, dir_path: &Path) -> Option<gtk4::TreeListRow> {
        if let Some(row) = self
            .imp()
            .dir_rows
            .borrow()
            .get(dir_path)
            .cloned()
            .and_then(|weak| weak.upgrade())
        {
            let matches = row
                .item()
                .and_downcast::<FileTreeItem>()
                .is_some_and(|item| item.is_dir() && item.path().as_deref() == Some(dir_path));
            if matches {
                return Some(row);
            }
            self.imp().dir_rows.borrow_mut().remove(dir_path);
        }

        let tree_model = self.imp().tree_model.borrow();
        let tree_model = tree_model.as_ref()?;
        for i in 0..tree_model.n_items() {
            let row = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()?;
            let item = row.item().and_downcast::<FileTreeItem>()?;
            if item.is_dir() && item.path().as_deref() == Some(dir_path) {
                self.imp()
                    .dir_rows
                    .borrow_mut()
                    .insert(dir_path.to_path_buf(), row.downgrade());
                return Some(row);
            }
        }
        None
    }

    pub(super) fn find_store_for_dir(&self, dir_path: &Path) -> Option<gio::ListStore> {
        if let Some(store) = self
            .imp()
            .dir_stores
            .borrow()
            .get(dir_path)
            .and_then(glib::WeakRef::upgrade)
        {
            return Some(store);
        }

        let store = self
            .find_dir_row(dir_path)?
            .children()
            .and_then(|m| m.downcast::<gio::ListStore>().ok())?;
        self.imp()
            .dir_stores
            .borrow_mut()
            .insert(dir_path.to_path_buf(), store.downgrade());
        Some(store)
    }

    /// Remove an item from the tree model by path. Returns true if found and removed.
    pub fn remove_from_model(&self, target_path: &Path) -> bool {
        let imp = self.imp();
        tree_loading::clear_dir_state(self, target_path);

        if let Some(location) = self.remove_cached_item(target_path) {
            #[expect(clippy::cast_possible_truncation)] // list store ≪ u32::MAX
            let idx = location.index as u32;
            match location.parent_dir.as_deref() {
                None => {
                    if let Some(ref root_store) = *imp.root_store.borrow()
                        && idx < root_store.n_items()
                    {
                        root_store.remove(idx);
                        return true;
                    }
                }
                Some(parent_dir) => {
                    if let Some(store) = self.find_store_for_dir(parent_dir)
                        && idx < store.n_items()
                    {
                        store.remove(idx);
                        return true;
                    }
                }
            }
        }

        if let Some(ref root_store) = *imp.root_store.borrow() {
            for i in 0..root_store.n_items() {
                if let Some(item) = root_store.item(i).and_downcast::<FileTreeItem>()
                    && item.path().as_deref() == Some(target_path)
                {
                    root_store.remove(i);
                    return true;
                }
            }
        }

        let Some(parent_dir) = target_path.parent() else {
            return false;
        };

        if let Some(store) = self.find_store_for_dir(parent_dir) {
            for j in 0..store.n_items() {
                if let Some(child) = store.item(j).and_downcast::<FileTreeItem>()
                    && child.path().as_deref() == Some(target_path)
                {
                    store.remove(j);
                    return true;
                }
            }
        } else {
            tracing::warn!(
                "remove_from_model: missing store for {}",
                parent_dir.display()
            );
        }
        false
    }

    fn shift_cached_indices(&self, parent_dir: Option<&Path>, start: usize, delta: isize) {
        let parent_key = parent_dir.map(Path::to_path_buf);
        for location in self.imp().item_locations.borrow_mut().values_mut() {
            if location.parent_dir == parent_key && location.index >= start {
                location.index = location.index.saturating_add_signed(delta);
            }
        }
    }

    fn remove_cached_item(&self, target_path: &Path) -> Option<ItemLocation> {
        let removed = self.imp().item_locations.borrow_mut().remove(target_path)?;
        match removed.parent_dir.as_deref() {
            None => {
                let mut root_paths = self.imp().root_paths.borrow_mut();
                let removed_index = if root_paths
                    .get(removed.index)
                    .is_some_and(|path| path == target_path)
                {
                    root_paths.remove(removed.index);
                    removed.index
                } else if let Some(position) = root_paths.iter().position(|path| path == target_path)
                {
                    root_paths.remove(position);
                    position
                } else {
                    removed.index
                };
                drop(root_paths);
                self.shift_cached_indices(None, removed_index.saturating_add(1), -1);
            }
            Some(parent_dir) => {
                let mut child_paths = self.imp().child_paths.borrow_mut();
                let mut remove_parent = false;
                if let Some(siblings) = child_paths.get_mut(parent_dir) {
                    let removed_index = if siblings
                        .get(removed.index)
                        .is_some_and(|path| path == target_path)
                    {
                        siblings.remove(removed.index);
                        removed.index
                    } else if let Some(position) =
                        siblings.iter().position(|path| path == target_path)
                    {
                        siblings.remove(position);
                        position
                    } else {
                        removed.index
                    };
                    self.shift_cached_indices(
                        Some(parent_dir),
                        removed_index.saturating_add(1),
                        -1,
                    );
                    if siblings.is_empty() {
                        remove_parent = true;
                    }
                }
                if remove_parent {
                    child_paths.remove(parent_dir);
                }
                drop(child_paths);
            }
        }
        Some(removed)
    }
}

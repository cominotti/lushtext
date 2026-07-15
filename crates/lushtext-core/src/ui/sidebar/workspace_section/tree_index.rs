// SPDX-License-Identifier: GPL-3.0-or-later

//! Tree index and cache helpers for `LushtextWorkspaceSection`.
//!
//! This file keeps path/index bookkeeping separate from the widget-facing tree
//! behavior so rename/delete/drill-down flows do not need to interleave with
//! cache maintenance details.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};

use super::super::file_tree_item::FileTreeItem;
use super::imp::ItemLocation;
use super::{LushtextWorkspaceSection, tree_loading};
use crate::services::file_tree::DirectoryRowState;

impl LushtextWorkspaceSection {
    pub(super) fn save_expanded_paths(&self) {
        if let Some(tree_model) = self.imp().tree_model.borrow().as_ref() {
            let mut expanded = self.imp().expanded_paths.borrow_mut();
            // Snapshot the current expanded state rather than accumulating a
            // historical union. Otherwise a refresh can re-expand rows the user
            // has since collapsed.
            expanded.clear();
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
        self.imp().folder_paths.borrow_mut().clear();
        self.imp().child_paths.borrow_mut().clear();
        self.imp().visible_path_counts.borrow_mut().clear();
        self.imp().item_locations.borrow_mut().clear();
    }

    pub(super) fn cache_top_level_item(&self, path: PathBuf, index: usize) {
        let imp = self.imp();
        let mut folder_paths = imp.folder_paths.borrow_mut();
        let insert_at = index.min(folder_paths.len());
        folder_paths.insert(insert_at, path.clone());
        drop(folder_paths);
        self.shift_cached_indices(None, insert_at, 1);
        self.cache_item_location(
            path,
            ItemLocation {
                parent_dir: None,
                index: insert_at,
            },
        );
    }

    /// Rebuild the item cache from the current top-level folder `ListStore` contents.
    pub(super) fn recache_top_level_store(&self, store: &gio::ListStore) {
        let old_paths = self.imp().folder_paths.borrow().clone();
        self.imp().folder_paths.borrow_mut().clear();
        self.imp()
            .item_locations
            .borrow_mut()
            .retain(|_, location| location.parent_dir.is_some());
        for path in old_paths {
            self.forget_visible_path_occurrence(&path);
        }

        for index in 0..store.n_items() {
            if let Some(item) = store.item(index).and_downcast::<FileTreeItem>()
                && let Some(path) = item.path()
            {
                self.cache_top_level_item(path, index as usize);
            }
        }
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
        self.cache_item_location(
            path,
            ItemLocation {
                parent_dir: Some(parent_key),
                index: insert_at,
            },
        );
    }

    /// Rebuild the direct-child cache for one directory from the current
    /// `ListStore` contents after a refresh splice.
    pub(super) fn recache_child_store(&self, parent_dir: &Path, store: &gio::ListStore) {
        let rows = (0..store.n_items())
            .filter_map(|index| store.item(index).and_downcast::<FileTreeItem>())
            .map(|item| DirectoryRowState {
                path: item.path(),
                is_dir: item.is_dir(),
                is_empty: item.is_empty(),
                is_placeholder: item.is_placeholder(),
            })
            .collect::<Vec<_>>();
        self.recache_child_rows_from_mirror(parent_dir, &rows);
    }

    /// Rebuild direct-child path caches from the accepted plain store mirror.
    pub(super) fn recache_child_rows_from_mirror(
        &self,
        parent_dir: &Path,
        rows: &[DirectoryRowState],
    ) {
        let new_occurrences = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.path.clone().map(|path| (index, path)))
            .collect::<Vec<_>>();
        let imp = self.imp();
        let metrics = replace_child_cache(
            &imp.folder_paths.borrow(),
            &mut imp.child_paths.borrow_mut(),
            &mut imp.visible_path_counts.borrow_mut(),
            &mut imp.item_locations.borrow_mut(),
            parent_dir,
            &new_occurrences,
        );
        imp.refresh_runtime
            .cache_rebuild_input_rows
            .set(metrics.input_rows);
        imp.refresh_runtime
            .cache_rebuild_operations
            .set(metrics.operations);
    }

    pub(super) fn rename_cached_item(&self, old_path: &Path, new_path: &Path) {
        let Some(location) = self.imp().item_locations.borrow_mut().remove(old_path) else {
            return;
        };

        match location.parent_dir.as_deref() {
            None => {
                if let Some(cached) = self.imp().folder_paths.borrow_mut().get_mut(location.index) {
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

        if let Some(parent_dir) = location.parent_dir.as_deref()
            && let Some(store) = self.find_store_for_dir(parent_dir)
        {
            tree_loading::record_child_store_path_update(self, &store, location.index, new_path);
        }
        self.forget_visible_path_occurrence(old_path);
        self.cache_item_location(new_path.to_path_buf(), location);
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
        tree_loading::record_child_store_insert(
            self,
            store,
            insert_pos as usize,
            std::slice::from_ref(item),
        );

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
    #[must_use]
    pub fn remove_from_model(&self, target_path: &Path) -> bool {
        let imp = self.imp();
        tree_loading::clear_dir_state(self, target_path);
        let mut removed_any = false;
        let may_have_multiple_visible_rows = self.visible_path_is_ambiguous(target_path);

        if let Some(location) = self.remove_cached_item(target_path) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "The sidebar child store is bounded to realistic directory sizes before converting to u32"
            )]
            let idx = location.index as u32;
            match location.parent_dir.as_deref() {
                None if let Some(ref top_level_store) = *imp.top_level_store.borrow()
                    && idx < top_level_store.n_items() =>
                {
                    top_level_store.remove(idx);
                    removed_any = true;
                }
                Some(parent_dir)
                    if let Some(store) = self.find_store_for_dir(parent_dir)
                        && idx < store.n_items() =>
                {
                    store.remove(idx);
                    tree_loading::record_child_store_remove(self, &store, idx as usize, 1);
                    removed_any = true;
                }
                _ => {}
            }
        }

        if may_have_multiple_visible_rows || !removed_any {
            if let Some(ref top_level_store) = *imp.top_level_store.borrow()
                && remove_matching_items(self, top_level_store, target_path, false)
            {
                self.recache_top_level_store(top_level_store);
                removed_any = true;
            }

            let child_stores = imp
                .dir_stores
                .borrow()
                .iter()
                .filter_map(|(parent_dir, weak_store)| {
                    weak_store
                        .upgrade()
                        .map(|store| (parent_dir.clone(), store))
                })
                .collect::<Vec<_>>();

            for (parent_dir, store) in child_stores {
                if remove_matching_items(self, &store, target_path, true) {
                    self.recache_child_store(&parent_dir, &store);
                    removed_any = true;
                }
            }

            if !removed_any {
                for (parent_dir, store) in self.visible_child_stores() {
                    if remove_matching_items(self, &store, target_path, true) {
                        self.recache_child_store(&parent_dir, &store);
                        removed_any = true;
                    }
                }
            }
        }

        removed_any
    }

    fn visible_child_stores(&self) -> Vec<(PathBuf, gio::ListStore)> {
        let Some(tree_model) = self.imp().tree_model.borrow().as_ref().cloned() else {
            return Vec::new();
        };
        let mut stores = Vec::new();
        for index in 0..tree_model.n_items() {
            let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>() else {
                continue;
            };
            let Some(item) = row.item().and_downcast::<FileTreeItem>() else {
                continue;
            };
            let Some(parent_dir) = item.path() else {
                continue;
            };
            let Some(store) = row
                .children()
                .and_then(|model| model.downcast::<gio::ListStore>().ok())
            else {
                continue;
            };
            stores.push((parent_dir, store));
        }
        stores
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
                let mut folder_paths = self.imp().folder_paths.borrow_mut();
                let removed_index = if folder_paths
                    .get(removed.index)
                    .is_some_and(|path| path == target_path)
                {
                    folder_paths.remove(removed.index);
                    removed.index
                } else if let Some(position) =
                    folder_paths.iter().position(|path| path == target_path)
                {
                    folder_paths.remove(position);
                    position
                } else {
                    removed.index
                };
                drop(folder_paths);
                self.shift_cached_indices(None, removed_index.saturating_add(1), -1);
                self.forget_visible_path_occurrence(target_path);
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
                self.forget_visible_path_occurrence(target_path);
            }
        }
        Some(removed)
    }

    fn cache_item_location(&self, path: PathBuf, location: ItemLocation) {
        let count = {
            let mut counts = self.imp().visible_path_counts.borrow_mut();
            let count = counts.entry(path.clone()).or_insert(0);
            *count += 1;
            *count
        };

        let mut locations = self.imp().item_locations.borrow_mut();
        if count == 1 {
            locations.insert(path, location);
        } else {
            locations.remove(&path);
        }
    }

    pub(super) fn forget_visible_path_occurrence(&self, path: &Path) {
        let remaining = {
            let mut counts = self.imp().visible_path_counts.borrow_mut();
            let Some(count) = counts.get_mut(path) else {
                return;
            };
            if *count <= 1 {
                counts.remove(path);
                0
            } else {
                *count -= 1;
                *count
            }
        };

        self.imp().item_locations.borrow_mut().remove(path);
        if remaining == 1 {
            self.restore_unique_item_location(path);
        }
    }

    fn restore_unique_item_location(&self, path: &Path) {
        if let Some(index) = self
            .imp()
            .folder_paths
            .borrow()
            .iter()
            .position(|folder| folder.as_path() == path)
        {
            self.imp().item_locations.borrow_mut().insert(
                path.to_path_buf(),
                ItemLocation {
                    parent_dir: None,
                    index,
                },
            );
            return;
        }

        for (parent_dir, siblings) in self.imp().child_paths.borrow().iter() {
            if let Some(index) = siblings.iter().position(|child| child.as_path() == path) {
                self.imp().item_locations.borrow_mut().insert(
                    path.to_path_buf(),
                    ItemLocation {
                        parent_dir: Some(parent_dir.clone()),
                        index,
                    },
                );
                return;
            }
        }
    }

    fn visible_path_is_ambiguous(&self, path: &Path) -> bool {
        self.imp()
            .visible_path_counts
            .borrow()
            .get(path)
            .copied()
            .unwrap_or(0)
            > 1
    }
}

/// Atomically replace one accepted child mirror's cache projection.
///
/// Old and new sibling rows are each visited a bounded number of times. A
/// duplicate path that becomes globally unique may require one linear pass over
/// the other already-materialized rows to recover its sole location; crucially,
/// that recovery is shared rather than repeated once per inserted row.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ChildCacheRebuildMetrics {
    input_rows: usize,
    operations: usize,
}

fn replace_child_cache(
    folder_paths: &[PathBuf],
    child_paths: &mut HashMap<PathBuf, Vec<PathBuf>>,
    visible_path_counts: &mut HashMap<PathBuf, usize>,
    item_locations: &mut HashMap<PathBuf, ItemLocation>,
    parent_dir: &Path,
    new_occurrences: &[(usize, PathBuf)],
) -> ChildCacheRebuildMetrics {
    let old_paths = child_paths.remove(parent_dir).unwrap_or_default();
    let input_rows = old_paths.len().saturating_add(new_occurrences.len());
    let mut operations = old_paths.len();
    let new_paths = new_occurrences
        .iter()
        .map(|(_, path)| path.clone())
        .collect::<Vec<_>>();
    operations = operations.saturating_add(new_occurrences.len());

    let mut old_counts = HashMap::<PathBuf, usize>::new();
    for path in old_paths {
        *old_counts.entry(path).or_default() += 1;
    }
    let mut new_counts = HashMap::<PathBuf, usize>::new();
    for (_, path) in new_occurrences {
        *new_counts.entry(path.clone()).or_default() += 1;
    }
    operations = operations.saturating_add(new_occurrences.len());
    let affected = old_counts
        .keys()
        .chain(new_counts.keys())
        .cloned()
        .collect::<HashSet<_>>();

    for path in &affected {
        let previous = visible_path_counts.get(path).copied().unwrap_or(0);
        let next = previous
            .saturating_sub(old_counts.get(path).copied().unwrap_or(0))
            .saturating_add(new_counts.get(path).copied().unwrap_or(0));
        if next == 0 {
            visible_path_counts.remove(path);
        } else {
            visible_path_counts.insert(path.clone(), next);
        }
    }
    operations = operations.saturating_add(affected.len());

    for path in &affected {
        item_locations.remove(path);
    }
    operations = operations.saturating_add(affected.len());
    if !new_paths.is_empty() {
        child_paths.insert(parent_dir.to_path_buf(), new_paths);
    }

    let mut unresolved_unique = affected
        .into_iter()
        .filter(|path| visible_path_counts.get(path) == Some(&1))
        .collect::<HashSet<_>>();
    operations = operations.saturating_add(old_counts.len().saturating_add(new_counts.len()));
    for (index, path) in new_occurrences {
        if unresolved_unique.remove(path) {
            item_locations.insert(
                path.clone(),
                ItemLocation {
                    parent_dir: Some(parent_dir.to_path_buf()),
                    index: *index,
                },
            );
        }
    }
    operations = operations.saturating_add(new_occurrences.len());

    if unresolved_unique.is_empty() {
        return ChildCacheRebuildMetrics {
            input_rows,
            operations,
        };
    }
    for (index, path) in folder_paths.iter().enumerate() {
        operations = operations.saturating_add(1);
        if unresolved_unique.remove(path) {
            item_locations.insert(
                path.clone(),
                ItemLocation {
                    parent_dir: None,
                    index,
                },
            );
        }
    }
    for (other_parent, siblings) in child_paths.iter() {
        if other_parent == parent_dir || unresolved_unique.is_empty() {
            continue;
        }
        for (index, path) in siblings.iter().enumerate() {
            operations = operations.saturating_add(1);
            if unresolved_unique.remove(path) {
                item_locations.insert(
                    path.clone(),
                    ItemLocation {
                        parent_dir: Some(other_parent.clone()),
                        index,
                    },
                );
            }
        }
    }
    debug_assert!(
        unresolved_unique.is_empty(),
        "visible occurrence counts must resolve every globally unique path"
    );
    ChildCacheRebuildMetrics {
        input_rows,
        operations,
    }
}

pub(super) fn child_cache_rebuild_operation_evidence(row_count: usize) -> (usize, usize) {
    let parent = PathBuf::from("/benchmark/parent");
    let old_paths = (0..row_count)
        .map(|index| PathBuf::from(format!("/benchmark/old-{index}")))
        .collect::<Vec<_>>();
    let new_occurrences = (0..row_count)
        .map(|index| (index, PathBuf::from(format!("/benchmark/new-{index}"))))
        .collect::<Vec<_>>();
    let mut child_paths = HashMap::from([(parent.clone(), old_paths.clone())]);
    let mut visible_path_counts = old_paths
        .iter()
        .map(|path| (path.clone(), 1usize))
        .collect::<HashMap<_, _>>();
    let mut item_locations = old_paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            (
                path,
                ItemLocation {
                    parent_dir: Some(parent.clone()),
                    index,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let metrics = replace_child_cache(
        &[],
        &mut child_paths,
        &mut visible_path_counts,
        &mut item_locations,
        &parent,
        &new_occurrences,
    );
    (metrics.input_rows, metrics.operations)
}

fn remove_matching_items(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    target_path: &Path,
    child_store: bool,
) -> bool {
    let mut removed_any = false;
    // Remove from the tail so each deletion cannot shift an unvisited row's index.
    for index in (0..store.n_items()).rev() {
        if let Some(item) = store.item(index).and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
        {
            store.remove(index);
            if child_store {
                tree_loading::record_child_store_remove(section, store, index as usize, 1);
            }
            removed_any = true;
        }
    }
    removed_any
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn test_path(id: u8) -> PathBuf {
        PathBuf::from(format!("/workspace/path-{id}"))
    }

    fn oracle(
        folder_paths: &[PathBuf],
        child_rows: &[(PathBuf, Vec<(usize, PathBuf)>)],
    ) -> (HashMap<PathBuf, usize>, HashMap<PathBuf, ItemLocation>) {
        let mut occurrences = HashMap::<PathBuf, Vec<ItemLocation>>::new();
        for (index, path) in folder_paths.iter().enumerate() {
            occurrences
                .entry(path.clone())
                .or_default()
                .push(ItemLocation {
                    parent_dir: None,
                    index,
                });
        }
        for (parent, rows) in child_rows {
            for (index, path) in rows {
                occurrences
                    .entry(path.clone())
                    .or_default()
                    .push(ItemLocation {
                        parent_dir: Some(parent.clone()),
                        index: *index,
                    });
            }
        }
        let counts = occurrences
            .iter()
            .map(|(path, locations)| (path.clone(), locations.len()))
            .collect();
        let locations = occurrences
            .into_iter()
            .filter_map(|(path, locations)| {
                (locations.len() == 1).then(|| (path, locations[0].clone()))
            })
            .collect();
        (counts, locations)
    }

    proptest! {
        #[test]
        fn bulk_child_cache_matches_occurrence_oracle_across_duplicates_and_reorders(
            folder_ids in prop::collection::vec(0u8..8, 0..12),
            other_ids in prop::collection::vec(0u8..8, 0..24),
            old_ids in prop::collection::vec(prop::option::of(0u8..8), 0..40),
            new_ids in prop::collection::vec(prop::option::of(0u8..8), 0..40),
        ) {
            let parent = PathBuf::from("/workspace/parent");
            let other_parent = PathBuf::from("/workspace/other");
            let folder_paths = folder_ids.into_iter().map(test_path).collect::<Vec<_>>();
            let old_rows = old_ids
                .into_iter()
                .enumerate()
                .filter_map(|(index, id)| id.map(|id| (index, test_path(id))))
                .collect::<Vec<_>>();
            let new_rows = new_ids
                .into_iter()
                .enumerate()
                .filter_map(|(index, id)| id.map(|id| (index, test_path(id))))
                .collect::<Vec<_>>();
            let other_rows = other_ids
                .into_iter()
                .enumerate()
                .map(|(index, id)| (index, test_path(id)))
                .collect::<Vec<_>>();

            let (mut counts, mut locations) = oracle(
                &folder_paths,
                &[
                    (parent.clone(), old_rows.clone()),
                    (other_parent.clone(), other_rows.clone()),
                ],
            );
            let mut child_paths = HashMap::from([
                (
                    parent.clone(),
                    old_rows.iter().map(|(_, path)| path.clone()).collect(),
                ),
                (
                    other_parent.clone(),
                    other_rows.iter().map(|(_, path)| path.clone()).collect(),
                ),
            ]);

            replace_child_cache(
                &folder_paths,
                &mut child_paths,
                &mut counts,
                &mut locations,
                &parent,
                &new_rows,
            );

            let (expected_counts, expected_locations) = oracle(
                &folder_paths,
                &[
                    (parent.clone(), new_rows.clone()),
                    (other_parent.clone(), other_rows.clone()),
                ],
            );
            prop_assert_eq!(counts, expected_counts);
            prop_assert_eq!(locations, expected_locations);
            prop_assert_eq!(
                child_paths.get(&other_parent),
                Some(&other_rows.iter().map(|(_, path)| path.clone()).collect::<Vec<_>>())
            );
            if new_rows.is_empty() {
                prop_assert!(!child_paths.contains_key(&parent));
            } else {
                prop_assert_eq!(
                    child_paths.get(&parent),
                    Some(&new_rows.iter().map(|(_, path)| path.clone()).collect::<Vec<_>>())
                );
            }
        }
    }
}

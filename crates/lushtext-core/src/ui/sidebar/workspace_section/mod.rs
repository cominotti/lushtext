// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-workspace section widget: header + file tree + context menus.

mod actions;
// Private implementation module (GObject pattern).
mod imp;

use super::file_tree_item::FileTreeItem;
use crate::model::workspace::WorkspaceId;
use crate::services;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use imp::ItemLocation;
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// glib::wrapper! generates the public wrapper type for this widget.
glib::wrapper! {
    pub struct LushtextWorkspaceSection(ObjectSubclass<imp::LushtextWorkspaceSection>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextWorkspaceSection {
    pub fn new(workspace_id: WorkspaceId) -> Self {
        let obj: Self = Object::builder().build();
        *obj.imp().workspace_id.borrow_mut() = workspace_id;
        obj
    }

    pub fn set_workspace_name(&self, name: &str) {
        self.imp().header_label.set_label(name);
    }

    pub fn workspace_name(&self) -> String {
        self.imp().header_label.label().to_string()
    }

    pub fn workspace_id(&self) -> WorkspaceId {
        self.imp().workspace_id.borrow().clone()
    }

    /// Load root paths into the file tree. Builds the `TreeListModel`
    /// and child models asynchronously for responsive UI.
    pub fn load_roots(&self, roots: &[(PathBuf, bool)]) {
        self.clear_all_dir_state();
        self.reset_item_cache();
        let root_store = gio::ListStore::new::<FileTreeItem>();
        for (root, is_dir) in roots {
            let item = FileTreeItem::new(root.clone(), *is_dir);
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
            item.downcast_ref::<FileTreeItem>()
                .filter(|fi| fi.is_dir())
                .and_then(|fi| fi.path())
                .map(|p| section.build_children_model(&p).upcast::<gio::ListModel>())
        });

        let selection = gtk4::SingleSelection::new(Some(tree_model.clone()));
        let imp = self.imp();
        imp.file_tree_view.set_model(Some(&selection));
        *imp.root_store.borrow_mut() = Some(root_store);
        *imp.tree_model.borrow_mut() = Some(tree_model);
        self.update_button_state();
    }

    /// Add a single root path to an existing file tree.
    /// `is_dir` avoids a `stat(2)` call — callers already know the entry type.
    pub fn add_root(&self, path: &Path, is_dir: bool) {
        let has_store = self.imp().root_store.borrow().is_some();
        if has_store {
            let already_exists = self.imp().root_paths.borrow().iter().any(|p| p == path);
            if !already_exists {
                let store_ref = self.imp().root_store.borrow();
                let root_store = store_ref.as_ref().unwrap();
                let item = FileTreeItem::new(path.to_path_buf(), is_dir);
                let index = root_store.n_items() as usize;
                root_store.append(&item);
                self.cache_root_item(path.to_path_buf(), index);
            }
        } else {
            self.load_roots(&[(path.to_path_buf(), is_dir)]);
        }
        self.update_button_state();
    }

    /// Returns true if this section has at least one root loaded.
    pub fn has_roots(&self) -> bool {
        self.imp()
            .root_store
            .borrow()
            .as_ref()
            .is_some_and(|s| s.n_items() > 0)
    }

    /// Update the add-folder button icon and tooltip based on whether roots exist.
    /// Empty workspace: "Add Folder to Workspace" (folder-new-symbolic)
    /// Workspace with roots: "Replace Workspace Root" (view-refresh-symbolic)
    fn update_button_state(&self) {
        let button = &self.imp().add_folder_button;
        if self.has_roots() {
            button.set_icon_name("view-refresh-symbolic");
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

    fn reset_item_cache(&self) {
        self.imp().root_paths.borrow_mut().clear();
        self.imp().child_paths.borrow_mut().clear();
        self.imp().item_locations.borrow_mut().clear();
    }

    fn cache_root_item(&self, path: PathBuf, index: usize) {
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

    fn cache_child_item(&self, parent_dir: &Path, path: PathBuf, index: usize) {
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
                } else if let Some(position) =
                    root_paths.iter().position(|path| path == target_path)
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

    fn rename_cached_item(&self, old_path: &Path, new_path: &Path) {
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

    fn append_item_preserving_placeholder(
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
            .filter(|existing| existing.is_placeholder())
            .map_or(store.n_items(), |_| store.n_items() - 1);

        if insert_pos == store.n_items() {
            store.append(item);
        } else {
            store.insert(insert_pos, item);
        }

        if let Some(path) = item.path() {
            self.cache_child_item(parent_dir, path, insert_pos as usize);
        }
    }

    fn find_dir_row(&self, dir_path: &Path) -> Option<gtk4::TreeListRow> {
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

    fn find_store_for_dir(&self, dir_path: &Path) -> Option<gio::ListStore> {
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
        self.clear_dir_state(target_path);

        if let Some(location) = self.remove_cached_item(target_path) {
            match location.parent_dir.as_deref() {
                None => {
                    if let Some(ref root_store) = *imp.root_store.borrow()
                        && (location.index as u32) < root_store.n_items()
                    {
                        root_store.remove(location.index as u32);
                        return true;
                    }
                }
                Some(parent_dir) => {
                    if let Some(store) = self.find_store_for_dir(parent_dir)
                        && (location.index as u32) < store.n_items()
                    {
                        store.remove(location.index as u32);
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

        let parent_dir = match target_path.parent() {
            Some(p) => p,
            None => return false,
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

    fn build_children_model(&self, dir_path: &Path) -> gio::ListStore {
        let store = gio::ListStore::new::<FileTreeItem>();
        let path = dir_path.to_path_buf();
        let cancel = Arc::new(AtomicBool::new(false));
        self.imp()
            .dir_stores
            .borrow_mut()
            .insert(path.clone(), store.downgrade());

        if let Some(previous) = self
            .imp()
            .child_scan_tokens
            .borrow_mut()
            .insert(path.clone(), Arc::clone(&cancel))
        {
            previous.store(true, Ordering::Release);
        }

        let section_weak = self.downgrade();
        services::async_task::spawn_blocking_then(
            (store.clone(), path.clone(), Arc::clone(&cancel)),
            move || {
                services::file_tree::scan_directory_bounded(&path, MAX_DIR_ENTRIES, Some(&cancel))
            },
            move |(store, path, cancel), scan| {
                if scan.cancelled {
                    if let Some(section) = section_weak.upgrade() {
                        section.finish_child_scan(&path, &cancel);
                    }
                    return;
                }

                let Some(section) = section_weak.upgrade() else {
                    return;
                };

                if !section.child_scan_is_active(&path, &cancel) {
                    return;
                }

                let mut existing = HashSet::with_capacity(store.n_items() as usize);
                for i in 0..store.n_items() {
                    if let Some(fi) = store.item(i).and_downcast::<FileTreeItem>()
                        && let Some(existing_path) = fi.path()
                    {
                        existing.insert(existing_path);
                    }
                }

                let remaining_budget = MAX_DIR_ENTRIES.saturating_sub(existing.len());
                let mut new_entries = Vec::with_capacity(scan.entries.len().min(remaining_budget));
                let mut truncated = scan.truncated;
                for (entry_path, is_dir) in scan.entries {
                    if existing.contains(&entry_path) {
                        continue;
                    }
                    if new_entries.len() >= remaining_budget {
                        truncated = true;
                        break;
                    }
                    new_entries.push((entry_path, is_dir));
                }

                if truncated {
                    tracing::warn!("Directory truncated to {MAX_DIR_ENTRIES} entries");
                }

                section.append_child_batches(store, path, cancel, new_entries, truncated);
            },
        );

        store
    }

    fn finish_child_scan(&self, dir_path: &Path, token: &Arc<AtomicBool>) {
        let mut tokens = self.imp().child_scan_tokens.borrow_mut();
        let should_remove = tokens
            .get(dir_path)
            .is_some_and(|active| Arc::ptr_eq(active, token));
        if should_remove {
            tokens.remove(dir_path);
        }
    }

    fn clear_dir_state(&self, dir_path: &Path) {
        self.imp()
            .dir_rows
            .borrow_mut()
            .retain(|path, _| path != dir_path && !path.starts_with(dir_path));
        self.imp()
            .dir_stores
            .borrow_mut()
            .retain(|path, _| path != dir_path && !path.starts_with(dir_path));
        self.imp()
            .child_paths
            .borrow_mut()
            .retain(|path, _| path != dir_path && !path.starts_with(dir_path));
        self.imp()
            .item_locations
            .borrow_mut()
            .retain(|path, _| path.as_path() == dir_path || !path.starts_with(dir_path));

        let cancelled: Vec<_> = {
            let mut tokens = self.imp().child_scan_tokens.borrow_mut();
            let paths: Vec<_> = tokens
                .keys()
                .filter(|path| path.as_path() == dir_path || path.starts_with(dir_path))
                .cloned()
                .collect();
            paths
                .into_iter()
                .filter_map(|path| tokens.remove(&path))
                .collect()
        };

        for token in cancelled {
            token.store(true, Ordering::Release);
        }
    }

    fn clear_all_dir_state(&self) {
        self.imp().dir_rows.borrow_mut().clear();
        self.imp().dir_stores.borrow_mut().clear();
        self.imp().child_paths.borrow_mut().clear();
        self.imp().item_locations.borrow_mut().clear();
        self.imp().root_paths.borrow_mut().clear();

        let cancelled: Vec<_> = self
            .imp()
            .child_scan_tokens
            .borrow_mut()
            .drain()
            .map(|(_, token)| token)
            .collect();
        for token in cancelled {
            token.store(true, Ordering::Release);
        }
    }

    fn child_scan_is_active(&self, dir_path: &Path, token: &Arc<AtomicBool>) -> bool {
        if token.load(Ordering::Acquire) {
            self.finish_child_scan(dir_path, token);
            return false;
        }

        {
            let tokens = self.imp().child_scan_tokens.borrow();
            let Some(active) = tokens.get(dir_path) else {
                return false;
            };
            if !Arc::ptr_eq(active, token) {
                return false;
            }
        }

        if let Some(row) = self.find_dir_row(dir_path)
            && !row.is_expanded()
        {
            token.store(true, Ordering::Release);
            self.finish_child_scan(dir_path, token);
            return false;
        }

        true
    }

    fn append_child_batches(
        &self,
        store: gio::ListStore,
        dir_path: PathBuf,
        token: Arc<AtomicBool>,
        entries: Vec<(PathBuf, bool)>,
        truncated: bool,
    ) {
        let pending = std::rc::Rc::new(RefCell::new(VecDeque::from(entries)));
        self.append_next_child_batch(store, dir_path, token, pending, truncated);
    }

    fn append_next_child_batch(
        &self,
        store: gio::ListStore,
        dir_path: PathBuf,
        token: Arc<AtomicBool>,
        pending: std::rc::Rc<RefCell<VecDeque<(PathBuf, bool)>>>,
        truncated: bool,
    ) {
        if !self.child_scan_is_active(&dir_path, &token) {
            return;
        }

        let mut batch = Vec::with_capacity(CHILD_APPEND_BATCH_SIZE);
        {
            let mut pending_entries = pending.borrow_mut();
            for _ in 0..CHILD_APPEND_BATCH_SIZE {
                let Some((entry_path, is_dir)) = pending_entries.pop_front() else {
                    break;
                };
                batch.push(FileTreeItem::new(entry_path, is_dir));
            }
        }

        if !batch.is_empty() {
            let start_index = self
                .imp()
                .child_paths
                .borrow()
                .get(&dir_path)
                .map_or(0, Vec::len);
            store.splice(store.n_items(), 0, &batch);
            for (offset, item) in batch.iter().enumerate() {
                if let Some(path) = item.path() {
                    self.cache_child_item(&dir_path, path, start_index + offset);
                }
            }
        }

        if pending.borrow().is_empty() {
            if truncated {
                let placeholder = [FileTreeItem::new_placeholder(truncated_directory_label())];
                store.splice(store.n_items(), 0, &placeholder);
            }
            self.finish_child_scan(&dir_path, &token);
            return;
        }

        let section_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(1), move || {
            if let Some(section) = section_weak.upgrade() {
                section.append_next_child_batch(store, dir_path, token, pending, truncated);
            }
        });
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
        && !file_item.is_dir()
        && let Some(ref path) = file_item.path()
    {
        callback(path);
    }
}

/// Maximum directory entries before truncation. A single `gio::ListStore`
/// with >10k items causes slow model diff updates in `GtkListView`.
/// Truncated directories show a placeholder row with the count.
const MAX_DIR_ENTRIES: usize = 10_000;
/// Rows appended per main-loop tick when populating a directory tree.
/// 256 items splice in <2ms, staying under the 16ms frame budget.
const CHILD_APPEND_BATCH_SIZE: usize = 256;

fn truncated_directory_label() -> String {
    format!("{MAX_DIR_ENTRIES}+ items - showing first {MAX_DIR_ENTRIES}")
}

impl Default for LushtextWorkspaceSection {
    fn default() -> Self {
        Self::new(WorkspaceId::default())
    }
}

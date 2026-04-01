// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree model builder for the sidebar GtkListView + GtkTreeListModel.
//!
//! Directory scanning runs on a background thread. The returned `ListStore`
//! is initially empty and populates asynchronously, keeping the main thread
//! responsive for large directories.

use crate::services::async_task;
use crate::ui::sidebar::file_tree_item::FileTreeItem;
use gtk4::gio;
use std::path::{Path, PathBuf};

/// Build the root `ListStore` for the tree model from a list of root paths.
pub fn build_root_model(roots: &[PathBuf]) -> gio::ListStore {
    let store = gio::ListStore::new::<FileTreeItem>();
    for root in roots {
        store.append(&FileTreeItem::new(root.clone(), root.is_dir()));
    }
    store
}

/// Build a child `ListStore` for a directory's contents.
///
/// Returns an empty store immediately and populates it from a background
/// thread. The `TreeListModel` reacts to `items-changed` signals
/// automatically, so the tree updates when entries arrive.
pub fn build_children_model(dir_path: &Path) -> gio::ListStore {
    let store = gio::ListStore::new::<FileTreeItem>();
    let path = dir_path.to_path_buf();

    async_task::spawn_blocking_then(
        store.clone(),
        move || scan_directory(&path),
        |store, entries| {
            for (path, is_dir) in entries {
                store.append(&FileTreeItem::new(path, is_dir));
            }
        },
    );

    store
}

/// Scan a directory and return sorted entries (directories first, then alphabetical).
fn scan_directory(dir_path: &Path) -> Vec<(PathBuf, bool)> {
    let read_dir = match std::fs::read_dir(dir_path) {
        Ok(rd) => rd,
        Err(e) => {
            tracing::warn!("Cannot read {}: {}", dir_path.display(), e);
            return Vec::new();
        }
    };

    let mut entries: Vec<(String, PathBuf, bool)> = read_dir
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            let is_dir = path.is_dir();
            Some((name, path, is_dir))
        })
        .collect();

    entries.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });

    entries.into_iter().map(|(_, p, d)| (p, d)).collect()
}

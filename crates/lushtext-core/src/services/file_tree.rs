// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree model builder for the sidebar GtkListView + GtkTreeListModel.

use crate::ui::sidebar::file_tree_item::FileTreeItem;
use gtk4::gio;
use std::path::Path;

/// Build the root `ListStore` for the tree model from a list of root paths.
pub fn build_root_model(roots: &[std::path::PathBuf]) -> gio::ListStore {
    let store = gio::ListStore::new::<FileTreeItem>();
    for root in roots {
        let is_dir = root.is_dir();
        store.append(&FileTreeItem::new(root.clone(), is_dir));
    }
    store
}

/// Build a child `ListStore` for a directory's contents.
/// Returns a sorted list with directories first, then files, both alphabetical.
pub fn build_children_model(dir_path: &Path) -> gio::ListStore {
    let store = gio::ListStore::new::<FileTreeItem>();

    let mut entries: Vec<(String, std::path::PathBuf, bool)> = Vec::new();

    if let Ok(read_dir) = std::fs::read_dir(dir_path) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files/dirs
            if name.starts_with('.') {
                continue;
            }

            let is_dir = path.is_dir();
            entries.push((name, path, is_dir));
        }
    }

    // Sort: directories first, then alphabetical (case-insensitive)
    entries.sort_by(|a, b| {
        b.2.cmp(&a.2) // dirs first
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });

    for (_, path, is_dir) in entries {
        store.append(&FileTreeItem::new(path, is_dir));
    }

    store
}

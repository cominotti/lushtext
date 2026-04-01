// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree sidebar widget.

pub mod file_tree_item;
mod imp;

use crate::services;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::path::{Path, PathBuf};

// Re-export for window integration
pub use file_tree_item::FileTreeItem;

glib::wrapper! {
    pub struct LushtextSidebar(ObjectSubclass<imp::LushtextSidebar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextSidebar {
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Set the workspace name displayed in the header.
    pub fn set_workspace_name(&self, name: &str) {
        self.imp().workspace_label.set_label(name);
    }

    /// Load root paths into the file tree. Builds the `TreeListModel`
    /// and child models asynchronously for responsive UI.
    pub fn load_roots(&self, roots: &[PathBuf]) {
        let root_store = gio::ListStore::new::<file_tree_item::FileTreeItem>();
        for root in roots {
            root_store.append(&file_tree_item::FileTreeItem::new(
                root.clone(),
                root.is_dir(),
            ));
        }

        let tree_model = gtk4::TreeListModel::new(root_store.clone(), false, false, |item| {
            item.downcast_ref::<file_tree_item::FileTreeItem>()
                .filter(|fi| fi.is_dir())
                .map(|fi| build_children_model(&fi.path()))
                .map(|m| m.upcast::<gio::ListModel>())
        });

        let selection = gtk4::SingleSelection::new(Some(tree_model.clone()));
        let imp = self.imp();
        imp.file_tree_view.set_model(Some(&selection));
        *imp.root_store.borrow_mut() = Some(root_store);
        *imp.tree_model.borrow_mut() = Some(tree_model);
    }

    /// Connect a handler for when a file is activated (double-click or Enter).
    ///
    /// The `GtkTreeExpander`'s internal `GestureClick` is disabled for file
    /// rows in `connect_bind` (see `imp.rs`), so `GtkListView`'s built-in
    /// double-click activation fires normally for files. Directory rows keep
    /// the expander gesture active for expand/collapse.
    pub fn connect_file_activated<F: Fn(&std::path::Path) + 'static>(&self, f: F) {
        self.imp()
            .file_tree_view
            .connect_activate(move |list_view, position| {
                activate_file_at(list_view, position, &f);
            });
    }

    /// Register a callback for when a file or directory is successfully renamed.
    pub fn connect_file_renamed<F: Fn(&Path, &Path) + 'static>(&self, f: F) {
        *self.imp().rename_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Register a callback for when a file or directory is successfully deleted.
    pub fn connect_file_deleted<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().delete_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Register a callback for when a new file is created (for auto-opening in a tab).
    pub fn connect_file_created<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().create_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Create a new file or directory inside (or alongside) the right-clicked item.
    pub(crate) fn create_new_item(&self, is_dir: bool) {
        let imp = self.imp();
        let context_path = imp.context_path.borrow().clone();
        let Some(context_path) = context_path else {
            return;
        };

        // Determine target directory
        let target_dir = if imp.context_is_dir.get() {
            context_path.clone()
        } else {
            match context_path.parent() {
                Some(p) => p.to_path_buf(),
                None => return,
            }
        };

        // Create on disk with a unique name (atomic, no TOCTOU)
        let base = if is_dir { "New Folder" } else { "New File" };
        let temp_path = match create_unique(&target_dir, base, is_dir) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(
                    "Failed to create new item in {}: {}",
                    target_dir.display(),
                    e
                );
                return;
            }
        };

        // If context item is a directory, expand it so the new item is visible
        if imp.context_is_dir.get() {
            if let Some(row) = self.find_dir_row(&context_path) {
                if !row.is_expanded() {
                    row.set_expanded(true);
                }
            }
        }

        // Find the target ListStore and add the new item
        let new_item = FileTreeItem::new(temp_path, is_dir);
        new_item.set_pending_rename(true);
        imp.is_new_item.set(true);

        if let Some(store) = self.find_store_for_dir(&target_dir) {
            store.append(&new_item);
        }
    }

    /// Start inline rename for the right-clicked item.
    pub(crate) fn begin_rename(&self) {
        let imp = self.imp();
        let path = imp.context_path.borrow().clone();
        let Some(path) = path else { return };
        let expander = imp.context_expander.borrow().clone();
        let Some(expander) = expander else { return };

        let content_box = expander
            .child()
            .and_downcast::<gtk4::Box>()
            .expect("expander child is Box");

        let icon = content_box
            .first_child()
            .and_downcast::<gtk4::Image>()
            .expect("first child is Image");

        let label = icon
            .next_sibling()
            .and_downcast::<gtk4::Label>()
            .expect("second child is Label");

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let entry = gtk4::Entry::new();
        entry.set_text(&name);
        entry.set_hexpand(true);
        entry.add_css_class("monospace");

        label.set_visible(false);
        content_box.append(&entry);
        entry.grab_focus();
        entry.select_region(0, -1);

        // Capture is_new_item now so closures are self-contained
        // (avoids reading the shared flag live, which could be stale)
        let is_new = self.imp().is_new_item.get();

        // Enter → confirm rename
        let sidebar_weak = self.downgrade();
        let path_c = path.clone();
        let entry_c = entry.clone();
        let label_c = label.clone();
        let box_c = content_box.clone();
        entry.connect_activate(move |_| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.confirm_rename(&path_c, &entry_c, &label_c, &box_c, is_new);
            }
        });

        // Escape → cancel
        let key_ctl = gtk4::EventControllerKey::new();
        let sidebar_weak = self.downgrade();
        let path_c = path.clone();
        let entry_c = entry.clone();
        let label_c = label.clone();
        let box_c = content_box.clone();
        key_ctl.connect_key_pressed(move |_, key, _, _| {
            if key == gdk4::Key::Escape {
                if is_new {
                    if let Some(sidebar) = sidebar_weak.upgrade() {
                        sidebar.cancel_new_item(&path_c, &entry_c, &label_c, &box_c);
                    }
                } else {
                    cancel_rename(&entry_c, &label_c, &box_c);
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        entry.add_controller(key_ctl);

        // Focus-out → cancel
        let focus_ctl = gtk4::EventControllerFocus::new();
        let sidebar_weak = self.downgrade();
        let path_c = path.clone();
        let entry_c = entry.clone();
        let label_c = label.clone();
        let box_c = content_box.clone();
        focus_ctl.connect_leave(move |_| {
            if is_new {
                if let Some(sidebar) = sidebar_weak.upgrade() {
                    sidebar.cancel_new_item(&path_c, &entry_c, &label_c, &box_c);
                }
            } else {
                cancel_rename(&entry_c, &label_c, &box_c);
            }
        });
        entry.add_controller(focus_ctl);
    }

    /// Complete the rename: perform fs rename, update model and label.
    fn confirm_rename(
        &self,
        old_path: &Path,
        entry: &gtk4::Entry,
        label: &gtk4::Label,
        content_box: &gtk4::Box,
        is_new: bool,
    ) {
        // Guard: entry already removed (double-fire from focus-out)
        if entry.parent().is_none() {
            return;
        }

        let new_name = entry.text();
        let new_name = new_name.trim();
        let old_name = old_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let imp = self.imp();

        // Cancel if name is empty or unchanged
        if new_name.is_empty() || new_name == old_name {
            if is_new {
                self.cancel_new_item(old_path, entry, label, content_box);
            } else {
                cancel_rename(entry, label, content_box);
            }
            return;
        }

        let new_path = old_path.with_file_name(new_name);

        // Prevent silent overwrite — rename(2) atomically replaces existing targets
        if new_path.exists() {
            tracing::error!("Cannot rename: {} already exists", new_path.display());
            cancel_rename(entry, label, content_box);
            return;
        }

        match std::fs::rename(old_path, &new_path) {
            Ok(()) => {
                // Update the FileTreeItem in-place
                if let Some(ref expander) = *imp.context_expander.borrow() {
                    if let Some(tree_row) = expander.list_row() {
                        if let Some(file_item) = tree_row
                            .item()
                            .and_downcast::<file_tree_item::FileTreeItem>()
                        {
                            file_item.set_path(new_path.clone());
                        }
                    }
                }

                label.set_label(new_name);
                cancel_rename(entry, label, content_box);
                imp.is_new_item.set(false);

                // Notify window
                if is_new && !imp.context_is_dir.get() {
                    if let Some(ref cb) = *imp.create_callback.borrow() {
                        cb(&new_path);
                    }
                } else if !is_new {
                    if let Some(ref cb) = *imp.rename_callback.borrow() {
                        cb(old_path, &new_path);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to rename {}: {}", old_path.display(), e);
                if is_new {
                    self.cancel_new_item(old_path, entry, label, content_box);
                } else {
                    cancel_rename(entry, label, content_box);
                }
            }
        }
    }

    /// Cancel a new item creation: delete the temp file/dir and remove from model.
    fn cancel_new_item(
        &self,
        temp_path: &Path,
        entry: &gtk4::Entry,
        label: &gtk4::Label,
        content_box: &gtk4::Box,
    ) {
        if entry.parent().is_none() {
            return;
        }
        cancel_rename(entry, label, content_box);
        self.imp().is_new_item.set(false);

        // Delete the temp file/dir from disk
        if temp_path.is_dir() {
            let _ = std::fs::remove_dir(temp_path);
        } else {
            let _ = std::fs::remove_file(temp_path);
        }

        // Remove from model
        self.remove_from_model(temp_path);
    }

    /// Show delete confirmation dialog for the right-clicked item.
    pub(crate) fn show_delete_confirmation(&self) {
        let imp = self.imp();
        let path = imp.context_path.borrow().clone();
        let Some(path) = path else { return };
        let is_dir = imp.context_is_dir.get();

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let kind = if is_dir { "directory" } else { "file" };

        let dialog = libadwaita::AlertDialog::builder()
            .heading(format!("Delete '{name}'?"))
            .body(format!("This will permanently delete the {kind}."))
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let sidebar_weak = self.downgrade();
        let path_c = path.clone();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "delete" {
                return;
            }
            let Some(sidebar) = sidebar_weak.upgrade() else {
                return;
            };

            let result = if is_dir {
                std::fs::remove_dir_all(&path_c)
            } else {
                std::fs::remove_file(&path_c)
            };

            match result {
                Ok(()) => {
                    sidebar.remove_from_model(&path_c);
                    if let Some(ref cb) = *sidebar.imp().delete_callback.borrow() {
                        cb(&path_c);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to delete {}: {}", path_c.display(), e);
                }
            }
        });

        // Present on the root window
        if let Some(root) = self.root() {
            dialog.present(Some(&root));
        }
    }

    /// Find the `TreeListRow` for a directory at the given path.
    fn find_dir_row(&self, dir_path: &Path) -> Option<gtk4::TreeListRow> {
        let tree_model = self.imp().tree_model.borrow();
        let tree_model = tree_model.as_ref()?;
        for i in 0..tree_model.n_items() {
            let row = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()?;
            let item = row.item().and_downcast::<FileTreeItem>()?;
            if item.path() == dir_path && item.is_dir() {
                return Some(row);
            }
        }
        None
    }

    /// Find the ListStore that holds children of the given directory.
    fn find_store_for_dir(&self, dir_path: &Path) -> Option<gio::ListStore> {
        self.find_dir_row(dir_path)?
            .children()
            .and_then(|m| m.downcast::<gio::ListStore>().ok())
    }

    /// Remove an item from the tree model by path.
    pub fn remove_from_model(&self, target_path: &Path) {
        let imp = self.imp();

        // Check root store first
        if let Some(ref root_store) = *imp.root_store.borrow() {
            for i in 0..root_store.n_items() {
                if let Some(item) = root_store
                    .item(i)
                    .and_downcast::<file_tree_item::FileTreeItem>()
                {
                    if item.path() == target_path {
                        root_store.remove(i);
                        return;
                    }
                }
            }
        }

        // Find the parent directory's child store via the TreeListModel
        let parent_dir = match target_path.parent() {
            Some(p) => p,
            None => return,
        };

        if let Some(row) = self.find_dir_row(parent_dir) {
            if let Some(children) = row.children() {
                if let Ok(store) = children.downcast::<gio::ListStore>() {
                    for j in 0..store.n_items() {
                        if let Some(child) =
                            store.item(j).and_downcast::<file_tree_item::FileTreeItem>()
                        {
                            if child.path() == target_path {
                                store.remove(j);
                                return;
                            }
                        }
                    }
                }
            } else {
                tracing::warn!(
                    "remove_from_model: parent dir collapsed, item not removed from model"
                );
            }
        }
    }
}

/// Extract the file item at the given position and call the callback if it's a file.
fn activate_file_at(
    list_view: &gtk4::ListView,
    position: u32,
    callback: &dyn Fn(&std::path::Path),
) {
    let Some(model) = list_view.model() else {
        return;
    };
    if let Some(item) = model.item(position) {
        if let Some(tree_row) = item.downcast_ref::<gtk4::TreeListRow>() {
            if let Some(file_item) = tree_row
                .item()
                .and_then(|i| i.downcast::<file_tree_item::FileTreeItem>().ok())
            {
                if !file_item.is_dir() {
                    callback(&file_item.path());
                }
            }
        }
    }
}

/// Build a child `ListStore` for a directory's contents.
/// Returns an empty store immediately and populates it from a background
/// thread via `spawn_blocking_then`.
fn build_children_model(dir_path: &Path) -> gio::ListStore {
    let store = gio::ListStore::new::<file_tree_item::FileTreeItem>();
    let path = dir_path.to_path_buf();

    services::async_task::spawn_blocking_then(
        store.clone(),
        move || services::file_tree::scan_directory(&path),
        |store, entries| {
            // Collect existing paths for O(1) dedup (items added by create_new_item)
            let existing: std::collections::HashSet<PathBuf> = (0..store.n_items())
                .filter_map(|i| {
                    store
                        .item(i)
                        .and_downcast::<file_tree_item::FileTreeItem>()
                        .map(|fi| fi.path())
                })
                .collect();

            for (entry_path, is_dir) in entries {
                if !existing.contains(&entry_path) {
                    store.append(&file_tree_item::FileTreeItem::new(entry_path, is_dir));
                }
            }
        },
    );

    store
}

/// Atomically create a file or directory with a unique name.
/// Uses `create_new(true)` for files and `create_dir` for directories
/// to avoid TOCTOU races. Retries with " 2", " 3", etc. on `AlreadyExists`.
fn create_unique(dir: &Path, base: &str, is_dir: bool) -> std::io::Result<PathBuf> {
    let candidates =
        std::iter::once(base.to_string()).chain((2..1000).map(|i| format!("{base} {i}")));

    for name in candidates {
        let path = dir.join(&name);
        let result = if is_dir {
            std::fs::create_dir(&path)
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map(|_| ())
        };
        match result {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("could not find unique name"))
}

/// Remove the rename entry and restore the label.
/// Guards against double-fire (entry already removed).
fn cancel_rename(entry: &gtk4::Entry, label: &gtk4::Label, content_box: &gtk4::Box) {
    if entry.parent().is_none() {
        return;
    }
    content_box.remove(entry);
    label.set_visible(true);
}

impl Default for LushtextSidebar {
    fn default() -> Self {
        Self::new()
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-workspace section widget: header + file tree + context menus.

mod imp;

use super::file_tree_item::FileTreeItem;
use crate::model::workspace::WorkspaceId;
use crate::services;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::path::{Path, PathBuf};

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
        let root_store = gio::ListStore::new::<FileTreeItem>();
        for (root, is_dir) in roots {
            root_store.append(&FileTreeItem::new(root.clone(), *is_dir));
        }

        let tree_model = gtk4::TreeListModel::new(root_store.clone(), false, false, |item| {
            item.downcast_ref::<FileTreeItem>()
                .filter(|fi| fi.is_dir())
                .map(|fi| build_children_model(&fi.path()))
                .map(|m| m.upcast::<gio::ListModel>())
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
            let store_ref = self.imp().root_store.borrow();
            let root_store = store_ref.as_ref().unwrap();
            let already_exists = (0..root_store.n_items()).any(|i| {
                root_store
                    .item(i)
                    .and_downcast::<FileTreeItem>()
                    .is_some_and(|fi| fi.path() == path)
            });
            if !already_exists {
                root_store.append(&FileTreeItem::new(path.to_path_buf(), is_dir));
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

    // --- File tree operations (moved from sidebar) ---

    /// Create a new file or directory inside (or alongside) the right-clicked item.
    /// The filesystem operation runs on a background thread to avoid blocking
    /// the UI, especially when `create_unique` retries on name collisions.
    pub(crate) fn create_new_item(&self, is_dir: bool) {
        let imp = self.imp();
        let context_path = imp.context_path.borrow().clone();
        let Some(context_path) = context_path else {
            return;
        };

        let target_dir = if imp.context_is_dir.get() {
            context_path.clone()
        } else {
            match context_path.parent() {
                Some(p) => p.to_path_buf(),
                None => return,
            }
        };

        let base = if is_dir { "New Folder" } else { "New File" };
        let context_is_dir = imp.context_is_dir.get();
        let base_owned = base.to_string();
        let target_dir_for_bg = target_dir.clone();

        services::async_task::spawn_blocking_then(
            self.clone(),
            move || create_unique(&target_dir_for_bg, &base_owned, is_dir),
            move |section, result| {
                let temp_path = match result {
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

                let mut was_collapsed = false;
                if context_is_dir {
                    if let Some(row) = section.find_dir_row(&context_path) {
                        if !row.is_expanded() {
                            was_collapsed = true;
                            row.set_expanded(true);
                        }
                    }
                }

                let new_item = FileTreeItem::new(temp_path, is_dir);
                new_item.set_pending_rename(true);
                section.imp().is_new_item.set(true);

                if was_collapsed {
                    let section_weak = section.downgrade();
                    glib::idle_add_local_once(move || {
                        if let Some(section) = section_weak.upgrade() {
                            if let Some(store) = section.find_store_for_dir(&target_dir) {
                                store.append(&new_item);
                            }
                        }
                    });
                } else if let Some(store) = section.find_store_for_dir(&target_dir) {
                    store.append(&new_item);
                }
            },
        );
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

        label.set_visible(false);
        content_box.append(&entry);
        entry.grab_focus();
        entry.select_region(0, -1);

        let is_new = self.imp().is_new_item.get();

        // Enter → confirm rename
        let section_weak = self.downgrade();
        let path_c = path.clone();
        let entry_c = entry.clone();
        let label_c = label.clone();
        let box_c = content_box.clone();
        entry.connect_activate(move |_| {
            if let Some(section) = section_weak.upgrade() {
                section.confirm_rename(&path_c, &entry_c, &label_c, &box_c, is_new);
            }
        });

        // Escape → cancel
        let key_ctl = gtk4::EventControllerKey::new();
        let section_weak = self.downgrade();
        let path_c = path.clone();
        let entry_c = entry.clone();
        let label_c = label.clone();
        let box_c = content_box.clone();
        key_ctl.connect_key_pressed(move |_, key, _, _| {
            if key == gdk4::Key::Escape {
                if is_new {
                    if let Some(section) = section_weak.upgrade() {
                        section.cancel_new_item(&path_c, &entry_c, &label_c, &box_c);
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
        let section_weak = self.downgrade();
        let path_c = path.clone();
        let entry_c = entry.clone();
        let label_c = label.clone();
        let box_c = content_box.clone();
        focus_ctl.connect_leave(move |_| {
            if is_new {
                if let Some(section) = section_weak.upgrade() {
                    section.cancel_new_item(&path_c, &entry_c, &label_c, &box_c);
                }
            } else {
                cancel_rename(&entry_c, &label_c, &box_c);
            }
        });
        entry.add_controller(focus_ctl);
    }

    fn confirm_rename(
        &self,
        old_path: &Path,
        entry: &gtk4::Entry,
        label: &gtk4::Label,
        content_box: &gtk4::Box,
        is_new: bool,
    ) {
        if entry.parent().is_none() {
            return;
        }

        let new_name = entry.text();
        let new_name = new_name.trim();
        let old_name = old_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if new_name.is_empty() || new_name == old_name {
            if is_new {
                self.cancel_new_item(old_path, entry, label, content_box);
            } else {
                cancel_rename(entry, label, content_box);
            }
            return;
        }

        let new_path = old_path.with_file_name(new_name);
        let new_name_owned = new_name.to_string();
        let is_dir = self.imp().context_is_dir.get();

        // Remove the inline entry immediately — label shows old name until rename completes
        let label = label.clone();
        cancel_rename(entry, &label, content_box);

        let old_path = old_path.to_path_buf();
        let new_path_c = new_path.clone();
        services::async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let result = std::fs::rename(&old_path, &new_path_c);
                (old_path, new_path_c, result)
            },
            move |section, (old_path, new_path, result)| {
                let imp = section.imp();
                match result {
                    Ok(()) => {
                        if let Some(ref expander) = *imp.context_expander.borrow() {
                            if let Some(tree_row) = expander.list_row() {
                                if let Some(file_item) =
                                    tree_row.item().and_downcast::<FileTreeItem>()
                                {
                                    file_item.set_path(new_path.clone());
                                }
                            }
                        }
                        label.set_label(&new_name_owned);
                        imp.is_new_item.set(false);

                        if is_new && !is_dir {
                            if let Some(ref cb) = *imp.create_callback.borrow() {
                                cb(&new_path);
                            }
                        } else if !is_new {
                            if let Some(ref cb) = *imp.rename_callback.borrow() {
                                cb(&old_path, &new_path);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to rename {}: {}", old_path.display(), e);
                        if is_new {
                            imp.is_new_item.set(false);
                            // Fire-and-forget cleanup on a background thread to avoid
                            // blocking the main thread on slow filesystems (NFS, FUSE).
                            let old_path_bg = old_path.clone();
                            std::thread::spawn(move || {
                                if is_dir {
                                    let _ = std::fs::remove_dir(&old_path_bg);
                                } else {
                                    let _ = std::fs::remove_file(&old_path_bg);
                                }
                            });
                            section.remove_from_model(&old_path);
                        }
                    }
                }
            },
        );
    }

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

        // Fire-and-forget deletion of the temp item on a background thread.
        // Uses context_is_dir instead of stat to avoid a synchronous syscall.
        let path = temp_path.to_path_buf();
        let is_dir = self.imp().context_is_dir.get();
        std::thread::spawn(move || {
            if is_dir {
                let _ = std::fs::remove_dir(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        });

        self.remove_from_model(temp_path);
    }

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

        let section_weak = self.downgrade();
        let path_c = path.clone();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "delete" {
                return;
            }
            let Some(section) = section_weak.upgrade() else {
                return;
            };

            let path_for_io = path_c.clone();
            services::async_task::spawn_blocking_then(
                section,
                move || {
                    let result = if is_dir {
                        std::fs::remove_dir_all(&path_for_io)
                    } else {
                        std::fs::remove_file(&path_for_io)
                    };
                    (path_for_io, result)
                },
                |section, (path, result)| match result {
                    Ok(()) => {
                        section.remove_from_model(&path);
                        if let Some(ref cb) = *section.imp().delete_callback.borrow() {
                            cb(&path);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to delete {}: {}", path.display(), e);
                    }
                },
            );
        });

        if let Some(root) = self.root() {
            dialog.present(Some(&root));
        }
    }

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

    fn find_store_for_dir(&self, dir_path: &Path) -> Option<gio::ListStore> {
        self.find_dir_row(dir_path)?
            .children()
            .and_then(|m| m.downcast::<gio::ListStore>().ok())
    }

    /// Remove an item from the tree model by path. Returns true if found and removed.
    pub fn remove_from_model(&self, target_path: &Path) -> bool {
        let imp = self.imp();

        if let Some(ref root_store) = *imp.root_store.borrow() {
            for i in 0..root_store.n_items() {
                if let Some(item) = root_store.item(i).and_downcast::<FileTreeItem>() {
                    if item.path() == target_path {
                        root_store.remove(i);
                        return true;
                    }
                }
            }
        }

        let parent_dir = match target_path.parent() {
            Some(p) => p,
            None => return false,
        };

        if let Some(row) = self.find_dir_row(parent_dir) {
            if let Some(children) = row.children() {
                if let Ok(store) = children.downcast::<gio::ListStore>() {
                    for j in 0..store.n_items() {
                        if let Some(child) = store.item(j).and_downcast::<FileTreeItem>() {
                            if child.path() == target_path {
                                store.remove(j);
                                return true;
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
        false
    }
}

/// Extract the file item at the given position and call the callback if it's a file.
fn activate_file_at(list_view: &gtk4::ListView, position: u32, callback: &dyn Fn(&Path)) {
    let Some(model) = list_view.model() else {
        return;
    };
    if let Some(item) = model.item(position) {
        if let Some(tree_row) = item.downcast_ref::<gtk4::TreeListRow>() {
            if let Some(file_item) = tree_row
                .item()
                .and_then(|i| i.downcast::<FileTreeItem>().ok())
            {
                if !file_item.is_dir() {
                    callback(&file_item.path());
                }
            }
        }
    }
}

/// Maximum directory entries before truncation. A single `gio::ListStore`
/// with >10k items causes slow model diff updates in `GtkListView`.
const MAX_DIR_ENTRIES: usize = 10_000;

/// Build a child `ListStore` for a directory's contents.
/// Uses `splice` to emit a single `items-changed` signal for the batch,
/// and caps entries at `MAX_DIR_ENTRIES`.
fn build_children_model(dir_path: &Path) -> gio::ListStore {
    let store = gio::ListStore::new::<FileTreeItem>();
    let path = dir_path.to_path_buf();

    services::async_task::spawn_blocking_then(
        store.clone(),
        move || services::file_tree::scan_directory(&path),
        |store, entries| {
            let existing: std::collections::HashSet<PathBuf> = (0..store.n_items())
                .filter_map(|i| {
                    store
                        .item(i)
                        .and_downcast::<FileTreeItem>()
                        .map(|fi| fi.path())
                })
                .collect();

            let new_items: Vec<FileTreeItem> = entries
                .into_iter()
                .filter(|(path, _)| !existing.contains(path))
                .take(MAX_DIR_ENTRIES)
                .map(|(path, is_dir)| FileTreeItem::new(path, is_dir))
                .collect();

            if new_items.len() >= MAX_DIR_ENTRIES {
                tracing::warn!("Directory truncated to {MAX_DIR_ENTRIES} entries");
            }

            let pos = store.n_items();
            store.splice(pos, 0, &new_items);
        },
    );

    store
}

/// Atomically create a file or directory with a unique name.
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
fn cancel_rename(entry: &gtk4::Entry, label: &gtk4::Label, content_box: &gtk4::Box) {
    if entry.parent().is_none() {
        return;
    }
    content_box.remove(entry);
    label.set_visible(true);
}

impl Default for LushtextWorkspaceSection {
    fn default() -> Self {
        Self::new(WorkspaceId::default())
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! File operation actions for workspace sections: create, rename, delete.

use super::FileTreeItem;
use crate::services;
use crate::services::filesystem::{mutate as fs_mutate, write as fs_write};
use glib::prelude::*;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

impl super::LushtextWorkspaceSection {
    // --- File tree operations ---

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
                if context_is_dir
                    && let Some(row) = section.find_dir_row(&context_path)
                    && !row.is_expanded()
                {
                    was_collapsed = true;
                    row.set_expanded(true);
                }

                let new_item = FileTreeItem::new(temp_path, is_dir, None);
                new_item.set_pending_rename(true);
                section.imp().is_new_item.set(true);

                if was_collapsed {
                    let section_weak = section.downgrade();
                    glib::idle_add_local_once(move || {
                        if let Some(section) = section_weak.upgrade()
                            && let Some(store) = section.find_store_for_dir(&target_dir)
                        {
                            section.append_item_preserving_placeholder(
                                &store,
                                &target_dir,
                                &new_item,
                            );
                        }
                    });
                } else if let Some(store) = section.find_store_for_dir(&target_dir) {
                    section.append_item_preserving_placeholder(&store, &target_dir, &new_item);
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
            .map(|n| n.to_string_lossy().into_owned())
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
        // Guard: if the entry was already removed from its parent box (by a
        // prior confirm or cancel), this is a double-fire from the focus-out
        // handler. Skip to avoid operating on a detached widget.
        if entry.parent().is_none() {
            return;
        }

        let new_name = entry.text();
        let new_name = new_name.trim();
        let old_name = old_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
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
                let result = fs_write::rename_durable(&old_path, &new_path_c);
                (old_path, new_path_c, result)
            },
            move |section, (old_path, new_path, result)| {
                let imp = section.imp();
                match result {
                    Ok(()) => {
                        if let Some(ref expander) = *imp.context_expander.borrow()
                            && let Some(tree_row) = expander.list_row()
                            && let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>()
                        {
                            if is_dir {
                                super::tree_loading::clear_dir_state(&section, &old_path);
                                imp.dir_rows
                                    .borrow_mut()
                                    .insert(new_path.clone(), tree_row.downgrade());
                            }
                            file_item.set_path(new_path.clone());
                            section.rename_cached_item(&old_path, &new_path);
                            if file_item.is_empty() == Some(true) {
                                label.set_markup(&format!(
                                    "{} <span alpha=\"60%\"><i>(Empty)</i></span>",
                                    glib::markup_escape_text(&new_name_owned)
                                ));
                            } else {
                                label.set_label(&new_name_owned);
                            }
                        }
                        imp.is_new_item.set(false);

                        if is_new
                            && !is_dir
                            && let Some(ref cb) = *imp.create_callback.borrow()
                        {
                            cb(&new_path);
                        } else if !is_new && let Some(ref cb) = *imp.rename_callback.borrow() {
                            cb(&old_path, &new_path);
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
                                    let _ = fs_mutate::remove_dir_if_exists(&old_path_bg);
                                } else {
                                    let _ = fs_mutate::remove_file_if_exists(&old_path_bg);
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
                let _ = fs_mutate::remove_dir_if_exists(&path);
            } else {
                let _ = fs_mutate::remove_file_if_exists(&path);
            }
        });

        self.remove_from_model(temp_path);
    }

    pub(crate) fn show_delete_confirmation(&self) {
        let imp = self.imp();
        let path = imp.context_path.borrow().clone();
        let Some(path) = path else { return };
        let is_dir = imp.context_is_dir.get();

        let name = super::super::file_tree_item::display_name_for_path(&path);

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
                        fs_mutate::remove_dir_all_if_exists(&path_for_io)
                    } else {
                        fs_mutate::remove_file_if_exists(&path_for_io)
                    };
                    (path_for_io, result)
                },
                |section, (path, result)| match result {
                    Ok(_) => {
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
}

/// Atomically create a file or directory with a unique name.
fn create_unique(dir: &Path, base: &str, is_dir: bool) -> std::io::Result<PathBuf> {
    let mut name = base.to_string();

    for attempt in 1..1000 {
        if attempt > 1 {
            name.clear();
            name.push_str(base);
            name.push(' ');
            let _ = write!(&mut name, "{attempt}");
        }

        let path = dir.join(&name);
        let result = if is_dir {
            fs_mutate::create_dir(&path).and_then(|()| fs_write::sync_parent_dir(&path))
        } else {
            fs_write::create_new_empty_file_durable(&path)
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
pub(super) fn cancel_rename(entry: &gtk4::Entry, label: &gtk4::Label, content_box: &gtk4::Box) {
    if entry.parent().is_none() {
        return;
    }
    content_box.remove(entry);
    label.set_visible(true);
}

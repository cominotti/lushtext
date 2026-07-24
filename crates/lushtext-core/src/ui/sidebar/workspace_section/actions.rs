// SPDX-License-Identifier: GPL-3.0-or-later

//! File operation actions for workspace sections: create, rename, delete.

use super::FileTreeItem;
use crate::services::filesystem::{mutate as fs_mutate, write as fs_write};
use crate::ui::accessibility;
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
        let target = imp.context_target.borrow().clone();
        let Some(target) = target else {
            return;
        };
        let context_path = target.path;

        let target_dir = if target.is_dir {
            context_path.clone()
        } else {
            match context_path.parent() {
                Some(p) => p.to_path_buf(),
                None => return,
            }
        };

        let base = if is_dir { "New Folder" } else { "New File" };
        let context_is_dir = target.is_dir;
        let base_owned = base.to_string();
        let target_dir_for_bg = target_dir.clone();

        gtk_lush_tasks::spawn_blocking_then(
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
                    // Schedule on GTK's main loop after expansion so the child
                    // model exists before this code looks up and appends to its store.
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
        let target = imp.context_target.borrow().clone();
        let Some(target) = target else { return };
        let path = target.path;
        let expander = target.expander;

        let content_box = expander
            .child()
            .and_downcast::<gtk4::Box>()
            .expect("expander child is Box");

        let mut child = content_box.first_child();
        let mut icon = None;
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(image) = widget.downcast::<gtk4::Image>() {
                icon = Some(image);
                break;
            }
        }
        let icon = icon.expect("row content contains Image");

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
        let is_new = imp.is_new_item.get();
        let kind = if target.is_dir { "folder" } else { "file" };
        let entry_label = if is_new {
            format!("Name new {kind}")
        } else {
            format!("Rename {name}")
        };
        let entry_description = if is_new {
            format!(
                "Enter confirms the new {kind}. Escape cancels and removes the temporary {kind}."
            )
        } else {
            "Enter confirms the new name. Escape cancels rename.".to_string()
        };
        accessibility::set_labelled_description(&entry, &entry_label, &entry_description);
        accessibility::set_key_shortcuts(&entry, "Enter, Escape");
        entry.set_tooltip_text(Some(&entry_description));

        label.set_visible(false);
        content_box.append(&entry);
        entry.grab_focus();
        entry.select_region(0, -1);

        // Entry, key, and focus handlers use weak row widgets so the signal
        // closures cannot keep recycled or detached inline-rename rows alive.
        let section_weak = self.downgrade();
        let path_c = path.clone();
        let label_weak = label.downgrade();
        let box_weak = content_box.downgrade();
        entry.connect_activate(move |entry| {
            if let (Some(section), Some(label), Some(content_box)) = (
                section_weak.upgrade(),
                label_weak.upgrade(),
                box_weak.upgrade(),
            ) {
                section.confirm_rename(&path_c, entry, &label, &content_box, is_new);
            }
        });

        let key_ctl = gtk4::EventControllerKey::new();
        let section_weak = self.downgrade();
        let path_c = path.clone();
        let entry_weak = entry.downgrade();
        let label_weak = label.downgrade();
        let box_weak = content_box.downgrade();
        key_ctl.connect_key_pressed(move |_, key, _, _| {
            if key == gdk4::Key::Escape {
                if let (Some(entry), Some(label), Some(content_box)) = (
                    entry_weak.upgrade(),
                    label_weak.upgrade(),
                    box_weak.upgrade(),
                ) {
                    if is_new {
                        if let Some(section) = section_weak.upgrade() {
                            section.cancel_new_item(&path_c, &entry, &label, &content_box);
                        }
                    } else {
                        cancel_rename(&entry, &label, &content_box);
                    }
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        entry.add_controller(key_ctl);

        let focus_ctl = gtk4::EventControllerFocus::new();
        let section_weak = self.downgrade();
        let path_c = path;
        let entry_weak = entry.downgrade();
        let label_weak = label.downgrade();
        let box_weak = content_box.downgrade();
        focus_ctl.connect_leave(move |_| {
            if let (Some(entry), Some(label), Some(content_box)) = (
                entry_weak.upgrade(),
                label_weak.upgrade(),
                box_weak.upgrade(),
            ) {
                if is_new {
                    if let Some(section) = section_weak.upgrade() {
                        section.cancel_new_item(&path_c, &entry, &label, &content_box);
                    }
                } else {
                    cancel_rename(&entry, &label, &content_box);
                }
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
        let is_dir = self
            .imp()
            .context_target
            .borrow()
            .as_ref()
            .is_some_and(|target| target.is_dir);

        // Restore the row immediately so focus-out cannot start a second rename
        // while the filesystem rename runs.
        let label = label.clone();
        cancel_rename(entry, &label, content_box);

        let old_path = old_path.to_path_buf();
        let new_path_c = new_path;
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let result = fs_write::rename_durable(&old_path, &new_path_c);
                (old_path, new_path_c, result)
            },
            move |section, (old_path, new_path, result)| {
                let imp = section.imp();
                match result {
                    Ok(()) => {
                        if let Some(ref target) = *imp.context_target.borrow()
                            && let Some(tree_row) = target.expander.list_row()
                            && let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>()
                        {
                            if is_dir {
                                // Move expansion intent to the new prefix before
                                // the old subtree state is retired; the renamed
                                // rows stay expanded in place.
                                section.rename_expanded_subtree(&old_path, &new_path);
                                super::tree_loading::clear_dir_state(&section, &old_path);
                                imp.dir_rows
                                    .borrow_mut()
                                    .insert(new_path.clone(), tree_row.downgrade());
                            }
                            file_item.set_path(new_path.clone());
                            // `set_path` mutates the row model in place, so the
                            // flattened model emits no splice for the watcher mirror.
                            section.refresh_workspace_watch_row(&tree_row);
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

                        // End the callback borrow before invoking so a callback
                        // that re-enters registration cannot panic; restore it
                        // unless invocation registered a replacement.
                        if is_new && !is_dir {
                            let cb = imp.create_callback.borrow_mut().take();
                            if let Some(cb) = cb {
                                cb(&new_path);
                                imp.create_callback.borrow_mut().get_or_insert(cb);
                            }
                        } else if !is_new {
                            let cb = imp.rename_callback.borrow_mut().take();
                            if let Some(cb) = cb {
                                cb(&old_path, &new_path);
                                imp.rename_callback.borrow_mut().get_or_insert(cb);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to rename {}: {}", old_path.display(), e);
                        if is_new {
                            imp.is_new_item.set(false);
                            spawn_temp_item_cleanup(old_path.clone(), is_dir);
                            let _ = section.remove_from_model(&old_path);
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

        // Uses the captured context target instead of stat to avoid a synchronous syscall.
        let path = temp_path.to_path_buf();
        let is_dir = self
            .imp()
            .context_target
            .borrow()
            .as_ref()
            .is_some_and(|target| target.is_dir);
        spawn_temp_item_cleanup(path, is_dir);

        let _ = self.remove_from_model(temp_path);
    }

    pub(crate) fn show_delete_confirmation(&self) {
        let imp = self.imp();
        let target = imp.context_target.borrow().clone();
        let Some(target) = target else { return };
        let path = target.path;
        let is_dir = target.is_dir;

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
        let path_c = path;
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "delete" {
                return;
            }
            let Some(section) = section_weak.upgrade() else {
                return;
            };

            let path_for_io = path_c.clone();
            gtk_lush_tasks::spawn_blocking_then(
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
                        let _ = section.remove_from_model(&path);
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

        accessibility::announce_with_lane(
            self,
            &format!("Delete {name}? This will permanently delete the {kind}."),
            accessibility::AnnouncementLane::Alert,
        );
        if let Some(root) = self.root() {
            dialog.present(Some(&root));
        }
    }

    pub(crate) fn show_remove_folder_confirmation(&self) {
        let imp = self.imp();
        let target = imp.context_target.borrow().clone();
        let Some(target) = target else { return };
        let path = target.path;
        let Some(folder_id) = target.workspace_folder_id else {
            return;
        };

        let name = super::super::file_tree_item::display_name_for_path(&path);

        let dialog = libadwaita::AlertDialog::builder()
            .heading(format!("Remove '{name}' from workspace?"))
            .body("The folder will be removed from this workspace. Files on disk and folder notes will be kept.")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("remove", "Remove");
        dialog.set_response_appearance("remove", libadwaita::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let section_weak = self.downgrade();
        dialog.connect_response(None::<&str>, move |_, response| {
            if response != "remove" {
                return;
            }
            if let Some(section) = section_weak.upgrade() {
                section.notify_remove_folder_requested(&folder_id, &path);
            }
        });

        accessibility::announce_with_lane(
            self,
            &format!("Remove {name} from workspace? Files on disk and folder notes will be kept."),
            accessibility::AnnouncementLane::Alert,
        );
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
            fs_write::create_dir_durable(&path)
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

/// Fire-and-forget removal of a temporary or failed inline item on a background
/// thread.
///
/// Both the cancelled-new-item flow and the failed-rename recovery flow need to
/// discard a placeholder path without blocking the GTK main thread on slow
/// filesystems (NFS, FUSE), so this intentionally bypasses
/// `gtk_lush_tasks::spawn_blocking_then` (which is for work whose completion
/// touches GTK state). The cleanup is best-effort — the user-facing flow is
/// already resolved — but a failure is logged at warning level so orphaned
/// placeholder debugging has a trail instead of a silent drop.
fn spawn_temp_item_cleanup(path: PathBuf, is_dir: bool) {
    std::thread::spawn(move || {
        let result = if is_dir {
            fs_mutate::remove_dir_if_exists(&path)
        } else {
            fs_mutate::remove_file_if_exists(&path)
        };
        if let Err(e) = result {
            tracing::warn!(
                "Failed to clean up temporary item {}: {}",
                path.display(),
                e
            );
        }
    });
}

/// Remove the rename entry and restore the label.
pub(super) fn cancel_rename(entry: &gtk4::Entry, label: &gtk4::Label, content_box: &gtk4::Box) {
    if entry.parent().is_none() {
        return;
    }
    content_box.remove(entry);
    label.set_visible(true);
}

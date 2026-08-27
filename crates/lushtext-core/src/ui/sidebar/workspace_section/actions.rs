// SPDX-License-Identifier: GPL-3.0-or-later

//! File operation actions for workspace sections: create, rename, delete.

use super::FileTreeItem;
use crate::services::filesystem::{
    metadata as fs_metadata, mutate as fs_mutate, write as fs_write,
};
use crate::services::notifications::NotificationSeverity;
use crate::ui::accessibility;
use crate::ui::sidebar::policy::{
    self, MAX_UNIQUE_NAME_ATTEMPTS, RenameIntent, WorkspaceRenameRefusal,
};
use crate::ui::sidebar::seams::{FileOperationFacts, FileOperationTicket};
use glib::prelude::*;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::*;
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

        let intent = policy::rename_intent(old_path, entry.text().as_str());
        let RenameIntent::Rename {
            new_path,
            new_name: new_name_owned,
        } = intent
        else {
            if is_new {
                self.cancel_new_item(old_path, entry, label, content_box);
            } else {
                cancel_rename(entry, label, content_box);
            }
            return;
        };

        // Captured once, at dispatch. The live `context_target` cell is replaced
        // by any right-click, by a new-item bind, and cleared on row recycling,
        // so re-reading it in the completion could apply this rename's row
        // projection to a different file's row. See `sidebar::seams`.
        let ticket = FileOperationTicket::new(
            old_path.to_path_buf(),
            self.imp()
                .context_target
                .borrow()
                .as_ref()
                .is_some_and(|target| target.is_dir),
            is_new,
        );
        let row = self
            .imp()
            .context_target
            .borrow()
            .as_ref()
            .and_then(|target| target.expander.list_row());

        // Restore the row immediately so focus-out cannot start a second rename
        // while the filesystem rename runs.
        let label = label.clone();
        cancel_rename(entry, &label, content_box);

        let worker_ticket = ticket.clone();
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                #[cfg(feature = "test-utils")]
                crate::ui::sidebar::test_policy::delay_rename_worker();
                let outcome = rename_target_guarded(worker_ticket.path(), &new_path);
                (new_path, outcome)
            },
            move |section, (new_path, outcome)| {
                let imp = section.imp();
                let old_path = ticket.path().to_path_buf();
                match outcome {
                    Ok(()) => {
                        let facts = FileOperationFacts::new(
                            row.as_ref()
                                .and_then(gtk4::TreeListRow::item)
                                .and_downcast::<FileTreeItem>()
                                .and_then(|item| item.path()),
                        );
                        if let Some(tree_row) = row.as_ref()
                            && ticket.row_is_current(&facts)
                            && let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>()
                        {
                            if ticket.is_dir() {
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
                            section.refresh_workspace_watch_row(tree_row);
                            section.rename_cached_item(&old_path, &new_path);
                            if file_item.is_empty() == Some(true) {
                                label.set_markup(&format!(
                                    "{} <span alpha=\"60%\"><i>(Empty)</i></span>",
                                    glib::markup_escape_text(&new_name_owned)
                                ));
                            } else {
                                label.set_label(&new_name_owned);
                            }
                        } else {
                            // The row this rename was issued for is gone or now
                            // describes another file. The rename itself
                            // succeeded, so repair the watcher mirror and the
                            // item cache through the section rather than through
                            // a stale row, and let the next refresh re-project.
                            if ticket.is_dir() {
                                section.rename_expanded_subtree(&old_path, &new_path);
                                super::tree_loading::clear_dir_state(&section, &old_path);
                            }
                            section.rename_cached_item(&old_path, &new_path);
                            section.request_workspace_watch_restart();
                        }
                        imp.is_new_item.set(false);

                        // End the callback borrow before invoking so a callback
                        // that re-enters registration cannot panic; restore it
                        // unless invocation registered a replacement.
                        if ticket.is_new() && !ticket.is_dir() {
                            let cb = imp.create_callback.borrow_mut().take();
                            if let Some(cb) = cb {
                                cb(&new_path);
                                imp.create_callback.borrow_mut().get_or_insert(cb);
                            }
                        } else if !ticket.is_new() {
                            let cb = imp.rename_callback.borrow_mut().take();
                            if let Some(cb) = cb {
                                cb(&old_path, &new_path);
                                imp.rename_callback.borrow_mut().get_or_insert(cb);
                            }
                        }
                    }
                    Err(RenameFailure::Refused(refusal)) => {
                        // Refusing is the whole point: the platform rename
                        // silently replaces a regular destination, and the
                        // replaced file's contents are unrecoverable.
                        section.emit_message(&refusal.message(), NotificationSeverity::Warning);
                        if ticket.is_new() {
                            imp.is_new_item.set(false);
                            // A refused *first* name still leaves the created
                            // placeholder on disk and in the tree, so it needs the
                            // same cleanup an I/O failure gets.
                            spawn_temp_item_cleanup(old_path.clone(), ticket.is_dir());
                            let _ = section.remove_from_model(&old_path);
                        }
                    }
                    Err(RenameFailure::Io(error)) => {
                        tracing::error!("Failed to rename {}: {}", old_path.display(), error);
                        if ticket.is_new() {
                            imp.is_new_item.set(false);
                            spawn_temp_item_cleanup(old_path.clone(), ticket.is_dir());
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
                    // Ordered against in-app writers for the same target: an
                    // editor save in flight must not have its temp-file rename
                    // land after this unlink and resurrect the deleted name.
                    //
                    // The directory branch is `remove_dir_all_if_exists` and is
                    // deliberately **recursive** — this is the user's explicit,
                    // confirmed "Delete this directory". That is unlike the
                    // placeholder cleanup below, whose directory branch is
                    // empty-only precisely because nothing there was confirmed.
                    let guard = fs_write::TargetWriteGuard::acquire(&path_for_io);
                    let result = match guard {
                        Ok(_guard) => {
                            if is_dir {
                                fs_mutate::remove_dir_all_if_exists(&path_for_io)
                            } else {
                                fs_mutate::remove_file_if_exists(&path_for_io)
                            }
                        }
                        Err(error) => Err(error),
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

/// Why a guarded workspace rename did not happen.
enum RenameFailure {
    /// The workflow refused the rename; the user is told why.
    Refused(WorkspaceRenameRefusal),
    /// The platform rename failed.
    Io(std::io::Error),
}

/// Rename one workspace path under the shared write guard, refusing to replace.
///
/// Three data-safety properties live here and nowhere else:
///
/// 1. **The destination is never replaced.** `rename_durable` is `rename(2)`,
///    which silently replaces a regular destination; the replaced file's
///    contents are unrecoverable. The existence check therefore happens *inside*
///    the worker while the guard is held, not on the GTK thread where it would
///    already be stale.
/// 2. **The rename is ordered against in-app writers.** An editor save resolves
///    its target, writes a temp file, and renames it into place. Without the
///    guard, a sidebar rename interleaved with that sequence lets the save's
///    final `rename()` **re-create the old filename** with the buffer bytes,
///    leaving the tab's new path stale on disk while the UI reports success.
/// 3. **Two guards cannot deadlock, and one target cannot deadlock against
///    itself.** This is subtler than it looks and the first version got it wrong:
///    `TargetWriteGuard` keys on the **resolved** identity — symlinks
///    canonicalize to their target, and a missing file resolves to its canonical
///    parent plus its name — while the obvious implementation sorts the **raw**
///    paths. Sorting raw paths orders nothing about the keys, so two concurrent
///    renames could still take the same pair in opposite order. Worse, renaming
///    the symlink `link` (→ `target`) to the name `target` resolves **both** paths
///    to the same key, and the second acquire would block on the first forever,
///    holding a worker slot until the process exits. Both are avoided by
///    resolving first, deduplicating, and acquiring in **resolved** order.
fn rename_target_guarded(old_path: &Path, new_path: &Path) -> Result<(), RenameFailure> {
    let source = fs_write::resolve_target_identity(old_path).map_err(RenameFailure::Io)?;
    let destination = fs_write::resolve_target_identity(new_path).map_err(RenameFailure::Io)?;

    let refuse_existing_destination = || {
        Err(RenameFailure::Refused(
            WorkspaceRenameRefusal::DestinationExists {
                name: new_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            },
        ))
    };

    if source == destination {
        // The two names already denote one target — a symlink renamed onto the
        // file it points at, most plainly. Refusing here is both the correct
        // answer and what keeps the two acquires below from deadlocking.
        return refuse_existing_destination();
    }

    let (first, second) = if source.as_path() <= destination.as_path() {
        (source, destination)
    } else {
        (destination, source)
    };
    let _first = fs_write::TargetWriteGuard::from_identity(first);
    let _second = fs_write::TargetWriteGuard::from_identity(second);

    // Atomic where the kernel supports it: `RENAME_NOREPLACE` makes "does the
    // destination exist" and "rename" one operation, so no other process can
    // create the destination between them. An `exists()` check plus a rename is
    // two syscalls and is therefore only best-effort against external writers —
    // adequate against LushText's own writers, which the guards above serialize,
    // but not against a concurrent `mv`.
    match fs_write::rename_durable_no_replace(old_path, new_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            refuse_existing_destination()
        }
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {
            // Older kernel or a filesystem without the flag: fall back to the
            // best-effort check, which still closes the in-app window.
            if fs_metadata::exists(new_path) {
                return refuse_existing_destination();
            }
            fs_write::rename_durable(old_path, new_path).map_err(RenameFailure::Io)
        }
        Err(error) => Err(RenameFailure::Io(error)),
    }
}

/// Atomically create a file or directory with a unique name.
fn create_unique(dir: &Path, base: &str, is_dir: bool) -> std::io::Result<PathBuf> {
    for attempt in 1..MAX_UNIQUE_NAME_ATTEMPTS {
        let path = dir.join(policy::unique_name_candidate(base, attempt));
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

/// Discard a temporary or failed inline placeholder, by **identity**, not by path.
///
/// Both the cancelled-new-item flow and the failed-rename recovery flow need to
/// remove a placeholder the workflow itself created. This used to be a plain
/// detached `std::thread::spawn` doing a path-only `remove_file_if_exists` with
/// no ordering against any other writer — so cancelling a `New File` placeholder
/// and then renaming a real file onto that same name could unlink **the user's
/// real file**. The repository already states the rule for the analogous draft
/// orphan-body case: record the candidate inode, acquire the stable write guard,
/// and recheck inode before deleting. Never delete by path alone.
///
/// The removal runs on the guarded worker pool so it is ordered against every
/// other in-app writer for the same target, and the `is_dir` branch stays
/// conservative: an empty-only `remove_dir_if_exists`, never a recursive delete.
fn spawn_temp_item_cleanup(path: PathBuf, is_dir: bool) {
    let expected_inode = fs_metadata::inode(&path).ok();
    gtk_lush_tasks::spawn_blocking_then(
        (),
        move || {
            #[cfg(feature = "test-utils")]
            crate::ui::sidebar::test_policy::delay_placeholder_cleanup();
            let guard = fs_write::TargetWriteGuard::acquire(&path);
            let Ok(_guard) = guard else {
                tracing::warn!(
                    "Skipping temporary-item cleanup for {}: write target could not be resolved",
                    path.display()
                );
                return None;
            };
            // Recheck identity under the guard: if the name now refers to a
            // different inode, another flow created or renamed something into
            // this path and it is not ours to delete.
            let current_inode = fs_metadata::inode(&path).ok();
            if expected_inode.is_none() || current_inode != expected_inode {
                tracing::debug!(
                    "Skipping temporary-item cleanup for {}: identity changed",
                    path.display()
                );
                return None;
            }
            let result = if is_dir {
                fs_mutate::remove_dir_if_exists(&path)
            } else {
                fs_mutate::remove_file_if_exists(&path)
            };
            Some((path, result.err()))
        },
        |(), reported| {
            if let Some((path, Some(error))) = reported {
                tracing::warn!(
                    "Failed to clean up temporary item {}: {}",
                    path.display(),
                    error
                );
            }
        },
    );
}

/// Remove the rename entry and restore the label.
pub(super) fn cancel_rename(entry: &gtk4::Entry, label: &gtk4::Label, content_box: &gtk4::Box) {
    if entry.parent().is_none() {
        return;
    }
    content_box.remove(entry);
    label.set_visible(true);
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Folder-tree loading, drill-down, and folder expansion helpers for one workspace section.
//!
//! This slice keeps the tree-model and drill-down orchestration together so the
//! public facade can stay focused on the widget API and callback surface.

use std::path::Path;

use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{gio, glib};

use crate::model::workspace::{FolderTreeEntry, WorkspaceFolder, WorkspaceFolderId};
use crate::services;
use crate::ui::accessibility;
use crate::ui::sidebar::file_tree_item::FileTreeItem;

use super::LushtextWorkspaceSection;

impl LushtextWorkspaceSection {
    /// Load workspace folder paths into the file tree.
    pub fn load_folders(&self, folders: &[FolderTreeEntry]) {
        *self.imp().original_folders.borrow_mut() = folders.to_vec();
        self.imp().workspace_folder_ids.borrow_mut().clear();
        self.imp().drilldown_stack.borrow_mut().clear();
        self.imp().drilldown_header_box.set_visible(false);
        self.load_folder_model(folders, false);
    }

    /// Load persisted workspace folders, preserving their stable folder ids.
    pub fn load_workspace_folders(&self, folders: &[WorkspaceFolder]) {
        let entries = folders
            .iter()
            .map(|folder| FolderTreeEntry::Directory {
                path: folder.path.clone(),
            })
            .collect::<Vec<_>>();
        *self.imp().workspace_folder_ids.borrow_mut() = folders
            .iter()
            .map(|folder| (folder.path.clone(), folder.id.clone()))
            .collect();
        *self.imp().original_folders.borrow_mut() = entries.clone();
        self.imp().drilldown_stack.borrow_mut().clear();
        self.imp().drilldown_header_box.set_visible(false);
        self.load_folder_model(&entries, false);
        self.sync_workspace_folder_reorder_handles();
        self.sync_file_row_states();
    }

    pub(super) fn load_folder_model(&self, folders: &[FolderTreeEntry], auto_expand: bool) {
        self.dismiss_peek_for_rebuild();
        self.save_expanded_paths();
        super::tree_loading::clear_all_dir_state(self);
        self.reset_item_cache();
        let top_level_store = gio::ListStore::new::<FileTreeItem>();
        for entry in folders {
            let folder_path = entry.path().to_path_buf();
            let is_dir = entry.is_dir();
            let item = self.file_item_for_folder_entry(&folder_path, is_dir);
            let index = top_level_store.n_items();
            top_level_store.append(&item);
            self.cache_top_level_item(folder_path.clone(), index as usize);
            if is_dir {
                schedule_folder_empty_check(self, &top_level_store, &item, folder_path, index);
            }
        }

        let section_weak = self.downgrade();
        // GTK4 trees are built from three pieces: ListStore holds observable row
        // data, TreeListModel flattens parent/child stores, and ListView/TreeExpander
        // render the flattened rows with indentation and arrows.
        let tree_model =
            gtk4::TreeListModel::new(top_level_store.clone(), false, false, move |item| {
                let section = section_weak.upgrade()?;
                let file_item = item.downcast_ref::<FileTreeItem>()?;
                if !file_item.is_dir() || file_item.is_empty() == Some(true) {
                    return None;
                }
                file_item.path().map(|path| {
                    super::tree_loading::build_children_model(&section, &path)
                        .upcast::<gio::ListModel>()
                })
            });

        let selection = gtk4::SingleSelection::new(Some(tree_model.clone()));
        self.install_peek_selection_model(&selection);
        let imp = self.imp();
        imp.file_tree_view.set_model(Some(&selection));
        *imp.top_level_store.borrow_mut() = Some(top_level_store);
        *imp.tree_model.borrow_mut() = Some(tree_model);
        self.sync_section_body_visibility();
        self.update_button_state();
        self.sync_workspace_folder_reorder_handles();
        self.restore_folder_model_state(auto_expand);
        self.sync_file_row_states();
        self.restart_workspace_watch();
    }

    /// Add a single folder path to an existing file tree.
    pub fn add_folder(&self, path: &Path, is_dir: bool) {
        self.add_folder_with_id(path, is_dir, None);
    }

    /// Add a workspace folder row with its persisted stable folder id.
    pub fn add_workspace_folder(&self, folder_id: &WorkspaceFolderId, path: &Path) {
        self.add_folder_with_id(path, true, Some(folder_id));
    }

    fn add_folder_with_id(&self, path: &Path, is_dir: bool, folder_id: Option<&WorkspaceFolderId>) {
        let new_entry = if is_dir {
            FolderTreeEntry::Directory {
                path: path.to_path_buf(),
            }
        } else {
            FolderTreeEntry::File {
                path: path.to_path_buf(),
            }
        };
        let has_store = !self.imp().original_folders.borrow().is_empty();
        if let Some(folder_id) = folder_id {
            self.imp()
                .workspace_folder_ids
                .borrow_mut()
                .insert(path.to_path_buf(), folder_id.clone());
        }
        if has_store {
            let already_exists = self
                .imp()
                .original_folders
                .borrow()
                .iter()
                .any(|entry| entry.path() == path);
            if !already_exists {
                self.imp().original_folders.borrow_mut().push(new_entry);
                if self.imp().drilldown_stack.borrow().is_empty() {
                    let store_ref = self.imp().top_level_store.borrow();
                    if let Some(top_level_store) = store_ref.as_ref() {
                        let item = self.file_item_for_folder_entry(path, is_dir);
                        let index = top_level_store.n_items();
                        top_level_store.append(&item);
                        self.cache_top_level_item(path.to_path_buf(), index as usize);
                        if is_dir {
                            schedule_folder_empty_check(
                                self,
                                top_level_store,
                                &item,
                                path.to_path_buf(),
                                index,
                            );
                        }
                    }
                }
            }
        } else {
            *self.imp().original_folders.borrow_mut() = vec![new_entry.clone()];
            self.load_folder_model(&[new_entry], false);
        }
        self.update_button_state();
        self.sync_workspace_folder_reorder_handles();
        self.sync_file_row_states();
        self.restart_workspace_watch();
    }

    /// Remove one persisted workspace folder from the top-level section model.
    pub fn remove_workspace_folder(&self, folder_id: &WorkspaceFolderId, path: &Path) {
        self.imp()
            .workspace_folder_ids
            .borrow_mut()
            .retain(|folder_path, cached_folder_id| {
                cached_folder_id != folder_id && folder_path.as_path() != path
            });
        self.imp()
            .original_folders
            .borrow_mut()
            .retain(|entry| entry.path() != path);

        if self.imp().drilldown_stack.borrow().is_empty() {
            let _ = self.remove_from_model(path);
            self.sync_section_body_visibility();
        }

        self.update_button_state();
        self.sync_workspace_folder_reorder_handles();
        self.sync_file_row_states();
        self.restart_workspace_watch();
    }

    /// Return whether one configured folder can move earlier or later in this section.
    pub(super) fn workspace_folder_move_availability(
        &self,
        folder_id: &WorkspaceFolderId,
    ) -> (bool, bool) {
        let folders = self.imp().original_folders.borrow();
        let folder_ids = self.imp().workspace_folder_ids.borrow();
        let Some(index) = folders.iter().position(|entry| {
            folder_ids
                .get(entry.path())
                .is_some_and(|candidate_id| candidate_id == folder_id)
        }) else {
            return (false, false);
        };

        (index > 0, index + 1 < folders.len())
    }

    /// Returns true if this section has at least one folder loaded.
    #[must_use]
    pub fn has_folders(&self) -> bool {
        !self.imp().original_folders.borrow().is_empty()
    }

    fn file_item_for_folder_entry(&self, path: &Path, is_dir: bool) -> FileTreeItem {
        if is_dir
            && self.imp().drilldown_stack.borrow().is_empty()
            && let Some(folder_id) = self.imp().workspace_folder_ids.borrow().get(path).cloned()
        {
            FileTreeItem::new_workspace_folder(path.to_path_buf(), folder_id, None)
        } else {
            FileTreeItem::new(path.to_path_buf(), is_dir, None)
        }
    }

    /// Focus the workspace panel on a specific deep directory.
    pub fn focus_folder(&self, dir_path: &Path) {
        self.imp()
            .drilldown_stack
            .borrow_mut()
            .push(dir_path.to_path_buf());

        let path_str = dir_path.to_string_lossy();
        self.imp().drilldown_path_label.set_label(&path_str);
        self.imp()
            .drilldown_path_label
            .set_tooltip_text(Some(&path_str));

        self.load_folder_model(
            &[FolderTreeEntry::Directory {
                path: dir_path.to_path_buf(),
            }],
            true,
        );
        self.notify_folder_focused();
    }

    /// Navigate one level up the drill-down stack.
    pub fn navigate_back(&self) {
        let mut stack = self.imp().drilldown_stack.borrow_mut();
        let popped_path = stack.pop();
        if let Some(parent_path) = stack.last().cloned() {
            let path_str = parent_path.to_string_lossy();
            self.imp().drilldown_path_label.set_label(&path_str);
            self.imp()
                .drilldown_path_label
                .set_tooltip_text(Some(&path_str));
            if let Some(target) = popped_path {
                *self.imp().pending_selection.borrow_mut() = Some(target);
            }
            drop(stack);
            self.load_folder_model(&[FolderTreeEntry::Directory { path: parent_path }], true);
        } else {
            if let Some(target) = popped_path {
                *self.imp().pending_selection.borrow_mut() = Some(target);
            }
            drop(stack);
            let original = self.imp().original_folders.borrow().clone();
            self.load_folder_model(&original, true);
        }
    }

    /// Return whether the workspace header has hidden this section's folder body.
    #[must_use]
    pub fn is_section_body_collapsed(&self) -> bool {
        self.imp().section_body_collapsed.get()
    }

    /// Hide or reveal the section body without changing folder-row expansion state.
    pub fn set_section_body_collapsed(&self, collapsed: bool) {
        if self.imp().section_body_collapsed.replace(collapsed) == collapsed {
            self.sync_section_body_visibility();
            return;
        }
        if collapsed {
            self.dismiss_peek_for_rebuild();
        }
        self.sync_section_body_visibility();
    }

    /// Toggle the workspace-level body collapse state from the header affordance.
    pub fn toggle_section_body_collapsed(&self) {
        self.set_section_body_collapsed(!self.is_section_body_collapsed());
    }

    fn sync_section_body_visibility(&self) {
        let imp = self.imp();
        let collapsed = imp.section_body_collapsed.get();
        let has_tree_rows = imp
            .top_level_store
            .borrow()
            .as_ref()
            .is_some_and(|store| store.n_items() > 0);
        let in_drilldown = !imp.drilldown_stack.borrow().is_empty();
        let show_body = !collapsed;

        imp.drilldown_header_box
            .set_visible(show_body && in_drilldown);
        imp.inner_scrolled_window
            .set_visible(show_body && has_tree_rows);
        imp.empty_folder_set_label
            .set_visible(show_body && !has_tree_rows && !in_drilldown);
        imp.collapse_button.set_icon_name(if collapsed {
            "pan-end-symbolic"
        } else {
            "pan-down-symbolic"
        });
        let (label, description) = if collapsed {
            ("Expand Workspace", "Show this workspace's folder list")
        } else {
            ("Collapse Workspace", "Hide this workspace's folder list")
        };
        imp.collapse_button.set_tooltip_text(Some(label));
        imp.collapse_button.update_property(&[
            gtk4::accessible::Property::Label(label),
            gtk4::accessible::Property::Description(description),
        ]);
        accessibility::set_expanded(&*imp.collapse_button, Some(!collapsed));
        accessibility::set_hidden(&*imp.file_tree_view, !show_body || !has_tree_rows);
        accessibility::set_hidden(
            &*imp.empty_folder_set_label,
            !show_body || has_tree_rows || in_drilldown,
        );
        let tree_value_text = if collapsed {
            "Workspace folder tree collapsed"
        } else if in_drilldown {
            "Focused folder view"
        } else if !has_tree_rows {
            "No folders in this workspace"
        } else {
            "Workspace folder tree"
        };
        accessibility::set_value_text(&*imp.file_tree_view, tree_value_text);
        self.sync_workspace_folder_reorder_handles();
        self.sync_file_row_states();
    }

    /// Collapses the top-level folders of this workspace section.
    pub fn collapse_folders(&self) {
        for row in self.expanded_folder_rows() {
            row.set_expanded(false);
        }
        self.sync_file_row_states();
        self.restart_workspace_watch();
    }

    /// Expands the top-level folders of this workspace section if they are not confirmed empty.
    pub fn expand_folders(&self) {
        for row in self.collapsed_non_empty_folder_rows() {
            row.set_expanded(true);
        }
        self.sync_file_row_states();
        self.restart_workspace_watch();
    }

    /// Toggle the expansion state of the top-level folders as one group.
    pub fn toggle_folders(&self) {
        let rows = self.toggleable_folder_rows();
        let any_collapsed = rows.iter().any(|row| !row.is_expanded());
        for row in rows {
            row.set_expanded(any_collapsed);
        }
        self.sync_file_row_states();
        self.restart_workspace_watch();
    }

    /// Select and scroll to a path after its row exists in the flattened tree model.
    pub(super) fn select_and_scroll_to(&self, target_path: &Path) {
        if let Some(tree_model) = self.imp().tree_model.borrow().as_ref() {
            for i in 0..tree_model.n_items() {
                if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                    && let Some(item) = row.item().and_downcast::<FileTreeItem>()
                    && item.path().as_deref() == Some(target_path)
                {
                    if let Some(selection) = self
                        .imp()
                        .file_tree_view
                        .model()
                        .and_downcast::<gtk4::SingleSelection>()
                    {
                        selection.set_selected(i);
                        self.imp()
                            .file_tree_view
                            .scroll_to(i, gtk4::ListScrollFlags::FOCUS, None);
                    }
                    self.imp().pending_selection.borrow_mut().take();
                    return;
                }
            }
        }
    }

    /// Keep manual refresh reachable even when there are no folders to reload.
    fn update_button_state(&self) {
        self.imp().refresh_button.set_sensitive(true);
    }

    fn folder_rows(&self) -> Vec<gtk4::TreeListRow> {
        let Some(tree_model) = self.imp().tree_model.borrow().as_ref().cloned() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for i in 0..tree_model.n_items() {
            if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                && row.depth() == 0
            {
                rows.push(row);
            }
        }
        rows
    }

    fn expanded_folder_rows(&self) -> Vec<gtk4::TreeListRow> {
        self.folder_rows()
            .into_iter()
            .filter(gtk4::TreeListRow::is_expanded)
            .collect()
    }

    fn collapsed_non_empty_folder_rows(&self) -> Vec<gtk4::TreeListRow> {
        self.folder_rows()
            .into_iter()
            .filter(|row| !row.is_expanded())
            .filter(|row| {
                row.item()
                    .and_downcast::<FileTreeItem>()
                    .is_some_and(|item| item.is_empty() != Some(true))
            })
            .collect()
    }

    fn toggleable_folder_rows(&self) -> Vec<gtk4::TreeListRow> {
        self.folder_rows()
            .into_iter()
            .filter(|row| {
                row.item()
                    .and_downcast::<FileTreeItem>()
                    .is_some_and(|item| {
                        item.is_dir() && !item.is_placeholder() && item.is_empty() != Some(true)
                    })
            })
            .collect()
    }

    fn restore_folder_model_state(&self, auto_expand: bool) {
        let section_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(section) = section_weak.upgrade() else {
                return;
            };

            for row in section.folder_rows() {
                let should_expand = auto_expand
                    || row
                        .item()
                        .and_downcast::<FileTreeItem>()
                        .and_then(|item| item.path())
                        .is_some_and(|path| section.imp().expanded_paths.borrow().contains(&path));
                if should_expand {
                    row.set_expanded(true);
                }
            }

            let pending_selection = section.imp().pending_selection.borrow().clone();
            if let Some(target_path) = pending_selection {
                section.select_and_scroll_to(&target_path);
            }
        });
    }
}

pub(super) fn schedule_folder_empty_check(
    section: &LushtextWorkspaceSection,
    top_level_store: &gio::ListStore,
    item: &FileTreeItem,
    folder_path: std::path::PathBuf,
    initial_index: u32,
) {
    let path_for_check = folder_path.clone();
    let section_weak = section.downgrade();
    gtk_lush_tasks::spawn_blocking_then(
        (top_level_store.clone(), item.clone(), folder_path),
        move || services::file_tree::is_dir_empty(&path_for_check),
        move |(top_level_store, item, folder_path), is_empty| {
            let Some(section) = section_weak.upgrade() else {
                return;
            };
            if !section.imp().drilldown_stack.borrow().is_empty()
                || item.path().as_deref() != Some(folder_path.as_path())
            {
                return;
            }

            let current_index = if top_level_store
                .item(initial_index)
                .and_downcast::<FileTreeItem>()
                .is_some_and(|current| {
                    current.path().as_deref() == Some(folder_path.as_path())
                        && current.workspace_folder_id() == item.workspace_folder_id()
                }) {
                Some(initial_index)
            } else {
                (0..top_level_store.n_items()).find(|candidate_index| {
                    top_level_store
                        .item(*candidate_index)
                        .and_downcast::<FileTreeItem>()
                        .is_some_and(|current| {
                            current.path().as_deref() == Some(folder_path.as_path())
                                && current.workspace_folder_id() == item.workspace_folder_id()
                        })
                })
            };

            let Some(current_index) = current_index else {
                return;
            };

            item.set_is_empty(Some(is_empty));
            top_level_store.splice(current_index, 1, &[item]);
        },
    );
}

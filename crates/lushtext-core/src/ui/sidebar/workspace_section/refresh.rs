// SPDX-License-Identifier: GPL-3.0-or-later

//! Refresh orchestration for one workspace section.
//!
//! Manual button clicks and automatic watcher updates both funnel through this
//! module so subtree reloads, whole-section rebuilds, and state restoration
//! stay consistent no matter what triggered the refresh.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::workspace::{FolderTreeEntry, WorkspaceFolderId};
use crate::services::notifications::NotificationSeverity;

use super::super::file_tree_item::FileTreeItem;
use super::LushtextWorkspaceSection;

/// Short debounce for automatic refresh bursts after the watcher already
/// coalesced backend events. This keeps rapid save/rename/remove sequences from
/// rebuilding the same subtree multiple times on the GTK thread.
const AUTO_REFRESH_DEBOUNCE_MS: u64 = 120;

/// Manual refreshes should feel immediate, but keeping a tiny timeout means the
/// button still participates in the same generation-guarded pipeline.
const MANUAL_REFRESH_DEBOUNCE_MS: u64 = 1;

enum RefreshPlan {
    Full,
    Directories(Vec<PathBuf>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FolderRowState {
    path: PathBuf,
    is_dir: bool,
    is_empty: Option<bool>,
    workspace_folder_id: Option<WorkspaceFolderId>,
}

impl LushtextWorkspaceSection {
    /// Queue a whole-section refresh from the header button.
    pub(super) fn request_manual_refresh(&self) {
        self.clear_refresh_error();
        self.imp()
            .refresh_runtime
            .manual_refresh_announcing
            .set(true);
        self.emit_message("Refreshing workspace folders", NotificationSeverity::Info);
        self.schedule_refresh(true, Vec::new());
    }

    /// Test helper for driving the automatic refresh path without filesystem events.
    #[cfg(feature = "test-utils")]
    pub fn queue_auto_refresh_for_test(&self, changed_paths: Vec<PathBuf>) {
        self.queue_auto_refresh(changed_paths);
    }

    /// Queue an automatic refresh from the filesystem watcher.
    pub(super) fn queue_auto_refresh(&self, changed_paths: Vec<PathBuf>) {
        if changed_paths.is_empty() {
            return;
        }
        self.schedule_refresh(false, changed_paths);
    }

    fn schedule_refresh(&self, full_reload: bool, changed_paths: Vec<PathBuf>) {
        let runtime = &self.imp().refresh_runtime;
        if full_reload {
            runtime.pending_full_reload.set(true);
        }
        runtime.pending_paths.borrow_mut().extend(changed_paths);

        let delay = if full_reload {
            MANUAL_REFRESH_DEBOUNCE_MS
        } else {
            AUTO_REFRESH_DEBOUNCE_MS
        };
        runtime
            .debounce
            .schedule(self, Duration::from_millis(delay), move |section, _| {
                section.apply_queued_refresh();
            });
    }

    fn apply_queued_refresh(&self) {
        let pending_paths = self.take_pending_refresh_paths();
        let pending_full = self
            .imp()
            .refresh_runtime
            .pending_full_reload
            .replace(false);
        if !pending_full && pending_paths.is_empty() {
            return;
        }

        self.snapshot_refresh_state();

        if pending_full {
            self.refresh_materialized_view();
            return;
        }

        match self.plan_refresh(&pending_paths) {
            RefreshPlan::Full => self.reload_current_view(),
            RefreshPlan::Directories(directories) => {
                for dir_path in directories {
                    self.reload_directory(&dir_path);
                }
            }
        }
    }

    /// Surface one recoverable scan failure without repeating the same warning.
    pub(super) fn report_refresh_error(&self, message: &str) {
        let mut last_error = self.imp().refresh_runtime.last_reported_error.borrow_mut();
        if last_error.as_deref() == Some(message) {
            return;
        }
        *last_error = Some(message.to_string());
        drop(last_error);
        self.sync_file_tree_error_state();
        self.emit_message(message, NotificationSeverity::Warning);
    }

    fn clear_refresh_error(&self) {
        self.imp()
            .refresh_runtime
            .last_reported_error
            .borrow_mut()
            .take();
        self.sync_file_tree_error_state();
    }

    fn take_pending_refresh_paths(&self) -> HashSet<PathBuf> {
        self.imp().refresh_runtime.pending_paths.take()
    }

    fn snapshot_refresh_state(&self) {
        self.save_expanded_paths();
        *self.imp().pending_selection.borrow_mut() = self.selected_tree_path();
    }

    fn selected_tree_path(&self) -> Option<PathBuf> {
        self.imp()
            .file_tree_view
            .model()
            .and_downcast::<gtk4::SingleSelection>()
            .and_then(|selection| selection.selected_item())
            .and_then(|row| row.downcast::<gtk4::TreeListRow>().ok())
            .and_then(|row| row.item())
            .and_then(|item| {
                item.downcast::<super::super::file_tree_item::FileTreeItem>()
                    .ok()
            })
            .and_then(|item| item.path())
    }

    fn reload_current_view(&self) {
        let folders = self.current_visible_folders();
        let auto_expand = !self.imp().drilldown_stack.borrow().is_empty();
        self.load_folder_model(&folders, auto_expand);
    }

    fn refresh_materialized_view(&self) {
        if !self.reconcile_top_level_store_in_place() {
            self.reload_current_view();
            return;
        }

        let mut expanded_paths = self
            .imp()
            .expanded_paths
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        expanded_paths.sort_by_key(|path| path.components().count());

        for dir_path in expanded_paths {
            if self.dir_has_expanded_store(&dir_path) {
                self.refresh_loaded_directory(&dir_path);
            }
        }
    }

    fn reload_directory(&self, dir_path: &Path) {
        if !self.refresh_loaded_directory(dir_path) {
            self.reload_current_view();
        }
    }

    fn refresh_loaded_directory(&self, dir_path: &Path) -> bool {
        let stores = self.expanded_stores_for_dir(dir_path);
        if stores.is_empty() {
            return false;
        }

        super::tree_loading::clear_dir_state(self, dir_path);
        for store in stores {
            super::tree_loading::populate_child_store(self, dir_path, &store);
        }
        true
    }

    fn plan_refresh(&self, changed_paths: &HashSet<PathBuf>) -> RefreshPlan {
        let current_folder_paths: HashSet<PathBuf> = self
            .current_visible_folders()
            .into_iter()
            .map(|entry| entry.path().to_path_buf())
            .collect();

        let mut directories = Vec::new();
        for changed_path in changed_paths {
            let Some(dir_path) =
                self.refresh_directory_for_path(changed_path, &current_folder_paths)
            else {
                return RefreshPlan::Full;
            };
            directories.push(dir_path);
        }

        RefreshPlan::Directories(minimize_refresh_directories(directories))
    }

    fn refresh_directory_for_path(
        &self,
        changed_path: &Path,
        current_folder_paths: &HashSet<PathBuf>,
    ) -> Option<PathBuf> {
        let mut candidate = Some(changed_path);
        while let Some(path) = candidate {
            let is_workspace_folder = current_folder_paths.contains(path);
            if is_workspace_folder && path == changed_path {
                return None;
            }
            if self.dir_has_expanded_store(path) {
                return Some(path.to_path_buf());
            }
            if is_workspace_folder {
                return None;
            }
            candidate = path.parent();
        }
        None
    }

    fn dir_has_expanded_store(&self, dir_path: &Path) -> bool {
        !self.expanded_stores_for_dir(dir_path).is_empty()
    }

    fn expanded_stores_for_dir(&self, dir_path: &Path) -> Vec<gtk4::gio::ListStore> {
        let Some(tree_model) = self.imp().tree_model.borrow().as_ref().cloned() else {
            return Vec::new();
        };

        let mut stores = Vec::new();
        for index in 0..tree_model.n_items() {
            let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>() else {
                continue;
            };
            if !row.is_expanded() {
                continue;
            }
            let Some(item) = row.item().and_downcast::<FileTreeItem>() else {
                continue;
            };
            if !item.is_dir() || item.path().as_deref() != Some(dir_path) {
                continue;
            }
            let Some(store) = row
                .children()
                .and_then(|children| children.downcast::<gtk4::gio::ListStore>().ok())
            else {
                continue;
            };
            stores.push(store);
        }
        stores
    }

    pub(super) fn current_visible_folders(&self) -> Vec<FolderTreeEntry> {
        self.imp()
            .drilldown_stack
            .borrow()
            .last()
            .cloned()
            .map_or_else(
                || self.imp().original_folders.borrow().clone(),
                |path| vec![FolderTreeEntry::Directory { path }],
            )
    }

    fn reconcile_top_level_store_in_place(&self) -> bool {
        let Some(top_level_store) = self.imp().top_level_store.borrow().as_ref().cloned() else {
            return false;
        };

        let current_folders = snapshot_folder_rows(&top_level_store);
        let desired_folders = desired_folder_rows(
            &self.current_visible_folders(),
            &current_folders,
            &self.imp().workspace_folder_ids.borrow(),
            self.imp().drilldown_stack.borrow().is_empty(),
        );
        if current_folders != desired_folders {
            let removed_paths = current_folders
                .iter()
                .filter(|current| {
                    !desired_folders
                        .iter()
                        .any(|desired| desired.path == current.path)
                })
                .map(|row| row.path.clone())
                .collect::<Vec<_>>();

            for path in removed_paths {
                super::tree_loading::clear_dir_state(self, &path);
            }

            let prefix = common_folder_prefix_len(&current_folders, &desired_folders);
            let suffix =
                common_folder_suffix_len(&current_folders[prefix..], &desired_folders[prefix..]);
            let removed = current_folders.len().saturating_sub(prefix + suffix);
            let replacement = build_folder_items(
                &desired_folders[prefix..desired_folders.len().saturating_sub(suffix)],
            );
            #[expect(
                clippy::cast_possible_truncation,
                reason = "The visible folder-row store is bounded to realistic workspace sizes before converting to u32"
            )]
            top_level_store.splice(prefix as u32, removed as u32, &replacement);
            self.recache_top_level_store(&top_level_store);
            self.restore_materialized_state();
        }

        self.schedule_top_level_folder_empty_checks(&top_level_store);
        self.sync_workspace_folder_reorder_handles();
        self.sync_file_row_states();
        true
    }

    fn schedule_top_level_folder_empty_checks(&self, top_level_store: &gtk4::gio::ListStore) {
        if !self.imp().drilldown_stack.borrow().is_empty() {
            return;
        }

        for index in 0..top_level_store.n_items() {
            if let Some(item) = top_level_store.item(index).and_downcast::<FileTreeItem>()
                && item.is_dir()
                && let Some(folder_path) = item.path()
            {
                if self
                    .find_dir_row(&folder_path)
                    .is_some_and(|row| row.is_expanded())
                {
                    continue;
                }
                super::folders::schedule_folder_empty_check(
                    self,
                    top_level_store,
                    &item,
                    folder_path,
                    index,
                );
            }
        }
    }

    fn restore_materialized_state(&self) {
        let expanded_paths = self.imp().expanded_paths.borrow().clone();
        let pending_selection = self.imp().pending_selection.borrow().clone();
        if expanded_paths.is_empty() && pending_selection.is_none() {
            return;
        }

        let section_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(1), move || {
            let Some(section) = section_weak.upgrade() else {
                return;
            };
            for path in expanded_paths {
                if let Some(row) = section.find_dir_row(&path) {
                    row.set_expanded(true);
                }
            }
            if let Some(path) = pending_selection {
                section.select_and_scroll_to(&path);
            }
        });
    }
}

fn minimize_refresh_directories(mut directories: Vec<PathBuf>) -> Vec<PathBuf> {
    directories.sort_by_key(|path| path.components().count());

    let mut unique = Vec::new();
    for dir in directories {
        if unique
            .iter()
            .any(|existing: &PathBuf| dir.starts_with(existing))
        {
            continue;
        }
        unique.push(dir);
    }
    unique
}

fn snapshot_folder_rows(store: &gtk4::gio::ListStore) -> Vec<FolderRowState> {
    let mut rows = Vec::with_capacity(store.n_items() as usize);
    for index in 0..store.n_items() {
        if let Some(item) = store.item(index).and_then(|obj| {
            obj.downcast::<crate::ui::sidebar::file_tree_item::FileTreeItem>()
                .ok()
        }) && let Some(path) = item.path()
        {
            rows.push(FolderRowState {
                path,
                is_dir: item.is_dir(),
                is_empty: item.is_empty(),
                workspace_folder_id: item.workspace_folder_id(),
            });
        }
    }
    rows
}

fn desired_folder_rows(
    folders: &[FolderTreeEntry],
    current_rows: &[FolderRowState],
    workspace_folder_ids: &HashMap<PathBuf, WorkspaceFolderId>,
    include_workspace_folder_ids: bool,
) -> Vec<FolderRowState> {
    folders
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let workspace_folder_id = if include_workspace_folder_ids && entry.is_dir() {
                workspace_folder_ids.get(entry.path()).cloned()
            } else {
                None
            };
            FolderRowState {
                path: entry.path().to_path_buf(),
                is_dir: entry.is_dir(),
                is_empty: if let Some(current) = current_rows.get(index)
                    && current.path == entry.path()
                    && current.is_dir == entry.is_dir()
                {
                    current.is_empty
                } else {
                    None
                },
                workspace_folder_id,
            }
        })
        .collect()
}

fn build_folder_items(rows: &[FolderRowState]) -> Vec<FileTreeItem> {
    rows.iter()
        .map(|row| {
            if row.is_dir
                && let Some(folder_id) = row.workspace_folder_id.as_ref()
            {
                FileTreeItem::new_workspace_folder(
                    row.path.clone(),
                    folder_id.clone(),
                    row.is_empty,
                )
            } else {
                FileTreeItem::new(row.path.clone(), row.is_dir, row.is_empty)
            }
        })
        .collect()
}

fn common_folder_prefix_len(current: &[FolderRowState], desired: &[FolderRowState]) -> usize {
    current
        .iter()
        .zip(desired.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn common_folder_suffix_len(current: &[FolderRowState], desired: &[FolderRowState]) -> usize {
    current
        .iter()
        .rev()
        .zip(desired.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Refresh orchestration for one workspace section.
//!
//! Manual button clicks and automatic watcher updates both funnel through this
//! module so subtree reloads, whole-section rebuilds, and state restoration
//! stay consistent no matter what triggered the refresh.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::workspace::WorkspaceEntry;
use crate::services::notifications::NotificationSeverity;

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

impl LushtextWorkspaceSection {
    /// Queue a whole-section refresh from the header button.
    pub(super) fn request_manual_refresh(&self) {
        if !self.has_roots() {
            self.emit_message(
                "Add a folder to this workspace before refreshing.",
                NotificationSeverity::Warning,
            );
            return;
        }
        self.schedule_refresh(true, Vec::new());
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

        let generation = runtime.generation.get().wrapping_add(1);
        runtime.generation.set(generation);

        let delay = if full_reload {
            MANUAL_REFRESH_DEBOUNCE_MS
        } else {
            AUTO_REFRESH_DEBOUNCE_MS
        };
        let section_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            let Some(section) = section_weak.upgrade() else {
                return;
            };
            if section.imp().refresh_runtime.generation.get() != generation {
                return;
            }
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
            self.reload_current_view();
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

    fn take_pending_refresh_paths(&self) -> HashSet<PathBuf> {
        self.imp().refresh_runtime.pending_paths.take()
    }

    fn snapshot_refresh_state(&self) {
        self.save_expanded_paths();
        *self.imp().pending_selection.borrow_mut() = self.selected_tree_path();
        self.prune_invalid_drilldown_stack();
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
            .filter(|path| path.exists())
    }

    fn reload_current_view(&self) {
        let roots = self.current_visible_roots();
        let auto_expand = !self.imp().drilldown_stack.borrow().is_empty();
        self.load_root_model(&roots, auto_expand);
    }

    fn reload_directory(&self, dir_path: &Path) {
        let Some(row) = self.find_dir_row(dir_path) else {
            self.reload_current_view();
            return;
        };
        if !row.is_expanded() {
            self.reload_current_view();
            return;
        }
        let Some(store) = self.find_store_for_dir(dir_path) else {
            self.reload_current_view();
            return;
        };

        super::tree_loading::clear_dir_state(self, dir_path);
        super::tree_loading::populate_child_store(self, dir_path, &store);
    }

    fn plan_refresh(&self, changed_paths: &HashSet<PathBuf>) -> RefreshPlan {
        let current_root_paths: Vec<PathBuf> = self
            .current_visible_roots()
            .into_iter()
            .map(|entry| entry.path().to_path_buf())
            .collect();

        let mut directories = Vec::new();
        for changed_path in changed_paths {
            let Some(dir_path) = self.refresh_directory_for_path(changed_path, &current_root_paths)
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
        current_root_paths: &[PathBuf],
    ) -> Option<PathBuf> {
        let mut candidate = Some(changed_path);
        while let Some(path) = candidate {
            let is_root = current_root_paths.iter().any(|root| root == path);
            if is_root && path == changed_path {
                return None;
            }
            if self.find_dir_row(path).is_some_and(|row| row.is_expanded())
                && self.find_store_for_dir(path).is_some()
            {
                return Some(path.to_path_buf());
            }
            if is_root {
                return None;
            }
            candidate = path.parent();
        }
        None
    }

    pub(super) fn current_visible_roots(&self) -> Vec<WorkspaceEntry> {
        self.imp()
            .drilldown_stack
            .borrow()
            .last()
            .cloned()
            .map_or_else(
                || self.imp().original_roots.borrow().clone(),
                |path| vec![WorkspaceEntry::Directory { path }],
            )
    }

    fn prune_invalid_drilldown_stack(&self) {
        let mut stack = self.imp().drilldown_stack.borrow_mut();
        while stack.last().is_some_and(|path| !path.exists()) {
            stack.pop();
        }

        if let Some(current) = stack.last() {
            let path_str = current.to_string_lossy();
            self.imp().drilldown_header_box.set_visible(true);
            self.imp().drilldown_path_label.set_label(&path_str);
            self.imp()
                .drilldown_path_label
                .set_tooltip_text(Some(&path_str));
        } else {
            self.imp().drilldown_header_box.set_visible(false);
        }
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

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
use crate::services::workspace_watch::WORKSPACE_WATCH_PATH_CAP;

use super::super::file_tree_item::FileTreeItem;
use super::LushtextWorkspaceSection;
#[cfg(feature = "test-utils")]
use super::WorkspaceScanPressureEvidence;

/// Short debounce for automatic refresh bursts after the watcher already
/// coalesced backend events. This keeps rapid save/rename/remove sequences from
/// rebuilding the same subtree multiple times on the GTK thread.
const AUTO_REFRESH_DEBOUNCE_MS: u64 = 120;

/// Manual refreshes should feel immediate, but keeping a tiny timeout means the
/// button still participates in the same generation-guarded pipeline.
const MANUAL_REFRESH_DEBOUNCE_MS: u64 = 1;
/// Expanded stores submitted per GTK turn before yielding to rendering/input.
const REFRESH_SCAN_DISPATCH_BATCH: usize = 32;

enum RefreshPlan {
    Full,
    Directories(Vec<RefreshDirectory>),
}

struct RefreshDirectory {
    path: PathBuf,
    stores: Vec<gtk4::gio::ListStore>,
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
        if !self.has_folders() {
            return;
        }
        self.imp()
            .refresh_runtime
            .manual_refresh_announcing
            .set(true);
        self.emit_message("Refreshing workspace folders", NotificationSeverity::Info);
        self.schedule_refresh(true, Vec::new(), true);
    }

    /// Test helper for driving the automatic refresh path without filesystem events.
    #[cfg(feature = "test-utils")]
    pub fn queue_auto_refresh_for_test(&self, changed_paths: Vec<PathBuf>) {
        self.queue_auto_refresh(changed_paths);
    }

    /// Test helper for driving mailbox overflow without filesystem events.
    #[cfg(feature = "test-utils")]
    pub fn queue_auto_full_refresh_for_test(&self) {
        self.queue_auto_full_refresh();
    }

    /// Queue an automatic refresh from the filesystem watcher.
    pub(super) fn queue_auto_refresh(&self, changed_paths: Vec<PathBuf>) {
        if changed_paths.is_empty() {
            return;
        }
        self.schedule_refresh(false, changed_paths, false);
    }

    /// Queue a conservative automatic refresh after mailbox overflow.
    pub(super) fn queue_auto_full_refresh(&self) {
        self.schedule_refresh(true, Vec::new(), false);
    }

    fn schedule_refresh(&self, full_reload: bool, changed_paths: Vec<PathBuf>, manual: bool) {
        let runtime = &self.imp().refresh_runtime;
        if full_reload {
            runtime.pending_paths.borrow_mut().clear();
            if runtime.pending_full_reload.replace(true) && !manual {
                return;
            }
        } else if runtime.pending_full_reload.get() {
            return;
        } else {
            let mut pending_paths = runtime.pending_paths.borrow_mut();
            for path in changed_paths {
                pending_paths.insert(path);
                if pending_paths.len() > WORKSPACE_WATCH_PATH_CAP {
                    pending_paths.clear();
                    runtime.pending_full_reload.set(true);
                    break;
                }
            }
        }

        let delay = if manual {
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

    /// Scalar pressure state for deterministic widget assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn refresh_pressure_for_test(&self) -> (usize, bool) {
        let runtime = &self.imp().refresh_runtime;
        (
            runtime.pending_paths.borrow().len(),
            runtime.pending_full_reload.get(),
        )
    }

    /// Scalar bounded-reconciliation evidence for widget tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn reconciliation_metrics_for_test(&self) -> (u64, usize, u64, u64, usize) {
        let refresh = &self.imp().refresh_runtime;
        (
            refresh.reconcile_batch_count.get(),
            refresh.reconcile_max_batch_rows.get(),
            refresh.reconcile_terminal_count.get(),
            refresh.reconcile_superseded_count.get(),
            self.imp().child_reconcile_sources.borrow().len(),
        )
    }

    /// Scalar terminal child-cache work evidence for linear-scale assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn child_cache_rebuild_metrics_for_test(&self) -> (usize, usize) {
        let refresh = &self.imp().refresh_runtime;
        (
            refresh.cache_rebuild_input_rows.get(),
            refresh.cache_rebuild_operations.get(),
        )
    }

    /// Apply already-queued pressure immediately for deterministic supersession tests.
    #[cfg(feature = "test-utils")]
    pub fn apply_queued_refresh_for_test(&self) {
        self.apply_queued_refresh();
    }

    /// Set the section-local batch cadence used by lifecycle-sensitive tests.
    #[cfg(feature = "test-utils")]
    pub fn set_reconciliation_batch_delay_for_test(&self, delay: Duration) {
        self.imp()
            .refresh_runtime
            .test_reconcile_batch_delay
            .set(delay);
    }

    /// Set a worker-side delay before directory and emptiness traversal.
    #[cfg(feature = "test-utils")]
    pub fn set_child_scan_delay_for_test(&self, delay: Duration) {
        self.imp().refresh_runtime.test_scan_delay.set(delay);
    }

    /// Return direct active/latest ownership and terminal-publication evidence.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn child_scan_pressure_for_test(&self) -> WorkspaceScanPressureEvidence {
        let refresh = &self.imp().refresh_runtime;
        WorkspaceScanPressureEvidence {
            active_scans: self.imp().child_active_scans.borrow().len(),
            pending_scans: self.imp().child_pending_scans.borrow().len(),
            admission_waiting_scans: self.imp().child_admission_scans.borrow().len()
                + self.imp().folder_empty_admission.borrow().len(),
            aggregate_active_tasks: super::tree_loading::workspace_scan_active_tasks_for_test(),
            aggregate_task_limit: super::tree_loading::WORKSPACE_SCAN_TASK_LIMIT,
            aggregate_task_high_water: super::tree_loading::workspace_scan_task_high_water_for_test(
            ),
            dispatch_queue: refresh.scan_dispatch_queue.borrow().len(),
            dispatch_queue_high_water: refresh.scan_dispatch_queue_high_water.get(),
            dispatch_batch_high_water: refresh.scan_dispatch_batch_high_water.get(),
            active_per_store_high_water: refresh.scan_active_per_store_high_water.get(),
            pending_per_store_high_water: refresh.scan_pending_per_store_high_water.get(),
            weak_pending_high_water: refresh.scan_weak_pending_high_water.get(),
            mirror_captures: refresh.scan_mirror_captures.get(),
            cancellation_requests: refresh.scan_cancellation_requests.get(),
            cancelled_terminals: refresh.scan_cancelled_terminals.get(),
            stale_completions: refresh.scan_stale_completions.get(),
            terminal_publications: refresh.scan_terminal_publications.get(),
            active_empty_probes: self.imp().folder_empty_active.borrow().len(),
            pending_empty_probes: self.imp().folder_empty_pending.borrow().len(),
            empty_probe_stale_rejections: refresh.empty_probe_stale_rejections.get(),
            empty_probe_terminal_publications: refresh.empty_probe_terminal_publications.get(),
        }
    }

    /// Number of top-level emptiness reads completed inside test workers.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn empty_probe_reads_for_test(&self) -> u64 {
        self.imp()
            .refresh_runtime
            .test_empty_probe_reads
            .load(std::sync::atomic::Ordering::Acquire)
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
                self.queue_refresh_directories(directories);
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
        self.imp().refresh_runtime.reconcile_batch_count.set(0);
        self.imp().refresh_runtime.reconcile_max_batch_rows.set(0);
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

        let mut expanded = self.expanded_store_index().into_iter().collect::<Vec<_>>();
        expanded.sort_by_key(|(path, _)| path.components().count());
        self.queue_refresh_directories(
            expanded
                .into_iter()
                .map(|(path, stores)| RefreshDirectory { path, stores })
                .collect(),
        );
    }

    fn queue_refresh_directories(&self, directories: Vec<RefreshDirectory>) {
        let runtime = &self.imp().refresh_runtime;
        if let Some(source) = runtime.scan_dispatch_source.take() {
            source.remove();
        }
        let mut queue = runtime.scan_dispatch_queue.borrow_mut();
        queue.clear();
        queue.extend(directories.into_iter().flat_map(|directory| {
            directory
                .stores
                .into_iter()
                .map(move |store| (directory.path.clone(), store.downgrade()))
        }));
        runtime.scan_dispatch_queue_high_water.set(
            runtime
                .scan_dispatch_queue_high_water
                .get()
                .max(queue.len()),
        );
        drop(queue);
        self.dispatch_refresh_scan_batch();
    }

    fn dispatch_refresh_scan_batch(&self) {
        let mut dispatched = 0usize;
        for _ in 0..REFRESH_SCAN_DISPATCH_BATCH {
            let Some((path, store)) = self
                .imp()
                .refresh_runtime
                .scan_dispatch_queue
                .borrow_mut()
                .pop_front()
            else {
                break;
            };
            if let Some(store) = store.upgrade() {
                super::tree_loading::populate_child_store(self, &path, &store);
            }
            dispatched = dispatched.saturating_add(1);
        }
        let refresh = &self.imp().refresh_runtime;
        refresh
            .scan_dispatch_batch_high_water
            .set(refresh.scan_dispatch_batch_high_water.get().max(dispatched));
        if self
            .imp()
            .refresh_runtime
            .scan_dispatch_queue
            .borrow()
            .is_empty()
        {
            self.imp().refresh_runtime.scan_dispatch_source.take();
            super::tree_loading::sync_child_scan_busy_state(self);
            return;
        }
        let section_weak = self.downgrade();
        let source = glib::idle_add_local_once(move || {
            if let Some(section) = section_weak.upgrade() {
                section.imp().refresh_runtime.scan_dispatch_source.take();
                section.dispatch_refresh_scan_batch();
            }
        });
        self.imp()
            .refresh_runtime
            .scan_dispatch_source
            .replace(Some(source));
        super::tree_loading::sync_child_scan_busy_state(self);
    }

    fn plan_refresh(&self, changed_paths: &HashSet<PathBuf>) -> RefreshPlan {
        let current_folder_paths: HashSet<PathBuf> = self
            .current_visible_folders()
            .into_iter()
            .map(|entry| entry.path().to_path_buf())
            .collect();

        let mut expanded_stores = self.expanded_store_index();
        let mut directories = Vec::new();
        for changed_path in changed_paths {
            let Some(dir_path) = Self::refresh_directory_for_path(
                changed_path,
                &current_folder_paths,
                &expanded_stores,
            ) else {
                return RefreshPlan::Full;
            };
            directories.push(dir_path);
        }

        RefreshPlan::Directories(
            minimize_refresh_directories(directories)
                .into_iter()
                .filter_map(|path| {
                    expanded_stores
                        .remove(&path)
                        .map(|stores| RefreshDirectory { path, stores })
                })
                .collect(),
        )
    }

    fn refresh_directory_for_path(
        changed_path: &Path,
        current_folder_paths: &HashSet<PathBuf>,
        expanded_stores: &HashMap<PathBuf, Vec<gtk4::gio::ListStore>>,
    ) -> Option<PathBuf> {
        let mut candidate = Some(changed_path);
        while let Some(path) = candidate {
            let is_workspace_folder = current_folder_paths.contains(path);
            if is_workspace_folder && path == changed_path {
                return None;
            }
            if expanded_stores.contains_key(path) {
                return Some(path.to_path_buf());
            }
            if is_workspace_folder {
                return None;
            }
            candidate = path.parent();
        }
        None
    }

    fn expanded_store_index(&self) -> HashMap<PathBuf, Vec<gtk4::gio::ListStore>> {
        let Some(tree_model) = self.imp().tree_model.borrow().as_ref().cloned() else {
            return HashMap::new();
        };

        let mut stores = HashMap::<PathBuf, Vec<gtk4::gio::ListStore>>::new();
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
            if !item.is_dir() {
                continue;
            }
            let Some(path) = item.path() else {
                continue;
            };
            let Some(store) = row
                .children()
                .and_then(|children| children.downcast::<gtk4::gio::ListStore>().ok())
            else {
                continue;
            };
            stores.entry(path).or_default().push(store);
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
    directories.sort_unstable();
    directories.dedup();

    let mut unique = Vec::<PathBuf>::new();
    for dir in directories {
        if unique
            .last()
            .is_some_and(|existing| dir.starts_with(existing))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_directory_minimization_keeps_only_shallowest_ancestors() {
        let directories = vec![
            PathBuf::from("/workspace/zeta"),
            PathBuf::from("/workspace/alpha/nested"),
            PathBuf::from("/workspace/alpha"),
            PathBuf::from("/workspace/zeta/deep/file"),
            PathBuf::from("/workspace/alpha"),
        ];

        assert_eq!(
            minimize_refresh_directories(directories),
            vec![
                PathBuf::from("/workspace/alpha"),
                PathBuf::from("/workspace/zeta")
            ]
        );
    }

    #[test]
    fn refresh_directory_minimization_handles_the_shared_path_cap() {
        let directories = (0..WORKSPACE_WATCH_PATH_CAP)
            .rev()
            .map(|index| PathBuf::from(format!("/workspace/{index:04}")))
            .collect();

        let minimized = minimize_refresh_directories(directories);

        assert_eq!(minimized.len(), WORKSPACE_WATCH_PATH_CAP);
        assert!(minimized.windows(2).all(|pair| pair[0] < pair[1]));
    }
}

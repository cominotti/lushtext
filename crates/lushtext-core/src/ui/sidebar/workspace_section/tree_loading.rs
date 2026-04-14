// SPDX-License-Identifier: GPL-3.0-or-later

//! Async child-tree loading helpers for `LushtextWorkspaceSection`.
//!
//! This stays in the driving adapter layer because it constructs GTK models
//! (`gio::ListStore`) and schedules main-loop batch appends. The filesystem
//! scan itself still lives in `services::file_tree` as plain Rust data.

use super::LushtextWorkspaceSection;
use crate::services;
use crate::services::file_tree::DirectoryEntry;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::super::file_tree_item::FileTreeItem;

/// Queue of child entries waiting to be appended on future main-loop ticks.
type PendingEntries = Rc<RefCell<VecDeque<DirectoryEntry>>>;

/// One row shape in a child `ListStore`, used to reconcile a refreshed scan
/// without clearing and re-adding the whole subtree.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ChildRowState {
    path: Option<PathBuf>,
    is_dir: bool,
    is_empty: Option<bool>,
    placeholder_label: Option<String>,
}

/// Maximum directory entries before truncation. A single `gio::ListStore`
/// with >10k items causes slow model diff updates in `GtkListView`.
/// Truncated directories show a placeholder row with the count.
const MAX_DIR_ENTRIES: usize = 10_000;
/// Rows appended per main-loop tick when populating a directory tree.
/// 256 items splice in <2ms, staying under the 16ms frame budget.
const CHILD_APPEND_BATCH_SIZE: usize = 256;

/// Build the child model for one expanded directory and kick off its background scan.
pub(super) fn build_children_model(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
) -> gio::ListStore {
    let store = gio::ListStore::new::<FileTreeItem>();
    // Expanding a directory materializes a new visible scope, so restart the
    // scoped watcher set now that this directory participates in auto-refresh.
    section.restart_workspace_watch();
    populate_child_store(section, dir_path, &store);
    store
}

/// Reuse an existing child store when a refresh only needs to reload one subtree.
pub(super) fn populate_child_store(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
    store: &gio::ListStore,
) {
    let store = store.clone();
    let path = dir_path.to_path_buf();
    let cancel = Arc::new(AtomicBool::new(false));
    section
        .imp()
        .dir_stores
        .borrow_mut()
        .insert(path.clone(), store.downgrade());

    if let Some(previous) = section
        .imp()
        .child_scan_tokens
        .borrow_mut()
        .insert(path.clone(), Arc::clone(&cancel))
    {
        previous.store(true, Ordering::Release);
    }

    let section_weak = section.downgrade();
    let lookahead_cap = gtk4::gio::Settings::new(crate::config::APP_ID)
        .uint(crate::config::keys::WORKSPACE_EMPTY_FOLDER_LOOKAHEAD_CAP)
        as usize;
    services::async_task::spawn_blocking_then(
        (store.clone(), path.clone(), Arc::clone(&cancel)),
        move || {
            services::file_tree::scan_directory_bounded(
                &path,
                MAX_DIR_ENTRIES,
                lookahead_cap,
                Some(&cancel),
            )
        },
        move |(store, path, cancel), scan| {
            if scan.cancelled {
                if let Some(section) = section_weak.upgrade() {
                    finish_child_scan(&section, &path, &cancel);
                }
                return;
            }

            let Some(section) = section_weak.upgrade() else {
                return;
            };

            if !child_scan_is_active(&section, &path, &cancel) {
                return;
            }

            apply_scanned_children(&section, store, path, cancel, scan.entries, scan.truncated);
        },
    );
}

/// Drop cached rows, child stores, and in-flight scans for one directory subtree.
pub(super) fn clear_dir_state(section: &LushtextWorkspaceSection, dir_path: &Path) {
    section
        .imp()
        .dir_rows
        .borrow_mut()
        .retain(|path, _| path != dir_path && !path.starts_with(dir_path));
    section
        .imp()
        .dir_stores
        .borrow_mut()
        .retain(|path, _| path != dir_path && !path.starts_with(dir_path));
    section
        .imp()
        .child_paths
        .borrow_mut()
        .retain(|path, _| path != dir_path && !path.starts_with(dir_path));
    section
        .imp()
        .item_locations
        .borrow_mut()
        .retain(|path, _| path.as_path() == dir_path || !path.starts_with(dir_path));

    let cancelled: Vec<_> = {
        let mut tokens = section.imp().child_scan_tokens.borrow_mut();
        let paths: Vec<_> = tokens
            .keys()
            .filter(|path| path.as_path() == dir_path || path.starts_with(dir_path))
            .cloned()
            .collect();
        paths
            .into_iter()
            .filter_map(|path| tokens.remove(&path))
            .collect()
    };

    for token in cancelled {
        token.store(true, Ordering::Release);
    }
}

/// Clear all cached tree-loading state before reloading the workspace roots.
pub(super) fn clear_all_dir_state(section: &LushtextWorkspaceSection) {
    section.imp().dir_rows.borrow_mut().clear();
    section.imp().dir_stores.borrow_mut().clear();
    section.imp().child_paths.borrow_mut().clear();
    section.imp().item_locations.borrow_mut().clear();
    section.imp().root_paths.borrow_mut().clear();

    let cancelled: Vec<_> = section
        .imp()
        .child_scan_tokens
        .borrow_mut()
        .drain()
        .map(|(_, token)| token)
        .collect();
    for token in cancelled {
        token.store(true, Ordering::Release);
    }
}

/// Drop the active scan token for `dir_path` if it still matches `token`.
fn finish_child_scan(section: &LushtextWorkspaceSection, dir_path: &Path, token: &Arc<AtomicBool>) {
    let mut tokens = section.imp().child_scan_tokens.borrow_mut();
    let should_remove = tokens
        .get(dir_path)
        .is_some_and(|active| Arc::ptr_eq(active, token));
    if should_remove {
        tokens.remove(dir_path);
    }
}

/// Check whether a pending child-scan callback still belongs to the current expanded row.
fn child_scan_is_active(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
    token: &Arc<AtomicBool>,
) -> bool {
    if token.load(Ordering::Acquire) {
        finish_child_scan(section, dir_path, token);
        return false;
    }

    {
        let tokens = section.imp().child_scan_tokens.borrow();
        let Some(active) = tokens.get(dir_path) else {
            return false;
        };
        if !Arc::ptr_eq(active, token) {
            return false;
        }
    }

    if let Some(row) = section.find_dir_row(dir_path)
        && !row.is_expanded()
    {
        token.store(true, Ordering::Release);
        finish_child_scan(section, dir_path, token);
        return false;
    }

    true
}

/// Schedule batched GTK appends so large directories do not block a whole frame.
fn append_child_batches(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    entries: Vec<DirectoryEntry>,
    truncated: bool,
) {
    let pending: PendingEntries = Rc::new(RefCell::new(VecDeque::from(entries)));
    append_next_child_batch(section, store, dir_path, token, pending, truncated);
}

/// Append one child batch, then requeue the next batch on the GTK main loop.
fn append_next_child_batch(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    pending: PendingEntries,
    truncated: bool,
) {
    if !child_scan_is_active(section, &dir_path, &token) {
        return;
    }

    let mut batch = Vec::with_capacity(CHILD_APPEND_BATCH_SIZE);
    {
        let mut pending_entries = pending.borrow_mut();
        for _ in 0..CHILD_APPEND_BATCH_SIZE {
            let Some(entry) = pending_entries.pop_front() else {
                break;
            };
            batch.push(FileTreeItem::new(entry.path, entry.is_dir, entry.is_empty));
        }
    }

    if !batch.is_empty() {
        let start_index = section
            .imp()
            .child_paths
            .borrow()
            .get(&dir_path)
            .map_or(0, Vec::len);
        store.splice(store.n_items(), 0, &batch);

        let mut to_expand = Vec::new();
        let mut to_select = None;

        for (offset, item) in batch.iter().enumerate() {
            if let Some(path) = item.path() {
                section.cache_child_item(&dir_path, path.clone(), start_index + offset);
                if section.imp().expanded_paths.borrow().contains(&path) {
                    to_expand.push(path.clone());
                }
                if let Some(pending) = section.imp().pending_selection.borrow().as_ref()
                    && pending == &path
                {
                    to_select = Some(path.clone());
                }
            }
        }

        if !to_expand.is_empty() || to_select.is_some() {
            let section_weak = section.downgrade();
            glib::timeout_add_local_once(Duration::from_millis(1), move || {
                if let Some(section) = section_weak.upgrade() {
                    for path in to_expand {
                        if let Some(row) = section.find_dir_row(&path) {
                            row.set_expanded(true);
                        }
                    }
                    if let Some(path) = to_select {
                        section.select_and_scroll_to(&path);
                    }
                }
            });
        }
    }

    if pending.borrow().is_empty() {
        if truncated {
            let placeholder = [FileTreeItem::new_placeholder(truncated_directory_label())];
            store.splice(store.n_items(), 0, &placeholder);
        }
        finish_child_scan(section, &dir_path, &token);
        return;
    }

    let section_weak = section.downgrade();
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        if let Some(section) = section_weak.upgrade() {
            append_next_child_batch(&section, store, dir_path, token, pending, truncated);
        }
    });
}

fn truncated_directory_label() -> String {
    format!("{MAX_DIR_ENTRIES}+ items - showing first {MAX_DIR_ENTRIES}")
}

fn apply_scanned_children(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    entries: Vec<DirectoryEntry>,
    truncated: bool,
) {
    if truncated {
        tracing::warn!("Directory truncated to {MAX_DIR_ENTRIES} entries");
    }

    if store.n_items() == 0 {
        append_child_batches(section, store, dir_path, token, entries, truncated);
        return;
    }

    reconcile_child_store(section, &store, &dir_path, &token, entries, truncated);
}

fn reconcile_child_store(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    dir_path: &Path,
    token: &Arc<AtomicBool>,
    entries: Vec<DirectoryEntry>,
    truncated: bool,
) {
    let current = snapshot_child_rows(store);
    let desired = desired_child_rows(entries, truncated);

    if current != desired {
        let prefix = common_prefix_len(&current, &desired);
        let suffix = common_suffix_len(&current[prefix..], &desired[prefix..]);
        let removed = current.len().saturating_sub(prefix + suffix);
        let replacement = build_child_items(&desired[prefix..desired.len().saturating_sub(suffix)]);
        #[expect(clippy::cast_possible_truncation)] // list store sizes are far below u32::MAX
        store.splice(prefix as u32, removed as u32, &replacement);
    }

    section.recache_child_store(dir_path, store);
    schedule_child_state_restore(section);
    finish_child_scan(section, dir_path, token);
}

fn snapshot_child_rows(store: &gio::ListStore) -> Vec<ChildRowState> {
    let mut rows = Vec::with_capacity(store.n_items() as usize);
    for index in 0..store.n_items() {
        if let Some(item) = store.item(index).and_downcast::<FileTreeItem>() {
            rows.push(ChildRowState {
                path: item.path(),
                is_dir: item.is_dir(),
                is_empty: item.is_empty(),
                placeholder_label: item.is_placeholder().then(|| item.name()),
            });
        }
    }
    rows
}

fn desired_child_rows(entries: Vec<DirectoryEntry>, truncated: bool) -> Vec<ChildRowState> {
    let mut rows = entries
        .into_iter()
        .map(|entry| ChildRowState {
            path: Some(entry.path),
            is_dir: entry.is_dir,
            is_empty: entry.is_empty,
            placeholder_label: None,
        })
        .collect::<Vec<_>>();

    if truncated {
        rows.push(ChildRowState {
            path: None,
            is_dir: false,
            is_empty: None,
            placeholder_label: Some(truncated_directory_label()),
        });
    }

    rows
}

fn build_child_items(rows: &[ChildRowState]) -> Vec<FileTreeItem> {
    rows.iter()
        .map(|row| match (&row.path, &row.placeholder_label) {
            (Some(path), _) => FileTreeItem::new(path.clone(), row.is_dir, row.is_empty),
            (None, Some(label)) => FileTreeItem::new_placeholder(label.clone()),
            (None, None) => FileTreeItem::new_placeholder(String::new()),
        })
        .collect()
}

fn common_prefix_len(current: &[ChildRowState], desired: &[ChildRowState]) -> usize {
    current
        .iter()
        .zip(desired.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn common_suffix_len(current: &[ChildRowState], desired: &[ChildRowState]) -> usize {
    current
        .iter()
        .rev()
        .zip(desired.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}

fn schedule_child_state_restore(section: &LushtextWorkspaceSection) {
    let expanded_paths = section.imp().expanded_paths.borrow().clone();
    let pending_selection = section.imp().pending_selection.borrow().clone();
    if expanded_paths.is_empty() && pending_selection.is_none() {
        return;
    }

    let section_weak = section.downgrade();
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        if let Some(section) = section_weak.upgrade() {
            for path in expanded_paths {
                if let Some(row) = section.find_dir_row(&path) {
                    row.set_expanded(true);
                }
            }
            if let Some(path) = pending_selection {
                section.select_and_scroll_to(&path);
            }
        }
    });
}

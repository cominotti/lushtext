// SPDX-License-Identifier: GPL-3.0-or-later

//! Async child-tree loading helpers for `LushtextWorkspaceSection`.
//!
//! This stays in the driving adapter layer because it constructs GTK models
//! (`gio::ListStore`) and schedules main-loop batch appends. The filesystem
//! scan itself still lives in `services::file_tree` as plain Rust data.

use super::LushtextWorkspaceSection;
use crate::services;
use crate::services::file_tree::DirectoryEntry;
use crate::services::notifications::NotificationSeverity;
use crate::ui::accessibility;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};
#[cfg(feature = "test-utils")]
use std::cell::Cell;
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

#[cfg(feature = "test-utils")]
thread_local! {
    /// Counts defensive DnD child-model fallbacks during widget regression tests.
    static DRAG_HOVER_EMPTY_CHILD_MODEL_COUNT: Cell<usize> = const { Cell::new(0) };
}

/// Build the child model for one expanded directory and kick off its background scan.
pub(super) fn build_children_model(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
) -> gio::ListStore {
    if super::dnd::folder_reorder_drag_is_active() {
        return empty_children_model_for_drag_hover(section, dir_path);
    }

    // ListStore is GObject's observable list; TreeListModel/ListView react when
    // rows are appended or replaced.
    let store = gio::ListStore::new::<FileTreeItem>();
    let section_weak = section.downgrade();
    // Expanding a directory materializes a new visible scope. Defer watcher
    // reconciliation so TreeListModel can return its child store immediately.
    glib::idle_add_local_once(move || {
        if let Some(section) = section_weak.upgrade() {
            section.restart_workspace_watch();
        }
    });
    populate_child_store(section, dir_path, &store);
    store
}

fn empty_children_model_for_drag_hover(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
) -> gio::ListStore {
    #[cfg(feature = "test-utils")]
    DRAG_HOVER_EMPTY_CHILD_MODEL_COUNT.with(|count| count.set(count.get() + 1));

    let store = gio::ListStore::new::<FileTreeItem>();
    let path = dir_path.to_path_buf();
    let section_weak = section.downgrade();
    // GTK can ask TreeListModel for children if a row auto-expands during DnD
    // hover. Return an empty temporary model and collapse the row back without
    // scanning or restarting watches; reorder hover must only move the line cue.
    glib::idle_add_local_once(move || {
        if let Some(section) = section_weak.upgrade()
            && let Some(row) = section.find_dir_row(&path)
            && row.is_expanded()
        {
            super::dnd::suppress_next_expanded_watch_for_drag(&row);
            row.set_expanded(false);
        }
    });
    store
}

/// Reset the defensive DnD fallback counter before a widget-test observation.
#[cfg(feature = "test-utils")]
pub(super) fn reset_drag_hover_child_model_count_for_test() {
    DRAG_HOVER_EMPTY_CHILD_MODEL_COUNT.with(|count| count.set(0));
}

/// Read how often drag hover accidentally requested child-model creation.
#[cfg(feature = "test-utils")]
pub(super) fn drag_hover_child_model_count_for_test() -> usize {
    DRAG_HOVER_EMPTY_CHILD_MODEL_COUNT.with(Cell::get)
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

    // Each expanded row gets its own scan token. Paths are not unique in the
    // flattened model when a workspace includes overlapping folders.
    section
        .imp()
        .child_scan_tokens
        .borrow_mut()
        .entry(path.clone())
        .or_default()
        .push(Arc::clone(&cancel));
    sync_child_scan_busy_state(section);

    let section_weak = section.downgrade();
    let lookahead_cap = gtk4::gio::Settings::new(crate::config::APP_ID)
        .uint(crate::config::keys::WORKSPACE_EMPTY_FOLDER_LOOKAHEAD_CAP)
        as usize;
    // Move only the store handle, path, and cancel token across the worker
    // boundary; the callback revalidates the token before touching visible GTK state.
    gtk_lush_tasks::spawn_blocking_then(
        (store, path.clone(), Arc::clone(&cancel)),
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

            if !child_scan_is_active(&section, &path, &store, &cancel) {
                return;
            }

            if let Some(error) = scan.error {
                section.report_refresh_error(&format!("Workspace refresh failed: {error}"));
                section.recache_child_store(&path, &store);
                finish_child_scan(&section, &path, &cancel);
                return;
            }

            apply_scanned_children(&section, store, path, cancel, scan.entries, scan.truncated);
        },
    );
}

/// Drop cached rows, child stores, and in-flight scans for one directory subtree.
pub(super) fn clear_dir_state(section: &LushtextWorkspaceSection, dir_path: &Path) {
    let removed_child_paths = section
        .imp()
        .child_paths
        .borrow()
        .iter()
        .filter(|(path, _)| path.as_path() == dir_path || path.starts_with(dir_path))
        .flat_map(|(_, paths)| paths.clone())
        .collect::<Vec<_>>();
    section
        .imp()
        .child_paths
        .borrow_mut()
        .retain(|path, _| path != dir_path && !path.starts_with(dir_path));
    for path in removed_child_paths {
        section.forget_visible_path_occurrence(&path);
    }

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
        .item_locations
        .borrow_mut()
        .retain(|path, _| path.as_path() == dir_path || !path.starts_with(dir_path));

    let cancelled: Vec<_> = {
        let mut tokens = section.imp().child_scan_tokens.borrow_mut();
        // Remove matching tokens while collecting them so callbacks see the
        // subtree as inactive before their cancellation flags are flipped.
        tokens
            .extract_if(|path, _| path.as_path() == dir_path || path.starts_with(dir_path))
            .flat_map(|(_, tokens)| tokens)
            .collect()
    };

    for token in cancelled {
        token.store(true, Ordering::Release);
    }
    sync_child_scan_busy_state(section);
}

/// Clear all cached tree-loading state before reloading the workspace folders.
pub(super) fn clear_all_dir_state(section: &LushtextWorkspaceSection) {
    section.imp().dir_rows.borrow_mut().clear();
    section.imp().dir_stores.borrow_mut().clear();
    section.imp().child_paths.borrow_mut().clear();
    section.imp().item_locations.borrow_mut().clear();
    section.imp().folder_paths.borrow_mut().clear();

    let cancelled: Vec<_> = section
        .imp()
        .child_scan_tokens
        .borrow_mut()
        .drain()
        .flat_map(|(_, tokens)| tokens)
        .collect();
    for token in cancelled {
        token.store(true, Ordering::Release);
    }
    sync_child_scan_busy_state(section);
}

/// Drop the active scan token for `dir_path` if it still matches `token`.
fn finish_child_scan(section: &LushtextWorkspaceSection, dir_path: &Path, token: &Arc<AtomicBool>) {
    let mut tokens = section.imp().child_scan_tokens.borrow_mut();
    let Some(active_tokens) = tokens.get_mut(dir_path) else {
        return;
    };
    active_tokens.retain(|active| !Arc::ptr_eq(active, token));
    if active_tokens.is_empty() {
        tokens.remove(dir_path);
    }
    drop(tokens);
    sync_child_scan_busy_state(section);
}

/// Mirror child directory scan activity into the tree's accessible busy state.
fn sync_child_scan_busy_state(section: &LushtextWorkspaceSection) {
    let busy = !section.imp().child_scan_tokens.borrow().is_empty();
    accessibility::set_busy(&*section.imp().file_tree_view, busy);
    if !busy
        && section
            .imp()
            .refresh_runtime
            .manual_refresh_announcing
            .replace(false)
    {
        section.emit_message("Workspace folders refreshed", NotificationSeverity::Info);
    }
}

/// Check whether a pending child-scan callback still belongs to the current expanded row.
fn child_scan_is_active(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
    store: &gio::ListStore,
    token: &Arc<AtomicBool>,
) -> bool {
    if token.load(Ordering::Acquire) {
        finish_child_scan(section, dir_path, token);
        return false;
    }

    {
        let tokens = section.imp().child_scan_tokens.borrow();
        let Some(active_tokens) = tokens.get(dir_path) else {
            return false;
        };
        if !active_tokens
            .iter()
            .any(|active| Arc::ptr_eq(active, token))
        {
            return false;
        }
    }

    if !expanded_dir_row_owns_store(section, dir_path, store) {
        token.store(true, Ordering::Release);
        finish_child_scan(section, dir_path, token);
        return false;
    }

    true
}

/// Verify this async scan still belongs to the expanded row owning this store.
fn expanded_dir_row_owns_store(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
    store: &gio::ListStore,
) -> bool {
    let Some(tree_model) = section.imp().tree_model.borrow().as_ref().cloned() else {
        return false;
    };
    for index in 0..tree_model.n_items() {
        if let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
            && row.is_expanded()
            && let Some(item) = row.item().and_downcast::<FileTreeItem>()
            && item.is_dir()
            && item.path().as_deref() == Some(dir_path)
            && row
                .children()
                .and_then(|children| children.downcast::<gio::ListStore>().ok())
                .is_some_and(|children| children.as_ptr() == store.as_ptr())
        {
            return true;
        }
    }
    false
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
    if !child_scan_is_active(section, &dir_path, &store, &token) {
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
        // splice() emits one items-changed signal for the whole batch, avoiding
        // one relayout per child row while large directories stream in.
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
        section.sync_file_row_states();
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

/// Apply a completed scan, streaming first loads and reconciling refreshes.
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

/// Reconcile a refreshed child store with minimal row churn.
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
        // Preserve matching edges and splice only the changed middle so
        // refreshes avoid rebuilding stable rows and losing expansion/selection state.
        let prefix = common_prefix_len(&current, &desired);
        let suffix = common_suffix_len(&current[prefix..], &desired[prefix..]);
        let removed = current.len().saturating_sub(prefix + suffix);
        let replacement = build_child_items(&desired[prefix..desired.len().saturating_sub(suffix)]);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Directory child batches are capped well below u32::MAX before they reach the GTK list store"
        )]
        store.splice(prefix as u32, removed as u32, &replacement);
    }

    section.recache_child_store(dir_path, store);
    schedule_child_state_restore(section);
    section.sync_file_row_states();
    finish_child_scan(section, dir_path, token);
}

/// Snapshot the current child store into comparable refresh state.
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

/// Build the desired child-row state from a fresh scan.
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

/// Convert comparable row state back into GTK row objects for splicing.
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

/// Restore expansion and pending selection after reconciliation may replace rows.
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

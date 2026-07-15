// SPDX-License-Identifier: GPL-3.0-or-later

//! Async child-tree loading helpers for `LushtextWorkspaceSection`.
//!
//! This stays in the driving adapter layer because it constructs GTK models
//! (`gio::ListStore`) and schedules main-loop batch appends. The filesystem
//! scan itself still lives in `services::file_tree` as plain Rust data.

use super::LushtextWorkspaceSection;
use crate::services;
use crate::services::file_tree::{
    DirectoryReconciliationPlan, DirectoryRowState, plan_directory_reconciliation,
};
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

struct ChildScanResult {
    cancelled: bool,
    error: Option<String>,
    truncated: bool,
    plan: Option<DirectoryReconciliationPlan>,
}

struct ChildReconcileProgress {
    position: usize,
    remove_remaining: usize,
    inserted: usize,
    replacement: VecDeque<DirectoryRowState>,
    mirror_generation: u64,
}

pub(super) struct ChildMirrorSnapshot {
    rows: Vec<DirectoryRowState>,
    generation: u64,
}

/// Maximum directory entries before truncation. A single `gio::ListStore`
/// with >10k items causes slow model diff updates in `GtkListView`.
/// Truncated directories show a placeholder row with the count.
const MAX_DIR_ENTRIES: usize = 10_000;
/// Rows appended per main-loop tick when populating a directory tree.
/// 256 items splice in <2ms, staying under the 16ms frame budget.
const CHILD_APPEND_BATCH_SIZE: usize = 256;
/// Changed rows at or below one batch retain the calibrated direct splice path.
const CHILD_RECONCILE_DIRECT_ROWS: usize = CHILD_APPEND_BATCH_SIZE;

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
    let store_key = child_store_key(&store);
    ensure_child_store_identity(section, &store);
    section
        .imp()
        .child_store_paths
        .borrow_mut()
        .insert(store_key, path.clone());
    cancel_store_reconciliation(section, store_key);
    if let Some(previous) = section
        .imp()
        .child_store_tokens
        .borrow_mut()
        .insert(store_key, Arc::clone(&cancel))
    {
        previous.store(true, Ordering::Release);
    }
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

    let (current_rows, mirror_generation) = {
        let mut mirrors = section.imp().child_row_mirrors.borrow_mut();
        let rows = mirrors
            .entry(store_key)
            .or_insert_with(|| snapshot_child_rows(&store))
            .clone();
        let generation = *section
            .imp()
            .child_row_mirror_generations
            .borrow_mut()
            .entry(store_key)
            .or_default();
        (rows, generation)
    };

    let section_weak = section.downgrade();
    let lookahead_cap = gtk4::gio::Settings::new(crate::config::APP_ID)
        .uint(crate::config::keys::WORKSPACE_EMPTY_FOLDER_LOOKAHEAD_CAP)
        as usize;
    // Move only the store handle, path, and cancel token across the worker
    // boundary; the callback revalidates the token before touching visible GTK state.
    gtk_lush_tasks::spawn_blocking_then(
        (store, path.clone(), Arc::clone(&cancel)),
        move || {
            let scan = services::file_tree::scan_directory_bounded(
                &path,
                MAX_DIR_ENTRIES,
                lookahead_cap,
                Some(&cancel),
            );
            let plan = if scan.cancelled || scan.error.is_some() {
                None
            } else {
                let desired_rows = desired_child_rows(scan.entries, scan.truncated);
                Some(plan_directory_reconciliation(&current_rows, &desired_rows))
            };
            ChildScanResult {
                cancelled: scan.cancelled,
                error: scan.error,
                truncated: scan.truncated,
                plan,
            }
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

            let Some(plan) = scan.plan else {
                finish_child_scan(&section, &path, &cancel);
                return;
            };
            apply_scanned_children(
                &section,
                store,
                path,
                cancel,
                mirror_generation,
                plan,
                scan.truncated,
            );
        },
    );
}

pub(super) fn snapshot_child_store_mirror(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
) -> Option<ChildMirrorSnapshot> {
    let store_key = child_store_key(store);
    let rows = section
        .imp()
        .child_row_mirrors
        .borrow()
        .get(&store_key)
        .cloned()?;
    let generation = section
        .imp()
        .child_row_mirror_generations
        .borrow()
        .get(&store_key)
        .copied()
        .unwrap_or(0);
    Some(ChildMirrorSnapshot { rows, generation })
}

pub(super) fn restore_child_store_mirror(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
    store: &gio::ListStore,
    snapshot: ChildMirrorSnapshot,
) {
    let store_key = child_store_key(store);
    ensure_child_store_identity(section, store);
    section
        .imp()
        .child_store_paths
        .borrow_mut()
        .insert(store_key, dir_path.to_path_buf());
    section
        .imp()
        .child_row_mirrors
        .borrow_mut()
        .insert(store_key, snapshot.rows);
    section
        .imp()
        .child_row_mirror_generations
        .borrow_mut()
        .insert(store_key, snapshot.generation);
}

/// Drop cached rows, child stores, and in-flight scans for one directory subtree.
pub(super) fn clear_dir_state(section: &LushtextWorkspaceSection, dir_path: &Path) {
    let removed_store_keys = section
        .imp()
        .child_store_paths
        .borrow()
        .iter()
        .filter_map(|(key, path)| {
            (path.as_path() == dir_path || path.starts_with(dir_path)).then_some(*key)
        })
        .collect::<Vec<_>>();
    for store_key in removed_store_keys {
        if let Some(token) = section
            .imp()
            .child_store_tokens
            .borrow_mut()
            .remove(&store_key)
        {
            token.store(true, Ordering::Release);
        }
        cancel_store_reconciliation(section, store_key);
        section
            .imp()
            .child_row_mirrors
            .borrow_mut()
            .remove(&store_key);
        section
            .imp()
            .child_row_mirror_generations
            .borrow_mut()
            .remove(&store_key);
        section
            .imp()
            .child_store_paths
            .borrow_mut()
            .remove(&store_key);
        section
            .imp()
            .child_store_refs
            .borrow_mut()
            .remove(&store_key);
    }
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
        retire_store_runtime_for_token(section, &token);
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
    for (_, (_, source)) in section.imp().child_reconcile_sources.borrow_mut().drain() {
        source.remove();
    }
    section.imp().child_row_mirrors.borrow_mut().clear();
    section.imp().child_store_paths.borrow_mut().clear();
    section.imp().child_store_refs.borrow_mut().clear();
    section
        .imp()
        .child_row_mirror_generations
        .borrow_mut()
        .clear();
    section.imp().child_store_tokens.borrow_mut().clear();

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
    let store_keys = section
        .imp()
        .child_store_tokens
        .borrow()
        .iter()
        .filter_map(|(key, active)| Arc::ptr_eq(active, token).then_some(*key))
        .collect::<Vec<_>>();
    for key in store_keys {
        section.imp().child_store_tokens.borrow_mut().remove(&key);
        cancel_store_reconciliation_if_token(section, key, token);
    }
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

    let store_key = child_store_key(store);
    let owns_store = section
        .imp()
        .child_store_tokens
        .borrow()
        .get(&store_key)
        .is_some_and(|active| Arc::ptr_eq(active, token));
    if !owns_store {
        token.store(true, Ordering::Release);
        finish_child_scan(section, dir_path, token);
        return false;
    }

    true
}

fn child_store_key(store: &gio::ListStore) -> usize {
    store.as_ptr() as usize
}

/// Reject state left behind when GLib reuses a released store's raw address.
fn ensure_child_store_identity(section: &LushtextWorkspaceSection, store: &gio::ListStore) {
    let store_key = child_store_key(store);
    let identity_matches = section
        .imp()
        .child_store_refs
        .borrow()
        .get(&store_key)
        .and_then(glib::WeakRef::upgrade)
        .is_some_and(|registered| registered.as_ptr() == store.as_ptr());
    if identity_matches {
        return;
    }

    if let Some(token) = section
        .imp()
        .child_store_tokens
        .borrow_mut()
        .remove(&store_key)
    {
        token.store(true, Ordering::Release);
        let mut scan_tokens = section.imp().child_scan_tokens.borrow_mut();
        scan_tokens.retain(|_, tokens| {
            tokens.retain(|active| !Arc::ptr_eq(active, &token));
            !tokens.is_empty()
        });
    }
    cancel_store_reconciliation(section, store_key);
    section
        .imp()
        .child_row_mirrors
        .borrow_mut()
        .remove(&store_key);
    section
        .imp()
        .child_row_mirror_generations
        .borrow_mut()
        .remove(&store_key);
    section
        .imp()
        .child_store_paths
        .borrow_mut()
        .remove(&store_key);
    section
        .imp()
        .child_store_refs
        .borrow_mut()
        .insert(store_key, store.downgrade());
    sync_child_scan_busy_state(section);
}

fn child_mirror_generation(section: &LushtextWorkspaceSection, store: &gio::ListStore) -> u64 {
    section
        .imp()
        .child_row_mirror_generations
        .borrow()
        .get(&child_store_key(store))
        .copied()
        .unwrap_or(0)
}

fn splice_child_store(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    position: usize,
    removed: usize,
    rows: &[DirectoryRowState],
    items: &[FileTreeItem],
) -> u64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Directory child stores are capped below u32::MAX"
    )]
    store.splice(position as u32, removed as u32, items);
    let store_key = child_store_key(store);
    let mut mirrors = section.imp().child_row_mirrors.borrow_mut();
    let mirror = mirrors.entry(store_key).or_default();
    let end = position.saturating_add(removed).min(mirror.len());
    let start = position.min(end);
    mirror.splice(start..end, rows.iter().cloned());
    drop(mirrors);
    let generation = {
        let mut generations = section.imp().child_row_mirror_generations.borrow_mut();
        let generation = generations.entry(store_key).or_default();
        *generation = generation.wrapping_add(1);
        *generation
    };
    let changed_rows = removed.max(rows.len());
    let refresh = &section.imp().refresh_runtime;
    refresh
        .reconcile_batch_count
        .set(refresh.reconcile_batch_count.get().saturating_add(1));
    refresh
        .reconcile_max_batch_rows
        .set(refresh.reconcile_max_batch_rows.get().max(changed_rows));
    generation
}

pub(super) fn record_child_store_insert(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    position: usize,
    items: &[FileTreeItem],
) {
    let rows = items.iter().map(row_state_from_item).collect::<Vec<_>>();
    let store_key = child_store_key(store);
    let mut mirrors = section.imp().child_row_mirrors.borrow_mut();
    let mirror = mirrors.entry(store_key).or_default();
    let position = position.min(mirror.len());
    mirror.splice(position..position, rows);
    drop(mirrors);
    advance_child_mirror_generation(section, store_key);
}

pub(super) fn record_child_store_remove(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    position: usize,
    removed: usize,
) {
    let store_key = child_store_key(store);
    let mut mirrors = section.imp().child_row_mirrors.borrow_mut();
    let mirror = mirrors.entry(store_key).or_default();
    let end = position.saturating_add(removed).min(mirror.len());
    if position < end {
        mirror.drain(position..end);
    }
    drop(mirrors);
    advance_child_mirror_generation(section, store_key);
}

pub(super) fn record_child_store_path_update(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    position: usize,
    new_path: &Path,
) {
    let store_key = child_store_key(store);
    if let Some(row) = section
        .imp()
        .child_row_mirrors
        .borrow_mut()
        .entry(store_key)
        .or_default()
        .get_mut(position)
    {
        row.path = Some(new_path.to_path_buf());
        advance_child_mirror_generation(section, store_key);
    }
}

fn advance_child_mirror_generation(section: &LushtextWorkspaceSection, store_key: usize) -> u64 {
    let mut generations = section.imp().child_row_mirror_generations.borrow_mut();
    let generation = generations.entry(store_key).or_default();
    *generation = generation.wrapping_add(1);
    *generation
}

fn row_state_from_item(item: &FileTreeItem) -> DirectoryRowState {
    DirectoryRowState {
        path: item.path(),
        is_dir: item.is_dir(),
        is_empty: item.is_empty(),
        is_placeholder: item.is_placeholder(),
    }
}

fn cancel_store_reconciliation(section: &LushtextWorkspaceSection, store_key: usize) {
    if let Some((_, source)) = section
        .imp()
        .child_reconcile_sources
        .borrow_mut()
        .remove(&store_key)
    {
        source.remove();
        let refresh = &section.imp().refresh_runtime;
        refresh
            .reconcile_superseded_count
            .set(refresh.reconcile_superseded_count.get().saturating_add(1));
    }
}

fn cancel_store_reconciliation_if_token(
    section: &LushtextWorkspaceSection,
    store_key: usize,
    token: &Arc<AtomicBool>,
) {
    let owns_source = section
        .imp()
        .child_reconcile_sources
        .borrow()
        .get(&store_key)
        .is_some_and(|(active, _)| Arc::ptr_eq(active, token));
    if owns_source {
        cancel_store_reconciliation(section, store_key);
    }
}

fn retire_store_runtime_for_token(section: &LushtextWorkspaceSection, token: &Arc<AtomicBool>) {
    let store_keys = section
        .imp()
        .child_store_tokens
        .borrow()
        .iter()
        .filter_map(|(key, active)| Arc::ptr_eq(active, token).then_some(*key))
        .collect::<Vec<_>>();
    for store_key in store_keys {
        section
            .imp()
            .child_store_tokens
            .borrow_mut()
            .remove(&store_key);
        cancel_store_reconciliation_if_token(section, store_key, token);
        section
            .imp()
            .child_row_mirrors
            .borrow_mut()
            .remove(&store_key);
        section
            .imp()
            .child_row_mirror_generations
            .borrow_mut()
            .remove(&store_key);
        section
            .imp()
            .child_store_paths
            .borrow_mut()
            .remove(&store_key);
        section
            .imp()
            .child_store_refs
            .borrow_mut()
            .remove(&store_key);
    }
}

fn truncated_directory_label() -> String {
    format!("{MAX_DIR_ENTRIES}+ items - showing first {MAX_DIR_ENTRIES}")
}

/// Apply one worker-computed plan through the direct or bounded GTK path.
fn apply_scanned_children(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    mirror_generation: u64,
    plan: DirectoryReconciliationPlan,
    truncated: bool,
) {
    if truncated {
        tracing::warn!("Directory truncated to {MAX_DIR_ENTRIES} entries");
    }

    if !child_scan_is_active(section, &dir_path, &store, &token)
        || child_mirror_generation(section, &store) != mirror_generation
    {
        finish_child_scan(section, &dir_path, &token);
        return;
    }

    match plan {
        DirectoryReconciliationPlan::Unchanged => {
            finish_child_reconciliation(section, &store, &dir_path, &token);
        }
        DirectoryReconciliationPlan::Splice {
            position,
            removed,
            replacement,
        } if removed.saturating_add(replacement.len()) <= CHILD_RECONCILE_DIRECT_ROWS => {
            let items = build_child_items(&replacement);
            splice_child_store(section, &store, position, removed, &replacement, &items);
            finish_child_reconciliation(section, &store, &dir_path, &token);
        }
        DirectoryReconciliationPlan::Splice {
            position,
            removed,
            replacement,
        } => {
            let progress = Rc::new(RefCell::new(ChildReconcileProgress {
                position,
                remove_remaining: removed,
                inserted: 0,
                replacement: VecDeque::from(replacement),
                mirror_generation,
            }));
            apply_next_reconcile_batch(section, store, dir_path, token, progress);
        }
    }
}

fn apply_next_reconcile_batch(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    progress: Rc<RefCell<ChildReconcileProgress>>,
) {
    let expected_generation = progress.borrow().mirror_generation;
    if !child_scan_is_active(section, &dir_path, &store, &token)
        || child_mirror_generation(section, &store) != expected_generation
    {
        finish_child_scan(section, &dir_path, &token);
        return;
    }

    let (position, removed, rows) = {
        let mut progress = progress.borrow_mut();
        if progress.remove_remaining > 0 {
            let removed = progress.remove_remaining.min(CHILD_APPEND_BATCH_SIZE);
            progress.remove_remaining -= removed;
            (progress.position, removed, Vec::new())
        } else {
            let mut rows = Vec::with_capacity(CHILD_APPEND_BATCH_SIZE);
            for _ in 0..CHILD_APPEND_BATCH_SIZE {
                let Some(row) = progress.replacement.pop_front() else {
                    break;
                };
                rows.push(row);
            }
            let position = progress.position.saturating_add(progress.inserted);
            progress.inserted = progress.inserted.saturating_add(rows.len());
            (position, 0, rows)
        }
    };
    let items = build_child_items(&rows);
    let generation = splice_child_store(section, &store, position, removed, &rows, &items);
    progress.borrow_mut().mirror_generation = generation;

    let complete = {
        let progress = progress.borrow();
        progress.remove_remaining == 0 && progress.replacement.is_empty()
    };
    if complete {
        finish_child_reconciliation(section, &store, &dir_path, &token);
        return;
    }
    schedule_next_reconcile_batch(section, store, dir_path, token, progress);
}

fn schedule_next_reconcile_batch(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    progress: Rc<RefCell<ChildReconcileProgress>>,
) {
    let store_key = child_store_key(&store);
    let section_weak = section.downgrade();
    let callback_token = Arc::clone(&token);
    #[cfg(feature = "test-utils")]
    let batch_delay = {
        let configured = section
            .imp()
            .refresh_runtime
            .test_reconcile_batch_delay
            .get();
        if configured.is_zero() {
            Duration::from_millis(1)
        } else {
            configured
        }
    };
    #[cfg(not(feature = "test-utils"))]
    let batch_delay = Duration::from_millis(1);
    let source = glib::timeout_add_local_once(batch_delay, move || {
        let Some(section) = section_weak.upgrade() else {
            return;
        };
        let owns_source = section
            .imp()
            .child_reconcile_sources
            .borrow()
            .get(&store_key)
            .is_some_and(|(active, _)| Arc::ptr_eq(active, &callback_token));
        if !owns_source {
            return;
        }
        section
            .imp()
            .child_reconcile_sources
            .borrow_mut()
            .remove(&store_key);
        apply_next_reconcile_batch(&section, store, dir_path, callback_token, progress);
    });
    if let Some((_, previous)) = section
        .imp()
        .child_reconcile_sources
        .borrow_mut()
        .insert(store_key, (token, source))
    {
        previous.remove();
    }
}

fn finish_child_reconciliation(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    dir_path: &Path,
    token: &Arc<AtomicBool>,
) {
    if !child_scan_is_active(section, dir_path, store, token) {
        finish_child_scan(section, dir_path, token);
        return;
    }
    let recached = {
        let mirrors = section.imp().child_row_mirrors.borrow();
        mirrors.get(&child_store_key(store)).is_some_and(|mirror| {
            section.recache_child_rows_from_mirror(dir_path, mirror);
            true
        })
    };
    if !recached {
        finish_child_scan(section, dir_path, token);
        return;
    }
    schedule_child_state_restore(section);
    section.sync_file_row_states();
    let refresh = &section.imp().refresh_runtime;
    refresh
        .reconcile_terminal_count
        .set(refresh.reconcile_terminal_count.get().saturating_add(1));
    finish_child_scan(section, dir_path, token);
}

/// Snapshot only when adopting a pre-existing store without a mirror.
fn snapshot_child_rows(store: &gio::ListStore) -> Vec<DirectoryRowState> {
    let mut rows = Vec::with_capacity(store.n_items() as usize);
    for index in 0..store.n_items() {
        if let Some(item) = store.item(index).and_downcast::<FileTreeItem>() {
            rows.push(row_state_from_item(&item));
        }
    }
    rows
}

/// Build the desired child-row state from a fresh scan.
fn desired_child_rows(
    entries: Vec<services::file_tree::DirectoryEntry>,
    truncated: bool,
) -> Vec<DirectoryRowState> {
    let mut rows = entries
        .into_iter()
        .map(DirectoryRowState::from_entry)
        .collect::<Vec<_>>();

    if truncated {
        rows.push(DirectoryRowState::truncation_placeholder());
    }

    rows
}

/// Convert comparable row state back into GTK row objects for splicing.
fn build_child_items(rows: &[DirectoryRowState]) -> Vec<FileTreeItem> {
    rows.iter()
        .map(|row| match row.path.as_ref() {
            Some(path) => FileTreeItem::new(path.clone(), row.is_dir, row.is_empty),
            None => FileTreeItem::new_placeholder(truncated_directory_label()),
        })
        .collect()
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

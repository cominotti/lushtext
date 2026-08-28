// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's directory-scan stage order.
//!
//! # Role
//!
//! This is the **execution** coordination role for scanning: the child scan worker,
//! child-store identity, the mirror generation and its bounded `splice()`
//! reconciliation, directory-state clearing, and the deferred expansion restore. It
//! stays in the driving adapter layer because it constructs GTK models
//! (`gio::ListStore`) and schedules main-loop batch appends; the filesystem scan
//! itself lives in `services::file_tree` as plain Rust data.
//!
//! Renamed from the pre-convention `tree_loading.rs`, which was **not one
//! coordination job**. Its admission gate moved to `scan_admission.rs` and its
//! drag-hover shield to `reorder_execution.rs`; what remains here is one job.
//!
//! # This module owns the materialization entry point
//!
//! `build_children_model` is the `GtkTreeListModel` create function — the single
//! point at which a directory's children come into existence. The workflow's
//! evidence surface MUST NOT reach it, directly or transitively: reading evidence
//! must not materialize toolkit state, start a scan, or queue a watcher restart.
//! Derive observations from `expanded_paths` instead, which is the authoritative live
//! set.
//!
//! # Expansion-state contract
//!
//! `schedule_child_state_restore` defers a restore, and its `expanded_paths` borrow
//! lives **inside** the deferred closure rather than in the scheduling scope, so a
//! user collapse between scheduling and the callback is never resurrected. Keep that
//! placement.

use super::LushtextWorkspaceSection;
use super::imp::ItemLocation;
use super::scan_admission;
use crate::services;
use crate::services::file_tree::{
    DirectoryReconciliationPlan, DirectoryRowState, plan_directory_reconciliation,
};
use crate::services::notifications::NotificationSeverity;
use crate::ui::accessibility;
use crate::ui::sidebar::policy::{
    WorkspaceScanFinish as ChildScanFinish, WorkspaceScanSubmission as ChildScanSubmission,
    WorkspaceScanTicket as ChildScanTicket,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
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

pub(super) struct ActiveChildScan {
    ticket: ChildScanTicket,
    path: PathBuf,
    token: Arc<AtomicBool>,
}

pub(super) struct PendingChildScan {
    ticket: ChildScanTicket,
    path: PathBuf,
    store: glib::WeakRef<gio::ListStore>,
    lookahead_cap: usize,
}

struct ChildReconcileProgress {
    position: usize,
    remove_remaining: usize,
    inserted: usize,
    replacement: VecDeque<DirectoryRowState>,
    mirror_generation: u64,
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

/// Build the child model for one expanded directory and kick off its background scan.
pub(super) fn build_children_model(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
) -> gio::ListStore {
    if super::reorder_execution::folder_reorder_drag_is_active() {
        return super::reorder_execution::empty_children_model_for_drag_hover(section, dir_path);
    }

    // ListStore is GObject's observable list; TreeListModel/ListView react when
    // rows are appended or replaced.
    let store = gio::ListStore::new::<FileTreeItem>();
    populate_child_store(section, dir_path, &store);
    store
}

/// Reuse an existing child store when a refresh only needs to reload one subtree.
pub(super) fn populate_child_store(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
    store: &gio::ListStore,
) {
    let path = dir_path.to_path_buf();
    let store_key = child_store_key(store);
    ensure_child_store_identity(section, store);
    section
        .imp()
        .child_store_paths
        .borrow_mut()
        .insert(store_key, path.clone());
    section
        .imp()
        .dir_stores
        .borrow_mut()
        .insert(path.clone(), store.downgrade());
    let lookahead_cap = gtk4::gio::Settings::new(crate::config::APP_ID)
        .uint(crate::config::keys::WORKSPACE_EMPTY_FOLDER_LOOKAHEAD_CAP)
        as usize;
    let lifetime = section.imp().child_scan_lifetime.get();
    let (submission, flight_metrics) = {
        let mut flights = section.imp().child_scan_flights.borrow_mut();
        let flight = flights.entry(store_key).or_default();
        let submission = flight.submit(lifetime);
        (submission, flight.metrics())
    };
    let refresh = &section.imp().refresh_runtime;
    refresh.scan_active_per_store_high_water.set(
        refresh
            .scan_active_per_store_high_water
            .get()
            .max(flight_metrics.active_high_water),
    );
    refresh.scan_pending_per_store_high_water.set(
        refresh
            .scan_pending_per_store_high_water
            .get()
            .max(flight_metrics.pending_high_water),
    );
    let ticket = match submission {
        ChildScanSubmission::Start(ticket) | ChildScanSubmission::QueueLatest { ticket, .. } => {
            ticket
        }
    };
    let request = PendingChildScan {
        ticket,
        path,
        store: store.downgrade(),
        lookahead_cap,
    };

    match submission {
        ChildScanSubmission::Start(_) => start_child_scan(section, store_key, request),
        ChildScanSubmission::QueueLatest { cancel_active, .. } => {
            section
                .imp()
                .child_pending_scans
                .borrow_mut()
                .insert(store_key, request);
            refresh.scan_weak_pending_high_water.set(1);
            refresh
                .scan_cancellation_requests
                .set(refresh.scan_cancellation_requests.get().saturating_add(1));
            let cancelled_before_admission = section
                .imp()
                .child_admission_scans
                .borrow_mut()
                .remove(&store_key)
                .is_some_and(|waiting| waiting.ticket == cancel_active);
            if cancelled_before_admission {
                finish_child_scan(section, store_key, cancel_active);
            } else {
                if let Some(active) = section.imp().child_active_scans.borrow().get(&store_key)
                    && active.ticket == cancel_active
                {
                    active.token.store(true, Ordering::Release);
                }
                let reconciliation_cancelled = cancel_store_reconciliation(section, store_key);
                if reconciliation_cancelled {
                    finish_child_scan(section, store_key, cancel_active);
                } else {
                    sync_child_scan_busy_state(section);
                }
            }
        }
    }
}

/// Admit one compact request, upgrading its weak store and capturing the mirror now.
pub(super) fn start_child_scan(
    section: &LushtextWorkspaceSection,
    store_key: usize,
    request: PendingChildScan,
) {
    let lifetime = section.imp().child_scan_lifetime.get();
    let is_active = section
        .imp()
        .child_scan_flights
        .borrow()
        .get(&store_key)
        .is_some_and(|flight| flight.active() == Some(request.ticket));
    if !is_active
        || request.ticket.lifetime != lifetime
        || section
            .imp()
            .child_store_paths
            .borrow()
            .get(&store_key)
            .is_none_or(|path| path != &request.path)
    {
        finish_child_scan(section, store_key, request.ticket);
        return;
    }
    let Some(permit) = scan_admission::try_acquire_workspace_scan_permit() else {
        section
            .imp()
            .child_admission_scans
            .borrow_mut()
            .insert(store_key, request);
        scan_admission::arm_workspace_scan_admission_retry(section);
        sync_child_scan_busy_state(section);
        return;
    };
    section
        .imp()
        .child_admission_scans
        .borrow_mut()
        .remove(&store_key);
    let Some(store) = request.store.upgrade() else {
        drop(permit);
        finish_child_scan(section, store_key, request.ticket);
        return;
    };
    if !child_store_identity_matches(section, store_key, &store) {
        drop(permit);
        finish_child_scan(section, store_key, request.ticket);
        return;
    }

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
    let refresh = &section.imp().refresh_runtime;
    refresh.scan_active_per_store_high_water.set(1);
    refresh
        .scan_mirror_captures
        .set(refresh.scan_mirror_captures.get().saturating_add(1));
    let cancel = Arc::new(AtomicBool::new(false));
    section.imp().child_active_scans.borrow_mut().insert(
        store_key,
        ActiveChildScan {
            ticket: request.ticket,
            path: request.path.clone(),
            token: Arc::clone(&cancel),
        },
    );
    sync_child_scan_busy_state(section);

    let section_weak = section.downgrade();
    let path = request.path;
    let ticket = request.ticket;
    let lookahead_cap = request.lookahead_cap;
    #[cfg(feature = "test-utils")]
    let scan_delay = section.imp().refresh_runtime.test_scan_delay.get();
    gtk_lush_tasks::spawn_blocking_then(
        (store, path.clone(), Arc::clone(&cancel), permit),
        move || {
            #[cfg(feature = "test-utils")]
            if !scan_delay.is_zero() {
                std::thread::sleep(scan_delay);
            }
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
        move |(store, path, cancel, _permit), scan| {
            let Some(section) = section_weak.upgrade() else {
                return;
            };
            if scan.cancelled {
                let refresh = &section.imp().refresh_runtime;
                refresh
                    .scan_cancelled_terminals
                    .set(refresh.scan_cancelled_terminals.get().saturating_add(1));
                finish_child_scan(&section, store_key, ticket);
                return;
            }
            if !child_scan_is_active(&section, store_key, ticket, &store, &cancel) {
                let refresh = &section.imp().refresh_runtime;
                refresh
                    .scan_stale_completions
                    .set(refresh.scan_stale_completions.get().saturating_add(1));
                finish_child_scan(&section, store_key, ticket);
                return;
            }

            if let Some(error) = scan.error {
                section.report_refresh_error(&format!("Workspace refresh failed: {error}"));
                section.recache_child_store(&path, &store);
                finish_child_scan(&section, store_key, ticket);
                return;
            }

            let Some(plan) = scan.plan else {
                finish_child_scan(&section, store_key, ticket);
                return;
            };
            apply_scanned_children(
                &section,
                ScannedChildrenPlan {
                    store,
                    dir_path: path,
                    token: cancel,
                    ticket,
                    mirror_generation,
                    plan,
                    truncated: scan.truncated,
                },
            );
        },
    );
}

/// Drop cached rows, child stores, and in-flight scans for one directory subtree.
pub(super) fn clear_dir_state(section: &LushtextWorkspaceSection, dir_path: &Path) {
    clear_dir_states(section, &HashSet::from([dir_path.to_path_buf()]));
}

/// Retire any number of removed directory roots with one pass per state map.
fn clear_dir_states(section: &LushtextWorkspaceSection, roots: &HashSet<PathBuf>) {
    if roots.is_empty() {
        return;
    }
    let under_removed_root =
        |path: &Path| path.ancestors().any(|ancestor| roots.contains(ancestor));
    super::folder_execution::cancel_folder_empty_probes_under_roots(section, roots);
    // Accepted removals retire expansion intent for the affected subtrees so a
    // later refresh cannot resurrect paths that no longer exist.
    section
        .imp()
        .expanded_paths
        .borrow_mut()
        .retain(|path| !under_removed_root(path));
    let removed_store_keys = section
        .imp()
        .child_store_paths
        .borrow()
        .iter()
        .filter_map(|(key, path)| under_removed_root(path).then_some(*key))
        .collect::<HashSet<_>>();
    for store_key in &removed_store_keys {
        cancel_child_store_runtime(section, *store_key);
    }
    section
        .imp()
        .child_row_mirrors
        .borrow_mut()
        .retain(|key, _| !removed_store_keys.contains(key));
    section
        .imp()
        .child_row_mirror_generations
        .borrow_mut()
        .retain(|key, _| !removed_store_keys.contains(key));
    section
        .imp()
        .child_store_paths
        .borrow_mut()
        .retain(|key, _| !removed_store_keys.contains(key));
    section
        .imp()
        .child_store_refs
        .borrow_mut()
        .retain(|key, _| !removed_store_keys.contains(key));
    let removed_child_paths = section
        .imp()
        .child_paths
        .borrow()
        .iter()
        .filter(|(path, _)| under_removed_root(path))
        .flat_map(|(_, paths)| paths.clone())
        .collect::<Vec<_>>();
    section
        .imp()
        .child_paths
        .borrow_mut()
        .retain(|path, _| !under_removed_root(path));
    for path in removed_child_paths {
        section.forget_visible_path_occurrence(&path);
    }

    section
        .imp()
        .dir_rows
        .borrow_mut()
        .retain(|path, _| !under_removed_root(path));
    section
        .imp()
        .dir_stores
        .borrow_mut()
        .retain(|path, _| !under_removed_root(path));
    section
        .imp()
        .item_locations
        .borrow_mut()
        .retain(|path, _| roots.contains(path) || !under_removed_root(path));

    sync_child_scan_busy_state(section);
}

/// Clear all cached tree-loading state before reloading the workspace folders.
pub(super) fn clear_all_dir_state(section: &LushtextWorkspaceSection) {
    section
        .imp()
        .child_scan_lifetime
        .set(section.imp().child_scan_lifetime.get().wrapping_add(1));
    for active in section.imp().child_active_scans.borrow_mut().drain() {
        active.1.token.store(true, Ordering::Release);
    }
    for flight in section.imp().child_scan_flights.borrow_mut().values_mut() {
        flight.cancel_all();
    }
    section.imp().child_scan_flights.borrow_mut().clear();
    section.imp().child_pending_scans.borrow_mut().clear();
    section.imp().child_admission_scans.borrow_mut().clear();
    section.imp().folder_empty_active.borrow_mut().clear();
    section.imp().folder_empty_pending.borrow_mut().clear();
    section.imp().folder_empty_admission.borrow_mut().clear();
    if let Some(source) = section.imp().workspace_scan_admission_source.take() {
        source.remove();
    }
    for flight in section.imp().folder_empty_flights.borrow_mut().values_mut() {
        flight.cancel_all();
    }
    section.imp().folder_empty_flights.borrow_mut().clear();
    section
        .imp()
        .refresh_runtime
        .scan_dispatch_queue
        .borrow_mut()
        .clear();
    if let Some(source) = section.imp().refresh_runtime.scan_dispatch_source.take() {
        source.remove();
    }
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
    sync_child_scan_busy_state(section);
}

/// Release one active scan and admit only its latest weak pending request.
fn finish_child_scan(
    section: &LushtextWorkspaceSection,
    store_key: usize,
    ticket: ChildScanTicket,
) {
    let active = {
        let mut active_scans = section.imp().child_active_scans.borrow_mut();
        let matches = active_scans
            .get(&store_key)
            .is_some_and(|active| active.ticket == ticket);
        matches.then(|| active_scans.remove(&store_key)).flatten()
    };
    if let Some(active) = active.as_ref() {
        cancel_store_reconciliation_if_token(section, store_key, &active.token);
    }
    let finish = section
        .imp()
        .child_scan_flights
        .borrow_mut()
        .get_mut(&store_key)
        .map_or(ChildScanFinish::Stale, |flight| flight.finish(ticket));
    let latest = match finish {
        ChildScanFinish::StartLatest(latest) => {
            let request = section
                .imp()
                .child_pending_scans
                .borrow_mut()
                .remove(&store_key)
                .filter(|request| request.ticket == latest);
            if request.is_none()
                && let Some(flight) = section
                    .imp()
                    .child_scan_flights
                    .borrow_mut()
                    .get_mut(&store_key)
            {
                flight.cancel_all();
            }
            request
        }
        ChildScanFinish::Stale | ChildScanFinish::Terminal => None,
    };
    sync_child_scan_busy_state(section);
    if let Some(request) = latest {
        start_child_scan(section, store_key, request);
    }
}

/// Mirror child directory scan activity into the tree's accessible busy state.
pub(super) fn sync_child_scan_busy_state(section: &LushtextWorkspaceSection) {
    let busy = child_scan_blocks_readiness(section);
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

pub(super) fn child_scan_blocks_readiness(section: &LushtextWorkspaceSection) -> bool {
    !section.imp().child_active_scans.borrow().is_empty()
        || !section.imp().child_pending_scans.borrow().is_empty()
        || !section.imp().child_admission_scans.borrow().is_empty()
        || !section.imp().child_reconcile_sources.borrow().is_empty()
        || !section.imp().folder_empty_active.borrow().is_empty()
        || !section.imp().folder_empty_pending.borrow().is_empty()
        || !section.imp().folder_empty_admission.borrow().is_empty()
        || !section
            .imp()
            .refresh_runtime
            .scan_dispatch_queue
            .borrow()
            .is_empty()
        || section
            .imp()
            .refresh_runtime
            .scan_dispatch_source
            .borrow()
            .is_some()
}

/// Check whether a pending child-scan callback still belongs to the current expanded row.
fn child_scan_is_active(
    section: &LushtextWorkspaceSection,
    store_key: usize,
    ticket: ChildScanTicket,
    store: &gio::ListStore,
    token: &Arc<AtomicBool>,
) -> bool {
    if token.load(Ordering::Acquire) {
        return false;
    }
    if child_store_key(store) != store_key
        || !child_store_identity_matches(section, store_key, store)
    {
        return false;
    }
    let lifetime = section.imp().child_scan_lifetime.get();
    let owns_current_flight = section
        .imp()
        .child_scan_flights
        .borrow()
        .get(&store_key)
        .is_some_and(|flight| flight.is_current(ticket, lifetime));
    let current_path = section
        .imp()
        .child_store_paths
        .borrow()
        .get(&store_key)
        .cloned();
    let owns_active_payload = section
        .imp()
        .child_active_scans
        .borrow()
        .get(&store_key)
        .is_some_and(|active| {
            active.ticket == ticket
                && current_path.as_ref() == Some(&active.path)
                && Arc::ptr_eq(&active.token, token)
        });
    owns_current_flight && owns_active_payload
}

fn child_store_key(store: &gio::ListStore) -> usize {
    store.as_ptr() as usize
}

/// Reject state left behind when GLib reuses a released store's raw address.
fn ensure_child_store_identity(section: &LushtextWorkspaceSection, store: &gio::ListStore) {
    let store_key = child_store_key(store);
    if child_store_identity_matches(section, store_key, store) {
        return;
    }

    cancel_child_store_runtime(section, store_key);
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

fn child_store_identity_matches(
    section: &LushtextWorkspaceSection,
    store_key: usize,
    store: &gio::ListStore,
) -> bool {
    section
        .imp()
        .child_store_refs
        .borrow()
        .get(&store_key)
        .and_then(glib::WeakRef::upgrade)
        .is_some_and(|registered| registered.as_ptr() == store.as_ptr())
}

fn cancel_child_store_runtime(section: &LushtextWorkspaceSection, store_key: usize) {
    section
        .imp()
        .child_pending_scans
        .borrow_mut()
        .remove(&store_key);
    section
        .imp()
        .child_admission_scans
        .borrow_mut()
        .remove(&store_key);
    if let Some(active) = section
        .imp()
        .child_active_scans
        .borrow_mut()
        .remove(&store_key)
    {
        active.token.store(true, Ordering::Release);
    }
    if let Some(mut flight) = section
        .imp()
        .child_scan_flights
        .borrow_mut()
        .remove(&store_key)
    {
        flight.cancel_all();
    }
    cancel_store_reconciliation(section, store_key);
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
    let generation = advance_child_mirror_generation(section, store_key);
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

fn cancel_store_reconciliation(section: &LushtextWorkspaceSection, store_key: usize) -> bool {
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
        true
    } else {
        false
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

fn truncated_directory_label() -> String {
    format!("{MAX_DIR_ENTRIES}+ items - showing first {MAX_DIR_ENTRIES}")
}

/// Owned worker result needed by one current direct or bounded GTK application.
struct ScannedChildrenPlan {
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    ticket: ChildScanTicket,
    mirror_generation: u64,
    plan: DirectoryReconciliationPlan,
    truncated: bool,
}

/// Apply one worker-computed plan through the direct or bounded GTK path.
fn apply_scanned_children(section: &LushtextWorkspaceSection, scanned: ScannedChildrenPlan) {
    let ScannedChildrenPlan {
        store,
        dir_path,
        token,
        ticket,
        mirror_generation,
        plan,
        truncated,
    } = scanned;
    if truncated {
        tracing::warn!("Directory truncated to {MAX_DIR_ENTRIES} entries");
    }

    let store_key = child_store_key(&store);
    if !child_scan_is_active(section, store_key, ticket, &store, &token)
        || child_mirror_generation(section, &store) != mirror_generation
    {
        finish_child_scan(section, store_key, ticket);
        return;
    }
    retire_removed_child_subtrees(section, &plan);

    match plan {
        DirectoryReconciliationPlan::Unchanged => {
            finish_child_reconciliation(section, &store, &dir_path, &token, ticket);
        }
        DirectoryReconciliationPlan::Splice {
            position,
            removed,
            replacement,
            ..
        } if removed.saturating_add(replacement.len()) <= CHILD_RECONCILE_DIRECT_ROWS => {
            let items = build_child_items(&replacement);
            splice_child_store(section, &store, position, removed, &replacement, &items);
            finish_child_reconciliation(section, &store, &dir_path, &token, ticket);
        }
        DirectoryReconciliationPlan::Splice {
            position,
            removed,
            replacement,
            ..
        } => {
            let progress = Rc::new(RefCell::new(ChildReconcileProgress {
                position,
                remove_remaining: removed,
                inserted: 0,
                replacement: VecDeque::from(replacement),
                mirror_generation,
            }));
            apply_next_reconcile_batch(section, store, dir_path, token, ticket, progress);
        }
    }
}

/// Cancel materialized descendants that disappear from an accepted parent splice.
fn retire_removed_child_subtrees(
    section: &LushtextWorkspaceSection,
    plan: &DirectoryReconciliationPlan,
) {
    let DirectoryReconciliationPlan::Splice {
        removed_directory_roots,
        ..
    } = plan
    else {
        return;
    };
    clear_dir_states(section, &removed_directory_roots.iter().cloned().collect());
}

fn apply_next_reconcile_batch(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    ticket: ChildScanTicket,
    progress: Rc<RefCell<ChildReconcileProgress>>,
) {
    let expected_generation = progress.borrow().mirror_generation;
    let store_key = child_store_key(&store);
    if !child_scan_is_active(section, store_key, ticket, &store, &token)
        || child_mirror_generation(section, &store) != expected_generation
    {
        finish_child_scan(section, store_key, ticket);
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
        finish_child_reconciliation(section, &store, &dir_path, &token, ticket);
        return;
    }
    schedule_next_reconcile_batch(section, store, dir_path, token, ticket, progress);
}

fn schedule_next_reconcile_batch(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    ticket: ChildScanTicket,
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
        apply_next_reconcile_batch(&section, store, dir_path, callback_token, ticket, progress);
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
    ticket: ChildScanTicket,
) {
    let store_key = child_store_key(store);
    if !child_scan_is_active(section, store_key, ticket, store, token) {
        finish_child_scan(section, store_key, ticket);
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
        finish_child_scan(section, store_key, ticket);
        return;
    }
    schedule_child_state_restore(section);
    section.sync_file_row_states();
    let refresh = &section.imp().refresh_runtime;
    refresh
        .reconcile_terminal_count
        .set(refresh.reconcile_terminal_count.get().saturating_add(1));
    refresh
        .scan_terminal_publications
        .set(refresh.scan_terminal_publications.get().saturating_add(1));
    finish_child_scan(section, store_key, ticket);
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

/// Restore expansion and pending selection after a refresh may replace rows.
///
/// Shared by watcher reconciliation and targeted in-place refresh; both defer
/// one main-loop tick so `GtkTreeListModel` row recycling settles first.
pub(super) fn schedule_child_state_restore(section: &LushtextWorkspaceSection) {
    if section.imp().expanded_paths.borrow().is_empty()
        && section.imp().pending_selection.borrow().is_none()
    {
        return;
    }

    let section_weak = section.downgrade();
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        if let Some(section) = section_weak.upgrade() {
            // Read expansion intent at apply time: the authoritative set is
            // live, so a collapse between scheduling and this callback must
            // not be resurrected by a stale snapshot.
            let expanded_paths = section.imp().expanded_paths.borrow().clone();
            for path in expanded_paths {
                if let Some(row) = section.find_dir_row(&path) {
                    row.set_expanded(true);
                }
            }
            let pending_selection = section.imp().pending_selection.borrow().clone();
            if let Some(path) = pending_selection {
                section.select_and_scroll_to(&path);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Child-store index and item-cache maintenance.
//
// Dissolved in from the pre-convention `tree_index.rs`. Its recorded destination
// split this content between `policy.rs` (pure index arithmetic) and here (cache
// maintenance); re-reading the file found **no pure arithmetic left in it** — the
// splice planning and common prefix/suffix logic already live in
// `services::file_tree`. So the whole module lands here, in the one role that
// materializes child stores and therefore owns their caches.
//
// Two contracts in this block are load-bearing and easy to break by moving code:
//
// * `derive_expanded_paths_from_model` is the **full** derivation and is reserved
//   for bootstrap, pre-replacement capture, and the test oracle. A targeted
//   in-place refresh MUST NOT rewalk the flattened model to rediscover expansion,
//   and the evidence surface MUST NOT call it at all — it advances the very capture
//   counters the surface reports.
// * `find_store_for_dir` and `find_dir_row` **mutate** their caches on what looks
//   like a read, and `visible_child_stores` calls `row.children()` with no
//   `is_expanded()` filter, which materializes children. These are the reasons the
//   evidence surface derives from `expanded_paths` instead.
// ---------------------------------------------------------------------------

impl LushtextWorkspaceSection {
    /// Derive the complete expanded-path set by walking the flattened model.
    ///
    /// This full scan is reserved for bootstrap, pre-replacement capture, and
    /// the test oracle. Targeted in-place refresh relies on the live
    /// `expanded_paths` set maintained by row expansion transitions and
    /// accepted reconciliation instead.
    pub(super) fn derive_expanded_paths_from_model(&self) -> Option<HashSet<PathBuf>> {
        let tree_model = self.imp().tree_model.borrow().as_ref().cloned()?;
        let runtime = &self.imp().refresh_runtime;
        runtime
            .expansion_capture_scans
            .set(runtime.expansion_capture_scans.get().saturating_add(1));
        runtime.expansion_capture_rows.set(
            runtime
                .expansion_capture_rows
                .get()
                .saturating_add(u64::from(tree_model.n_items())),
        );

        let mut expanded = HashSet::new();
        for i in 0..tree_model.n_items() {
            if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                && row.is_expanded()
                && let Some(item) = row.item().and_downcast::<FileTreeItem>()
                && let Some(path) = item.path()
            {
                expanded.insert(path);
            }
        }
        Some(expanded)
    }

    /// Replace the live expansion set with a full flattened-model derivation.
    ///
    /// Call this only before a genuine model replacement or broad reload. The
    /// snapshot replaces (never unions with) the current state so a rebuild
    /// cannot re-expand rows the user has since collapsed.
    pub(super) fn save_expanded_paths(&self) {
        if let Some(derived) = self.derive_expanded_paths_from_model() {
            *self.imp().expanded_paths.borrow_mut() = derived;
        }
    }

    /// Mirror one live row expansion transition into the authoritative set.
    pub(super) fn record_row_expansion_transition(&self, row: &gtk4::TreeListRow) {
        // Rows being destroyed by a splice or an ancestor collapse can still
        // emit property notifications; only rows still present in the flattened
        // model carry user expansion intent.
        if row.position() == gtk4::INVALID_LIST_POSITION {
            return;
        }
        let Some(path) = row
            .item()
            .and_downcast::<FileTreeItem>()
            .filter(FileTreeItem::is_dir)
            .and_then(|item| item.path())
        else {
            return;
        };
        let mut expanded = self.imp().expanded_paths.borrow_mut();
        if row.is_expanded() {
            expanded.insert(path);
            return;
        }

        // Collapsing hides every descendant from the flattened model, which is
        // exactly what a whole-model expansion snapshot captures next:
        // descendant restoration intent does not survive an ancestor collapse.
        // Overlapping workspace folders can still show a pruned path through a
        // duplicate row, so ambiguous candidates fall back to a subtree-scoped
        // model reconciliation instead of a blind prefix prune.
        let mut needs_model_reconcile = false;
        let subtree = expanded
            .iter()
            .filter(|candidate| candidate.starts_with(&path))
            .cloned()
            .collect::<Vec<_>>();
        for candidate in subtree {
            if self.visible_path_is_ambiguous(&candidate) {
                needs_model_reconcile = true;
            } else {
                expanded.remove(&candidate);
            }
        }
        drop(expanded);
        if needs_model_reconcile {
            self.reconcile_expanded_subtree_from_model(&path);
        }
    }

    /// Re-derive expansion intent under one prefix from the flattened model.
    ///
    /// This duplicate-aware fallback runs only when a collapsed subtree
    /// contains paths that other visible rows may still show expanded.
    fn reconcile_expanded_subtree_from_model(&self, prefix: &Path) {
        let Some(tree_model) = self.imp().tree_model.borrow().as_ref().cloned() else {
            return;
        };
        let mut expanded = self.imp().expanded_paths.borrow_mut();
        expanded.retain(|candidate| !candidate.starts_with(prefix));
        for i in 0..tree_model.n_items() {
            if let Some(row) = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()
                && row.is_expanded()
                && let Some(item) = row.item().and_downcast::<FileTreeItem>()
                && let Some(path) = item.path()
                && path.starts_with(prefix)
            {
                expanded.insert(path);
            }
        }
    }

    /// Rewrite expansion intent for a renamed directory subtree.
    ///
    /// The renamed rows stay expanded in place because inline rename mutates
    /// item paths without a splice, so their restoration intent must follow
    /// the new prefix instead of being retired with the old one.
    pub(super) fn rename_expanded_subtree(&self, old_path: &Path, new_path: &Path) {
        let mut expanded = self.imp().expanded_paths.borrow_mut();
        let moved = expanded
            .iter()
            .filter(|path| path.starts_with(old_path))
            .cloned()
            .collect::<Vec<_>>();
        for path in moved {
            expanded.remove(&path);
            match path.strip_prefix(old_path) {
                Ok(suffix) if suffix.as_os_str().is_empty() => {
                    expanded.insert(new_path.to_path_buf());
                }
                Ok(suffix) => {
                    expanded.insert(new_path.join(suffix));
                }
                Err(_) => {
                    expanded.insert(path);
                }
            }
        }
    }

    /// Run the full flattened-model expansion derivation as a test oracle.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn derived_expanded_paths_for_test(&self) -> Option<HashSet<PathBuf>> {
        self.derive_expanded_paths_from_model()
    }

    pub(super) fn reset_item_cache(&self) {
        self.imp().folder_paths.borrow_mut().clear();
        self.imp().child_paths.borrow_mut().clear();
        self.imp().visible_path_counts.borrow_mut().clear();
        self.imp().item_locations.borrow_mut().clear();
    }

    pub(super) fn cache_top_level_item(&self, path: PathBuf, index: usize) {
        let imp = self.imp();
        let mut folder_paths = imp.folder_paths.borrow_mut();
        let insert_at = index.min(folder_paths.len());
        folder_paths.insert(insert_at, path.clone());
        drop(folder_paths);
        self.shift_cached_indices(None, insert_at, 1);
        self.cache_item_location(
            path,
            ItemLocation {
                parent_dir: None,
                index: insert_at,
            },
        );
    }

    /// Rebuild the item cache from the current top-level folder `ListStore` contents.
    pub(super) fn recache_top_level_store(&self, store: &gio::ListStore) {
        let old_paths = self.imp().folder_paths.borrow().clone();
        self.imp().folder_paths.borrow_mut().clear();
        self.imp()
            .item_locations
            .borrow_mut()
            .retain(|_, location| location.parent_dir.is_some());
        for path in old_paths {
            self.forget_visible_path_occurrence(&path);
        }

        for index in 0..store.n_items() {
            if let Some(item) = store.item(index).and_downcast::<FileTreeItem>()
                && let Some(path) = item.path()
            {
                self.cache_top_level_item(path, index as usize);
            }
        }
    }

    /// Cache a child row's parent directory and index for O(1) later lookup.
    pub(super) fn cache_child_item(&self, parent_dir: &Path, path: PathBuf, index: usize) {
        let imp = self.imp();
        let parent_key = parent_dir.to_path_buf();
        let mut child_paths = imp.child_paths.borrow_mut();
        let siblings = child_paths.entry(parent_key.clone()).or_default();
        let insert_at = index.min(siblings.len());
        siblings.insert(insert_at, path.clone());
        drop(child_paths);
        self.shift_cached_indices(Some(parent_dir), insert_at, 1);
        self.cache_item_location(
            path,
            ItemLocation {
                parent_dir: Some(parent_key),
                index: insert_at,
            },
        );
    }

    /// Rebuild the direct-child cache for one directory from the current
    /// `ListStore` contents after a refresh splice.
    pub(super) fn recache_child_store(&self, parent_dir: &Path, store: &gio::ListStore) {
        let rows = (0..store.n_items())
            .filter_map(|index| store.item(index).and_downcast::<FileTreeItem>())
            .map(|item| row_state_from_item(&item))
            .collect::<Vec<_>>();
        self.recache_child_rows_from_mirror(parent_dir, &rows);
    }

    /// Rebuild direct-child path caches from the accepted plain store mirror.
    pub(super) fn recache_child_rows_from_mirror(
        &self,
        parent_dir: &Path,
        rows: &[DirectoryRowState],
    ) {
        let new_occurrences = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.path.clone().map(|path| (index, path)))
            .collect::<Vec<_>>();
        let imp = self.imp();
        let metrics = replace_child_cache(
            &imp.folder_paths.borrow(),
            &mut imp.child_paths.borrow_mut(),
            &mut imp.visible_path_counts.borrow_mut(),
            &mut imp.item_locations.borrow_mut(),
            parent_dir,
            &new_occurrences,
        );
        imp.refresh_runtime
            .cache_rebuild_input_rows
            .set(metrics.input_rows);
        imp.refresh_runtime
            .cache_rebuild_operations
            .set(metrics.operations);
    }

    pub(super) fn rename_cached_item(&self, old_path: &Path, new_path: &Path) {
        let Some(location) = self.imp().item_locations.borrow_mut().remove(old_path) else {
            return;
        };

        match location.parent_dir.as_deref() {
            None => {
                if let Some(cached) = self.imp().folder_paths.borrow_mut().get_mut(location.index) {
                    *cached = new_path.to_path_buf();
                }
            }
            Some(parent_dir) => {
                if let Some(siblings) = self.imp().child_paths.borrow_mut().get_mut(parent_dir)
                    && let Some(cached) = siblings.get_mut(location.index)
                {
                    *cached = new_path.to_path_buf();
                }
            }
        }

        if let Some(parent_dir) = location.parent_dir.as_deref()
            && let Some(store) = self.find_store_for_dir(parent_dir)
        {
            record_child_store_path_update(self, &store, location.index, new_path);
        }
        self.forget_visible_path_occurrence(old_path);
        self.cache_item_location(new_path.to_path_buf(), location);
    }

    pub(super) fn append_item_preserving_placeholder(
        &self,
        store: &gio::ListStore,
        parent_dir: &Path,
        item: &FileTreeItem,
    ) {
        let insert_pos = store
            .n_items()
            .checked_sub(1)
            .and_then(|idx| store.item(idx))
            .and_then(|obj| obj.downcast::<FileTreeItem>().ok())
            .filter(FileTreeItem::is_placeholder)
            .map_or_else(|| store.n_items(), |_| store.n_items() - 1);

        if insert_pos == store.n_items() {
            store.append(item);
        } else {
            store.insert(insert_pos, item);
        }
        record_child_store_insert(self, store, insert_pos as usize, std::slice::from_ref(item));

        if let Some(path) = item.path() {
            self.cache_child_item(parent_dir, path, insert_pos as usize);
        }
    }

    /// Resolve the live `TreeListRow` for a directory path, reusing the weak cache when possible.
    pub(super) fn find_dir_row(&self, dir_path: &Path) -> Option<gtk4::TreeListRow> {
        if let Some(row) = self
            .imp()
            .dir_rows
            .borrow()
            .get(dir_path)
            .cloned()
            .and_then(|weak| weak.upgrade())
        {
            let matches = row
                .item()
                .and_downcast::<FileTreeItem>()
                .is_some_and(|item| item.is_dir() && item.path().as_deref() == Some(dir_path));
            if matches {
                return Some(row);
            }
            self.imp().dir_rows.borrow_mut().remove(dir_path);
        }

        let tree_model = self.imp().tree_model.borrow();
        let tree_model = tree_model.as_ref()?;
        for i in 0..tree_model.n_items() {
            let row = tree_model.item(i).and_downcast::<gtk4::TreeListRow>()?;
            let item = row.item().and_downcast::<FileTreeItem>()?;
            if item.is_dir() && item.path().as_deref() == Some(dir_path) {
                self.imp()
                    .dir_rows
                    .borrow_mut()
                    .insert(dir_path.to_path_buf(), row.downgrade());
                return Some(row);
            }
        }
        None
    }

    pub(super) fn find_store_for_dir(&self, dir_path: &Path) -> Option<gio::ListStore> {
        if let Some(store) = self
            .imp()
            .dir_stores
            .borrow()
            .get(dir_path)
            .and_then(glib::WeakRef::upgrade)
        {
            return Some(store);
        }

        let store = self
            .find_dir_row(dir_path)?
            .children()
            .and_then(|m| m.downcast::<gio::ListStore>().ok())?;
        self.imp()
            .dir_stores
            .borrow_mut()
            .insert(dir_path.to_path_buf(), store.downgrade());
        Some(store)
    }

    /// Remove an item from the tree model by path. Returns true if found and removed.
    #[must_use]
    pub fn remove_from_model(&self, target_path: &Path) -> bool {
        let imp = self.imp();
        clear_dir_state(self, target_path);
        let mut removed_any = false;
        let may_have_multiple_visible_rows = self.visible_path_is_ambiguous(target_path);

        if let Some(location) = self.remove_cached_item(target_path) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "The sidebar child store is bounded to realistic directory sizes before converting to u32"
            )]
            let idx = location.index as u32;
            match location.parent_dir.as_deref() {
                None if let Some(ref top_level_store) = *imp.top_level_store.borrow()
                    && idx < top_level_store.n_items() =>
                {
                    top_level_store.remove(idx);
                    removed_any = true;
                }
                Some(parent_dir)
                    if let Some(store) = self.find_store_for_dir(parent_dir)
                        && idx < store.n_items() =>
                {
                    store.remove(idx);
                    record_child_store_remove(self, &store, idx as usize, 1);
                    removed_any = true;
                }
                _ => {}
            }
        }

        if may_have_multiple_visible_rows || !removed_any {
            if let Some(ref top_level_store) = *imp.top_level_store.borrow()
                && remove_matching_items(self, top_level_store, target_path, false)
            {
                self.recache_top_level_store(top_level_store);
                removed_any = true;
            }

            let child_stores = imp
                .dir_stores
                .borrow()
                .iter()
                .filter_map(|(parent_dir, weak_store)| {
                    weak_store
                        .upgrade()
                        .map(|store| (parent_dir.clone(), store))
                })
                .collect::<Vec<_>>();

            for (parent_dir, store) in child_stores {
                if remove_matching_items(self, &store, target_path, true) {
                    self.recache_child_store(&parent_dir, &store);
                    removed_any = true;
                }
            }

            if !removed_any {
                for (parent_dir, store) in self.visible_child_stores() {
                    if remove_matching_items(self, &store, target_path, true) {
                        self.recache_child_store(&parent_dir, &store);
                        removed_any = true;
                    }
                }
            }
        }

        removed_any
    }

    fn visible_child_stores(&self) -> Vec<(PathBuf, gio::ListStore)> {
        let Some(tree_model) = self.imp().tree_model.borrow().as_ref().cloned() else {
            return Vec::new();
        };
        let mut stores = Vec::new();
        for index in 0..tree_model.n_items() {
            let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>() else {
                continue;
            };
            let Some(item) = row.item().and_downcast::<FileTreeItem>() else {
                continue;
            };
            let Some(parent_dir) = item.path() else {
                continue;
            };
            let Some(store) = row
                .children()
                .and_then(|model| model.downcast::<gio::ListStore>().ok())
            else {
                continue;
            };
            stores.push((parent_dir, store));
        }
        stores
    }

    fn shift_cached_indices(&self, parent_dir: Option<&Path>, start: usize, delta: isize) {
        let parent_key = parent_dir.map(Path::to_path_buf);
        for location in self.imp().item_locations.borrow_mut().values_mut() {
            if location.parent_dir == parent_key && location.index >= start {
                location.index = location.index.saturating_add_signed(delta);
            }
        }
    }

    fn remove_cached_item(&self, target_path: &Path) -> Option<ItemLocation> {
        let removed = self.imp().item_locations.borrow_mut().remove(target_path)?;
        match removed.parent_dir.as_deref() {
            None => {
                let mut folder_paths = self.imp().folder_paths.borrow_mut();
                let removed_index = if folder_paths
                    .get(removed.index)
                    .is_some_and(|path| path == target_path)
                {
                    folder_paths.remove(removed.index);
                    removed.index
                } else if let Some(position) =
                    folder_paths.iter().position(|path| path == target_path)
                {
                    folder_paths.remove(position);
                    position
                } else {
                    removed.index
                };
                drop(folder_paths);
                self.shift_cached_indices(None, removed_index.saturating_add(1), -1);
                self.forget_visible_path_occurrence(target_path);
            }
            Some(parent_dir) => {
                let mut child_paths = self.imp().child_paths.borrow_mut();
                let mut remove_parent = false;
                if let Some(siblings) = child_paths.get_mut(parent_dir) {
                    let removed_index = if siblings
                        .get(removed.index)
                        .is_some_and(|path| path == target_path)
                    {
                        siblings.remove(removed.index);
                        removed.index
                    } else if let Some(position) =
                        siblings.iter().position(|path| path == target_path)
                    {
                        siblings.remove(position);
                        position
                    } else {
                        removed.index
                    };
                    self.shift_cached_indices(
                        Some(parent_dir),
                        removed_index.saturating_add(1),
                        -1,
                    );
                    if siblings.is_empty() {
                        remove_parent = true;
                    }
                }
                if remove_parent {
                    child_paths.remove(parent_dir);
                }
                drop(child_paths);
                self.forget_visible_path_occurrence(target_path);
            }
        }
        Some(removed)
    }

    fn cache_item_location(&self, path: PathBuf, location: ItemLocation) {
        let count = {
            let mut counts = self.imp().visible_path_counts.borrow_mut();
            let count = counts.entry(path.clone()).or_insert(0);
            *count += 1;
            *count
        };

        let mut locations = self.imp().item_locations.borrow_mut();
        if count == 1 {
            locations.insert(path, location);
        } else {
            locations.remove(&path);
        }
    }

    pub(super) fn forget_visible_path_occurrence(&self, path: &Path) {
        let remaining = {
            let mut counts = self.imp().visible_path_counts.borrow_mut();
            let Some(count) = counts.get_mut(path) else {
                return;
            };
            if *count <= 1 {
                counts.remove(path);
                0
            } else {
                *count -= 1;
                *count
            }
        };

        self.imp().item_locations.borrow_mut().remove(path);
        if remaining == 1 {
            self.restore_unique_item_location(path);
        }
    }

    fn restore_unique_item_location(&self, path: &Path) {
        if let Some(index) = self
            .imp()
            .folder_paths
            .borrow()
            .iter()
            .position(|folder| folder.as_path() == path)
        {
            self.imp().item_locations.borrow_mut().insert(
                path.to_path_buf(),
                ItemLocation {
                    parent_dir: None,
                    index,
                },
            );
            return;
        }

        for (parent_dir, siblings) in self.imp().child_paths.borrow().iter() {
            if let Some(index) = siblings.iter().position(|child| child.as_path() == path) {
                self.imp().item_locations.borrow_mut().insert(
                    path.to_path_buf(),
                    ItemLocation {
                        parent_dir: Some(parent_dir.clone()),
                        index,
                    },
                );
                return;
            }
        }
    }

    fn visible_path_is_ambiguous(&self, path: &Path) -> bool {
        self.imp()
            .visible_path_counts
            .borrow()
            .get(path)
            .copied()
            .unwrap_or(0)
            > 1
    }
}

/// Bounded-cost evidence for one child-cache replacement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ChildCacheRebuildMetrics {
    input_rows: usize,
    operations: usize,
}

/// Atomically replace one accepted child mirror's cache projection.
///
/// Old and new sibling rows are each visited a bounded number of times. A
/// duplicate path that becomes globally unique may require one linear pass over
/// the other already-materialized rows to recover its sole location; crucially,
/// that recovery is shared rather than repeated once per inserted row.
fn replace_child_cache(
    folder_paths: &[PathBuf],
    child_paths: &mut HashMap<PathBuf, Vec<PathBuf>>,
    visible_path_counts: &mut HashMap<PathBuf, usize>,
    item_locations: &mut HashMap<PathBuf, ItemLocation>,
    parent_dir: &Path,
    new_occurrences: &[(usize, PathBuf)],
) -> ChildCacheRebuildMetrics {
    let old_paths = child_paths.remove(parent_dir).unwrap_or_default();
    let input_rows = old_paths.len().saturating_add(new_occurrences.len());
    let mut operations = old_paths.len();
    let new_paths = new_occurrences
        .iter()
        .map(|(_, path)| path.clone())
        .collect::<Vec<_>>();
    operations = operations.saturating_add(new_occurrences.len());

    let mut old_counts = HashMap::<PathBuf, usize>::new();
    for path in old_paths {
        *old_counts.entry(path).or_default() += 1;
    }
    let mut new_counts = HashMap::<PathBuf, usize>::new();
    for (_, path) in new_occurrences {
        *new_counts.entry(path.clone()).or_default() += 1;
    }
    operations = operations.saturating_add(new_occurrences.len());
    let affected = old_counts
        .keys()
        .chain(new_counts.keys())
        .cloned()
        .collect::<HashSet<_>>();

    for path in &affected {
        let previous = visible_path_counts.get(path).copied().unwrap_or(0);
        let next = previous
            .saturating_sub(old_counts.get(path).copied().unwrap_or(0))
            .saturating_add(new_counts.get(path).copied().unwrap_or(0));
        if next == 0 {
            visible_path_counts.remove(path);
        } else {
            visible_path_counts.insert(path.clone(), next);
        }
    }
    operations = operations.saturating_add(affected.len());

    for path in &affected {
        item_locations.remove(path);
    }
    operations = operations.saturating_add(affected.len());
    if !new_paths.is_empty() {
        child_paths.insert(parent_dir.to_path_buf(), new_paths);
    }

    let mut unresolved_unique = affected
        .into_iter()
        .filter(|path| visible_path_counts.get(path) == Some(&1))
        .collect::<HashSet<_>>();
    operations = operations.saturating_add(old_counts.len().saturating_add(new_counts.len()));
    for (index, path) in new_occurrences {
        if unresolved_unique.remove(path) {
            item_locations.insert(
                path.clone(),
                ItemLocation {
                    parent_dir: Some(parent_dir.to_path_buf()),
                    index: *index,
                },
            );
        }
    }
    operations = operations.saturating_add(new_occurrences.len());

    if unresolved_unique.is_empty() {
        return ChildCacheRebuildMetrics {
            input_rows,
            operations,
        };
    }
    for (index, path) in folder_paths.iter().enumerate() {
        operations = operations.saturating_add(1);
        if unresolved_unique.remove(path) {
            item_locations.insert(
                path.clone(),
                ItemLocation {
                    parent_dir: None,
                    index,
                },
            );
        }
    }
    for (other_parent, siblings) in child_paths.iter() {
        // Nothing ever re-enters `unresolved_unique`, so an empty set ends the
        // search rather than merely skipping this parent.
        if unresolved_unique.is_empty() {
            break;
        }
        if other_parent == parent_dir {
            continue;
        }
        for (index, path) in siblings.iter().enumerate() {
            operations = operations.saturating_add(1);
            if unresolved_unique.remove(path) {
                item_locations.insert(
                    path.clone(),
                    ItemLocation {
                        parent_dir: Some(other_parent.clone()),
                        index,
                    },
                );
            }
        }
    }
    debug_assert!(
        unresolved_unique.is_empty(),
        "visible occurrence counts must resolve every globally unique path"
    );
    ChildCacheRebuildMetrics {
        input_rows,
        operations,
    }
}

pub(super) fn child_cache_rebuild_operation_evidence(row_count: usize) -> (usize, usize) {
    let parent = PathBuf::from("/benchmark/parent");
    let old_paths = (0..row_count)
        .map(|index| PathBuf::from(format!("/benchmark/old-{index}")))
        .collect::<Vec<_>>();
    let new_occurrences = (0..row_count)
        .map(|index| (index, PathBuf::from(format!("/benchmark/new-{index}"))))
        .collect::<Vec<_>>();
    let mut child_paths = HashMap::from([(parent.clone(), old_paths.clone())]);
    let mut visible_path_counts = old_paths
        .iter()
        .map(|path| (path.clone(), 1usize))
        .collect::<HashMap<_, _>>();
    let mut item_locations = old_paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            (
                path,
                ItemLocation {
                    parent_dir: Some(parent.clone()),
                    index,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let metrics = replace_child_cache(
        &[],
        &mut child_paths,
        &mut visible_path_counts,
        &mut item_locations,
        &parent,
        &new_occurrences,
    );
    (metrics.input_rows, metrics.operations)
}

fn remove_matching_items(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    target_path: &Path,
    child_store: bool,
) -> bool {
    let mut removed_any = false;
    // Remove from the tail so each deletion cannot shift an unvisited row's index.
    for index in (0..store.n_items()).rev() {
        if let Some(item) = store.item(index).and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
        {
            store.remove(index);
            if child_store {
                record_child_store_remove(section, store, index as usize, 1);
            }
            removed_any = true;
        }
    }
    removed_any
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use proptest::prelude::*;

    fn test_path(id: u8) -> PathBuf {
        PathBuf::from(format!("/workspace/path-{id}"))
    }

    fn oracle(
        folder_paths: &[PathBuf],
        child_rows: &[(PathBuf, Vec<(usize, PathBuf)>)],
    ) -> (HashMap<PathBuf, usize>, HashMap<PathBuf, ItemLocation>) {
        let mut occurrences = HashMap::<PathBuf, Vec<ItemLocation>>::new();
        for (index, path) in folder_paths.iter().enumerate() {
            occurrences
                .entry(path.clone())
                .or_default()
                .push(ItemLocation {
                    parent_dir: None,
                    index,
                });
        }
        for (parent, rows) in child_rows {
            for (index, path) in rows {
                occurrences
                    .entry(path.clone())
                    .or_default()
                    .push(ItemLocation {
                        parent_dir: Some(parent.clone()),
                        index: *index,
                    });
            }
        }
        let counts = occurrences
            .iter()
            .map(|(path, locations)| (path.clone(), locations.len()))
            .collect();
        let locations = occurrences
            .into_iter()
            .filter_map(|(path, locations)| {
                (locations.len() == 1).then(|| (path, locations[0].clone()))
            })
            .collect();
        (counts, locations)
    }

    proptest! {
        #[test]
        fn bulk_child_cache_matches_occurrence_oracle_across_duplicates_and_reorders(
            folder_ids in prop::collection::vec(0u8..8, 0..12),
            other_ids in prop::collection::vec(0u8..8, 0..24),
            old_ids in prop::collection::vec(prop::option::of(0u8..8), 0..40),
            new_ids in prop::collection::vec(prop::option::of(0u8..8), 0..40),
        ) {
            let parent = PathBuf::from("/workspace/parent");
            let other_parent = PathBuf::from("/workspace/other");
            let folder_paths = folder_ids.into_iter().map(test_path).collect::<Vec<_>>();
            let old_rows = old_ids
                .into_iter()
                .enumerate()
                .filter_map(|(index, id)| id.map(|id| (index, test_path(id))))
                .collect::<Vec<_>>();
            let new_rows = new_ids
                .into_iter()
                .enumerate()
                .filter_map(|(index, id)| id.map(|id| (index, test_path(id))))
                .collect::<Vec<_>>();
            let other_rows = other_ids
                .into_iter()
                .enumerate()
                .map(|(index, id)| (index, test_path(id)))
                .collect::<Vec<_>>();

            let (mut counts, mut locations) = oracle(
                &folder_paths,
                &[
                    (parent.clone(), old_rows.clone()),
                    (other_parent.clone(), other_rows.clone()),
                ],
            );
            let mut child_paths = HashMap::from([
                (
                    parent.clone(),
                    old_rows.iter().map(|(_, path)| path.clone()).collect(),
                ),
                (
                    other_parent.clone(),
                    other_rows.iter().map(|(_, path)| path.clone()).collect(),
                ),
            ]);

            replace_child_cache(
                &folder_paths,
                &mut child_paths,
                &mut counts,
                &mut locations,
                &parent,
                &new_rows,
            );

            let (expected_counts, expected_locations) = oracle(
                &folder_paths,
                &[
                    (parent.clone(), new_rows.clone()),
                    (other_parent.clone(), other_rows.clone()),
                ],
            );
            prop_assert_eq!(counts, expected_counts);
            prop_assert_eq!(locations, expected_locations);
            prop_assert_eq!(
                child_paths.get(&other_parent),
                Some(&other_rows.iter().map(|(_, path)| path.clone()).collect::<Vec<_>>())
            );
            if new_rows.is_empty() {
                prop_assert!(!child_paths.contains_key(&parent));
            } else {
                prop_assert_eq!(
                    child_paths.get(&parent),
                    Some(&new_rows.iter().map(|(_, path)| path.clone()).collect::<Vec<_>>())
                );
            }
        }
    }
}

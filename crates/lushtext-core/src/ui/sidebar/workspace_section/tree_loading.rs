// SPDX-License-Identifier: GPL-3.0-or-later

//! Async child-tree loading helpers for `LushtextWorkspaceSection`.
//!
//! This stays in the driving adapter layer because it constructs GTK models
//! (`gio::ListStore`) and schedules main-loop batch appends. The filesystem
//! scan itself still lives in `services::file_tree` as plain Rust data.

use super::LushtextWorkspaceSection;
use crate::model::workspace_scan::{
    WorkspaceScanFinish as ChildScanFinish, WorkspaceScanSubmission as ChildScanSubmission,
    WorkspaceScanTicket as ChildScanTicket,
};
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
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
/// Process-wide child/emptiness tasks allowed to retain admitted scan payloads.
pub(super) const WORKSPACE_SCAN_TASK_LIMIT: usize = 4;
/// Compact admission retries share one frame-paced source per section.
const WORKSPACE_SCAN_ADMISSION_RETRY: Duration = Duration::from_millis(16);

static ACTIVE_WORKSPACE_SCAN_TASKS: AtomicUsize = AtomicUsize::new(0);
static WORKSPACE_SCAN_TASK_HIGH_WATER: AtomicUsize = AtomicUsize::new(0);

pub(super) struct WorkspaceScanPermit;

impl Drop for WorkspaceScanPermit {
    fn drop(&mut self) {
        ACTIVE_WORKSPACE_SCAN_TASKS.fetch_sub(1, Ordering::Release);
    }
}

pub(super) fn try_acquire_workspace_scan_permit() -> Option<WorkspaceScanPermit> {
    let admitted = ACTIVE_WORKSPACE_SCAN_TASKS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
            (active < WORKSPACE_SCAN_TASK_LIMIT).then_some(active + 1)
        })
        .ok()?;
    WORKSPACE_SCAN_TASK_HIGH_WATER.fetch_max(admitted + 1, Ordering::AcqRel);
    Some(WorkspaceScanPermit)
}

#[cfg(feature = "test-utils")]
pub(super) fn workspace_scan_active_tasks_for_test() -> usize {
    ACTIVE_WORKSPACE_SCAN_TASKS.load(Ordering::Acquire)
}

#[cfg(feature = "test-utils")]
pub(super) fn workspace_scan_task_high_water_for_test() -> usize {
    WORKSPACE_SCAN_TASK_HIGH_WATER.load(Ordering::Acquire)
}

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
fn start_child_scan(
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
    let Some(permit) = try_acquire_workspace_scan_permit() else {
        section
            .imp()
            .child_admission_scans
            .borrow_mut()
            .insert(store_key, request);
        arm_workspace_scan_admission_retry(section);
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

pub(super) fn arm_workspace_scan_admission_retry(section: &LushtextWorkspaceSection) {
    if section
        .imp()
        .workspace_scan_admission_source
        .borrow()
        .is_some()
    {
        return;
    }
    let section_weak = section.downgrade();
    let source = glib::timeout_add_local(WORKSPACE_SCAN_ADMISSION_RETRY, move || {
        let Some(section) = section_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        retry_workspace_scan_admission(&section)
    });
    section
        .imp()
        .workspace_scan_admission_source
        .replace(Some(source));
}

fn retry_workspace_scan_admission(section: &LushtextWorkspaceSection) -> glib::ControlFlow {
    for _ in 0..WORKSPACE_SCAN_TASK_LIMIT {
        let child_key = section
            .imp()
            .child_admission_scans
            .borrow()
            .keys()
            .next()
            .copied();
        let child = child_key.and_then(|key| {
            section
                .imp()
                .child_admission_scans
                .borrow_mut()
                .remove(&key)
                .map(|request| (key, request))
        });
        if let Some((store_key, request)) = child {
            start_child_scan(section, store_key, request);
        } else if !super::folders::retry_one_folder_empty_admission(section) {
            break;
        }
        if ACTIVE_WORKSPACE_SCAN_TASKS.load(Ordering::Acquire) >= WORKSPACE_SCAN_TASK_LIMIT {
            break;
        }
    }

    let waiting = !section.imp().child_admission_scans.borrow().is_empty()
        || !section.imp().folder_empty_admission.borrow().is_empty();
    if waiting {
        glib::ControlFlow::Continue
    } else {
        section.imp().workspace_scan_admission_source.take();
        glib::ControlFlow::Break
    }
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
    super::folders::cancel_folder_empty_probes_under_roots(section, roots);
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

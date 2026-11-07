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
use crate::model::list_budget::turn_continues;
use crate::model::workspace_visibility::WorkspaceEntryVisibility;
use crate::services;
use crate::services::file_tree::{
    DirectoryPlaceholder, DirectoryReconciliationPlan, DirectoryRowState,
    plan_directory_reconciliation,
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
    /// The budget row's counts when the retained-byte budget cut the listing.
    limited: Option<(usize, usize)>,
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
    visibility: WorkspaceEntryVisibility,
}

struct ChildReconcileProgress {
    position: usize,
    remove_remaining: usize,
    inserted: usize,
    replacement: VecDeque<DirectoryRowState>,
    mirror_generation: u64,
    /// The loading row that ends the directory while batches remain, and the
    /// listing's total for its label (`bound-lists-by-budget-not-count` D6).
    loading_row: Option<(FileTreeItem, usize)>,
    /// Whether the directory was empty when this load began. Its first rows
    /// make `GtkListView` create row widgets, up to its realized-row cap
    /// (about 205, A1), and each costs a setup, a bind and a first layout:
    /// one 256-row batch created about 200 of them in one dispatch (19 ms,
    /// and a 19 ms frame after it, at 10,000 entries; `main` the same). So
    /// the first `CHILD_APPEND_BATCH_SIZE` rows go in batches of
    /// `FIRST_BATCH_ROWS`, the turn's time slice stops after a few of them,
    /// and the load waits past one frame after the very first.
    warming_up: bool,
    /// Rows a previous store of this directory left in the path caches, when
    /// this load's store took the directory over (a collapse and re-expansion,
    /// or GTK's discarded `is_expandable()` probe store, A22). They are
    /// forgotten in time-sliced chunks before the first batch, instead of in
    /// one dispatch that grows with the previous listing.
    stale_cache_rows: Vec<PathBuf>,
    /// Rows to reserve in the caches once `stale_cache_rows` is forgotten.
    reserve_rows: Option<usize>,
}

/// The retained-byte budget a directory scan applies: the workflow policy's,
/// unless a widget test or a smoke binary lowers it.
fn directory_retained_bytes() -> u64 {
    let mut bytes = crate::ui::sidebar::policy::SIDEBAR_DIRECTORY_RETAINED_BYTES;
    if let Some(smoke) = crate::services::smoke_list_budgets::sidebar_directory_retained_bytes() {
        bytes = smoke;
    }
    #[cfg(feature = "test-utils")]
    if let Some(test) = crate::ui::sidebar::test_policy::directory_retained_bytes_override() {
        bytes = test;
    }
    bytes
}

/// Rows one batch splices when populating a directory tree. A turn applies
/// batches while `model::list_budget::turn_continues` holds; one batch splice
/// costs well under a millisecond however large the store (GTK axiom A88).
///
/// Kind: cap; a power of two, rounded down.
const CHILD_APPEND_BATCH_SIZE: usize = 256;
/// Stale cached rows one unit of a load's purge forgets (see
/// `ChildReconcileProgress::stale_cache_rows`).
///
/// Kind: cap; a power of two, rounded down.
const STALE_CACHE_ROWS_PER_UNIT: usize = 4 * 1024;

/// Items a retiring listing's store, or rows its mirror, drop per unit; a
/// turn runs units while `model::list_budget::turn_continues` holds.
///
/// Kind: cap; a power of two, rounded down.
const RETIRED_LISTING_ROWS_PER_UNIT: usize = 4 * 1024;

/// A collapsed directory's listing on its way out: the child store GTK no
/// longer shows, which nothing else holds, and the plain row mirror that
/// projected it. Both are emptied in slices, so no dispatch finalizes a
/// whole listing (ledger A91).
pub(crate) struct RetiringListing {
    store: gio::ListStore,
    mirror: Vec<DirectoryRowState>,
}

/// Rows per batch while a directory that was empty receives its first
/// `CHILD_APPEND_BATCH_SIZE` rows (see `ChildReconcileProgress::warming_up`).
///
/// Kind: cap; a power of two, rounded down.
const FIRST_BATCH_ROWS: usize = 32;
/// Changed rows at or below one batch retain the calibrated direct splice path.
///
/// Kind: cap; a power of two, rounded down.
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
    let settings = crate::ui::settings_writes::shared();
    let lookahead_cap =
        settings.uint(crate::config::keys::WORKSPACE_EMPTY_FOLDER_LOOKAHEAD_CAP) as usize;
    let visibility = crate::ui::workspace_visibility::workspace_entry_visibility(&settings);
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
        visibility,
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

    // A load this scan supersedes may have left its loading row: remove it
    // before the mirror is captured, or the new plan would find no common
    // suffix past it and replace (deselect) every row after the change.
    retire_stale_loading_row(section, &store, store_key);
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
    let visibility = request.visibility;
    let scan_budget = crate::ui::sidebar::policy::sidebar_scan_budget(directory_retained_bytes());
    #[cfg(feature = "test-utils")]
    let scan_delay = section.imp().refresh_runtime.test_scan_delay.get();
    gtk_lush_tasks::spawn_blocking_then(
        (store, path.clone(), Arc::clone(&cancel), permit),
        move || {
            #[cfg(feature = "test-utils")]
            if !scan_delay.is_zero() {
                std::thread::sleep(scan_delay);
            }
            let scan = services::file_tree::scan_sidebar_directory(
                &path,
                scan_budget,
                lookahead_cap,
                || cancel.load(Ordering::Acquire),
                &visibility,
            );
            let limited = scan
                .truncated
                .then_some((scan.entries.len(), scan.examined_entries));
            let plan = if scan.cancelled || scan.error.is_some() {
                None
            } else {
                let desired_rows = desired_child_rows(scan.entries, limited);
                Some(plan_directory_reconciliation(&current_rows, &desired_rows))
            };
            ChildScanResult {
                cancelled: scan.cancelled,
                error: scan.error,
                limited,
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
                    limited: scan.limited,
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
    section
        .imp()
        .child_cache_owners
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
    section.release_retained_child_stores(under_removed_root);
    section
        .imp()
        .item_locations
        .borrow_mut()
        .retain(|path, _| roots.contains(path) || !under_removed_root(path));

    sync_child_scan_busy_state(section);
}

/// Retire `store`, which the section held for a directory GTK no longer
/// shows through it. A store anyone else still holds (GTK through another
/// row, an in-flight scan, a load's next batch) is only let go, so whoever
/// holds it disposes it as before; a store only the section holds loses its
/// scan runtime and mirror bookkeeping now and is emptied in slices.
fn retire_child_store(section: &LushtextWorkspaceSection, store: gio::ListStore) {
    if store.ref_count() > 1 {
        return;
    }
    let store_key = child_store_key(&store);
    cancel_child_store_runtime(section, store_key);
    let imp = section.imp();
    let mirror = imp
        .child_row_mirrors
        .borrow_mut()
        .remove(&store_key)
        .unwrap_or_default();
    imp.child_row_mirror_generations
        .borrow_mut()
        .remove(&store_key);
    imp.child_store_paths.borrow_mut().remove(&store_key);
    imp.child_store_refs.borrow_mut().remove(&store_key);
    imp.retiring_child_stores
        .borrow_mut()
        .push_back(RetiringListing { store, mirror });
    if imp.child_store_retirement_source.borrow().is_some() {
        return;
    }
    let section_weak = section.downgrade();
    // `G_PRIORITY_LOW`: one main-loop iteration dispatches every ready source
    // of its top priority together (ledger A81), so at the default priority a
    // retirement turn stacks onto a directory's load turn; re-expanding a
    // collapsed 131,072-entry directory measured 17.5–23.0 ms iterations that
    // way. At low priority it runs only in iterations nothing else needs.
    let source =
        glib::timeout_add_local_full(Duration::from_millis(1), glib::Priority::LOW, move || {
            let Some(section) = section_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if drain_retiring_listings(&section) {
                return glib::ControlFlow::Continue;
            }
            section
                .imp()
                .child_store_retirement_source
                .borrow_mut()
                .take();
            glib::ControlFlow::Break
        });
    imp.child_store_retirement_source.replace(Some(source));
}

/// One turn of listing retirement: drop retired items and mirror rows in
/// units of `RETIRED_LISTING_ROWS_PER_UNIT` while the turn's slice lasts,
/// releasing each listing once empty. Returns whether any remain.
fn drain_retiring_listings(section: &LushtextWorkspaceSection) -> bool {
    let started_us = glib::monotonic_time();
    let mut units = 0usize;
    loop {
        let elapsed = u64::try_from(glib::monotonic_time().saturating_sub(started_us)).unwrap_or(0);
        if !turn_continues(elapsed, units) {
            return true;
        }
        // Take the listing out of the queue while its slice drops, so no
        // borrow is held across the store's `items-changed` or a drop.
        let Some(mut listing) = section.imp().retiring_child_stores.borrow_mut().pop_front() else {
            return false;
        };
        let items = listing.store.n_items();
        if items > 0 {
            let unit = u32::try_from(RETIRED_LISTING_ROWS_PER_UNIT).unwrap_or(u32::MAX);
            let take = items.min(unit);
            listing
                .store
                .splice(items - take, take, &[] as &[glib::Object]);
        } else if !listing.mirror.is_empty() {
            let keep = listing
                .mirror
                .len()
                .saturating_sub(RETIRED_LISTING_ROWS_PER_UNIT);
            listing.mirror.truncate(keep);
        }
        units += 1;
        if listing.store.n_items() > 0 || !listing.mirror.is_empty() {
            section
                .imp()
                .retiring_child_stores
                .borrow_mut()
                .push_front(listing);
        }
    }
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
    section.release_retained_child_stores(|_| true);
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
    // A new store at a recycled address owns no cache.
    section
        .imp()
        .child_cache_owners
        .borrow_mut()
        .retain(|_, owner| *owner != store_key);
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
    let old_path_rows = mirror.iter().filter(|row| row.path.is_some()).count();
    let removed_paths = mirror
        .splice(start..end, rows.iter().cloned())
        .filter_map(|row| row.path)
        .collect::<Vec<_>>();
    drop(mirrors);
    // Keep the directory's path caches current per applied batch, so no
    // terminal rebuilds a whole directory in one dispatch
    // (`bound-lists-by-budget-not-count` D6). Placeholder rows only ever end
    // a listing, so a store position below the path rows is a path index.
    let parent = section
        .imp()
        .child_store_paths
        .borrow()
        .get(&store_key)
        .cloned();
    if let Some(parent) = parent
        && child_store_is_live(section, &parent, store_key)
    {
        let inserted_paths = rows
            .iter()
            .filter_map(|row| row.path.clone())
            .collect::<Vec<_>>();
        section.splice_child_rows_cache(
            store_key,
            &parent,
            start.min(old_path_rows),
            old_path_rows,
            &removed_paths,
            &inserted_paths,
        );
    }
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

/// Whether `store_key` is the store the section currently shows for
/// `parent`: the latest registered in `dir_stores`, of a directory the user
/// has expanded. A collapsed directory's rows are not in the flattened model,
/// so nothing locates them through the path caches; and GTK's expander probes
/// `is_expandable()` with a store it discards (A22), whose rows must not
/// enter the caches.
fn child_store_is_live(
    section: &LushtextWorkspaceSection,
    parent: &Path,
    store_key: usize,
) -> bool {
    section.imp().expanded_paths.borrow().contains(parent)
        && section
            .imp()
            .dir_stores
            .borrow()
            .get(parent)
            .and_then(glib::WeakRef::upgrade)
            .is_some_and(|store| child_store_key(&store) == store_key)
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
        placeholder: item.placeholder(),
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

/// Owned worker result needed by one current direct or bounded GTK application.
struct ScannedChildrenPlan {
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    ticket: ChildScanTicket,
    mirror_generation: u64,
    plan: DirectoryReconciliationPlan,
    limited: Option<(usize, usize)>,
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
        limited,
    } = scanned;
    if let Some((shown, total)) = limited {
        tracing::warn!("Directory listing limited by its retained-byte budget: {shown} of {total}");
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
            let mirror_was_empty = section
                .imp()
                .child_row_mirrors
                .borrow()
                .get(&child_store_key(&store))
                .is_none_or(Vec::is_empty);
            // A directory that takes several turns ends with a loading row
            // until its last batch; the row is part of the mirror, so a later
            // reconciliation removes it if this one is superseded.
            let total_rows = replacement.iter().filter(|row| row.path.is_some()).count();
            let stale_cache_rows = take_over_child_cache(section, &store, &dir_path);
            // Reserving rehashes what the maps hold, so a takeover reserves
            // only once the previous store's rows are forgotten.
            let reserve_rows = if stale_cache_rows.is_empty() {
                reserve_child_cache_capacity(section, &store, &dir_path, total_rows);
                None
            } else {
                Some(total_rows)
            };
            let mirror_generation =
                append_loading_row(section, &store, total_rows).unwrap_or(mirror_generation);
            let loading_row = store
                .item(store.n_items().saturating_sub(1))
                .and_downcast::<FileTreeItem>()
                .filter(|item| item.placeholder() == Some(DirectoryPlaceholder::Loading))
                .map(|item| (item, total_rows));
            let progress = Rc::new(RefCell::new(ChildReconcileProgress {
                position,
                remove_remaining: removed,
                inserted: 0,
                replacement: VecDeque::from(replacement),
                mirror_generation,
                loading_row,
                warming_up: mirror_was_empty,
                stale_cache_rows,
                reserve_rows,
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
        retire_loading_row(section, &store, &progress);
        finish_child_scan(section, store_key, ticket);
        return;
    }

    let turn_started_us = glib::monotonic_time();
    let mut batches_this_turn = 0usize;
    let mut after_frame = false;
    while !progress.borrow().stale_cache_rows.is_empty() {
        let elapsed =
            u64::try_from(glib::monotonic_time().saturating_sub(turn_started_us)).unwrap_or(0);
        if !turn_continues(elapsed, batches_this_turn) {
            break;
        }
        let chunk = {
            let mut progress = progress.borrow_mut();
            let keep = progress
                .stale_cache_rows
                .len()
                .saturating_sub(STALE_CACHE_ROWS_PER_UNIT);
            progress.stale_cache_rows.split_off(keep)
        };
        section.forget_stale_child_rows(&dir_path, &chunk);
        batches_this_turn += 1;
    }
    if !progress.borrow().stale_cache_rows.is_empty() {
        schedule_next_reconcile_batch(section, store, dir_path, token, ticket, progress, false);
        return;
    }
    let reserve_rows = progress.borrow_mut().reserve_rows.take();
    if let Some(rows) = reserve_rows {
        reserve_child_cache_capacity(section, &store, &dir_path, rows);
    }
    loop {
        let elapsed =
            u64::try_from(glib::monotonic_time().saturating_sub(turn_started_us)).unwrap_or(0);
        if !turn_continues(elapsed, batches_this_turn) {
            break;
        }
        let (position, removed, rows) = {
            let mut progress = progress.borrow_mut();
            if progress.remove_remaining > 0 {
                let removed = progress.remove_remaining.min(CHILD_APPEND_BATCH_SIZE);
                progress.remove_remaining -= removed;
                (progress.position, removed, Vec::new())
            } else {
                let batch_rows =
                    if progress.warming_up && progress.inserted < CHILD_APPEND_BATCH_SIZE {
                        FIRST_BATCH_ROWS
                    } else {
                        CHILD_APPEND_BATCH_SIZE
                    };
                let mut rows = Vec::with_capacity(batch_rows);
                for _ in 0..batch_rows {
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
        batches_this_turn += 1;
        let first_small_batch = progress.borrow().warming_up
            && !rows.is_empty()
            && progress.borrow().inserted == rows.len();

        let complete = {
            let progress = progress.borrow();
            progress.remove_remaining == 0 && progress.replacement.is_empty()
        };
        if complete {
            retire_loading_row(section, &store, &progress);
            finish_child_reconciliation(section, &store, &dir_path, &token, ticket);
            return;
        }
        if first_small_batch {
            after_frame = true;
            break;
        }
    }
    {
        let refresh = &section.imp().refresh_runtime;
        refresh.reconcile_max_batches_per_turn.set(
            refresh
                .reconcile_max_batches_per_turn
                .get()
                .max(batches_this_turn),
        );
    }
    // The loading row's label moves at most once per turn.
    if let Some((item, total)) = progress.borrow().loading_row.as_ref() {
        let listed = progress.borrow().inserted.min(*total);
        item.set_placeholder_label(crate::ui::sidebar::policy::loading_row_label(
            listed, *total,
        ));
        section.refresh_placeholder_row_accessibility(item);
    }
    schedule_next_reconcile_batch(
        section,
        store,
        dir_path,
        token,
        ticket,
        progress,
        after_frame,
    );
}

/// How long the load waits after a first small batch: past one 60 Hz frame
/// interval, so the list is laid out in the slice bin's band before the next
/// batch (see `ChildReconcileProgress::warming_up`).
const AFTER_FIRST_BATCH_DELAY: Duration = Duration::from_millis(20);

fn schedule_next_reconcile_batch(
    section: &LushtextWorkspaceSection,
    store: gio::ListStore,
    dir_path: PathBuf,
    token: Arc<AtomicBool>,
    ticket: ChildScanTicket,
    progress: Rc<RefCell<ChildReconcileProgress>>,
    after_frame: bool,
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
    let batch_delay = if after_frame {
        batch_delay.max(AFTER_FIRST_BATCH_DELAY)
    } else {
        batch_delay
    };
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
    // The path caches were maintained per applied batch
    // (`splice_child_store`), so the terminal does not grow with the
    // directory: it publishes, schedules the state restore and syncs the
    // realized rows (`bound-lists-by-budget-not-count` D6).
    if !section
        .imp()
        .child_row_mirrors
        .borrow()
        .contains_key(&store_key)
    {
        finish_child_scan(section, store_key, ticket);
        return;
    }
    let _ = dir_path;
    let terminal_started_us = glib::monotonic_time();
    schedule_child_state_restore(section);
    section.sync_file_row_states();
    let refresh = &section.imp().refresh_runtime;
    refresh.reconcile_last_terminal_us.set(
        u64::try_from(glib::monotonic_time().saturating_sub(terminal_started_us)).unwrap_or(0),
    );
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

/// Build the desired child-row state from a fresh scan: every listed entry,
/// then the budget row when the retained-byte budget cut the listing.
fn desired_child_rows(
    entries: Vec<services::file_tree::DirectoryEntry>,
    limited: Option<(usize, usize)>,
) -> Vec<DirectoryRowState> {
    let mut rows = entries
        .into_iter()
        .map(DirectoryRowState::from_entry)
        .collect::<Vec<_>>();

    if let Some((shown, total)) = limited {
        rows.push(DirectoryRowState::placeholder_row(
            DirectoryPlaceholder::Budget { shown, total },
        ));
    }

    rows
}

/// The label a placeholder row of `placeholder` starts with.
fn placeholder_label(placeholder: DirectoryPlaceholder) -> String {
    match placeholder {
        DirectoryPlaceholder::Budget { shown, total } => {
            crate::ui::sidebar::policy::budget_row_label(shown, total)
        }
        DirectoryPlaceholder::Loading => crate::ui::sidebar::policy::loading_row_label(0, 0),
    }
}

/// Convert comparable row state back into GTK row objects for splicing.
fn build_child_items(rows: &[DirectoryRowState]) -> Vec<FileTreeItem> {
    rows.iter()
        .map(|row| match (row.path.as_ref(), row.placeholder) {
            (Some(path), _) => FileTreeItem::new(path.clone(), row.is_dir, row.is_empty),
            (None, Some(placeholder)) => {
                FileTreeItem::new_placeholder(placeholder_label(placeholder), placeholder)
            }
            (None, None) => FileTreeItem::new_placeholder(
                placeholder_label(DirectoryPlaceholder::Loading),
                DirectoryPlaceholder::Loading,
            ),
        })
        .collect()
}

/// Reserve room for `additional` rows in the directory's mirror and path
/// caches before its batches apply. Without it the two path-keyed maps grow
/// by doubling mid-load and rehash every entry in one batch: 23 ms and 41 ms
/// single dispatches at 131,072 entries (trace, `bound-lists-by-budget-not-count`).
/// Reserved here, the rehash costs only the entries other directories hold.
fn reserve_child_cache_capacity(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    dir_path: &Path,
    additional: usize,
) {
    let store_key = child_store_key(store);
    let imp = section.imp();
    if let Some(mirror) = imp.child_row_mirrors.borrow_mut().get_mut(&store_key) {
        mirror.reserve(additional.saturating_add(1));
    }
    if !child_store_is_live(section, dir_path, store_key) {
        return;
    }
    imp.child_paths
        .borrow_mut()
        .entry(dir_path.to_path_buf())
        .or_default()
        .reserve(additional);
    imp.item_locations.borrow_mut().reserve(additional);
    imp.visible_path_counts.borrow_mut().reserve(additional);
}

/// Append the loading row to a directory about to load over several turns,
/// returning the new mirror generation; `None` when one batch suffices.
fn append_loading_row(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    total_rows: usize,
) -> Option<u64> {
    if total_rows <= CHILD_APPEND_BATCH_SIZE {
        return None;
    }
    let row = DirectoryRowState::placeholder_row(DirectoryPlaceholder::Loading);
    let item = FileTreeItem::new_placeholder(
        crate::ui::sidebar::policy::loading_row_label(0, total_rows),
        DirectoryPlaceholder::Loading,
    );
    let end = usize::try_from(store.n_items()).unwrap_or(usize::MAX);
    Some(splice_child_store(
        section,
        store,
        end,
        0,
        std::slice::from_ref(&row),
        std::slice::from_ref(&item),
    ))
}

/// Make `store` the owner of `dir_path`'s path caches before its load, and
/// hand back the rows a previous store left there, to be forgotten in slices.
/// Nothing when the store already owns them or is not the live one.
fn take_over_child_cache(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    dir_path: &Path,
) -> Vec<PathBuf> {
    let store_key = child_store_key(store);
    let imp = section.imp();
    if !child_store_is_live(section, dir_path, store_key)
        || imp.child_cache_owners.borrow().get(dir_path) == Some(&store_key)
    {
        return Vec::new();
    }
    let mirror_paths = imp
        .child_row_mirrors
        .borrow()
        .get(&store_key)
        .map_or(0, |rows| {
            rows.iter().filter(|row| row.path.is_some()).count()
        });
    if mirror_paths != 0 {
        // The store already has rows the caches never followed: leave the
        // first batch's consistency check to rebuild from its mirror.
        return Vec::new();
    }
    imp.child_cache_owners
        .borrow_mut()
        .insert(dir_path.to_path_buf(), store_key);
    let stale = imp
        .child_paths
        .borrow_mut()
        .remove(dir_path)
        .unwrap_or_default();
    if !stale.is_empty() {
        let refresh = &imp.refresh_runtime;
        refresh
            .cache_owner_resets
            .set(refresh.cache_owner_resets.get().saturating_add(1));
    }
    stale
}

/// Remove a loading row whose load no longer runs (it was superseded), so a
/// new scan plans against the listing alone.
fn retire_stale_loading_row(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    store_key: usize,
) {
    if section
        .imp()
        .child_reconcile_sources
        .borrow()
        .contains_key(&store_key)
    {
        return;
    }
    let Some(last) = store.n_items().checked_sub(1) else {
        return;
    };
    let stale = store
        .item(last)
        .and_downcast::<FileTreeItem>()
        .is_some_and(|item| item.placeholder() == Some(DirectoryPlaceholder::Loading));
    if stale {
        let position = usize::try_from(last).unwrap_or(usize::MAX);
        splice_child_store(section, store, position, 1, &[], &[]);
    }
}

/// Remove the loading row a reconciliation added, at its terminal or when it
/// is abandoned, so no stale loading row outlives its load.
fn retire_loading_row(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
    progress: &Rc<RefCell<ChildReconcileProgress>>,
) {
    let Some((item, _)) = progress.borrow_mut().loading_row.take() else {
        return;
    };
    let last = store.n_items().checked_sub(1);
    if let Some(last) = last
        && store
            .item(last)
            .is_some_and(|object| object == item.clone().upcast::<glib::Object>())
    {
        let position = usize::try_from(last).unwrap_or(usize::MAX);
        splice_child_store(section, store, position, 1, &[], &[]);
    }
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
        // model carry user expansion intent. A row `GtkTreeListModel` is tearing
        // down reports `expanded = false` while `position()` still answers with
        // a stale index (observed: `0`, not `INVALID_LIST_POSITION`), so the
        // position alone cannot tell a destroyed row from a live collapse. The
        // row is live only if the model still hands it back at that position;
        // otherwise treating the teardown as a collapse prunes intent the
        // deferred restore was about to re-apply, and an expanded folder
        // silently collapses whenever a refresh splices its segment.
        let position = row.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return;
        }
        // The model cell is mutably borrowed while a replacement model is being
        // installed; a transition arriving then belongs to that install, so keep
        // the pre-guard behaviour instead of panicking on a nested borrow.
        let row_is_live = match self.imp().tree_model.try_borrow() {
            Ok(model) => model
                .as_ref()
                .and_then(|model| model.item(position))
                .and_downcast::<gtk4::TreeListRow>()
                .is_some_and(|live| live == *row),
            Err(_) => true,
        };
        if !row_is_live {
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
            expanded.insert(path.clone());
            drop(expanded);
            self.retain_shown_child_store(&path, row);
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
        // The collapse unlinked this listing and every expanded descendant's:
        // retire them now that GTK holds none of them.
        self.release_retained_child_stores(|retained| retained.starts_with(&path));
        if needs_model_reconcile {
            self.reconcile_expanded_subtree_from_model(&path);
        }
    }

    /// Hold the child store the expanded row at `path` shows, retiring the
    /// one held for that path before (a refresh's replacement row).
    fn retain_shown_child_store(&self, path: &Path, row: &gtk4::TreeListRow) {
        let Some(store) = row.children().and_downcast::<gio::ListStore>() else {
            return;
        };
        let previous = self
            .imp()
            .retained_child_stores
            .borrow_mut()
            .insert(path.to_path_buf(), store.clone());
        if let Some(previous) = previous
            && previous != store
        {
            retire_child_store(self, previous);
        }
    }

    /// Stop holding every retained store whose directory matches `released`,
    /// retiring each one nothing else holds.
    pub(super) fn release_retained_child_stores(&self, released: impl Fn(&Path) -> bool) {
        let stores = {
            let mut retained = self.imp().retained_child_stores.borrow_mut();
            let paths = retained
                .keys()
                .filter(|path| released(path))
                .cloned()
                .collect::<Vec<_>>();
            paths
                .into_iter()
                .filter_map(|path| retained.remove(&path))
                .collect::<Vec<_>>()
        };
        for store in stores {
            retire_child_store(self, store);
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

    /// Apply one store splice to a directory's path caches: O(batch) plus
    /// the siblings after the splice, never the whole workspace. Falls back to
    /// the full rebuild only when the caches disagree with the mirror they
    /// project (counted in `cache_full_rebuilds`).
    /// Forget one chunk of a previous store's cached rows of `parent_dir`.
    fn forget_stale_child_rows(&self, parent_dir: &Path, rows: &[PathBuf]) {
        let imp = self.imp();
        forget_child_occurrences(
            &imp.folder_paths.borrow(),
            &imp.child_paths.borrow(),
            &mut imp.visible_path_counts.borrow_mut(),
            &mut imp.item_locations.borrow_mut(),
            parent_dir,
            rows,
        );
    }

    pub(super) fn splice_child_rows_cache(
        &self,
        store_key: usize,
        parent_dir: &Path,
        path_start: usize,
        old_path_rows: usize,
        removed: &[PathBuf],
        inserted: &[PathBuf],
    ) {
        let imp = self.imp();
        let owner = imp.child_cache_owners.borrow().get(parent_dir).copied();
        let owned = owner == Some(store_key);
        let consistent =
            owned && imp.child_paths.borrow().get(parent_dir).map_or(0, Vec::len) == old_path_rows;
        if !consistent {
            // A new live store took over (or the caches drifted): project
            // this store's whole mirror once, then follow it per batch.
            let rows = imp
                .child_row_mirrors
                .borrow()
                .get(&store_key)
                .cloned()
                .unwrap_or_default();
            self.recache_child_rows_from_mirror(parent_dir, &rows);
            imp.child_cache_owners
                .borrow_mut()
                .insert(parent_dir.to_path_buf(), store_key);
            let refresh = &imp.refresh_runtime;
            if owned {
                refresh
                    .cache_full_rebuilds
                    .set(refresh.cache_full_rebuilds.get().saturating_add(1));
            } else if owner.is_some() {
                refresh
                    .cache_owner_resets
                    .set(refresh.cache_owner_resets.get().saturating_add(1));
            }
            return;
        }
        let operations = splice_child_cache(
            &imp.folder_paths.borrow(),
            &mut imp.child_paths.borrow_mut(),
            &mut imp.visible_path_counts.borrow_mut(),
            &mut imp.item_locations.borrow_mut(),
            parent_dir,
            path_start,
            removed,
            inserted,
        );
        let refresh = &imp.refresh_runtime;
        refresh.cache_batch_operations_high_water.set(
            refresh
                .cache_batch_operations_high_water
                .get()
                .max(operations),
        );
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

/// Apply one splice of a directory's path rows to the caches, as
/// [`replace_child_cache`] would after the same splice, without visiting the
/// unchanged rows before it: the removed and inserted paths, then the
/// siblings after the splice when their indices move. A path whose occurrences
/// drop back to one elsewhere is resolved by the same shared recovery walk.
/// Returns the operations performed.
#[expect(
    clippy::too_many_arguments,
    reason = "the four caches are separate RefCell borrows owned by the section"
)]
fn splice_child_cache(
    folder_paths: &[PathBuf],
    child_paths: &mut HashMap<PathBuf, Vec<PathBuf>>,
    visible_path_counts: &mut HashMap<PathBuf, usize>,
    item_locations: &mut HashMap<PathBuf, ItemLocation>,
    parent_dir: &Path,
    path_start: usize,
    removed: &[PathBuf],
    inserted: &[PathBuf],
) -> usize {
    let mut operations = removed.len().saturating_add(inserted.len());
    let siblings = child_paths.entry(parent_dir.to_path_buf()).or_default();
    let start = path_start.min(siblings.len());
    let end = start.saturating_add(removed.len()).min(siblings.len());
    siblings.splice(start..end, inserted.iter().cloned());
    let tail_start = start.saturating_add(inserted.len());
    let moved = inserted.len() != removed.len();
    let recover =
        release_child_occurrences(visible_path_counts, item_locations, parent_dir, removed);
    for (offset, path) in inserted.iter().enumerate() {
        let next = visible_path_counts
            .get(path)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        visible_path_counts.insert(path.clone(), next);
        if next == 1 {
            item_locations.insert(
                path.clone(),
                ItemLocation {
                    parent_dir: Some(parent_dir.to_path_buf()),
                    index: start.saturating_add(offset),
                },
            );
        } else {
            item_locations.remove(path);
        }
    }
    let siblings = child_paths
        .get(parent_dir)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if moved {
        for (offset, path) in siblings.iter().enumerate().skip(tail_start) {
            operations = operations.saturating_add(1);
            if let Some(location) = item_locations.get_mut(path)
                && location.parent_dir.as_deref() == Some(parent_dir)
            {
                location.index = offset;
            }
        }
    }
    if siblings.is_empty() {
        child_paths.remove(parent_dir);
    }
    operations.saturating_add(resolve_unique_occurrences(
        folder_paths,
        child_paths,
        visible_path_counts,
        item_locations,
        recover,
    ))
}

/// Forget `removed`, rows of `parent_dir` that left the caches' sibling list
/// already (a superseded store's rows, purged in slices): their occurrence
/// counts and locations, resolving any path left unique elsewhere.
fn forget_child_occurrences(
    folder_paths: &[PathBuf],
    child_paths: &HashMap<PathBuf, Vec<PathBuf>>,
    visible_path_counts: &mut HashMap<PathBuf, usize>,
    item_locations: &mut HashMap<PathBuf, ItemLocation>,
    parent_dir: &Path,
    removed: &[PathBuf],
) -> usize {
    let recover =
        release_child_occurrences(visible_path_counts, item_locations, parent_dir, removed);
    removed.len().saturating_add(resolve_unique_occurrences(
        folder_paths,
        child_paths,
        visible_path_counts,
        item_locations,
        recover,
    ))
}

/// Drop one occurrence of each of `removed` (rows of `parent_dir`), returning
/// the paths left with exactly one occurrence elsewhere.
fn release_child_occurrences(
    visible_path_counts: &mut HashMap<PathBuf, usize>,
    item_locations: &mut HashMap<PathBuf, ItemLocation>,
    parent_dir: &Path,
    removed: &[PathBuf],
) -> Vec<PathBuf> {
    let mut recover = Vec::new();
    for path in removed {
        let next = visible_path_counts
            .get(path)
            .copied()
            .unwrap_or(0)
            .saturating_sub(1);
        if next == 0 {
            visible_path_counts.remove(path);
            item_locations.remove(path);
        } else {
            visible_path_counts.insert(path.clone(), next);
            if item_locations
                .get(path)
                .is_some_and(|location| location.parent_dir.as_deref() == Some(parent_dir))
            {
                item_locations.remove(path);
            }
            if next == 1 {
                recover.push(path.clone());
            }
        }
    }
    recover
}

/// Find the sole location of each path in `recover` that has exactly one
/// occurrence left and no location: one shared walk of the folder rows and
/// the materialized child rows, as `replace_child_cache` does.
fn resolve_unique_occurrences(
    folder_paths: &[PathBuf],
    child_paths: &HashMap<PathBuf, Vec<PathBuf>>,
    visible_path_counts: &HashMap<PathBuf, usize>,
    item_locations: &mut HashMap<PathBuf, ItemLocation>,
    recover: Vec<PathBuf>,
) -> usize {
    let mut unresolved = recover
        .into_iter()
        .filter(|path| {
            visible_path_counts.get(path) == Some(&1) && !item_locations.contains_key(path)
        })
        .collect::<HashSet<_>>();
    if unresolved.is_empty() {
        return 0;
    }
    let mut operations = 0usize;
    for (index, path) in folder_paths.iter().enumerate() {
        operations = operations.saturating_add(1);
        if unresolved.remove(path) {
            item_locations.insert(
                path.clone(),
                ItemLocation {
                    parent_dir: None,
                    index,
                },
            );
        }
    }
    for (other_parent, siblings) in child_paths {
        if unresolved.is_empty() {
            break;
        }
        for (index, path) in siblings.iter().enumerate() {
            operations = operations.saturating_add(1);
            if unresolved.remove(path) {
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
    operations
}

#[cfg(test)]
mod index_tests {
    use super::*;
    use proptest::prelude::*;

    /// Rows the sidebar plans for a directory of `count` files under
    /// `retained_bytes`, and the budget row's counts if it is planned.
    fn directory_rows(count: usize, retained_bytes: u64) -> (usize, Option<(usize, usize)>) {
        let dir = tempfile::tempdir().expect("tempdir");
        for index in 0..count {
            crate::services::filesystem::fixture::write_text(
                &dir.path().join(format!("f{index:05}")),
                "",
            );
        }
        let scan = services::file_tree::scan_sidebar_directory(
            dir.path(),
            crate::ui::sidebar::policy::sidebar_scan_budget(retained_bytes),
            8,
            || false,
            &crate::model::workspace_visibility::WorkspaceEntryVisibility::default(),
        );
        let limited = scan
            .truncated
            .then_some((scan.entries.len(), scan.examined_entries));
        let rows = desired_child_rows(scan.entries, limited);
        let budget = rows.iter().find_map(|row| match row.placeholder {
            Some(DirectoryPlaceholder::Budget { shown, total }) => Some((shown, total)),
            _ => None,
        });
        (rows.iter().filter(|row| row.path.is_some()).count(), budget)
    }

    /// No count cap (`bound-lists-by-budget-not-count`): a directory past
    /// `main`'s 10,000 entries is listed in full, and a budget cut states
    /// exactly how many entries are shown of how many.
    #[test]
    fn no_loss_a_directory_past_mains_cap_lists_every_entry() {
        let budget = crate::ui::sidebar::policy::SIDEBAR_DIRECTORY_RETAINED_BYTES;
        assert_eq!(directory_rows(10_001, budget), (10_001, None));
        let (shown, limited) = directory_rows(500, 64 * 1024);
        assert!(shown < 500);
        assert_eq!(limited, Some((shown, 500)));
    }

    proptest! {
        /// Per-batch cache maintenance equals the full rebuild after the same
        /// splice, across duplicates in other parents and top-level folders.
        #[test]
        fn spliced_child_cache_matches_the_full_rebuild(
            folder_ids in prop::collection::vec(0u8..8, 0..12),
            other_ids in prop::collection::vec(0u8..8, 0..24),
            old_ids in prop::collection::vec(0u8..8, 0..40),
            start in 0usize..40,
            removed in 0usize..40,
            inserted_ids in prop::collection::vec(0u8..8, 0..40),
        ) {
            let parent = PathBuf::from("/workspace/parent");
            let other_parent = PathBuf::from("/workspace/other");
            let folder_paths = folder_ids.into_iter().map(test_path).collect::<Vec<_>>();
            let old = old_ids.into_iter().map(test_path).collect::<Vec<_>>();
            let start = start.min(old.len());
            let end = start.saturating_add(removed).min(old.len());
            let inserted = inserted_ids.into_iter().map(test_path).collect::<Vec<_>>();
            let mut new = old.clone();
            let removed_paths = new.splice(start..end, inserted.iter().cloned()).collect::<Vec<_>>();
            let other_rows = other_ids
                .into_iter()
                .enumerate()
                .map(|(index, id)| (index, test_path(id)))
                .collect::<Vec<_>>();
            let indexed = |rows: &[PathBuf]| {
                rows.iter().cloned().enumerate().collect::<Vec<_>>()
            };

            let (mut counts, mut locations) = oracle(
                &folder_paths,
                &[
                    (parent.clone(), indexed(&old)),
                    (other_parent.clone(), other_rows.clone()),
                ],
            );
            let mut child_paths = HashMap::from([(
                other_parent.clone(),
                other_rows.iter().map(|(_, path)| path.clone()).collect::<Vec<_>>(),
            )]);
            if !old.is_empty() {
                child_paths.insert(parent.clone(), old);
            }
            splice_child_cache(
                &folder_paths,
                &mut child_paths,
                &mut counts,
                &mut locations,
                &parent,
                start,
                &removed_paths,
                &inserted,
            );
            let (expected_counts, expected_locations) = oracle(
                &folder_paths,
                &[(parent.clone(), indexed(&new)), (other_parent, other_rows)],
            );
            prop_assert_eq!(counts, expected_counts);
            prop_assert_eq!(locations, expected_locations);
            if new.is_empty() {
                prop_assert!(!child_paths.contains_key(&parent));
            } else {
                prop_assert_eq!(child_paths.get(&parent), Some(&new));
            }
        }
    }

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

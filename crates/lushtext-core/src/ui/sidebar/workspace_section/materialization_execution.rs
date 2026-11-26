// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's **deferred child-store
//! materialization** stage order.
//!
//! # Role
//!
//! Coordination, `execution`, qualified by the stage order it serves, nested under the
//! workflow's canonical role home in `ui/sidebar/`.
//!
//! # Probe vs. install inversion and resumption point
//!
//! `GtkTreeListModel`'s create closure (`folder_execution.rs` →
//! `scan_execution::build_children_model`) cannot tell whether GTK is probing
//! expandability (`GtkTreeListRow::is_expandable()`, which runs on every bind,
//! recycled rebind, `hide-expander` change, and collapse `children` notify) or
//! installing children (`set_expanded(true)`), per ledger A135. GTK reveals the
//! difference only after the create call returns: a probe's model loses its last
//! reference inside `is_expandable()`, whereas an installed model stays alive in
//! `node->model`.
//!
//! `build_children_model` therefore creates an empty `gio::ListStore` without
//! registering it or doing any I/O, and calls [`enqueue_child_store_materialization`].
//! Control resumes in the per-section `glib::Priority::HIGH` bounded drain
//! (`drain_pending_materializations`), which pops at most
//! `policy::MATERIALIZATION_DRAIN_BATCH` entries per dispatch, classifies each
//! entry through `policy::materialization_disposition`, drops dead probes and
//! retired or already-materialized entries, and calls
//! `scan_execution::populate_child_store` only for live, installed stores.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use glib::object::{ObjectExt, ObjectType};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{gio, glib};

use super::LushtextWorkspaceSection;
use crate::ui::sidebar::policy::{
    MaterializationDisposition, MaterializationFacts, materialization_disposition,
    materialization_drain_step,
};

/// One child `gio::ListStore` returned by `build_children_model`, waiting for
/// the `Priority::HIGH` drain to observe whether GTK kept it alive.
pub(super) struct PendingMaterialization {
    store: glib::WeakRef<gio::ListStore>,
    path: PathBuf,
    lifetime: u64,
    retired: bool,
}

/// Queue a newly created child store for deferred materialization and arm the
/// `Priority::HIGH` drain if not already armed.
///
/// Must NOT call `sync_child_scan_busy_state`: at enqueue time a probe store is
/// still alive until `is_expandable()` returns and unrefs it, so syncing
/// readiness here would transiently toggle the tree's accessible `busy` state
/// during mere binding or scrolling.
pub(super) fn enqueue_child_store_materialization(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
    store: &gio::ListStore,
) {
    let imp = section.imp();
    let refresh = &imp.refresh_runtime;
    refresh
        .child_model_creations
        .set(refresh.child_model_creations.get().saturating_add(1));

    let entry = PendingMaterialization {
        store: store.downgrade(),
        path: dir_path.to_path_buf(),
        lifetime: imp.child_scan_lifetime.get(),
        retired: false,
    };
    imp.pending_materializations.borrow_mut().push_back(entry);
    arm_materialization_drain(section);
}

/// Return whether any queued entry belongs to a still-live installed store of
/// the active section lifetime, so readiness never mistakes "expanded, awaiting
/// drain" for "scanned and empty".
pub(super) fn has_live_pending_materializations(section: &LushtextWorkspaceSection) -> bool {
    let imp = section.imp();
    if imp.disposed.get() {
        return false;
    }
    let current_lifetime = imp.child_scan_lifetime.get();
    imp.pending_materializations.borrow().iter().any(|entry| {
        !entry.retired && entry.lifetime == current_lifetime && entry.store.upgrade().is_some()
    })
}

/// Mark queued entries under `roots` retired when `clear_dir_states` removes
/// a subtree, so the drain drops them even if GTK still holds the store.
pub(super) fn retire_materializations_under_roots(
    section: &LushtextWorkspaceSection,
    roots: &HashSet<PathBuf>,
) {
    if roots.is_empty() {
        return;
    }
    let under_removed_root =
        |path: &Path| path.ancestors().any(|ancestor| roots.contains(ancestor));
    for entry in section
        .imp()
        .pending_materializations
        .borrow_mut()
        .iter_mut()
    {
        if under_removed_root(&entry.path) {
            entry.retired = true;
        }
    }
}

/// Mark queued entries backed by `store` retired when a collapsed directory's
/// child store moves into `retiring_child_stores` before the `Priority::HIGH`
/// drain runs, returning whether any live queued entry was retired.
pub(super) fn retire_materializations_for_store(
    section: &LushtextWorkspaceSection,
    store: &gio::ListStore,
) -> bool {
    let target_ptr = store.as_ptr();
    let mut retired_any = false;
    for entry in section
        .imp()
        .pending_materializations
        .borrow_mut()
        .iter_mut()
    {
        if !entry.retired
            && entry
                .store
                .upgrade()
                .is_some_and(|candidate| candidate.as_ptr() == target_ptr)
        {
            entry.retired = true;
            retired_any = true;
        }
    }
    retired_any
}

/// Cancel the armed drain and discard all queued entries on full rebuild or dispose.
pub(super) fn retire_all_materializations(section: &LushtextWorkspaceSection) {
    let imp = section.imp();
    imp.materialization_drain_armed.set(false);
    if let Some(source) = imp.materialization_drain_source.borrow_mut().take() {
        source.remove();
    }
    imp.pending_materializations.borrow_mut().clear();
}

fn arm_materialization_drain(section: &LushtextWorkspaceSection) {
    let imp = section.imp();
    if imp.disposed.get() || imp.materialization_drain_armed.get() {
        return;
    }
    imp.materialization_drain_armed.set(true);
    let section_weak = section.downgrade();
    // `Priority::HIGH` dispatches in the same main-loop iteration, before
    // default-priority readiness polls, watch refreshes, and GDK redraws.
    let source = glib::idle_add_local_full(glib::Priority::HIGH, move || {
        let Some(section) = section_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        drain_pending_materializations(&section);
        glib::ControlFlow::Break
    });
    imp.materialization_drain_source
        .borrow_mut()
        .replace(source);
}

fn drain_pending_materializations(section: &LushtextWorkspaceSection) {
    let imp = section.imp();
    imp.materialization_drain_source.borrow_mut().take();
    imp.materialization_drain_armed.set(false);

    let (batch, rearm) = {
        let mut queue = imp.pending_materializations.borrow_mut();
        let step = materialization_drain_step(queue.len());
        let batch: Vec<PendingMaterialization> = queue.drain(..step.take).collect();
        (batch, step.rearm)
    };

    if rearm && !imp.disposed.get() {
        arm_materialization_drain(section);
    }

    let mut applied_materialize = false;

    for entry in batch {
        // Re-read inside the loop so a re-entrant lifetime bump or dispose during
        // an earlier `populate_child_store` call governs remaining batch entries.
        let current_lifetime = imp.child_scan_lifetime.get();
        let disposed = imp.disposed.get();
        let upgraded = entry.store.upgrade();
        let already_materialized = upgraded.as_ref().is_some_and(|store| {
            let store_key = super::scan_execution::child_store_key(store);
            super::scan_execution::child_store_identity_matches(section, store_key, store)
        });
        let facts = MaterializationFacts {
            store_is_alive: upgraded.is_some(),
            lifetime_is_current: !disposed && entry.lifetime == current_lifetime,
            path_retired: entry.retired,
            already_materialized,
        };
        match materialization_disposition(facts) {
            MaterializationDisposition::DiscardProbe => {
                let refresh = &imp.refresh_runtime;
                refresh
                    .child_model_probes_discarded
                    .set(refresh.child_model_probes_discarded.get().saturating_add(1));
            }
            MaterializationDisposition::Retired
            | MaterializationDisposition::AlreadyMaterialized => {}
            MaterializationDisposition::Materialize => {
                if let Some(store) = upgraded {
                    let refresh = &imp.refresh_runtime;
                    refresh
                        .child_materializations
                        .set(refresh.child_materializations.get().saturating_add(1));
                    applied_materialize = true;
                    super::scan_execution::populate_child_store(section, &entry.path, &store);
                }
            }
        }
    }

    if applied_materialize {
        super::scan_execution::sync_child_scan_busy_state(section);
    }
}

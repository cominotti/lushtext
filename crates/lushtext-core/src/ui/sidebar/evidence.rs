// SPDX-License-Identifier: GPL-3.0-or-later

//! The workspace tree workflow's one typed evidence surface.
//!
//! # Role
//!
//! This is the **evidence** role, at the workflow's canonical role home. It is the
//! single source of this workflow's observable state: widget tests read it, and the
//! `window.workspace` automation snapshot projects from it, so the workflow and the
//! exported contract share one derivation.
//!
//! # No materialization — the reason this surface exists at all
//!
//! Reading evidence MUST NOT make the toolkit do work. This workflow is the hazard
//! that rule was written for, because its model is a lazily materialized
//! `GtkTreeListModel`: several innocuous-looking accessors *create* state.
//!
//! Six such accessors are known and are recorded with line evidence in the change's
//! `evidence/evidence-surface-materialization.md`. **This surface reaches none of
//! them**, directly or transitively:
//!
//! | Hazard | Why it must not be reached |
//! | --- | --- |
//! | `scan_execution::build_children_model` | the `GtkTreeListModel` create function — the materialization entry point itself |
//! | `find_store_for_dir` | calls `row.children()` **and inserts into** the `dir_stores` cache: a nominal read materializes a child store, starts a background scan, and mutates a cache |
//! | `find_dir_row` | **evicts** from `dir_rows` on a lookup |
//! | `visible_child_stores` | calls `row.children()` with **no `is_expanded()` filter**, materializing every flattened row's children |
//! | `derive_expanded_paths_from_model` | advances the `expansion_capture_scans` / `expansion_capture_rows` counters **this surface itself reports** — an observer that changes the metric it observes is not an observation |
//! | `set_expanded(true)` | materializes children **and** fires the `notify::expanded` hook that queues a watcher restart |
//!
//! Instead, every expansion figure is derived from **`expanded_paths`**, which
//! `.agents/rules/ui.md` already names the authoritative live set, kept current by
//! row `notify::expanded` transitions, accepted reconciliation retirement, and rename
//! prefix rewrites. Deriving from the authoritative set is not a workaround for the
//! hazard; it is the correct source, and avoiding the hazard is a consequence.
//!
//! # Child-collection honesty and disposal safety
//!
//! This workflow's state lives across a **variable-sized set of per-workspace section
//! widgets**, so every aggregated field here is bounded by the live section count and
//! answers honestly when there are **zero** workspaces.
//!
//! On disposal this surface is safe for a stronger reason than a guarded read: **it
//! reads no `TemplateChild` at all.** Every per-section field comes from a `Cell` or
//! `RefCell` on the subclass's imp struct, which outlives GTK's `dispose()`, and the
//! one widget call it makes — `is_visible()` — is valid on a disposed-but-alive
//! widget. So there is no panicking accessor to guard.
//!
//! An earlier draft of this surface carried a `disposed_sections_skipped` field and a
//! `header_box.try_get()` predicate to feed it. Both were removed once the section's
//! `dispose()` was actually read: it does **not** call `dispose_template()`, so its
//! template children are never cleared and the predicate could never fire. A guard
//! that cannot fire is worse than no guard, because it implies a hazard has been
//! handled. If a future change makes the section clear its template children, add the
//! guard back **together with** a test that drives the state.
//!
//! # Borrow discipline
//!
//! One accessor reads the whole surface through shared borrows, so **no field may be
//! read from inside a mutable borrow of the state it reads**. That is a runtime panic
//! rather than a compile error, so [`LushtextSidebar::workspace_tree_evidence`]
//! computes every derived scalar into a local and drops each `Ref` **before** building
//! the struct literal. Do not add a second, narrower accessor to make a nested read
//! possible: that reintroduces the scattered getters this surface replaced.
//!
//! `workspace_snapshot_evidence` is not such an accessor. It is the *single derivation*
//! of the ten fields the exported `window.workspace` snapshot serializes, and the full
//! surface builds itself from its result rather than repeating it — so the snapshot
//! avoids allocating every section's collections per D-Bus poll while the two values
//! still cannot drift. An earlier revision hand-copied the body into both, which is the
//! duplication this note now forbids.
//!
//! # Scope honesty for the scan counters
//!
//! `process_active_scan_tasks`, `process_scan_task_high_water`, and
//! `process_scan_task_limit` are **process-global**, shared by every section in every
//! window. They are named `process_*` deliberately: an earlier design would have
//! presented them as per-section state, and a window with zero workspaces would then
//! have reported scans belonging elsewhere.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::LushtextSidebar;
use super::workspace_section::{self, LushtextWorkspaceSection};

/// One complete observation of the workspace tree workflow's state.
///
/// Every field is a scalar or an owned value: the surface never hands out a borrow,
/// a widget, or a GTK collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceTreeEvidence {
    // --- workspace list ---
    /// Persisted workspaces.
    pub workspace_count: usize,
    /// Configured folder memberships across all workspaces.
    pub folder_count: usize,
    /// Folder memberships covered by the current scope.
    pub scoped_folder_count: usize,
    /// Whether no workspace is persisted at all.
    pub no_workspaces: bool,
    /// Live section widgets, including any the filter has hidden.
    pub section_count: usize,
    /// Sections currently visible under the workspace scope filter.
    pub visible_section_count: usize,
    /// One observation per live section, in `sections` order.
    ///
    /// Bounded by the live section count, and empty with zero workspaces. The
    /// aggregates above are derived from exactly these, so a reader can always check an
    /// aggregate against its parts.
    pub sections: Vec<WorkspaceSectionEvidence>,

    // --- scope and its filter animation ---
    /// Current scope kind: `all` or `workspace`.
    pub scope_kind: String,
    /// Concrete workspace id when the scope targets one workspace.
    pub scope_workspace_id: Option<String>,
    /// User-visible name of the scoped workspace, if any.
    pub scope_workspace_name: Option<String>,
    /// Whether the scope filter fade sequence is running.
    pub filter_animation_active: bool,

    // --- persistence ---
    /// Whether persistence remains dirty, active, failed, or retry-waiting.
    pub persistence_pending: bool,
    /// Whether one snapshot currently owns the worker slot.
    pub persistence_inflight: bool,
    /// Whether the newest requested generation retains a failed terminal.
    pub persistence_failed: bool,
    /// Newest requested persistence generation.
    pub persistence_requested_generation: u64,
    /// Newest durably accepted persistence generation.
    pub persistence_durable_generation: u64,
    /// Close-time flush callbacks still awaiting a terminal.
    pub persistence_flush_waiters: usize,

    // --- expansion, derived from the authoritative live set only ---
    /// Directories the user currently has expanded, summed across sections.
    pub expanded_path_count: usize,
    /// Full-derivation scans performed so far, summed across sections.
    ///
    /// Reading this surface must never increase it; that is the assertion the
    /// no-materialization proof is built on.
    pub expansion_capture_scans: u64,
    /// Rows visited by those full derivations, summed across sections.
    pub expansion_capture_rows: u64,

    // --- scan admission (process-global; see the module doc) ---
    /// Directory-scan tasks currently admitted **process-wide**.
    pub process_active_scan_tasks: usize,
    /// High-water mark of admitted scan tasks **process-wide**.
    pub process_scan_task_high_water: usize,
    /// The **process-wide** admitted-scan ceiling.
    pub process_scan_task_limit: usize,

    // --- watch and refresh readiness ---
    /// Whether any section still has watcher, mailbox, or refresh work pending.
    pub refresh_blocks_readiness: bool,
    /// Sections whose watch-install worker is in flight.
    pub sections_with_watch_worker_inflight: usize,
    /// Sections whose watcher settled as unavailable for the current targets.
    pub sections_with_watch_unavailable: usize,
}

impl LushtextSidebar {
    /// Derive the scalars the exported `window.workspace` snapshot serializes.
    ///
    /// **This is not a second derivation — it is *the* derivation of these ten fields.**
    /// [`LushtextSidebar::workspace_tree_evidence`] calls it and moves the result into
    /// its own struct, so there is exactly one place each of these values is computed
    /// and drift between the surface and the snapshot is not expressible. An earlier
    /// revision hand-copied the body into both accessors, which is precisely the
    /// scattered-getter regression the evidence rules forbid.
    ///
    /// **Why the snapshot stops here rather than reading the full surface.** The
    /// snapshot is read on every read-only D-Bus poll and serializes ten scalars, while
    /// the full surface additionally clones each section's watch-target vector,
    /// expansion set, and both file-row identity sets. Building all of that to serialize
    /// ten numbers turns an O(1) poll into O(sections x rows) allocation.
    ///
    /// Inert for the same reasons as the full surface: it reads `workspaces_file`,
    /// `current_scope`, the persistence state, and one `Cell`, and reaches none of the
    /// six materializing accessors named in the module doc.
    #[must_use]
    pub(crate) fn workspace_snapshot_evidence(&self) -> WorkspaceSnapshotEvidence {
        let imp = self.imp();

        let (workspace_count, folder_count, no_workspaces) = {
            let file = imp.workspaces_file.borrow();
            (
                file.workspaces.len(),
                file.all_workspace_folder_paths().len(),
                file.workspaces.is_empty(),
            )
        };
        let scoped_folder_count = self.current_scope_folder_paths().len();

        let scope = imp.current_scope.borrow().clone();
        let scope_kind = super::policy::workspace_scope_kind_name(&scope).to_string();
        let (scope_workspace_id, scope_workspace_name) = match &scope {
            crate::model::workspace::WorkspaceScope::Workspace(id) => {
                // `None`, not `Some("")`, when the scoped workspace is not in the file.
                // The exported field is documented as "the selected workspace name, **if
                // any**", and `workspace_name_for_id` answers with an empty string for an
                // unknown id — so passing it through unfiltered would report a present
                // name that is not a name, next to a non-null `scope_workspace_id`.
                let name = self.workspace_name_for_id(id);
                (
                    Some(id.as_str().to_string()),
                    (!name.is_empty()).then_some(name),
                )
            }
            crate::model::workspace::WorkspaceScope::All => (None, None),
        };

        let (persistence_pending, persistence_inflight) = {
            let state = imp.persistence.borrow();
            (
                state.has_pending_work(),
                state.in_flight_generation().is_some(),
            )
        };

        WorkspaceSnapshotEvidence {
            scope_kind,
            scope_workspace_id,
            scope_workspace_name,
            workspace_count,
            folder_count,
            scoped_folder_count,
            no_workspaces,
            persistence_inflight,
            persistence_pending,
            filter_animation_active: imp.workspace_filter_animation_active.get(),
        }
    }

    /// Read one complete observation of this workflow's state.
    ///
    /// Side-effect free by construction, and that is a contract rather than an
    /// aspiration: see the module doc for the six accessors this must never reach and
    /// for the borrow discipline the body below follows.
    #[must_use]
    pub fn workspace_tree_evidence(&self) -> WorkspaceTreeEvidence {
        let imp = self.imp();

        // --- workspace list, scope, and the persistence booleans ---
        // Delegated, not repeated: these ten values have exactly one derivation, which
        // the exported snapshot also reads. Every borrow it takes is dropped before it
        // returns, so nothing here is read inside a live borrow.
        let shared = self.workspace_snapshot_evidence();

        // --- persistence generations, which the exported contract does not carry ---
        let (persistence_failed, persistence_requested_generation, persistence_durable_generation) = {
            let state = imp.persistence.borrow();
            (
                state.is_failed(),
                state.requested_generation().value(),
                state.durable_generation().value(),
            )
        };
        let persistence_flush_waiters = imp.persistence_flush_waiters.borrow().len();

        // --- per-section evidence, one pass over the live section set ---
        // The `sections` borrow is dropped before the struct literal is built.
        let section_evidence: Vec<WorkspaceSectionEvidence> = {
            let sections = imp.sections.borrow();
            sections
                .iter()
                .map(LushtextWorkspaceSection::workspace_section_evidence)
                .collect()
        };
        let visible_section_count = {
            let sections = imp.sections.borrow();
            sections
                .iter()
                .filter(|section| section.is_visible())
                .count()
        };
        // Every aggregate below derives from exactly those, so a reader can check an
        // aggregate against its parts.
        let section_count = section_evidence.len();
        let expanded_path_count: usize = section_evidence
            .iter()
            .map(|section| section.expanded_paths.len())
            .sum();
        let expansion_capture_scans: u64 = section_evidence
            .iter()
            .map(|section| section.expansion_capture_scans)
            .sum();
        let expansion_capture_rows: u64 = section_evidence
            .iter()
            .map(|section| section.expansion_capture_rows)
            .sum();
        let refresh_blocks_readiness = section_evidence
            .iter()
            .any(|section| section.refresh_blocks_readiness);
        let sections_with_watch_worker_inflight = section_evidence
            .iter()
            .filter(|section| section.watcher_worker_inflight)
            .count();
        let sections_with_watch_unavailable = section_evidence
            .iter()
            .filter(|section| section.watcher_unavailability_is_current)
            .count();

        WorkspaceTreeEvidence {
            workspace_count: shared.workspace_count,
            folder_count: shared.folder_count,
            scoped_folder_count: shared.scoped_folder_count,
            no_workspaces: shared.no_workspaces,
            section_count,
            visible_section_count,
            sections: section_evidence,
            scope_kind: shared.scope_kind,
            scope_workspace_id: shared.scope_workspace_id,
            scope_workspace_name: shared.scope_workspace_name,
            filter_animation_active: shared.filter_animation_active,
            persistence_pending: shared.persistence_pending,
            persistence_inflight: shared.persistence_inflight,
            persistence_failed,
            persistence_requested_generation,
            persistence_durable_generation,
            persistence_flush_waiters,
            expanded_path_count,
            expansion_capture_scans,
            expansion_capture_rows,
            process_active_scan_tasks: workspace_section::process_active_scan_tasks(),
            process_scan_task_high_water: workspace_section::process_scan_task_high_water(),
            process_scan_task_limit: workspace_section::process_scan_task_limit(),
            refresh_blocks_readiness,
            sections_with_watch_worker_inflight,
            sections_with_watch_unavailable,
        }
    }
}

/// The scalar subset of this workflow's state that the exported snapshot serializes.
///
/// [`WorkspaceTreeEvidence`] is built **from** one of these, so these fields have a
/// single derivation and the two cannot disagree. It exists only so a polled snapshot
/// need not allocate the full surface's per-section collections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceSnapshotEvidence {
    pub(crate) scope_kind: String,
    pub(crate) scope_workspace_id: Option<String>,
    pub(crate) scope_workspace_name: Option<String>,
    pub(crate) workspace_count: usize,
    pub(crate) folder_count: usize,
    pub(crate) scoped_folder_count: usize,
    pub(crate) no_workspaces: bool,
    pub(crate) persistence_inflight: bool,
    pub(crate) persistence_pending: bool,
    pub(crate) filter_animation_active: bool,
}

/// One complete observation of a single workspace **section**'s state.
///
/// The workflow owns one evidence *module*, and this is its second granularity: the
/// workflow's state genuinely lives at two levels, because a sidebar holds N sections
/// and most of what a reader wants to know — scan pressure, watch targets, expansion —
/// is per-section. [`WorkspaceTreeEvidence`] aggregates from these, and tests that hold
/// only a section read this directly.
///
/// The same rules apply here as to the sidebar surface: reads are inert (no accessor
/// below materializes toolkit state or advances a metric this type reports), every
/// derived scalar is computed into a local before the struct literal so no field is read
/// from inside a mutable borrow, and nothing here is a `TemplateChild`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSectionEvidence {
    /// Whether this section still has watcher, mailbox, or refresh work pending.
    pub refresh_blocks_readiness: bool,

    // --- directory-scan pressure and admission ---
    /// Per-store scan flight pressure, plus the process-global admission counters.
    pub scan_pressure: workspace_section::WorkspaceScanPressureEvidence,

    // --- refresh planning ---
    /// Targeted paths queued for the next refresh pass.
    pub refresh_pending_paths: usize,
    /// Whether a pending full reload dominates the queued targeted paths.
    pub refresh_pending_full_reload: bool,
    /// Batched reconciliation passes completed.
    pub reconcile_batch_count: u64,
    /// Largest single reconciliation batch, in rows.
    pub reconcile_max_batch_rows: usize,
    /// Reconciliations that reached a terminal.
    pub reconcile_terminal_count: u64,
    /// Reconciliations superseded before their terminal.
    pub reconcile_superseded_count: u64,
    /// Child stores currently sourcing a reconciliation.
    pub child_reconcile_sources: usize,
    /// Rows fed into child-cache rebuilds.
    pub cache_rebuild_input_rows: usize,
    /// Child-cache rebuild operations performed.
    pub cache_rebuild_operations: usize,
    /// Empty-probe reads issued for top-level folder rows.
    pub empty_probe_reads: u64,

    // --- watch ---
    /// The effective materialized watch target set.
    pub watch_targets: Vec<crate::services::workspace_watch::WorkspaceWatchTarget>,
    /// Monotonic identity of that target set.
    pub watch_target_generation: u64,
    /// Whether the installed watcher matches the current target generation.
    pub watcher_is_current: bool,
    /// Whether terminal unavailability belongs to the latest effective targets.
    pub watcher_unavailability_is_current: bool,
    /// Whether the install worker is in flight.
    pub watcher_worker_inflight: bool,
    /// Install workers started, for restart-churn assertions.
    pub watcher_worker_starts: usize,
    /// The installed handle's bounded coalescing mailbox, if a watcher is installed.
    pub watch_mailbox: Option<crate::services::workspace_watch::WorkspaceWatchMailboxSnapshot>,
    /// Notices GTK consumed on the most recent poll.
    pub watch_last_poll_notices: usize,
    /// Flattened rows the mirror has touched since the last reset.
    ///
    /// **Non-destructive.** The pre-convention seam this replaces was a `take`, so
    /// counting mutated. Resetting is now a separate test-only drive, which is what the
    /// evidence rules mean by "the reset must be separated from its observation".
    ///
    /// **Instrumentation, and honest about it: always `0` without `test-utils`.** The
    /// counter is recorded only under that feature, because incrementing it on every
    /// mirror splice is churn a production build should not pay for a number nothing
    /// reads. Recorded here rather than left for a reader to discover, which is the
    /// same honesty the `process_*` scan counters owe about their scope.
    pub watch_target_rows_touched: usize,

    // --- expansion, from the authoritative live set only ---
    /// Directories the user currently has expanded.
    pub expanded_paths: std::collections::HashSet<std::path::PathBuf>,
    /// Full-derivation scans performed. Reading this must never increase it.
    pub expansion_capture_scans: u64,
    /// Rows visited across those full derivations.
    pub expansion_capture_rows: u64,

    // --- context menu target ---
    /// Path the most recent context-menu interaction targeted.
    pub context_target_path: Option<std::path::PathBuf>,
    /// Workspace folder id of that target, when it is a top-level folder row.
    pub context_target_workspace_folder_id: Option<crate::model::workspace::WorkspaceFolderId>,

    // --- reorder drag ---
    /// Times the reorder drag fell back to the inert row shield.
    pub reorder_drag_hover_fallback_count: usize,

    // --- the window's file-row projection, as this section sees it ---
    /// Paths this section renders with the open-tab marker.
    pub open_row_paths: std::collections::HashSet<std::path::PathBuf>,
    /// Paths this section renders with the active-tab marker.
    pub active_row_paths: std::collections::HashSet<std::path::PathBuf>,
}

impl LushtextWorkspaceSection {
    /// Read one complete observation of this section's state.
    ///
    /// Inert: no accessor reached here materializes a child store, runs the
    /// `GtkTreeListModel` create function, advances an expansion-capture counter, or
    /// mutates a cache. Expansion comes from `expanded_paths`, the authoritative live
    /// set.
    #[must_use]
    pub fn workspace_section_evidence(&self) -> WorkspaceSectionEvidence {
        let imp = self.imp();

        let refresh_blocks_readiness = self.workspace_refresh_blocks_readiness();
        let scan_pressure = self.child_scan_pressure_evidence();

        let (refresh_pending_paths, refresh_pending_full_reload) = {
            let runtime = &imp.refresh_runtime;
            (
                runtime.pending_paths.borrow().len(),
                runtime.pending_full_reload.get(),
            )
        };
        let refresh = &imp.refresh_runtime;
        let reconcile_batch_count = refresh.reconcile_batch_count.get();
        let reconcile_max_batch_rows = refresh.reconcile_max_batch_rows.get();
        let reconcile_terminal_count = refresh.reconcile_terminal_count.get();
        let reconcile_superseded_count = refresh.reconcile_superseded_count.get();
        let child_reconcile_sources = imp.child_reconcile_sources.borrow().len();
        let cache_rebuild_input_rows = refresh.cache_rebuild_input_rows.get();
        let cache_rebuild_operations = refresh.cache_rebuild_operations.get();
        let empty_probe_reads = refresh.empty_probe_reads_for_evidence();

        let (
            watch_targets,
            watch_target_generation,
            watch_target_rows_touched,
            watcher_is_current,
            watcher_unavailability_is_current,
            watch_mailbox,
        ) = imp.watch_runtime.watch_evidence();
        let watcher_worker_inflight = imp.watch_runtime.watch_worker_inflight_for_evidence();
        let watcher_worker_starts = imp.watch_runtime.worker_starts_for_evidence();
        let watch_last_poll_notices = imp.watch_runtime.last_poll_notices_for_evidence();

        let expanded_paths = imp.expanded_paths.borrow().clone();
        let expansion_capture_scans = refresh.expansion_capture_scans.get();
        let expansion_capture_rows = refresh.expansion_capture_rows.get();

        let (context_target_path, context_target_workspace_folder_id) =
            imp.context_target_evidence();

        let reorder_drag_hover_fallback_count = workspace_section::drag_hover_child_model_count();

        let (open_row_paths, active_row_paths) = {
            let snapshot = imp.file_row_state_snapshot.borrow();
            (snapshot.open_identities(), snapshot.active_identities())
        };

        WorkspaceSectionEvidence {
            refresh_blocks_readiness,
            scan_pressure,
            refresh_pending_paths,
            refresh_pending_full_reload,
            reconcile_batch_count,
            reconcile_max_batch_rows,
            reconcile_terminal_count,
            reconcile_superseded_count,
            child_reconcile_sources,
            cache_rebuild_input_rows,
            cache_rebuild_operations,
            empty_probe_reads,
            watch_targets,
            watch_target_generation,
            watcher_is_current,
            watcher_unavailability_is_current,
            watcher_worker_inflight,
            watcher_worker_starts,
            watch_mailbox,
            watch_last_poll_notices,
            watch_target_rows_touched,
            expanded_paths,
            expansion_capture_scans,
            expansion_capture_rows,
            context_target_path,
            context_target_workspace_folder_id,
            reorder_drag_hover_fallback_count,
            open_row_paths,
            active_row_paths,
        }
    }
}

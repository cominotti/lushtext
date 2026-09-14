// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the eviction workflow's single observable state.
//!
//! One accessor reads the whole surface, replacing six separate `*_for_test`
//! inspection functions that each read one counter.
//!
//! * **Reading must not mutate.** Every field is a `Cell` read or a borrow of
//!   the ledger's already-computed aggregate. In particular the surface must not
//!   call `reconcile()` or `upsert()` to find out what the totals *would* be:
//!   those advance the ledger's own accounting, and an observer that changes the
//!   metric it reports is not an observation.
//! * **No field may be read from inside a mutable borrow.** `ledger` is a
//!   `RefCell`, and the workflow takes `borrow_mut()` on it during reconciliation
//!   and upsert. Every scalar below is therefore computed *before* the struct
//!   literal and the `Ref` is dropped, so no read outlives the value it
//!   produced. Reading this surface from inside `update_editor_memory_record`
//!   would panic at runtime rather than fail to compile, which is why the
//!   constraint is stated here rather than left to discipline.
//! * **A disposed widget is a stage, and it is a stage *transitively*.** The
//!   window's `tab_view` is reached through `try_get()`, but that is only half
//!   of it: `live_residency` walks the pages, and
//!   `LushtextEditorPage::estimated_live_buffer_bytes` and
//!   `eligible_for_memory_eviction` both reach `source_view()` — a panicking
//!   `TemplateChild` deref — through `buffer()` and `is_modified()`. A page
//!   whose own `dispose()` has run is therefore **skipped**, via
//!   `LushtextEditorPage::is_disposed`, rather than panicked on. This is the
//!   bounded-child half of the rule: aggregate over a variable-sized set of
//!   child widgets, answer honestly when it is empty, and skip a disposed child
//!   rather than crashing on it. The production accessors are left alone,
//!   because production only ever reaches a live page.
//!
//!   **The guard is proved by deliberate red at the code level, and no widget
//!   test is retained for it — deliberately.** Replacing `is_disposed()` with
//!   `false` and disposing a page that is still in the tab view panics this
//!   surface with *"Failed to retrieve template child"*, so the guard is load-
//!   bearing rather than decorative. But arranging that state requires
//!   `run_dispose()` on a page whose `AdwTabPage` still holds it, which GTK
//!   itself reports as `Gtk-CRITICAL ... has a parent ... during dispose` — an
//!   incorrect teardown, not a reachable production state. The guard stays
//!   because the rule this workflow's own change wrote requires an evidence
//!   surface not to reach a disposed child *transitively*, and because it costs
//!   one comparison; the absence of a test for it is recorded here rather than
//!   left for a later reader to mistake for an oversight.
//!
//! Reading materializes nothing: the ledger is a plain scalar map, and the tab
//! walk inspects an eagerly-held `AdwTabView`.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::model::editor_memory::{EditorMemoryBudgetOutcome, EditorResidency};
use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;

/// Everything a test or probe may observe about editor-memory eviction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorMemoryEvictionEvidence {
    /// Completed aggregate passes, for burst-coalescing assertions.
    pub evaluation_count: u64,
    /// Full tab scans performed for enforcement or reconciliation.
    pub full_scan_count: u64,
    /// The ledger's current saturating byte total across resident editors.
    pub incremental_total_bytes: u64,
    /// Whether the ledger's aggregate currently reconciles with its records.
    pub ledger_reconciles: bool,
    /// Result of the most recently completed aggregate pass.
    pub last_outcome: EditorMemoryBudgetOutcome,
    /// Whether a pass is running right now.
    pub evaluation_running: bool,
    /// Whether a transition arrived that a running pass must resnapshot for.
    pub evaluation_armed: bool,
    /// Whether the aggregate is currently untrustworthy.
    pub accounting_uncertain: bool,
    /// Idle dispatches used to prove candidate application stays bounded.
    pub eviction_dispatch_count: u64,
    /// Editor pages currently open, bounded by the tab count.
    ///
    /// `0` when the tab view is gone, and a disposed page is skipped rather
    /// than panicked on.
    pub resident_pages: usize,
}

/// Read the whole editor-memory eviction surface.
#[must_use]
pub fn editor_memory_eviction_evidence(window: &LushtextWindow) -> EditorMemoryEvictionEvidence {
    let memory = &window.imp().editor_memory;

    // Every ledger-derived scalar is taken first and the borrow dropped before
    // the struct literal below, so no read is live while another is taken.
    // The reconciliation check needs a live GTK snapshot, so it is built
    // *before* the ledger borrow is taken and compared after — never while a
    // borrow from an earlier field is still alive.
    let observed = live_residency(window);
    let (incremental_total_bytes, ledger_reconciles) = {
        let ledger = memory.ledger.borrow();
        (ledger.total_bytes(), ledger.snapshot() == observed)
    };

    EditorMemoryEvictionEvidence {
        evaluation_count: memory.evaluation_count.get(),
        full_scan_count: memory.full_scan_count.get(),
        incremental_total_bytes,
        ledger_reconciles,
        last_outcome: memory.last_outcome.get(),
        evaluation_running: memory.evaluation_running.get(),
        evaluation_armed: memory.evaluation_armed.get(),
        accounting_uncertain: memory.accounting_uncertain.get(),
        eviction_dispatch_count: memory.eviction_dispatch_count.get(),
        resident_pages: resident_pages(window),
    }
}

/// One current GTK-observed residency snapshot, sorted to match the ledger's.
///
/// Read-only: it builds a local vector and never calls `reconcile()` or
/// `upsert()`, because those advance the very accounting this surface reports.
fn live_residency(window: &LushtextWindow) -> Vec<EditorResidency> {
    let Some(tab_view) = window.imp().tab_view.try_get() else {
        return Vec::new();
    };
    let selected = tab_view.selected_page();
    let mut current = Vec::with_capacity(usize::try_from(tab_view.n_pages()).unwrap_or(0));
    for index in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(index);
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
            // Skip a page whose template children are already cleared: every
            // field below reaches `source_view()` transitively and would panic.
            if editor.is_disposed() {
                continue;
            }
            current.push(EditorResidency {
                editor_id: editor.as_ptr() as usize,
                estimated_bytes: editor.estimated_live_buffer_bytes(),
                access_generation: editor.memory_access_generation(),
                policy_generation: editor.memory_policy_generation(),
                eligible_for_eviction: editor
                    .eligible_for_memory_eviction(selected.as_ref() == Some(&page)),
            });
        }
    }
    current.sort_unstable_by_key(|record| record.editor_id);
    current
}

/// Count the open editor pages without dereferencing a cleared template child.
fn resident_pages(window: &LushtextWindow) -> usize {
    let Some(tab_view) = window.imp().tab_view.try_get() else {
        return 0;
    };
    (0..tab_view.n_pages())
        .filter(|index| {
            tab_view
                .nth_page(*index)
                .child()
                .downcast_ref::<LushtextEditorPage>()
                .is_some()
        })
        .count()
}

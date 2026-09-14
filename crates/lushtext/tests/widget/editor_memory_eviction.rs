// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget and window-integration tests for `WFR-EDITOR-MEMORY-EVICTION`.
//!
//! The eviction *behavior* — coalescing, budget enforcement, protected pages,
//! stale-plan handling — is covered by the long-standing tests in `window.rs`,
//! which now read this row's evidence surface. This module carries what the
//! migration added: the three mandated surface proofs, and the state-extreme
//! sweep for a workflow whose aggregate is only meaningful across tab counts.

use crate::common::{ensure_gtk_init, flush_events, present_window, test_window};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::window::{
    EditorMemoryEvictionEvidence, LushtextWindow, editor_memory_eviction_evidence,
};

fn evidence(window: &LushtextWindow) -> EditorMemoryEvictionEvidence {
    editor_memory_eviction_evidence(window)
}

/// Proof 1 of 3 — **reentrancy / side-effect freedom**.
///
/// The surface is read after each operation that takes a mutable borrow of the
/// state it reads, and repeated reads of unchanged state must be identical.
///
/// This row's specific hazard is named in the surface's module doc: `ledger` is a
/// `RefCell` that the workflow takes `borrow_mut()` on during `upsert()` and
/// `reconcile()`, and the surface derives two fields from it. A surface that
/// held one borrow while taking another — or that called `reconcile()` to find
/// out whether the ledger reconciles — would panic at runtime rather than fail
/// to compile. The driving operations below each take that mutable borrow.
#[test]
fn test_editor_memory_eviction_evidence_reads_stay_side_effect_free_across_ledger_mutation() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    // No context: no tabs, nothing tracked.
    let empty = evidence(&window);
    assert_eq!(empty.resident_pages, 0);
    assert_eq!(empty.incremental_total_bytes, 0);
    assert!(empty.ledger_reconciles);
    assert!(!empty.evaluation_running);
    assert_eq!(evidence(&window), empty, "repeated reads must be equal");

    // `new_tab` reaches `track_editor_memory` -> `update_editor_memory_record`,
    // which takes `ledger.borrow_mut()` for an upsert.
    window.new_tab();
    flush_events();
    let one_tab = evidence(&window);
    assert_eq!(one_tab.resident_pages, 1);
    assert!(
        one_tab.ledger_reconciles,
        "the ledger must agree with live residency after a tracked insert"
    );
    assert_eq!(evidence(&window), one_tab);

    // Editing mutates the residency estimate and the policy generation.
    let editor = window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| {
            page.child()
                .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
                .ok()
        })
        .expect("an active editor");
    editor.buffer().set_text("residency probe\n");
    flush_events();
    let dirty = evidence(&window);
    assert!(dirty.ledger_reconciles);
    assert_eq!(evidence(&window), dirty);

    // A second tab, then closing back down: both take `borrow_mut()`, the second
    // through `untrack_editor_memory` -> `ledger.remove()`.
    window.new_tab();
    flush_events();
    let two_tabs = evidence(&window);
    assert_eq!(two_tabs.resident_pages, 2);
    assert_eq!(evidence(&window), two_tabs);

    let tab_view = window.imp().tab_view.clone();
    tab_view.close_page(&tab_view.nth_page(1));
    flush_events();
    let after_close = evidence(&window);
    assert_eq!(after_close.resident_pages, 1);
    assert!(
        after_close.ledger_reconciles,
        "a detached page's record must be removed, not stranded"
    );
    assert_eq!(evidence(&window), after_close);
}

/// Proof 2 of 3 — **disposal**.
///
/// GTK4 clears template children in `dispose()` before Rust's `Drop`. This
/// surface's tab-view walk feeds two fields — `resident_pages` and the live
/// snapshot behind `ledger_reconciles` — so a cleared child must yield an honest
/// answer for a *set*, not a panic.
#[test]
fn test_editor_memory_eviction_evidence_answers_honestly_after_the_window_is_disposed() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    window.new_tab();
    window.new_tab();
    flush_events();
    assert_eq!(evidence(&window).resident_pages, 2);

    // Closing is not disposing.
    window.close();
    flush_events();
    assert_eq!(
        evidence(&window).resident_pages,
        2,
        "sanity: closing is not disposing"
    );

    // SAFETY: this test window is disposed exactly once, and everything after
    // this point only reads the evidence surface, which must answer honestly on
    // a disposed widget rather than panicking.
    unsafe { window.run_dispose() };

    let disposed = evidence(&window);
    assert_eq!(
        disposed.resident_pages, 0,
        "a cleared tab view must report zero resident pages rather than panicking"
    );
    assert_eq!(
        evidence(&window),
        disposed,
        "repeated reads of a disposed window must stay identical"
    );
}

/// Proof 3 of 3 — **non-materialization**.
///
/// The requirement fires for a surface covering a lazily created collection.
/// This surface walks `AdwTabView`, which holds its pages eagerly. The sharper
/// risk here is not toolkit materialization but **ledger** mutation: the
/// `ledger_reconciles` field is a comparison against a freshly built live
/// snapshot, and computing it via `reconcile()` — the obvious implementation —
/// would rewrite the ledger and advance the very accounting the surface reports.
/// Every counter is therefore compared before and after each read.
#[test]
fn test_editor_memory_eviction_evidence_reads_materialize_no_state() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    // Empty extreme.
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    let before_empty = evidence(&window);
    let _ = evidence(&window);
    let _ = evidence(&window);
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert_eq!(
        evidence(&window),
        before_empty,
        "reading an empty window must change nothing"
    );

    window.new_tab();
    window.new_tab();
    flush_events();

    let pages_before = window.imp().tab_view.n_pages();
    let first = evidence(&window);
    let second = evidence(&window);
    let third = evidence(&window);

    assert_eq!(first, second);
    assert_eq!(second, third);
    assert_eq!(
        window.imp().tab_view.n_pages(),
        pages_before,
        "reading must not materialize a page"
    );
    // The counters the surface reports must not be advanced by reporting them.
    assert_eq!(first.evaluation_count, third.evaluation_count);
    assert_eq!(first.full_scan_count, third.full_scan_count);
    assert_eq!(
        first.incremental_total_bytes, third.incremental_total_bytes,
        "reading must not re-reconcile the ledger"
    );
    assert_eq!(first.eviction_dispatch_count, third.eviction_dispatch_count);
}

/// State-extreme sweep plus the row's data-safety obligation (task 7.4).
///
/// The revalidation rule is what stands between a stale plan and a cleared
/// buffer, and its `is_active` clause is the one a swap would silently invert.
/// This drives the real workflow across tab counts and asserts that the selected
/// page is never the one reclaimed.
#[test]
fn test_editor_memory_eviction_never_reclaims_the_selected_page() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    // Extreme 1 — no tabs. Selecting nothing must be safe and must not wedge the
    // guard. The workflow's production trigger is
    // `refresh_selected_tab_model_projections`, reached by a tab-selection
    // change; with no pages there is nothing to select, so the surface is read
    // directly to confirm the empty state rather than spending a test seam.
    flush_events();
    let no_tabs = evidence(&window);
    assert_eq!(no_tabs.resident_pages, 0);
    assert!(
        !no_tabs.evaluation_running,
        "a pass over an empty window must not leave the overlap guard set"
    );

    // Extreme 2 — several tabs with content, then an explicit pass.
    for index in 0..4 {
        window.new_tab();
        flush_events();
        if let Some(editor) = window.imp().tab_view.selected_page().and_then(|page| {
            page.child()
                .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
                .ok()
        }) {
            editor.buffer().set_text(&format!("tab {index}\n"));
        }
    }
    flush_events();

    // The production trigger: switch selection, which stamps recency, reloads an
    // evicted page, and runs the budget nudge — all through
    // `refresh_selected_tab_model_projections`. No test seam is spent.
    let tab_view = window.imp().tab_view.clone();
    tab_view.set_selected_page(&tab_view.nth_page(0));
    flush_events();
    tab_view.set_selected_page(&tab_view.nth_page(3));
    flush_events();
    let selected_before = window.imp().tab_view.selected_page();
    flush_events();

    let populated = evidence(&window);
    assert_eq!(populated.resident_pages, 4);
    assert!(
        populated.ledger_reconciles,
        "the ledger must agree with live residency after a pass"
    );
    assert_eq!(
        window.imp().tab_view.selected_page(),
        selected_before,
        "a memory pass must not change which page is selected"
    );

    // The data-safety assertion: whatever the pass decided, the selected page's
    // buffer is still resident. `policy::candidate_is_still_evictable` vetoes an
    // active candidate, and that veto is the whole protection.
    let selected_editor = window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| {
            page.child()
                .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
                .ok()
        })
        .expect("a selected editor");
    assert!(
        !selected_editor.is_evicted(),
        "the selected page must never be evicted"
    );

    // And the guard is released, so a later transition can still start a pass.
    assert!(
        !populated.evaluation_running || populated.evaluation_armed,
        "a finished pass must release the overlap guard or record a re-arm"
    );
}

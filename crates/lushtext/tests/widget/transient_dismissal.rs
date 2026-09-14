// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget and window-integration tests for `WFR-TRANSIENT-DISMISSAL`.
//!
//! The row's Escape-ladder and click-away *behavior* is covered by the
//! long-standing tests in `command_palette.rs`, which drive the real decision
//! through `handle_transient_escape_for_test`. This module carries what the
//! migration added: the three mandated proofs for the row's evidence surface,
//! plus the state-extreme sweep for a shell surface that must answer with no
//! tabs, with one, and with several.

use crate::common::{ensure_gtk_init, flush_events, present_window, test_window};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::window::{
    EscapeOutcome, LushtextWindow, TransientDismissal, TransientDismissalEvidence,
    transient_dismissal_evidence,
};

fn evidence(window: &LushtextWindow) -> TransientDismissalEvidence {
    transient_dismissal_evidence(window)
}

fn activate_action(window: &LushtextWindow, name: &str) {
    gtk4::prelude::ActionGroupExt::activate_action(window, name, None);
}

/// Proof 1 of 3 — **reentrancy / side-effect freedom**.
///
/// The surface is read *after* each operation that takes a mutable borrow of the
/// state it reads, and repeated reads of unchanged state must be identical. The
/// specific hazard this row carries is the child-handled latch: the workflow's
/// own Escape path consumes it with `Cell::replace(false)` while the surface must
/// only `get()` it. A surface that consumed the latch would pass a naive
/// equality check on the *first* read pair and then silently change what the next
/// real Escape does.
#[test]
fn test_transient_dismissal_evidence_reads_stay_side_effect_free_across_shell_mutation() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    // No context: no tabs, no surfaces, no Focus Mode.
    let empty = evidence(&window);
    assert!(!empty.surfaces.command_palette_revealed);
    assert!(!empty.surfaces.editor_search_visible);
    assert!(!empty.surfaces.search_panel_revealed);
    assert!(!empty.child_escape_handled);
    assert!(!empty.focus_mode_active);
    assert_eq!(empty.topmost_dismissible, None);
    assert_eq!(empty.escape_outcome, EscapeOutcome::Unhandled);
    assert_eq!(evidence(&window), empty, "repeated reads must be equal");

    // Tab creation takes a mutable borrow of the window's open-path state.
    window.new_tab();
    flush_events();
    let with_tab = evidence(&window);
    assert_eq!(with_tab.topmost_dismissible, None);
    assert_eq!(evidence(&window), with_tab);

    // Opening the palette mutates revealer state and the palette's saved focus.
    activate_action(&window, "toggle-command-palette");
    flush_events();
    let with_palette = evidence(&window);
    assert!(with_palette.surfaces.command_palette_revealed);
    assert_eq!(
        with_palette.topmost_dismissible,
        Some(TransientDismissal::CommandPalette)
    );
    assert_eq!(
        with_palette.escape_outcome,
        EscapeOutcome::Dismiss(TransientDismissal::CommandPalette)
    );
    assert_eq!(evidence(&window), with_palette);

    // The latch, reached through its real production path rather than a new test
    // seam: Escape inside the palette arrives as the search entry's `stop-search`,
    // the palette emits `close-requested`, and the window latches *and* closes the
    // palette in that handler. No actuation seam is spent to get here.
    //
    // Deliberately **not** followed by `flush_events()`. Signal emission is
    // synchronous, so the latch is set by the time this call returns — but it is
    // cleared by a `glib::idle_add_local_once` one main-loop turn later, which a
    // flush would run. Flushing here would observe `false` and the test would
    // read as though the latch were never set.
    window
        .imp()
        .command_palette
        .imp()
        .search_entry
        .emit_stop_search();

    let latched = evidence(&window);
    assert!(
        latched.child_escape_handled,
        "the palette's close-requested handler must latch the Escape"
    );
    assert!(
        !latched.surfaces.command_palette_revealed,
        "the same handler closed the palette"
    );
    assert_eq!(
        latched.escape_outcome,
        EscapeOutcome::ChildAlreadyHandled,
        "the latch outranks the ladder and Focus Mode"
    );
    // The load-bearing assertion: two reads of the latch must leave it spendable.
    // `Cell::get()` here versus the workflow's `Cell::replace(false)` is the whole
    // difference, and both spellings compile and return the same `bool`.
    assert_eq!(
        evidence(&window),
        latched,
        "reading the latch must not consume it"
    );
    assert!(
        window.handle_transient_escape_for_test(),
        "the latch survived two surface reads and is still spendable"
    );

    // Spent exactly once: the next Escape finds nothing and declines.
    let after_dismiss = evidence(&window);
    assert!(!after_dismiss.child_escape_handled);
    assert_eq!(after_dismiss.escape_outcome, EscapeOutcome::Unhandled);
    assert_eq!(evidence(&window), after_dismiss);
    assert!(!window.handle_transient_escape_for_test());

    // Closing back down to no tabs mutates the same state in the other direction.
    let tab_view = &window.imp().tab_view;
    tab_view.close_page(&tab_view.nth_page(0));
    flush_events();
    let after_close = evidence(&window);
    assert_eq!(evidence(&window), after_close);
}

/// Proof 2 of 3 — **disposal**.
///
/// A disposed widget is a stage. GTK4 clears template children in `dispose()`,
/// before Rust's `Drop`, so every field derived from one must be read through
/// `try_get()` and answer honestly. Two convenience accessors are the trap here
/// and the surface must not use either: `LushtextWindow::active_editor` derefs
/// `imp().tab_view`, and `LushtextEditorPage::is_search_visible` derefs the
/// editor's own `search_revealer`. Both read as ordinary calls at the call site.
#[test]
fn test_transient_dismissal_evidence_answers_honestly_after_the_window_is_disposed() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    window.new_tab();
    activate_action(&window, "toggle-command-palette");
    flush_events();
    assert!(evidence(&window).surfaces.command_palette_revealed);

    // Closing is not disposing: GTK defers template-child teardown, so a
    // close-only test would prove nothing about this contract.
    window.close();
    flush_events();
    assert!(
        evidence(&window).surfaces.command_palette_revealed,
        "sanity: closing is not disposing, so the palette is still visible here"
    );

    // SAFETY: this test window is disposed exactly once, and everything after
    // this point only reads the evidence surface, which must answer honestly on
    // a disposed widget rather than panicking.
    unsafe { window.run_dispose() };

    let disposed = evidence(&window);
    assert!(
        !disposed.surfaces.command_palette_revealed,
        "a disposed window must report no visible palette rather than panicking"
    );
    assert!(
        !disposed.surfaces.editor_search_visible,
        "the editor's own search revealer must be reached through try_get()"
    );
    assert!(!disposed.surfaces.search_panel_revealed);
    assert!(!disposed.surfaces.primary_menu_active);
    assert!(!disposed.surfaces.notes_menu_active);
    assert_eq!(
        disposed.topmost_dismissible, None,
        "a disposed window owns no dismissible surface"
    );
    assert_eq!(
        evidence(&window),
        disposed,
        "repeated reads of a disposed window must stay identical"
    );
}

/// Proof 3 of 3 — **non-materialization**.
///
/// Recorded with its reason rather than skipped. The requirement fires for a
/// surface covering a lazily created collection — `GtkTreeListModel` is the one
/// in this tree, whose accessors *perform* work by materializing descendants.
/// This surface reads two revealers' `reveals_child()`, two menu buttons'
/// `is_active()`, and `AdwTabView::selected_page()`. `AdwTabView` holds its pages
/// eagerly and registers no store, so there is no unmaterialized state a read
/// could bring into being. The observable proxy is asserted anyway, in both the
/// empty and the populated extreme, because "prove it, do not assert it" applies
/// to the negative finding too.
#[test]
fn test_transient_dismissal_evidence_reads_materialize_no_toolkit_state() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    // Empty extreme: reading a window with no pages must not create one.
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    let _ = evidence(&window);
    let _ = evidence(&window);
    assert_eq!(
        window.imp().tab_view.n_pages(),
        0,
        "reading the surface must not materialize a page"
    );

    window.new_tab();
    window.new_tab();
    flush_events();

    let pages_before = window.imp().tab_view.n_pages();
    let selected_before = window
        .imp()
        .tab_view
        .selected_page()
        .map(|page| page.child());

    let first = evidence(&window);
    let second = evidence(&window);
    assert_eq!(first, second);

    assert_eq!(
        window.imp().tab_view.n_pages(),
        pages_before,
        "reading the surface must not change the page count"
    );
    assert_eq!(
        window
            .imp()
            .tab_view
            .selected_page()
            .map(|page| page.child()),
        selected_before,
        "reading the surface must not change the selection"
    );
}

/// State-extreme sweep: the ladder must answer for every surface it can dismiss,
/// including the two header menus, which no existing test reaches.
///
/// The menus are the untested half of the ladder: `command_palette.rs` covers the
/// palette and the two search surfaces, and nothing covered the menu arms, so a
/// swapped `primary_menu_active`/`notes_menu_active` pair would have been
/// invisible — which is the exact defect the seam value object exists to prevent.
#[test]
fn test_transient_dismissal_ladder_dismisses_each_header_menu() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    window.new_tab();
    flush_events();

    for (label, button) in [
        ("primary", window.imp().primary_menu_button.clone()),
        ("notes", window.imp().notes_menu_button.clone()),
    ] {
        button.popup();
        flush_events();
        if !button.is_active() {
            // A headless session may decline to map the popover; skip rather than
            // assert against the compositor, but do not silently pass the arm.
            eprintln!("transient-dismissal: {label} menu did not activate; arm skipped");
            continue;
        }

        let observed = evidence(&window);
        let expected = if label == "primary" {
            TransientDismissal::PrimaryMenu
        } else {
            TransientDismissal::NotesMenu
        };
        assert_eq!(
            observed.topmost_dismissible,
            Some(expected),
            "{label} menu must be the topmost dismissible surface"
        );

        assert!(window.handle_transient_escape_for_test());
        flush_events();
        assert!(
            !button.is_active(),
            "{label} menu must be closed by one Escape"
        );
        assert_eq!(evidence(&window).topmost_dismissible, None);
    }
}

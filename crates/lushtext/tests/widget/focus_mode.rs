// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget and window-integration tests for `WFR-FOCUS-MODE`.
//!
//! Focus Mode's chrome, affordance, and fullscreen *behavior* is covered by the
//! long-standing tests in `window.rs`. This module carries what the migration
//! added: the three mandated proofs for the row's evidence surface, and the
//! state-extreme sweep for a workflow whose whole contract — restore exactly
//! what was borrowed — is only visible across several entry states.

use crate::common::{ensure_gtk_init, flush_events, present_window, test_window};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::window::{FocusModeEvidence, LushtextWindow, focus_mode_evidence};

fn evidence(window: &LushtextWindow) -> FocusModeEvidence {
    focus_mode_evidence(window)
}

fn set_focus_mode(window: &LushtextWindow, active: bool) {
    gtk4::prelude::ActionGroupExt::activate_action(
        window,
        "set-focus-mode",
        Some(&active.to_variant()),
    );
    flush_events();
}

/// Proof 1 of 3 — **reentrancy / side-effect freedom**.
///
/// The surface is read after each operation that takes a mutable borrow of the
/// state it reads, and repeated reads of unchanged state must be identical.
/// The hazard specific to this row is that several fields are *derivations* over
/// live shell state — the exit-restoration decision, the readable-column
/// applicability, the count of editors carrying the mode — so a surface that
/// computed them by calling the workflow's own `apply_*` helpers would set widget
/// state while reporting on it.
#[test]
fn test_focus_mode_evidence_reads_stay_side_effect_free_across_shell_mutation() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    // No context: not focused, no tabs.
    let empty = evidence(&window);
    assert!(!empty.active);
    assert!(empty.header_bar_visible);
    assert!(empty.status_bar_visible);
    assert_eq!(empty.editors_with_focus_mode, 0);
    assert!(!empty.preview_takes_readable_column);
    assert_eq!(evidence(&window), empty, "repeated reads must be equal");

    // Tab creation takes a mutable borrow of the window's open-path state.
    window.new_tab();
    flush_events();
    let with_tab = evidence(&window);
    assert_eq!(with_tab.editors_with_focus_mode, 0);
    assert_eq!(evidence(&window), with_tab);

    // Entering mutates the capture cells, chrome visibility, and every editor.
    set_focus_mode(&window, true);
    let focused = evidence(&window);
    assert!(focused.active);
    assert!(
        !focused.header_bar_visible && !focused.status_bar_visible,
        "entering must suppress persistent chrome"
    );
    assert_eq!(
        focused.editors_with_focus_mode, 1,
        "the open editor must carry the mode"
    );
    assert_eq!(evidence(&window), focused);

    // A second tab while focused: the new page must match the shell, and the
    // count is a bounded aggregate over a set that just changed size.
    window.new_tab();
    flush_events();
    let two_tabs = evidence(&window);
    assert_eq!(two_tabs.editors_with_focus_mode, 2);
    assert_eq!(evidence(&window), two_tabs);

    // An explicit preview choice, reached through its real production path
    // rather than a new test seam: activating `toggle-preview-pane` while
    // focused takes the early-return branch that records the choice and changes
    // nothing else.
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-preview-pane", None);
    flush_events();
    let after_choice = evidence(&window);
    assert!(after_choice.preview_changed_while_focused);
    assert!(
        !after_choice.exit_restores.side_by_side_preview,
        "an explicit choice must suppress restoration"
    );
    assert_eq!(evidence(&window), after_choice);

    // Exiting restores chrome and clears the mode from every editor.
    set_focus_mode(&window, false);
    let restored = evidence(&window);
    assert!(!restored.active);
    assert!(restored.header_bar_visible && restored.status_bar_visible);
    assert_eq!(restored.editors_with_focus_mode, 0);
    assert_eq!(evidence(&window), restored);
}

/// Proof 2 of 3 — **disposal**.
///
/// Every field derived from a `TemplateChild` is reached through `try_get()` in
/// the row's called presentation surface. The traps here are the header bar, the
/// status bar, the affordance revealer, and the tab view — the last of which the
/// editor-count aggregate walks, so the *set* must answer `0` rather than
/// panicking on a cleared child.
#[test]
fn test_focus_mode_evidence_answers_honestly_after_the_window_is_disposed() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    window.new_tab();
    flush_events();
    set_focus_mode(&window, true);
    assert!(evidence(&window).active);
    assert_eq!(evidence(&window).editors_with_focus_mode, 1);

    // Closing is not disposing: GTK defers template-child teardown.
    window.close();
    flush_events();
    assert!(
        evidence(&window).active,
        "sanity: closing is not disposing, so the mode is still reported here"
    );

    // Leave the mode so the chrome is *visible* again: otherwise the
    // post-dispose `false` below is the same answer a live read would give and
    // asserts nothing.
    set_focus_mode(&window, false);
    flush_events();
    let live = evidence(&window);
    assert!(
        live.header_bar_visible && live.status_bar_visible,
        "precondition: with the mode off the chrome is visible, so the \
         post-dispose `false` is a real change rather than a coincidence"
    );

    // SAFETY: this test window is disposed exactly once, and everything after
    // this point only reads the evidence surface, which must answer honestly on
    // a disposed widget rather than panicking.
    unsafe { window.run_dispose() };

    let disposed = evidence(&window);
    // Focus Mode had already hidden the chrome, so `false` here would also be
    // the *live* answer — the assertion would pass on a surface that never
    // consulted the widgets at all. Exiting the mode first is what makes the
    // post-dispose `false` mean "the cleared child read as not visible".
    assert!(
        !disposed.header_bar_visible && !disposed.status_bar_visible,
        "a cleared chrome child reads as not visible rather than panicking"
    );
    assert!(!disposed.affordance_revealed);
    assert!(!disposed.affordance_contains_focus);
    assert_eq!(
        disposed.editors_with_focus_mode, 0,
        "the bounded aggregate must answer zero once the tab view is gone"
    );
    assert_eq!(
        evidence(&window),
        disposed,
        "repeated reads of a disposed window must stay identical"
    );
}

/// Proof 3 of 3 — **non-materialization**.
///
/// The requirement fires for a surface covering a lazily created collection;
/// `GtkTreeListModel` is the one in this tree. This surface's only collection
/// walk is over `AdwTabView`, which holds its pages eagerly and registers no
/// store, so no read can bring state into being. Proved rather than asserted, in
/// both the empty and populated extremes, and including the mode-carrying editor
/// count — the field most likely to be tempted into calling `apply_to_editors`.
#[test]
fn test_focus_mode_evidence_reads_materialize_no_toolkit_state() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    // Empty extreme: reading must not create a page.
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    let _ = evidence(&window);
    let _ = evidence(&window);
    assert_eq!(window.imp().tab_view.n_pages(), 0);

    window.new_tab();
    window.new_tab();
    flush_events();
    set_focus_mode(&window, true);

    let pages_before = window.imp().tab_view.n_pages();
    let selected_before = window
        .imp()
        .tab_view
        .selected_page()
        .map(|page| page.child());
    let applied_before = evidence(&window).editors_with_focus_mode;

    let first = evidence(&window);
    let second = evidence(&window);
    assert_eq!(first, second);

    assert_eq!(window.imp().tab_view.n_pages(), pages_before);
    assert_eq!(
        window
            .imp()
            .tab_view
            .selected_page()
            .map(|page| page.child()),
        selected_before
    );
    assert_eq!(
        evidence(&window).editors_with_focus_mode,
        applied_before,
        "reading must not apply the mode to anything"
    );
}

/// State-extreme sweep for the restoration contract.
///
/// The whole point of the row is handing back exactly what it borrowed, and that
/// is only visible across entry states. The fullscreen arm is the one no existing
/// test covers: a user who was *already* fullscreen must stay fullscreen on exit.
#[test]
fn test_focus_mode_restores_only_what_it_borrowed() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    window.new_tab();
    flush_events();

    // Extreme 1 — entered from a non-fullscreen window: exit gives fullscreen back.
    set_focus_mode(&window, true);
    let focused = evidence(&window);
    assert!(!focused.entry.was_fullscreen);
    assert!(
        focused.exit_restores.leave_fullscreen,
        "fullscreen Focus Mode took is Focus Mode's to drop"
    );
    set_focus_mode(&window, false);

    // Extreme 2 — entered from an already-fullscreen window: exit keeps it.
    window.fullscreen();
    flush_events();
    if window.is_fullscreen() {
        set_focus_mode(&window, true);
        let from_fullscreen = evidence(&window);
        assert!(from_fullscreen.entry.was_fullscreen);
        assert!(
            !from_fullscreen.exit_restores.leave_fullscreen,
            "a fullscreen the user already had is not Focus Mode's to drop"
        );
        set_focus_mode(&window, false);
        assert!(
            window.is_fullscreen(),
            "exit must not drop a fullscreen it did not take"
        );
        window.unfullscreen();
        flush_events();
    } else {
        // Headless compositors may decline the fullscreen request; do not assert
        // against the compositor, but do not silently pass the arm either.
        eprintln!("focus-mode: compositor declined fullscreen; extreme 2 skipped");
    }

    // Extreme 3 — no tabs at all. The mode must still toggle, and the bounded
    // aggregate must answer zero rather than requiring an editor to exist.
    let tab_view = window.imp().tab_view.clone();
    // Bounded: a close that is refused would otherwise spin this loop forever
    // and the lane would report a timeout rather than the real cause.
    for _ in 0..32 {
        if tab_view.n_pages() == 0 {
            break;
        }
        tab_view.close_page(&tab_view.nth_page(0));
        flush_events();
    }
    assert_eq!(tab_view.n_pages(), 0, "every tab closed within the bound");
    set_focus_mode(&window, true);
    let no_tabs = evidence(&window);
    assert!(no_tabs.active, "Focus Mode must be reachable with no tabs");
    assert_eq!(no_tabs.editors_with_focus_mode, 0);
    assert!(!no_tabs.header_bar_visible);
    set_focus_mode(&window, false);
    assert!(evidence(&window).header_bar_visible);
}

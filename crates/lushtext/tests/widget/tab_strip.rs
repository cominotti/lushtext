// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget coverage for `WFR-TAB-STRIP`'s evidence surface and state extremes.
//!
//! The three surface proofs `.agents/rules/widget-wiring.md` requires, driven
//! rather than asserted, plus the collection-surface state matrix. The
//! workflow's behavioural coverage (pin, reorder, bulk close, cancellation)
//! lives in `window.rs` alongside the rest of the shell's action coverage and is
//! unchanged by the migration.

use crate::common::{
    ensure_gtk_init, fixture, flush_events, present_window, test_window, wait_until,
};
use glib::object::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::window::{LushtextWindow, TabStripEvidence, tab_strip_evidence};
use std::path::PathBuf;
use std::time::Duration;

/// Read the workflow's one surface.
fn evidence(window: &LushtextWindow) -> TabStripEvidence {
    tab_strip_evidence(window)
}

fn seed_files(names: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = names
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            fixture::write_text(&path, "contents\n");
            path
        })
        .collect();
    (dir, paths)
}

fn open_all(window: &LushtextWindow, paths: &[PathBuf]) {
    for path in paths {
        window.open_document(path);
    }
    wait_until(Duration::from_secs(5), || {
        evidence(window).page_count == i32::try_from(paths.len()).expect("page count fits")
    });
}

fn aim_menu_at(window: &LushtextWindow, title: &str) -> libadwaita::TabPage {
    let tab_view = &window.imp().tab_view;
    let page = (0..tab_view.n_pages())
        .map(|index| tab_view.nth_page(index))
        .find(|page| {
            page.child()
                .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
                .is_ok_and(|editor| editor.title() == title)
        })
        .unwrap_or_else(|| panic!("tab '{title}' not found"));
    tab_view.emit_by_name::<()>("setup-menu", &[&page]);
    flush_events();
    page
}

/// Proof 1 of 3 — **reentrancy**.
///
/// Driven through each operation that takes a mutable borrow of the state the
/// accessor reads: setting the menu target (`target_page`), configuring pages
/// (`configured_pages`), and an authorized bulk close
/// (`preconfirmed_close_pages` plus the projection-refresh depth). The surface
/// is read *after* each one; reading *while* a borrow is held is the panic the
/// constraint prevents, not a demonstration of it.
#[test]
fn test_tab_strip_evidence_reads_stay_side_effect_free_across_menu_and_close_mutation() {
    ensure_gtk_init();
    let (_dir, files) = seed_files(&["a.txt", "b.txt", "c.txt"]);
    let window = test_window();
    present_window(&window);

    let empty = evidence(&window);
    assert_eq!(empty, evidence(&window), "an empty strip must read stably");

    open_all(&window, &files);
    let opened = evidence(&window);
    assert_eq!(opened.page_count, 3);
    assert_eq!(opened.configured_page_count, 3);
    assert_eq!(
        opened,
        evidence(&window),
        "two adjacent reads after page configuration must be identical"
    );

    // Drive the `target_page` mutable borrow.
    aim_menu_at(&window, "b.txt");
    let targeted = evidence(&window);
    assert_eq!(targeted.menu_target_index, Some(1));
    assert_eq!(targeted, evidence(&window));
    assert_ne!(
        targeted.menu_target_index, opened.menu_target_index,
        "the drive must actually have changed the state being re-read"
    );

    // Drive the pinned-layout mutable path.
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-tab-pinned", None);
    wait_until(Duration::from_secs(3), || {
        evidence(&window)
            .pinned_layout
            .iter()
            .any(|entry| entry.pinned)
    });
    let pinned = evidence(&window);
    assert_eq!(pinned, evidence(&window));

    // Drive the bulk-close path, which borrows `preconfirmed_close_pages`
    // mutably and moves the projection-refresh depth up and back down.
    aim_menu_at(&window, "b.txt");
    gtk4::prelude::ActionGroupExt::activate_action(&window, "close-other-tabs", None);
    wait_until(Duration::from_secs(5), || evidence(&window).page_count == 1);
    let closed = evidence(&window);
    assert_eq!(closed, evidence(&window));
    assert_eq!(
        closed.preconfirmed_close_count, 0,
        "every authorization token deposited by the batch must have been spent"
    );
    assert_eq!(
        closed.projection_refresh_defer_depth, 0,
        "the projection-refresh batch must have been released"
    );
}

/// Proof 2 of 3 — **disposal honesty**.
///
/// `tab_view` is a `TemplateChild`, which GTK4 clears in `dispose()` before
/// Rust's `Drop`. The panicking `Deref` accessor would turn a teardown
/// observation into a crash.
#[test]
fn test_tab_strip_evidence_answers_honestly_after_the_window_is_disposed() {
    ensure_gtk_init();
    let (_dir, files) = seed_files(&["a.txt", "b.txt"]);
    let window = test_window();
    present_window(&window);
    open_all(&window, &files);
    assert_eq!(evidence(&window).page_count, 2);

    // Closing is not disposing: GTK defers template-child teardown.
    window.close();
    flush_events();

    // SAFETY: this test window is disposed exactly once, and everything after
    // this point only reads the evidence surface.
    unsafe { window.run_dispose() };

    let disposed = evidence(&window);
    assert_eq!(
        disposed.page_count, 0,
        "a cleared tab view reports zero pages rather than panicking"
    );
    assert!(disposed.pinned_layout.is_empty());
    assert!(
        disposed.titles.is_empty(),
        "the bounded aggregate must answer empty once the tab view is gone"
    );
    assert_eq!(disposed.menu_target_index, None);
    assert!(!disposed.any_page_saving);
    assert_eq!(
        evidence(&window),
        disposed,
        "repeated reads of a disposed window must stay identical"
    );
}

/// Proof 3 of 3 — **non-materialization**.
///
/// `AdwTabView` holds its pages eagerly and registers no lazily created store,
/// so no read can bring state into being. Proved in both extremes rather than
/// asserted, including the two counters a read could plausibly be tempted to
/// touch: the authorization token set and the projection-refresh depth.
#[test]
fn test_tab_strip_evidence_reads_materialize_no_toolkit_state() {
    ensure_gtk_init();
    let (_dir, files) = seed_files(&["a.txt", "b.txt", "c.txt", "d.txt"]);
    let window = test_window();
    present_window(&window);

    let empty = evidence(&window);
    for _ in 0..5 {
        assert_eq!(evidence(&window), empty);
    }

    open_all(&window, &files);
    aim_menu_at(&window, "c.txt");
    let populated = evidence(&window);
    for _ in 0..5 {
        assert_eq!(evidence(&window), populated);
    }
    assert_eq!(
        evidence(&window).configured_page_count,
        populated.configured_page_count,
        "reading must not configure a page"
    );
    assert_eq!(
        evidence(&window).preconfirmed_close_count,
        0,
        "reading must not deposit a bulk-close authorization"
    );
    assert_eq!(
        evidence(&window).projection_refresh_defer_depth,
        0,
        "reading must not open a projection-refresh batch"
    );
    assert_eq!(
        evidence(&window).menu_target_index,
        populated.menu_target_index,
        "reading must not clear or move the menu target"
    );
}

/// State extremes for a collection surface: none, one, and many-or-awkward.
#[test]
fn test_tab_strip_state_extremes_keep_the_menu_honest() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    // No context: every menu-backed action is disabled and nothing is targeted.
    let empty = evidence(&window);
    assert_eq!(empty.page_count, 0);
    assert_eq!(empty.menu_target_index, None);
    assert_eq!(
        empty.menu_action_enabled, [false; 5],
        "with no tabs, no tab-menu action may be reachable"
    );

    // One tab: the target exists, but nothing can be closed around it and it
    // cannot move in either direction.
    let (_dir, files) = seed_files(&["only.txt"]);
    open_all(&window, &files);
    aim_menu_at(&window, "only.txt");
    let single = evidence(&window);
    assert_eq!(single.menu_target_index, Some(0));
    assert!(single.menu_action_enabled[0], "pin stays available");
    assert!(
        !single.menu_action_enabled[1] && !single.menu_action_enabled[2],
        "a lone tab has nothing to close to its right and no others to close"
    );
    assert!(
        !single.menu_action_enabled[3] && !single.menu_action_enabled[4],
        "a lone tab cannot move in either direction"
    );

    // Many, with a pinned leading segment and a long awkward title.
    let (_dir2, many) = seed_files(&[
        "b.txt",
        "c.txt",
        "a-very-long-document-name-that-would-not-fit-a-narrow-strip.txt",
        "e.txt",
    ]);
    open_all(&window, &[files, many].concat());
    aim_menu_at(&window, "b.txt");
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-tab-pinned", None);
    wait_until(Duration::from_secs(3), || {
        evidence(&window)
            .pinned_layout
            .first()
            .is_some_and(|e| e.pinned)
    });

    let dense = evidence(&window);
    assert_eq!(dense.page_count, 5);
    assert_eq!(
        dense.pinned_layout.iter().filter(|e| e.pinned).count(),
        1,
        "exactly one tab is pinned, and it leads the strip"
    );
    assert!(
        dense.titles.iter().any(|title| title.len() > 40),
        "the awkward long title is present and is reported in full"
    );

    // The pinned tab itself cannot move out of a one-page pinned segment.
    aim_menu_at(&window, "b.txt");
    let pinned_target = evidence(&window);
    assert_eq!(pinned_target.menu_target_index, Some(0));
    assert!(
        !pinned_target.menu_action_enabled[3] && !pinned_target.menu_action_enabled[4],
        "a lone pinned tab has nowhere to go inside its own segment"
    );

    // The first unpinned tab cannot move left into the pinned segment.
    let first_unpinned_title = dense.titles[1].clone();
    aim_menu_at(&window, &first_unpinned_title);
    let unpinned_target = evidence(&window);
    assert!(
        !unpinned_target.menu_action_enabled[3],
        "the first unpinned tab must not move into the pinned segment"
    );
    assert!(unpinned_target.menu_action_enabled[4]);
    assert!(
        unpinned_target.menu_action_enabled[1] && unpinned_target.menu_action_enabled[2],
        "with four tabs to its right and others beside it, both bulk closes are live"
    );
}

/// Data-safety (task 7.2): a bulk close spends every authorization it deposits.
///
/// The authorization token is keyed by the page's **heap address**
/// (`page.as_ptr() as usize`). A token that outlived its page would, once the
/// allocator recycled that address for a new `AdwTabPage`, silently authorize
/// closing a *different, modified* tab without its save-changes dialog.
///
/// **The candidate was audited and cleared, and this test records why rather
/// than pretending to fix it.** A sweep that retired unspent tokens at the end
/// of the batch was written and then removed, because with it removed this test
/// still passes: eligible targets are collected from the **live page order**
/// (`surfaces::collect_tab_pages`), so a detached page can never enter a batch,
/// and every target's `close_page` reaches the request handler, which consumes
/// the token as its first act. The invariant holds structurally.
///
/// What this test guards is the protocol, not a fixed defect: if a later change
/// deposits a token for a page it does not close, or stops consuming tokens in
/// the request handler, the count stops being zero and this fails.
#[test]
fn test_bulk_close_spends_every_authorization_it_deposits() {
    ensure_gtk_init();
    let (_dir, files) = seed_files(&["keep.txt", "gone.txt", "other.txt"]);
    let window = test_window();
    present_window(&window);
    open_all(&window, &files);

    // Detach one of the future targets before the batch runs, so its close
    // request cannot consume the token the batch is about to deposit for it.
    let doomed = aim_menu_at(&window, "gone.txt");
    window.imp().tab_view.close_page(&doomed);
    wait_until(Duration::from_secs(5), || evidence(&window).page_count == 2);
    assert_eq!(
        evidence(&window).preconfirmed_close_count,
        0,
        "precondition: no authorization is outstanding before the batch"
    );

    aim_menu_at(&window, "keep.txt");
    gtk4::prelude::ActionGroupExt::activate_action(&window, "close-other-tabs", None);
    wait_until(Duration::from_secs(5), || evidence(&window).page_count == 1);

    assert_eq!(
        evidence(&window).titles,
        vec!["keep.txt"],
        "the already-detached page was never eligible, and the live one closed"
    );
    assert_eq!(
        evidence(&window).preconfirmed_close_count,
        0,
        "no bulk-close authorization may survive the batch that created it"
    );
    assert_eq!(
        evidence(&window).projection_refresh_defer_depth,
        0,
        "the projection-refresh batch must have been released"
    );
}

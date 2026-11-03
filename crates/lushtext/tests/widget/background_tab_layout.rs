// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for `defer-background-tab-layout`: a hidden tab's editor view
//! lays nothing out (ledger A82, A83), while its document stays loaded,
//! savable, and positioned.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::common::{
    ensure_gtk_init, fixture, flush_after_delay, flush_events, present_window, test_window,
    wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use lushtext_core::ui::window::LushtextWindow;

/// Lines in a probe document: about 150 validation batches of 2000 px
/// (ledger A31) at the default font with wrapping off.
const LINES: usize = 1 << 14;
/// Iterations within which a `G_PRIORITY_DEFAULT_IDLE` source counts as not
/// held back by validation: the selected small document's own few batches
/// fit in it, a hidden 16,384-line document's 150 do not (ledger A80).
const NOT_HELD_ITERATIONS: usize = 1 << 5;

fn dense_document(path: &Path, fill: char) {
    let line = format!("{}\n", fill.to_string().repeat(40));
    fixture::write_text(path, &line.repeat(LINES));
}

fn editor_at(window: &LushtextWindow, index: i32) -> LushtextEditorPage {
    window
        .imp()
        .tab_view
        .nth_page(index)
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("editor page")
}

fn wait_loaded(editor: &LushtextEditorPage) {
    // A worker load plus a sliced install of a 656 KiB document.
    wait_until(Duration::from_secs(30), || {
        let evidence = editor.load_evidence();
        evidence.load_state == EditorLoadState::Loaded && !evidence.installation_active
    });
    assert_eq!(editor.load_state(), EditorLoadState::Loaded);
}

/// Iterations of the default context until a `G_PRIORITY_DEFAULT_IDLE`
/// source armed now dispatches.
fn iterations_until_default_idle() -> usize {
    let dispatched = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = std::rc::Rc::clone(&dispatched);
    glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
        flag.set(true);
        glib::ControlFlow::Break
    });
    let context = glib::MainContext::default();
    let started = Instant::now();
    let mut iterations = 0;
    while !dispatched.get() && started.elapsed() < Duration::from_secs(60) {
        if context.iteration(false) {
            iterations += 1;
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    iterations
}

#[test]
fn test_background_tabs_lay_out_nothing_while_hidden() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    let small = dir.path().join("small.txt");
    dense_document(&first, 'a');
    dense_document(&second, 'b');
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);

    // Every open selects its page; only the last one stays shown.
    window.open_document(&first);
    window.open_document(&second);
    window.open_document(&small);
    let (first, second, small) = (
        editor_at(&window, 0),
        editor_at(&window, 1),
        editor_at(&window, 2),
    );
    for editor in [&first, &second, &small] {
        wait_loaded(editor);
    }
    flush_events();

    // A view-wide restyle (the kind a font, zoom or spacing change makes)
    // invalidates every line of every view; a hidden view that still lays
    // out its document validates all of it again (A80, A82).
    for editor in [&first, &second] {
        editor.source_view().set_pixels_above_lines(4);
    }
    let held = iterations_until_default_idle();
    assert!(
        held <= NOT_HELD_ITERATIONS,
        "no hidden view holds a lower-priority idle back behind its validation (A80): {held} iterations"
    );
    assert!(
        small.view_shows_document(),
        "the shown tab lays out its document"
    );
    for editor in [&first, &second] {
        assert!(
            !editor.view_shows_document(),
            "a hidden tab's view shows the placeholder"
        );
        // The document itself is whole: workflows read the document buffer.
        assert_eq!(
            editor.buffer().char_count(),
            i32::try_from(LINES * 41).expect("fits")
        );
        assert_eq!(editor.source_view().buffer().char_count(), 0);
        assert!(!editor.is_modified());
    }

    // Shown: the view gets its document back.
    window
        .imp()
        .tab_view
        .set_selected_page(&window.imp().tab_view.nth_page(0));
    flush_after_delay(Duration::from_millis(50));
    assert!(first.view_shows_document());
    assert_eq!(
        first.source_view().buffer(),
        *first.buffer().upcast_ref::<gtk4::TextBuffer>()
    );
}

#[test]
fn test_a_hidden_tab_seen_laying_out_is_detached_and_keeps_its_top_line() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let big = dir.path().join("big.txt");
    let small = dir.path().join("small.txt");
    dense_document(&big, 'c');
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);

    window.open_document(&big);
    let big = editor_at(&window, 0);
    wait_loaded(&big);
    assert!(big.view_shows_document());
    let target = big.buffer().iter_at_line(1 << 10).expect("line exists");
    big.scroll_to_document_iter(&target, 0.0, true, 0.0, 0.0);
    flush_after_delay(Duration::from_millis(300));
    let top = big.visible_top_line();
    assert!(
        top >= 1 << 9,
        "the shown view scrolled down, top line {top}"
    );

    window.open_document(&small);
    let small = editor_at(&window, 1);
    wait_loaded(&small);
    // Re-layout every line of the hidden view: more space above each line
    // changes every line's height, which the view validates from its idle
    // while hidden, growing its adjustment's upper batch by batch (A83).
    big.source_view().set_pixels_above_lines(4);
    wait_until(Duration::from_secs(10), || !big.view_shows_document());
    assert!(
        !big.view_shows_document(),
        "a hidden view seen laying out is detached"
    );
    let (detaches, _) = big.view_attachment_counts();
    assert!(
        detaches >= 2,
        "detached at construction and while hidden, {detaches}"
    );
    let kept = big.visible_top_line();
    assert!(
        kept.abs_diff(top) <= 1,
        "the detached tab keeps its top line: {kept} vs {top}"
    );

    window
        .imp()
        .tab_view
        .set_selected_page(&window.imp().tab_view.nth_page(0));
    flush_after_delay(Duration::from_millis(50));
    assert!(big.view_shows_document());
    // The reattached view lays the document out again from nothing and is
    // held at the kept line meanwhile (A83); the last scroll lands exactly
    // once it is laid out.
    wait_until(Duration::from_secs(10), || {
        big.visible_top_line().abs_diff(top) <= 1
    });
    let shown = big.visible_top_line();
    assert!(
        shown.abs_diff(top) <= 1,
        "the reattached view shows the kept top line: {shown} vs {top}"
    );
}

/// A Protected View document takes GTK's primary-selection provider off its
/// buffer while its view is realized; detaching that view (hidden, seen laying
/// out) and attaching it again must keep that bookkeeping true, or GTK's
/// `set_buffer` removes a provider the buffer no longer has (a `Gtk-CRITICAL`
/// the widget runner fails on).
#[test]
fn test_a_protected_tab_detaches_and_reattaches_without_primary_selection_criticals() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let protected = dir.path().join("protected.txt");
    let small = dir.path().join("small.txt");
    let giant = "giant ".repeat(
        usize::try_from(lushtext_core::model::document_scale::L_PROTECT_EDIT / 6 + 2)
            .expect("a small length"),
    );
    fixture::write_text(&protected, &format!("head\n{giant}\ntail\n"));
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);

    window.open_document(&protected);
    let protected = editor_at(&window, 0);
    wait_loaded(&protected);
    flush_after_delay(Duration::from_millis(100));
    assert!(protected.is_protected_view());
    assert!(protected.view_shows_document());

    window.open_document(&small);
    let small = editor_at(&window, 1);
    wait_loaded(&small);
    protected.source_view().set_pixels_above_lines(4);
    wait_until(Duration::from_secs(10), || !protected.view_shows_document());

    window
        .imp()
        .tab_view
        .set_selected_page(&window.imp().tab_view.nth_page(0));
    flush_after_delay(Duration::from_millis(100));
    assert!(protected.view_shows_document());
    assert!(protected.is_protected_view());
}

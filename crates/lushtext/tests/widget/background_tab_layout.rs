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
use sourceview5::prelude::*;

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

fn giant_paragraph() -> String {
    "giant ".repeat(
        usize::try_from(lushtext_core::model::document_scale::L_PROTECT_EDIT / 6 + 2)
            .expect("a small length"),
    )
}

/// The content type GTK's primary-selection provider offers for the whole
/// document selected, or "none" (the selection is cleared again after).
fn primary_type_with_all_selected(editor: &LushtextEditorPage) -> String {
    let buffer = editor.buffer();
    let (start, end) = (buffer.start_iter(), buffer.end_iter());
    buffer.select_range(&start, &end);
    flush_after_delay(Duration::from_millis(50));
    let name = primary_type(editor);
    let start = buffer.start_iter();
    buffer.select_range(&start, &start);
    flush_after_delay(Duration::from_millis(50));
    name
}

fn primary_type(editor: &LushtextEditorPage) -> String {
    editor
        .source_view()
        .primary_clipboard()
        .content()
        .map_or_else(|| "none".to_owned(), |c| c.type_().name().to_owned())
}

/// Open `small` (selected), then re-lay out `editor`'s hidden view until it
/// is detached.
fn hide_until_detached(window: &LushtextWindow, editor: &LushtextEditorPage, small: &Path) {
    window.open_document(small);
    wait_loaded(&editor_at(window, 1));
    editor.source_view().set_pixels_above_lines(4);
    wait_until(Duration::from_secs(10), || !editor.view_shows_document());
    assert!(!editor.view_shows_document(), "detached while hidden");
    assert!(editor.source_view().is_realized());
}

fn show_first_tab(window: &LushtextWindow) {
    window
        .imp()
        .tab_view
        .set_selected_page(&window.imp().tab_view.nth_page(0));
    flush_after_delay(Duration::from_millis(200));
}

/// Review M1: Protected View entered while the realized view showed its
/// placeholder removed a primary-selection registration the document did not
/// have (`Gtk-CRITICAL gtk_text_buffer_remove_selection_clipboard`), and once
/// attached GTK's provider stayed on PRIMARY, offering visible-only folded
/// text.
#[test]
fn test_protected_view_entered_while_detached_keeps_the_primary_provider_off() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("doc.txt");
    let small = dir.path().join("small.txt");
    dense_document(&path, 'd');
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let editor = editor_at(&window, 0);
    wait_loaded(&editor);
    flush_after_delay(Duration::from_millis(100));
    hide_until_detached(&window, &editor, &small);

    fixture::write_text(&path, &format!("head\n{}\ntail\n", giant_paragraph()));
    editor.load_file_async(&path);
    wait_loaded(&editor);
    flush_after_delay(Duration::from_millis(200));
    assert!(editor.is_protected_view());
    assert!(!editor.view_shows_document());

    show_first_tab(&window);
    assert!(editor.view_shows_document());
    assert_eq!(
        primary_type_with_all_selected(&editor),
        "none",
        "a protected document offers nothing on PRIMARY"
    );
}

/// Detaching a realized hidden view moves GTK's primary-selection provider
/// off the document, which cleared the user's PRIMARY selection when the
/// hidden document owned it; the detach holds a reference so it stays.
#[test]
fn test_detaching_a_hidden_view_keeps_the_users_primary_selection() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("doc.txt");
    let small = dir.path().join("small.txt");
    dense_document(&path, 'p');
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let editor = editor_at(&window, 0);
    wait_loaded(&editor);
    let buffer = editor.buffer();
    let start = buffer.start_iter();
    let end = buffer.iter_at_offset(40);
    buffer.select_range(&start, &end);
    flush_after_delay(Duration::from_millis(50));
    let owned = primary_type(&editor);
    assert_ne!(owned, "none", "the selection owns PRIMARY");

    hide_until_detached(&window, &editor, &small);
    assert_eq!(primary_type(&editor), owned, "still offered while detached");
    show_first_tab(&window);
    assert!(editor.view_shows_document());
    assert_eq!(primary_type(&editor), owned, "and after the attach");
    // GTK's own registration is back exactly once: clearing the selection
    // withdraws the provider.
    let start = buffer.start_iter();
    buffer.select_range(&start, &start);
    flush_after_delay(Duration::from_millis(50));
    assert_eq!(primary_type(&editor), "none");
}

/// Review M2: with an assistive technology listening, a swap sends the
/// whole document as one text change (about 2 ms per MiB in one dispatch),
/// so no view is detached, and a detached view with a small document attaches
/// as soon as a listener appears.
#[test]
fn test_no_view_is_detached_while_an_assistive_technology_listens() {
    use lushtext_core::ui::at_listeners;
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("doc.txt");
    let small = dir.path().join("small.txt");
    dense_document(&path, 'q');
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let editor = editor_at(&window, 0);
    wait_loaded(&editor);
    hide_until_detached(&window, &editor, &small);
    let (detaches, attaches) = editor.view_attachment_counts();

    at_listeners::simulate_listener(true);
    assert!(at_listeners::listeners_active());
    assert!(
        !editor.view_shows_document(),
        "queued: attached from a later dispatch, one editor per dispatch"
    );
    wait_until(Duration::from_secs(5), || editor.view_shows_document());
    assert!(
        editor.view_shows_document(),
        "a small detached document attaches when a listener appears"
    );
    assert_eq!(editor.view_attachment_counts(), (detaches, attaches + 1));
    editor.source_view().set_pixels_above_lines(8);
    flush_after_delay(Duration::from_millis(500));
    assert!(
        editor.view_shows_document(),
        "a hidden view laying out stays attached while a listener listens"
    );
    assert_eq!(editor.view_attachment_counts().0, detaches);

    at_listeners::simulate_listener(false);
    assert!(!at_listeners::listeners_active());
    editor.source_view().set_pixels_above_lines(2);
    wait_until(Duration::from_secs(10), || !editor.view_shows_document());
    assert!(
        !editor.view_shows_document(),
        "detached again once it stopped"
    );
}

/// Review M2: without a listener the swap walks every line of the document
/// (27–28 ms for 2.56 million lines), so a document over the line cap
/// (`VIEW_DETACH_MAX_LINES`, 2^18) is never detached.
#[test]
fn test_a_document_over_the_line_cap_is_never_detached() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("many-lines.txt");
    let small = dir.path().join("small.txt");
    fixture::write_text(&path, &"x\n".repeat((1 << 18) + 1));
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let editor = editor_at(&window, 0);
    wait_loaded(&editor);
    assert!(editor.view_shows_document());
    let (detaches, _) = editor.view_attachment_counts();
    window.open_document(&small);
    wait_loaded(&editor_at(&window, 1));
    editor.source_view().set_pixels_above_lines(4);
    flush_after_delay(Duration::from_millis(500));
    assert!(editor.view_shows_document(), "too many lines to detach");
    assert_eq!(editor.view_attachment_counts().0, detaches);
}

/// The kept position inside a long wrapped paragraph is the display line at
/// the top, not the paragraph start (review: the reattached view jumped back
/// to the paragraph's first line).
#[test]
fn test_a_detached_view_comes_back_inside_a_long_wrapped_paragraph() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("wrapped.txt");
    let small = dir.path().join("small.txt");
    // Paragraphs under the wrap gate (`L_wrap_edit`, 8,192), each many
    // display lines tall.
    let paragraph = format!("{}\n", "wrapped words ".repeat(560));
    fixture::write_text(&path, &paragraph.repeat(96));
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let editor = editor_at(&window, 0);
    wait_loaded(&editor);
    let view = editor.source_view();
    assert_ne!(view.wrap_mode(), gtk4::WrapMode::None);
    // The start of the display line at the top of the view.
    let top_offset = |view: &sourceview5::View| {
        let top = view.visible_rect().y();
        let (mut shown, _) = view.line_at_y(top);
        let paragraph = shown.line();
        loop {
            let mut next = shown;
            if !view.forward_display_line(&mut next)
                || next.line() != paragraph
                || view.iter_location(&next).y() > top
            {
                break shown.offset();
            }
            shown = next;
        }
    };
    // Within one display line (at most a few hundred characters at the
    // harness's monitor width), far less than the half paragraph a
    // paragraph-start position would be off by.
    const DISPLAY_LINE_CHARS: i32 = 1000;
    let paragraph_chars = i32::try_from(paragraph.chars().count()).expect("fits");
    let target = editor
        .buffer()
        .iter_at_offset(48 * paragraph_chars + paragraph_chars / 2);
    // A scroll queued before the lines above it have heights lands elsewhere
    // (A83), so it is repeated until the view is there.
    wait_until(Duration::from_secs(20), || {
        editor.scroll_to_document_iter(&target, 0.0, true, 0.0, 0.0);
        (top_offset(view) - target.offset()).abs() < DISPLAY_LINE_CHARS
    });
    let kept = top_offset(view);
    assert!(
        kept - 48 * paragraph_chars > paragraph_chars / 4,
        "scrolled inside the paragraph: {kept}"
    );

    hide_until_detached(&window, &editor, &small);
    show_first_tab(&window);
    wait_until(Duration::from_secs(10), || {
        (top_offset(view) - kept).abs() < DISPLAY_LINE_CHARS
    });
    let shown = top_offset(view);
    assert!(
        (shown - kept).abs() < DISPLAY_LINE_CHARS,
        "the reattached view shows the kept display line: {shown} vs {kept}"
    );
}

/// A background slice waiting behind ever-ready `G_PRIORITY_DEFAULT_IDLE`
/// work runs at that priority after `BACKGROUND_SLICE_AGE`, and removing the
/// stored source still cancels it.
#[test]
fn test_a_background_slice_ages_into_the_paced_priority() {
    use lushtext_core::ui::input_quiet::paced_idle_aging;
    ensure_gtk_init();
    let busy = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, || {
        std::thread::sleep(Duration::from_millis(1));
        glib::ControlFlow::Continue
    });
    let low = glib::Priority::from(glib::ffi::G_PRIORITY_DEFAULT_IDLE + 1);
    let age = Duration::from_millis(1 << 8);
    let ran = std::rc::Rc::new(std::cell::Cell::new(None::<Instant>));
    let flag = std::rc::Rc::clone(&ran);
    let queued = Instant::now();
    paced_idle_aging(
        low,
        glib::Priority::DEFAULT_IDLE,
        age,
        move || flag.set(Some(Instant::now())),
        |_| {},
    );
    let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
    let cancelled_flag = std::rc::Rc::clone(&cancelled);
    let stored = std::rc::Rc::new(std::cell::RefCell::new(None::<glib::SourceId>));
    let store = std::rc::Rc::clone(&stored);
    paced_idle_aging(
        low,
        glib::Priority::DEFAULT_IDLE,
        age,
        move || cancelled_flag.set(true),
        move |id| {
            store.replace(Some(id));
        },
    );
    stored
        .borrow_mut()
        .take()
        .expect("a stored source")
        .remove();

    let context = glib::MainContext::default();
    while ran.get().is_none() && queued.elapsed() < Duration::from_secs(5) {
        context.iteration(false);
    }
    let waited = ran.get().expect("the aged slice ran") - queued;
    assert!(waited >= age, "it waited its age first: {waited:?}");
    let until = Instant::now() + age;
    while Instant::now() < until {
        context.iteration(false);
    }
    assert!(!cancelled.get(), "a removed slice never runs");
    busy.remove();
}

/// Review verdict (`eviction/switch-hidden-dense`): a reattached view lays a
/// dense document out again from nothing in 2000 px batches of up to 28 ms
/// (A83), so the attach hides what it cannot show and reveals it outward from
/// the kept line in `V_SLICE` ranges, as a restyle does; nothing stays hidden
/// and no text changes.
#[test]
fn test_showing_a_never_shown_dense_tab_reveals_it_in_paced_ranges() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let dense = dir.path().join("dense.txt");
    let small = dir.path().join("small.txt");
    let body = format!("{}\n", "d".repeat(710)).repeat(400);
    fixture::write_text(&dense, &body);
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&dense);
    window.open_document(&small);
    let (dense, small) = (editor_at(&window, 0), editor_at(&window, 1));
    wait_loaded(&dense);
    wait_loaded(&small);
    flush_events();
    assert!(!dense.view_shows_document(), "never shown: placeholder");
    let before = dense.load_evidence().restyle.restyles;

    show_first_tab(&window);
    assert!(dense.view_shows_document());
    assert_eq!(
        dense.load_evidence().restyle.restyles,
        before + 1,
        "the reattach is paced like a restyle"
    );
    wait_until(Duration::from_secs(20), || {
        !dense.load_evidence().restyle.pending
    });
    let evidence = dense.load_evidence().restyle;
    assert!(!evidence.pending, "everything is revealed");
    assert!(evidence.reveals > 1, "in several ranges: {evidence:?}");
    let buffer = dense.buffer();
    assert_eq!(
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .as_str(),
        body,
        "the visible text is the whole document again"
    );
    assert!(!dense.is_modified());
}

/// Review round 2, SF1: when a listener appears, detached editors attach one
/// per dispatch, the selected tab's first, and the queue stops when the
/// listener leaves.
#[test]
fn test_detached_editors_attach_one_per_dispatch_when_a_listener_appears() {
    use lushtext_core::ui::at_listeners;
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let paths: Vec<_> = (0..3)
        .map(|index| {
            let path = dir.path().join(format!("doc{index}.txt"));
            fixture::write_text(&path, &format!("document {index}\n"));
            path
        })
        .collect();
    let small = dir.path().join("small.txt");
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    // Each open shows its tab; the first three are detached by re-laying
    // them out while hidden.
    for path in &paths {
        window.open_document(path);
    }
    window.open_document(&small);
    let editors: Vec<_> = (0..3).map(|index| editor_at(&window, index)).collect();
    for editor in &editors {
        wait_loaded(editor);
        editor.source_view().set_pixels_above_lines(4);
    }
    wait_until(Duration::from_secs(10), || {
        editors.iter().all(|editor| !editor.view_shows_document())
    });
    assert!(editors.iter().all(|editor| !editor.view_shows_document()));

    at_listeners::simulate_listener(true);
    let attached = || {
        editors
            .iter()
            .filter(|editor| editor.view_shows_document())
            .count()
    };
    assert_eq!(attached(), 0, "nothing attaches inside the notification");
    let context = glib::MainContext::default();
    let mut seen = Vec::new();
    let started = Instant::now();
    while attached() < 3 && started.elapsed() < Duration::from_secs(5) {
        if context.iteration(false) {
            let now = attached();
            assert!(
                now <= seen.last().copied().unwrap_or(0) + 1,
                "at most one editor attaches per dispatch: {seen:?} then {now}"
            );
            seen.push(now);
        }
    }
    assert_eq!(attached(), 3, "every small detached editor attaches");

    // Leaving cancels a queue still pending.
    at_listeners::simulate_listener(false);
    for editor in &editors {
        editor.source_view().set_pixels_above_lines(8);
    }
    wait_until(Duration::from_secs(10), || attached() == 0);
    at_listeners::simulate_listener(true);
    at_listeners::simulate_listener(false);
    flush_after_delay(Duration::from_millis(200));
    assert_eq!(attached(), 0, "a cancelled queue attaches nothing");
}

/// Review round 2, SF4: a dense document's reattach hides what it has not
/// revealed yet, and GtkSourceView's search skips invisible text (A33), so a
/// Replace All right after the switch replaced only the revealed matches.
#[test]
fn test_replace_all_right_after_a_paced_reattach_replaces_every_match() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let dense = dir.path().join("dense.txt");
    let small = dir.path().join("small.txt");
    let line = format!("needle {}\n", "d".repeat(700));
    fixture::write_text(&dense, &line.repeat(400));
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&dense);
    window.open_document(&small);
    let (dense, small) = (editor_at(&window, 0), editor_at(&window, 1));
    wait_loaded(&dense);
    wait_loaded(&small);
    flush_events();
    assert!(!dense.view_shows_document());

    // Shown, then Replace All in the same dispatch: the reveal has not run.
    window
        .imp()
        .tab_view
        .set_selected_page(&window.imp().tab_view.nth_page(0));
    assert!(dense.view_shows_document());
    assert!(
        dense.load_evidence().restyle.pending,
        "the reattach is paced"
    );
    dense.show_replace();
    let bar = dense.search_bar();
    // The entry hands its text to the search settings only after its
    // search-changed delay; set both now, without running the main loop
    // (which would let the reveal finish first).
    bar.search_entry().set_text("needle");
    bar.search_context()
        .expect("a search context")
        .settings()
        .set_search_text(Some("needle"));
    bar.replace_entry().set_text("pin");
    bar.replace_all();
    let buffer = dense.buffer();
    let text = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string();
    assert_eq!(text.matches("needle").count(), 0, "every match replaced");
    assert_eq!(text.matches("pin ").count(), 400);
    assert!(
        !dense.load_evidence().restyle.pending,
        "nothing left hidden"
    );
}

/// Review round 2, N1: a dense document of more than 2^14 lines reattaches
/// unpaced (hidden lines still cost validation, and they add no height).
#[test]
fn test_a_dense_document_over_the_paced_reattach_line_limit_reattaches_unpaced() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let dense = dir.path().join("dense-many.txt");
    let small = dir.path().join("small.txt");
    // 60 characters a line is dense (224 lines exceed `V_SLICE`).
    fixture::write_text(
        &dense,
        &format!("{}\n", "m".repeat(60)).repeat((1 << 14) + 1),
    );
    fixture::write_text(&small, "small\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&dense);
    window.open_document(&small);
    let (dense, small) = (editor_at(&window, 0), editor_at(&window, 1));
    wait_loaded(&dense);
    wait_loaded(&small);
    flush_events();
    assert!(!dense.view_shows_document());
    let before = dense.load_evidence().restyle.restyles;
    show_first_tab(&window);
    assert!(dense.view_shows_document());
    assert_eq!(
        dense.load_evidence().restyle.restyles,
        before,
        "over the line limit: no paced reveal"
    );
}

/// Replace All with nothing to replace: sourceview5 0.11's `replace_all`
/// binding panicked on GtkSourceView's zero count (a pre-existing crash the
/// review round 2 tests found).
#[test]
fn test_replace_all_with_no_match_is_a_no_op() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plain.txt");
    fixture::write_text(&path, "nothing to find here\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let editor = editor_at(&window, 0);
    wait_loaded(&editor);
    editor.show_replace();
    let bar = editor.search_bar();
    bar.search_context()
        .expect("a search context")
        .settings()
        .set_search_text(Some("absent"));
    bar.replace_entry().set_text("x");
    bar.replace_all();
    flush_events();
    assert!(!editor.is_modified());
    let buffer = editor.buffer();
    assert_eq!(
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .as_str(),
        "nothing to find here\n"
    );
}

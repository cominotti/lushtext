// SPDX-License-Identifier: GPL-3.0-or-later

//! The post-open settle of an opened document (`bound-post-open-settle`):
//! the deferred language attach stage (design D2, review R1, R2, R6, R8), the
//! paced install of dense documents (D6.1, R13, R14), EditorConfig applied
//! before the first insert (D6.2, R17), and the settle readiness the
//! `document-settled` predicate reads (D1).
//!
//! Assertions read the load workflow's evidence surface (`LoadEvidence`) and
//! the buffer's language, through end-state waits.

use crate::common::{
    active_editor, ensure_gtk_init, flush_after_delay, present_window, test_window, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::buffer_replacement::V_SLICE;
use lushtext_core::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use lushtext_core::ui::editor_page::load::policy::{ATTACH_DEADLINE_US, SettleStage};
use lushtext_core::ui::editor_page::{EditorLoadState, InlineAlertLifetime, LushtextEditorPage};
use lushtext_core::ui::window::LushtextWindow;
use sourceview5::prelude::{BufferExt as _, ViewExt as _};
use std::time::Duration;

/// Async budget: a load plus the first frame plus the low-priority attach.
const SETTLE_BUDGET: Duration = Duration::from_secs(10);

fn write(dir: &tempfile::TempDir, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    lushtext_core::services::filesystem::fixture::write_text(&path, text);
    path
}

fn language_id(editor: &LushtextEditorPage) -> Option<String> {
    editor
        .buffer()
        .language()
        .map(|language| language.id().to_string())
}

fn opened(window: &LushtextWindow, path: &std::path::Path) -> LushtextEditorPage {
    window.open_document(path);
    let editor = active_editor(window);
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
    });
    editor
}

const HTML: &str = "<!doctype html>\n<html><body><p class=\"a\">hello</p></body></html>\n";

#[test]
fn test_the_activation_dispatch_attaches_no_language() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "page.html", HTML);
    window.open_document(&path);
    let editor = active_editor(&window);
    // `open_document` returned: the activation turn is over, and no language
    // was attached inside it.
    assert_eq!(
        language_id(&editor),
        None,
        "the activation turn attached a language"
    );
    wait_until(SETTLE_BUDGET, || {
        language_id(&editor).as_deref() == Some("html")
    });
    let attach = editor.load_evidence().language_attach;
    assert!(
        attach.first_frame_seen,
        "the attach ran before the editor's first frame"
    );
    assert_eq!(
        attach.stages_run, 1,
        "the language came from the attach stage"
    );
    assert!(!attach.pending);
}

#[test]
fn test_neither_publish_nor_the_replacement_terminal_attaches() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "page.html", HTML);
    let editor = opened(&window, &path);
    wait_until(SETTLE_BUDGET, || {
        language_id(&editor).as_deref() == Some("html")
    });
    // Every language the editor holds came through one stage (review R1).
    assert_eq!(editor.load_evidence().language_attach.stages_run, 1);
    // A reload of the same document requests the same language: no stage.
    editor.load_file_async(&path);
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
    });
    flush_after_delay(Duration::from_millis(200));
    assert_eq!(language_id(&editor).as_deref(), Some("html"));
    assert_eq!(editor.load_evidence().language_attach.stages_run, 1);
}

#[test]
fn test_a_markdown_document_is_markdown_while_its_attach_is_pending() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "notes.md", "# Heading\n\nBody text.\n");
    window.open_document(&path);
    let editor = active_editor(&window);
    let mut saw_pending = false;
    wait_until(SETTLE_BUDGET, || {
        if editor.load_evidence().language_attach.pending {
            saw_pending = true;
            assert_eq!(
                editor.document_language_id().as_deref(),
                Some("markdown"),
                "a pending attach must report the requested language (review R2)"
            );
        }
        language_id(&editor).as_deref() == Some("markdown")
    });
    assert!(saw_pending, "the attach stage was never observed pending");
}

#[test]
fn test_a_tab_that_is_not_selected_attaches_only_once_shown() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let first = write(&dir, "first.html", HTML);
    let second = write(&dir, "second.txt", "plain\n");
    let background = opened(&window, &first);
    let _foreground = opened(&window, &second);
    // Reload the background tab while it is not mapped: its attach waits.
    background.set_file_path(&dir.path().join("renamed.rs"));
    flush_after_delay(Duration::from_millis(500));
    assert!(!background.source_view().is_mapped());
    assert!(background.load_evidence().language_attach.pending);
    assert_eq!(background.document_language_id().as_deref(), Some("rust"));
    let tab_view = &window.imp().tab_view;
    tab_view.set_selected_page(&tab_view.page(&background));
    wait_until(SETTLE_BUDGET, || {
        language_id(&background).as_deref() == Some("rust")
    });
}

#[test]
fn test_the_attach_waits_for_input_quiet_until_its_deadline() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "page.html", HTML);
    window.open_document(&path);
    let editor = active_editor(&window);
    // Keys keep arriving: the attach must not run before the deadline. The
    // input comes from a high-priority source, which GLib dispatches before
    // the attach's low-priority timeout whenever both are ready: input fed
    // from the test loop between flushes left a gap whenever one dispatch of
    // the loaded machine took longer than the quiet interval, and the attach
    // then ran (FLAKY under load, 2026-11-02).
    let feeding = std::rc::Rc::new(std::cell::Cell::new(true));
    let feed = feeding.clone();
    lushtext_core::ui::input_quiet::simulate_user_input();
    glib::timeout_add_local_full(Duration::from_millis(1), glib::Priority::HIGH, move || {
        if !feed.get() {
            return glib::ControlFlow::Break;
        }
        lushtext_core::ui::input_quiet::simulate_user_input();
        glib::ControlFlow::Continue
    });
    let started = std::time::Instant::now();
    let deadline = Duration::from_micros(u64::try_from(ATTACH_DEADLINE_US).expect("positive"));
    while started.elapsed() < deadline / 2 {
        flush_after_delay(Duration::from_millis(5));
    }
    assert_eq!(
        language_id(&editor),
        None,
        "the attach ran while keys arrived"
    );
    // Under continuous input, the deadline lets it run anyway: input keeps
    // arriving every millisecond, far inside the quiet interval, so only the
    // deadline can let the attach run.
    while started.elapsed() < SETTLE_BUDGET && language_id(&editor).as_deref() != Some("html") {
        flush_after_delay(Duration::from_millis(2));
    }
    feeding.set(false);
    assert_eq!(language_id(&editor).as_deref(), Some("html"));
    assert!(
        started.elapsed() >= deadline,
        "the attach ran before its deadline"
    );
    assert_eq!(editor.load_evidence().language_attach.deadline_runs, 1);
}

#[test]
fn test_an_opened_document_reaches_settled() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "page.html", HTML);
    let editor = opened(&window, &path);
    wait_until(SETTLE_BUDGET, || {
        editor.load_evidence().settle_stage == SettleStage::Settled
    });
    assert_eq!(language_id(&editor).as_deref(), Some("html"));
}

/// `lines` lines of `width` visible characters.
fn dense(width: usize, lines: usize) -> String {
    format!("{}\n", "d".repeat(width)).repeat(lines)
}

#[test]
fn test_a_dense_document_installs_in_paced_slices() {
    for wrap in [false, true] {
        ensure_gtk_init();
        let window = test_window();
        present_window(&window);
        let dir = tempfile::tempdir().expect("tempdir");
        let text = dense(710, 160);
        let path = write(&dir, "dense.txt", &text);
        window.open_document(&path);
        let editor = active_editor(&window);
        editor.source_view().set_wrap_mode(if wrap {
            gtk4::WrapMode::WordChar
        } else {
            gtk4::WrapMode::None
        });
        wait_until(SETTLE_BUDGET, || {
            editor.load_state() == EditorLoadState::Loaded
        });
        let evidence = editor.load_evidence();
        assert!(
            evidence.installation_chunked,
            "a dense document must be sliced"
        );
        let visible = u64::try_from(text.len() - 160).expect("fits");
        assert!(
            evidence.installation_slice_count >= visible.div_ceil(V_SLICE),
            "{} slices for {visible} visible characters",
            evidence.installation_slice_count
        );
        assert_eq!(crate::common::editor_text(&editor), text);
    }
}

#[test]
fn test_editorconfig_tab_width_does_not_change_after_a_dense_install_starts() {
    // GtkTextView invalidates every line on a default-style property change
    // and queues no resize (A37), so on a dense document such a change after
    // the first slice re-lays out everything installed so far in one dispatch.
    // EditorConfig's tab width (R17) is therefore applied before the first
    // slice; this test fails when it is applied after it.
    //
    // `bottom-margin` is deliberately not observed. The end-of-document
    // overscroll follows the viewport height, which the editor's map handler
    // and its viewport-bounds handler already apply before a load's planning
    // worker returns, so no fixture of an opened, mapped editor changes it
    // after the first slice, with or without the before-install refresh (that
    // refresh only covers an install starting before the map idle has run).
    // Its later changes come from a resize or the first map of a background
    // tab, which the A73 exception runs at `G_PRIORITY_LOW` instead.
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    write(&dir, ".editorconfig", "root = true\n\n[*]\ntab_width = 7\n");
    let path = write(&dir, "dense.txt", &dense(710, 160));
    window.open_document(&path);
    let editor = active_editor(&window);
    let late_changes = std::rc::Rc::new(std::cell::Cell::new(0u32));
    {
        let late_changes = late_changes.clone();
        let buffer = editor.buffer();
        editor
            .source_view()
            .connect_notify_local(Some("tab-width"), move |_, _| {
                // The tab opened empty, so text in the buffer means the first
                // slice is in.
                if buffer.char_count() > 0 {
                    late_changes.set(late_changes.get() + 1);
                }
            });
    }
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
    });
    flush_after_delay(Duration::from_millis(300));
    assert!(editor.load_evidence().installation_chunked);
    assert_eq!(editor.source_view().tab_width(), 7);
    assert_eq!(
        late_changes.get(),
        0,
        "tab-width changed after the first slice"
    );
}

#[test]
fn test_a_short_line_document_stays_direct() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "short.txt", &dense(40, 2_000));
    let editor = opened(&window, &path);
    let evidence = editor.load_evidence();
    assert!(!evidence.installation_chunked);
    assert_eq!(evidence.installation_slice_count, 0);
}

#[test]
fn test_editorconfig_is_applied_before_the_first_insert() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    write(&dir, ".editorconfig", "root = true\n\n[*]\ntab_width = 7\n");
    let path = write(&dir, "tabs.txt", "\tindented\n");
    let editor = opened(&window, &path);
    let evidence = editor.load_evidence();
    assert!(evidence.formatting_applied_before_insert);
    assert_eq!(editor.formatting_overrides().tab_width, Some(7));
    assert_eq!(editor.source_view().tab_width(), 7);
}

#[test]
fn test_the_glob_table_guesses_exactly_like_gtksourceview() {
    ensure_gtk_init();
    lushtext_core::ui::language_catalog::start();
    wait_until(SETTLE_BUDGET, lushtext_core::ui::language_catalog::is_ready);
    let table = lushtext_core::ui::language_catalog::glob_table().expect("catalog ready");
    let manager = lushtext_core::ui::language_catalog::manager();
    // A name for every glob of every shipped language, as a full path the
    // way LushText passes it (review R4), plus names that overlap globs.
    let mut names: Vec<String> = table
        .languages()
        .flat_map(|language| language.globs.iter())
        .filter(|glob| !glob.is_empty())
        .flat_map(|glob| {
            let bare = glob.replace('*', "x").replace('?', "y");
            [format!("/home/u/project/{bare}"), bare]
        })
        .collect();
    names.extend(
        [
            "/p/CMakeLists.txt",
            "CMakeLists.txt",
            "/p/x.h",
            "/p/x.in",
            "/p/Makefile",
            "/p/makefile",
            "/p/x.HTML",
            "/p/x.tar.gz",
            "/p/.bashrc",
            "/p/x.md",
            "/p/x.rs",
            "/p/x.json",
            "/p/no-extension",
            "/p/dir.with.dots/file",
        ]
        .map(str::to_owned),
    );
    assert!(names.len() > 100, "{} names", names.len());
    for name in &names {
        let expected = manager
            .guess_language(Some(name.as_str()), None::<&str>)
            .map(|language| language.id().to_string());
        assert_eq!(table.guess(name).map(str::to_owned), expected, "{name}");
    }
}

/// A presented window with a dense 710 document open and installed.
fn dense_window(name: &str) -> (LushtextWindow, LushtextEditorPage, tempfile::TempDir) {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, name, &dense(710, 400));
    let editor = opened(&window, &path);
    (window, editor, dir)
}

#[test]
fn test_a_wrap_toggle_on_a_dense_document_is_paced() {
    let (_window, editor, _dir) = dense_window("dense-wrap.txt");
    let before = editor.load_evidence().restyle;
    let wrap = editor.source_view().wrap_mode() != gtk4::WrapMode::None;
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", !wrap);
    let after_interaction = editor.load_evidence().restyle;
    assert_eq!(
        after_interaction.restyles,
        before.restyles + 1,
        "the toggle was not paced"
    );
    assert!(after_interaction.pending);
    wait_until(SETTLE_BUDGET, || !editor.load_evidence().restyle.pending);
    let done = editor.load_evidence().restyle;
    assert!(done.reveals > 1, "the rest was revealed in one dispatch");
    assert!(
        done.largest_reveal_chars <= V_SLICE,
        "a reveal dispatch made {} characters visible",
        done.largest_reveal_chars
    );
    assert_eq!(crate::common::editor_text(&editor), dense(710, 400));
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", wrap);
}

#[test]
fn test_a_selection_during_a_paced_reveal_reveals_everything_before_a_copy() {
    // GtkTextBuffer's clipboard serializer copies visible text only, so a cut
    // of hidden paragraphs would lose them.
    let (_window, editor, _dir) = dense_window("dense-select.txt");
    let wrap = editor.source_view().wrap_mode() != gtk4::WrapMode::None;
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", !wrap);
    assert!(editor.load_evidence().restyle.pending);
    let buffer = editor.buffer();
    let (start, end) = buffer.bounds();
    buffer.select_range(&start, &end);
    assert!(
        !editor.load_evidence().restyle.pending,
        "a selection must end the reveal"
    );
    let (start, end) = buffer.bounds();
    assert_eq!(
        buffer.text(&start, &end, false).as_str(),
        dense(710, 400),
        "every paragraph is visible to the clipboard"
    );
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", wrap);
}

#[test]
fn test_a_document_that_is_not_dense_restyles_directly() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let editor = opened(&window, &write(&dir, "short.txt", &dense(40, 400)));
    let wrap = editor.source_view().wrap_mode() != gtk4::WrapMode::None;
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", !wrap);
    assert_eq!(editor.load_evidence().restyle.restyles, 0);
    assert_eq!(
        editor.source_view().wrap_mode() != gtk4::WrapMode::None,
        !wrap
    );
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", wrap);
}

#[test]
fn test_a_restyle_paces_the_selected_dense_editor_first_and_the_other_when_shown() {
    let (window, background, dir) = dense_window("first.txt");
    let selected = opened(&window, &write(&dir, "second.txt", &dense(710, 400)));
    assert!(!background.source_view().is_mapped());
    let paced = lushtext_core::ui::editor_page::restyle::prepare_paced_restyle_everywhere();
    assert_eq!(paced.len(), 2);
    lushtext_core::ui::editor_page::restyle::start_paced_reveal_everywhere(&paced);
    wait_until(SETTLE_BUDGET, || !selected.load_evidence().restyle.pending);
    flush_after_delay(Duration::from_millis(300));
    let waiting = background.load_evidence().restyle;
    assert!(
        waiting.pending && waiting.reveals == 0,
        "a hidden editor revealed while unmapped"
    );
    let tab_view = &window.imp().tab_view;
    tab_view.set_selected_page(&tab_view.page(&background));
    wait_until(SETTLE_BUDGET, || {
        !background.load_evidence().restyle.pending
    });
    assert!(background.load_evidence().restyle.largest_reveal_chars <= V_SLICE);
}

/// Count every change of the view's `editable` property: a default-style
/// property, whose change invalidates every line of the document (A37).
fn count_editable_changes(editor: &LushtextEditorPage) -> std::rc::Rc<std::cell::Cell<u32>> {
    let changes = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let counter = changes.clone();
    editor
        .source_view()
        .connect_notify_local(Some("editable"), move |_, _| counter.set(counter.get() + 1));
    changes
}

#[test]
fn test_install_replacement_and_save_never_toggle_editable() {
    // Restoring `editable` at a terminal re-laid out the whole document in
    // 2000 px batches of up to 18 ms, after a first dispatch of 42-45 ms (an
    // 11 MiB restore). The workflows hold the buffer-level edit lock instead.
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = dense(710, 160);
    let path = write(&dir, "dense-lock.txt", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    let changes = count_editable_changes(&editor);
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
    });
    assert_eq!(changes.get(), 0, "the dense install toggled editable");

    let outcomes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    editor.replace_buffer_for_test(
        dense(700, 160),
        1,
        std::rc::Rc::new(std::cell::Cell::new(true)),
        outcomes.clone(),
    );
    wait_until(SETTLE_BUDGET, || !outcomes.borrow().is_empty());
    assert_eq!(changes.get(), 0, "the replacement toggled editable");

    let saved = std::rc::Rc::new(std::cell::Cell::new(false));
    let saved_flag = saved.clone();
    editor.save_file_async(move |_| saved_flag.set(true));
    wait_until(SETTLE_BUDGET, || saved.get());
    assert_eq!(changes.get(), 0, "the save toggled editable");
    assert!(editor.accepts_user_edits());
}

/// Every route an edit takes to the buffer, as GTK drives it: a key
/// (`insert-at-cursor`, the keybinding signal typing reaches), a paste from
/// the clipboard, an interactive insert (the route drag-and-drop and
/// input-method commits take: `gtk_text_buffer_insert_interactive`), a
/// deletion, a paintable, a child anchor, and undo.
///
/// The three typed routes are held and inserted at the caret once the
/// workflow releases the buffer (`keep-keystrokes-during-install`), as
/// [`TYPED_ROUTES`]; the others are refused.
/// What the typed routes of [`attempt_every_edit_route`] insert, in order:
/// the key and the interactive insert synchronously, the paste when the
/// clipboard read completes.
const TYPED_ROUTES: &str = "KEYDROPPASTE";

fn attempt_every_edit_route(editor: &LushtextEditorPage) {
    let view = editor.source_view();
    view.emit_by_name::<()>("insert-at-cursor", &[&"KEY"]);
    view.clipboard().set_text("PASTE");
    view.emit_paste_clipboard();
    let buffer = editor.buffer();
    let _ = buffer.insert_interactive_at_cursor("DROP", true);
    // A paintable and a child anchor each add U+FFFC through their own
    // signals, not `insert-text`.
    let mut at = buffer.end_iter();
    buffer.insert_paintable(&mut at, &gtk4::gdk::Paintable::new_empty(1, 1));
    let mut at = buffer.end_iter();
    buffer.insert_child_anchor(&mut at, &gtk4::TextChildAnchor::new());
    let (mut start, mut end) = buffer.bounds();
    buffer.delete(&mut start, &mut end);
    buffer.undo();
    flush_after_delay(Duration::from_millis(100));
}

#[test]
fn test_every_edit_route_is_held_or_refused_while_a_dense_install_owns_the_buffer() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = dense(710, 400);
    let path = write(&dir, "dense-refuse.txt", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    // Step the main loop one dispatch at a time until the first slice is in:
    // a draining wait would run the whole paced install.
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + SETTLE_BUDGET;
    while editor.buffer().char_count() == 0 {
        assert!(std::time::Instant::now() < deadline, "no slice arrived");
        context.iteration(true);
    }
    assert_eq!(editor.load_state(), EditorLoadState::Loading);
    assert!(!editor.accepts_user_edits());
    attempt_every_edit_route(&editor);
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
            && crate::common::editor_text(&editor).len() == TYPED_ROUTES.len() + text.len()
    });
    // Only the typed routes reached the document, at the caret, after the
    // install; the refused ones changed nothing.
    assert_eq!(
        crate::common::editor_text(&editor),
        format!("{TYPED_ROUTES}{text}")
    );
    assert!(editor.is_modified());
    assert!(editor.draft_dirty());
    assert!(editor.accepts_user_edits());
    // The refused undo popped nothing: one undo removes the typed text.
    editor.buffer().undo();
    assert_eq!(crate::common::editor_text(&editor), text);
}

#[test]
fn test_every_edit_route_is_held_or_refused_while_a_save_owns_the_buffer() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "save-refuse.txt", "start\n");
    let editor = opened(&window, &path);
    // Short lines: one giant paragraph would enter Protected View (C2).
    let typed = crate::common::short_lines('x', 2_500_001);
    // One undoable user edit, so a refused undo has history it must keep.
    let mut end = editor.buffer().end_iter();
    let _ = editor.buffer().insert_interactive(&mut end, &typed, true);
    assert!(editor.buffer().can_undo());
    editor.reset_transient_save_admission_for_test();
    editor.pause_next_save_snapshot_for_test();
    let saved = std::rc::Rc::new(std::cell::Cell::new(None));
    let saved_result = saved.clone();
    editor.save_file_async(move |result| saved_result.set(Some(result.is_ok())));
    wait_until(SETTLE_BUDGET, || editor.save_evidence().capture_in_flight);
    assert!(!editor.accepts_user_edits());
    attempt_every_edit_route(&editor);
    editor.resume_save_snapshot_for_test();
    let expected = format!("start\n{typed}");
    wait_until(SETTLE_BUDGET, || {
        saved.get().is_some()
            && crate::common::editor_text(&editor).len() == TYPED_ROUTES.len() + expected.len()
    });
    assert_eq!(saved.get(), Some(true));
    // The save wrote what the buffer held when it took the buffer; the typed
    // routes went in at the caret after it, leaving the document modified.
    assert_eq!(
        lushtext_core::services::filesystem::read::text(&path).expect("saved file"),
        expected
    );
    assert_eq!(
        crate::common::editor_text(&editor),
        format!("{TYPED_ROUTES}{expected}")
    );
    assert!(editor.is_modified());
    assert!(editor.accepts_user_edits());
    // The refused undo did not consume the history: the typed text is one
    // undo step, and the earlier edit is still undoable after it.
    editor.buffer().undo();
    assert_eq!(crate::common::editor_text(&editor), expected);
    assert!(editor.buffer().can_undo());
    editor.buffer().undo();
    assert_eq!(crate::common::editor_text(&editor), "start\n");
}

#[test]
fn test_a_user_action_during_a_replacement_leaves_bookmark_lines_alone() {
    // GTK brackets a key press in a user action even when the edit lock
    // refuses the insert; the bookmark reconcile then read marks the
    // half-cleared buffer had moved, and persisted those lines.
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = dense(710, 400);
    let path = write(&dir, "dense-bookmark.txt", &text);
    let editor = opened(&window, &path);
    wait_until(SETTLE_BUDGET, || {
        let evidence = window.notes_evidence();
        evidence.active_document_file_backed && evidence.active_document_sidecar_resolved
    });
    let buffer = editor.buffer();
    buffer.place_cursor(&buffer.iter_at_line(300).expect("line 300"));
    let _ = editor.toggle_bookmark_at_cursor();
    assert_eq!(editor.bookmark_records().first().map(|b| b.line), Some(300));
    let changes = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let counted = changes.clone();
    editor.connect_bookmarks_changed(move || counted.set(counted.get() + 1));

    let outcomes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    editor.replace_buffer_for_test(
        text,
        1,
        std::rc::Rc::new(std::cell::Cell::new(true)),
        outcomes.clone(),
    );
    // One dispatch at a time until the old text is partly cleared.
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + SETTLE_BUDGET;
    while editor.buffer().line_count() > 300 && outcomes.borrow().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the clear never started"
        );
        context.iteration(true);
    }
    assert!(
        outcomes.borrow().is_empty(),
        "the replacement already finished"
    );
    buffer.begin_user_action();
    buffer.end_user_action();
    assert_eq!(
        changes.get(),
        0,
        "a user action during the replacement reconciled the bookmarks"
    );
    wait_until(SETTLE_BUDGET, || !outcomes.borrow().is_empty());
}

/// The view's text `GtkDropTarget`s (GtkTextView's own, which accepts a
/// string), as GTK's drop dispatch would find them.
pub(crate) fn drop_targets(editor: &LushtextEditorPage) -> Vec<gtk4::DropTarget> {
    let controllers = editor.source_view().observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|item| item.downcast::<gtk4::DropTarget>().ok())
        .filter(|target| {
            target
                .formats()
                .is_some_and(|formats| formats.contains_type(glib::Type::STRING))
        })
        .collect()
}

/// Drive every drop target's `drop` with `text`, as GTK's drop dispatch does
/// (the ledger A63 probe's method). Returns whether any target reported the
/// drop done. (`motion` is not driven: without a real `GdkDrop` the view's
/// motion handler reads a null drop.)
pub(crate) fn drop_text(editor: &LushtextEditorPage, text: &str) -> bool {
    let mut accepted = false;
    for target in drop_targets(editor) {
        accepted |= target.emit_by_name::<bool>(
            "drop",
            &[&glib::BoxedValue(text.to_value()), &5.0f64, &5.0f64],
        );
    }
    accepted
}

#[test]
fn test_a_drop_is_refused_while_a_save_owns_the_buffer() {
    // The view stays editable under the edit lock, so GtkTextView's drop
    // handler reported a drop the buffer refused as done
    // (`gtk_text_buffer_insert_interactive` returns TRUE), and a MOVE drag's
    // source then deleted its text: gone from both ends.
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "save-drop.txt", "start\n");
    let editor = opened(&window, &path);
    let typed = crate::common::short_lines('x', 2_500_001);
    let mut end = editor.buffer().end_iter();
    let _ = editor.buffer().insert_interactive(&mut end, &typed, true);
    editor.reset_transient_save_admission_for_test();
    editor.pause_next_save_snapshot_for_test();
    let saved = std::rc::Rc::new(std::cell::Cell::new(None));
    let saved_result = saved.clone();
    editor.save_file_async(move |result| saved_result.set(Some(result.is_ok())));
    wait_until(SETTLE_BUDGET, || editor.save_evidence().capture_in_flight);
    assert!(!editor.accepts_user_edits());
    assert!(
        !drop_text(&editor, "DROPPED"),
        "a drop was reported done while the save held the edit lock"
    );
    editor.resume_save_snapshot_for_test();
    wait_until(SETTLE_BUDGET, || saved.get().is_some());
    assert_eq!(saved.get(), Some(true));
    assert_eq!(
        crate::common::editor_text(&editor),
        format!("start\n{typed}")
    );
    // Unlocked again: the drop target is back and a drop lands.
    assert!(editor.accepts_user_edits());
    assert!(
        !drop_targets(&editor).is_empty(),
        "the drop target did not come back"
    );
    assert!(drop_text(&editor, "DROPPED"));
    assert!(crate::common::editor_text(&editor).contains("DROPPED"));
}

#[test]
fn test_a_replacement_leaves_no_modified_marks_collapsed_on_the_first_line() {
    // Clearing the old text moved every modified-line mark onto line 0, and
    // GtkSourceView sorts a line's marks in every frame's gutter and
    // background paint: a dense restore of 2 MB took about 100 s in the debug
    // widget lane, each frame spending hundreds of milliseconds in that sort.
    let (_window, editor, _dir) = dense_window("dense-marks.txt");
    let buffer = editor.buffer();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, &dense(500, 400));
    flush_after_delay(Duration::from_millis(200));
    let marks_before: usize = (0..buffer.line_count())
        .map(|line| buffer.source_marks_at_line(line, None).len())
        .sum();
    assert!(
        marks_before > 1,
        "precondition: the edit recorded modified marks"
    );

    let outcomes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    editor.replace_buffer_for_test(
        dense(700, 400),
        1,
        std::rc::Rc::new(std::cell::Cell::new(true)),
        outcomes.clone(),
    );
    wait_until(SETTLE_BUDGET, || !outcomes.borrow().is_empty());
    assert!(
        buffer.source_marks_at_line(0, None).len() <= 1,
        "{} marks collapsed on the first line",
        buffer.source_marks_at_line(0, None).len()
    );
}

#[test]
fn test_an_inline_alert_leaves_a_dense_document_margin_alone() {
    // An alert above the editor shortens its view. Recomputing the
    // end-of-document overscroll from the shorter view changed
    // `bottom-margin`, a default-style property (A37), and GtkTextView laid
    // out the whole dense document again: a train of ~18 ms validation
    // dispatches after every 11 MiB local-history restore, whose "Restored"
    // alert is such an alert.
    let (_window, editor, _dir) = dense_window("dense-alert.txt");
    flush_after_delay(Duration::from_millis(300));
    let view = editor.source_view();
    let margin = view.bottom_margin();
    let changes = std::rc::Rc::new(std::cell::Cell::new(0u32));
    {
        let changes = changes.clone();
        view.connect_notify_local(Some("bottom-margin"), move |_, _| {
            changes.set(changes.get() + 1);
        });
    }
    let height = view.visible_rect().height();
    editor.emit_inline_notification(
        InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Restored from Local History".to_string(),
            body: "The previous buffer state was saved as a safety snapshot.".to_string(),
            primary_button: Some("Undo Restore".to_string()),
            secondary_button: None,
        },
        InlineAlertLifetime::SupersededByLoad,
    );
    wait_until(SETTLE_BUDGET, || view.visible_rect().height() < height);
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(view.bottom_margin(), margin);
    assert_eq!(
        changes.get(),
        0,
        "the alert changed the dense document's margin"
    );
}

#[test]
fn test_a_paced_reveal_never_removes_its_tag_beyond_what_it_hid() {
    // Removing a size-affecting tag invalidates the whole range given, tagged
    // or not, so the reveal's cleanup must not remove it from the whole
    // buffer: that re-laid out the whole dense document after every reveal.
    let (_window, editor, _dir) = dense_window("dense-reveal-cleanup.txt");
    let wrap = editor.source_view().wrap_mode() != gtk4::WrapMode::None;
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", !wrap);
    assert!(editor.load_evidence().restyle.pending);
    let widest = std::rc::Rc::new(std::cell::Cell::new(0i32));
    let handler = {
        let widest = widest.clone();
        editor.buffer().connect_remove_tag(move |_, _, start, end| {
            widest.set(widest.get().max(end.offset() - start.offset()));
        })
    };
    wait_until(SETTLE_BUDGET, || !editor.load_evidence().restyle.pending);
    editor.buffer().disconnect(handler);
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", wrap);
    let bound = i32::try_from(2 * V_SLICE).unwrap_or(i32::MAX);
    assert!(
        widest.get() <= bound,
        "a removal spanned {} characters of a {}-character buffer",
        widest.get(),
        editor.buffer().char_count()
    );
}

// ─── Chunked highlighted install (`chunked-highlight-attach`, C6b) ──────────

/// About `bytes` of short-line Markdown: headings, lists, emphasis, code.
fn markdown(bytes: usize) -> String {
    let mut text = String::with_capacity(bytes + 128);
    let mut section = 0usize;
    while text.len() < bytes {
        section += 1;
        text.push_str(&format!("## Section {section}\n\n"));
        text.push_str(&format!(
            "Paragraph {section} with *emphasis*, **strong** text and `inline code`.\n\n"
        ));
        for item in 0..4 {
            text.push_str(&format!(
                "- item {item} of [link {section}](https://example.com/{section})\n"
            ));
        }
        text.push_str("\n```rust\nfn main() { println!(\"hi\"); }\n```\n\n");
    }
    text
}

/// The bytes above Markdown's attach-after-install threshold and within
/// `S_SYNTAX` (1 MiB): on `main` its syntax is off (`TotalSize`).
const LARGE_MARKDOWN: usize = 400 * 1024;

/// Record the buffer's character count every time a language is set on it.
fn chars_at_attach(editor: &LushtextEditorPage) -> std::rc::Rc<std::cell::RefCell<Vec<i32>>> {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let record = seen.clone();
    editor
        .buffer()
        .connect_notify_local(Some("language"), move |buffer, _| {
            if buffer.language().is_some() {
                record.borrow_mut().push(buffer.char_count());
            }
        });
    seen
}

#[test]
fn test_a_large_markdown_document_installs_highlighted_chunk_by_chunk() {
    use lushtext_core::model::highlight_chunking::HIGHLIGHT_CHUNK_BYTES;
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = markdown(LARGE_MARKDOWN);
    let path = write(&dir, "large.md", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    let at_attach = chars_at_attach(&editor);
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
    });
    assert!(
        editor.document_scale().syntax_enabled,
        "a 400 KiB Markdown document keeps syntax highlighting"
    );
    assert_eq!(language_id(&editor).as_deref(), Some("markdown"));
    let evidence = editor.load_evidence();
    assert!(evidence.installation_highlight_chunked);
    let chunks = u64::try_from(text.len())
        .expect("fits")
        .div_ceil(HIGHLIGHT_CHUNK_BYTES);
    assert!(
        evidence.installation_slice_count >= chunks,
        "{} slices for {} bytes",
        evidence.installation_slice_count,
        text.len()
    );
    assert_eq!(evidence.language_attach.install_requests, 1);
    let attaches = at_attach.borrow().clone();
    assert!(!attaches.is_empty(), "the language was attached");
    assert!(
        attaches
            .iter()
            .all(|chars| u64::try_from(*chars).expect("positive") <= HIGHLIGHT_CHUNK_BYTES),
        "every attach met at most one chunk: {attaches:?}"
    );
    assert_eq!(crate::common::editor_text(&editor), text);
    assert!(editor.buffer().is_highlight_syntax());
    wait_until(SETTLE_BUDGET, || {
        editor.load_evidence().settle_stage == SettleStage::Settled
    });
}

#[test]
fn test_a_large_html_document_stays_gated() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = HTML.repeat((64 * 1024) / HTML.len());
    let path = write(&dir, "large.html", &text);
    let editor = opened(&window, &path);
    flush_after_delay(Duration::from_millis(300));
    assert!(!editor.document_scale().syntax_enabled);
    assert!(!editor.load_evidence().installation_highlight_chunked);
    assert_eq!(language_id(&editor), None);
}

#[test]
fn test_a_markdown_document_below_its_threshold_keeps_the_c6_order() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = markdown(64 * 1024);
    let path = write(&dir, "small.md", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    let at_attach = chars_at_attach(&editor);
    wait_until(SETTLE_BUDGET, || {
        language_id(&editor).as_deref() == Some("markdown")
    });
    assert!(!editor.load_evidence().installation_highlight_chunked);
    let whole = i32::try_from(text.chars().count()).expect("fits");
    let attaches = at_attach.borrow().clone();
    assert!(
        attaches.iter().all(|chars| *chars == 0 || *chars == whole),
        "below its threshold the language meets an empty buffer or the whole text: {attaches:?}"
    );
}

#[test]
fn test_a_background_tab_installs_highlighted_without_being_shown() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = markdown(LARGE_MARKDOWN);
    let background_path = write(&dir, "background.md", &text);
    let foreground_path = write(&dir, "foreground.txt", "plain\n");
    // Every open selects its page; only the last one stays shown, before
    // either document was read.
    window.open_document(&background_path);
    window.open_document(&foreground_path);
    let tab_view = &window.imp().tab_view;
    let editor_at = |index: i32| {
        tab_view
            .nth_page(index)
            .child()
            .downcast::<LushtextEditorPage>()
            .expect("editor page")
    };
    let (background, foreground) = (editor_at(0), editor_at(1));
    wait_until(SETTLE_BUDGET, || {
        background.load_state() == EditorLoadState::Loaded
    });
    assert!(!background.source_view().is_mapped());
    assert!(background.load_evidence().installation_highlight_chunked);
    assert_eq!(language_id(&background).as_deref(), Some("markdown"));
    assert_eq!(crate::common::editor_text(&background), text);
    assert_eq!(foreground.load_state(), EditorLoadState::Loaded);
}

#[test]
fn test_a_cancelled_chunked_install_leaves_no_language_and_no_partial_text() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "cancel.md", &markdown(LARGE_MARKDOWN));
    window.open_document(&path);
    let editor = active_editor(&window);
    // Cancel from inside an insert once a few chunks are in with the
    // language attached: the test loop drains every ready source, so the
    // install would otherwise finish between two polls. A reentrant cancel
    // is a supported path (the session is unborrowed while GTK inserts).
    let cancelled_at = std::rc::Rc::new(std::cell::Cell::new(None));
    {
        let (record, weak) = (cancelled_at.clone(), editor.downgrade());
        editor.buffer().connect_changed(move |buffer| {
            if record.get().is_none()
                && buffer.language().is_some()
                && buffer.char_count() > 3 * 8 * 1024
                && let Some(editor) = weak.upgrade()
                && editor.load_state() == EditorLoadState::Loading
            {
                record.set(Some(buffer.char_count()));
                editor.cancel_load();
            }
        });
    }
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Failed && editor.buffer().char_count() == 0
    });
    assert!(
        cancelled_at.get().is_some(),
        "the install was cancelled while chunking"
    );
    assert_eq!(
        language_id(&editor),
        None,
        "the partial chunks were cleared without a language"
    );
    assert!(editor.load_evidence().installation_incomplete);
}

#[test]
fn test_a_reload_of_a_highlighted_large_document_detaches_before_clearing() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = markdown(LARGE_MARKDOWN);
    let path = write(&dir, "reload.md", &text);
    let editor = opened(&window, &path);
    wait_until(SETTLE_BUDGET, || {
        language_id(&editor).as_deref() == Some("markdown")
    });
    let detached_with = std::rc::Rc::new(std::cell::Cell::new(None));
    {
        let record = detached_with.clone();
        editor
            .buffer()
            .connect_notify_local(Some("language"), move |buffer, _| {
                if buffer.language().is_none() && record.get().is_none() {
                    record.set(Some(buffer.char_count()));
                }
            });
    }
    editor.load_file_async(&path);
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
            && language_id(&editor).as_deref() == Some("markdown")
    });
    assert_eq!(
        detached_with.get(),
        Some(i32::try_from(text.chars().count()).expect("fits")),
        "the language came off before the first deleted slice"
    );
    assert!(editor.load_evidence().installation_highlight_chunked);
    assert_eq!(crate::common::editor_text(&editor), text);
}

#[test]
fn test_most_chunks_go_in_on_the_engines_coverage_report() {
    // Review S5: the A84 gate itself. If the coverage predicate always failed,
    // every chunk would wait once behind the worker and this fails.
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "coverage.md", &markdown(LARGE_MARKDOWN));
    let editor = opened(&window, &path);
    let attach = editor.load_evidence().language_attach;
    let total = attach.chunks_on_coverage + attach.chunks_after_wait;
    assert!(total > 16, "{attach:?}");
    assert!(
        attach.chunks_on_coverage * 4 >= total * 3,
        "most chunks went in on coverage: {attach:?}"
    );
}

#[test]
fn test_a_chunk_without_a_coverage_report_waits_once_and_goes_in() {
    // Review S5, the stall fallback: highlighting switched off after the
    // attach, so the engine reports no analysed range; every later chunk
    // waits once at `G_PRIORITY_LOW + 1` and the install still finishes. If
    // the predicate always held, no chunk would wait and this fails.
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = markdown(LARGE_MARKDOWN);
    let path = write(&dir, "stall.md", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    // Stop every `highlight-updated` emission once the language is on during
    // the install, so the coverage watch hears nothing: an engine that
    // reports nothing (cleared by A29, say) must not park the install.
    let silenced = std::rc::Rc::new(std::cell::Cell::new(false));
    {
        let (silenced, weak) = (silenced.clone(), editor.downgrade());
        editor
            .buffer()
            .connect_local("highlight-updated", false, move |values| {
                if let Some(editor) = weak.upgrade()
                    && editor.load_state() == EditorLoadState::Loading
                    && editor.buffer().char_count() > 0
                {
                    silenced.set(true);
                    if let Ok(buffer) = values[0].get::<sourceview5::Buffer>() {
                        buffer.stop_signal_emission_by_name("highlight-updated");
                    }
                }
                None
            });
    }
    wait_until(Duration::from_secs(60), || {
        editor.load_state() == EditorLoadState::Loaded
    });
    assert!(silenced.get(), "the engine reported during the install");
    let attach = editor.load_evidence().language_attach;
    assert!(
        attach.chunks_after_wait > 16 && attach.chunks_on_coverage <= 1,
        "chunks waited once each: {attach:?}"
    );
    assert_eq!(crate::common::editor_text(&editor), text);
}

#[test]
fn test_a_rename_during_a_background_chunked_install_does_not_wait_for_the_tab() {
    // Review S1: the identity change re-requests the attach while the
    // install is parked on it; the stage must stay armed the install's way,
    // or the hidden tab would wait for a map that never comes.
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = markdown(LARGE_MARKDOWN);
    let background_path = write(&dir, "renamed-before.md", &text);
    let foreground_path = write(&dir, "foreground.txt", "plain\n");
    window.open_document(&background_path);
    window.open_document(&foreground_path);
    let background = window
        .imp()
        .tab_view
        .nth_page(0)
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("editor page");
    let renamed = dir.path().join("renamed-after.md");
    // A busy idle one step above `G_PRIORITY_LOW` holds the attach stage
    // (a `G_PRIORITY_LOW` idle) pending while the background install's own
    // steps (`G_PRIORITY_DEFAULT_IDLE + 1`) still run; the rename lands while
    // the install is parked on the stage, and the hold ends.
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    {
        let (done, weak) = (done.clone(), background.downgrade());
        glib::idle_add_local_full(
            glib::Priority::from(glib::ffi::G_PRIORITY_LOW - 1),
            move || {
                std::thread::sleep(Duration::from_micros(200));
                if done.get() || weak.upgrade().is_none() {
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            },
        );
    }
    {
        let (done, weak) = (done.clone(), background.downgrade());
        glib::timeout_add_local_full(Duration::from_millis(1), glib::Priority::HIGH, move || {
            let Some(editor) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if done.get() {
                return glib::ControlFlow::Break;
            }
            if editor.load_state() == EditorLoadState::Loading
                && editor.buffer().char_count() > 0
                && editor.load_evidence().language_attach.pending
            {
                done.set(true);
                editor.set_file_path(&renamed);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }
    // `set_file_path` reports the tab `Loaded` at once; the install itself
    // ends when its session does.
    wait_until(Duration::from_secs(30), || {
        done.get() && !background.load_evidence().installation_active
    });
    assert!(!background.source_view().is_mapped());
    assert_eq!(language_id(&background).as_deref(), Some("markdown"));
    assert_eq!(crate::common::editor_text(&background), text);
}

#[test]
fn test_an_evicted_large_markdown_reloads_highlighted_without_a_language_during_the_clear() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = markdown(LARGE_MARKDOWN);
    let large = write(&dir, "evicted.md", &text);
    let small = write(&dir, "small.txt", "plain\n");
    let editor = opened(&window, &large);
    wait_until(SETTLE_BUDGET, || {
        language_id(&editor).as_deref() == Some("markdown")
    });
    let _other = opened(&window, &small);
    let cleared_with = std::rc::Rc::new(std::cell::Cell::new(None));
    {
        let record = cleared_with.clone();
        editor
            .buffer()
            .connect_notify_local(Some("language"), move |buffer, _| {
                if buffer.language().is_none() && record.get().is_none() {
                    record.set(Some(buffer.char_count()));
                }
            });
    }
    editor.evict();
    wait_until(Duration::from_secs(30), || editor.is_evicted());
    assert_eq!(
        cleared_with.get(),
        Some(i32::try_from(text.chars().count()).expect("fits")),
        "the eviction's clear took the language off before its first slice"
    );
    let tab_view = &window.imp().tab_view;
    tab_view.set_selected_page(&tab_view.page(&editor));
    wait_until(Duration::from_secs(30), || {
        !editor.is_evicted()
            && editor.load_state() == EditorLoadState::Loaded
            && language_id(&editor).as_deref() == Some("markdown")
    });
    assert!(editor.load_evidence().installation_highlight_chunked);
    assert_eq!(crate::common::editor_text(&editor), text);
}

#[test]
fn test_a_save_formatting_mirror_back_installs_a_large_markdown_in_chunks() {
    use lushtext_core::model::highlight_chunking::HIGHLIGHT_CHUNK_BYTES;
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        &dir,
        ".editorconfig",
        "root = true\n[*]\ntrim_trailing_whitespace = true\n",
    );
    let clean = markdown(LARGE_MARKDOWN);
    let path = write(&dir, "mirror.md", &clean.replace(".\n", ".   \n"));
    let editor = opened(&window, &path);
    wait_until(SETTLE_BUDGET, || {
        language_id(&editor).as_deref() == Some("markdown")
    });
    let attaches = chars_at_attach(&editor);
    editor
        .buffer()
        .insert(&mut editor.buffer().end_iter(), "end.   \n");
    let saved = std::rc::Rc::new(std::cell::Cell::new(None));
    {
        let saved = saved.clone();
        editor.save_file_async(move |result| saved.set(Some(result.is_ok())));
    }
    wait_until(Duration::from_secs(60), || {
        saved.get().is_some() && !editor.buffer_replacement_evidence().in_progress
    });
    assert_eq!(saved.get(), Some(true));
    let expected = format!("{clean}end.\n");
    assert_eq!(
        crate::common::editor_text(&editor),
        expected,
        "mirrored back"
    );
    assert_eq!(language_id(&editor).as_deref(), Some("markdown"));
    let turns = editor.buffer_replacement_evidence().slice_count;
    assert!(
        turns
            >= u64::try_from(expected.len())
                .expect("fits")
                .div_ceil(HIGHLIGHT_CHUNK_BYTES),
        "{turns} turns"
    );
    let attaches = attaches.borrow().clone();
    assert!(
        !attaches.is_empty()
            && attaches
                .iter()
                .all(|chars| u64::try_from(*chars).expect("positive") <= HIGHLIGHT_CHUNK_BYTES),
        "every attach met at most one chunk: {attaches:?}"
    );
}

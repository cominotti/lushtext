// SPDX-License-Identifier: GPL-3.0-or-later

//! Held typing (`keep-keystrokes-during-install`): text typed while a load
//! installation, a whole-buffer replacement or a save owns the buffer, or
//! while the file is still being read, is kept and inserted at the caret once
//! the document is whole, as one undo step; everything else is refused with a
//! warning, and held text never goes into the wrong document.
//!
//! Keys are driven through GtkTextView's `insert-at-cursor` and `backspace`
//! keybinding signals, the routes a typed key takes (with the view's own
//! editability, unlike `insert_interactive_at_cursor(.., true)`).

use crate::common::{
    active_editor, editor_text, ensure_gtk_init, present_window, short_lines, test_window,
    wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use lushtext_core::ui::window::LushtextWindow;
use std::time::Duration;

/// A load, a paced install and the settle dispatch after it.
const BUDGET: Duration = Duration::from_secs(30);

fn write(dir: &tempfile::TempDir, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    lushtext_core::services::filesystem::fixture::write_text(&path, text);
    path
}

fn dense(width: usize, lines: usize) -> String {
    format!("{}\n", "d".repeat(width)).repeat(lines)
}

/// Type `text` one key at a time, as GtkTextView's key route does.
fn type_keys(editor: &LushtextEditorPage, text: &str) {
    let view = editor.source_view();
    for ch in text.chars() {
        view.emit_by_name::<()>("insert-at-cursor", &[&ch.to_string()]);
    }
}

fn backspace(editor: &LushtextEditorPage) {
    editor.source_view().emit_by_name::<()>("backspace", &[]);
}

/// Open `path` and step the main loop one dispatch at a time until the
/// install's first slice is in (a draining wait would run the whole install).
fn open_until_installing(window: &LushtextWindow, path: &std::path::Path) -> LushtextEditorPage {
    window.open_document(path);
    let editor = active_editor(window);
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + BUDGET;
    while editor.buffer().char_count() == 0 {
        assert!(std::time::Instant::now() < deadline, "no slice arrived");
        context.iteration(true);
    }
    assert_eq!(editor.load_state(), EditorLoadState::Loading);
    assert!(editor.edits_locked(), "the install holds the edit lock");
    editor
}

/// Wait until the load is published and no typed text is held.
fn wait_settled(editor: &LushtextEditorPage) {
    wait_until(BUDGET, || {
        editor.load_state() != EditorLoadState::Loading
            && !editor.edits_locked()
            && !editor.holds_typed_input()
    });
}

fn presented() -> (LushtextWindow, tempfile::TempDir) {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    (window, tempfile::tempdir().expect("tempdir"))
}

fn assert_typing_during_install_is_kept(text: &str, name: &str) {
    let (window, dir) = presented();
    let path = write(&dir, name, text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "hello");
    assert!(
        !editor_text(&editor).contains("hello"),
        "{name}: held text reached the buffer while the install owned it"
    );
    wait_settled(&editor);
    assert_eq!(
        editor_text(&editor),
        format!("hello{text}"),
        "{name}: every typed character, in order, at the caret the install left"
    );
    assert!(
        editor.is_modified(),
        "{name}: the replay modifies the document"
    );
    // One undo step removes the replay and nothing installed.
    editor.buffer().undo();
    assert_eq!(editor_text(&editor), text, "{name}: one undo step");
}

#[test]
fn test_typing_during_a_dense_install_keeps_every_character() {
    assert_typing_during_install_is_kept(&dense(710, 400), "dense-held.txt");
}

#[test]
fn test_typing_during_a_large_install_keeps_every_character() {
    // Short lines above the 1 MiB direct threshold: sliced by size.
    assert_typing_during_install_is_kept(&short_lines('t', 3_000_000), "large-held.txt");
}

#[test]
fn test_typing_while_the_file_is_read_is_not_deleted_by_the_install() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "planning-held.txt", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    assert_eq!(editor.load_state(), EditorLoadState::Loading);
    assert!(!editor.edits_locked(), "still planning: no install yet");
    type_keys(&editor, "early");
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("early{text}"));
}

#[test]
fn test_a_draft_holds_the_replayed_text() {
    use lushtext_core::services::{draft_service, json_store};
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "draft-held.txt", &text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "kept");
    wait_settled(&editor);
    assert!(editor.draft_dirty(), "the replay marks the draft dirty");
    window.autosave_tick_for_test();
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&path);
    wait_until(Duration::from_secs(60), || {
        let evidence = window.draft_evidence();
        !evidence.autosave_inflight && !evidence.mutation_inflight && !editor.draft_dirty()
    });
    let draft = draft_service::read_draft(&data_dir, &draft_id)
        .expect("read draft")
        .expect("a draft body");
    assert_eq!(draft, format!("kept{text}"));
}

#[test]
fn test_backspace_edits_held_text_and_other_edits_are_refused_with_a_warning() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "backspace-held.txt", &text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "abX");
    backspace(&editor);
    // A deletion cannot be held: refused, and reported. (The caret sits
    // before installed text, so Delete has a character to delete.)
    let buffer = editor.buffer();
    buffer.place_cursor(&buffer.start_iter());
    editor
        .source_view()
        .emit_by_name::<()>("delete-from-cursor", &[&gtk4::DeleteType::Chars, &1i32]);
    let refused = editor.held_input_evidence().refused_edits;
    assert!(refused >= 1, "the deletion was refused and reported");
    assert!(
        editor.held_input_evidence().notices >= 2,
        "holding and refusal notices"
    );
    type_keys(&editor, "c");
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("abc{text}"));
}

#[test]
fn test_an_insertion_over_the_cap_is_refused_whole() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "overflow-held.txt", &text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "a");
    let paste = "p".repeat(lushtext_core::model::held_input::HELD_TEXT_CAP_BYTES);
    editor
        .source_view()
        .emit_by_name::<()>("insert-at-cursor", &[&paste]);
    assert_eq!(editor.held_input_evidence().overflows, 1);
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("a{text}"));
}

#[test]
fn test_typing_into_a_document_that_opens_protected_shows_the_notice() {
    let (window, dir) = presented();
    // One paragraph above L_PROTECT_EDIT (50,000): Protected View.
    let text = format!("{}\nshort\n", "g".repeat(60_000));
    let path = write(&dir, "protected-held.txt", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    type_keys(&editor, "lost?");
    wait_settled(&editor);
    assert!(editor.is_protected_view(), "the document opened protected");
    assert!(
        !editor_text(&editor).contains("lost?"),
        "held text went into a view-only document"
    );
    let evidence = editor.held_input_evidence();
    assert_eq!(evidence.discarded_view_only, 1);
    assert_eq!(evidence.discarded_chars, 5);
    assert_eq!(evidence.replays, 0);
}

#[test]
fn test_a_cancelled_install_does_not_receive_the_held_text() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "cancel-held.txt", &text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "orphan");
    editor.cancel_load();
    wait_settled(&editor);
    assert_eq!(editor.load_state(), EditorLoadState::Failed);
    assert!(!editor_text(&editor).contains("orphan"));
    let evidence = editor.held_input_evidence();
    assert_eq!(evidence.discarded_no_document, 1);
    assert_eq!(evidence.discarded_chars, 6);
    assert_eq!(evidence.replays, 0);
}

#[test]
fn test_a_superseding_load_of_the_same_tab_receives_the_held_text_once() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "supersede-held.txt", &text);
    let other_text = dense(700, 300);
    let other = write(&dir, "supersede-other.txt", &other_text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "moved");
    // A newer load of the same tab supersedes the install, which clears what
    // it had inserted.
    editor.load_file_async(&other);
    wait_settled(&editor);
    assert_eq!(editor.load_state(), EditorLoadState::Loaded);
    assert_eq!(editor_text(&editor), format!("moved{other_text}"));
    let evidence = editor.held_input_evidence();
    assert_eq!(evidence.replays, 1);
    assert_eq!(evidence.replayed_chars, 5);
    // No other tab got it.
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_typing_during_a_replacement_is_inserted_after_it() {
    let (window, dir) = presented();
    let path = write(&dir, "replace-held.txt", "start\n");
    window.open_document(&path);
    let editor = active_editor(&window);
    wait_settled(&editor);
    // Above the 1 MiB direct threshold: the replacement runs in turns.
    let body = short_lines('r', 3_000_000);
    let outcomes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    editor.replace_buffer_for_test(
        body.clone(),
        1,
        std::rc::Rc::new(std::cell::Cell::new(true)),
        outcomes.clone(),
    );
    assert!(editor.edits_locked(), "the replacement holds the edit lock");
    type_keys(&editor, "after");
    wait_until(BUDGET, || !outcomes.borrow().is_empty());
    wait_settled(&editor);
    let buffer = editor.buffer();
    let caret = buffer.iter_at_mark(&buffer.get_insert()).offset();
    let installed = editor_text(&editor);
    assert_eq!(installed.chars().count(), body.chars().count() + 5);
    assert!(installed.contains("after"), "the held text was inserted");
    assert_eq!(editor.held_input_evidence().replays, 1);
    assert!(caret >= 5, "the caret follows the inserted text");
}

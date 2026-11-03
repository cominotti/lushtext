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
use libadwaita::prelude::{AdwApplicationWindowExt, AlertDialogExt};
use lushtext_core::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use lushtext_core::ui::window::LushtextWindow;
use std::time::Duration;

use crate::post_open_settle::{drop_targets, drop_text};

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
fn test_typing_into_a_document_that_opens_protected_goes_to_a_new_tab() {
    let (window, dir) = presented();
    // One paragraph above L_PROTECT_EDIT (50,000): Protected View.
    let text = format!("{}\nshort\n", "g".repeat(60_000));
    let path = write(&dir, "protected-held.txt", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    type_keys(&editor, "kept?");
    wait_settled(&editor);
    assert!(editor.is_protected_view(), "the document opened protected");
    assert!(
        !editor_text(&editor).contains("kept?"),
        "held text went into a view-only document"
    );
    let evidence = editor.held_input_evidence();
    assert_eq!(evidence.rescued_view_only, 1);
    assert_eq!(evidence.rescued_chars, 5);
    assert_eq!(evidence.replays, 0);
    assert_rescued_into_new_tab(&window, "kept?");
}

/// The held text is in a new Untitled tab, modified and draft-dirty (the
/// Preserved Drafts "Open" path), never dropped.
fn assert_rescued_into_new_tab(window: &LushtextWindow, typed: &str) {
    let tab_view = &window.imp().tab_view;
    let rescue = || {
        (0..tab_view.n_pages())
            .filter_map(|i| {
                tab_view
                    .nth_page(i)
                    .child()
                    .downcast::<LushtextEditorPage>()
                    .ok()
            })
            .find(|e| e.file_path().is_none() && editor_text(e) == typed)
    };
    wait_until(BUDGET, || rescue().is_some());
    let rescued = rescue().expect("the rescue tab");
    assert!(rescued.is_modified(), "the rescued text is unsaved work");
    assert!(
        rescued.draft_dirty(),
        "the rescued text reaches its own draft"
    );
}

#[test]
fn test_a_cancelled_install_does_not_receive_the_held_text_and_a_tab_does() {
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
    assert_eq!(evidence.rescued_no_document, 1);
    assert_eq!(evidence.rescued_chars, 6);
    assert_eq!(evidence.replays, 0);
    assert_rescued_into_new_tab(&window, "orphan");
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

/// Open `path`, wait for its load, and return the editor.
fn opened(window: &LushtextWindow, path: &std::path::Path) -> LushtextEditorPage {
    window.open_document(path);
    let editor = active_editor(window);
    wait_settled(&editor);
    editor
}

/// A document whose save snapshots in chunks (above the direct-snapshot
/// size), so `pause_next_save_snapshot_for_test` can hold the save. Short
/// lines: one giant paragraph would enter Protected View.
fn chunked_save_document() -> String {
    short_lines('s', 2_500_001)
}

/// Start a save whose snapshot waits until `resume_save_snapshot_for_test`,
/// so the save holds the buffer while the test types.
fn hold_a_save(editor: &LushtextEditorPage) -> std::rc::Rc<std::cell::Cell<Option<bool>>> {
    editor.reset_transient_save_admission_for_test();
    editor.pause_next_save_snapshot_for_test();
    let saved = std::rc::Rc::new(std::cell::Cell::new(None));
    let saved_result = saved.clone();
    editor.save_file_async(move |result| saved_result.set(Some(result.is_ok())));
    wait_until(BUDGET, || editor.save_evidence().capture_in_flight);
    assert!(editor.edits_locked(), "the save holds the edit lock");
    saved
}

/// Let the held save finish, stepping one dispatch at a time, and stop in the
/// gap between its release and the held text's settle dispatch.
fn finish_save_into_the_gap(
    editor: &LushtextEditorPage,
    saved: &std::rc::Rc<std::cell::Cell<Option<bool>>>,
) {
    editor.resume_save_snapshot_for_test();
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + BUDGET;
    while saved.get().is_none() || editor.is_saving() {
        assert!(std::time::Instant::now() < deadline, "the save never ended");
        context.iteration(true);
    }
    assert_eq!(saved.get(), Some(true));
    assert!(
        editor.has_held_text() && !editor.edits_locked(),
        "stopped in the release-to-settle gap"
    );
}

fn alert_dialog(window: &LushtextWindow) -> Option<libadwaita::AlertDialog> {
    window
        .visible_dialog()
        .and_then(|dialog| dialog.downcast::<libadwaita::AlertDialog>().ok())
}

fn wait_for_save_changes_dialog(window: &LushtextWindow) -> libadwaita::AlertDialog {
    wait_until(BUDGET, || {
        alert_dialog(window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading == "Save Changes?")
    });
    alert_dialog(window).expect("the Save Changes dialog")
}

fn choose_save(dialog: &libadwaita::AlertDialog) {
    dialog.emit_by_name::<()>("response", &[&"save"]);
}

#[test]
fn test_closing_a_tab_in_the_release_gap_asks_and_saves_the_held_text() {
    let (window, dir) = presented();
    let body = chunked_save_document();
    let path = write(&dir, "gap-tab.txt", &body);
    let editor = opened(&window, &path);
    let saved = hold_a_save(&editor);
    type_keys(&editor, "typed");
    finish_save_into_the_gap(&editor, &saved);
    assert!(
        !editor.is_modified(),
        "the held text is not in the buffer yet"
    );
    assert!(editor.has_unsaved_changes(), "held text counts as unsaved");
    // While text is held the drop targets stay off: a MOVE drag's source
    // would otherwise delete text the buffer never took.
    assert!(
        drop_targets(&editor).is_empty(),
        "drop targets came back early"
    );
    let page = window.imp().tab_view.selected_page().expect("the tab");
    window.imp().tab_view.close_page(&page);
    let dialog = wait_for_save_changes_dialog(&window);
    assert_eq!(window.imp().tab_view.n_pages(), 1);
    choose_save(&dialog);
    wait_until(BUDGET, || window.imp().tab_view.n_pages() == 0);
    assert_eq!(
        lushtext_core::services::filesystem::read::text(&path).expect("saved file"),
        format!("typed{body}"),
        "Save wrote the held text"
    );
}

#[test]
fn test_closing_the_window_in_the_release_gap_asks_and_saves_the_held_text() {
    let (window, dir) = presented();
    crate::common::wait_for_startup_data_flow(&window);
    let body = chunked_save_document();
    let path = write(&dir, "gap-window.txt", &body);
    let editor = opened(&window, &path);
    let saved = hold_a_save(&editor);
    type_keys(&editor, "typed");
    finish_save_into_the_gap(&editor, &saved);
    window.close();
    let dialog = wait_for_save_changes_dialog(&window);
    choose_save(&dialog);
    wait_until(BUDGET, || {
        lushtext_core::services::filesystem::read::text(&path)
            .is_ok_and(|t| t == format!("typed{body}"))
    });
}

#[test]
fn test_closing_a_loading_tab_with_held_text_is_refused_and_the_text_is_kept() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "loading-close.txt", &text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "wait");
    let page = window.imp().tab_view.selected_page().expect("the tab");
    // The refusal is synchronous: the close-page handler finishes it at once.
    window.imp().tab_view.close_page(&page);
    assert_eq!(
        window.imp().tab_view.n_pages(),
        1,
        "the close was not refused"
    );
    // Refused, not asked: a Save now would write a half-installed buffer.
    assert!(
        alert_dialog(&window).is_none(),
        "a dialog asked while a workflow owns the buffer"
    );
    assert_eq!(editor.held_input_evidence().held_chars, 4);
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("wait{text}"));
    // Closing the window while a load owns the buffer is refused the same way.
    let other = write(&dir, "loading-close-2.txt", &text);
    let editor = open_until_installing(&window, &other);
    type_keys(&editor, "w");
    window.close();
    assert!(
        alert_dialog(&window).is_none(),
        "no dialog while a workflow owns the buffer"
    );
    assert!(window.is_visible(), "the window closed with held text");
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("w{text}"));
}

#[test]
fn test_quit_asks_before_dropping_unsaved_documents() {
    let (window, dir) = presented();
    crate::common::wait_for_startup_data_flow(&window);
    let path = write(&dir, "quit.txt", "start\n");
    let editor = opened(&window, &path);
    type_keys(&editor, "x");
    assert!(editor.is_modified());
    let app = window.application().expect("the window's application");
    app.activate_action("quit", None);
    let dialog = wait_for_save_changes_dialog(&window);
    choose_save(&dialog);
    wait_until(BUDGET, || {
        lushtext_core::services::filesystem::read::text(&path).is_ok_and(|t| t == "xstart\n")
    });
}

#[test]
fn test_a_load_holds_the_drop_targets_until_the_held_text_is_in() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "drop-held.txt", &text);
    window.open_document(&path);
    let editor = active_editor(&window);
    assert_eq!(editor.load_state(), EditorLoadState::Loading);
    assert!(!editor.edits_locked(), "still planning: no install yet");
    // A drop reported done here would let a MOVE drag's source delete text
    // the install then clears.
    assert!(
        !drop_text(&editor, "DROPPED"),
        "a drop was accepted while loading"
    );
    type_keys(&editor, "a");
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("a{text}"));
    assert!(
        !drop_targets(&editor).is_empty(),
        "the drop targets came back"
    );
}

#[test]
fn test_a_tab_moved_to_another_window_keeps_its_load_and_held_text() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "transfer-held.txt", &text);
    let app = window
        .application()
        .and_downcast::<libadwaita::Application>()
        .expect("application");
    let other = LushtextWindow::new(&app);
    present_window(&other);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "moved");
    let page = window.imp().tab_view.selected_page().expect("the tab");
    window
        .imp()
        .tab_view
        .transfer_page(&page, &other.imp().tab_view, 0);
    assert_eq!(
        editor.load_state(),
        EditorLoadState::Loading,
        "precondition: the transfer happens mid-install"
    );
    wait_settled(&editor);
    assert_eq!(
        editor.load_state(),
        EditorLoadState::Loaded,
        "the transfer cancelled the load"
    );
    assert_eq!(editor_text(&editor), format!("moved{text}"));
    assert!(
        other.imp().open_paths.borrow().contains(&path),
        "the receiving window does not know the tab's path"
    );
    assert!(
        !window.imp().open_paths.borrow().contains(&path),
        "the source window still claims the path"
    );
}

#[test]
fn test_a_save_only_episode_replays_where_the_first_character_was_held() {
    let (window, dir) = presented();
    let body = chunked_save_document();
    let path = write(&dir, "anchor.txt", &format!("one\n{body}"));
    let editor = opened(&window, &path);
    let buffer = editor.buffer();
    buffer.place_cursor(&buffer.iter_at_line(1).expect("line 1"));
    let saved = hold_a_save(&editor);
    type_keys(&editor, "ab");
    // A click elsewhere while the save holds the buffer.
    buffer.place_cursor(&buffer.end_iter());
    editor.resume_save_snapshot_for_test();
    wait_until(BUDGET, || saved.get().is_some());
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("one\nab{body}"));
}

#[test]
fn test_typing_right_after_the_replay_never_undoes_installed_text() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "undo-after.txt", &text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "held");
    wait_settled(&editor);
    type_keys(&editor, "Z");
    assert_eq!(editor_text(&editor), format!("heldZ{text}"));
    // One undo action, like typing: the typing after it may coalesce with
    // it, but undo never reaches the installed document.
    let buffer = editor.buffer();
    let mut steps = 0;
    while buffer.can_undo() {
        buffer.undo();
        steps += 1;
        assert!(steps <= 2, "more undo steps than the typed text");
        assert!(editor_text(&editor).ends_with(&text));
    }
    assert_eq!(editor_text(&editor), text);
}

#[test]
fn test_an_overflowing_paste_is_never_partly_held() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "paste-held.txt", &text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "k");
    // One user action, two segments: the second overflows the cap.
    let half = "p".repeat(lushtext_core::model::held_input::HELD_TEXT_CAP_BYTES / 2 + 1);
    let buffer = editor.buffer();
    buffer.begin_user_action();
    let _ = buffer.insert_interactive_at_cursor(&half, true);
    let _ = buffer.insert_interactive_at_cursor(&half, true);
    buffer.end_user_action();
    assert_eq!(
        editor.held_input_evidence().held_chars,
        1,
        "a paste was partly held"
    );
    assert_eq!(editor.held_input_evidence().overflows, 1);
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("k{text}"));
}

#[test]
fn test_an_input_method_sees_and_edits_the_held_text() {
    let (window, dir) = presented();
    let text = dense(710, 400);
    let path = write(&dir, "ime-held.txt", &text);
    let editor = open_until_installing(&window, &path);
    type_keys(&editor, "viet");
    let view: gtk4::TextView = editor.source_view().clone().upcast();
    let controllers = view.observe_controllers();
    let im = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|item| item.downcast::<gtk4::EventControllerKey>().ok())
        .find_map(|key| key.im_context())
        .expect("the view's input-method context");
    let (surrounding, cursor, _) = im.surrounding_with_selection().expect("an answer");
    let before = &surrounding[..usize::try_from(cursor).expect("cursor")];
    assert!(
        before.ends_with("viet"),
        "the held text is not before the cursor: {before:?}"
    );
    // Telex: delete the last character, then commit the composed one.
    assert!(im.delete_surrounding(-1, 1));
    assert_eq!(editor.held_input_evidence().held_chars, 3);
    type_keys(&editor, "ê");
    wait_settled(&editor);
    assert_eq!(editor_text(&editor), format!("vieê{text}"));
}

// --- A partial install is never the user's modification -------------------
//
// While a load installs a document in slices the buffer holds part of the
// file, and GtkTextBuffer's modified flag says only that the buffer changed.
// Without held text or a recovered draft the tab is not modified by the user:
// its close asks nothing, and no save, from Ctrl+S, the Save Changes dialog or
// Save As, may write the partial (or still empty) buffer over a file.

/// The file's bytes, read back.
fn bytes(path: &std::path::Path) -> String {
    lushtext_core::services::filesystem::read::text(path).expect("the file")
}

/// Wait for any save the test started to end and the load to settle.
fn wait_quiet(editor: &LushtextEditorPage) {
    wait_until(BUDGET, || {
        !editor.is_saving() && editor.load_state() != EditorLoadState::Loading
    });
}

#[test]
fn test_a_tab_mid_install_is_not_modified_and_closes_without_asking() {
    let (window, dir) = presented();
    let body = chunked_save_document();
    let path = write(&dir, "install-close.txt", &body);
    let editor = open_until_installing(&window, &path);
    assert!(
        !editor.is_modified() && !editor.has_unsaved_changes(),
        "a partial install reported a modification the user never made"
    );
    let page = window.imp().tab_view.selected_page().expect("the tab");
    window.imp().tab_view.close_page(&page);
    assert!(
        alert_dialog(&window).is_none(),
        "closing a loading tab asked to save a partial buffer"
    );
    wait_until(BUDGET, || window.imp().tab_view.n_pages() == 0);
    assert_eq!(bytes(&path), body, "the file changed");
}

#[test]
fn test_the_close_dialog_saves_the_edited_document_and_never_the_installing_one() {
    let (window, dir) = presented();
    crate::common::wait_for_startup_data_flow(&window);
    let edited_path = write(&dir, "edited.txt", "edited\n");
    let edited = opened(&window, &edited_path);
    edited
        .buffer()
        .insert(&mut edited.buffer().start_iter(), "user ");
    assert!(edited.is_modified());
    let body = chunked_save_document();
    let path = write(&dir, "install-window-close.txt", &body);
    let installing = open_until_installing(&window, &path);
    assert!(
        window
            .modified_editors()
            .iter()
            .all(|(_, editor)| editor != &installing),
        "the installing tab was offered for saving"
    );
    window.close();
    let dialog = wait_for_save_changes_dialog(&window);
    choose_save(&dialog);
    wait_until(BUDGET, || bytes(&edited_path) == "user edited\n");
    wait_until(BUDGET, || !window.is_visible());
    assert_eq!(bytes(&path), body, "the installing document's file changed");
}

#[test]
fn test_ctrl_s_during_an_install_is_refused_and_writes_nothing() {
    let (window, dir) = presented();
    let body = chunked_save_document();
    let path = write(&dir, "install-ctrl-s.txt", &body);
    let editor = open_until_installing(&window, &path);
    let refused = std::rc::Rc::new(std::cell::Cell::new(None));
    let result = refused.clone();
    editor.save_file_async(move |outcome| result.set(Some(outcome.is_err())));
    assert_eq!(refused.get(), Some(true), "the save was not refused");
    gtk4::prelude::WidgetExt::activate_action(&window, "win.save", None).expect("win.save");
    wait_quiet(&editor);
    assert_eq!(bytes(&path), body, "Ctrl+S wrote a partial install");
    assert_eq!(editor_text(&editor), body, "the install did not complete");
    assert!(!editor.is_modified());
}

#[test]
fn test_save_as_during_an_install_never_writes_the_partial_buffer() {
    let (window, dir) = presented();
    let body = chunked_save_document();
    // While the file is still read the buffer is empty: a Save As onto the
    // file would truncate it.
    let read_path = write(&dir, "read-save-as.txt", &body);
    window.open_document(&read_path);
    let reading = active_editor(&window);
    assert_eq!(reading.load_state(), EditorLoadState::Loading);
    assert_eq!(reading.buffer().char_count(), 0, "the read phase is over");
    window.select_save_as_destination_for_test(&read_path);
    wait_quiet(&reading);
    assert_eq!(
        bytes(&read_path),
        body,
        "Save As truncated the file it was reading"
    );
    assert_eq!(editor_text(&reading), body, "Save As cancelled the read");
    // Mid-install, onto the file being installed.
    let path = write(&dir, "install-save-as.txt", &body);
    let editor = open_until_installing(&window, &path);
    window.select_save_as_destination_for_test(&path);
    wait_quiet(&editor);
    assert_eq!(bytes(&path), body, "Save As wrote a partial install");
    assert_eq!(
        editor_text(&editor),
        body,
        "Save As cancelled the install it refused"
    );
}

#[test]
fn test_an_eviction_cut_short_never_leaves_an_emptied_buffer_to_save() {
    let (window, dir) = presented();
    let body = chunked_save_document();
    let path = write(&dir, "evict-cancel.txt", &body);
    let editor = opened(&window, &path);
    editor.evict();
    assert!(editor.buffer_replacement_evidence().in_progress);
    assert!(
        !editor.is_modified(),
        "an eviction in progress reported a modification"
    );
    // One clear turn, then anything that advances the page's memory-policy
    // generation (an access, an edit elsewhere) makes the eviction stale.
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + BUDGET;
    while editor.buffer().char_count() == i32::try_from(body.chars().count()).expect("fits") {
        assert!(std::time::Instant::now() < deadline, "no clear turn ran");
        context.iteration(true);
    }
    editor.set_memory_estimate_for_test(None);
    wait_until(BUDGET, || !editor.buffer_replacement_evidence().in_progress);
    assert!(
        editor.is_evicted() || editor_text(&editor) == body,
        "a stale eviction left a partial buffer that is neither the document nor evicted"
    );
    let saved = std::rc::Rc::new(std::cell::Cell::new(None));
    let result = saved.clone();
    editor.save_file_async(move |outcome| result.set(Some(outcome.is_ok())));
    wait_until(BUDGET, || saved.get().is_some() && !editor.is_saving());
    assert_eq!(bytes(&path), body, "a save wrote an emptied buffer");
}

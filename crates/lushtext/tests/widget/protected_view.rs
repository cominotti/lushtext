// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for Protected View (`long-line-protected-view`, C2): the fold
//! at insertion (design D1, task 3.4).
//!
//! Every fixture is synthetic. Thresholds come from
//! `lushtext_core::model::{document_scale, protected_fold}`, never restated.

use crate::common::{
    editor_text, ensure_gtk_init, fixture, flush_events, present_window, test_application,
    wait_until,
};
use gtk4::prelude::*;
use lushtext_core::model::document_scale::L_PROTECT_EDIT;
use lushtext_core::model::paragraph_boundary::DocumentProfile;
use lushtext_core::model::protected_fold::HEAD_CAP;
use lushtext_core::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use std::time::Duration;

/// A policy constant as a `usize` length.
fn n(value: u64) -> usize {
    usize::try_from(value).expect("policy constants fit usize")
}

fn present(page: &LushtextEditorPage) -> gtk4::ApplicationWindow {
    let app = test_application();
    let window = gtk4::ApplicationWindow::builder()
        .application(&app)
        .default_width(1000)
        .default_height(600)
        .child(page)
        .build();
    present_window(&window);
    window
}

fn load_and_wait(page: &LushtextEditorPage, path: &std::path::Path) {
    page.load_file_async(path);
    wait_until(Duration::from_secs(60), || {
        page.load_evidence().load_state == EditorLoadState::Loaded && !page.is_saving()
    });
    flush_events();
}

/// Short lines, then each of `giants` paragraphs, then short lines.
fn body_with_giants(giants: &[usize]) -> String {
    let short: String = (0..30)
        .map(|line| format!("short line {line} of the fixture\n"))
        .collect();
    let mut body = short.clone();
    for (index, &len) in giants.iter().enumerate() {
        let unit = format!("word{index} ");
        let mut giant = unit.repeat(len / unit.len() + 1);
        giant.truncate(len);
        body.push_str(&giant);
        body.push('\n');
        body.push_str(&short);
    }
    body
}

/// Every paragraph's visible character count, in order.
fn visible_lengths(page: &LushtextEditorPage) -> Vec<u64> {
    let buffer = page.buffer();
    (0..buffer.line_count())
        .map(|line| {
            let mut end = buffer.iter_at_line(line).expect("line exists");
            if !end.ends_line() {
                end.forward_to_line_end();
            }
            u64::try_from(end.visible_line_offset()).expect("non-negative")
        })
        .collect()
}

/// The fold the worker plans is what the buffer shows at the install's end,
/// in both install modes, and the buffer still holds every byte.
fn assert_installed_folded(body: &str, name: &str) {
    ensure_gtk_init();
    let profile = DocumentProfile::of_text(body);
    let plan = profile
        .fold_plan
        .as_deref()
        .unwrap_or_else(|| panic!("{name}: the fixture must be protected"))
        .clone();
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join(format!("{name}.txt"));
    fixture::write_text(&path, body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);

    assert_eq!(
        editor_text(&page),
        body,
        "{name}: the buffer holds every byte"
    );
    let visible = visible_lengths(&page);
    for (line, &shown) in visible.iter().enumerate() {
        let line = u64::try_from(line).expect("fits");
        match plan.range_of(line) {
            Some(range) => assert_eq!(shown, range.head_chars, "{name}: line {line} head"),
            None => assert!(shown <= HEAD_CAP, "{name}: line {line} shows {shown}"),
        }
    }
    assert!(plan.folded_paragraphs() >= 1, "{name}: something is folded");
}

#[test]
fn test_protected_load_folds_at_insertion_directly() {
    // Under the synchronous install threshold: one direct install.
    let body = body_with_giants(&[n(L_PROTECT_EDIT) + 1, n(L_PROTECT_EDIT) * 2]);
    assert!(body.len() < 1024 * 1024);
    assert_installed_folded(&body, "direct");
}

#[test]
fn test_protected_load_folds_at_insertion_in_slices() {
    // Over the synchronous threshold: the chunked install, and paragraphs
    // larger than one slice (each installs in one turn).
    let body = body_with_giants(&[700_000, 120_000, 650_000]);
    assert!(body.len() > 1024 * 1024);
    assert_installed_folded(&body, "sliced");
}

#[test]
fn test_protected_fixture_kinds_fold_at_insertion() {
    // design D1, task 3.4: the remaining synthetic fixture kinds. The worker
    // plan's density bound per `R_FOLD` window is Kani-proved (H1); this test
    // proves the buffer shows exactly that plan.
    let giant = "g".repeat(n(L_PROTECT_EDIT) + 1);
    let dense_row = "d".repeat(400);
    let dense: String = (0..300).map(|_| format!("{dense_row}\n")).collect();
    let prose_row = "prose ".repeat(100);
    let prose: String = (0..200).map(|_| format!("{prose_row}\n")).collect();
    assert_installed_folded(&body_with_giants(&[1_900_000]), "1.9M");
    assert_installed_folded(&format!("a\r{giant}\rb\r"), "cr-only");
    assert_installed_folded(&format!("a\u{2029}{giant}\u{2029}b"), "u2029");
    assert_installed_folded(&format!("{dense}{giant}\n{dense}"), "dense");
    assert_installed_folded(&format!("{prose}{giant}\n{prose}"), "prose-600");
}

#[test]
fn test_a_word_wrap_toggle_changes_no_fold() {
    ensure_gtk_init();
    let body = body_with_giants(&[n(L_PROTECT_EDIT) * 3]);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("wrap.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    let before = visible_lengths(&page);
    let view = page.source_view();
    for mode in [gtk4::WrapMode::None, gtk4::WrapMode::WordChar] {
        view.set_wrap_mode(mode);
        flush_events();
        assert_eq!(
            visible_lengths(&page),
            before,
            "{mode:?}: the fold is unchanged"
        );
        assert!(page.is_protected_view());
    }
}

#[test]
fn test_unprotected_load_is_not_folded() {
    ensure_gtk_init();
    let body = body_with_giants(&[n(L_PROTECT_EDIT)]);
    assert_eq!(DocumentProfile::of_text(&body).fold_plan, None);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("editable.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    assert!(
        page.buffer()
            .tag_table()
            .lookup(lushtext_core::ui::protected_fold::FOLD_TAG_NAME)
            .is_none(),
        "an editable document never gets a fold tag"
    );
    let longest = visible_lengths(&page).into_iter().max().unwrap_or(0);
    assert_eq!(longest, L_PROTECT_EDIT);
}

// ─── Edit-growth entry and the entry undo (tasks 3.3, 3.3a) ─────────────────

use lushtext_core::model::protected_fold::N_DENSE;

/// A loaded, editable page holding `body`.
fn editable_page(body: &str, name: &str) -> (LushtextEditorPage, gtk4::ApplicationWindow) {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join(format!("{name}.txt"));
    fixture::write_text(&path, body);
    let page = LushtextEditorPage::new();
    let window = present(&page);
    load_and_wait(&page, &path);
    assert!(!page.is_protected_view(), "{name}: starts editable");
    // The file was read; the directory may go.
    drop(dir);
    (page, window)
}

/// Insert `text` at line `line`'s end as one user action (a paste).
fn paste_at_line_end(page: &LushtextEditorPage, line: i32, text: &str) {
    let buffer = page.buffer();
    let mut iter = buffer.iter_at_line(line).expect("line exists");
    if !iter.ends_line() {
        iter.forward_to_line_end();
    }
    buffer.begin_user_action();
    buffer.insert_interactive(&mut iter, text, true);
    buffer.end_user_action();
    flush_events();
}

fn insert_mark_position(page: &LushtextEditorPage) -> (i32, i32) {
    let buffer = page.buffer();
    let iter = buffer.iter_at_mark(&buffer.get_insert());
    (iter.line(), iter.line_offset())
}

fn assert_entered(page: &LushtextEditorPage, grown_line: i32, name: &str) {
    assert!(page.is_protected_view(), "{name}: entered Protected View");
    let view = page.source_view();
    assert!(!view.is_editable(), "{name}: not editable");
    assert!(!view.is_cursor_visible(), "{name}: no caret");
    assert_eq!(
        insert_mark_position(page),
        (grown_line, 0),
        "{name}: marks at the grown paragraph's start"
    );
    let selection = page.buffer().iter_at_mark(&page.buffer().selection_bound());
    assert_eq!(
        (selection.line(), selection.line_offset()),
        (grown_line, 0),
        "{name}: no selection"
    );
    let visible = visible_lengths(page);
    let shown = visible[usize::try_from(grown_line).expect("non-negative")];
    assert!(
        shown <= HEAD_CAP,
        "{name}: the grown paragraph shows {shown}"
    );
}

#[test]
fn test_paste_over_the_entry_threshold_enters_protected_view() {
    let body = body_with_giants(&[]);
    let (page, _window) = editable_page(&body, "paste");
    let before = editor_text(&page);
    let run = "p".repeat(n(L_PROTECT_EDIT) + 1);
    paste_at_line_end(&page, 5, &run);
    assert_entered(&page, 5, "paste");
    let mut expected = before;
    let line_end = expected
        .match_indices('\n')
        .nth(5)
        .map(|(index, _)| index)
        .expect("line 5 ends");
    expected.insert_str(line_end, &run);
    assert_eq!(editor_text(&page), expected, "the paste is in the buffer");
}

#[test]
fn test_joining_two_paragraphs_over_the_threshold_enters_protected_view() {
    let half = "j".repeat(n(L_PROTECT_EDIT) / 2 + 1);
    let body = format!("short\n{half}\n{half}\nshort\n");
    let (page, _window) = editable_page(&body, "join");
    let buffer = page.buffer();
    let mut start = buffer.iter_at_line(1).expect("line 1");
    start.forward_to_line_end();
    let mut end = start;
    end.forward_char();
    buffer.begin_user_action();
    buffer.delete_interactive(&mut start, &mut end, true);
    buffer.end_user_action();
    flush_events();
    assert_entered(&page, 1, "join");
    assert_eq!(editor_text(&page), format!("short\n{half}{half}\nshort\n"));

    // The entry undo re-splits the joined paragraph and leaves Protected View.
    assert!(page.protected_entry_undo_available());
    assert!(page.undo_protected_entry());
    flush_events();
    assert!(!page.is_protected_view());
    assert!(page.source_view().is_editable());
    assert_eq!(editor_text(&page), body, "the join is undone");
    assert!(
        visible_lengths(&page)
            .iter()
            .all(|&shown| shown < L_PROTECT_EDIT),
        "no fold is left"
    );
}

#[test]
fn test_an_edit_that_creates_a_dense_window_folds_it() {
    // 250 medium lines: any 224 of them hold far more than V_BATCH, so once
    // the paste makes the document protected, the edited window is dense and
    // every paragraph in it shows at most N_DENSE.
    let medium = "m".repeat(400);
    let body: String = (0..250).map(|_| format!("{medium}\n")).collect();
    let (page, _window) = editable_page(&body, "dense");
    paste_at_line_end(&page, 10, &"d".repeat(n(L_PROTECT_EDIT) + 1));
    assert!(page.is_protected_view());
    let visible = visible_lengths(&page);
    for line in [0usize, 5, 9, 11, 20, 100] {
        assert!(
            visible[line] <= N_DENSE,
            "line {line} of the dense edited window shows {}",
            visible[line]
        );
    }
    assert!(
        visible[10] <= N_DENSE,
        "the grown paragraph shows {}",
        visible[10]
    );
}

#[test]
fn test_entry_undo_removes_the_typed_group_and_leaves_protected_view() {
    // Typing crosses the threshold; GtkSourceView groups the typed characters
    // into one undo step, and the entry undo removes the whole group.
    let base = "t".repeat(n(L_PROTECT_EDIT) - 2);
    let body = format!("short\n{base}\nshort\n");
    let (page, _window) = editable_page(&body, "undo");
    let buffer = page.buffer();
    let mut at = buffer.iter_at_line(1).expect("line 1");
    at.forward_to_line_end();
    buffer.place_cursor(&at);
    for typed in ['a', 'b', 'c', 'd', 'e', 'f'] {
        buffer.begin_user_action();
        buffer.insert_interactive_at_cursor(&typed.to_string(), page.source_view().is_editable());
        buffer.end_user_action();
    }
    flush_events();
    assert!(page.is_protected_view(), "typing crossed the threshold");
    assert!(
        page.protected_entry_undo_available(),
        "the entry undo is offered"
    );

    assert!(page.undo_protected_entry(), "the entry undo ran");
    flush_events();
    assert_eq!(editor_text(&page), body, "the whole typed group is gone");
    assert!(!page.is_protected_view(), "editable again");
    assert!(page.source_view().is_editable());
    assert!(page.source_view().is_cursor_visible());
    assert!(
        visible_lengths(&page)
            .iter()
            .all(|&shown| shown < L_PROTECT_EDIT),
        "no fold remains"
    );
    // The entry undo is one-shot.
    assert!(!page.protected_entry_undo_available());
    assert!(!page.undo_protected_entry());
}

#[test]
fn test_entry_undo_waits_while_a_save_holds_the_edit_lock() {
    // Ctrl+Z during the save of an edit-entered session: the edit lock
    // refuses the buffer's undo, and the entry undo used to tear Protected
    // View down anyway, leaving the giant paragraph unfolded and editable.
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("save-undo.txt");
    let body = "short\nline\n";
    fixture::write_text(&path, body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    paste_at_line_end(&page, 0, &"g".repeat(n(L_PROTECT_EDIT) + 1));
    assert!(page.is_protected_view(), "the paste entered Protected View");
    assert!(page.protected_entry_undo_available());
    let grown = editor_text(&page);

    let saved = std::rc::Rc::new(std::cell::Cell::new(None));
    let saved_result = saved.clone();
    page.save_file_async(move |result| saved_result.set(Some(result.is_ok())));
    // One dispatch at a time: a draining wait could run the save's completion.
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !page.edits_locked() {
        assert!(
            saved.get().is_none(),
            "the save finished before it was observed"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "the save never took the lock"
        );
        context.iteration(true);
    }
    assert!(
        !page.protected_entry_undo_available(),
        "no entry undo is offered while the save holds the lock"
    );
    assert!(!page.undo_protected_entry(), "the entry undo refused");
    assert!(page.is_protected_view(), "still protected");
    assert!(!page.source_view().is_editable(), "still caret-free");
    assert_eq!(editor_text(&page), grown, "nothing undone");

    wait_until(Duration::from_secs(10), || saved.get().is_some());
    assert_eq!(saved.get(), Some(true));
    assert!(
        page.protected_entry_undo_available(),
        "offered again after the save"
    );
    assert!(page.undo_protected_entry());
    flush_events();
    assert_eq!(editor_text(&page), body);
    assert!(!page.is_protected_view());
}

// ─── Install entry and the reload exit (tasks 4.3, 3.3a) ───────────────────

#[test]
fn test_loading_a_protected_file_enters_protected_view_without_entry_undo() {
    ensure_gtk_init();
    let body = body_with_giants(&[n(L_PROTECT_EDIT) + 1]);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("installed.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    assert!(page.is_protected_view(), "a protected body enters on load");
    assert!(
        !page.source_view().is_editable(),
        "not editable after the install"
    );
    assert!(
        !page.source_view().is_cursor_visible(),
        "no caret after the install"
    );
    assert!(
        !page.protected_entry_undo_available(),
        "a load entry offers no entry undo"
    );
    assert!(!page.undo_protected_entry());
    assert_eq!(editor_text(&page), body, "nothing changed");
    let evidence = page.protected_view_evidence();
    assert!(evidence.protected && !evidence.entered_by_edit);
    // Every protected document is past C1's thresholds for these features
    // (design D10): the minimap, syntax, invisibles and search highlighting
    // are off for the session.
    let scale = page.document_scale();
    assert!(!scale.minimap_allowed, "no minimap");
    assert!(!scale.syntax_enabled, "no syntax highlighting");
    assert!(!scale.invisibles_allowed, "no invisibles");
    assert!(!scale.search_highlight_allowed, "no search highlighting");
    assert_eq!(
        evidence.source_bytes,
        Some(u64::try_from(body.len()).expect("fits")),
        "the installed body is the session's text source"
    );

    // The file shrinks below the threshold and is reloaded from disk: the
    // reload re-derives, and the document is editable again.
    let small = body_with_giants(&[]);
    fixture::write_text(&path, &small);
    load_and_wait(&page, &path);
    assert!(!page.is_protected_view(), "a reload from disk re-derives");
    assert!(
        page.source_view().is_editable(),
        "editable after the reload"
    );
    assert!(page.source_view().is_cursor_visible(), "the caret is back");
    assert_eq!(editor_text(&page), small);
    assert_eq!(
        page.protected_view_evidence().source_bytes,
        None,
        "an editable document keeps no source"
    );
}

#[test]
fn test_a_reload_leaving_protected_view_restores_editable_before_the_new_body() {
    // `editable` invalidates every line (A37): restored at the install
    // terminal, it re-laid out the whole new body once more.
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("leave.txt");
    fixture::write_text(&path, &body_with_giants(&[n(L_PROTECT_EDIT) + 1]));
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    assert!(page.is_protected_view());

    // Large enough to install in several slices.
    let small = format!("NEWBODY\n{}", crate::common::short_lines('s', 2_000_000));
    fixture::write_text(&path, &small);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let handler = {
        let seen = seen.clone();
        let buffer = page.buffer();
        page.source_view().connect_editable_notify(move |view| {
            let start = buffer.start_iter();
            let mut end = start;
            end.forward_chars(7);
            seen.borrow_mut().push((
                view.is_editable(),
                buffer.text(&start, &end, true).to_string(),
            ));
        })
    };
    load_and_wait(&page, &path);
    page.source_view().disconnect(handler);
    assert!(!page.is_protected_view());
    assert!(page.source_view().is_editable());
    assert_eq!(
        seen.borrow().len(),
        1,
        "editable changed once: {:?}",
        seen.borrow()
    );
    assert!(
        seen.borrow().iter().all(|(_, head)| head != "NEWBODY"),
        "editable changed after the new body was installed: {:?}",
        seen.borrow()
    );
}

// ─── Caret-free interaction (tasks 4.3, 4.7) ───────────────────────────────

/// A loaded protected page whose short lines hold a number at line 2.
fn protected_page_with_number() -> (LushtextEditorPage, gtk4::ApplicationWindow, String) {
    ensure_gtk_init();
    let mut body = String::from("short\nshort\nvalue 123 here\n");
    body.push_str(&body_with_giants(&[n(L_PROTECT_EDIT) + 1]));
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("interaction.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let window = present(&page);
    load_and_wait(&page, &path);
    assert!(page.is_protected_view());
    (page, window, body)
}

fn marks(page: &LushtextEditorPage) -> (i32, i32) {
    let buffer = page.buffer();
    (
        buffer.iter_at_mark(&buffer.get_insert()).offset(),
        buffer.iter_at_mark(&buffer.selection_bound()).offset(),
    )
}

#[test]
fn test_mutating_keybinding_signals_leave_a_protected_document_unchanged() {
    let (page, _window, body) = protected_page_with_number();
    let buffer = page.buffer();
    let view = page.source_view().clone();
    // Put the marks on the number, as an assistive technology could.
    let mut at_number = buffer.iter_at_line(2).expect("line 2");
    at_number.forward_chars(7);
    buffer.place_cursor(&at_number);
    let before = marks(&page);

    view.emit_by_name::<()>("select-all", &[&true]);
    view.emit_by_name::<()>("paste-clipboard", &[]);
    view.emit_by_name::<()>("cut-clipboard", &[]);
    view.emit_by_name::<()>("backspace", &[]);
    view.emit_by_name::<()>("delete-from-cursor", &[&gtk4::DeleteType::Chars, &1i32]);
    view.emit_by_name::<()>("insert-at-cursor", &[&"x"]);
    view.emit_by_name::<()>("change-number", &[&1i32]);
    view.emit_by_name::<()>("change-case", &[&sourceview5::ChangeCaseType::Upper]);
    view.emit_by_name::<()>("join-lines", &[]);
    view.emit_by_name::<()>("toggle-cursor-visible", &[]);
    flush_events();

    assert_eq!(editor_text(&page), body, "the buffer is unchanged");
    assert_eq!(marks(&page), before, "no mark moved");
    assert!(!buffer.is_modified(), "still unmodified");
    assert!(!view.is_cursor_visible(), "still caret-free");
}

#[test]
fn test_move_cursor_scrolls_without_moving_a_mark() {
    let (page, _window, _body) = protected_page_with_number();
    let view = page.source_view().clone();
    wait_until(Duration::from_secs(10), || {
        view.vadjustment()
            .is_some_and(|adjustment| adjustment.upper() > adjustment.page_size())
    });
    let vadjustment = view.vadjustment().expect("vertical adjustment");
    let before = marks(&page);
    let top = vadjustment.value();
    view.emit_by_name::<()>("move-cursor", &[&gtk4::MovementStep::Pages, &1i32, &false]);
    flush_events();
    assert_eq!(marks(&page), before, "the marks stay");
    assert!(vadjustment.value() > top, "the viewport moved down a page");
}

// ─── Recovery of a giant-paragraph draft (tasks 7.10, design D13) ──────────

/// Seed a draft of `draft` for a file holding "small\n", restore it at startup,
/// and return the window, the recovered editor, the file and its directory.
fn recover_draft(
    draft: &str,
) -> (
    lushtext_core::ui::window::LushtextWindow,
    LushtextEditorPage,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    use crate::common::{active_editor, test_window};
    use lushtext_core::model::draft::{DraftEntry, DraftManifest};
    use lushtext_core::model::session::{SessionData, SessionTab};
    use lushtext_core::services::{draft_service, editor_io, json_store, session_service};

    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let file_path = dir.path().join("recovered.txt");
    fixture::write_text(&file_path, "small\n");
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&file_path);
    draft_service::fixture::write_body(&data_dir, &draft_id, draft).expect("seed draft");
    let mtime = editor_io::mtime_secs(&file_path).expect("file mtime");
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id,
                original_path: Some(file_path.clone()),
                original_mtime_secs: Some(mtime),
                saved_at_secs: 1,
            }],
            cleanup_continuation: None,
        },
    )
    .expect("save manifest");
    session_service::save(
        &data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(file_path.clone()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let draft_chars = draft.chars().count();
    let window = test_window();
    present_window(&window);
    // The recovered body, plus the stand-in terminator after an undelimited
    // folded last paragraph (`ui::protected_fold`, A79), once the bounded
    // replacement has finished.
    wait_until(Duration::from_secs(60), || {
        let editor = active_editor(&window);
        !editor.buffer_replacement_evidence().in_progress
            && usize::try_from(editor.buffer().char_count())
                .is_ok_and(|chars| chars == draft_chars || chars == draft_chars + 1)
    });
    flush_events();
    let editor = active_editor(&window);
    (window, editor, file_path, dir)
}

#[test]
fn test_recovering_a_giant_paragraph_draft_folds_it_and_keeps_it_as_the_source() {
    // The crash-recovery smoke's shape: a body whose last paragraph is
    // several million characters with no delimiter after it.
    let draft = format!("{}{}", body_with_giants(&[]), "g".repeat(3_000_000));
    let (window, editor, _path, _dir) = recover_draft(&draft);
    assert_eq!(
        editor_text(&editor),
        draft,
        "the draft is recovered byte for byte"
    );
    assert!(
        editor.is_protected_view(),
        "a giant recovered paragraph enters Protected View"
    );
    assert!(
        editor.buffer().is_modified(),
        "a recovered draft stays modified"
    );
    assert!(!editor.source_view().is_editable(), "and is caret-free");
    let last = visible_lengths(&editor).last().copied().expect("lines");
    assert!(
        last <= HEAD_CAP,
        "the recovered giant paragraph shows {last}, at most HEAD_CAP"
    );
    assert_eq!(
        editor.protected_view_evidence().source_bytes,
        Some(u64::try_from(draft.len()).expect("fits")),
        "the recovered body is the session's text source"
    );

    // Autosave neither snapshots nor rewrites it: the buffer equals the
    // persisted body, so the editor is not draft-dirty and a tick writes
    // nothing (design D13).
    assert!(
        !editor.draft_dirty(),
        "a recovered body is already persisted"
    );
    window.autosave_tick_for_test();
    wait_until(Duration::from_secs(30), || {
        let evidence = window.draft_evidence();
        !evidence.autosave_inflight && !evidence.mutation_inflight
    });
    assert!(!editor.draft_dirty());
    let data_dir = lushtext_core::services::json_store::data_dir();
    let draft_id = editor.draft_id().expect("draft id");
    assert_eq!(
        lushtext_core::services::draft_service::read_draft(&data_dir, &draft_id)
            .expect("read draft")
            .as_deref(),
        Some(draft.as_str()),
        "the persisted body is unchanged"
    );
}

#[test]
fn test_saving_a_recovered_protected_draft_writes_its_source_verbatim() {
    // Mixed line endings: the ordinary save refuses them; a protected save
    // writes the source exactly (design D9).
    let draft = format!(
        "crlf line\r\nlf line\n{}\r\nlast\n",
        "s".repeat(n(L_PROTECT_EDIT) + 10)
    );
    let (_window, editor, path, _dir) = recover_draft(&draft);
    assert!(editor.is_protected_view());
    let saved = std::rc::Rc::new(std::cell::Cell::new(None));
    let saved_result = std::rc::Rc::clone(&saved);
    editor.save_file_async(move |result| {
        saved_result.set(Some(result.is_ok()));
    });
    wait_until(Duration::from_secs(30), || saved.get().is_some());
    assert_eq!(saved.get(), Some(true), "the protected save succeeds");
    assert_eq!(
        fixture::read_bytes(&path),
        draft.as_bytes(),
        "the file holds the recovered source byte for byte, mixed endings included"
    );
    assert!(
        !editor.buffer().is_modified(),
        "the saved document is clean"
    );
    assert!(editor.is_protected_view(), "and stays protected");
}

// ─── Clipboard routes (task 4.7a, C6 review finding) ───────────────────────

/// Read the view's clipboard as text, pumping the main loop.
fn clipboard_text(page: &LushtextEditorPage) -> Option<String> {
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot = std::rc::Rc::clone(&result);
    page.source_view()
        .clipboard()
        .read_text_async(None::<&gio::Cancellable>, move |text| {
            slot.replace(Some(text.ok().flatten().map(|text| text.to_string())));
        });
    wait_until(Duration::from_secs(30), || result.borrow().is_some());
    result.take().flatten()
}

#[test]
fn test_copy_across_a_folded_paragraph_keeps_the_hidden_tail() {
    let (page, _window, body) = protected_page_with_number();
    let buffer = page.buffer();
    // Clear the clipboard so a stale value cannot pass the test.
    page.source_view().clipboard().set_text("");
    // A selection across the folded paragraph, as an assistive technology
    // could set it: from line 2 to the end of the document.
    let start = buffer.iter_at_line(2).expect("line 2");
    let end = buffer.end_iter();
    buffer.select_range(&start, &end);
    let expected: String = body
        .chars()
        .skip(usize::try_from(start.offset()).expect("offset"))
        .collect();
    // The primary selection does not offer the folded range: the buffer's
    // primary clipboard is off while protected.
    let primary = std::rc::Rc::new(std::cell::RefCell::new(None));
    let primary_slot = std::rc::Rc::clone(&primary);
    page.source_view().primary_clipboard().read_text_async(
        None::<&gio::Cancellable>,
        move |text| {
            primary_slot.replace(Some(text.ok().flatten().map(|text| text.to_string())));
        },
    );
    wait_until(Duration::from_secs(30), || primary.borrow().is_some());
    assert!(
        primary.take().flatten().is_none_or(|text| text.is_empty()),
        "the primary selection offers nothing of a protected document"
    );
    page.source_view().emit_by_name::<()>("copy-clipboard", &[]);
    wait_until(Duration::from_secs(30), || {
        clipboard_text(&page).is_some_and(|text| !text.is_empty())
    });
    let copied = clipboard_text(&page).expect("the clipboard holds text");
    assert_eq!(
        copied.len(),
        expected.len(),
        "the copy holds every character, hidden tails included"
    );
    assert_eq!(copied, expected);

    // Cut is refused: the buffer is unchanged.
    page.source_view().emit_by_name::<()>("cut-clipboard", &[]);
    flush_events();
    assert_eq!(editor_text(&page), body);
}

// ─── Worker find (tasks 6.1, 6.2, 6.6) ─────────────────────────────────────

#[test]
fn test_protected_find_runs_on_the_source_and_finds_hidden_text() {
    ensure_gtk_init();
    // A marker deep inside a giant paragraph's hidden tail, and one in a
    // short visible line.
    let giant = format!(
        "{}zqzhidden{}",
        "h".repeat(n(L_PROTECT_EDIT)),
        "t".repeat(1_000)
    );
    let body = format!("short zqzhidden line\n{giant}\nshort\n");
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("find.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    assert!(page.is_protected_view());

    page.show_search();
    flush_events();
    assert!(
        page.search_bar().search_context().is_none(),
        "no GtkSourceSearchContext is attached in Protected View"
    );
    page.search_bar().search_entry().set_text("zqzhidden");
    wait_until(Duration::from_secs(30), || {
        page.protected_view_evidence().find_matches == 2
    });
    assert_eq!(page.protected_view_evidence().find_current, Some(0));
    assert!(
        !page.protected_view_evidence().find_pending,
        "the request finished"
    );
    page.search_bar().move_next();
    assert_eq!(
        page.protected_view_evidence().find_current,
        Some(1),
        "the second match lies in the hidden tail and is reachable"
    );
    page.search_bar().move_next();
    assert_eq!(
        page.protected_view_evidence().find_current,
        Some(0),
        "navigation wraps"
    );
    page.search_bar().move_prev();
    assert_eq!(page.protected_view_evidence().find_current, Some(1));
    assert_eq!(editor_text(&page), body, "find changed nothing");

    // Leaving Protected View (a reload of a small file) gives find back to
    // the search context.
    fixture::write_text(&path, "small zqzhidden\n");
    load_and_wait(&page, &path);
    assert!(!page.is_protected_view());
    assert!(
        page.search_bar().search_context().is_some(),
        "the editable document's find attaches its search context again"
    );
}

#[test]
fn test_an_edit_entered_session_finds_and_copies_its_hidden_tail() {
    // An edit-growth session has no install body to keep as its source: it
    // captures the buffer after the entry, and find and copy wait for it.
    let body = body_with_giants(&[]);
    let (page, _window) = editable_page(&body, "edit-source");
    // Several capture chunks deep, with a multibyte tail.
    let run = format!(
        "{}zqzgrown{}",
        "p".repeat(n(L_PROTECT_EDIT) * 4),
        "é".repeat(80_000)
    );
    paste_at_line_end(&page, 5, &run);
    assert_entered(&page, 5, "edit-source");
    let text = editor_text(&page);
    // Mark and tag changes between capture dispatches (the view moves its own
    // marks when it scrolls) make the capture re-position by byte index.
    let buffer = page.buffer();
    let probe = buffer.create_mark(None, &buffer.start_iter(), true);
    let probe_tag = buffer.create_tag(None, &[]).expect("tag");
    let flip = std::cell::Cell::new(false);
    wait_until(Duration::from_secs(60), || {
        flip.set(!flip.get());
        let line = if flip.get() { 0 } else { 2 };
        buffer.move_mark(&probe, &buffer.iter_at_line(line).expect("line"));
        let (start, _) = buffer.bounds();
        let mut end = start;
        end.forward_char();
        buffer.apply_tag(&probe_tag, &start, &end);
        page.protected_view_evidence().source_bytes == Some(u64::try_from(text.len()).expect("len"))
    });
    buffer.remove_tag(&probe_tag, &buffer.start_iter(), &buffer.end_iter());
    buffer.delete_mark(&probe);

    page.show_search();
    flush_events();
    page.search_bar().search_entry().set_text("zqzgrown");
    wait_until(Duration::from_secs(30), || {
        !page.protected_view_evidence().find_pending
            && page.protected_view_evidence().find_matches == 1
    });

    page.source_view().clipboard().set_text("");
    let buffer = page.buffer();
    let (start, end) = buffer.bounds();
    buffer.select_range(&start, &end);
    page.source_view().emit_by_name::<()>("copy-clipboard", &[]);
    wait_until(Duration::from_secs(30), || {
        clipboard_text(&page).is_some_and(|copied| !copied.is_empty())
    });
    assert_eq!(
        clipboard_text(&page).expect("the clipboard holds text"),
        text,
        "the copy holds the grown paragraph's hidden tail"
    );

    // The entry undo drops the captured source.
    assert!(page.undo_protected_entry());
    assert_eq!(page.protected_view_evidence().source_bytes, None);
}

#[test]
fn test_eviction_releases_the_source_and_the_reload_retains_a_new_one() {
    ensure_gtk_init();
    // A tail over one protected slice, so the install leaves boundaries.
    let body = body_with_giants(&[1_000_000]);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("evict.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    let before = page.protected_view_evidence();
    assert!(before.protected);
    assert_eq!(
        before.source_bytes,
        Some(u64::try_from(body.len()).expect("len"))
    );
    let chars = u64::try_from(page.buffer().char_count()).expect("count");
    assert_eq!(
        page.estimated_live_buffer_bytes(),
        lushtext_core::model::editor_memory::estimate_live_editor_bytes(
            chars,
            page.file_size(),
            false
        ) + before.source_bytes.expect("source")
            + lushtext_core::model::editor_memory::protected_boundary_bytes(
                page.load_evidence().protected.boundaries_live
            ),
        "the source and the boundary marks are charged to the live-editor estimate"
    );
    assert!(
        page.load_evidence().protected.boundaries_live > 0,
        "the body was installed sliced"
    );

    page.evict();
    wait_until(Duration::from_secs(30), || page.is_evicted());
    let evicted = page.protected_view_evidence();
    assert!(
        evicted.protected,
        "an evicted protected tab stays protected"
    );
    assert_eq!(
        evicted.source_bytes, None,
        "eviction releases the source with the buffer"
    );

    load_and_wait(&page, &path);
    let reloaded = page.protected_view_evidence();
    assert!(reloaded.protected);
    assert_eq!(
        reloaded.source_bytes, before.source_bytes,
        "the reload retains its body"
    );
    assert_eq!(reloaded.folded_paragraphs, before.folded_paragraphs);
    assert_eq!(reloaded.hidden_chars, before.hidden_chars);
    assert_eq!(editor_text(&page), body);
}

#[test]
fn test_save_as_of_protected_fixtures_is_byte_identical() {
    // design D9: every protected save writes the source verbatim, with no
    // formatting (an `.editorconfig` asks for trimming and a final newline)
    // and no line-ending normalization; the source equals the buffer's full
    // text, hidden tails included.
    ensure_gtk_init();
    let giant = "g ".repeat(n(L_PROTECT_EDIT));
    let huge = "h".repeat(1_900_000);
    let fixtures: Vec<(&str, Vec<u8>)> = vec![
        ("lf", format!("a  \n{giant}\nend  ").into_bytes()),
        (
            "bom",
            [
                &[0xEF, 0xBB, 0xBF][..],
                format!("a\n{giant}\nb\n").as_bytes(),
            ]
            .concat(),
        ),
        ("crlf", format!("a  \r\n{giant}\r\nb\r\n").into_bytes()),
        ("cr", format!("a\r{giant}\rb\r").into_bytes()),
        ("mixed", format!("a\r\n{giant}\nb\rc").into_bytes()),
        ("u2029", format!("a\u{2029}{giant}\u{2029}b").into_bytes()),
        ("huge", format!("a\n{huge}\nb\n").into_bytes()),
    ];
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    fixture::write_text(
        &dir.path().join(".editorconfig"),
        "root = true\n[*]\ntrim_trailing_whitespace = true\ninsert_final_newline = true\nend_of_line = lf\n",
    );
    let window = crate::common::test_window();
    present_window(&window);
    for (name, bytes) in fixtures {
        let path = dir.path().join(format!("{name}.txt"));
        fixture::write_bytes(&path, &bytes);
        window.open_document(&path);
        let page = crate::common::active_editor(&window);
        wait_until(Duration::from_secs(60), || {
            page.load_evidence().load_state == EditorLoadState::Loaded
        });
        flush_events();
        assert!(page.is_protected_view(), "{name}: protected");
        let text = editor_text(&page);
        assert_eq!(
            page.protected_view_evidence().source_bytes,
            Some(u64::try_from(text.len()).expect("len")),
            "{name}: the source is the buffer's full text"
        );
        let copy = dir.path().join(format!("{name}-copy.txt"));
        window.select_save_as_destination_for_test(&copy);
        wait_until(Duration::from_secs(60), || {
            page.file_path().as_deref() == Some(copy.as_path()) && !page.is_saving()
        });
        assert!(
            fixture::read_bytes(&copy) == bytes,
            "{name}: Save As writes the file's bytes unchanged"
        );
        assert_eq!(editor_text(&page), text, "{name}: no mirror-back");
    }
}

#[test]
fn test_print_is_refused_for_a_protected_document() {
    use lushtext_core::ui::window::{
        PrintOutcome, notification_evidence, with_print_runner_for_test,
    };
    ensure_gtk_init();
    let body = body_with_giants(&[n(L_PROTECT_EDIT) + 1]);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("print.txt");
    fixture::write_text(&path, &body);
    let window = crate::common::test_window();
    present_window(&window);
    window.open_document(&path);
    let page = crate::common::active_editor(&window);
    wait_until(Duration::from_secs(60), || {
        page.load_evidence().load_state == EditorLoadState::Loaded
    });
    flush_events();
    assert!(page.is_protected_view());
    let before = notification_evidence(&window).announcements.status_lane;
    let ran = std::rc::Rc::new(std::cell::Cell::new(false));
    let ran_in_runner = std::rc::Rc::clone(&ran);
    with_print_runner_for_test(
        move |_| {
            ran_in_runner.set(true);
            PrintOutcome::Completed
        },
        || crate::common::activate_action(&window, "print"),
    );
    flush_events();
    assert!(!ran.get(), "no print operation starts");
    assert_eq!(
        notification_evidence(&window).announcements.status_lane,
        before + 1,
        "the refusal is announced once in the status lane"
    );
}

#[test]
fn test_a_protected_reload_reruns_find_on_the_new_source() {
    // A reload that stays protected replaces the source: the results of the
    // previous source must not survive it (the session generation moved).
    ensure_gtk_init();
    let giant = "h".repeat(n(L_PROTECT_EDIT) + 1);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("refind.txt");
    fixture::write_text(&path, &format!("zqz one\n{giant}\nzqz two\n"));
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    page.show_search();
    flush_events();
    page.search_bar().search_entry().set_text("zqz");
    wait_until(Duration::from_secs(30), || {
        page.protected_view_evidence().find_matches == 2
    });

    fixture::write_text(&path, &format!("{giant}\nzqz only\n"));
    load_and_wait(&page, &path);
    assert!(page.is_protected_view());
    wait_until(Duration::from_secs(30), || {
        let evidence = page.protected_view_evidence();
        !evidence.find_pending && evidence.find_matches == 1
    });
    assert_eq!(page.protected_view_evidence().find_current, Some(0));
}

#[test]
fn test_the_automation_snapshot_counts_protected_find_matches() {
    // `search.editor_match_count` must report the worker find's count: a
    // protected document attaches no search context, and automation clients
    // (the responsiveness lane's keystroke group) read the count there.
    ensure_gtk_init();
    let giant = format!(
        "{}zqzsnap{}",
        "h".repeat(n(L_PROTECT_EDIT)),
        "t".repeat(500)
    );
    let body = format!("zqzsnap\n{giant}\nshort\n");
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("snapshot-find.txt");
    fixture::write_text(&path, &body);
    let window = crate::common::test_window();
    present_window(&window);
    window.open_document(&path);
    let page = crate::common::active_editor(&window);
    wait_until(Duration::from_secs(60), || {
        page.load_evidence().load_state == EditorLoadState::Loaded
    });
    assert!(page.is_protected_view());
    page.show_search();
    flush_events();
    page.search_bar().search_entry().set_text("zqzsnap");
    wait_until(Duration::from_secs(30), || {
        let evidence = page.protected_view_evidence();
        !evidence.find_pending && evidence.find_matches == 2
    });
    let app = window
        .application()
        .expect("window application")
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("LushtextApplication");
    let snapshot = lushtext_core::ui::automation::app_snapshot(&app);
    let search = snapshot.window.expect("window snapshot").search;
    assert_eq!(search.editor_match_count, Some(2));
}

// ─── Sliced protected install (`sliced-protected-install`, C3) ─────────────

use lushtext_core::model::buffer_replacement::SliceBudget;
use lushtext_core::model::protected_install::{
    PRODUCTION_LIMITS, PROTECTED_CLEAR_SLICE_BYTES, PROTECTED_TAIL_SLICE_BYTES,
    ProtectedInstallCursor, SliceKind, plan_protected_install,
};

/// The hidden-tail slices the planner gives `body`, and the boundary marks
/// the executor leaves: one per slice plus one paragraph-start mark, for
/// every tail that needs more than one slice (a one-slice tail goes in with
/// its head, unmarked).
fn expected_slices_and_boundaries(
    body: &str,
    plan: &lushtext_core::model::protected_fold::FoldPlan,
) -> (u64, u64) {
    let mut cursor = ProtectedInstallCursor::new();
    let (mut slices, mut boundaries, mut run) = (0, 0, 0);
    while let Some(slice) = plan_protected_install(
        body,
        plan.ranges(),
        &mut cursor,
        SliceBudget::BYTES_ONLY,
        PRODUCTION_LIMITS,
    ) {
        if slice.kind == SliceKind::HiddenTail {
            run += 1;
            if slice.last_of_tail {
                slices += run;
                if run > 1 {
                    boundaries += run + 1;
                }
                run = 0;
            }
        }
    }
    (slices, boundaries)
}

/// The largest insertion and deletion `page`'s buffer sees, in bytes and
/// characters, recorded from the signals' before-handlers.
#[derive(Default)]
struct BufferRuns {
    largest_insert_bytes: std::cell::Cell<usize>,
    largest_delete_chars: std::cell::Cell<i32>,
}

fn watch_buffer_runs(page: &LushtextEditorPage) -> std::rc::Rc<BufferRuns> {
    let runs = std::rc::Rc::new(BufferRuns::default());
    let buffer = page.buffer();
    let inserted = std::rc::Rc::clone(&runs);
    buffer.connect_insert_text(move |_, _, text| {
        let largest = &inserted.largest_insert_bytes;
        largest.set(largest.get().max(text.len()));
    });
    let deleted = std::rc::Rc::clone(&runs);
    buffer.connect_delete_range(move |_, start, end| {
        let largest = &deleted.largest_delete_chars;
        largest.set(largest.get().max(end.offset() - start.offset()));
    });
    runs
}

fn usize_of(value: u64) -> usize {
    usize::try_from(value).expect("fits usize")
}

#[test]
fn test_a_protected_tail_goes_in_and_out_in_bounded_slices() {
    // Failing first on C2's one-turn executor: its 3,000,000-character tail
    // went in with one `insert_with_tags`, and a reload's clear deleted the
    // whole paragraph in one turn (its head's clear slice ran to the line end).
    ensure_gtk_init();
    let body = body_with_giants(&[3_000_000]);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("sliced.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    let runs = watch_buffer_runs(&page);
    load_and_wait(&page, &path);
    assert!(page.is_protected_view());
    assert_eq!(editor_text(&page), body);
    assert!(
        runs.largest_insert_bytes.get() <= PROTECTED_TAIL_SLICE_BYTES,
        "the largest insertion was {} bytes",
        runs.largest_insert_bytes.get()
    );

    load_and_wait(&page, &path);
    assert_eq!(editor_text(&page), body, "the reload is byte-identical");
    let clear_bound = PROTECTED_CLEAR_SLICE_BYTES + usize_of(HEAD_CAP);
    assert!(
        usize::try_from(runs.largest_delete_chars.get()).expect("non-negative") <= clear_bound,
        "the largest deletion was {} characters",
        runs.largest_delete_chars.get()
    );
}

/// The fold tag's ranges on `line`, as character columns.
fn fold_tag_runs(page: &LushtextEditorPage, line: i32) -> Vec<(i32, i32)> {
    let buffer = page.buffer();
    let tag = buffer
        .tag_table()
        .lookup(lushtext_core::ui::protected_fold::FOLD_TAG_NAME)
        .expect("the fold tag exists");
    let mut iter = buffer.iter_at_line(line).expect("line exists");
    let mut end = iter;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    let mut runs = Vec::new();
    let mut run_start = iter.has_tag(&tag).then_some(iter.line_offset());
    while iter < end && iter.forward_to_tag_toggle(Some(&tag)) {
        if iter > end {
            break;
        }
        match run_start.take() {
            Some(start) => runs.push((start, iter.line_offset())),
            None => run_start = Some(iter.line_offset()),
        }
    }
    if let Some(start) = run_start {
        runs.push((start, end.line_offset()));
    }
    runs
}

/// Install `body` through the sliced protected path and check byte identity,
/// the fold, the slice and boundary counts, one tag range per tail, and that
/// no protected paragraph showed more than its head at any main-loop turn.
fn assert_sliced_install(body: &str, name: &str) -> lushtext_core::ui::editor_page::LoadEvidence {
    ensure_gtk_init();
    let profile = DocumentProfile::of_text(body);
    let plan = profile
        .fold_plan
        .as_deref()
        .unwrap_or_else(|| panic!("{name}: the fixture must be protected"))
        .clone();
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join(format!("{name}.txt"));
    fixture::write_text(&path, body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    // Between the install's turns, every folded paragraph shows at most its
    // head: the tail is never laid out untagged across a dispatch (D2).
    let overshow = std::rc::Rc::new(std::cell::Cell::new(None));
    let watcher = {
        let (page, plan, overshow) = (page.clone(), plan.clone(), std::rc::Rc::clone(&overshow));
        glib::timeout_add_local(Duration::from_millis(1), move || {
            let lines = visible_lengths(&page);
            for range in plan.ranges() {
                if let Some(&shown) = lines.get(usize_of(range.paragraph))
                    && shown > range.head_chars
                {
                    overshow.set(Some((range.paragraph, shown)));
                }
            }
            glib::ControlFlow::Continue
        })
    };
    load_and_wait(&page, &path);
    watcher.remove();
    assert_eq!(
        overshow.get(),
        None,
        "{name}: a paragraph showed past its head"
    );
    assert_eq!(editor_text(&page), body, "{name}: byte identity");
    let visible = visible_lengths(&page);
    for range in plan.ranges() {
        let line = i32::try_from(range.paragraph).expect("fits");
        assert_eq!(
            visible[usize_of(range.paragraph)],
            range.head_chars,
            "{name}: line {line} head"
        );
        let head = i32::try_from(range.head_chars).expect("fits");
        let tail = i32::try_from(range.tail_chars).expect("fits");
        assert_eq!(
            fold_tag_runs(&page, line),
            vec![(head, head + tail)],
            "{name}: line {line}'s tail is one fold-tag range"
        );
    }
    let evidence = page.load_evidence();
    let protected = evidence.protected;
    assert!(evidence.installation_chunked, "{name}: sliced");
    let (slices, boundaries) = expected_slices_and_boundaries(body, &plan);
    assert_eq!(
        protected.tail_slices, slices,
        "{name}: every planned tail slice"
    );
    assert!(
        usize_of(protected.largest_tail_slice_bytes) <= PROTECTED_TAIL_SLICE_BYTES,
        "{name}: {protected:?}"
    );
    assert_eq!(
        protected.boundaries_live, boundaries,
        "{name}: one boundary per slice plus one per paragraph start, for multi-slice tails"
    );
    // The install never ran the edit-time entry (C1's watch, C2's growth).
    let view = page.protected_view_evidence();
    assert!(view.protected && !view.entered_by_edit, "{name}: {view:?}");
    assert!(!view.entry_undo_available, "{name}");
    assert!(!page.buffer().is_modified(), "{name}: unmodified");
    evidence
}

#[test]
fn test_sliced_protected_installs_are_byte_identical_and_folded() {
    let giant = |len: usize| "g".repeat(len);
    let head = usize_of(HEAD_CAP);
    let budget = PROTECTED_TAIL_SLICE_BYTES;
    assert_sliced_install(&format!("a\r{}\rb\r", giant(600_000)), "cr-only");
    assert_sliced_install(&format!("a\u{2029}{}\u{2029}b", giant(600_000)), "u2029");
    // A `\r\n` right at the end of a tail slice's window, and one byte inside.
    assert_sliced_install(
        &format!("x\n{}\r\ny\n", giant(head + budget)),
        "crlf-at-edge",
    );
    assert_sliced_install(
        &format!("x\n{}\r\ny\n", giant(head + budget - 1)),
        "crlf-inside",
    );
    // Multibyte scalars at every byte phase of the slice ends.
    for phase in 0..4 {
        let tail = "é€𝄞a".repeat(70_000);
        assert_sliced_install(&format!("{}{tail}\nend", "p".repeat(phase)), "multibyte");
    }
    // Several protected paragraphs among short ones, the last unterminated.
    let mixed = format!(
        "{}{}",
        body_with_giants(&[700_000, 120_000, 650_000]),
        giant(400_000)
    );
    assert_sliced_install(&mixed, "mixed");
}

/// Check that a cancelled load left nothing partial: an empty, unsaveable
/// buffer with no boundary left.
fn cancel_and_check(page: &LushtextEditorPage, name: &str) {
    let evidence = page.load_evidence();
    assert_eq!(
        page.buffer().char_count(),
        0,
        "{name}: nothing partial stays"
    );
    assert!(
        evidence.installation_incomplete,
        "{name}: saving stays blocked"
    );
    assert_eq!(
        evidence.protected.boundaries_live, 0,
        "{name}: no boundary left"
    );
}

/// Cancel `page`'s load from a high-priority idle, so between two of its
/// turns, once `signal` has been emitted `count` times on its buffer
/// (`apply-tag` once per hidden-tail slice, `delete-range` once per clear
/// turn). Returns the handler to disconnect.
fn cancel_after(
    page: &LushtextEditorPage,
    signal: &'static str,
    count: u32,
) -> glib::SignalHandlerId {
    let seen = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let page_weak = page.downgrade();
    page.buffer().connect_local(signal, false, move |values| {
        // `apply-tag` counts the fold tag only (C5's wrap gate applies its
        // own tag in the same emissions).
        let other_tag = values
            .get(1)
            .and_then(|value| value.get::<gtk4::TextTag>().ok())
            .is_some_and(|tag| {
                tag.name().as_deref() != Some(lushtext_core::ui::protected_fold::FOLD_TAG_NAME)
            });
        if other_tag {
            return None;
        }
        seen.set(seen.get() + 1);
        if seen.get() == count {
            let page_weak = page_weak.clone();
            glib::idle_add_local_full(glib::Priority::HIGH, move || {
                if let Some(page) = page_weak.upgrade() {
                    page.cancel_load();
                }
                glib::ControlFlow::Break
            });
        }
        None
    })
}

#[test]
fn test_cancelling_a_sliced_protected_install_leaves_nothing_saveable() {
    ensure_gtk_init();
    let body = body_with_giants(&[3_000_000, 2_000_000]);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("cancel.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    for (slices, name) in [
        (1, "after the first tail slice"),
        (3, "between tail slices"),
    ] {
        let handler = cancel_after(&page, "apply-tag", slices);
        page.load_file_async(&path);
        wait_until(Duration::from_secs(60), || {
            let evidence = page.load_evidence();
            !evidence.installation_active && evidence.installation_incomplete
        });
        page.buffer().disconnect(handler);
        flush_events();
        let evidence = page.load_evidence();
        assert!(
            evidence.protected.tail_slices >= u64::from(slices),
            "{name}: {:?}",
            evidence.protected
        );
        cancel_and_check(&page, name);
        load_and_wait(&page, &path);
        assert_eq!(
            editor_text(&page),
            body,
            "{name}: the re-install is byte-identical"
        );
    }
    // While a reload clears the protected buffer.
    let handler = cancel_after(&page, "delete-range", 2);
    page.load_file_async(&path);
    wait_until(Duration::from_secs(60), || {
        let evidence = page.load_evidence();
        !evidence.installation_active && evidence.installation_incomplete
    });
    page.buffer().disconnect(handler);
    cancel_and_check(&page, "while clearing");
    load_and_wait(&page, &path);
    assert_eq!(editor_text(&page), body, "after a cancelled clear");
}

#[test]
fn test_no_boundary_mark_outlives_eviction_reload_or_replacement() {
    ensure_gtk_init();
    let body = body_with_giants(&[1_000_000]);
    let dir = tempfile::tempdir().expect("protected-view tempdir");
    let path = dir.path().join("marks.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    let installed = page.load_evidence().protected.boundaries_live;
    assert!(
        installed > 2,
        "a sliced tail leaves boundaries: {installed}"
    );

    page.evict();
    wait_until(Duration::from_secs(30), || page.is_evicted());
    assert_eq!(
        page.load_evidence().protected.boundaries_live,
        0,
        "eviction"
    );

    load_and_wait(&page, &path);
    assert_eq!(
        page.load_evidence().protected.boundaries_live,
        installed,
        "a reload leaves exactly its own install's boundaries"
    );
    load_and_wait(&page, &path);
    assert_eq!(page.load_evidence().protected.boundaries_live, installed);

    let outcomes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    page.replace_buffer_for_test(
        "plain replacement\n".to_string(),
        1,
        std::rc::Rc::new(std::cell::Cell::new(true)),
        std::rc::Rc::clone(&outcomes),
    );
    wait_until(Duration::from_secs(30), || !outcomes.borrow().is_empty());
    assert_eq!(editor_text(&page), "plain replacement\n");
    let replaced = page.buffer_replacement_evidence();
    assert_eq!(replaced.protected.boundaries_live, 0, "replacement");
    assert!(replaced.protected.clear_turns >= 1, "{replaced:?}");
}

/// A recovered undelimited folded last paragraph of `giant` characters ends
/// the buffer with the stand-in terminator, outside the document text.
fn assert_terminated_recovery(name: &str, giant: usize) {
    // A point below the layout maps to the last line's end and GTK then
    // walks that line once per hidden byte (ledger A79), on every frame
    // GtkSourceView draws below the text: 3.5 s frames recovering a
    // 33.5M-character draft on `main`. A stand-in terminator, which is not
    // document text, keeps the folded paragraph off the last line.
    let draft = format!("{}{}", body_with_giants(&[]), "g".repeat(giant));
    let (_window, editor, _path, _dir) = recover_draft(&draft);
    let buffer = editor.buffer();
    let raw = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string();
    assert_eq!(
        raw,
        format!("{draft}\n"),
        "{name}: one stand-in after the body"
    );
    assert_eq!(
        editor_text(&editor),
        draft,
        "{name}: the document text is the draft"
    );
    let mut last = buffer.end_iter();
    assert!(last.backward_char());
    let terminator = buffer
        .tag_table()
        .lookup(lushtext_core::ui::protected_fold::TERMINATOR_TAG_NAME)
        .expect("the terminator tag exists");
    assert!(last.has_tag(&terminator), "{name}: the stand-in is tagged");
    let started = std::time::Instant::now();
    let _ = editor.source_view().iter_at_location(0, 100_000);
    assert!(
        started.elapsed() < Duration::from_millis(5),
        "{name}: a point below the text maps at once: {:?}",
        started.elapsed()
    );
}

#[test]
fn test_an_undelimited_folded_last_paragraph_is_never_the_last_line_direct() {
    assert_terminated_recovery("direct", n(L_PROTECT_EDIT) + 1);
}

#[test]
fn test_an_undelimited_folded_last_paragraph_is_never_the_last_line_sliced() {
    assert_terminated_recovery("sliced", 1_000_000);
}

/// The stand-in terminator never reaches the bytes on disk (review of PR #77:
/// the test helper `editor_text` re-implements `document_end_iter`, so it
/// cannot catch a leak). A recovered undelimited folded draft is written back
/// by the draft autosave, which snapshots the buffer (drafts have no protected
/// source path), and the body on disk must be the original bytes, with no
/// trailing `\n`. `giant` picks the snapshot: below 2,097,152 characters (an
/// 8 MiB estimate) a direct read, above it the chunked capture.
fn assert_autosaved_draft_is_byte_identical(name: &str, giant: usize) {
    use lushtext_core::services::{draft_service, json_store};
    let draft = format!("{}{}", body_with_giants(&[]), "g".repeat(giant));
    let (window, editor, path, _dir) = recover_draft(&draft);
    assert!(editor.is_protected_view(), "{name}: protected");
    let buffer = editor.buffer();
    assert_eq!(
        usize::try_from(buffer.char_count()).expect("fits"),
        draft.chars().count() + 1,
        "{name}: the buffer holds the stand-in"
    );
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&path);
    // Overwrite the recovered body, so only a real write can restore it.
    draft_service::fixture::write_body(&data_dir, &draft_id, "stale").expect("stale body");
    editor.set_draft_dirty(true);
    window.autosave_tick_for_test();
    wait_until(Duration::from_secs(60), || {
        let evidence = window.draft_evidence();
        !evidence.autosave_inflight
            && !evidence.mutation_inflight
            && !editor.draft_dirty()
            && draft_service::read_draft(&data_dir, &draft_id)
                .expect("read body")
                .as_deref()
                != Some("stale")
    });
    let written = draft_service::read_draft(&data_dir, &draft_id)
        .expect("read body")
        .expect("a body");
    assert_eq!(written.len(), draft.len(), "{name}: same length on disk");
    assert!(
        written.as_bytes() == draft.as_bytes(),
        "{name}: the same bytes on disk"
    );
}

#[test]
fn test_an_autosaved_folded_draft_is_byte_identical_direct_snapshot() {
    assert_autosaved_draft_is_byte_identical("direct", 1_000_000);
}

#[test]
fn test_an_autosaved_folded_draft_is_byte_identical_chunked_snapshot() {
    assert_autosaved_draft_is_byte_identical("chunked", 3_000_000);
}

#[test]
fn test_recovering_a_protected_draft_inserts_its_tail_in_bounded_slices() {
    let draft = format!("{}{}", body_with_giants(&[]), "g".repeat(3_000_000));
    let (_window, editor, _path, _dir) = recover_draft(&draft);
    assert_eq!(editor_text(&editor), draft);
    let protected = editor.buffer_replacement_evidence().protected;
    assert!(protected.tail_slices >= 3_000_000 / PROTECTED_TAIL_SLICE_BYTES as u64);
    assert!(usize_of(protected.largest_tail_slice_bytes) <= PROTECTED_TAIL_SLICE_BYTES);
    assert_eq!(protected.boundaries_live, protected.tail_slices + 1);
}

#[test]
fn test_a_dense_protected_document_folds_small_tails_without_boundaries() {
    // Many folded paragraphs whose tails each fit one slice (a dense window
    // beside a giant): they go in head then tail, several per turn, with no
    // boundary mark; only the giant's tail is sliced and marked.
    let dense_row = "d".repeat(4_000);
    let dense: String = (0..400).map(|_| format!("{dense_row}\n")).collect();
    let body = format!("{dense}{}\n{dense}", "g".repeat(1_200_000));
    let evidence = assert_sliced_install(&body, "dense-with-giant");
    let folded = DocumentProfile::of_text(&body)
        .fold_plan
        .expect("protected")
        .folded_paragraphs();
    assert!(folded > 700, "{folded}");
    assert!(
        evidence.installation_slice_count < folded / 4,
        "small tails batch into turns: {} turns for {folded} folded paragraphs",
        evidence.installation_slice_count
    );
}

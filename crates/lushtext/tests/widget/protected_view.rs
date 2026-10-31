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
    wait_until(Duration::from_secs(60), || {
        usize::try_from(active_editor(&window).buffer().char_count())
            .is_ok_and(|chars| chars == draft_chars)
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
    let (_window, editor, _path, _dir) = recover_draft(&draft);
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
    let run = format!(
        "{}zqzgrown{}",
        "p".repeat(n(L_PROTECT_EDIT)),
        "q".repeat(500)
    );
    paste_at_line_end(&page, 5, &run);
    assert_entered(&page, 5, "edit-source");
    let text = editor_text(&page);
    wait_until(Duration::from_secs(30), || {
        page.protected_view_evidence().source_bytes == Some(u64::try_from(text.len()).expect("len"))
    });

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
    let body = body_with_giants(&[n(L_PROTECT_EDIT) * 2]);
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

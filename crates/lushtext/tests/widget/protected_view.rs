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

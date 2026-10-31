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

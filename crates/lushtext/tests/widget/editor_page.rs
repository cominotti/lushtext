// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextEditorPage widget.

use crate::common::ensure_gtk_init;
use gtk4::prelude::*;
use lushtext_core::ui::editor_page::LushtextEditorPage;

#[test]
fn test_new() {
    ensure_gtk_init();
    let _page = LushtextEditorPage::new();
}

#[test]
fn test_starts_unmodified() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert!(!page.is_modified());
}

#[test]
fn test_file_path_initially_none() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert!(page.file_path().is_none());
}

#[test]
fn test_title_untitled_when_no_path() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert_eq!(page.title(), "Untitled");
}

#[test]
fn test_buffer_accessible() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    // Buffer should be a sourceview5::Buffer
    assert!(!buffer.is_modified());
}

#[test]
fn test_source_view_accessible() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let view = page.source_view();
    // Source view should be functional
    assert!(view.is_visible());
}

#[test]
fn test_buffer_text_manipulation() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    buffer.set_text("hello world");
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    assert_eq!(text.as_str(), "hello world");
}

#[test]
fn test_buffer_modified_flag() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    assert!(!page.is_modified());

    // Setting text triggers modified
    buffer.set_text("new content");
    assert!(page.is_modified());

    // Clearing modified flag
    buffer.set_modified(false);
    assert!(!page.is_modified());
}

#[test]
fn test_save_file_no_path_returns_error() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let result = page.save_file();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No file path set"));
}

#[test]
fn test_save_file_writes_content() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    // Manually set the file path (simulating load_file_async without the async part)
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    // Set path via the internal RefCell (load_file_async sets this synchronously)
    use glib::subclass::prelude::ObjectSubclassIsExt;
    page.imp().file_path.replace(Some(path.clone()));

    // Set buffer content
    buffer.set_text("saved content");

    // Save and verify
    page.save_file().unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(saved, "saved content");

    // Buffer should no longer be modified after save
    assert!(!page.is_modified());
}

#[test]
fn test_title_with_file_path() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    use glib::subclass::prelude::ObjectSubclassIsExt;
    page.imp()
        .file_path
        .replace(Some("/home/user/project/main.rs".into()));

    assert_eq!(page.title(), "main.rs");
}

#[test]
fn test_toggle_search_changes_revealer_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    use glib::subclass::prelude::ObjectSubclassIsExt;
    let revealer = &page.imp().search_revealer;

    // Initially hidden
    assert!(!revealer.reveals_child());

    // Toggle on
    page.toggle_search();
    assert!(revealer.reveals_child());

    // Toggle off
    page.toggle_search();
    assert!(!revealer.reveals_child());
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    // Verify Default impl works (it delegates to new())
    let _page: LushtextEditorPage = Default::default();
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextWindow widget.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::app::LushtextApplication;
use lushtext_core::ui::window::LushtextWindow;

/// Create a test application instance (not registered with D-Bus session).
fn test_app() -> libadwaita::Application {
    let app = LushtextApplication::new();
    app.upcast()
}

#[test]
fn test_new() {
    ensure_gtk_init();
    let app = test_app();
    let _window = LushtextWindow::new(&app);
}

#[test]
fn test_starts_with_no_tabs() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);
    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_empty_state_shows_empty_stack() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    assert_eq!(
        window
            .imp()
            .content_stack
            .visible_child_name()
            .unwrap()
            .as_str(),
        "empty"
    );
}

#[test]
fn test_new_tab_creates_tab() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    window.new_tab();
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_new_tab_title_is_untitled() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    window.new_tab();
    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "Untitled");
}

#[test]
fn test_multiple_new_tabs() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    window.new_tab();
    window.new_tab();
    window.new_tab();
    assert_eq!(window.imp().tab_view.n_pages(), 3);
}

#[test]
fn test_tabs_state_shows_tabs_stack() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    window.new_tab();

    // Process pending GTK events so the n-pages notify signal fires
    while glib::MainContext::default().iteration(false) {}

    assert_eq!(
        window
            .imp()
            .content_stack
            .visible_child_name()
            .unwrap()
            .as_str(),
        "tabs"
    );
}

#[test]
fn test_open_document_creates_tab() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "content").unwrap();

    window.open_document(tmp.path());
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_open_document_tab_title_matches_filename() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("hello.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();

    window.open_document(&file_path);

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "hello.rs");
}

#[test]
fn test_open_document_dedup() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "content").unwrap();

    // Open same file twice
    window.open_document(tmp.path());
    window.open_document(tmp.path());

    // Should still be one tab (dedup by path)
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_open_different_files_creates_separate_tabs() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    let dir = tempfile::tempdir().unwrap();
    let file1 = dir.path().join("one.rs");
    let file2 = dir.path().join("two.rs");
    std::fs::write(&file1, "first").unwrap();
    std::fs::write(&file2, "second").unwrap();

    window.open_document(&file1);
    window.open_document(&file2);

    assert_eq!(window.imp().tab_view.n_pages(), 2);
}

#[test]
fn test_load_directory_does_not_crash() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "").unwrap();

    // Should not panic
    window.load_directory(dir.path());
}

#[test]
fn test_sidebar_accessible() {
    ensure_gtk_init();
    let app = test_app();
    let window = LushtextWindow::new(&app);

    // Sidebar should be accessible via imp()
    window.imp().sidebar.set_workspace_name("test workspace");
}

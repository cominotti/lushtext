// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextSidebar multi-workspace orchestrator.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::sidebar::LushtextSidebar;
use lushtext_core::ui::window::LushtextWindow;

/// Create a window attached to a test application.
fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

// --- Sidebar construction ---

#[test]
fn test_sidebar_new() {
    ensure_gtk_init();
    let _sidebar = LushtextSidebar::new();
}

#[test]
fn test_sidebar_new_workspace_label() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().new_workspace_label.label().as_str(),
        "New Workspace"
    );
}

#[test]
fn test_sidebar_starts_with_no_sections() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert!(sidebar.imp().sections.borrow().is_empty());
}

#[test]
fn test_sidebar_new_workspace_button_exists() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    let _button = &sidebar.imp().new_workspace_button;
}

#[test]
fn test_new_workspace_affordance_stays_above_sections_scroll_area() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let first = sidebar
        .first_child()
        .expect("first child is the fixed new-workspace box");
    let last = sidebar
        .last_child()
        .and_downcast::<gtk4::ScrolledWindow>()
        .expect("last child is the workspace scrolled window");

    assert!(first.is::<gtk4::Box>());
    assert_eq!(first.as_ptr(), sidebar.imp().new_workspace_button.parent().unwrap().as_ptr());
    assert_eq!(last.as_ptr(), sidebar.imp().outer_scrolled_window.as_ptr());
}

#[test]
fn test_sidebar_outer_scroller_allows_horizontal_overflow() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().outer_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Automatic
    );
    assert!(sidebar.imp().outer_scrolled_window.propagates_natural_width());
}

// --- Window integration: tab path updates (moved from old sidebar.rs) ---

#[test]
fn test_update_tab_path_exact_match() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.rs");
    std::fs::write(&old_path, "fn main() {}").unwrap();
    window.open_document(&old_path);

    let new_path = dir.path().join("new.rs");
    window.update_tab_path(&old_path, &new_path);

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "new.rs");
}

#[test]
fn test_update_tab_path_directory_prefix_rewrite() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let old_dir = dir.path().join("old_dir");
    std::fs::create_dir(&old_dir).unwrap();
    let file_path = old_dir.join("file.rs");
    std::fs::write(&file_path, "content").unwrap();
    window.open_document(&file_path);

    let new_dir = dir.path().join("new_dir");
    window.update_tab_path(&old_dir, &new_dir);

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "file.rs");

    let editor = page
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .unwrap();
    assert_eq!(editor.file_path().unwrap(), new_dir.join("file.rs"));
}

#[test]
fn test_update_tab_path_no_match_is_noop() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("keep.rs");
    std::fs::write(&file_path, "content").unwrap();
    window.open_document(&file_path);

    window.update_tab_path(
        std::path::Path::new("/tmp/other.rs"),
        std::path::Path::new("/tmp/renamed.rs"),
    );

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "keep.rs");
}

/// Drain all pending events from the GTK main loop.
fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

// --- Window integration: close tabs ---

#[test]
fn test_close_tab_for_path_exact() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("doomed.rs");
    std::fs::write(&file_path, "").unwrap();
    window.open_document(&file_path);
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    window.close_tab_for_path(&file_path);
    flush_events();
    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_close_tab_for_path_directory_closes_children() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let f1 = sub.join("a.rs");
    let f2 = sub.join("b.rs");
    let f3 = dir.path().join("outside.rs");
    std::fs::write(&f1, "").unwrap();
    std::fs::write(&f2, "").unwrap();
    std::fs::write(&f3, "").unwrap();

    window.open_document(&f1);
    window.open_document(&f2);
    window.open_document(&f3);
    assert_eq!(window.imp().tab_view.n_pages(), 3);

    window.close_tab_for_path(&sub);
    flush_events();
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    let remaining = window
        .imp()
        .tab_view
        .nth_page(0)
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .unwrap();
    assert_eq!(remaining.file_path().unwrap(), f3);
}

#[test]
fn test_close_tab_for_path_nonexistent_is_noop() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    window.close_tab_for_path(std::path::Path::new("/does/not/exist"));
    flush_events();
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

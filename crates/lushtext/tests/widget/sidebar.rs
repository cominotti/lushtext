// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextSidebar multi-workspace orchestrator.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::sidebar::LushtextSidebar;
use lushtext_core::ui::window::LushtextWindow;
use std::time::{Duration, Instant};

const WARNING_BAR_ROW_HEIGHT: i32 = 54;

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
    let separator_after_top = first
        .next_sibling()
        .expect("separator follows the new-workspace box");
    let scroller = separator_after_top
        .next_sibling()
        .and_downcast::<gtk4::ScrolledWindow>()
        .expect("workspace scroller sits between the fixed top and bottom rows");
    let separator_before_footer = scroller
        .next_sibling()
        .expect("separator precedes the footer row");
    let footer = separator_before_footer
        .next_sibling()
        .and_downcast::<gtk4::Box>()
        .expect("footer row is the last child");

    assert!(first.is::<gtk4::Box>());
    assert_eq!(first.as_ptr(), sidebar.imp().new_workspace_button.parent().unwrap().as_ptr());
    assert!(separator_after_top.is::<gtk4::Separator>());
    assert_eq!(scroller.as_ptr(), sidebar.imp().outer_scrolled_window.as_ptr());
    assert!(separator_before_footer.is::<gtk4::Separator>());
    assert_eq!(footer.as_ptr(), sidebar.imp().workspace_size_box.as_ptr());
}

#[test]
fn test_sidebar_outer_scroller_disables_horizontal_scrollbar() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().outer_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );
    assert!(!sidebar.imp().outer_scrolled_window.propagates_natural_width());
}

#[test]
fn test_sidebar_footer_buttons_exist_and_default_to_comfy() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    assert_eq!(sidebar.imp().small_width_button.label().as_deref(), Some("Small (20%)"));
    assert_eq!(
        sidebar.imp().comfy_width_button.label().as_deref(),
        Some("Comfy (30%)")
    );
    assert_eq!(sidebar.imp().large_width_button.label().as_deref(), Some("Large (40%)"));
    assert!(!sidebar.imp().small_width_button.is_active());
    assert!(sidebar.imp().comfy_width_button.is_active());
    assert!(!sidebar.imp().large_width_button.is_active());
}

#[test]
fn test_sidebar_new_workspace_affordance_matches_document_restored_warning_height() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1200, 800);
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().new_workspace_box.height() > 0
    });

    let sidebar_height = window.imp().sidebar.imp().new_workspace_box.height();
    assert_eq!(
        sidebar_height, WARNING_BAR_ROW_HEIGHT,
        "new workspace affordance height should preserve the warning-bar sizing contract (sidebar={sidebar_height}, expected={WARNING_BAR_ROW_HEIGHT})",
    );

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().workspace_size_box.height() > 0
    });
    let footer_height = window.imp().sidebar.imp().workspace_size_box.height();
    assert_eq!(
        footer_height, WARNING_BAR_ROW_HEIGHT,
        "workspace size footer should match the same fixed row height contract (footer={footer_height}, expected={WARNING_BAR_ROW_HEIGHT})",
    );
    assert_eq!(footer_height, sidebar_height);
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

fn flush_after_delay(delay: Duration) {
    std::thread::sleep(delay);
    flush_events();
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }
    panic!("condition was not met within {:?}", timeout);
}

fn present_window(window: &LushtextWindow) {
    window.present();
    flush_events();
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextWindow widget.

use crate::common::ensure_gtk_init;
use gio::prelude::{ActionExt, ActionGroupExt, ActionMapExt};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::app::LushtextApplication;
use lushtext_core::ui::editor_page::LushtextEditorPage;
use lushtext_core::ui::window::LushtextWindow;

/// Create a window attached to a test application (not registered with D-Bus).
fn test_window() -> LushtextWindow {
    let app: libadwaita::Application = LushtextApplication::new().upcast();
    LushtextWindow::new(&app)
}

/// Drain all pending events from the GTK main loop.
fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

/// Look up a window action's enabled state.
fn action_enabled(window: &LushtextWindow, name: &str) -> bool {
    let action = window
        .lookup_action(name)
        .unwrap_or_else(|| panic!("action '{name}' not found"));
    action.is_enabled()
}

/// Activate a named window action and drain pending events.
fn activate_action(window: &LushtextWindow, name: &str) {
    ActionGroupExt::activate_action(window, name, None);
    flush_events();
}

/// Get the active editor page from the window.
fn active_editor(window: &LushtextWindow) -> LushtextEditorPage {
    window
        .imp()
        .tab_view
        .selected_page()
        .unwrap()
        .child()
        .downcast::<LushtextEditorPage>()
        .unwrap()
}

/// Get the visible child name of the content stack.
fn visible_stack_name(window: &LushtextWindow) -> String {
    window
        .imp()
        .content_stack
        .visible_child_name()
        .unwrap()
        .to_string()
}

// --- Construction ---

#[test]
fn test_new() {
    ensure_gtk_init();
    let _window = test_window();
}

#[test]
fn test_starts_with_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

// --- Content stack state ---

#[test]
fn test_empty_state_shows_empty_stack() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(visible_stack_name(&window), "empty");
}

#[test]
fn test_tabs_state_shows_tabs_stack() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    assert_eq!(visible_stack_name(&window), "tabs");
}

// --- Tab management ---

#[test]
fn test_new_tab_creates_tab() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_new_tab_title_is_untitled() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "Untitled");
}

#[test]
fn test_multiple_new_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    window.new_tab();
    window.new_tab();
    assert_eq!(window.imp().tab_view.n_pages(), 3);
}

// --- File opening ---

#[test]
fn test_open_document_creates_tab() {
    ensure_gtk_init();
    let window = test_window();

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "content").unwrap();

    window.open_document(tmp.path());
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_open_document_tab_title_matches_filename() {
    ensure_gtk_init();
    let window = test_window();

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
    let window = test_window();

    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "content").unwrap();

    window.open_document(tmp.path());
    window.open_document(tmp.path());
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_open_different_files_creates_separate_tabs() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file1 = dir.path().join("one.rs");
    let file2 = dir.path().join("two.rs");
    std::fs::write(&file1, "first").unwrap();
    std::fs::write(&file2, "second").unwrap();

    window.open_document(&file1);
    window.open_document(&file2);
    assert_eq!(window.imp().tab_view.n_pages(), 2);
}

// --- Sidebar ---

#[test]
fn test_load_directory_does_not_crash() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "").unwrap();
    window.load_directory(dir.path());
}

#[test]
fn test_sidebar_workspace_name() {
    ensure_gtk_init();
    let window = test_window();
    window.imp().sidebar.set_workspace_name("test workspace");
}

// --- Action enabled/disabled state ---

#[test]
fn test_tab_actions_disabled_when_no_tabs() {
    ensure_gtk_init();
    let window = test_window();

    assert!(!action_enabled(&window, "toggle-search"));
    assert!(!action_enabled(&window, "save"));
    assert!(!action_enabled(&window, "close-tab"));
}

#[test]
fn test_tab_actions_enabled_when_tab_exists() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();

    assert!(action_enabled(&window, "toggle-search"));
    assert!(action_enabled(&window, "save"));
    assert!(action_enabled(&window, "close-tab"));
}

#[test]
fn test_tab_independent_actions_always_enabled() {
    ensure_gtk_init();
    let window = test_window();

    assert!(action_enabled(&window, "new-tab"));
    assert!(action_enabled(&window, "open-file"));
    assert!(action_enabled(&window, "open-folder"));
}

// --- Toggle-search via action system ---

#[test]
fn test_toggle_search_action_opens_search() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();

    activate_action(&window, "toggle-search");

    let editor = active_editor(&window);
    assert!(editor.imp().search_revealer.reveals_child());
}

#[test]
fn test_toggle_search_action_closes_search() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();

    activate_action(&window, "toggle-search");
    activate_action(&window, "toggle-search");

    let editor = active_editor(&window);
    assert!(!editor.imp().search_revealer.reveals_child());
}

#[test]
fn test_toggle_search_noop_when_disabled() {
    ensure_gtk_init();
    let window = test_window();

    assert!(!action_enabled(&window, "toggle-search"));
    activate_action(&window, "toggle-search");

    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_toggle_search_survives_event_loop() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.toggle_search();
    assert!(page.imp().search_revealer.reveals_child());

    flush_events();
    assert!(page.imp().search_revealer.reveals_child());
}

// --- Close-tab via action ---

#[test]
fn test_close_tab_action_removes_tab() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    activate_action(&window, "close-tab");

    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_close_tab_disables_tab_actions() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert!(action_enabled(&window, "toggle-search"));

    activate_action(&window, "close-tab");

    assert!(!action_enabled(&window, "toggle-search"));
    assert!(!action_enabled(&window, "save"));
    assert!(!action_enabled(&window, "close-tab"));
}

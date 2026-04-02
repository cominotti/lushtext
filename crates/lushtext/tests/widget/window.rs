// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextWindow widget.

use crate::common::ensure_gtk_init;
use gio::prelude::{ActionExt, ActionGroupExt, ActionMapExt};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::app::LushtextApplication;
use lushtext_core::config::keys;
use lushtext_core::ui::editor_page::LushtextEditorPage;
use lushtext_core::ui::window::clamp_sidebar_position;
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

/// Read the metadata_box's own "visible" property, bypassing is_visible()
/// which checks the parent chain (and returns false for unrealized windows).
fn metadata_box_visible(window: &LushtextWindow) -> bool {
    window
        .imp()
        .status_bar
        .imp()
        .metadata_box
        .property::<bool>("visible")
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
fn test_sidebar_accessible() {
    ensure_gtk_init();
    let window = test_window();
    let _sidebar = &window.imp().sidebar;
}

#[test]
fn test_sidebar_footer_exists() {
    ensure_gtk_init();
    let window = test_window();
    let sidebar_imp = window.imp().sidebar.imp();
    assert_eq!(
        sidebar_imp.new_workspace_label.label().as_str(),
        "New Workspace"
    );
}

#[test]
fn test_sidebar_sections_box_exists() {
    ensure_gtk_init();
    let window = test_window();
    let _sections_box = &window.imp().sidebar.imp().sections_box;
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

// --- Status bar integration ---

#[test]
fn test_status_bar_accessible() {
    ensure_gtk_init();
    let window = test_window();
    let _status_bar = &window.imp().status_bar;
}

#[test]
fn test_status_bar_metadata_hidden_when_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    assert!(!metadata_box_visible(&window));
}

#[test]
fn test_status_bar_metadata_visible_after_new_tab() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();
    // Note: is_visible() checks the parent chain, so it returns false for
    // unrealized windows. Use the "visible" property directly instead.
    assert!(metadata_box_visible(&window));
}

#[test]
fn test_status_bar_metadata_hidden_after_closing_all_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();
    activate_action(&window, "close-tab");
    assert!(!metadata_box_visible(&window));
}

#[test]
fn test_status_bar_file_size_empty_for_untitled_tab() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();
    let size_text = window.imp().status_bar.imp().file_size_label.label();
    assert_eq!(size_text.as_str(), "");
}

#[test]
fn test_status_bar_encoding_shows_utf8() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();
    let enc_text = window.imp().status_bar.imp().encoding_label.label();
    assert_eq!(enc_text.as_str(), "UTF-8");
}

#[test]
fn test_status_bar_push_message_from_window() {
    ensure_gtk_init();
    let window = test_window();
    window.imp().status_bar.push_message(
        "Test message",
        lushtext_core::ui::status_bar::MessageKind::Info,
    );
    let msg_text = window.imp().status_bar.imp().message_label.label();
    assert_eq!(msg_text.as_str(), "Test message");
}

// --- Save-as action enabled/disabled ---

#[test]
fn test_save_as_action_disabled_when_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    assert!(!action_enabled(&window, "save-as"));
}

#[test]
fn test_save_as_action_enabled_when_tab_exists() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert!(action_enabled(&window, "save-as"));
}

#[test]
fn test_save_as_action_disabled_after_closing_all_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert!(action_enabled(&window, "save-as"));

    activate_action(&window, "close-tab");
    assert!(!action_enabled(&window, "save-as"));
}

// --- GSettings window state keys exist with correct defaults ---

#[test]
fn test_gsettings_window_width_default() {
    ensure_gtk_init();
    let window = test_window();
    let width = window.imp().settings.int(keys::WINDOW_WIDTH);
    assert_eq!(width, 1200);
}

#[test]
fn test_gsettings_window_height_default() {
    ensure_gtk_init();
    let window = test_window();
    let height = window.imp().settings.int(keys::WINDOW_HEIGHT);
    assert_eq!(height, 800);
}

#[test]
fn test_gsettings_window_maximized_default() {
    ensure_gtk_init();
    let window = test_window();
    let maximized = window.imp().settings.boolean(keys::WINDOW_MAXIMIZED);
    assert!(!maximized);
}

#[test]
fn test_gsettings_sidebar_position_default() {
    ensure_gtk_init();
    let window = test_window();
    let pos = window.imp().settings.int(keys::SIDEBAR_POSITION);
    assert_eq!(pos, 250);
}

// --- Sidebar paned position ---

#[test]
fn test_sidebar_paned_restores_default_position() {
    ensure_gtk_init();
    let window = test_window();
    // The paned should start at the GSettings default (250)
    assert_eq!(window.imp().main_paned.position(), 250);
}

// --- Window default size restored from GSettings ---

#[test]
fn test_window_restores_default_size() {
    ensure_gtk_init();
    let window = test_window();
    let (w, h) = window.default_size();
    assert_eq!(w, 1200);
    assert_eq!(h, 800);
}

// --- Sidebar clamp function unit tests ---

#[test]
fn test_clamp_noop_when_within_limit() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(300);

    // Window width 1200 → max 400. Position 300 is fine.
    clamp_sidebar_position(
        paned,
        1200,
        &window.imp().settings,
        &window.imp().last_sidebar_pos,
    );
    assert_eq!(paned.position(), 300);
}

#[test]
fn test_clamp_reduces_when_over_limit() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(500);

    // Window width 1200 → max 400. Position 500 exceeds.
    clamp_sidebar_position(
        paned,
        1200,
        &window.imp().settings,
        &window.imp().last_sidebar_pos,
    );
    assert_eq!(paned.position(), 400);
}

#[test]
fn test_clamp_at_exact_limit() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(400);

    // Window width 1200 → max 400. Position 400 is exactly at limit.
    clamp_sidebar_position(
        paned,
        1200,
        &window.imp().settings,
        &window.imp().last_sidebar_pos,
    );
    assert_eq!(paned.position(), 400);
}

#[test]
fn test_clamp_noop_when_window_width_zero() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(500);

    // Width 0 = unrealized window. Should not clamp.
    clamp_sidebar_position(
        paned,
        0,
        &window.imp().settings,
        &window.imp().last_sidebar_pos,
    );
    assert_eq!(paned.position(), 500);
}

#[test]
fn test_clamp_simulates_unmaximize_scenario() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;

    // Simulate: sidebar was at 1/3 of 1920px maximized window
    paned.set_position(640);

    // Window un-maximizes to 1200px — sidebar must be clamped to 400
    clamp_sidebar_position(
        paned,
        1200,
        &window.imp().settings,
        &window.imp().last_sidebar_pos,
    );
    assert_eq!(paned.position(), 400);
}

#[test]
fn test_clamp_persists_to_gsettings() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let settings = &window.imp().settings;
    paned.set_position(350);

    clamp_sidebar_position(paned, 1200, settings, &window.imp().last_sidebar_pos);
    assert_eq!(settings.int(keys::SIDEBAR_POSITION), 350);
}

#[test]
fn test_clamp_persists_clamped_value_to_gsettings() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let settings = &window.imp().settings;
    paned.set_position(600);

    // Clamp to 400, should persist 400 not 600
    clamp_sidebar_position(paned, 1200, settings, &window.imp().last_sidebar_pos);
    assert_eq!(settings.int(keys::SIDEBAR_POSITION), 400);
}

// --- Tab modified dot (• prefix in tab title) ---

#[test]
fn test_new_tab_no_dot_initially() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "Untitled");
}

#[test]
fn test_open_document_no_dot_initially() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("test.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    window.open_document(&file_path);
    flush_events();

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "test.rs");
}

#[test]
fn test_modified_buffer_shows_dot_in_tab() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("test.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    window.open_document(&file_path);
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("modified content");
    flush_events();

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "• test.rs");
}

#[test]
fn test_save_clears_dot_in_tab() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("saveme.rs");
    std::fs::write(&file_path, "original").unwrap();
    window.open_document(&file_path);
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("changed");
    flush_events();
    assert!(window.imp().tab_view.nth_page(0).title().starts_with('•'));

    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    editor.save_file_async(move |r| {
        r.unwrap();
        done_clone.set(true);
    });
    while !done.get() {
        glib::MainContext::default().iteration(true);
    }
    flush_events();
    assert_eq!(
        window.imp().tab_view.nth_page(0).title().as_str(),
        "saveme.rs"
    );
}

#[test]
fn test_untitled_tab_modified_shows_dot() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("some text");
    flush_events();

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "• Untitled");
}

// --- Header bar title/subtitle ---

#[test]
fn test_header_title_shows_lushtext_when_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(window.imp().title_widget.title().as_str(), "LushText");
    assert_eq!(window.imp().title_widget.subtitle().as_str(), "");
}

#[test]
fn test_header_title_shows_filename_after_open() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("hello.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    window.open_document(&file_path);
    flush_events();

    assert_eq!(window.imp().title_widget.title().as_str(), "hello.rs");
}

#[test]
fn test_header_subtitle_shows_filepath_after_open() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("hello.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    window.open_document(&file_path);
    flush_events();

    assert_eq!(
        window.imp().title_widget.subtitle().as_str(),
        file_path.display().to_string()
    );
}

#[test]
fn test_header_title_shows_untitled_for_new_tab() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    assert_eq!(window.imp().title_widget.title().as_str(), "Untitled");
}

#[test]
fn test_header_subtitle_empty_for_untitled() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    assert_eq!(window.imp().title_widget.subtitle().as_str(), "");
}

#[test]
fn test_header_title_updates_on_tab_switch() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file1 = dir.path().join("first.rs");
    let file2 = dir.path().join("second.rs");
    std::fs::write(&file1, "one").unwrap();
    std::fs::write(&file2, "two").unwrap();

    window.open_document(&file1);
    window.open_document(&file2);
    flush_events();

    // Currently on second tab
    assert_eq!(window.imp().title_widget.title().as_str(), "second.rs");

    // Switch to first tab
    let first_page = window.imp().tab_view.nth_page(0);
    window.imp().tab_view.set_selected_page(&first_page);
    flush_events();

    assert_eq!(window.imp().title_widget.title().as_str(), "first.rs");
}

#[test]
fn test_header_title_resets_after_closing_all_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();
    assert_eq!(window.imp().title_widget.title().as_str(), "Untitled");

    activate_action(&window, "close-tab");
    assert_eq!(window.imp().title_widget.title().as_str(), "LushText");
    assert_eq!(window.imp().title_widget.subtitle().as_str(), "");
}

// --- Header bar modified dot (• prefix in title) ---

#[test]
fn test_header_title_no_dot_when_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    assert!(!window.imp().title_widget.title().starts_with('•'));
}

#[test]
fn test_header_title_no_dot_for_clean_file() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("clean.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    window.open_document(&file_path);
    flush_events();

    assert_eq!(window.imp().title_widget.title().as_str(), "clean.rs");
}

#[test]
fn test_header_title_dot_when_buffer_modified() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("dirty.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    window.open_document(&file_path);
    flush_events();

    active_editor(&window).buffer().set_text("changed");
    flush_events();

    assert_eq!(window.imp().title_widget.title().as_str(), "• dirty.rs");
}

#[test]
fn test_header_title_dot_cleared_after_save() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("saved.rs");
    std::fs::write(&file_path, "original").unwrap();
    window.open_document(&file_path);
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("changed");
    flush_events();
    assert!(window.imp().title_widget.title().starts_with('•'));

    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    editor.save_file_async(move |r| {
        r.unwrap();
        done_clone.set(true);
    });
    while !done.get() {
        glib::MainContext::default().iteration(true);
    }
    flush_events();
    assert_eq!(window.imp().title_widget.title().as_str(), "saved.rs");
}

#[test]
fn test_header_title_dot_cleared_after_closing_all_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    active_editor(&window).buffer().set_text("dirty");
    flush_events();
    assert!(window.imp().title_widget.title().starts_with('•'));

    activate_action(&window, "close-tab");
    assert_eq!(window.imp().title_widget.title().as_str(), "LushText");
}

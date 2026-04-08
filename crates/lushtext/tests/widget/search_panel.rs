// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the workspace search panel and its components.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::app::LushtextApplication;
use lushtext_core::ui::search_panel::LushtextSearchPanel;
use lushtext_core::ui::search_panel::item::SearchResultItem;
use lushtext_core::ui::window::LushtextWindow;

/// Drain all pending events from the GTK main loop.
fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

/// Create a window attached to a test application (not registered with D-Bus).
fn test_window() -> LushtextWindow {
    let app: libadwaita::Application = LushtextApplication::new().upcast();
    LushtextWindow::new(&app)
}

// ---------------------------------------------------------------------------
// SearchResultItem GObject adapter
// ---------------------------------------------------------------------------

#[test]
fn test_search_result_item_new_file() {
    ensure_gtk_init();
    let item = SearchResultItem::new_file("/home/user/project/src/main.rs", "src/main.rs", 5);
    assert!(item.is_file_item());
    assert!(!item.is_match_item());
    assert_eq!(item.file_path(), "/home/user/project/src/main.rs");
    assert_eq!(item.display_path(), "src/main.rs");
    assert_eq!(item.match_count(), 5);
    assert_eq!(item.line_number(), 0);
    assert!(item.line_content().is_empty());
}

#[test]
fn test_search_result_item_new_match() {
    ensure_gtk_init();
    let item =
        SearchResultItem::new_match("/home/user/project/src/main.rs", 42, "fn main() {", 0, 2);
    assert!(!item.is_file_item());
    assert!(item.is_match_item());
    assert_eq!(item.file_path(), "/home/user/project/src/main.rs");
    assert_eq!(item.line_number(), 42);
    assert_eq!(item.line_content(), "fn main() {");
    assert_eq!(item.match_count(), 0);
    assert!(item.display_path().is_empty());
    assert_eq!(item.match_start(), 0);
    assert_eq!(item.match_end(), 2);
}

#[test]
fn test_search_result_item_match_count_is_mutable() {
    ensure_gtk_init();
    let item = SearchResultItem::new_file("/path/to/file.rs", "file.rs", 0);
    assert_eq!(item.match_count(), 0);
    item.set_match_count(10);
    assert_eq!(item.match_count(), 10);
    item.set_match_count(25);
    assert_eq!(item.match_count(), 25);
}

// ---------------------------------------------------------------------------
// LushtextSearchPanel widget
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_construction() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Verify template children are accessible.
    assert!(panel.imp().search_entry.text().is_empty());
    assert!(panel.imp().count_label.text().is_empty());
    assert!(!panel.imp().error_label.is_visible());
}

#[test]
fn test_search_panel_set_query() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("hello world");
    assert_eq!(panel.query(), "hello world");
}

#[test]
fn test_search_panel_set_workspace_roots() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let roots = vec![
        std::path::PathBuf::from("/project/root1"),
        std::path::PathBuf::from("/project/root2"),
    ];
    panel.set_workspace_roots(roots.clone());
    assert_eq!(*panel.imp().workspace_roots.borrow(), roots);
}

#[test]
fn test_search_panel_connect_close_requested() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let called_clone = called.clone();
    panel.connect_close_requested(move || {
        called_clone.set(true);
    });
    // Callback is stored.
    assert!(panel.imp().close_requested_callback.borrow().is_some());
}

#[test]
fn test_search_panel_connect_open_file() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    let called_clone = called.clone();
    panel.connect_open_file(move |_path, _line| {
        called_clone.set(true);
    });
    // Callback is stored.
    assert!(panel.imp().open_file_callback.borrow().is_some());
}

#[test]
fn test_search_panel_clear_results_resets_state() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Simulate some state.
    panel.imp().total_matches.set(42);
    panel.imp().total_files.set(5);
    panel.imp().result_capped.set(true);

    // Add a file group to root_store.
    let file_item = SearchResultItem::new_file("/test.rs", "test.rs", 3);
    panel.imp().root_store.append(&file_item);

    // Trigger clear via start_search with empty query.
    panel.start_search("");

    assert_eq!(panel.imp().total_matches.get(), 0);
    assert_eq!(panel.imp().total_files.get(), 0);
    assert!(!panel.imp().result_capped.get());
    assert_eq!(panel.imp().root_store.n_items(), 0);
    assert!(panel.imp().file_groups.borrow().is_empty());
}

// ---------------------------------------------------------------------------
// Results height clamping
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_clamp_results_height_sets_max_content_height() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Default: no max-content-height (-1 means unbounded in GTK).
    assert_eq!(panel.imp().results_scroll.max_content_height(), -1);

    // Clamp to 300px.
    panel.clamp_results_height(300);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 300);

    // Clamp to a smaller value.
    panel.clamp_results_height(200);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 200);
}

#[test]
fn test_search_panel_clamp_results_height_respects_minimum() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Values below 100 are clamped to 100 (matches min-content-height in template).
    panel.clamp_results_height(50);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 100);

    panel.clamp_results_height(0);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 100);

    panel.clamp_results_height(-10);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 100);
}

#[test]
fn test_search_panel_clamp_results_height_guard_skips_redundant_set() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // First call sets the value.
    panel.clamp_results_height(300);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 300);

    // Second call with the same value is a no-op (guard check).
    // We verify by confirming the value is still correct — the guard prevents
    // unnecessary set_max_content_height calls that would trigger re-layout.
    panel.clamp_results_height(300);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 300);
}

// ---------------------------------------------------------------------------
// Window integration: toggle action + revealer
// ---------------------------------------------------------------------------

#[test]
fn test_toggle_search_panel_action_exists_and_enabled() {
    ensure_gtk_init();
    let window = test_window();
    let action = window.lookup_action("toggle-search-panel");
    assert!(action.is_some(), "toggle-search-panel action must exist");
    assert!(action.unwrap().is_enabled(), "action must be enabled");
}

#[test]
fn test_toggle_search_panel_revealer_shows_on_activate() {
    ensure_gtk_init();
    let window = test_window();

    // Initially hidden (GSettings default is false).
    let revealer = &window.imp().search_panel_revealer;
    assert!(!revealer.reveals_child());

    // Activate the toggle action — should reveal the panel.
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-search-panel", None);
    flush_events();
    assert!(revealer.reveals_child());
}

#[test]
fn test_toggle_search_panel_close_hides_revealer() {
    ensure_gtk_init();
    let window = test_window();

    // Open the panel.
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-search-panel", None);
    flush_events();
    assert!(window.imp().search_panel_revealer.reveals_child());

    // Close the panel.
    window.close_search_panel();
    flush_events();
    assert!(!window.imp().search_panel_revealer.reveals_child());
}

// ---------------------------------------------------------------------------
// Story 1.3: Match range on SearchResultItem
// ---------------------------------------------------------------------------

#[test]
fn test_search_result_item_match_range_stored_and_returned() {
    ensure_gtk_init();
    let item = SearchResultItem::new_match("/path/test.rs", 10, "let x = 42;", 4, 5);
    assert_eq!(item.match_start(), 4);
    assert_eq!(item.match_end(), 5);
}

#[test]
fn test_search_result_item_match_range_defaults_to_zero() {
    ensure_gtk_init();
    // File items should have default (0, 0) match range.
    let item = SearchResultItem::new_file("/path/test.rs", "test.rs", 3);
    assert_eq!(item.match_start(), 0);
    assert_eq!(item.match_end(), 0);
}

// ---------------------------------------------------------------------------
// Story 1.3: Toggle buttons on search panel
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_has_toggle_template_children() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let imp = panel.imp();
    // All toggle buttons should be accessible as template children.
    assert!(!imp.case_toggle.is_active());
    assert!(!imp.regex_toggle.is_active());
    assert!(!imp.word_toggle.is_active());
    assert!(imp.more_toggle.is_sensitive()); // Enabled in Story 1.4
}

#[test]
fn test_search_panel_toggle_initial_state_all_off() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // GSettings defaults are all false — toggles should be inactive.
    assert!(!panel.imp().case_toggle.is_active());
    assert!(!panel.imp().regex_toggle.is_active());
    assert!(!panel.imp().word_toggle.is_active());
}

#[test]
fn test_search_panel_gsettings_keys_exist_with_defaults() {
    ensure_gtk_init();
    // The panel construction binds GSettings — if keys are missing, it panics.
    // This test verifies construction succeeds (keys exist) and defaults are correct.
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // All toggles should be off (matching GSettings defaults of false).
    assert!(!panel.imp().case_toggle.is_active());
    assert!(!panel.imp().regex_toggle.is_active());
    assert!(!panel.imp().word_toggle.is_active());
}

// ---------------------------------------------------------------------------
// Story 1.4: Options panel, gitignore toggle, glob filter
// ---------------------------------------------------------------------------

#[test]
fn test_more_toggle_is_sensitive() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // more_toggle was a placeholder in Story 1.3 (sensitive=false).
    // Story 1.4 enables it.
    assert!(panel.imp().more_toggle.is_sensitive());
}

#[test]
fn test_options_revealer_exists_and_starts_hidden() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Options revealer starts with reveal_child=false (GSettings default).
    assert!(!panel.imp().options_revealer.reveals_child());
}

#[test]
fn test_gitignore_toggle_exists_and_starts_active() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Gitignore toggle defaults to active=true (GSettings default for search-gitignore).
    assert!(panel.imp().gitignore_toggle.is_active());
}

#[test]
fn test_glob_entry_exists_and_starts_empty() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(panel.imp().glob_entry.text().is_empty());
}

#[test]
fn test_gsettings_search_panel_options_expanded_default() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // GSettings default for search-panel-options-expanded is false.
    // The more_toggle should be inactive (not expanded).
    assert!(!panel.imp().more_toggle.is_active());
    // The options revealer should be hidden.
    assert!(!panel.imp().options_revealer.reveals_child());
}

#[test]
fn test_gsettings_search_gitignore_default() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // GSettings default for search-gitignore is true.
    assert!(panel.imp().gitignore_toggle.is_active());
}

#[test]
fn test_clear_results_removes_warning_class() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Simulate warning state from truncation.
    panel.imp().count_label.add_css_class("warning");
    assert!(panel.imp().count_label.has_css_class("warning"));

    // Clear results should remove the warning class.
    panel.start_search("");
    assert!(!panel.imp().count_label.has_css_class("warning"));
}

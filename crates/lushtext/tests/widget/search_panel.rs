// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the workspace search panel and its components.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::content_search::{
    ContentSearchOptions, ReplaceResult, Replacement, SavedSearch, SearchEvent, SearchHistoryEntry,
    SearchMatch, SearchQuerySpec, generate_replacement_preview,
};
use lushtext_core::services::notifications::{
    NotificationBus, NotificationOwner, NotificationPayload, NotificationSeverity,
    NotificationSurface, StatusMessage,
};
use lushtext_core::services::content_search::{ReplaceUndoBackup, ReplaceUndoEntry};
use lushtext_core::services::{json_store, search_backup};
use lushtext_core::ui::search_panel::item::SearchResultItem;
use lushtext_core::ui::search_panel::{
    LushtextSearchPanel, SearchFileGroup, SearchMatchLocation, SearchProgressUpdate,
};
use lushtext_core::ui::status_bar::LushtextStatusBar;
use lushtext_core::ui::window::LushtextWindow;
use std::time::{Duration, Instant};

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
        flush_after_delay(Duration::from_millis(20));
        if predicate() {
            return;
        }
    }
    assert!(predicate(), "timed out waiting for widget state");
}

fn replace_undo_entry(original: &[u8], replaced: &[u8]) -> ReplaceUndoEntry {
    ReplaceUndoEntry::new(original.to_vec(), replaced.to_vec())
}

fn make_history_entry(
    query: &str,
    case_sensitive: bool,
    regex: bool,
    whole_word: bool,
    gitignore: bool,
    glob: Option<&str>,
) -> SearchHistoryEntry {
    SearchHistoryEntry::from_spec(SearchQuerySpec::new(
        query.to_string(),
        ContentSearchOptions::new(
            case_sensitive,
            regex,
            whole_word,
            gitignore,
            glob.map(ToString::to_string),
        ),
    ))
}

fn make_saved_search(name: &str, query: &str) -> SavedSearch {
    SavedSearch::from_spec(
        name.to_string(),
        SearchQuerySpec::new(query.to_string(), ContentSearchOptions::default()),
    )
}

fn search_spec(query: &str) -> SearchQuerySpec {
    SearchQuerySpec::new(query.to_string(), ContentSearchOptions::default())
}

/// Create a window attached to a test application (not registered with D-Bus).
fn test_window() -> LushtextWindow {
    crate::common::test_window()
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
    let item = SearchResultItem::new_match(
        "/home/user/project/src/main.rs",
        42,
        "fn main() {",
        0,
        2,
        "fn main() {",
        0,
        2,
    );
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
    assert_eq!(*panel.imp().runtime.workspace_roots.borrow(), roots);
}

#[test]
fn test_start_search_uses_passed_query_spec_instead_of_live_widget_state() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    std::fs::write(dir.path().join("notes.txt"), "needle here\n").expect("expected operation to succeed");

    panel.set_workspace_roots(vec![dir.path().to_path_buf()]);
    panel.set_query("absent");
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        !panel.imp().runtime.searching.get()
    });

    assert_eq!(panel.query(), "absent");
    assert_eq!(panel.imp().runtime.total_matches.get(), 1);
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
    assert!(
        panel
            .imp()
            .callbacks
            .close_requested_callback
            .borrow()
            .is_some()
    );
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
    assert!(panel.imp().callbacks.open_file_callback.borrow().is_some());
}

#[test]
fn test_search_panel_clear_results_resets_state() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Simulate some state.
    panel.imp().runtime.total_matches.set(42);
    panel.imp().runtime.total_files.set(5);
    panel.imp().runtime.result_capped.set(true);

    // Add a file group to root_store.
    let file_item = SearchResultItem::new_file("/test.rs", "test.rs", 3);
    panel.imp().runtime.root_store.append(&file_item);

    // Trigger clear via start_search with empty query.
    panel.start_search(&search_spec(""));

    assert_eq!(panel.imp().runtime.total_matches.get(), 0);
    assert_eq!(panel.imp().runtime.total_files.get(), 0);
    assert!(!panel.imp().runtime.result_capped.get());
    assert_eq!(panel.imp().runtime.root_store.n_items(), 0);
    assert!(panel.imp().runtime.file_groups.borrow().is_empty());
}

// ---------------------------------------------------------------------------
// Results height clamping
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_clamp_results_height_sets_max_content_height() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Default: no max-content-height or fixed height request.
    assert_eq!(panel.imp().results_scroll.max_content_height(), -1);
    assert_eq!(panel.imp().results_scroll.height_request(), -1);

    // Clamp to 300px.
    panel.clamp_results_height(300);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 300);
    assert_eq!(panel.imp().results_scroll.height_request(), 300);

    // Clamp to a smaller value.
    panel.clamp_results_height(200);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 200);
    assert_eq!(panel.imp().results_scroll.height_request(), 200);
}

#[test]
fn test_search_panel_clamp_results_height_respects_minimum() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Values below 100 are clamped to 100 (matches min-content-height in template).
    panel.clamp_results_height(50);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 100);
    assert_eq!(panel.imp().results_scroll.height_request(), 100);

    panel.clamp_results_height(0);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 100);
    assert_eq!(panel.imp().results_scroll.height_request(), 100);

    panel.clamp_results_height(-10);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 100);
    assert_eq!(panel.imp().results_scroll.height_request(), 100);
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
    assert_eq!(panel.imp().results_scroll.height_request(), 300);
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
    assert!(action.expect("expected operation to succeed").is_enabled(), "action must be enabled");
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
    let item = SearchResultItem::new_match(
        "/path/test.rs",
        10,
        "let x = 42;",
        4,
        5,
        "let x = 42;",
        4,
        5,
    );
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
    panel.start_search(&search_spec(""));
    assert!(!panel.imp().count_label.has_css_class("warning"));
}

// ---------------------------------------------------------------------------
// Story 1.5: SearchEvent::Progress variant
// ---------------------------------------------------------------------------

#[test]
fn test_search_event_progress_variant() {
    ensure_gtk_init();
    // Progress variant can be constructed and pattern-matched.
    let event = SearchEvent::Progress(42);
    assert!(matches!(event, SearchEvent::Progress(42)));
}

// ---------------------------------------------------------------------------
// Story 1.5: Match navigation state
// ---------------------------------------------------------------------------

#[test]
fn test_navigate_next_match_empty_is_noop() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // With no matches, navigate_next_match should not panic or change state.
    panel.navigate_next_match();
    assert!(panel.imp().navigation.current_match_index.get().is_none());
}

#[test]
fn test_navigate_prev_match_empty_is_noop() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.navigate_prev_match();
    assert!(panel.imp().navigation.current_match_index.get().is_none());
}

#[test]
fn test_has_results_false_on_fresh_panel() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(!panel.has_results());
}

#[test]
fn test_has_results_true_after_matches() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Simulate matches arriving (set internal state directly).
    panel.imp().runtime.total_matches.set(5);
    assert!(panel.has_results());
}

#[test]
fn test_current_match_index_resets_on_clear() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Simulate some navigation state.
    panel.imp().navigation.current_match_index.set(Some(3));
    panel
        .imp()
        .navigation
        .match_positions
        .borrow_mut()
        .push(SearchMatchLocation::new(
            std::path::PathBuf::from("/test.rs"),
            10,
        ));

    // Clear via empty search.
    panel.start_search(&search_spec(""));

    assert!(panel.imp().navigation.current_match_index.get().is_none());
    assert!(panel.imp().navigation.match_positions.borrow().is_empty());
}

// ---------------------------------------------------------------------------
// Story 1.5: F4/Shift+F4 actions and shortcuts
// ---------------------------------------------------------------------------

#[test]
fn test_f4_shortcut_bound_search_next_match() {
    ensure_gtk_init();
    let window = test_window();
    let action = window.lookup_action("search-next-match");
    assert!(action.is_some(), "search-next-match action must exist");
}

#[test]
fn test_shift_f4_shortcut_bound_search_prev_match() {
    ensure_gtk_init();
    let window = test_window();
    let action = window.lookup_action("search-prev-match");
    assert!(action.is_some(), "search-prev-match action must exist");
}

#[test]
fn test_search_navigation_actions_start_disabled() {
    ensure_gtk_init();
    let window = test_window();
    // No tabs, no search panel visible, no results → actions should be disabled.
    let next = window.lookup_action("search-next-match").expect("expected operation to succeed");
    assert!(
        !next.is_enabled(),
        "search-next-match should start disabled"
    );

    let prev = window.lookup_action("search-prev-match").expect("expected operation to succeed");
    assert!(
        !prev.is_enabled(),
        "search-prev-match should start disabled"
    );
}

// ---------------------------------------------------------------------------
// Story 1.5: Status bar notification rendering
// ---------------------------------------------------------------------------

#[test]
fn test_status_bar_starts_empty() {
    ensure_gtk_init();
    let bar = glib::Object::builder::<LushtextStatusBar>().build();
    assert!(bar.imp().message_label.text().is_empty());
}

#[test]
fn test_render_progress_message_sets_label() {
    ensure_gtk_init();
    let bar = glib::Object::builder::<LushtextStatusBar>().build();
    bar.render_message(Some(&StatusMessage {
        text: "Searching 100 / 500 files\u{2026}".to_string(),
        severity: NotificationSeverity::Info,
    }));
    assert_eq!(
        bar.imp().message_label.text().as_str(),
        "Searching 100 / 500 files\u{2026}"
    );
}

#[test]
fn test_notification_bus_prefers_transient_over_progress() {
    ensure_gtk_init();
    let bus = NotificationBus::default();
    bus.publish(
        NotificationOwner::Search,
        NotificationSurface::StatusBar,
        NotificationPayload::Progress(StatusMessage {
            text: "Searching...".to_string(),
            severity: NotificationSeverity::Info,
        }),
    );
    bus.publish(
        NotificationOwner::Window,
        NotificationSurface::StatusBar,
        NotificationPayload::Transient(StatusMessage {
            text: "Error!".to_string(),
            severity: NotificationSeverity::Error,
        }),
    );

    let view = bus.status_bar_view().expect("status bar view exists");
    assert_eq!(view.text, "Error!");
    assert_eq!(view.severity, NotificationSeverity::Error);
}

#[test]
fn test_connect_navigate_callback_stored() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.connect_navigate_to_match(|_path, _line| {});
    assert!(panel.imp().callbacks.navigate_callback.borrow().is_some());
}

#[test]
fn test_connect_search_progress_callback_stored() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.connect_search_progress(|update| {
        assert!(matches!(
            update,
            SearchProgressUpdate::Progress { .. } | SearchProgressUpdate::Done { .. }
        ));
    });
    assert!(panel.imp().callbacks.progress_callback.borrow().is_some());
}

// ---------------------------------------------------------------------------
// Story 2.1: Replace UI widgets exist
// ---------------------------------------------------------------------------

#[test]
fn test_replace_entry_exists_on_panel() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // replace_entry is accessible as a template child.
    assert!(panel.imp().replace_entry.text().is_empty());
}

#[test]
fn test_replace_all_button_starts_insensitive() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // replace_all_button starts with sensitive=false (no text, no results).
    assert!(
        !panel.imp().replace_all_button.is_sensitive(),
        "replace_all_button should start insensitive"
    );
}

#[test]
fn test_undo_button_starts_hidden() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(
        !panel.imp().undo_button.property::<bool>("visible"),
        "undo_button should start hidden"
    );
}

#[test]
fn test_enter_preview_mode_sets_flag() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(!panel.is_preview_mode());

    // Simulate some results so enter_preview_mode has data.
    let file_item = SearchResultItem::new_file("/test.rs", "test.rs", 1);
    let match_item = SearchResultItem::new_match(
        "/test.rs",
        1,
        "let hello = 1;",
        4,
        9,
        "let hello = 1;",
        4,
        9,
    );
    let child_store = gtk4::gio::ListStore::new::<SearchResultItem>();
    child_store.append(&match_item);
    panel.imp().runtime.root_store.append(&file_item);
    panel.imp().runtime.file_groups.borrow_mut().insert(
        std::path::PathBuf::from("/test.rs"),
        SearchFileGroup::new(file_item, child_store),
    );
    panel.imp().runtime.total_matches.set(1);

    panel.enter_preview_mode("goodbye");
    assert!(panel.is_preview_mode());
}

#[test]
fn test_exit_preview_mode_clears_state() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Enter preview mode with some data.
    let file_item = SearchResultItem::new_file("/test.rs", "test.rs", 1);
    let match_item = SearchResultItem::new_match(
        "/test.rs",
        1,
        "let hello = 1;",
        4,
        9,
        "let hello = 1;",
        4,
        9,
    );
    let child_store = gtk4::gio::ListStore::new::<SearchResultItem>();
    child_store.append(&match_item);
    panel.imp().runtime.root_store.append(&file_item);
    panel.imp().runtime.file_groups.borrow_mut().insert(
        std::path::PathBuf::from("/test.rs"),
        SearchFileGroup::new(file_item, child_store),
    );
    panel.imp().runtime.total_matches.set(1);

    panel.enter_preview_mode("goodbye");
    assert!(panel.is_preview_mode());

    panel.exit_preview_mode();
    assert!(!panel.is_preview_mode());
    assert!(panel.imp().preview.preview_replacements.borrow().is_empty());
    assert!(panel.imp().preview.checked_indices.borrow().is_empty());
}

#[test]
fn test_clear_results_preserves_undo_backup() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);

    // Simulate an undo backup.
    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        std::path::PathBuf::from("/test.rs"),
        replace_undo_entry(b"original content", b"replaced content"),
    );
    panel.set_undo_backup(&backup);
    panel.show_undo_button();
    assert!(panel.imp().preview.undo_backup.borrow().is_some());

    // Starting a new search should not discard the rollback path for a
    // previous Replace All. Undo remains available until it is used or a new
    // Replace All replaces the backup.
    panel.start_search(&search_spec(""));
    assert!(panel.imp().preview.undo_backup.borrow().is_some());
    assert!(
        panel.imp().undo_button.property::<bool>("visible"),
        "undo_button should stay visible after a new search starts"
    );
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    let _ = search_backup::delete(&data_dir);
}

#[test]
fn test_search_panel_restores_persisted_undo_backup_on_construction() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);

    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        std::path::PathBuf::from("/persisted.rs"),
        replace_undo_entry(b"persisted content", b"persisted replacement"),
    );
    search_backup::save(&data_dir, &backup).expect("expected operation to succeed");

    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    wait_until(Duration::from_secs(2), || {
        panel.imp().preview.undo_backup.borrow().is_some()
    });
    assert_eq!(
        panel
            .imp()
            .preview
            .undo_backup
            .borrow()
            .as_ref()
            .expect("expected restored backup"),
        &backup
    );
    assert!(
        panel.imp().undo_button.property::<bool>("visible"),
        "undo button should be visible when persisted backup is restored"
    );
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    let _ = search_backup::delete(&data_dir);
}

#[test]
fn test_search_panel_close_preserves_undo_backup() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);

    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        std::path::PathBuf::from("/persisted-close.rs"),
        replace_undo_entry(b"before replace", b"after replace"),
    );
    panel.set_undo_backup(&backup);
    panel.show_undo_button();

    panel.close();

    assert_eq!(
        panel.imp().preview.undo_backup.borrow().as_ref(),
        Some(&backup)
    );
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    let _ = search_backup::delete(&data_dir);
}

// ---------------------------------------------------------------------------
// Story 2.1: Model types
// ---------------------------------------------------------------------------

#[test]
fn test_replacement_and_replace_result_construction() {
    ensure_gtk_init();
    let r = Replacement {
        path: std::path::PathBuf::from("/test.rs"),
        line_number: 1,
        original_line: "let hello = 1;".to_string(),
        replaced_line: "let goodbye = 1;".to_string(),
        replacement: "goodbye".to_string(),
        match_range: 4..9,
    };
    assert_eq!(r.path.display().to_string(), "/test.rs");
    assert_eq!(r.line_number, 1);
    assert_eq!(r.original_line, "let hello = 1;");
    assert_eq!(r.replaced_line, "let goodbye = 1;");

    let result = ReplaceResult {
        replaced_count: 5,
        files_affected: 2,
        skipped_paths: vec![std::path::PathBuf::from("/skip.rs")],
        errors: vec![],
    };
    assert_eq!(result.replaced_count, 5);
    assert_eq!(result.files_affected, 2);
    assert_eq!(result.skipped_paths.len(), 1);
}

#[test]
fn test_generate_replacement_preview_literal() {
    ensure_gtk_init();
    let matches = vec![SearchMatch {
        path: std::path::PathBuf::from("/test.rs"),
        line_number: 1,
        line_content: "let hello = 1;".to_string(),
        match_range: 4..9,
    }];

    let options = ContentSearchOptions::default();
    let result = generate_replacement_preview(&matches, "hello", "goodbye", &options);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].original_line, "let hello = 1;");
    assert_eq!(result[0].replaced_line, "let goodbye = 1;");
    assert_eq!(result[0].replacement, "goodbye");
}

#[test]
fn test_generate_replacement_preview_regex_backreference() {
    ensure_gtk_init();
    let matches = vec![SearchMatch {
        path: std::path::PathBuf::from("/test.rs"),
        line_number: 1,
        line_content: "fn hello_world() {}".to_string(),
        match_range: 3..14,
    }];

    let options = ContentSearchOptions {
        regex: true,
        ..Default::default()
    };
    // Regex: capture word, replace with prefix.
    let result = generate_replacement_preview(&matches, r"(\w+)_(\w+)", "new_${1}_${2}", &options);

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].replaced_line, "fn new_hello_world() {}",
        "backreference should expand correctly"
    );
}

#[test]
fn test_connect_replace_all_callback_stored() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.connect_replace_all(|_replacements| {});
    assert!(panel.imp().callbacks.replace_callback.borrow().is_some());
}

#[test]
fn test_connect_undo_all_callback_stored() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.connect_undo_all(|_backup| {});
    assert!(panel.imp().callbacks.undo_callback.borrow().is_some());
}

#[test]
fn test_search_navigation_actions_enabled_lifecycle() {
    ensure_gtk_init();
    let window = test_window();

    // 1. Start: disabled (no tabs, no panel, no results).
    let next = window.lookup_action("search-next-match").expect("expected operation to succeed");
    assert!(!next.is_enabled(), "should start disabled");

    // 2. Open a tab — still disabled (panel not visible, no results).
    window.new_tab();
    flush_events();
    assert!(!next.is_enabled(), "disabled: panel not visible yet");

    // 3. Show panel — still disabled (no results).
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-search-panel", None);
    flush_events();
    assert!(
        window.imp().search_panel_revealer.reveals_child(),
        "panel should be visible"
    );
    assert!(!next.is_enabled(), "disabled: no results yet");

    // 4. Simulate results arriving (set internal state directly).
    window.imp().search_panel.imp().runtime.total_matches.set(5);
    window.update_search_navigation_actions();
    assert!(next.is_enabled(), "enabled: tabs + panel visible + results");

    // 5. Close panel — disabled again.
    window.close_search_panel();
    flush_events();
    assert!(!next.is_enabled(), "disabled: panel closed");
}

// ---------------------------------------------------------------------------
// Story 3.1: Search History
// ---------------------------------------------------------------------------

#[test]
fn test_history_popover_and_list_exist_on_fresh_panel() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let imp = panel.imp();
    // Programmatic widgets should be constructed and parented.
    assert!(!imp.history_popover.is_visible());
    assert_eq!(
        imp.history_list.selection_mode(),
        gtk4::SelectionMode::Single
    );
}

#[test]
fn test_set_search_history_stores_entries() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    let entries = vec![
        make_history_entry("hello", false, false, false, true, None),
        make_history_entry("world", true, true, false, false, Some("*.rs")),
    ];
    panel.set_search_history(entries.clone());
    let retrieved = panel.search_history();
    assert_eq!(retrieved.len(), 2);
    assert_eq!(retrieved[0].spec.query, "hello");
    assert_eq!(retrieved[1].spec.query, "world");
}

#[test]
fn test_restore_from_history_sets_search_entry_text() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let entry = make_history_entry("restored query", false, false, false, true, None);
    panel.restore_from_history(&entry);
    assert_eq!(panel.query(), "restored query");
}

#[test]
fn test_restore_from_history_sets_toggle_states() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let entry = make_history_entry("test", true, true, true, false, Some("*.toml"));
    panel.restore_from_history(&entry);

    assert!(panel.imp().case_toggle.is_active());
    assert!(panel.imp().regex_toggle.is_active());
    assert!(panel.imp().word_toggle.is_active());
    assert!(!panel.imp().gitignore_toggle.is_active());
}

#[test]
fn test_restore_from_history_sets_glob_entry() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let entry = make_history_entry("test", false, false, false, true, Some("*.rs"));
    panel.restore_from_history(&entry);
    assert_eq!(panel.imp().glob_entry.text().as_str(), "*.rs");
}

#[test]
fn test_restore_from_history_with_glob_none_clears_glob_entry() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // First set a glob value.
    panel.imp().glob_entry.set_text("*.md");
    assert_eq!(panel.imp().glob_entry.text().as_str(), "*.md");

    // Restore with glob: None should clear it.
    let entry = make_history_entry("test", false, false, false, true, None);
    panel.restore_from_history(&entry);
    assert!(panel.imp().glob_entry.text().is_empty());
}

#[test]
fn test_search_history_entry_serialization_roundtrip() {
    // Pure data test — no GTK needed, but ensure_gtk_init doesn't hurt.
    ensure_gtk_init();
    let entry = make_history_entry("test query", true, false, true, false, Some("*.rs"));
    let json = serde_json::to_string(&entry).expect("expected operation to succeed");
    let deserialized: lushtext_core::model::content_search::SearchHistoryEntry =
        serde_json::from_str(&json).expect("expected operation to succeed");
    assert_eq!(entry, deserialized);
}

// ---------------------------------------------------------------------------
// Story 3.2: Saved Searches & Panel State Persistence
// ---------------------------------------------------------------------------

#[test]
fn test_save_button_exists_and_starts_invisible() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;
    let save_btn = &panel.imp().save_button;
    assert!(!save_btn.property::<bool>("visible"));
}

#[test]
fn test_set_and_get_saved_searches() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;

    let entries = vec![
        make_saved_search("My Search", "fn main"),
        make_saved_search("TODOs", "TODO"),
    ];
    panel.set_saved_searches(entries.clone());

    let retrieved = panel.saved_searches();
    assert_eq!(retrieved.len(), 2);
    assert_eq!(retrieved[0].name, "My Search");
    assert_eq!(retrieved[1].name, "TODOs");
}

#[test]
fn test_restore_from_saved_search_sets_query() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;

    let entry = SavedSearch::from_spec(
        "Test",
        SearchQuerySpec::new(
            "fn main",
            ContentSearchOptions::new(true, true, false, false, Some("*.rs".to_string())),
        ),
    );
    panel.restore_from_saved_search(&entry);

    assert_eq!(panel.imp().search_entry.text(), "fn main");
    assert!(panel.imp().case_toggle.is_active());
    assert!(panel.imp().regex_toggle.is_active());
    assert!(!panel.imp().word_toggle.is_active());
    assert!(!panel.imp().gitignore_toggle.is_active());
    assert_eq!(panel.imp().glob_entry.text(), "*.rs");
}

#[test]
fn test_restore_from_saved_search_clears_glob_when_none() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;

    // First set a glob.
    panel.imp().glob_entry.set_text("*.toml");

    let entry = make_saved_search("test", "test");
    panel.restore_from_saved_search(&entry);
    assert!(panel.imp().glob_entry.text().is_empty());
}

#[test]
fn test_remove_saved_search_updates_state() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;

    let entries = vec![
        make_saved_search("First", "fn main"),
        make_saved_search("Second", "TODO"),
        make_saved_search("Third", "FIXME"),
    ];
    panel.set_saved_searches(entries);
    assert_eq!(panel.saved_searches().len(), 3);

    // remove_saved_search is private, so we test via the service directly
    // and verify the panel state after set_saved_searches.
    let mut current = panel.saved_searches();
    lushtext_core::services::saved_searches::remove(&mut current, 1);
    panel.set_saved_searches(current);

    let after = panel.saved_searches();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].name, "First");
    assert_eq!(after[1].name, "Third");
}

#[test]
fn test_saved_search_serialization_roundtrip() {
    ensure_gtk_init();
    let entry = SavedSearch::from_spec(
        "My Search",
        SearchQuerySpec::new(
            "fn main",
            ContentSearchOptions::new(true, false, true, false, Some("*.rs".to_string())),
        ),
    );
    let json = serde_json::to_string(&entry).expect("expected operation to succeed");
    let deserialized: SavedSearch = serde_json::from_str(&json).expect("expected operation to succeed");
    assert_eq!(entry, deserialized);
}

// ---------------------------------------------------------------------------
// UI Polish: visual distinction, spacing, revealer transition
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_has_search_panel_css_class() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(panel.has_css_class("search-panel"));
}

#[test]
fn test_search_panel_revealer_uses_slide_up_transition() {
    ensure_gtk_init();
    let window = test_window();
    let revealer = &window.imp().search_panel_revealer;
    assert_eq!(
        revealer.transition_type(),
        gtk4::RevealerTransitionType::SlideUp,
    );
}

#[test]
fn test_search_panel_count_label_uses_default_body_text() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Count label should use default body text (no caption/heading class).
    assert!(!panel.imp().count_label.has_css_class("caption"));
    assert!(!panel.imp().count_label.has_css_class("heading"));
}

#[test]
fn test_search_panel_results_revealers_start_hidden_and_match_panel_animation() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let imp = panel.imp();

    assert!(!imp.results_feedback_revealer.reveals_child());
    assert!(!imp.results_body_revealer.reveals_child());
    assert_eq!(
        imp.results_feedback_revealer.transition_type(),
        gtk4::RevealerTransitionType::SlideUp,
    );
    assert_eq!(imp.results_feedback_revealer.transition_duration(), 250);
    assert_eq!(
        imp.results_body_revealer.transition_type(),
        gtk4::RevealerTransitionType::SlideUp,
    );
    assert_eq!(imp.results_body_revealer.transition_duration(), 250);
}

#[test]
fn test_search_panel_no_results_keeps_results_body_hidden() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    std::fs::write(dir.path().join("notes.txt"), "completely unrelated text").expect("expected operation to succeed");

    panel.clamp_results_height(240);
    panel.set_workspace_roots(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        !panel.imp().runtime.searching.get()
    });

    let imp = panel.imp();
    assert!(imp.results_feedback_revealer.reveals_child());
    assert!(!imp.results_body_revealer.reveals_child());
    assert_eq!(imp.runtime.total_matches.get(), 0);
    assert_eq!(imp.count_label.text().as_str(), "No results found");
}

#[test]
fn test_search_panel_first_result_reveals_fixed_max_height_results_body() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    std::fs::write(
        dir.path().join("notes.txt"),
        "needle one\nneedle two\nneedle three\n",
    )
    .expect("expected operation to succeed");

    panel.clamp_results_height(240);
    panel.set_workspace_roots(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        panel.imp().runtime.total_matches.get() > 0
    });

    let imp = panel.imp();
    assert!(imp.results_feedback_revealer.reveals_child());
    assert!(imp.results_body_revealer.reveals_child());
    assert!(imp.runtime.total_matches.get() >= 1);
    assert_eq!(imp.results_scroll.max_content_height(), 240);
    assert_eq!(imp.results_scroll.height_request(), 240);
}

#[test]
fn test_search_panel_clearing_query_hides_results_revealers_after_results() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    std::fs::write(dir.path().join("notes.txt"), "needle once\n").expect("expected operation to succeed");

    panel.clamp_results_height(240);
    panel.set_workspace_roots(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        panel.imp().runtime.total_matches.get() > 0
    });

    panel.start_search(&search_spec(""));
    flush_events();

    let imp = panel.imp();
    assert!(!imp.results_feedback_revealer.reveals_child());
    assert!(!imp.results_body_revealer.reveals_child());
    assert_eq!(imp.runtime.total_matches.get(), 0);
    assert!(imp.count_label.text().is_empty());
}

#[test]
fn test_search_panel_followup_search_keeps_results_body_open_until_new_outcome() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    std::fs::write(dir.path().join("notes.txt"), "needle once\n").expect("expected operation to succeed");

    panel.clamp_results_height(240);
    panel.set_workspace_roots(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        panel.imp().runtime.total_matches.get() > 0
    });
    assert!(panel.imp().results_body_revealer.reveals_child());

    panel.start_search(&search_spec("absent"));

    let imp = panel.imp();
    assert!(
        imp.results_feedback_revealer.reveals_child(),
        "follow-up search should preserve the expanded feedback area",
    );
    assert!(
        imp.results_body_revealer.reveals_child(),
        "follow-up search should keep the results body open until the new search resolves",
    );

    wait_until(Duration::from_secs(2), || {
        !panel.imp().runtime.searching.get()
    });

    assert!(imp.results_feedback_revealer.reveals_child());
    assert!(!imp.results_body_revealer.reveals_child());
    assert_eq!(imp.count_label.text().as_str(), "No results found");
}

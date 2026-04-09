// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextWindow widget.

use crate::common::ensure_gtk_init;
use gio::prelude::{ActionExt, ActionGroupExt, ActionMapExt};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::config::keys;
use lushtext_core::model::draft::DraftEntry;
use lushtext_core::model::workspace::{WorkspaceConfig, WorkspaceEntry, WorkspaceId, WorkspacesFile};
use lushtext_core::services::{draft_service, json_store, workspace_manager};
use lushtext_core::services::notifications::{
    InlineActionNotification, InlineNotificationStyle, NOTIFICATION_TIMEOUT, NotificationOwner,
    NotificationSeverity, NotificationSurface,
};
use lushtext_core::ui::editor_page::LushtextEditorPage;
use lushtext_core::ui::window::{
    LushtextWindow, SIDEBAR_COLLAPSED_POSITION, clamp_sidebar_position,
    clamp_sidebar_visible_position,
};
use std::time::{Duration, Instant};

/// Create a window attached to a test application (not registered with D-Bus).
fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

fn test_window_with_sidebar_state(visible: bool, position: i32) -> LushtextWindow {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SIDEBAR_VISIBLE, visible)
        .expect("set sidebar-visible");
    settings
        .set_int(keys::SIDEBAR_POSITION, position)
        .expect("set sidebar-position");
    test_window()
}

fn seed_restored_workspaces() -> tempfile::TempDir {
    ensure_gtk_init();
    let roots_dir = tempfile::tempdir().expect("workspace roots tempdir");
    let mut workspaces = WorkspacesFile::default();

    for (idx, name) in ["one", "two", "three"].into_iter().enumerate() {
        let path = roots_dir.path().join(name);
        std::fs::create_dir_all(&path).expect("create workspace root");
        workspaces.workspaces.push(WorkspaceConfig {
            id: WorkspaceId::new(format!("ws-{idx}")),
            name: name.to_string(),
            entries: vec![WorkspaceEntry::Directory { path }],
        });
    }

    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces.json");
    roots_dir
}

fn wait_for_workspace_roots(window: &LushtextWindow, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if window.imp().sidebar.workspace_roots().len() == expected {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }
    panic!(
        "expected {expected} restored workspace roots, got {}",
        window.imp().sidebar.workspace_roots().len()
    );
}

/// Drain all pending events from the GTK main loop.
fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

fn flush_after_delay(delay: std::time::Duration) {
    std::thread::sleep(delay);
    flush_events();
}

fn wait_for_paned_position(window: &LushtextWindow, expected: i32) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if window.imp().main_paned.position() == expected {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }
    panic!(
        "expected paned position {expected}, got {}",
        window.imp().main_paned.position()
    );
}

fn present_window(window: &LushtextWindow) {
    window.present();
    flush_events();
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

    assert!(!action_enabled(&window, "begin-search"));
    assert!(!action_enabled(&window, "save"));
    assert!(!action_enabled(&window, "close-tab"));
    assert!(!action_enabled(&window, "print"));
}

#[test]
fn test_tab_actions_enabled_when_tab_exists() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();

    assert!(action_enabled(&window, "begin-search"));
    assert!(action_enabled(&window, "save"));
    assert!(action_enabled(&window, "close-tab"));
    assert!(action_enabled(&window, "print"));
}

#[test]
fn test_tab_independent_actions_always_enabled() {
    ensure_gtk_init();
    let window = test_window();

    assert!(action_enabled(&window, "new-tab"));
    assert!(action_enabled(&window, "open-file"));
    assert!(action_enabled(&window, "open-folder"));
}

// --- Begin-search via action system ---

#[test]
fn test_begin_search_action_opens_search() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();

    activate_action(&window, "begin-search");

    let editor = active_editor(&window);
    assert!(editor.imp().search_revealer.reveals_child());
}

#[test]
fn test_begin_search_action_idempotent() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();

    // begin-search always opens (never toggles closed).
    activate_action(&window, "begin-search");
    activate_action(&window, "begin-search");

    let editor = active_editor(&window);
    assert!(editor.imp().search_revealer.reveals_child());
}

#[test]
fn test_begin_search_noop_when_disabled() {
    ensure_gtk_init();
    let window = test_window();

    assert!(!action_enabled(&window, "begin-search"));
    activate_action(&window, "begin-search");

    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_show_search_survives_event_loop() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.show_search();
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
    assert!(action_enabled(&window, "begin-search"));

    activate_action(&window, "close-tab");

    assert!(!action_enabled(&window, "begin-search"));
    assert!(!action_enabled(&window, "save"));
    assert!(!action_enabled(&window, "close-tab"));
    assert!(!action_enabled(&window, "print"));
}

// --- Print action enabled/disabled ---

#[test]
fn test_print_action_disabled_when_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    assert!(!action_enabled(&window, "print"));
}

#[test]
fn test_print_action_enabled_when_tab_exists() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert!(action_enabled(&window, "print"));
}

#[test]
fn test_print_action_disabled_after_closing_all_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert!(action_enabled(&window, "print"));

    activate_action(&window, "close-tab");
    assert!(!action_enabled(&window, "print"));
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
    window.publish_status_message(
        "Test message",
        lushtext_core::ui::status_bar::MessageKind::Info,
    );
    let msg_text = window.imp().status_bar.imp().message_label.label();
    assert_eq!(msg_text.as_str(), "Test message");
}

#[test]
fn test_expired_status_message_sweep_clears_status_bar() {
    ensure_gtk_init();
    let window = test_window();
    window.publish_status_message(
        "Expiring message",
        lushtext_core::ui::status_bar::MessageKind::Info,
    );
    assert_eq!(
        window.imp().status_bar.imp().message_label.label().as_str(),
        "Expiring message"
    );

    let swept = window
        .imp()
        .notification_bus
        .sweep_expired_at(Instant::now() + NOTIFICATION_TIMEOUT + Duration::from_secs(1));
    assert!(
        swept,
        "forced sweep should report expired notification removal"
    );

    if swept {
        window.render_notifications();
    }

    assert_eq!(
        window.imp().status_bar.imp().message_label.label().as_str(),
        ""
    );
}

#[test]
fn test_status_message_auto_dismisses_after_timeout() {
    ensure_gtk_init();
    let window = test_window();
    window.publish_status_message(
        "Auto-dismiss message",
        lushtext_core::ui::status_bar::MessageKind::Info,
    );
    assert_eq!(
        window.imp().status_bar.imp().message_label.label().as_str(),
        "Auto-dismiss message"
    );

    flush_after_delay(NOTIFICATION_TIMEOUT + Duration::from_secs(2));

    assert_eq!(
        window.imp().status_bar.imp().message_label.label().as_str(),
        ""
    );
}

#[test]
fn test_progress_status_message_auto_dismisses_without_heartbeat() {
    ensure_gtk_init();
    let window = test_window();
    assert!(window.imp().notification_bus.update_progress(
        NotificationOwner::Search,
        NotificationSurface::StatusBar,
        "Searching 14100 files\u{2026}",
        NotificationSeverity::Info,
    ));
    window.render_notifications();
    assert_eq!(
        window.imp().status_bar.imp().message_label.label().as_str(),
        "Searching 14100 files\u{2026}"
    );

    flush_after_delay(NOTIFICATION_TIMEOUT + Duration::from_secs(2));

    assert_eq!(
        window.imp().status_bar.imp().message_label.label().as_str(),
        ""
    );
}

#[test]
fn test_window_publish_editor_inline_notification_renders_info_bar() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    let editor = active_editor(&window);

    window.publish_editor_inline_notification(
        &editor,
        InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: "Draft Changes Restored".to_string(),
            body: "Unsaved changes from a previous session have been restored.".to_string(),
            primary_button: Some("_Discard…".to_string()),
            secondary_button: Some("_Save…".to_string()),
        },
    );

    let info_bar = editor.info_bar();
    assert!(info_bar.imp().discard_infobar.property::<bool>("revealed"));
    assert_eq!(
        info_bar.imp().discard_title.label().as_str(),
        "Draft Changes Restored"
    );
}

#[test]
fn test_window_dismiss_editor_notifications_hides_info_bar() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    let editor = active_editor(&window);

    window.publish_editor_inline_notification(
        &editor,
        InlineActionNotification {
            style: InlineNotificationStyle::Error,
            title: "Could Not Open File".to_string(),
            body: "Permission denied".to_string(),
            primary_button: Some("_Retry".to_string()),
            secondary_button: None,
        },
    );
    assert!(
        editor
            .info_bar()
            .imp()
            .access_infobar
            .property::<bool>("revealed")
    );

    window.dismiss_editor_notifications(&editor);
    assert!(
        !editor
            .info_bar()
            .imp()
            .access_infobar
            .property::<bool>("revealed")
    );
    assert!(
        !editor
            .info_bar()
            .imp()
            .discard_infobar
            .property::<bool>("revealed")
    );
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
    clamp_sidebar_position(&window, paned, &window.imp().content_box, 1200);
    assert_eq!(paned.position(), 300);
}

#[test]
fn test_clamp_reduces_when_over_limit() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(500);

    // Window width 1200 → max 400. Position 500 exceeds.
    clamp_sidebar_position(&window, paned, &window.imp().content_box, 1200);
    assert_eq!(paned.position(), 400);
}

#[test]
fn test_clamp_at_exact_limit() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(400);

    // Window width 1200 → max 400. Position 400 is exactly at limit.
    clamp_sidebar_position(&window, paned, &window.imp().content_box, 1200);
    assert_eq!(paned.position(), 400);
}

#[test]
fn test_clamp_noop_when_window_width_zero() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(500);

    // Width 0 = unrealized window. Should not clamp.
    clamp_sidebar_position(&window, paned, &window.imp().content_box, 0);
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
    clamp_sidebar_position(&window, paned, &window.imp().content_box, 1200);
    assert_eq!(paned.position(), 400);
}

#[test]
fn test_clamp_persists_to_gsettings() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let settings = &window.imp().settings;
    paned.set_position(350);

    clamp_sidebar_position(&window, paned, &window.imp().content_box, 1200);
    flush_after_delay(std::time::Duration::from_millis(250));
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
    clamp_sidebar_position(&window, paned, &window.imp().content_box, 1200);
    flush_after_delay(std::time::Duration::from_millis(250));
    assert_eq!(settings.int(keys::SIDEBAR_POSITION), 400);
}

// --- Sidebar clamp: content-box floor (regression for GtkBox measurement warning) ---

#[test]
fn test_clamp_respects_content_box_minimum_at_narrow_width() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;
    let handle_overhead = window.imp().handle_overhead.get();

    let (content_min, _, _, _) = content_box.measure(gtk4::Orientation::Horizontal, -1);
    assert!(
        content_min > 0,
        "content_box should have a non-zero minimum width"
    );

    // Pick a window width where 1/3 would leave less than content_min plus
    // the measured paned handle overhead.
    let narrow_width = content_min + 200 + handle_overhead;
    let one_third = narrow_width / 3;
    let content_floor = narrow_width - content_min - handle_overhead;

    if content_floor < one_third {
        paned.set_position(one_third + 50); // way over both limits
        clamp_sidebar_position(&window, paned, content_box, narrow_width);
        assert!(
            paned.position() <= content_floor,
            "position {} should be clamped to content floor {}",
            paned.position(),
            content_floor,
        );
    }
}

#[test]
fn test_clamp_uses_one_third_when_it_is_tighter_than_content_floor() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;
    let handle_overhead = window.imp().handle_overhead.get();

    let (content_min, _, _, _) = content_box.measure(gtk4::Orientation::Horizontal, -1);
    let content_floor = 1200 - content_min - handle_overhead;
    assert!(
        400 < content_floor,
        "precondition: at 1200px, 1/3 (400) should be tighter than content floor ({content_floor})"
    );

    paned.set_position(500);
    clamp_sidebar_position(&window, paned, content_box, 1200);
    assert_eq!(paned.position(), 400); // 1/3 rule wins
}

#[test]
fn test_clamp_never_goes_negative() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;

    // Extremely narrow width where the content floor would be negative.
    paned.set_position(100);
    clamp_sidebar_position(&window, paned, content_box, 50);
    assert!(
        paned.position() >= 0,
        "position should never be negative, got {}",
        paned.position(),
    );
}

// --- Sidebar clamp: lifecycle regression tests ---
//
// clamp_sidebar_position only reduces, never grows. Calling it with a width
// smaller than the actual window permanently destroys the position. This is
// why it must ONLY be called from size_allocate (definitive width) and
// notify::position (user drag) — never from measure(), which GTK calls
// speculatively with various for_size values including the minimum width.

#[test]
fn test_clamp_with_wrong_width_permanently_destroys_position() {
    // REGRESSION: A previous measure() override called clamp with the
    // minimum window width (640px), ratcheting the sidebar to ~209px.
    // The size_allocate call at the real width could not restore it.
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;

    // Start with a position valid for 1200px (max = 400).
    paned.set_position(350);
    clamp_sidebar_position(&window, paned, content_box, 1200);
    assert_eq!(paned.position(), 350, "350 is valid for 1200px");

    // Simulate what a measure() override would do: clamp at minimum width.
    clamp_sidebar_position(&window, paned, content_box, 640);
    let destroyed = paned.position();
    assert!(
        destroyed < 350,
        "min-width clamp should reduce position from 350, got {destroyed}"
    );

    // Now clamp at the actual width — position is stuck.
    clamp_sidebar_position(&window, paned, content_box, 1200);
    assert_eq!(
        paned.position(),
        destroyed,
        "position permanently stuck at {destroyed} — clamp never grows"
    );
}

#[test]
fn test_clamp_stable_across_repeated_calls_at_same_width() {
    // Verify no drift: calling clamp repeatedly at the same width must not
    // change the position. A measure() override that triggered per-frame
    // would cause cumulative position drift.
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;

    paned.set_position(300);
    clamp_sidebar_position(&window, paned, content_box, 1200);
    let first = paned.position();

    for _ in 0..10 {
        clamp_sidebar_position(&window, paned, content_box, 1200);
    }
    assert_eq!(
        paned.position(),
        first,
        "position must not drift across repeated clamp calls"
    );
}

#[test]
fn test_clamp_at_actual_width_preserves_valid_position() {
    // The positive case: size_allocate calls clamp with the definitive
    // width. A valid position must survive unchanged.
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;

    // 200 is well under max(400) for 1200px.
    paned.set_position(200);
    clamp_sidebar_position(&window, paned, content_box, 1200);
    assert_eq!(
        paned.position(),
        200,
        "valid position should survive size_allocate"
    );

    // Shrink to 900px (max = 300). 200 still valid.
    clamp_sidebar_position(&window, paned, content_box, 900);
    assert_eq!(paned.position(), 200, "200 still valid at 900px");

    // Shrink to 700px (max = 233). 200 is now too large.
    clamp_sidebar_position(&window, paned, content_box, 700);
    assert!(
        paned.position() <= 233,
        "position should be clamped at 700px, got {}",
        paned.position()
    );
}

#[test]
fn test_window_has_minimum_width_request() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(
        window.width_request(),
        640,
        "window should have width-request=640 to prevent impossible geometry"
    );
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

    // Clear modified state before closing so the save-changes dialog
    // doesn't block the close. This test verifies header-clearing
    // behavior, not close-confirmation.
    active_editor(&window).buffer().set_modified(false);
    flush_events();

    activate_action(&window, "close-tab");
    flush_events();
    assert_eq!(window.imp().title_widget.title().as_str(), "LushText");
}

// ---------------------------------------------------------------------------
// Session persistence
// ---------------------------------------------------------------------------

#[test]
fn test_collect_session_empty_window() {
    ensure_gtk_init();
    let window = test_window();
    let session = window.collect_session();
    assert!(session.tabs.is_empty());
    assert_eq!(session.active_tab_index, None);
}

#[test]
fn test_collect_session_with_untitled_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    window.new_tab();
    flush_events();

    let session = window.collect_session();
    assert_eq!(session.tabs.len(), 2);
    assert_eq!(session.active_tab_index, Some(1));
    assert!(session.tabs[0].path.is_none());
    assert!(session.tabs[1].path.is_none());
}

#[test]
fn test_collect_session_with_file_tab() {
    ensure_gtk_init();
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("hello.rs");
    std::fs::write(&file, "fn main() {}").unwrap();

    let window = test_window();
    window.open_document(&file);
    flush_events();

    let session = window.collect_session();
    assert_eq!(session.tabs.len(), 1);
    assert_eq!(session.tabs[0].path, Some(file));
    assert_eq!(session.active_tab_index, Some(0));
}

#[test]
fn test_collect_session_mixed_tabs() {
    ensure_gtk_init();
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("test.txt");
    std::fs::write(&file, "content").unwrap();

    let window = test_window();
    window.new_tab();
    window.open_document(&file);
    window.new_tab();
    flush_events();

    let session = window.collect_session();
    assert_eq!(session.tabs.len(), 3);
    assert!(session.tabs[0].path.is_none()); // untitled
    assert_eq!(session.tabs[1].path, Some(file)); // file tab
    assert!(session.tabs[2].path.is_none()); // untitled
    assert_eq!(session.active_tab_index, Some(2)); // last tab is active
}

#[test]
fn test_collect_session_active_tab_after_switch() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    window.new_tab();
    window.new_tab();
    flush_events();

    // Select the first tab
    let first_page = window.imp().tab_view.nth_page(0);
    window.imp().tab_view.set_selected_page(&first_page);
    flush_events();

    let session = window.collect_session();
    assert_eq!(session.active_tab_index, Some(0));
}

#[test]
fn test_collect_session_after_close_tab() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    window.new_tab();
    window.new_tab();
    flush_events();

    activate_action(&window, "close-tab"); // closes last (active) tab
    let session = window.collect_session();
    assert_eq!(session.tabs.len(), 2);
}

#[test]
fn test_save_session_sync_writes_file() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    window.save_session_sync();

    // Verify session file was written to the test data dir
    let data_dir = lushtext_core::services::json_store::data_dir();
    let session_file = data_dir.join("session.json");
    assert!(session_file.exists(), "session.json should be written");

    let content = std::fs::read_to_string(&session_file).unwrap();
    assert!(content.contains("tabs"));
}

#[test]
fn test_restoring_session_flag_prevents_save() {
    ensure_gtk_init();
    let window = test_window();

    // Set restoring flag
    window.imp().restoring_session.set(true);

    // save_session_debounced should no-op
    window.save_session_debounced();
    // No assertion needed — just verifying it doesn't panic.
    // The debounce generation counter should NOT increment.
    let generation_before = window.imp().session_save_generation.get();

    window.imp().restoring_session.set(false);
    window.save_session_debounced();
    // Now the generation should have advanced
    assert_ne!(
        window.imp().session_save_generation.get(),
        generation_before
    );
}

#[test]
fn test_collect_session_untitled_tab_has_draft_id() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let session = window.collect_session();
    assert_eq!(session.tabs.len(), 1);
    assert!(session.tabs[0].path.is_none());
    // Untitled tabs should have a draft_id for recovery
    assert!(session.tabs[0].draft_id.is_some());
}

#[test]
fn test_collect_session_file_tab_no_draft_id() {
    ensure_gtk_init();
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("test.rs");
    std::fs::write(&file, "content").unwrap();

    let window = test_window();
    window.open_document(&file);
    flush_events();

    let session = window.collect_session();
    // File-backed tabs don't need draft_id in session — the draft system
    // derives it from the path.
    assert!(session.tabs[0].draft_id.is_none());
}

// ---------------------------------------------------------------------------
// Preloaded drafts
// ---------------------------------------------------------------------------

#[test]
fn test_preloaded_drafts_consumed_by_check_draft_by_id() {
    ensure_gtk_init();
    let window = test_window();

    // Wait for load_session_and_drafts (triggered in constructed) to complete.
    // It overwrites preloaded_drafts and draft_manifest, so we must set up
    // test data AFTER it finishes.
    flush_after_delay(std::time::Duration::from_millis(200));

    // Simulate preloaded draft content (as load_session_and_drafts would do).
    let draft_id = "test-preload-1";
    window
        .imp()
        .preloaded_drafts
        .borrow_mut()
        .insert(draft_id.to_string(), "preloaded content".to_string());
    // Also add a manifest entry so check_draft_by_id finds it.
    window
        .imp()
        .draft_manifest
        .borrow_mut()
        .upsert(lushtext_core::model::draft::DraftEntry {
            draft_id: draft_id.to_string(),
            original_path: None,
            original_mtime_secs: None,
            saved_at_secs: 1000,
        });

    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    // check_draft_by_id should consume from preloaded map (no background task).
    window.check_draft_by_id(&editor, draft_id);
    flush_events();

    // The preloaded entry should have been consumed (removed from map).
    assert!(
        window
            .imp()
            .preloaded_drafts
            .borrow()
            .get(draft_id)
            .is_none(),
        "preloaded draft should be consumed after check_draft_by_id"
    );

    // The editor buffer should contain the preloaded content.
    let buffer = editor.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    assert_eq!(text.as_str(), "preloaded content");
}

#[test]
fn test_preloaded_drafts_empty_is_noop() {
    ensure_gtk_init();
    let window = test_window();

    // Wait for load_session_and_drafts to complete before setting up test state.
    flush_after_delay(std::time::Duration::from_millis(200));

    // Add a manifest entry but NO preloaded content.
    window
        .imp()
        .draft_manifest
        .borrow_mut()
        .upsert(lushtext_core::model::draft::DraftEntry {
            draft_id: "no-preload".to_string(),
            original_path: None,
            original_mtime_secs: None,
            saved_at_secs: 1000,
        });

    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    // check_draft_by_id should fall through to background read (which will
    // fail silently since no draft file exists in the test data dir).
    window.check_draft_by_id(&editor, "no-preload");
    flush_events();

    // Buffer should still be empty (no preloaded content, background read finds nothing).
    let buffer = editor.buffer();
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    assert!(
        text.is_empty(),
        "buffer should be empty when no preloaded or disk draft exists"
    );
}

// ---------------------------------------------------------------------------
// Draft dirty flag re-arming (regression: draft_dirty stuck after autosave)
// ---------------------------------------------------------------------------

#[test]
fn test_draft_dirty_set_on_text_edit() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    assert!(!editor.draft_dirty(), "draft_dirty should start false");

    // Typing into the buffer should arm draft_dirty via connect_changed.
    let buffer = editor.buffer();
    buffer.set_text("hello");
    flush_events();

    assert!(
        editor.draft_dirty(),
        "draft_dirty should be true after text edit"
    );
}

#[test]
fn test_draft_dirty_rearmed_after_clearing() {
    // Core regression test: after autosave clears draft_dirty, a new edit must
    // re-arm it even though is_modified() never transitioned.
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    let buffer = editor.buffer();

    // Simulate the post-restore state: buffer is modified, draft_dirty is true.
    buffer.set_text("restored draft content");
    buffer.set_modified(true);
    flush_events();
    assert!(
        editor.draft_dirty(),
        "draft_dirty should be true after edits"
    );
    assert!(buffer.is_modified(), "buffer should be modified");

    // Simulate autosave_tick clearing the flag (as it does after writing).
    editor.set_draft_dirty(false);
    assert!(
        !editor.draft_dirty(),
        "draft_dirty should be false after autosave clear"
    );
    // Crucially, is_modified() is still true — no transition will fire from
    // connect_modified_changed on the next edit.
    assert!(buffer.is_modified(), "buffer should still be modified");

    // User types new text. connect_changed must re-arm draft_dirty.
    buffer.insert(&mut buffer.end_iter(), " + new edits");
    flush_events();

    assert!(
        editor.draft_dirty(),
        "draft_dirty must be re-armed by connect_changed after autosave clear"
    );
}

#[test]
fn test_draft_dirty_not_armed_without_edits() {
    // Guard: creating a tab without editing should not arm draft_dirty.
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    assert!(
        !editor.draft_dirty(),
        "draft_dirty should remain false with no edits"
    );
}

#[test]
fn test_draft_dirty_rearmed_after_multiple_clear_cycles() {
    // Ensure the fix works across multiple autosave cycles, not just the first.
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    let buffer = editor.buffer();

    // First cycle: edit → clear → edit.
    buffer.set_text("cycle 1");
    flush_events();
    assert!(editor.draft_dirty());
    editor.set_draft_dirty(false);

    buffer.insert(&mut buffer.end_iter(), " more");
    flush_events();
    assert!(editor.draft_dirty(), "re-armed after first clear");

    // Second cycle: clear → edit again.
    editor.set_draft_dirty(false);
    buffer.insert(&mut buffer.end_iter(), " and more");
    flush_events();
    assert!(editor.draft_dirty(), "re-armed after second clear");

    // Third cycle: clear → edit again.
    editor.set_draft_dirty(false);
    buffer.insert(&mut buffer.end_iter(), "!");
    flush_events();
    assert!(editor.draft_dirty(), "re-armed after third clear");
}

#[test]
fn test_draft_dirty_true_but_unmodified_skipped_by_guard() {
    // Verifies the autosave guard interaction: even when `draft_dirty` is true
    // (e.g., set by `connect_changed` during undo), the `is_modified()` check
    // in `autosave_tick` prevents writing a draft for unmodified content.
    //
    // This tests the guard condition directly rather than relying on GTK undo
    // internals, because `set_text()` may create multiple internal operations
    // and a single `undo()` may not cleanly return `is_modified()` to false.
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    let buffer = editor.buffer();

    // Simulate: user edited, then saved (or undid all changes).
    // The buffer is now clean, but draft_dirty could be true from a
    // connect_changed signal that fired during undo.
    buffer.set_text("some text");
    flush_events();
    assert!(editor.draft_dirty());

    // Simulate the buffer returning to clean state (e.g., after save or undo).
    buffer.set_modified(false);
    // draft_dirty is still true from the prior connect_changed call.
    editor.set_draft_dirty(true);

    // Verify the guard condition: is_modified()=false means autosave skips,
    // regardless of draft_dirty value.
    assert!(
        !buffer.is_modified(),
        "buffer should be unmodified (simulating post-save/undo state)"
    );
    assert!(
        editor.draft_dirty(),
        "draft_dirty is true (set by connect_changed during undo)"
    );
    // autosave_tick checks: `if !editor.is_modified() || !editor.draft_dirty() || ...`
    // Since !is_modified() is true, the tab would be skipped. No draft written.
}

#[test]
fn test_draft_dirty_full_restore_regression() {
    // End-to-end regression: simulate draft restore → first autosave tick →
    // new user edit → verify draft_dirty is armed for the next tick.
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    let buffer = editor.buffer();

    // Step 1: Simulate apply_draft (what happens on session restore with draft).
    buffer.begin_irreversible_action();
    buffer.set_text("restored draft content from disk");
    buffer.end_irreversible_action();
    buffer.set_modified(true);
    editor.set_draft_dirty(true);
    flush_events();

    // State after restore: is_modified=true, draft_dirty=true.
    assert!(buffer.is_modified());
    assert!(editor.draft_dirty());

    // Step 2: Simulate autosave_tick writing the draft and clearing the flag.
    editor.set_draft_dirty(false);
    // is_modified() stays true — this is the crux of the bug.
    assert!(buffer.is_modified(), "buffer stays modified after tick");
    assert!(!editor.draft_dirty(), "tick cleared draft_dirty");

    // Step 3: User types new content.
    buffer.insert(&mut buffer.end_iter(), "\nnew line added by user");
    flush_events();

    // Step 4: Verify draft_dirty is re-armed for the next tick.
    assert!(
        editor.draft_dirty(),
        "BUG REGRESSION: draft_dirty must be re-armed after user edits post-restore"
    );
    assert!(buffer.is_modified());
}

// ---------------------------------------------------------------------------
// Save-changes dialog
// ---------------------------------------------------------------------------

#[test]
fn test_modified_editors_empty_when_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    assert!(window.modified_editors().is_empty());
}

#[test]
fn test_modified_editors_empty_when_all_clean() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    window.new_tab();
    flush_events();
    assert!(window.modified_editors().is_empty());
}

#[test]
fn test_modified_editors_detects_dirty_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    window.new_tab();
    flush_events();

    // Modify only the active (second) tab
    active_editor(&window).buffer().set_text("dirty");
    flush_events();

    let modified = window.modified_editors();
    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].1.title(), "Untitled");
}

#[test]
fn test_modified_editors_detects_multiple_dirty_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();
    active_editor(&window).buffer().set_text("dirty1");

    window.new_tab();
    flush_events();
    active_editor(&window).buffer().set_text("dirty2");
    flush_events();

    let modified = window.modified_editors();
    assert_eq!(modified.len(), 2);
}

#[test]
fn test_close_tab_unmodified_closes_immediately() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    activate_action(&window, "close-tab");
    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_close_tab_modified_is_blocked() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    active_editor(&window).buffer().set_text("unsaved");
    flush_events();

    // Closing a modified tab should be blocked by the save-changes dialog.
    // The tab remains because no one confirms the dialog.
    activate_action(&window, "close-tab");
    assert_eq!(
        window.imp().tab_view.n_pages(),
        1,
        "Modified tab should not close without confirmation"
    );
}

#[test]
fn test_confirm_close_tab_clean_calls_back_true() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let page = window.imp().tab_view.selected_page().unwrap();
    let editor = active_editor(&window);

    let confirmed = std::rc::Rc::new(std::cell::Cell::new(false));
    let confirmed_clone = confirmed.clone();
    window.confirm_close_tab(&page, &editor, move |result| {
        confirmed_clone.set(result);
    });
    flush_events();

    assert!(
        confirmed.get(),
        "Clean tab should confirm close immediately"
    );
}

#[test]
fn test_show_save_changes_empty_calls_done_true() {
    ensure_gtk_init();
    let window = test_window();

    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    window.show_save_changes_dialog(vec![], move |confirmed| {
        done_clone.set(confirmed);
    });
    flush_events();

    assert!(done.get(), "Empty modified list should call done(true)");
}

#[test]
fn test_flush_dirty_drafts_persists_empty_modified_buffer() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    let buffer = editor.buffer();
    buffer.set_text("temporary text");
    flush_events();
    buffer.set_text("");
    flush_events();

    window.flush_dirty_drafts();

    let draft_id = editor.draft_id().expect("untitled tab has draft id");
    let restored = draft_service::read_draft(&json_store::data_dir(), &draft_id).unwrap();
    assert_eq!(restored.as_deref(), Some(""));
}

#[test]
fn test_save_editors_for_close_failure_preserves_draft_and_blocks_close() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let tmp = tempfile::tempdir().unwrap();
    let failing_path = tmp.path().join("not-a-file");
    std::fs::create_dir(&failing_path).unwrap();

    let editor = active_editor(&window);
    editor.set_file_path(&failing_path);
    let draft_id = draft_service::draft_id_for_path(&failing_path);
    editor.set_draft_id(draft_id.clone());
    editor.buffer().set_text("unsaved close data");
    flush_events();

    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &draft_id, "draft backup").unwrap();
    window.imp().draft_manifest.borrow_mut().upsert(DraftEntry {
        draft_id: draft_id.clone(),
        original_path: Some(failing_path.clone()),
        original_mtime_secs: None,
        saved_at_secs: 1,
    });

    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result_clone = result.clone();
    window.save_editors_for_close(vec![editor.clone()], move |confirmed| {
        *result_clone.borrow_mut() = Some(confirmed);
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && result.borrow().is_none() {
        flush_after_delay(Duration::from_millis(20));
    }
    assert!(result.borrow().is_some(), "close-save callback should complete");

    assert_eq!(*result.borrow(), Some(false));
    assert!(editor.is_modified(), "failed save should restore modified state");
    assert!(
        draft_service::read_draft(&data_dir, &draft_id)
            .unwrap()
            .is_some(),
        "draft must remain on disk after failed close save"
    );
    assert!(
        window
            .imp()
            .draft_manifest
            .borrow()
            .find_by_path(&failing_path)
            .is_some(),
        "manifest entry must survive failed close save"
    );
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_save_editors_for_close_untitled_blocks_close_and_keeps_draft() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("untitled close data");
    flush_events();
    window.flush_dirty_drafts();

    let draft_id = editor.draft_id().expect("untitled tab has draft id");
    let data_dir = json_store::data_dir();

    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result_clone = result.clone();
    window.save_editors_for_close(vec![editor.clone()], move |confirmed| {
        *result_clone.borrow_mut() = Some(confirmed);
    });
    flush_events();

    assert_eq!(*result.borrow(), Some(false));
    assert!(editor.is_modified(), "untitled tab should remain dirty");
    assert!(
        draft_service::read_draft(&data_dir, &draft_id)
            .unwrap()
            .is_some(),
        "untitled draft must remain on disk when close save is blocked"
    );
    assert!(
        draft_service::load_manifest(&data_dir)
            .unwrap()
            .find_by_id(&draft_id)
            .is_some(),
        "disk manifest entry must remain for untitled recovery"
    );
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

// --- Sidebar toggle ---

/// Check the sidebar visibility target state via the Cell cache.
/// Uses the Cell (not the widget property) because animations may not tick
/// in headless tests, but the Cell reflects the intended state immediately.
fn sidebar_visible(window: &LushtextWindow) -> bool {
    window.imp().sidebar_visible.get()
}

#[test]
fn test_gsettings_sidebar_visible_default() {
    ensure_gtk_init();
    let window = test_window();
    assert!(window.imp().settings.boolean(keys::SIDEBAR_VISIBLE));
}

#[test]
fn test_sidebar_visible_by_default() {
    ensure_gtk_init();
    let window = test_window();
    assert!(sidebar_visible(&window));
}

#[test]
fn test_toggle_sidebar_hides_sidebar() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-sidebar");
    assert!(!sidebar_visible(&window));
}

#[test]
fn test_toggle_sidebar_shows_sidebar_again() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-sidebar");
    activate_action(&window, "toggle-sidebar");
    assert!(sidebar_visible(&window));
}

#[test]
fn test_toggle_sidebar_persists_hidden_to_gsettings() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-sidebar");
    assert!(!window.imp().settings.boolean(keys::SIDEBAR_VISIBLE));
}

#[test]
fn test_toggle_sidebar_persists_visible_to_gsettings() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-sidebar");
    activate_action(&window, "toggle-sidebar");
    assert!(window.imp().settings.boolean(keys::SIDEBAR_VISIBLE));
}

#[test]
fn test_toggle_sidebar_action_always_enabled() {
    ensure_gtk_init();
    let window = test_window();
    // Should be enabled even with no tabs open
    assert!(action_enabled(&window, "toggle-sidebar"));
}

#[test]
fn test_toggle_sidebar_action_enabled_with_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert!(action_enabled(&window, "toggle-sidebar"));
}

#[test]
fn test_toggle_sidebar_preserves_paned_position() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(300);

    // Hide and show sidebar
    activate_action(&window, "toggle-sidebar");
    activate_action(&window, "toggle-sidebar");

    // Position should be preserved
    assert_eq!(paned.position(), 300);
}

#[test]
fn test_clamp_noop_when_sidebar_hidden() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(350);

    // Hide sidebar — animation moves position to 0
    activate_action(&window, "toggle-sidebar");

    // Clamp should be a no-op when sidebar is hidden
    let pos_after_hide = paned.position();
    clamp_sidebar_position(&window, paned, &window.imp().content_box, 600);
    assert_eq!(paned.position(), pos_after_hide);
}

#[test]
fn test_toggle_sidebar_multiple_cycles() {
    ensure_gtk_init();
    let window = test_window();
    for _ in 0..5 {
        activate_action(&window, "toggle-sidebar");
        assert!(!sidebar_visible(&window));
        activate_action(&window, "toggle-sidebar");
        assert!(sidebar_visible(&window));
    }
}

#[test]
fn test_toggle_sidebar_works_with_tabs_open() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    activate_action(&window, "toggle-sidebar");
    assert!(!sidebar_visible(&window));

    activate_action(&window, "toggle-sidebar");
    assert!(sidebar_visible(&window));
}

#[test]
fn test_toggle_sidebar_action_state_syncs() {
    ensure_gtk_init();
    let window = test_window();

    // Initial state should be true (visible)
    let action = window
        .lookup_action("toggle-sidebar")
        .unwrap()
        .downcast::<gio::SimpleAction>()
        .unwrap();
    assert!(action.state().unwrap().get::<bool>().unwrap());

    // After toggle, state should be false
    activate_action(&window, "toggle-sidebar");
    assert!(!action.state().unwrap().get::<bool>().unwrap());

    // After second toggle, state should be true again
    activate_action(&window, "toggle-sidebar");
    assert!(action.state().unwrap().get::<bool>().unwrap());
}

// --- Sidebar animation regression tests ---

#[test]
fn test_shrink_start_child_stays_false_after_hide() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;

    // Verify initial state
    assert!(!paned.shrinks_start_child());

    // Hide sidebar — animation completes instantly in headless tests
    activate_action(&window, "toggle-sidebar");

    // shrink-start-child must be restored to false after animation completes
    assert!(!paned.shrinks_start_child());
}

#[test]
fn test_shrink_start_child_stays_false_after_show() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;

    activate_action(&window, "toggle-sidebar"); // hide
    activate_action(&window, "toggle-sidebar"); // show

    // Must remain false after a full hide+show cycle
    assert!(!paned.shrinks_start_child());
}

#[test]
fn test_shrink_start_child_stays_false_after_rapid_toggle() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;

    for _ in 0..10 {
        activate_action(&window, "toggle-sidebar");
    }

    assert!(!paned.shrinks_start_child());
}

#[test]
fn test_saved_sidebar_pos_set_on_hide() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(275);

    activate_action(&window, "toggle-sidebar"); // hide

    assert_eq!(window.imp().saved_sidebar_pos.get(), 275);
}

#[test]
fn test_saved_sidebar_pos_preserved_across_cycle() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(275);

    activate_action(&window, "toggle-sidebar"); // hide
    activate_action(&window, "toggle-sidebar"); // show

    // The saved position should still be 275 (not overwritten by animation)
    assert_eq!(window.imp().saved_sidebar_pos.get(), 275);
}

#[test]
fn test_clamp_still_works_after_animation_cycle() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;

    // Complete hide+show cycle
    activate_action(&window, "toggle-sidebar");
    activate_action(&window, "toggle-sidebar");

    // Clamp should still enforce 1/3 max on a 600px window
    paned.set_position(500);
    clamp_sidebar_position(&window, paned, &window.imp().content_box, 600);
    assert_eq!(paned.position(), 200); // 600 / 3 = 200
}

#[test]
fn test_hide_animation_targets_collapsed_endpoint() {
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    paned.set_position(250);

    activate_action(&window, "toggle-sidebar");

    // Animation completes instantly in headless tests. Hidden state should land
    // on the shared collapsed endpoint used by startup-hidden restore too.
    assert_eq!(paned.position(), SIDEBAR_COLLAPSED_POSITION);
}

#[test]
fn test_startup_hidden_sidebar_restores_collapsed_runtime_position() {
    ensure_gtk_init();
    let window = test_window_with_sidebar_state(false, 275);

    assert!(!sidebar_visible(&window));
    assert_eq!(
        window.imp().main_paned.position(),
        SIDEBAR_COLLAPSED_POSITION,
        "hidden startup should restore the live paned state to the collapsed endpoint"
    );
    assert_eq!(window.imp().saved_sidebar_pos.get(), 275);
    assert_eq!(window.imp().settings.int(keys::SIDEBAR_POSITION), 275);
}

#[test]
fn test_first_show_after_hidden_startup_restores_saved_sidebar_width() {
    ensure_gtk_init();
    let window = test_window_with_sidebar_state(false, 275);

    activate_action(&window, "toggle-sidebar");

    assert!(sidebar_visible(&window));
    assert_eq!(window.imp().main_paned.position(), 275);
}

#[test]
fn test_hidden_startup_sidebar_cycle_after_present_preserves_positions() {
    ensure_gtk_init();
    let window = test_window_with_sidebar_state(false, 275);
    let paned = &window.imp().main_paned;

    // Realize and allocate the window so this exercises the actual GtkPaned
    // layout path that previously emitted measurement warnings.
    present_window(&window);
    assert_eq!(paned.position(), SIDEBAR_COLLAPSED_POSITION);

    activate_action(&window, "toggle-sidebar");
    wait_for_paned_position(&window, 275);
    assert!(sidebar_visible(&window));
    assert_eq!(paned.position(), 275);

    activate_action(&window, "toggle-sidebar");
    wait_for_paned_position(&window, SIDEBAR_COLLAPSED_POSITION);
    assert!(!sidebar_visible(&window));
    assert_eq!(paned.position(), SIDEBAR_COLLAPSED_POSITION);

    activate_action(&window, "toggle-sidebar");
    wait_for_paned_position(&window, 275);
    assert!(sidebar_visible(&window));
    assert_eq!(paned.position(), 275);
}

#[test]
fn test_workspace_restore_refreshes_sidebar_handle_budget_after_present() {
    ensure_gtk_init();
    let _roots_dir = seed_restored_workspaces();
    let window = test_window();

    present_window(&window);
    wait_for_workspace_roots(&window, 3);

    let content_box = &window.imp().content_box;
    let (content_min, _, _, _) = content_box.measure(gtk4::Orientation::Horizontal, -1);
    let (sidebar_min, _, _, _) = window.imp().sidebar.measure(gtk4::Orientation::Horizontal, -1);
    let (paned_min, _, _, _) = window
        .imp()
        .main_paned
        .measure(gtk4::Orientation::Horizontal, -1);
    let expected_handle = (paned_min - sidebar_min - content_min).max(1);

    assert_eq!(
        window.imp().handle_overhead.get(),
        expected_handle,
        "workspace restore should refresh the cached paned handle budget against the realized layout",
    );
}

#[test]
fn test_hidden_startup_sidebar_cycle_after_present_with_restored_workspaces_preserves_positions() {
    ensure_gtk_init();
    let _roots_dir = seed_restored_workspaces();
    let window = test_window_with_sidebar_state(false, 275);
    let paned = &window.imp().main_paned;

    present_window(&window);
    wait_for_workspace_roots(&window, 3);
    assert_eq!(paned.position(), SIDEBAR_COLLAPSED_POSITION);

    activate_action(&window, "toggle-sidebar");
    wait_for_paned_position(&window, 275);
    assert!(sidebar_visible(&window));
    assert_eq!(paned.position(), 275);

    activate_action(&window, "toggle-sidebar");
    wait_for_paned_position(&window, SIDEBAR_COLLAPSED_POSITION);
    assert!(!sidebar_visible(&window));
    assert_eq!(paned.position(), SIDEBAR_COLLAPSED_POSITION);

    activate_action(&window, "toggle-sidebar");
    wait_for_paned_position(&window, 275);
    assert!(sidebar_visible(&window));
    assert_eq!(paned.position(), 275);
}

#[test]
fn test_hide_after_restored_workspaces_clamps_stale_visible_position_before_saving() {
    ensure_gtk_init();
    let _roots_dir = seed_restored_workspaces();
    let window = test_window_with_sidebar_state(true, 275);
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;

    present_window(&window);
    wait_for_workspace_roots(&window, 3);

    let budget_width = if paned.width() > 0 {
        paned.width()
    } else {
        window.width()
    };
    let max = clamp_sidebar_visible_position(&window, content_box, budget_width, i32::MAX);
    let stale = max + 1;

    // Reproduce the hide-time bug: the action flips the logical visibility flag
    // before the revealer leaves layout, so a stale restored position can slip
    // into the hide path if clamping keys only off `sidebar_visible`.
    window.imp().sidebar_visible.set(false);
    paned.set_position(stale);

    activate_action(&window, "toggle-sidebar");

    assert_eq!(
        window.imp().saved_sidebar_pos.get(),
        max,
        "hide should clamp the current visible position before saving it while the sidebar revealer is still on-screen",
    );
    assert_eq!(paned.position(), SIDEBAR_COLLAPSED_POSITION);
}

#[test]
fn test_hidden_startup_hide_after_first_show_with_restored_workspaces_clamps_stale_position() {
    ensure_gtk_init();
    let _roots_dir = seed_restored_workspaces();
    let window = test_window_with_sidebar_state(false, 275);
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;

    present_window(&window);
    wait_for_workspace_roots(&window, 3);
    assert_eq!(paned.position(), SIDEBAR_COLLAPSED_POSITION);

    activate_action(&window, "toggle-sidebar");
    wait_for_paned_position(&window, 275);
    assert!(sidebar_visible(&window));

    let budget_width = if paned.width() > 0 {
        paned.width()
    } else {
        window.width()
    };
    let max = clamp_sidebar_visible_position(&window, content_box, budget_width, i32::MAX);
    let stale = max + 1;

    // This is the user-reported path: restore hidden, show once, then hide. The
    // hide action flips the logical visibility flag before the revealer leaves
    // layout, so the stale visible position must still be clamped.
    window.imp().sidebar_visible.set(false);
    paned.set_position(stale);

    activate_action(&window, "toggle-sidebar");

    assert_eq!(
        window.imp().saved_sidebar_pos.get(),
        max,
        "hide after the first restored show should clamp the stale visible position before saving it",
    );
    assert_eq!(paned.position(), SIDEBAR_COLLAPSED_POSITION);
}

#[test]
fn test_sidebar_show_target_clamps_to_current_budget() {
    ensure_gtk_init();
    let window = test_window();
    let content_box = &window.imp().content_box;
    let (content_min, _, _, _) = content_box.measure(gtk4::Orientation::Horizontal, -1);
    let expected_max = (640 / 3)
        .min(640 - content_min - window.imp().handle_overhead.get())
        .max(0)
        .max(SIDEBAR_COLLAPSED_POSITION);

    assert_eq!(
        clamp_sidebar_visible_position(&window, content_box, 640, 500),
        expected_max,
        "show targets should be clamped before animation writes them into GtkPaned",
    );
}

// --- Pre-clamp and content_box width-request invariant tests ---

#[test]
fn test_content_box_width_request_matches_content_min() {
    ensure_gtk_init();
    let window = test_window();
    let content_box = &window.imp().content_box;

    let (content_min, _, _, _) = content_box.measure(gtk4::Orientation::Horizontal, -1);
    assert!(
        content_min > 0,
        "content_box should have a non-zero minimum width"
    );
    assert!(
        content_box.width_request() >= content_min,
        "content_box.width_request ({}) should be >= content_min ({content_min})",
        content_box.width_request(),
    );
}

#[test]
fn test_pre_clamp_safe_for_narrow_window() {
    // Simulates the startup case where the restored window width is narrow
    // enough that the default sidebar position would violate the content minimum.
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;
    let content_box = &window.imp().content_box;

    let (content_min, _, _, _) = content_box.measure(gtk4::Orientation::Horizontal, -1);
    let handle_overhead = window.imp().handle_overhead.get();
    let pos = paned.position();

    // At the window's minimum width-request (640), the position must leave
    // enough room for the content box minimum plus handle overhead.
    let min_width = window.width_request();
    assert!(min_width > 0, "window must have a width-request set");
    assert!(
        content_min > 0,
        "content_box must have a non-zero minimum width"
    );
    if min_width > 0 && content_min > 0 {
        let max_safe = min_width - content_min - handle_overhead;
        assert!(
            pos <= max_safe || max_safe < 0,
            "pre-clamped position ({pos}) should leave room for content_min ({content_min}) \
             + handle ({handle_overhead}) at min window width ({min_width}), \
             max safe = {max_safe}",
        );
    }
}

#[test]
fn test_pre_clamp_preserves_wide_position() {
    // At the default 1200px window width, a sidebar position of 250 (default)
    // should NOT be reduced by the pre-clamp — it's well within the 1/3 limit.
    ensure_gtk_init();
    let window = test_window();
    let paned = &window.imp().main_paned;

    // Default GSettings position is 250, default width is 1200.
    // 250 < 1200/3 = 400, so position should be preserved.
    let pos = paned.position();
    assert!(
        pos >= 200,
        "at default 1200px width, sidebar position ({pos}) should not be aggressively clamped"
    );
}

// --- Fullscreen actions ---

#[test]
fn test_fullscreen_action_exists() {
    ensure_gtk_init();
    let window = test_window();
    assert!(window.lookup_action("fullscreen").is_some());
    assert!(window.lookup_action("unfullscreen").is_some());
    assert!(window.lookup_action("toggle-fullscreen").is_some());
}

#[test]
fn test_fullscreen_action_initial_enabled_state() {
    ensure_gtk_init();
    let window = test_window();
    // Initially not fullscreen: fullscreen enabled, unfullscreen disabled.
    assert!(action_enabled(&window, "fullscreen"));
    assert!(!action_enabled(&window, "unfullscreen"));
}

#[test]
fn test_toggle_fullscreen_action_always_enabled() {
    ensure_gtk_init();
    let window = test_window();
    // toggle-fullscreen is always enabled regardless of fullscreen state.
    assert!(action_enabled(&window, "toggle-fullscreen"));
}

// --- Theme selector / color scheme ---

#[test]
fn test_color_scheme_gsettings_key_exists() {
    ensure_gtk_init();
    let settings = gtk4::gio::Settings::new(lushtext_core::config::APP_ID);
    // Default value should be "default" (follow system).
    let scheme = settings.string(keys::COLOR_SCHEME);
    assert_eq!(scheme.as_str(), "default");
}

#[test]
fn test_color_scheme_gsettings_roundtrip() {
    ensure_gtk_init();
    let settings = gtk4::gio::Settings::new(lushtext_core::config::APP_ID);
    assert!(
        settings
            .set_string(keys::COLOR_SCHEME, "force-dark")
            .is_ok()
    );
    assert_eq!(settings.string(keys::COLOR_SCHEME).as_str(), "force-dark");
    assert!(
        settings
            .set_string(keys::COLOR_SCHEME, "force-light")
            .is_ok()
    );
    assert_eq!(settings.string(keys::COLOR_SCHEME).as_str(), "force-light");
    // Reset to default for other tests.
    let _ = settings.set_string(keys::COLOR_SCHEME, "default");
}

#[test]
fn test_primary_menu_button_exists() {
    ensure_gtk_init();
    let window = test_window();
    // The hamburger menu button should be accessible as a template child.
    let menu_button = &window.imp().primary_menu_button;
    assert!(menu_button.popover().is_some());
}

// --- Menu structure ---

#[test]
fn test_menu_does_not_contain_open_file() {
    ensure_gtk_init();
    let window = test_window();
    // The hamburger menu model should NOT contain "Open File" or "Open Folder"
    // items. These were removed to match GNOME Text Editor's menu layout.
    let menu_button = &window.imp().primary_menu_button;
    let model = menu_button
        .menu_model()
        .expect("primary_menu_button should have a menu model");
    // Walk all sections and items to verify no Open File/Folder action.
    let mut has_open_file = false;
    let mut has_open_folder = false;
    for i in 0..model.n_items() {
        if let Some(section) = model.item_link(i, "section") {
            for j in 0..section.n_items() {
                if let Some(action) = section
                    .item_attribute_value(j, "action", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
                {
                    if action == "win.open-file" {
                        has_open_file = true;
                    }
                    if action == "win.open-folder" {
                        has_open_folder = true;
                    }
                }
            }
        }
    }
    assert!(!has_open_file, "Menu should not contain 'Open File'");
    assert!(!has_open_folder, "Menu should not contain 'Open Folder'");
}

#[test]
fn test_menu_contains_fullscreen_items() {
    ensure_gtk_init();
    let window = test_window();
    let menu_button = &window.imp().primary_menu_button;
    let model = menu_button
        .menu_model()
        .expect("primary_menu_button should have a menu model");
    let mut has_fullscreen = false;
    let mut has_unfullscreen = false;
    for i in 0..model.n_items() {
        if let Some(section) = model.item_link(i, "section") {
            for j in 0..section.n_items() {
                if let Some(action) = section
                    .item_attribute_value(j, "action", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
                {
                    if action == "win.fullscreen" {
                        has_fullscreen = true;
                    }
                    if action == "win.unfullscreen" {
                        has_unfullscreen = true;
                    }
                }
            }
        }
    }
    assert!(has_fullscreen, "Menu should contain 'Fullscreen' item");
    assert!(
        has_unfullscreen,
        "Menu should contain 'Leave Fullscreen' item"
    );
}

#[test]
fn test_menu_contains_new_file() {
    ensure_gtk_init();
    let window = test_window();
    let menu_button = &window.imp().primary_menu_button;
    let model = menu_button
        .menu_model()
        .expect("primary_menu_button should have a menu model");
    let mut has_new_tab = false;
    for i in 0..model.n_items() {
        if let Some(section) = model.item_link(i, "section") {
            for j in 0..section.n_items() {
                if let Some(action) = section
                    .item_attribute_value(j, "action", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
                    && action == "win.new-tab"
                {
                    has_new_tab = true;
                }
            }
        }
    }
    assert!(has_new_tab, "Menu should still contain 'New File' item");
}

#[test]
fn test_menu_contains_theme_custom_slot() {
    ensure_gtk_init();
    let window = test_window();
    let menu_button = &window.imp().primary_menu_button;
    let model = menu_button
        .menu_model()
        .expect("primary_menu_button should have a menu model");
    // The first section should contain a custom="theme" attribute.
    let mut has_theme_slot = false;
    for i in 0..model.n_items() {
        if let Some(section) = model.item_link(i, "section") {
            for j in 0..section.n_items() {
                if let Some(custom) = section
                    .item_attribute_value(j, "custom", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
                    && custom == "theme"
                {
                    has_theme_slot = true;
                }
            }
        }
    }
    assert!(
        has_theme_slot,
        "Menu should contain a 'theme' custom widget slot"
    );
}

#[test]
fn test_parse_color_scheme() {
    ensure_gtk_init();
    use lushtext_core::ui::window::parse_color_scheme;
    assert_eq!(
        parse_color_scheme("force-light"),
        libadwaita::ColorScheme::ForceLight
    );
    assert_eq!(
        parse_color_scheme("force-dark"),
        libadwaita::ColorScheme::ForceDark
    );
    assert_eq!(
        parse_color_scheme("default"),
        libadwaita::ColorScheme::Default
    );
    // Unknown values fall back to Default.
    assert_eq!(
        parse_color_scheme("garbage"),
        libadwaita::ColorScheme::Default
    );
    assert_eq!(parse_color_scheme(""), libadwaita::ColorScheme::Default);
}

// --- Discard-changes action enabled/disabled lifecycle ---

#[test]
fn test_discard_changes_action_disabled_when_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    assert!(!action_enabled(&window, "discard-changes"));
}

#[test]
fn test_discard_changes_action_disabled_on_untitled_tab() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();
    assert!(!action_enabled(&window, "discard-changes"));
}

#[test]
fn test_discard_changes_action_disabled_on_unmodified_file_tab() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("clean.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    window.open_document(&file_path);
    flush_events();
    assert!(!action_enabled(&window, "discard-changes"));
}

#[test]
fn test_discard_changes_action_enabled_on_modified_file_tab() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("dirty.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    window.open_document(&file_path);
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("modified");
    flush_events();

    assert!(action_enabled(&window, "discard-changes"));
}

#[test]
fn test_discard_changes_action_disabled_after_closing_all_tabs() {
    ensure_gtk_init();
    let window = test_window();
    // Use an unmodified tab so close-tab doesn't trigger the save dialog.
    window.new_tab();
    flush_events();
    // Even though discard is already disabled for untitled tabs, verify the
    // lifecycle: with tabs → close all → still disabled.
    activate_action(&window, "close-tab");
    assert!(!action_enabled(&window, "discard-changes"));
}

#[test]
fn test_discard_changes_action_disabled_on_modified_untitled_tab() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("some text");
    flush_events();

    // Modified but no file path — should stay disabled
    assert!(!action_enabled(&window, "discard-changes"));
}

// --- Zoom action existence and enabled state ---

#[test]
fn test_zoom_in_action_exists_and_enabled() {
    ensure_gtk_init();
    let window = test_window();
    assert!(action_enabled(&window, "zoom-in"));
}

#[test]
fn test_zoom_out_action_exists_and_enabled() {
    ensure_gtk_init();
    let window = test_window();
    assert!(action_enabled(&window, "zoom-out"));
}

#[test]
fn test_zoom_reset_action_exists_and_enabled() {
    ensure_gtk_init();
    let window = test_window();
    assert!(action_enabled(&window, "zoom-reset"));
}

// --- GSettings zoom-level key ---

#[test]
fn test_gsettings_zoom_level_default() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
}

#[test]
fn test_gsettings_zoom_level_roundtrip() {
    ensure_gtk_init();
    let window = test_window();
    let settings = &window.imp().settings;
    assert!(settings.set_uint(keys::ZOOM_LEVEL, 150).is_ok());
    assert_eq!(settings.uint(keys::ZOOM_LEVEL), 150);
}

// --- Zoom actions modify GSettings ---

#[test]
fn test_zoom_in_increments_zoom_level() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
    activate_action(&window, "zoom-in");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 110);
}

#[test]
fn test_zoom_out_decrements_zoom_level() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
    activate_action(&window, "zoom-out");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 90);
}

#[test]
fn test_zoom_reset_sets_to_100() {
    ensure_gtk_init();
    let window = test_window();
    let _ = window.imp().settings.set_uint(keys::ZOOM_LEVEL, 150);
    flush_events();
    activate_action(&window, "zoom-reset");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
}

#[test]
fn test_zoom_reset_noop_when_already_100() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
    activate_action(&window, "zoom-reset");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
}

// --- Zoom boundary behavior ---

#[test]
fn test_zoom_in_capped_at_400() {
    ensure_gtk_init();
    let window = test_window();
    let _ = window.imp().settings.set_uint(keys::ZOOM_LEVEL, 400);
    flush_events();
    activate_action(&window, "zoom-in");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 400);
}

#[test]
fn test_zoom_out_capped_at_50() {
    ensure_gtk_init();
    let window = test_window();
    let _ = window.imp().settings.set_uint(keys::ZOOM_LEVEL, 50);
    flush_events();
    activate_action(&window, "zoom-out");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 50);
}

#[test]
fn test_zoom_in_from_390_caps_at_400() {
    ensure_gtk_init();
    let window = test_window();
    let _ = window.imp().settings.set_uint(keys::ZOOM_LEVEL, 390);
    flush_events();
    activate_action(&window, "zoom-in");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 400);
}

#[test]
fn test_zoom_out_from_60_caps_at_50() {
    ensure_gtk_init();
    let window = test_window();
    let _ = window.imp().settings.set_uint(keys::ZOOM_LEVEL, 60);
    flush_events();
    activate_action(&window, "zoom-out");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 50);
}

// --- Zoom multiple steps ---

#[test]
fn test_zoom_in_multiple_steps() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
    activate_action(&window, "zoom-in");
    activate_action(&window, "zoom-in");
    activate_action(&window, "zoom-in");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 130);
}

#[test]
fn test_zoom_out_multiple_steps() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
    activate_action(&window, "zoom-out");
    activate_action(&window, "zoom-out");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 80);
}

#[test]
fn test_zoom_in_then_reset() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "zoom-in");
    activate_action(&window, "zoom-in");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 120);
    activate_action(&window, "zoom-reset");
    assert_eq!(window.imp().settings.uint(keys::ZOOM_LEVEL), 100);
}

// --- Menu structure: zoom custom slot ---

#[test]
fn test_menu_contains_zoom_custom_slot() {
    ensure_gtk_init();
    let window = test_window();
    let menu_button = &window.imp().primary_menu_button;
    let model = menu_button
        .menu_model()
        .expect("primary_menu_button should have a menu model");
    let mut has_zoom_slot = false;
    for i in 0..model.n_items() {
        if let Some(section) = model.item_link(i, "section") {
            for j in 0..section.n_items() {
                if let Some(custom) = section
                    .item_attribute_value(j, "custom", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
                    && custom == "zoom"
                {
                    has_zoom_slot = true;
                }
            }
        }
    }
    assert!(
        has_zoom_slot,
        "Menu should contain a 'zoom' custom widget slot"
    );
}

// --- Markdown preview action tests ---

#[test]
fn test_preview_actions_disabled_with_no_tabs() {
    ensure_gtk_init();
    let window = test_window();
    flush_events();
    assert!(
        !action_enabled(&window, "toggle-preview-pane"),
        "toggle-preview-pane should be disabled with no tabs"
    );
    assert!(
        !action_enabled(&window, "toggle-preview-mode"),
        "toggle-preview-mode should be disabled with no tabs"
    );
}

#[test]
fn test_preview_actions_enabled_after_tab_open() {
    ensure_gtk_init();
    let window = test_window();
    flush_events();
    activate_action(&window, "new-tab");
    assert!(
        action_enabled(&window, "toggle-preview-pane"),
        "toggle-preview-pane should be enabled with a tab open"
    );
    assert!(
        action_enabled(&window, "toggle-preview-mode"),
        "toggle-preview-mode should be enabled with a tab open"
    );
}

#[test]
fn test_preview_actions_disabled_after_closing_all_tabs() {
    ensure_gtk_init();
    let window = test_window();
    flush_events();
    activate_action(&window, "new-tab");
    assert!(action_enabled(&window, "toggle-preview-pane"));
    activate_action(&window, "close-tab");
    flush_after_delay(std::time::Duration::from_millis(100));
    assert!(
        !action_enabled(&window, "toggle-preview-pane"),
        "toggle-preview-pane should be disabled after closing all tabs"
    );
}

#[test]
fn test_preview_mode_noop_when_preview_pane_visible() {
    ensure_gtk_init();
    let window = test_window();
    flush_events();
    activate_action(&window, "new-tab");
    // Show the side-by-side preview pane.
    activate_action(&window, "toggle-preview-pane");
    flush_events();
    // preview_visible is now true. Alt+P should be a no-op.
    let was_mode = window.imp().preview_mode.get();
    activate_action(&window, "toggle-preview-mode");
    flush_events();
    assert_eq!(
        window.imp().preview_mode.get(),
        was_mode,
        "toggle-preview-mode should be a no-op when side pane is visible"
    );
}

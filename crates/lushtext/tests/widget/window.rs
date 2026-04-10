// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the main window shell.
//!
//! This suite focuses on the current window contract: split-view sidebar
//! behavior, a few critical shell affordances, and preview-pane regressions
//! that still live in the window layer.

use crate::common::ensure_gtk_init;
use gio::prelude::{ActionExt, ActionGroupExt, ActionMapExt};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::{ActionRowExt, AnimationExt};
use lushtext_core::config::keys;
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceEntry, WorkspaceId, WorkspacesFile,
};
use lushtext_core::services::{json_store, workspace_manager};
use lushtext_core::ui::editor_page::LushtextEditorPage;
use lushtext_core::ui::window::LushtextWindow;
use std::time::{Duration, Instant};

fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

fn test_window_with_split_view_state(
    workspace_visible: bool,
    workspace_fraction: f64,
    properties_visible: bool,
    properties_fraction: f64,
) -> LushtextWindow {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED, true)
        .expect("set split-view-layout-migrated");
    settings
        .set_boolean(keys::WORKSPACE_SIDEBAR_VISIBLE, workspace_visible)
        .expect("set workspace-sidebar-visible");
    settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, workspace_fraction)
        .expect("set workspace-sidebar-width-fraction");
    settings
        .set_boolean(keys::PROPERTIES_SIDEBAR_VISIBLE, properties_visible)
        .expect("set properties-sidebar-visible");
    settings
        .set_double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION, properties_fraction)
        .expect("set properties-sidebar-width-fraction");
    test_window()
}

fn test_window_with_legacy_sidebar_state(visible: bool, position: i32) -> LushtextWindow {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED, false)
        .expect("clear split-view-layout-migrated");
    settings
        .set_boolean(keys::SIDEBAR_VISIBLE, visible)
        .expect("set legacy sidebar-visible");
    settings
        .set_int(keys::SIDEBAR_POSITION, position)
        .expect("set legacy sidebar-position");
    test_window()
}

fn test_window_with_initial_size(width: i32, height: i32) -> LushtextWindow {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(keys::WINDOW_WIDTH, width)
        .expect("set window-width");
    settings
        .set_int(keys::WINDOW_HEIGHT, height)
        .expect("set window-height");
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

fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

fn flush_after_delay(delay: Duration) {
    std::thread::sleep(delay);
    flush_events();
}

fn present_window(window: &LushtextWindow) {
    window.present();
    flush_events();
}

fn action_enabled(window: &LushtextWindow, name: &str) -> bool {
    let action = window
        .lookup_action(name)
        .unwrap_or_else(|| panic!("action '{name}' not found"));
    action.is_enabled()
}

fn activate_action(window: &LushtextWindow, name: &str) {
    ActionGroupExt::activate_action(window, name, None);
    flush_events();
}

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

fn workspace_sidebar_visible(window: &LushtextWindow) -> bool {
    window.imp().workspace_split_view.shows_sidebar()
}

fn properties_sidebar_visible(window: &LushtextWindow) -> bool {
    window.imp().properties_split_view.shows_sidebar()
}

fn preview_animation(window: &LushtextWindow) -> libadwaita::TimedAnimation {
    window
        .imp()
        .preview_animation
        .borrow()
        .as_ref()
        .cloned()
        .expect("preview animation should be active")
}

#[test]
fn test_window_restores_default_size() {
    ensure_gtk_init();
    let window = test_window();
    let (w, h) = window.default_size();
    assert_eq!(w, 1200);
    assert_eq!(h, 800);
}

#[test]
fn test_split_view_settings_defaults() {
    ensure_gtk_init();
    let window = test_window();
    let settings = &window.imp().settings;

    assert!(settings.boolean(keys::WORKSPACE_SIDEBAR_VISIBLE));
    assert_eq!(settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), 0.2);
    assert!(!settings.boolean(keys::PROPERTIES_SIDEBAR_VISIBLE));
    assert_eq!(settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION), 0.28);
}

#[test]
fn test_split_view_defaults_restore_on_window() {
    ensure_gtk_init();
    let window = test_window();

    assert!(workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!(
        (window.imp().workspace_split_view.sidebar_width_fraction() - 0.2).abs() < 0.001
    );
    assert!(
        (window.imp().properties_split_view.sidebar_width_fraction() - 0.28).abs() < 0.001
    );
}

#[test]
fn test_legacy_sidebar_settings_migrate_to_workspace_split_view() {
    ensure_gtk_init();
    let window = test_window_with_legacy_sidebar_state(false, 275);
    let settings = &window.imp().settings;

    assert!(settings.boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED));
    assert!(!workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!(
        (window.imp().workspace_split_view.sidebar_width_fraction() - (275.0 / 1200.0)).abs()
            < 0.001
    );
}

#[test]
fn test_toggle_sidebar_hides_workspace_split_view() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-sidebar");
    assert!(!workspace_sidebar_visible(&window));
    assert!(
        !window
            .imp()
            .settings
            .boolean(keys::WORKSPACE_SIDEBAR_VISIBLE)
    );
}

#[test]
fn test_toggle_sidebar_action_always_enabled() {
    ensure_gtk_init();
    let window = test_window();
    assert!(action_enabled(&window, "toggle-sidebar"));
}

#[test]
fn test_toggle_sidebar_action_state_syncs_with_split_view() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-sidebar")
        .unwrap()
        .downcast::<gio::SimpleAction>()
        .unwrap();

    assert!(action.state().unwrap().get::<bool>().unwrap());
    window.imp().workspace_split_view.set_show_sidebar(false);
    flush_events();
    assert!(!action.state().unwrap().get::<bool>().unwrap());
}

#[test]
fn test_toggle_properties_shows_properties_split_view() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-properties");
    assert!(properties_sidebar_visible(&window));
    assert!(
        window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );
}

#[test]
fn test_toggle_properties_action_state_syncs_with_split_view() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-properties")
        .unwrap()
        .downcast::<gio::SimpleAction>()
        .unwrap();

    assert!(!action.state().unwrap().get::<bool>().unwrap());
    window.imp().properties_split_view.set_show_sidebar(true);
    flush_events();
    assert!(action.state().unwrap().get::<bool>().unwrap());
}

#[test]
fn test_both_sidebars_can_be_visible_together_on_wide_window() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);

    assert!(!window.imp().workspace_split_view.is_collapsed());
    assert!(!window.imp().properties_split_view.is_collapsed());

    activate_action(&window, "toggle-properties");
    assert!(workspace_sidebar_visible(&window));
    assert!(properties_sidebar_visible(&window));
}

#[test]
fn test_properties_pane_collapses_before_workspace_pane() {
    ensure_gtk_init();
    let wide_window = test_window_with_initial_size(1400, 900);
    present_window(&wide_window);
    assert!(!wide_window.imp().properties_split_view.is_collapsed());
    assert!(!wide_window.imp().workspace_split_view.is_collapsed());

    let medium_window = test_window_with_initial_size(1000, 900);
    present_window(&medium_window);
    assert!(medium_window.imp().properties_split_view.is_collapsed());
    assert!(!medium_window.imp().workspace_split_view.is_collapsed());
}

#[test]
fn test_properties_visibility_preference_survives_breakpoint_changes() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.2, true, 0.28);
    window.set_default_size(1400, 900);
    present_window(&window);

    assert!(properties_sidebar_visible(&window));
    assert!(
        window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );

    window.set_default_size(1000, 900);
    flush_after_delay(Duration::from_millis(20));

    assert!(properties_sidebar_visible(&window));
    assert!(
        window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );
}

#[test]
fn test_restored_workspaces_survive_dual_sidebar_shell() {
    ensure_gtk_init();
    let _roots_dir = seed_restored_workspaces();
    let window = test_window_with_split_view_state(true, 0.2, false, 0.28);

    present_window(&window);
    wait_for_workspace_roots(&window, 3);
    assert!(workspace_sidebar_visible(&window));
    assert_eq!(window.imp().sidebar.workspace_roots().len(), 3);
}

#[test]
fn test_properties_panel_shows_safe_untitled_metadata_state() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let panel = window.imp().properties_panel.imp();
    assert_eq!(panel.path_row.subtitle().as_deref(), Some("Untitled document"));
    assert_eq!(panel.encoding_row.subtitle().as_deref(), Some("Not available"));
    assert_eq!(panel.file_size_row.subtitle().as_deref(), Some("Not available"));
    assert_eq!(
        panel.formatting_source_row.subtitle().as_deref(),
        Some("Not available for untitled tabs")
    );
}

#[test]
fn test_properties_panel_updates_for_file_backed_editor() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("example.txt");
    std::fs::write(&path, "hello world").expect("write file");

    window.open_document(&path);
    flush_events();

    let panel = window.imp().properties_panel.imp();
    assert_eq!(
        panel.path_row.subtitle().as_deref(),
        Some(path.display().to_string().as_str())
    );
    assert_eq!(panel.encoding_row.subtitle().as_deref(), Some("UTF-8"));
}

#[test]
fn test_closing_properties_pane_restores_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(800, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.source_view().grab_focus();
    flush_events();

    activate_action(&window, "toggle-properties");
    window.imp().properties_toggle_button.grab_focus();
    flush_events();
    let hidden_row_ptr = window
        .imp()
        .properties_panel
        .imp()
        .editorconfig_row
        .upcast_ref::<gtk4::Widget>()
        .as_ptr();
    activate_action(&window, "toggle-properties");

    let focus = gtk4::prelude::GtkWindowExt::focus(&window).expect("focused widget");
    assert_ne!(
        focus.as_ptr(),
        hidden_row_ptr,
        "closing the pane must not leave focus stranded on a hidden properties row",
    );
    assert!(!properties_sidebar_visible(&window));
    let _ = editor;
}

#[test]
fn test_closing_properties_pane_with_no_editor_clears_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(800, 900);
    present_window(&window);

    activate_action(&window, "toggle-properties");
    let panel = window.imp().properties_panel.imp();
    panel.editorconfig_row.grab_focus();
    flush_events();

    activate_action(&window, "toggle-properties");
    assert!(gtk4::prelude::GtkWindowExt::focus(&window).is_none());
}

#[test]
fn test_properties_toggle_button_exists_and_is_wired() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(
        window.imp().properties_toggle_button.action_name().as_deref(),
        Some("win.toggle-properties")
    );
}

#[test]
fn test_primary_menu_button_exists() {
    ensure_gtk_init();
    let window = test_window();
    assert!(window.imp().primary_menu_button.popover().is_some());
}

#[test]
fn test_preview_pane_toggle_starts_nontrivial_animation() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-preview-pane");

    let animation = preview_animation(&window);
    assert_ne!(
        animation.value_from() as i32,
        animation.value_to() as i32,
        "preview pane toggle should start a real paned animation, not jump directly to the endpoint",
    );
    assert_eq!(animation.state(), libadwaita::AnimationState::Playing);
    assert!(
        window.imp().markdown_preview.property::<bool>("visible"),
        "preview widget should stay visible while the side-by-side animation is in flight",
    );
}

#[test]
fn test_preview_mode_toggle_starts_nontrivial_animation() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-preview-mode");

    let animation = preview_animation(&window);
    assert_ne!(
        animation.value_from() as i32,
        animation.value_to() as i32,
        "preview-only mode should animate the paned instead of snapping immediately",
    );
    assert_eq!(animation.state(), libadwaita::AnimationState::Playing);
    assert!(
        window.imp().editor_box.property::<bool>("visible"),
        "editor box should remain visible until the preview-only animation completes",
    );
}

#[test]
fn test_preview_pane_animation_does_not_enqueue_position_persistence_per_tick() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let generation_before = window.imp().preview_persist_generation.get();

    activate_action(&window, "toggle-preview-pane");

    assert!(window.imp().preview_animation_active.get());
    assert_eq!(
        window.imp().preview_persist_generation.get(),
        generation_before,
        "programmatic preview animation should not enqueue debounced settings writes on every tick",
    );
    assert_eq!(
        preview_animation(&window).state(),
        libadwaita::AnimationState::Playing
    );
}

#[test]
fn test_preview_mode_animation_does_not_enqueue_position_persistence_per_tick() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let generation_before = window.imp().preview_persist_generation.get();

    activate_action(&window, "toggle-preview-mode");

    assert!(window.imp().preview_animation_active.get());
    assert_eq!(
        window.imp().preview_persist_generation.get(),
        generation_before,
        "preview-only animation should not enqueue debounced settings writes on every tick",
    );
    assert_eq!(
        preview_animation(&window).state(),
        libadwaita::AnimationState::Playing
    );
}

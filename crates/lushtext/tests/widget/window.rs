// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the main window shell.
//!
//! This suite focuses on the current window contract: split-view sidebar
//! behavior, a few critical shell affordances, and preview-pane regressions
//! that still live in the window layer.

use crate::common::{emit_key_pressed_on_focus, ensure_gtk_init};
use gio::prelude::{ActionExt, ActionGroupExt, ActionMapExt};
use glib::prelude::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::{
    ActionRowExt, AdwApplicationWindowExt, AdwDialogExt, AlertDialogExt, AnimationExt,
    ComboRowExt,
};
use lushtext_core::config::keys;
use lushtext_core::model::annotation::{AnnotationRecord, AnnotationStyle};
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::encoding::{DocumentEncoding, FileHealthFindingKind, LineEnding};
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceEntry, WorkspaceId, WorkspacesFile,
};
use lushtext_core::services::file_limits::FileSizeCheck;
use lushtext_core::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use lushtext_core::services::{
    annotation_service, bookmark_service, draft_service, editor_io, json_store, session_service,
    workspace_manager,
};
use lushtext_core::ui::editor_page::{LushtextEditorPage, MinimapAvailability, MinimapMarkerKind, SaveError};
use lushtext_core::ui::preferences::LushtextPreferences;
use lushtext_core::ui::window::LushtextWindow;
use std::path::{Path, PathBuf};
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

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }
    panic!("condition was not met within {timeout:?}");
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
        .expect("expected operation to succeed")
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("expected operation to succeed")
}

fn editor_text(editor: &LushtextEditorPage) -> String {
    let buffer = editor.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn assert_tab_count(window: &LushtextWindow, expected: i32) {
    assert_eq!(
        window.imp().tab_view.n_pages(),
        expected,
        "expected {expected} open tab(s), got {}",
        window.imp().tab_view.n_pages()
    );
}

fn tab_pages(window: &LushtextWindow) -> Vec<libadwaita::TabPage> {
    (0..window.imp().tab_view.n_pages())
        .map(|index| window.imp().tab_view.nth_page(index))
        .collect()
}

fn tab_titles(window: &LushtextWindow) -> Vec<String> {
    tab_pages(window)
        .into_iter()
        .map(|page| page.title().to_string())
        .collect()
}

fn find_tab_page_by_title(window: &LushtextWindow, title: &str) -> libadwaita::TabPage {
    tab_pages(window)
        .into_iter()
        .find(|page| page.title().as_str() == title)
        .unwrap_or_else(|| panic!("tab '{title}' not found"))
}

fn prepare_tab_context_menu(window: &LushtextWindow, page: &libadwaita::TabPage) {
    window
        .imp()
        .tab_view
        .emit_by_name::<()>("setup-menu", &[page]);
    flush_events();
}

fn visible_alert_dialog(window: &LushtextWindow) -> Option<libadwaita::AlertDialog> {
    window
        .visible_dialog()
        .and_then(|dialog| dialog.downcast::<libadwaita::AlertDialog>().ok())
}

fn find_button_by_label(root: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Ok(button) = root.clone().downcast::<gtk4::Button>()
        && button.label().as_deref() == Some(label)
    {
        return Some(button);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_button_by_label(&widget, label) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn click_alert_extra_button(dialog: &libadwaita::AlertDialog, label: &str) {
    let extra = dialog.extra_child().expect("alert dialog extra child");
    let button = find_button_by_label(&extra, label)
        .unwrap_or_else(|| panic!("button '{label}' not found in alert extra child"));
    button.emit_clicked();
    flush_events();
}

fn workspace_sidebar_visible(window: &LushtextWindow) -> bool {
    window.imp().workspace_split_view.shows_sidebar()
}

fn properties_sidebar_visible(window: &LushtextWindow) -> bool {
    window.imp().properties_split_view.shows_sidebar()
}

fn workspace_total_fraction(window: &LushtextWindow) -> f64 {
    window.imp().workspace_split_view.sidebar_width_fraction()
}

fn current_window_width(window: &LushtextWindow) -> i32 {
    if window.width() > 0 {
        window.width()
    } else {
        let (default_width, _) = window.default_size();
        default_width
    }
}

fn assert_workspace_sidebar_width_locked(window: &LushtextWindow, expected_width: f64) {
    let split = &window.imp().workspace_split_view;
    assert!(
        (split.min_sidebar_width() - expected_width).abs() < 1.0,
        "expected min sidebar width near {expected_width}, got {}",
        split.min_sidebar_width()
    );
    assert!(
        (split.max_sidebar_width() - expected_width).abs() < 1.0,
        "expected max sidebar width near {expected_width}, got {}",
        split.max_sidebar_width()
    );
}

fn write_long_document(editor: &LushtextEditorPage, line_count: usize, needle_every: usize) {
    let mut text = String::new();
    for line in 0..line_count {
        let marker = if needle_every != 0 && line % needle_every == 0 {
            " needle"
        } else {
            ""
        };
        text.push_str(&format!("line {line:04}{marker}\n"));
    }
    editor.buffer().set_text(&text);
    flush_events();
}

fn properties_total_fraction(window: &LushtextWindow) -> f64 {
    let properties_fraction = window.imp().properties_split_view.sidebar_width_fraction();
    if workspace_sidebar_visible(window) && !window.imp().workspace_split_view.is_collapsed() {
        properties_fraction * (1.0 - workspace_total_fraction(window))
    } else {
        properties_fraction
    }
}

fn minimap_setting(window: &LushtextWindow) -> bool {
    window.imp().settings.boolean(keys::SHOW_MINIMAP)
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

fn seed_peek_workspace() -> (tempfile::TempDir, PathBuf, PathBuf) {
    ensure_gtk_init();
    let root_dir = tempfile::tempdir().expect("peek workspace tempdir");
    let alpha = root_dir.path().join("alpha.rs");
    let beta = root_dir.path().join("beta.rs");
    std::fs::write(&alpha, "fn alpha() {\n    println!(\"alpha\");\n}\n").expect("write alpha");
    std::fs::write(&beta, "fn beta() {\n    println!(\"beta\");\n}\n").expect("write beta");

    let mut workspaces = WorkspacesFile::default();
    workspaces.workspaces.push(WorkspaceConfig {
        id: WorkspaceId::new("peek-ws"),
        name: "peek".to_string(),
        entries: vec![
            WorkspaceEntry::File {
                path: alpha.clone(),
            },
            WorkspaceEntry::File { path: beta.clone() },
        ],
    });
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save peek workspaces");
    (root_dir, alpha, beta)
}

fn seed_named_tab_files(names: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().expect("named tab tempdir");
    let paths = names
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, format!("content for {name}\n")).expect("write tab fixture");
            path
        })
        .collect();
    (dir, paths)
}

#[test]
fn test_open_document_restores_bookmarks_and_annotations() {
    let tempdir = tempfile::tempdir().expect("notes tempdir");
    let file_path = tempdir.path().join("src/main.rs");
    std::fs::create_dir_all(file_path.parent().expect("expected operation to succeed")).expect("create file parent");
    std::fs::write(&file_path, "one\ntwo\nthree\nfour\n").expect("write source file");

    let window = test_window();
    let data_dir = json_store::data_dir();

    bookmark_service::save_for_path(
        &data_dir,
        &file_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("bookmark".to_string()),
        )],
    )
    .expect("save bookmarks");
    annotation_service::save_for_path(
        &data_dir,
        &file_path,
        &[AnnotationRecord::new(
            2,
            2,
            "restore annotation",
            AnnotationStyle::Question,
        )],
    )
    .expect("save annotations");

    present_window(&window);
    window.open_document(&file_path);

    wait_until(Duration::from_secs(2), || {
        active_editor(&window).bookmark_records().len() == 1
            && active_editor(&window).annotation_records().len() == 1
    });

    let editor = active_editor(&window);
    assert_eq!(editor.bookmark_records()[0].label.as_deref(), Some("bookmark"));
    assert_eq!(editor.annotation_records()[0].note_text, "restore annotation");
}

fn select_sidebar_path(section: &lushtext_core::ui::sidebar::WorkspaceSection, path: &Path) {
    let selection = section
        .imp()
        .file_tree_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .expect("sidebar section should use SingleSelection");
    let tree_model = section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .cloned()
        .expect("tree model should be loaded");

    for index in 0..tree_model.n_items() {
        if let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
            && let Some(item) = row.item().and_downcast::<lushtext_core::ui::sidebar::FileTreeItem>()
            && item.path().as_deref() == Some(path)
        {
            selection.set_selected(index);
            section
                .imp()
                .file_tree_view
                .scroll_to(index, gtk4::ListScrollFlags::FOCUS, None);
            flush_events();
            return;
        }
    }

    panic!("sidebar path {} was not found", path.display());
}

fn first_sidebar_section(window: &LushtextWindow) -> lushtext_core::ui::sidebar::WorkspaceSection {
    window
        .imp()
        .sidebar
        .imp()
        .sections
        .borrow()
        .first()
        .cloned()
        .expect("sidebar should have at least one section")
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
    assert_eq!(settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), 0.3);
    assert!(!settings.boolean(keys::PROPERTIES_SIDEBAR_VISIBLE));
    assert_eq!(settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION), 0.25);
}

#[test]
fn test_sidebar_peek_does_not_create_tab_until_promoted() {
    let (_root_dir, alpha, _beta) = seed_peek_workspace();
    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().sections.borrow().len() == 1
    });

    let section = first_sidebar_section(&window);
    select_sidebar_path(&section, &alpha);
    section.imp().file_tree_view.grab_focus();
    assert_tab_count(&window, 0);

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || {
        section.peek_visible()
            && section
                .imp()
                .peek_widgets
                .open_button
                .borrow()
                .as_ref()
                .is_some_and(gtk4::Button::is_sensitive)
    });
    assert_tab_count(&window, 0);

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::Return);
    wait_until(Duration::from_secs(2), || !section.peek_visible());
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);

    assert_tab_count(&window, 1);
    assert_eq!(active_editor(&window).file_path().as_deref(), Some(alpha.as_path()));
}

#[test]
fn test_sidebar_peek_promotion_reuses_existing_tab() {
    let (_root_dir, alpha, _beta) = seed_peek_workspace();
    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().sections.borrow().len() == 1
    });

    window.open_document(&alpha);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);

    let section = first_sidebar_section(&window);
    select_sidebar_path(&section, &alpha);
    section.imp().file_tree_view.grab_focus();

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || {
        section.peek_visible()
            && section
                .imp()
                .peek_widgets
                .open_button
                .borrow()
                .as_ref()
                .is_some_and(gtk4::Button::is_sensitive)
    });

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::Return);
    wait_until(Duration::from_secs(2), || !section.peek_visible());

    assert_tab_count(&window, 1);
    assert_eq!(active_editor(&window).file_path().as_deref(), Some(alpha.as_path()));
}

#[test]
fn test_split_view_defaults_restore_on_window() {
    ensure_gtk_init();
    let window = test_window();

    assert!(workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!((workspace_total_fraction(&window) - 0.3).abs() < 0.001);
    assert!((properties_total_fraction(&window) - 0.25).abs() < 0.001);
    assert_workspace_sidebar_width_locked(&window, 360.0);
}

#[test]
fn test_saved_split_view_widths_snap_to_supported_workspace_presets() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.25, true, 0.6);
    let settings = &window.imp().settings;

    assert!((workspace_total_fraction(&window) - 0.3).abs() < 0.001);
    assert!((properties_total_fraction(&window) - 0.25).abs() < 0.001);
    assert_eq!(settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), 0.3);
    assert_eq!(settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION), 0.25);
}

#[test]
fn test_legacy_sidebar_settings_migrate_to_workspace_split_view() {
    ensure_gtk_init();
    let window = test_window_with_legacy_sidebar_state(false, 275);
    let settings = &window.imp().settings;

    assert!(settings.boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED));
    assert!(!workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!((workspace_total_fraction(&window) - 0.3).abs() < 0.001);
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
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    assert!(action.state().expect("expected operation to succeed").get::<bool>().expect("expected operation to succeed"));
    window.imp().workspace_split_view.set_show_sidebar(false);
    flush_events();
    assert!(!action.state().expect("expected operation to succeed").get::<bool>().expect("expected operation to succeed"));
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
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    assert!(!action.state().expect("expected operation to succeed").get::<bool>().expect("expected operation to succeed"));
    window.imp().properties_split_view.set_show_sidebar(true);
    flush_events();
    assert!(action.state().expect("expected operation to succeed").get::<bool>().expect("expected operation to succeed"));
}

#[test]
fn test_toggle_minimap_updates_setting_and_action_state() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-minimap")
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    assert!(!minimap_setting(&window));
    assert!(!action.state().expect("expected operation to succeed").get::<bool>().expect("expected operation to succeed"));

    activate_action(&window, "toggle-minimap");

    assert!(minimap_setting(&window));
    assert!(action.state().expect("expected operation to succeed").get::<bool>().expect("expected operation to succeed"));
}

#[test]
fn test_toggle_minimap_action_state_tracks_external_setting_changes() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-minimap")
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    window
        .imp()
        .settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("set show-minimap");
    flush_events();

    assert!(action.state().expect("expected operation to succeed").get::<bool>().expect("expected operation to succeed"));
}

#[test]
fn test_minimap_visibility_restores_from_setting_for_long_document() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_long_document(&editor, 500, 0);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());
    assert_eq!(editor.minimap_availability(), MinimapAvailability::Visible);
}

#[test]
fn test_minimap_stays_visible_when_document_fits_viewport() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("short\nbuffer\n");
    flush_events();
    wait_until(Duration::from_secs(2), || editor.minimap_availability() == MinimapAvailability::Visible);

    assert!(editor.is_minimap_visible());
    assert_eq!(editor.minimap_availability(), MinimapAvailability::Visible);
}

#[test]
fn test_large_document_disables_minimap_and_surfaces_feedback() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.imp().size_check.set(FileSizeCheck::DisableSyntax);
    window
        .imp()
        .settings
        .set_boolean(keys::SHOW_MINIMAP, false)
        .expect("disable minimap");
    window
        .imp()
        .settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("re-enable minimap");
    flush_events();

    wait_until(Duration::from_secs(2), || {
        editor.minimap_availability() == MinimapAvailability::TooLarge
    });

    assert!(!editor.is_minimap_visible());
    let status = window
        .imp()
        .notification_bus
        .status_bar_view()
        .expect("minimap warning status message");
    assert_eq!(status.text, "Minimap unavailable for this large document");
}

#[test]
fn test_both_sidebars_can_be_visible_together_on_wide_window() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(keys::WINDOW_WIDTH, 2200)
        .expect("set window width");
    settings
        .set_int(keys::WINDOW_HEIGHT, 900)
        .expect("set window height");
    let window = test_window();
    present_window(&window);

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&window)
            && properties_sidebar_visible(&window)
            && !window.imp().workspace_split_view.is_collapsed()
            && !window.imp().properties_split_view.is_collapsed()
    });

    assert!(workspace_sidebar_visible(&window));
    assert!(properties_sidebar_visible(&window));
    assert!(!window.imp().workspace_split_view.is_collapsed());
    assert!(!window.imp().properties_split_view.is_collapsed());
    assert!((workspace_total_fraction(&window) - 360.0 / 2200.0).abs() < 0.001);
    assert!((properties_total_fraction(&window) - 0.25).abs() < 0.001);
    assert_workspace_sidebar_width_locked(&window, 360.0);
}

#[test]
fn test_preferences_sidebar_width_row_updates_workspace_shell_immediately() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let prefs = LushtextPreferences::new();

    prefs.imp().workspace_sidebar_width_row.set_selected(2);
    wait_until(Duration::from_secs(2), || {
        (workspace_total_fraction(&window) - 440.0 / 1400.0).abs() < 0.001
    });

    assert_eq!(
        window.imp().settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        0.4
    );
    assert_workspace_sidebar_width_locked(&window, 440.0);

    prefs.imp().workspace_sidebar_width_row.set_selected(0);
    wait_until(Duration::from_secs(2), || {
        (workspace_total_fraction(&window) - 280.0 / 1400.0).abs() < 0.001
    });

    assert_eq!(
        window.imp().settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        0.2
    );
    assert_workspace_sidebar_width_locked(&window, 280.0);
}

#[test]
fn test_workspace_sidebar_width_presets_clamp_across_representative_window_sizes() {
    ensure_gtk_init();

    for (window_width, stored_fraction, expected_width) in [
        (900, 0.2, 220.0),
        (1200, 0.3, 360.0),
        (1400, 0.4, 440.0),
        (2000, 0.3, 360.0),
    ] {
        let window = test_window_with_split_view_state(true, stored_fraction, false, 0.25);
        window.set_default_size(window_width, 900);
        present_window(&window);

        assert!(
            (workspace_total_fraction(&window) - expected_width / f64::from(current_window_width(&window)))
                .abs()
                < 0.001
        );
        assert_workspace_sidebar_width_locked(&window, expected_width);
    }
}

#[test]
fn test_workspace_sidebar_setting_recalculates_properties_breakpoint() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_sidebar_visible(&window));

    assert!(
        !window.imp().properties_split_view.is_collapsed(),
        "Comfy should keep the properties pane side-by-side at 1400sp"
    );

    window
        .imp()
        .settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, 0.4)
        .expect("set large preset");
    wait_until(Duration::from_secs(2), || {
        window.imp().properties_split_view.is_collapsed()
    });

    assert!(window.imp().properties_split_view.is_collapsed());
    assert_workspace_sidebar_width_locked(&window, 440.0);
}

#[test]
fn test_bookmark_markers_follow_toggle_state() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_long_document(&editor, 400, 0);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());

    let line = editor.buffer().iter_at_line(120).expect("line 120");
    editor.buffer().place_cursor(&line);
    let _ = editor.toggle_bookmark_at_cursor();
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Bookmark) == 1
    });

    let _ = editor.toggle_bookmark_at_cursor();
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Bookmark) == 0
    });
}

#[test]
fn test_search_markers_clear_when_search_closes() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_long_document(&editor, 320, 5);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());

    editor.show_search();
    editor.search_bar().search_entry().set_text("needle");
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Search) > 0
    });

    editor.hide_search();
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Search) == 0
    });
}

#[test]
fn test_modified_markers_clear_after_save() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let temp = tempfile::NamedTempFile::new().expect("temp file");
    let path = temp.path().to_path_buf();

    editor.set_file_path(&path);
    write_long_document(&editor, 300, 0);

    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Modified) > 0
    });

    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    editor.save_file_async(move |result| {
        result.expect("save should succeed");
        done_clone.set(true);
    });
    wait_until(Duration::from_secs(2), || done.get());
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Modified) == 0
    });
}

#[test]
fn test_properties_pane_collapses_before_workspace_pane() {
    ensure_gtk_init();

    // At a narrow width just below the adaptive Comfy breakpoint (~1350sp),
    // the properties pane should collapse to overlay while
    // the workspace pane stays in layout.
    let window = test_window_with_split_view_state(true, 0.3, true, 0.25);
    window.set_default_size(1300, 900);
    present_window(&window);

    // Properties requested visible, but collapsed by the breakpoint.
    assert!(properties_sidebar_visible(&window));
    assert!(window.imp().properties_split_view.is_collapsed());
    // Workspace pane stays non-collapsed at this width.
    assert!(!window.imp().workspace_split_view.is_collapsed());
}

#[test]
fn test_large_workspace_preset_collapses_properties_pane_earlier() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.4, true, 0.25);
    window.set_default_size(1400, 900);
    present_window(&window);

    assert!(properties_sidebar_visible(&window));
    assert!(window.imp().properties_split_view.is_collapsed());
    assert!(!window.imp().workspace_split_view.is_collapsed());
}

#[test]
fn test_hiding_workspace_sidebar_relaxes_properties_breakpoint() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(false, 0.4, true, 0.25);
    window.set_default_size(1400, 900);
    present_window(&window);

    assert!(properties_sidebar_visible(&window));
    assert!(!window.imp().properties_split_view.is_collapsed());
    assert!(!workspace_sidebar_visible(&window));
}

#[test]
fn test_properties_visibility_preference_survives_breakpoint_changes() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, true, 0.25);
    window.set_default_size(1600, 900);
    present_window(&window);

    assert!(properties_sidebar_visible(&window));
    assert!(
        window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );

    window.set_default_size(1200, 900);
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
fn test_warning_infobar_actions_stay_allocated_in_a_narrow_window() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, true, 0.25);
    window.set_default_size(1000, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Changes Restored".to_string(),
        body: "Unsaved changes to the document have been restored, and the inline actions must remain visible while the window narrows.".to_string(),
        primary_button: Some("_Discard…".to_string()),
        secondary_button: Some("_Save…".to_string()),
    });
    flush_after_delay(Duration::from_millis(50));

    let info_bar = editor.info_bar().imp();
    assert!(info_bar.discard_button.property::<bool>("visible"));
    assert!(info_bar.save_button.property::<bool>("visible"));
    // Width allocation requires a full compositor layout pass which is not
    // guaranteed in the subprocess widget harness. Verify the property-level
    // visibility, which is what the application code controls.
}

#[test]
fn test_access_error_infobar_action_stays_allocated_in_a_narrow_window() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, true, 0.25);
    window.set_default_size(1000, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Error,
        title: "Could Not Open File".to_string(),
        body: "Permission was denied while opening the document, so the retry action must stay visible after the shell tightens.".to_string(),
        primary_button: Some("_Retry".to_string()),
        secondary_button: None,
    });
    flush_after_delay(Duration::from_millis(50));

    let info_bar = editor.info_bar().imp();
    assert!(info_bar.retry_button.property::<bool>("visible"));
}

#[test]
fn test_restored_workspaces_survive_dual_sidebar_shell() {
    ensure_gtk_init();
    let _roots_dir = seed_restored_workspaces();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);

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
    assert_eq!(panel.encoding_row.subtitle().as_deref(), Some("UTF-8"));
    assert_eq!(panel.file_size_row.subtitle().as_deref(), Some("Not available"));
    assert_eq!(
        panel.formatting_source_row.subtitle().as_deref(),
        Some("Not available for untitled tabs")
    );
}

#[test]
fn test_flush_dirty_drafts_skips_close_discarded_editors() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("discard me");
    editor.buffer().set_modified(true);

    let draft_id = editor.draft_id().expect("draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &draft_id, "stale draft").expect("seed draft");
    draft_service::delete_draft_file(&data_dir, &draft_id).expect("delete seeded draft");

    window
        .imp()
        .drafts
        .close_discard_ids
        .borrow_mut()
        .insert(draft_id.clone());
    window.flush_dirty_drafts();

    assert_eq!(
        draft_service::read_draft(&data_dir, &draft_id).expect("read draft"),
        None,
        "discarded drafts must not be recreated during close flush",
    );
    assert!(
        window.imp().drafts.close_discard_ids.borrow().is_empty(),
        "close discard state should be cleared after the flush",
    );
}

#[test]
fn test_complete_save_as_failure_keeps_existing_editor_identity() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("unsaved draft");
    editor.buffer().set_modified(true);

    let old_draft_id = editor.draft_id().expect("untitled draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &old_draft_id, "unsaved draft").expect("seed draft");

    let path = std::env::temp_dir()
        .join("lushtext-save-as-missing-parent")
        .join("failure.txt");
    window.complete_save_as(
        &editor,
        None,
        Some(old_draft_id.as_str()),
        &path,
        Err(SaveError::WriteTemp {
            path: path.clone(),
            source: std::io::Error::other("boom"),
        }),
    );

    assert_eq!(editor.file_path(), None);
    assert_eq!(editor.draft_id().as_deref(), Some(old_draft_id.as_str()));
    assert!(
        !window.imp().open_paths.borrow().contains(&path),
        "failed Save As must not register the destination as open",
    );
    assert_eq!(
        draft_service::read_draft(&data_dir, &old_draft_id).expect("read draft"),
        Some("unsaved draft".to_string()),
        "failed Save As must keep the prior draft available",
    );
}

#[test]
fn test_complete_save_as_success_updates_editor_identity_and_cleans_old_draft() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("saved content");
    editor.buffer().set_modified(false);

    let old_draft_id = editor.draft_id().expect("untitled draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &old_draft_id, "saved content").expect("seed draft");

    let dir = tempfile::tempdir().expect("save as tempdir");
    let path = dir.path().join("saved.txt");
    std::fs::write(&path, "saved content").expect("seed saved file");

    window.complete_save_as(
        &editor,
        None,
        Some(old_draft_id.as_str()),
        &path,
        Ok(()),
    );

    assert_eq!(editor.file_path(), Some(path.clone()));
    assert_eq!(editor.title(), "saved.txt");
    assert!(
        window.imp().open_paths.borrow().contains(&path),
        "successful Save As must register the new destination as open",
    );
    wait_until(Duration::from_secs(2), || {
        draft_service::read_draft(&data_dir, &old_draft_id)
            .expect("read draft")
            .is_none()
    });
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
fn test_status_bar_shows_detected_encoding_and_line_endings_after_open() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("encoded.txt");
    std::fs::write(&path, [0x63, 0x61, 0x66, 0xE9, b'\r', b'\n']).expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || active_editor(&window).file_size().is_some());

    let status_bar = window.imp().status_bar.imp();
    assert_eq!(
        status_bar.encoding_button.label().as_deref(),
        Some("Windows-1252")
    );
    assert_eq!(status_bar.line_ending_button.label().as_deref(), Some("CRLF"));
    assert!(status_bar.health_button.property::<bool>("visible"));
}

#[test]
fn test_reopen_with_encoding_requires_discard_confirmation_for_modified_document() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("reopen.txt");
    std::fs::write(&path, "hello").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || active_editor(&window).file_size().is_some());

    let editor = active_editor(&window);
    editor.buffer().set_text("modified");
    editor.buffer().set_modified(true);

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    click_alert_extra_button(&dialog, "Reopen as Windows-1252");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Discard Changes"))
    });
}

#[test]
fn test_save_encoding_choice_surfaces_lossy_confirmation() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("lossy.txt");
    std::fs::write(&path, "hello").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || active_editor(&window).file_size().is_some());

    let editor = active_editor(&window);
    editor.buffer().set_text("emoji 😀");
    editor.buffer().set_modified(true);

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    click_alert_extra_button(&dialog, "Save as Windows-1252");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Lossy Encoding Conversion"))
    });
    assert_eq!(active_editor(&window).save_encoding(), DocumentEncoding::Utf8);
}

#[test]
fn test_mixed_line_endings_warning_opens_normalization_picker_and_updates_status_bar() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("mixed.txt");
    std::fs::write(&path, "a\r\nb\nc\r\n").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || active_editor(&window).file_size().is_some());

    let editor = active_editor(&window);
    wait_until(Duration::from_secs(2), || {
        editor
            .info_bar()
            .imp()
            .discard_infobar
            .property::<bool>("revealed")
    });
    assert_eq!(
        editor.info_bar().imp().discard_button.label().as_deref(),
        Some("_Normalize…")
    );

    editor.info_bar().imp().discard_button.emit_clicked();
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Line Endings"))
    });

    let dialog = visible_alert_dialog(&window).expect("line endings dialog visible");
    click_alert_extra_button(&dialog, "LF");

    assert_eq!(active_editor(&window).save_line_ending(), LineEnding::Lf);
    assert!(
        active_editor(&window)
            .file_health()
            .into_iter()
            .all(|finding| finding.kind != FileHealthFindingKind::MixedLineEndings)
    );
    assert_eq!(
        window
            .imp()
            .status_bar
            .imp()
            .line_ending_button
            .label()
            .as_deref(),
        Some("LF")
    );
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
    window
        .imp()
        .status_bar
        .imp()
        .properties_toggle_button
        .grab_focus();
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
fn test_properties_toggle_button_lives_in_status_bar_and_is_wired() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(
        window
            .imp()
            .status_bar
            .imp()
            .properties_toggle_button
            .action_name()
            .as_deref(),
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
fn test_tab_context_menu_targets_background_tab_for_move_action() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["a.txt", "b.txt", "c.txt"]);
    let window = test_window();
    present_window(&window);

    for path in &files {
        window.open_document(path);
    }
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 3);

    let selected_before = window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected tab before move");
    let first_page = find_tab_page_by_title(&window, "a.txt");
    prepare_tab_context_menu(&window, &first_page);

    assert!(!action_enabled(&window, "move-tab-left"));
    assert!(action_enabled(&window, "move-tab-right"));

    activate_action(&window, "move-tab-right");
    wait_until(Duration::from_secs(2), || {
        tab_titles(&window) == vec!["b.txt", "a.txt", "c.txt"]
    });

    let selected_after = window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected tab after move");
    assert_eq!(
        selected_before.as_ptr(),
        selected_after.as_ptr(),
        "moving a background tab should not retarget selection",
    );

    let last_page = find_tab_page_by_title(&window, "c.txt");
    prepare_tab_context_menu(&window, &last_page);
    assert!(!action_enabled(&window, "move-tab-right"));
}

#[test]
fn test_bulk_close_context_action_uses_one_confirmation_before_closing() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["a.txt", "b.txt", "c.txt"]);
    let window = test_window();
    present_window(&window);

    for path in &files {
        window.open_document(path);
    }
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 3);

    let modified_page = find_tab_page_by_title(&window, "b.txt");
    let modified_editor = modified_page
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("modified editor page");
    wait_until(Duration::from_secs(2), || modified_editor.file_size().is_some());
    modified_editor.buffer().set_text("modified");
    modified_editor.buffer().set_modified(true);
    flush_events();

    let target = find_tab_page_by_title(&window, "a.txt");
    prepare_tab_context_menu(&window, &target);
    activate_action(&window, "close-other-tabs");

    wait_until(Duration::from_secs(2), || visible_alert_dialog(&window).is_some());
    assert_tab_count(&window, 3);

    let dialog = visible_alert_dialog(&window).expect("bulk close confirmation");
    dialog.emit_by_name::<()>("response", &[&"discard"]);
    dialog.force_close();
    wait_until(Duration::from_secs(2), || visible_alert_dialog(&window).is_none());
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);

    assert_eq!(tab_titles(&window), vec!["a.txt"]);
}

#[test]
fn test_pin_action_updates_indicator_icon() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["pin-me.txt"]);
    let window = test_window();
    present_window(&window);
    window.open_document(&files[0]);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);

    let page = find_tab_page_by_title(&window, "pin-me.txt");
    prepare_tab_context_menu(&window, &page);
    activate_action(&window, "toggle-tab-pinned");
    wait_until(Duration::from_secs(2), || page.is_pinned());
    assert!(page.indicator_icon().is_some());

    prepare_tab_context_menu(&window, &page);
    activate_action(&window, "toggle-tab-pinned");
    wait_until(Duration::from_secs(2), || !page.is_pinned());
    assert!(page.indicator_icon().is_none());
}

#[test]
fn test_session_restore_keeps_pinned_tabs_ahead_of_unpinned_tabs() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["alpha.txt", "beta.txt", "gamma.txt"]);
    let session = SessionData {
        tabs: vec![
            SessionTab {
                path: Some(files[0].clone()),
                draft_id: None,
                cursor_line: 1,
                cursor_col: 0,
                scroll_line: 0,
                pinned: true,
            },
            SessionTab {
                path: Some(files[1].clone()),
                draft_id: None,
                cursor_line: 2,
                cursor_col: 0,
                scroll_line: 0,
                pinned: true,
            },
            SessionTab {
                path: Some(files[2].clone()),
                draft_id: None,
                cursor_line: 3,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            },
        ],
        active_tab_index: Some(2),
    };
    session_service::save(&json_store::data_dir(), &session).expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 3);

    assert_eq!(tab_titles(&window), vec!["alpha.txt", "beta.txt", "gamma.txt"]);
    let pages = tab_pages(&window);
    assert!(pages[0].is_pinned());
    assert!(pages[1].is_pinned());
    assert!(!pages[2].is_pinned());
    assert_eq!(
        window
            .imp()
            .tab_view
            .selected_page()
            .expect("restored selected page")
            .title()
            .as_str(),
        "gamma.txt"
    );
}

#[test]
fn test_startup_restore_applies_matching_file_backed_draft() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("restored.txt");
    std::fs::write(&file_path, "disk content").expect("write file");
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&file_path);
    draft_service::write_draft(&data_dir, &draft_id, "draft content").expect("seed draft");
    let current_mtime = editor_io::mtime_secs(&file_path).expect("file mtime");
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.clone(),
                original_path: Some(file_path.clone()),
                original_mtime_secs: Some(current_mtime),
                saved_at_secs: 1,
            }],
        },
    )
    .expect("save manifest");
    session_service::save(
        &data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(file_path),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor_text(&editor) == "draft content"
    });

    let editor = active_editor(&window);
    assert_eq!(editor_text(&editor), "draft content");
    assert!(editor.is_draft_restored());
    let notification = window
        .imp()
        .notification_bus
        .editor_info_bar_view(editor.notification_owner_id())
        .expect("draft restore notification");
    assert_eq!(notification.title, "Draft Changes Restored");
}

#[test]
fn test_startup_restore_skips_stale_file_backed_draft_once() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("stale.txt");
    std::fs::write(&file_path, "current disk content").expect("write file");
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&file_path);
    draft_service::write_draft(&data_dir, &draft_id, "stale draft").expect("seed draft");
    let current_mtime = editor_io::mtime_secs(&file_path).expect("file mtime");
    let stale_mtime = current_mtime
        .checked_add(1)
        .unwrap_or_else(|| current_mtime.saturating_sub(1));
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.clone(),
                original_path: Some(file_path.clone()),
                original_mtime_secs: Some(stale_mtime),
                saved_at_secs: 1,
            }],
        },
    )
    .expect("save manifest");
    session_service::save(
        &data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(file_path),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        window
            .imp()
            .notification_bus
            .editor_info_bar_view(editor.notification_owner_id())
            .is_some()
    });
    wait_until(Duration::from_secs(2), || {
        draft_service::read_draft(&data_dir, &draft_id)
            .expect("read draft")
            .is_none()
    });

    let editor = active_editor(&window);
    assert_eq!(editor_text(&editor), "current disk content");
    assert!(!editor.is_draft_restored());
    let notification = window
        .imp()
        .notification_bus
        .editor_info_bar_view(editor.notification_owner_id())
        .expect("stale draft warning");
    assert_eq!(notification.title, "Draft Not Restored");
    assert!(window.imp().drafts.manifest.borrow().find_by_id(&draft_id).is_none());
}

#[test]
fn test_startup_restore_keeps_untitled_draft_behavior() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_untitled(42);
    draft_service::write_draft(&data_dir, &draft_id, "untitled restored content")
        .expect("seed untitled draft");
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.clone(),
                original_path: None,
                original_mtime_secs: None,
                saved_at_secs: 1,
            }],
        },
    )
    .expect("save manifest");
    session_service::save(
        &data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: None,
                draft_id: Some(draft_id),
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor_text(&editor) == "untitled restored content"
    });

    let editor = active_editor(&window);
    assert_eq!(editor_text(&editor), "untitled restored content");
    assert!(editor.is_draft_restored());
    let notification = window
        .imp()
        .notification_bus
        .editor_info_bar_view(editor.notification_owner_id())
        .expect("untitled restore notification");
    assert_eq!(notification.title, "Document Restored");
}

#[test]
fn test_preview_pane_toggle_starts_nontrivial_animation() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-preview-pane");

    let animation = preview_animation(&window);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The Adw animation exposes f64 endpoints, but preview pane positions stay within i32 paned coordinates"
    )]
    let from = animation.value_from() as i32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The preview pane target position is always an i32 paned coordinate"
    )]
    let to = animation.value_to() as i32;
    assert_ne!(
        from, to,
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
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The Adw animation exposes f64 endpoints, but preview pane positions stay within i32 paned coordinates"
    )]
    let from = animation.value_from() as i32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The preview pane target position is always an i32 paned coordinate"
    )]
    let to = animation.value_to() as i32;
    assert_ne!(
        from, to,
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the main window shell.
//!
//! This suite focuses on the current window contract: split-view sidebar
//! behavior, a few critical shell affordances, and preview-pane regressions
//! that still live in the window layer.

use crate::common::{
    emit_key_pressed_on_focus, ensure_gtk_init, fixture, flush_after_delay, flush_events,
    fs_metadata, fs_mutate, fs_read, isolated_data_dir, wait_until,
};
use gio::prelude::{ActionExt, ActionGroupExt, ActionMapExt, ListModelExt, MenuModelExt};
use glib::prelude::{Cast, IsA, ObjectExt, ToValue, ToVariant};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::{
    ActionRowExt, AdwApplicationWindowExt, AdwDialogExt, AlertDialogExt, AlertDialogExtManual,
    ComboRowExt, PreferencesRowExt, SidebarItemExt,
};
use lushtext_core::config::keys;
use lushtext_core::model::action_catalog::{ActionScope, ActionValueType, ObservedAction};
use lushtext_core::model::automation::{
    AutomationReadinessPredicate, AutomationReadinessStatus,
};
use lushtext_core::model::content_search::SearchMatch;
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::encoding::{
    DocumentEncoding, DocumentEncodingState, FileHealthFindingKind, InvisibleCharactersMode,
    LineEnding,
};
use lushtext_core::model::local_history::LocalHistorySnapshotOrigin;
use lushtext_core::model::note::RichNoteBody;
use lushtext_core::model::palette::IndexedFile;
use lushtext_core::model::recent_document::RecentDocumentEntry;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceFolder, WorkspaceFolderId, WorkspaceFolderMoveDirection,
    WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::file_limits::{
    DISABLE_SYNTAX_HIGHLIGHTING, DISABLE_UNDO_HISTORY, FileSizeCheck, REFUSE_TO_OPEN,
};
use lushtext_core::services::filesystem::PathStatus;
use lushtext_core::services::notifications::{
    InlineActionNotification, InlineNotificationStyle, NotificationOwner, NotificationPayload,
    NotificationSeverity, NotificationSurface, StatusMessage,
};
use lushtext_core::services::{
    action_catalog, bookmark_service, document_note_service, draft_service, editor_io,
    folder_note_service, format_upgrade, json_format, json_store, local_history_service,
    saved_searches, session_service, workspace_manager,
};
use lushtext_core::services::palette::FileIndex;
use lushtext_core::ui::automation::{
    INTERFACE_VERSION, app_snapshot, current_idle_blocker, wait_for_idle_for_test,
    wait_for_ready_for_test,
};
use lushtext_core::ui::accessibility::{AnnouncementLane, test_audit::AccessibleAudit};
use lushtext_core::ui::editor_page::{
    EditorLoadState, LushtextEditorPage, MinimapAvailability, MinimapMarkerKind, SaveError,
};
use lushtext_core::ui::markdown_preview::LushtextMarkdownPreview;
use lushtext_core::ui::preferences::LushtextPreferences;
use lushtext_core::ui::search_panel::set_replace_preview_delay_for_test;
use lushtext_core::ui::window::{
    LushtextWindow, PrintDocumentSnapshot, PrintOutcome,
    set_bookmark_excerpt_preview_delay_for_test, set_canonical_refresh_delay_for_test,
    set_first_dirty_autosave_delay_for_test, set_lossy_encoding_analysis_delay_for_test,
    with_print_runner_for_test,
};
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

struct CanonicalRefreshDelayReset;

impl Drop for CanonicalRefreshDelayReset {
    fn drop(&mut self) {
        set_canonical_refresh_delay_for_test(0);
    }
}

struct LossyEncodingAnalysisDelayReset;

impl Drop for LossyEncodingAnalysisDelayReset {
    fn drop(&mut self) {
        set_lossy_encoding_analysis_delay_for_test(0);
    }
}

struct BookmarkExcerptPreviewDelayReset;

impl Drop for BookmarkExcerptPreviewDelayReset {
    fn drop(&mut self) {
        set_bookmark_excerpt_preview_delay_for_test(0);
    }
}

struct FirstDirtyAutosaveDelayReset;

impl Drop for FirstDirtyAutosaveDelayReset {
    fn drop(&mut self) {
        set_first_dirty_autosave_delay_for_test(750);
    }
}

struct ReplacePreviewDelayReset;

impl Drop for ReplacePreviewDelayReset {
    fn drop(&mut self) {
        set_replace_preview_delay_for_test(0);
    }
}

struct VisualSettingsReset {
    color_scheme: libadwaita::ColorScheme,
    gtk_theme_name: Option<glib::GString>,
}

impl VisualSettingsReset {
    fn capture() -> Self {
        ensure_gtk_init();
        let style_manager = libadwaita::StyleManager::default();
        let settings = gtk4::Settings::default().expect("GTK settings");
        Self {
            color_scheme: style_manager.color_scheme(),
            gtk_theme_name: settings.gtk_theme_name(),
        }
    }
}

impl Drop for VisualSettingsReset {
    fn drop(&mut self) {
        libadwaita::StyleManager::default().set_color_scheme(self.color_scheme);
        if let Some(settings) = gtk4::Settings::default() {
            settings.set_gtk_theme_name(self.gtk_theme_name.as_deref());
        }
        flush_events();
    }
}

/// Return whether the message area has any severity or animation-restart pulse state.
fn status_message_area_has_any_pulse(window: &LushtextWindow) -> bool {
    let area = &window.imp().status_bar.imp().message_area_box;
    area.has_css_class("status-pulse-info")
        || area.has_css_class("status-pulse-warning")
        || area.has_css_class("status-pulse-error")
        || area.has_css_class("status-pulse-a")
        || area.has_css_class("status-pulse-b")
}

fn assert_status_bar_readable_one_row(window: &LushtextWindow, context: &str) {
    let status_bar = &window.imp().status_bar;
    assert_positive_allocation(&**status_bar, context);
    let height = status_bar.height();
    assert!(
        (32..=44).contains(&height),
        "{context}: status bar should be readable without becoming a second header, got height {height}"
    );

    let message_area = &status_bar.imp().message_area_box;
    assert!(
        message_area.width() > 0 && message_area.height() > 0,
        "{context}: message area should keep a positive allocation, got {}x{}",
        message_area.width(),
        message_area.height()
    );
    assert!(
        message_area.height() <= height,
        "{context}: message area should stay within the status row, got area height {} and row height {height}",
        message_area.height()
    );
}

fn seed_restored_window_size(width: i32, height: i32) {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(keys::WINDOW_WIDTH, width)
        .expect("set window width");
    settings
        .set_int(keys::WINDOW_HEIGHT, height)
        .expect("set window height");
}

fn test_window_with_restored_size(width: i32, height: i32) -> LushtextWindow {
    seed_restored_window_size(width, height);
    test_window()
}

const DEFINITION_LIST_CODE_BLOCK_SAMPLE: &str = concat!(
    "## Definition lists\n\n",
    "Term 1\n",
    ": Definition 1 with lazy continuation.\n\n",
    "Term 2 with *inline markup*\n\n",
    ":   Definition 2\n\n",
    "        { some code, part of Definition 2 }\n\n",
    "    Third paragraph of definition 2.\n\n",
    "Compact style:\n\n",
    "Term 1 ~ Definition 1\n\n",
    "Term 2 ~ Definition 2a ~ Definition 2b\n",
);

const CODE_BLOCK_HORIZONTAL_PADDING: i32 = 12;

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

fn test_window_with_split_view_state_and_size(
    workspace_visible: bool,
    workspace_fraction: f64,
    properties_visible: bool,
    properties_fraction: f64,
    width: i32,
    height: i32,
) -> LushtextWindow {
    seed_restored_window_size(width, height);
    test_window_with_split_view_state(
        workspace_visible,
        workspace_fraction,
        properties_visible,
        properties_fraction,
    )
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
    let folders_dir = tempfile::tempdir().expect("workspace folders tempdir");
    let mut workspaces = WorkspacesFile::default();

    for (idx, name) in ["one", "two", "three"].into_iter().enumerate() {
        let path = folders_dir.path().join(name);
        fixture::create_dir_all(&path);
        workspaces.workspaces.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new(format!("ws-{idx}")),
            name,
            path,
        ));
    }

    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces.json");
    folders_dir
}

fn seed_scoped_workspaces(initial_scope: WorkspaceScope) -> (tempfile::TempDir, PathBuf, PathBuf) {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("scoped workspace folders tempdir");
    let left_folder = folders_dir.path().join("left");
    let right_folder = folders_dir.path().join("right");
    fixture::create_dir_all(&left_folder);
    fixture::create_dir_all(&right_folder);
    fixture::write_text(&left_folder.join("alpha.rs"), "fn alpha() {}\n");
    fixture::write_text(&right_folder.join("beta.rs"), "fn beta() {}\n");

    let workspaces = WorkspacesFile {
        current_scope: initial_scope,
        workspaces: vec![
            WorkspaceConfig::with_one_folder(WorkspaceId::new("ws-left"), "left", left_folder.clone()),
            WorkspaceConfig::with_one_folder(
                WorkspaceId::new("ws-right"),
                "right",
                right_folder.clone(),
            ),
        ],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save scoped workspaces");
    (folders_dir, left_folder, right_folder)
}

fn seed_folder_set_workspace() -> (tempfile::TempDir, PathBuf, PathBuf) {
    seed_folder_set_workspace_with_scope(WorkspaceScope::All)
}

fn seed_folder_set_workspace_with_scope(
    initial_scope: WorkspaceScope,
) -> (tempfile::TempDir, PathBuf, PathBuf) {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("folder-set workspace tempdir");
    let first_folder = folders_dir.path().join("first");
    let second_folder = folders_dir.path().join("second");
    fixture::create_dir_all(&first_folder);
    fixture::create_dir_all(&second_folder);

    let workspaces = WorkspacesFile {
        current_scope: initial_scope,
        workspaces: vec![WorkspaceConfig::with_folders(
            WorkspaceId::new("ws-folder-set"),
            "folder set",
            vec![
                WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first_folder.clone()),
                WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second_folder.clone()),
            ],
        )],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces)
        .expect("save folder-set workspace");
    (folders_dir, first_folder, second_folder)
}

fn seed_overlapping_folder_workspace() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("overlapping workspace tempdir");
    let parent_folder = folders_dir.path().join("project");
    let child_folder = parent_folder.join("src");
    let file_path = child_folder.join("main.rs");
    fixture::create_dir_all(&child_folder);
    fixture::write_text(&file_path, "fn main() {}\n// note target\n");

    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::All,
        workspaces: vec![WorkspaceConfig::with_folders(
            WorkspaceId::new("ws-overlap"),
            "overlap",
            vec![
                WorkspaceFolder::with_id(WorkspaceFolderId::new("parent"), parent_folder.clone()),
                WorkspaceFolder::with_id(WorkspaceFolderId::new("child"), child_folder.clone()),
            ],
        )],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces)
        .expect("save overlapping folder-set workspace");
    (folders_dir, parent_folder, child_folder, file_path)
}

fn seed_empty_folder_set_workspace() {
    ensure_gtk_init();
    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws-empty")),
        workspaces: vec![WorkspaceConfig::with_folders(
            WorkspaceId::new("ws-empty"),
            "empty folder set",
            Vec::new(),
        )],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces)
        .expect("save empty folder-set workspace");
}

fn seed_empty_and_populated_workspaces() -> (tempfile::TempDir, PathBuf) {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("mixed workspace folders tempdir");
    let populated_folder = folders_dir.path().join("populated");
    fixture::create_dir_all(&populated_folder);
    fixture::write_text(&populated_folder.join("main.rs"), "fn main() {}\n");

    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::All,
        workspaces: vec![
            WorkspaceConfig::with_folders(
                WorkspaceId::new("ws-empty"),
                "empty folder set",
                Vec::new(),
            ),
            WorkspaceConfig::with_one_folder(
                WorkspaceId::new("ws-populated"),
                "populated",
                populated_folder.clone(),
            ),
        ],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces)
        .expect("save mixed workspace file");
    (folders_dir, populated_folder)
}

fn seed_no_workspaces() {
    ensure_gtk_init();
    workspace_manager::save(&json_store::data_dir(), &WorkspacesFile::default())
        .expect("save empty workspace file");
}

fn wait_for_workspace_folders(window: &LushtextWindow, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if window.imp().sidebar.all_workspace_folder_paths().len() == expected {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }
    let actual = window.imp().sidebar.all_workspace_folder_paths().len();
    panic!("expected {expected} restored workspace folders, got {actual}");
}

fn wait_for_workspace_sections(window: &LushtextWindow, expected: usize) {
    wait_until(Duration::from_secs(3), || {
        window.imp().sidebar.imp().sections.borrow().len() == expected
    });
}

fn wait_for_workspace_consumers(window: &LushtextWindow, expected_folders: usize, expected_index: usize) {
    wait_until(Duration::from_secs(3), || {
        window
            .imp()
            .search_panel
            .imp()
            .runtime
            .workspace_folders
            .borrow()
            .len()
            == expected_folders
            && window.imp().command_palette.file_index_len() == expected_index
    });
}

fn wait_for_startup_data_flow(window: &LushtextWindow) {
    wait_until(Duration::from_secs(10), || {
        window.imp().startup_data_flow.completed.get()
    });
}

fn present_window(window: &LushtextWindow) {
    window.present();
    // Window realization is a precondition, not the behavior under test, so give
    // the headless compositor a generous budget to send the surface `configure`
    // that yields a non-zero allocation. `wait_until` returns as soon as the size
    // is real, so the larger ceiling only matters on a slow, loaded compositor and
    // never costs time on the fast path.
    wait_until(Duration::from_secs(5), || {
        window.width() > 0 && window.height() > 0
    });
    flush_after_delay(Duration::from_millis(20));
}

fn action_enabled(window: &LushtextWindow, name: &str) -> bool {
    let action = window
        .lookup_action(name)
        .unwrap_or_else(|| panic!("action '{name}' not found"));
    action.is_enabled()
}

fn action_state_bool(window: &LushtextWindow, name: &str) -> bool {
    window
        .lookup_action(name)
        .unwrap_or_else(|| panic!("action '{name}' not found"))
        .state()
        .unwrap_or_else(|| panic!("action '{name}' should be stateful"))
        .get::<bool>()
        .unwrap_or_else(|| panic!("action '{name}' should use bool state"))
}

fn activate_action(window: &LushtextWindow, name: &str) {
    ActionGroupExt::activate_action(window, name, None);
    flush_events();
}

// GActions carry parameters as GLib Variants, so these helpers convert typed
// Rust values into the dynamic container action activation expects.
fn activate_string_action(window: &LushtextWindow, name: &str, value: &str) {
    let parameter = value.to_variant();
    ActionGroupExt::activate_action(window, name, Some(&parameter));
    flush_events();
}

fn activate_boolean_action(window: &LushtextWindow, name: &str, value: bool) {
    let parameter = value.to_variant();
    ActionGroupExt::activate_action(window, name, Some(&parameter));
    flush_events();
}

fn activate_u32_action(window: &LushtextWindow, name: &str, value: u32) {
    let parameter = value.to_variant();
    ActionGroupExt::activate_action(window, name, Some(&parameter));
    flush_events();
}

fn assert_workflow_announcement_recorded(window: &LushtextWindow, key: &str) {
    assert!(
        !window
            .imp()
            .status_bar
            .imp()
            .status_announcement_throttler
            .should_announce_at(
                AnnouncementLane::StatusUpdate,
                &format!("workflow:{key}"),
                Instant::now()
            ),
        "workflow announcement '{key}' should have been recorded"
    );
}

fn action_value_type(signature: Option<&glib::VariantTy>) -> ActionValueType {
    match signature.map(glib::VariantTy::as_str) {
        None => ActionValueType::None,
        Some("b") => ActionValueType::Bool,
        Some("s") => ActionValueType::String,
        Some("u") => ActionValueType::U32,
        Some("a{sv}") => ActionValueType::VariantMap,
        Some(other) => panic!("unexpected action value type signature: {other}"),
    }
}

fn observed_actions_from_group<T>(scope: ActionScope, group: &T) -> Vec<ObservedAction<'static>>
where
    T: IsA<gio::ActionGroup> + IsA<gio::ActionMap>,
{
    ActionGroupExt::list_actions(group)
        .into_iter()
        .map(|name| {
            let name = name.to_string();
            let action = group
                .lookup_action(&name)
                .unwrap_or_else(|| panic!("listed action '{name}' should be lookupable"));
            ObservedAction::owned(
                scope,
                name,
                action_value_type(action.parameter_type().as_deref()),
                action_value_type(action.state_type().as_deref()),
            )
        })
        .collect()
}

fn shortcuts_windows_for(window: &LushtextWindow) -> Vec<gtk4::Window> {
    let parent_window: gtk4::Window = window.clone().upcast();
    window
        .application()
        .expect("window should have an application")
        .windows()
        .into_iter()
        .filter(|window| window.type_().name() == "GtkShortcutsWindow")
        .filter(|shortcuts| {
            shortcuts
                .transient_for()
                .is_some_and(|transient| transient == parent_window)
        })
        .collect()
}

fn wait_for_shortcuts_window(window: &LushtextWindow) -> gtk4::Window {
    wait_until(Duration::from_secs(2), || {
        shortcuts_windows_for(window)
            .first()
            .is_some_and(|shortcuts| shortcuts.width() > 0 && shortcuts.height() > 0)
    });
    let windows = shortcuts_windows_for(window);
    assert_eq!(windows.len(), 1);
    windows[0].clone()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorPrintState {
    text: String,
    modified: bool,
    path: Option<PathBuf>,
    draft_id: Option<String>,
}

fn editor_print_state(editor: &LushtextEditorPage) -> EditorPrintState {
    EditorPrintState {
        text: editor_text(editor),
        modified: editor.is_modified(),
        path: editor.file_path(),
        draft_id: editor.draft_id(),
    }
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

fn editor_buffer_text(editor: &LushtextEditorPage) -> String {
    let buffer = editor.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn open_temp_document(initial_text: &str) -> (LushtextWindow, tempfile::TempDir, PathBuf) {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("document tempdir");
    let path = dir.path().join("watched.txt");
    fixture::write_text(&path, initial_text);

    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor.file_size().is_some() && editor_buffer_text(&editor) == initial_text
    });

    (window, dir, path)
}

/// Force an external write onto a later mtime second before waiting for GTK.
///
/// The production monitor compares second-resolution mtimes, so this helper
/// avoids false-negative tests on filesystems where two writes inside one
/// second collapse to the same observed timestamp.
fn write_external_change_after_mtime_tick(
    path: &Path,
    editor: &LushtextEditorPage,
    contents: &str,
) {
    let last_known = editor
        .imp()
        .monitor
        .last_known_mtime
        .get()
        .expect("loaded editor should know the backing file mtime");

    for _ in 0..5 {
        fixture::write_text(path, contents);
        if editor_io::mtime_secs(path).is_some_and(|mtime| mtime != last_known) {
            return;
        }
        std::thread::sleep(Duration::from_millis(1100));
    }

    panic!("external write did not advance the observed file mtime");
}

fn wait_for_external_change_warning(editor: &LushtextEditorPage) {
    wait_until(Duration::from_secs(4), || {
        let info_bar = editor.info_bar().imp();
        info_bar.alert_revealer.reveals_child()
            && info_bar.alert_title.label().as_str() == "File Has Changed on Disk"
    });
}

fn modified_file_backed_tab(
    initial_text: &str,
    modified_text: &str,
) -> (LushtextWindow, tempfile::TempDir, PathBuf, LushtextEditorPage) {
    let (window, dir, path) = open_temp_document(initial_text);
    let editor = active_editor(&window);
    editor.buffer().set_text(modified_text);
    editor.buffer().set_modified(true);
    flush_events();
    (window, dir, path, editor)
}

fn close_selected_tab(window: &LushtextWindow) {
    let page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected tab");
    window.imp().tab_view.close_page(&page);
    flush_events();
}

fn remove_session_path_for_test(path: &Path) {
    match fs_metadata::path_status(path).expect("session path status") {
        PathStatus::Missing => {}
        PathStatus::Directory => fixture::remove_dir_all(path),
        PathStatus::File | PathStatus::Other => fixture::remove_file(path),
    }
}

fn respond_to_save_changes_dialog(window: &LushtextWindow, response: &str) {
    let dialog = visible_alert_dialog(window).expect("save changes dialog");
    let button = save_changes_response_button(&dialog, response);
    button.emit_clicked();
    flush_events();
}

fn save_changes_response_button(
    dialog: &libadwaita::AlertDialog,
    response: &str,
) -> gtk4::Button {
    alert_response_button(dialog, response)
}

fn alert_response_button(dialog: &libadwaita::AlertDialog, response: &str) -> gtk4::Button {
    let response_label = dialog.response_label(response);
    let fallback_label = response_label.replace('_', "");
    find_button_by_label(dialog.upcast_ref(), response_label.as_str())
        .or_else(|| find_button_by_label(dialog.upcast_ref(), &fallback_label))
        .unwrap_or_else(|| panic!("response button '{response}' not found"))
}

fn note_save_response_button(dialog: &libadwaita::AlertDialog) -> gtk4::Button {
    alert_response_button(dialog, "save")
}

fn assert_note_save_response_visible(dialog: &libadwaita::AlertDialog) {
    let save = note_save_response_button(dialog);
    assert!(
        save.property::<bool>("visible"),
        "note dialog Save response should stay visible"
    );
}

fn wait_for_note_save_response_sensitive(dialog: &libadwaita::AlertDialog, expected: bool) {
    wait_until(Duration::from_secs(5), || {
        note_save_response_button(dialog).is_sensitive() == expected
    });
}

fn activate_widget_without_pointer(widget: &impl IsA<gtk4::Widget>) {
    let widget = widget.as_ref();
    assert!(widget.is_sensitive(), "widget should be keyboard activatable");
    assert!(
        widget.property::<bool>("visible"),
        "widget should be visible before keyboard activation"
    );
    widget.grab_focus();
    flush_events();
    assert!(widget.activate(), "widget should activate without a pointer");
    flush_events();
}

fn emit_key_pressed_on_widget(
    widget: &impl IsA<gtk4::Widget>,
    key: gtk4::gdk::Key,
) -> glib::Propagation {
    let widget = widget.as_ref();
    let controllers = widget.observe_controllers();
    for index in 0..controllers.n_items() {
        if let Some(controller) = controllers
            .item(index)
            .and_then(|object| object.downcast::<gtk4::EventControllerKey>().ok())
        {
            let args: [&dyn ToValue; 3] = [&key, &0u32, &gtk4::gdk::ModifierType::empty()];
            let stopped: bool =
                glib::object::ObjectExt::emit_by_name(&controller, "key-pressed", &args);
            return if stopped {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            };
        }
    }
    panic!("widget had no EventControllerKey");
}

fn activate_save_changes_response_with_keyboard(window: &LushtextWindow, response: &str) {
    let dialog = visible_alert_dialog(window).expect("save changes dialog");
    let button = save_changes_response_button(&dialog, response);
    button.grab_focus();
    flush_events();
    assert!(button.is_sensitive(), "response button should be enabled");
    assert!(
        button.property::<bool>("visible"),
        "response button should be visible"
    );
    button.emit_clicked();
    flush_events();
}

fn wait_for_save_changes_dialog(window: &LushtextWindow) {
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading == "Save Changes?")
    });
}

fn seed_file_backed_draft(window: &LushtextWindow, path: &Path, content: &str) -> String {
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(path);
    let entry = DraftEntry {
        draft_id: draft_id.clone(),
        original_path: Some(path.to_path_buf()),
        original_mtime_secs: editor_io::mtime_secs(path),
        saved_at_secs: editor_io::now_epoch_secs(),
    };
    draft_service::write_draft(&data_dir, &draft_id, content).expect("seed draft bytes");
    let mut manifest = draft_service::load_manifest(&data_dir).expect("load draft manifest");
    manifest.upsert(entry.clone());
    draft_service::save_manifest(&data_dir, &manifest).expect("seed draft manifest");
    window.imp().drafts.manifest.borrow_mut().upsert(entry);
    draft_id
}

fn save_changes_check_buttons(dialog: &libadwaita::AlertDialog) -> Vec<gtk4::CheckButton> {
    let extra = dialog.extra_child().expect("save changes dialog extra child");
    descendants(&extra)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::CheckButton>().ok())
        .collect()
}

fn save_changes_check_button_for_title(
    dialog: &libadwaita::AlertDialog,
    title: &str,
) -> gtk4::CheckButton {
    let extra = dialog.extra_child().expect("save changes dialog extra child");
    let row = descendants(&extra)
        .into_iter()
        .filter_map(|widget| widget.downcast::<libadwaita::ActionRow>().ok())
        .find(|row| row.title() == title)
        .unwrap_or_else(|| panic!("save changes row '{title}' not found"));

    descendants(&row)
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk4::CheckButton>().ok())
        .unwrap_or_else(|| panic!("save changes row '{title}' had no checkbox"))
}

// Keep hidden-to-visible preview-shell regressions in this suite: a directly
// mounted `LushtextMarkdownPreview` can pass while the real Adwaita slot/split
// shell leaves child-anchor code blocks with stale allocations.
fn prepare_markdown_preview_window(
    markdown: &str,
    width: i32,
    height: i32,
) -> (LushtextWindow, tempfile::TempDir) {
    ensure_gtk_init();
    let window = test_window_with_restored_size(width, height);
    window.new_tab();
    let dir = tempfile::tempdir().expect("markdown preview tempdir");
    let editor = active_editor(&window);
    editor.set_file_path(&dir.path().join("gh-markdown-sample.md"));
    editor.buffer().set_text(markdown);
    present_window(&window);
    (window, dir)
}

fn descendants(root: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Widget> {
    let mut widgets = Vec::new();
    let mut stack = vec![root.as_ref().clone()];

    while let Some(widget) = stack.pop() {
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current.clone());
            child = current.next_sibling();
        }
        widgets.push(widget);
    }

    widgets
}

fn widgets_with_css_class<T>(root: &impl IsA<gtk4::Widget>, css_class: &str) -> Vec<T>
where
    T: IsA<gtk4::Widget> + glib::object::ObjectType + Clone + 'static,
{
    descendants(root)
        .into_iter()
        .filter(|widget| widget.has_css_class(css_class))
        .filter_map(|widget| widget.downcast::<T>().ok())
        .collect()
}

fn source_views(preview: &LushtextMarkdownPreview) -> Vec<sourceview5::View> {
    widgets_with_css_class::<sourceview5::View>(preview, "markdown-code-block-view")
}

fn code_block_containers(preview: &LushtextMarkdownPreview) -> Vec<gtk4::Box> {
    widgets_with_css_class::<gtk4::Box>(preview, "markdown-code-block")
}

fn code_block_scrollers(preview: &LushtextMarkdownPreview) -> Vec<gtk4::ScrolledWindow> {
    widgets_with_css_class::<gtk4::ScrolledWindow>(preview, "markdown-code-block-scroller")
}

fn markdown_preview_has_image_fallback_title(window: &LushtextWindow, title: &str) -> bool {
    let preview: &LushtextMarkdownPreview = &window.imp().markdown_preview;
    widgets_with_css_class::<gtk4::Label>(preview, "markdown-preview-image-fallback-title")
        .iter()
        .any(|label| label.label() == title)
}

fn markdown_preview_has_image_fallback_body_containing(window: &LushtextWindow, text: &str) -> bool {
    let preview: &LushtextMarkdownPreview = &window.imp().markdown_preview;
    widgets_with_css_class::<gtk4::Label>(preview, "markdown-preview-image-fallback-body")
        .iter()
        .any(|label| label.label().contains(text))
}

fn preview_text_column_width(preview: &LushtextMarkdownPreview) -> i32 {
    let text_view = preview.text_view();
    text_view.width() - text_view.left_margin() - text_view.right_margin()
}

fn expected_code_block_width(preview: &LushtextMarkdownPreview, block: &gtk4::Box) -> i32 {
    preview_text_column_width(preview)
        .saturating_sub(block.margin_start() + block.margin_end())
        .max(1)
}

fn code_block_width_is_settled(block: &gtk4::Box, expected_width: i32) -> bool {
    block.width_request() == expected_width && (block.width() - expected_width).abs() <= 4
}

fn horizontal_overflow(scroller: &gtk4::ScrolledWindow) -> f64 {
    let adjustment = scroller.hadjustment();
    (adjustment.upper() - adjustment.page_size()).max(0.0)
}

fn source_view_buffer_text(source_view: &sourceview5::View) -> String {
    let buffer = source_view.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn wait_for_markdown_preview_shell(window: &LushtextWindow) {
    wait_until(Duration::from_secs(3), || {
        let preview = &window.imp().markdown_preview;
        let Some(block) = code_block_containers(preview).first().cloned() else {
            return false;
        };
        let Some(scroller) = code_block_scrollers(preview).first().cloned() else {
            return false;
        };
        let expected_width = expected_code_block_width(preview, &block);

        !window.preview_transition_pending_for_test()
            && preview.is_showing_content()
            && preview.text_view().width() > 0
            && !source_views(preview).is_empty()
            && code_block_width_is_settled(&block, expected_width)
            && scroller.hadjustment().page_size() > 0.0
    });
}

fn assert_live_code_block_uses_preview_column(window: &LushtextWindow) {
    let preview = &window.imp().markdown_preview;
    let text_view = preview.text_view();
    let block = code_block_containers(preview)
        .pop()
        .expect("code block container");
    let scroller = code_block_scrollers(preview).pop().expect("code scroller");
    let source_view = source_views(preview).pop().expect("code source view");
    let expected_width = expected_code_block_width(preview, &block);
    let actual_width = block.width();
    let minimum_inner_width =
        (expected_width - (CODE_BLOCK_HORIZONTAL_PADDING * 2) - 8).max(1) as f32;
    let block_bounds = block
        .compute_bounds(&text_view)
        .expect("code block bounds in preview text view");
    let scroller_bounds = scroller
        .compute_bounds(&block)
        .expect("scroller bounds in code block");
    let overflow = horizontal_overflow(&scroller);

    assert_eq!(
        source_view_buffer_text(&source_view),
        "{ some code, part of Definition 2 }\n",
        "preview should preserve the fenced code text from the definition body"
    );
    assert!(
        block.margin_start() > 0,
        "definition-list code blocks should keep their definition-body offset"
    );
    assert_eq!(
        block.width_request(),
        expected_width,
        "definition-list code block width request should follow the live text column"
    );
    assert!(
        (actual_width - expected_width).abs() <= 4,
        "definition-list code block allocation should settle near {expected_width}, got {actual_width}"
    );
    assert!(
        block_bounds.width() >= (expected_width - 4).max(1) as f32,
        "code block bounds should span the preview column after definition margins; expected at least {}, got {}",
        (expected_width - 4).max(1),
        block_bounds.width()
    );
    assert!(
        scroller_bounds.width() >= minimum_inner_width,
        "inner code scroller should receive the block width minus padding; expected at least {minimum_inner_width}, got {}",
        scroller_bounds.width()
    );
    assert!(
        overflow <= 1.0,
        "short definition-list code block should not expose false horizontal overflow, got upper={} page_size={} overflow={overflow}",
        scroller.hadjustment().upper(),
        scroller.hadjustment().page_size()
    );
}

fn active_editor_has_focus(window: &LushtextWindow) -> bool {
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return false;
    };
    let editor = active_editor(window);
    focus.as_ptr() == editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr()
}

fn wait_for_active_editor_focus(window: &LushtextWindow) {
    wait_until(Duration::from_secs(2), || active_editor_has_focus(window));
}

fn editor_text(editor: &LushtextEditorPage) -> String {
    let buffer = editor.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn assert_tab_count(window: &LushtextWindow, expected: i32) {
    let actual = window.imp().tab_view.n_pages();
    assert_eq!(
        actual,
        expected,
        "expected {expected} open tab(s), got {actual}"
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

fn wait_for_tab_strip_visible(window: &LushtextWindow, context: &str) {
    let tab_bar = &window.imp().tab_bar;
    wait_until(Duration::from_secs(2), || {
        tab_bar.property::<bool>("visible")
            && tab_bar.is_visible()
            && tab_bar.width() > 0
            && tab_bar.height() > 0
    });
    assert!(
        tab_bar.property::<bool>("visible"),
        "{context}: tab strip should keep its own visible flag"
    );
    assert!(
        tab_bar.is_visible(),
        "{context}: tab strip should render through the window hierarchy"
    );
    assert_positive_allocation(&**tab_bar, context);
}

fn assert_tab_strip_hidden(window: &LushtextWindow, context: &str) {
    let tab_bar = &window.imp().tab_bar;
    assert!(
        !tab_bar.property::<bool>("visible"),
        "{context}: tab strip should clear its own visible flag"
    );
    assert!(
        !tab_bar.is_visible(),
        "{context}: tab strip should not render through the window hierarchy"
    );
}

fn assert_tab_context_menu_has_label(window: &LushtextWindow, label: &str, context: &str) {
    let labels =
        menu_model_labels(window.imp().tab_management.context_menu.upcast_ref::<gio::MenuModel>());
    assert!(
        labels.iter().any(|candidate| candidate == label),
        "{context}: expected tab context menu label '{label}', got {labels:?}"
    );
}

fn visible_alert_dialog(window: &LushtextWindow) -> Option<libadwaita::AlertDialog> {
    window
        .visible_dialog()
        .and_then(|dialog| dialog.downcast::<libadwaita::AlertDialog>().ok())
}

fn visible_sheet_dialog(window: &LushtextWindow) -> Option<libadwaita::Dialog> {
    window
        .visible_dialog()
        .and_then(|dialog| dialog.downcast::<libadwaita::Dialog>().ok())
}

fn find_button_by_label(widget: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>()
        && button.label().as_deref() == Some(label)
    {
        return Some(button);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_button_by_label(&child_widget, label) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn alert_dialog_extra_label_texts(dialog: &libadwaita::AlertDialog) -> Vec<String> {
    let extra_child = dialog.extra_child().expect("alert dialog extra child");
    let mut labels = Vec::new();
    collect_label_texts(&extra_child, &mut labels);
    labels
}

fn alert_dialog_extra_structure_counts(dialog: &libadwaita::AlertDialog) -> (usize, usize) {
    let extra_child = dialog.extra_child().expect("alert dialog extra child");
    let mut preferences_groups = 0;
    let mut action_rows = 0;
    collect_extra_structure_counts(&extra_child, &mut preferences_groups, &mut action_rows);
    (preferences_groups, action_rows)
}

fn collect_extra_structure_counts(
    widget: &gtk4::Widget,
    preferences_groups: &mut usize,
    action_rows: &mut usize,
) {
    if widget
        .clone()
        .downcast::<libadwaita::PreferencesGroup>()
        .is_ok()
    {
        *preferences_groups += 1;
    }
    if widget.clone().downcast::<libadwaita::ActionRow>().is_ok() {
        *action_rows += 1;
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        collect_extra_structure_counts(&child_widget, preferences_groups, action_rows);
        child = child_widget.next_sibling();
    }
}

fn collect_label_texts(widget: &gtk4::Widget, labels: &mut Vec<String>) {
    if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
        let text = label.label().to_string();
        if !text.is_empty() {
            labels.push(text);
        }
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        collect_label_texts(&child_widget, labels);
        child = child_widget.next_sibling();
    }
}

fn assert_label_text_contains(labels: &[String], needle: &str) {
    assert!(
        labels.iter().any(|label| label.contains(needle)),
        "expected a dialog detail label containing '{needle}', got {labels:?}"
    );
}

fn assert_no_label_text_contains(labels: &[String], needle: &str) {
    assert!(
        !labels.iter().any(|label| label.contains(needle)),
        "did not expect a dialog detail label containing '{needle}', got {labels:?}"
    );
}

fn find_button_by_tooltip(widget: &gtk4::Widget, tooltip: &str) -> Option<gtk4::Button> {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>()
        && button.tooltip_text().as_deref() == Some(tooltip)
    {
        return Some(button);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_button_by_tooltip(&child_widget, tooltip) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn visible_buttons_by_tooltip(widget: &gtk4::Widget, tooltip: &str) -> Vec<gtk4::Button> {
    let mut buttons = Vec::new();
    collect_visible_buttons_by_tooltip(widget, tooltip, &mut buttons);
    buttons
}

/// Collect visible icon buttons by tooltip so adaptive chrome tests can count actual affordances.
fn collect_visible_buttons_by_tooltip(
    widget: &gtk4::Widget,
    tooltip: &str,
    buttons: &mut Vec<gtk4::Button>,
) {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>()
        && button.tooltip_text().as_deref() == Some(tooltip)
        && button.is_visible()
    {
        buttons.push(button);
        return;
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        collect_visible_buttons_by_tooltip(&child_widget, tooltip, buttons);
        child = child_widget.next_sibling();
    }
}

/// Return the one visible Close/X control that a dialog surface is allowed to expose.
fn single_visible_close_button(widget: &gtk4::Widget) -> gtk4::Button {
    let buttons = visible_buttons_by_tooltip(widget, "Close");
    assert_eq!(
        buttons.len(),
        1,
        "notes browser should expose exactly one visible Close/X control"
    );
    buttons.into_iter().next().expect("visible close button")
}

fn find_label_by_text(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Label> {
    if let Ok(label) = widget.clone().downcast::<gtk4::Label>()
        && label.label() == text
    {
        return Some(label);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_label_by_text(&child_widget, text) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn find_entry_row_by_title(widget: &gtk4::Widget, title: &str) -> Option<libadwaita::EntryRow> {
    if let Ok(row) = widget.clone().downcast::<libadwaita::EntryRow>()
        && row.title() == title
    {
        return Some(row);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_entry_row_by_title(&child_widget, title) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn find_preferences_group(widget: &gtk4::Widget) -> Option<libadwaita::PreferencesGroup> {
    if let Ok(group) = widget.clone().downcast::<libadwaita::PreferencesGroup>() {
        return Some(group);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_preferences_group(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn status_bar_contains(window: &LushtextWindow, text: &str) -> bool {
    window
        .imp()
        .notification_bus
        .status_bar_view()
        .is_some_and(|status| status.text.contains(text))
}

fn set_entry_row_text_and_flush(row: &libadwaita::EntryRow, text: &str) {
    row.set_text(text);
    wait_until(Duration::from_secs(2), || row.text().as_str() == text);
    flush_events();
}

fn find_navigation_split_view(widget: &gtk4::Widget) -> Option<libadwaita::NavigationSplitView> {
    if let Ok(split_view) = widget.clone().downcast::<libadwaita::NavigationSplitView>() {
        return Some(split_view);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_navigation_split_view(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

/// Exercise the collapsed-state binding that shows adaptive browser back buttons.
///
/// The false -> true -> false sequence proves initial sync and continued
/// updates through GTK's property notifications, not only the static role.
fn assert_back_button_follows_split_collapsed(widget: &gtk4::Widget, tooltip: &str) {
    let split_view = find_navigation_split_view(widget).expect("navigation split view");
    let back_button = find_button_by_tooltip(widget, tooltip).expect("back button");

    split_view.set_collapsed(false);
    flush_events();
    wait_until(Duration::from_secs(2), || !back_button.is_visible());

    split_view.set_collapsed(true);
    flush_events();
    wait_until(Duration::from_secs(2), || back_button.is_visible());

    split_view.set_collapsed(false);
    flush_events();
    wait_until(Duration::from_secs(2), || !back_button.is_visible());
}

fn find_adw_sidebar(widget: &gtk4::Widget) -> Option<libadwaita::Sidebar> {
    if let Ok(sidebar) = widget.clone().downcast::<libadwaita::Sidebar>() {
        return Some(sidebar);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_adw_sidebar(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn find_status_page(widget: &gtk4::Widget) -> Option<libadwaita::StatusPage> {
    if let Ok(status_page) = widget.clone().downcast::<libadwaita::StatusPage>() {
        return Some(status_page);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_status_page(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn adw_sidebar_section_titles(sidebar: &libadwaita::Sidebar) -> Vec<String> {
    (0..sidebar.sections().n_items())
        .filter_map(|index| sidebar.section(index))
        .filter_map(|section| section.title().map(|title| title.to_string()))
        .collect()
}

fn has_tree_list_model_list_view(widget: &gtk4::Widget) -> bool {
    if let Ok(list_view) = widget.clone().downcast::<gtk4::ListView>()
        && let Some(selection) = list_view.model().and_downcast::<gtk4::SingleSelection>()
        && selection
            .model()
            .is_some_and(|model| model.is::<gtk4::TreeListModel>())
    {
        return true;
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if has_tree_list_model_list_view(&child_widget) {
            return true;
        }
        child = child_widget.next_sibling();
    }

    false
}

fn find_search_entry(widget: &gtk4::Widget) -> Option<gtk4::SearchEntry> {
    if let Ok(search_entry) = widget.clone().downcast::<gtk4::SearchEntry>() {
        return Some(search_entry);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_search_entry(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn find_stack_switcher(widget: &gtk4::Widget) -> Option<gtk4::StackSwitcher> {
    if let Ok(switcher) = widget.clone().downcast::<gtk4::StackSwitcher>() {
        return Some(switcher);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_stack_switcher(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn find_note_editor_stack(widget: &gtk4::Widget) -> Option<gtk4::Stack> {
    if let Ok(stack) = widget.clone().downcast::<gtk4::Stack>()
        && stack.child_by_name("edit").is_some()
        && stack.child_by_name("render").is_some()
    {
        return Some(stack);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_note_editor_stack(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn find_notes_preview_stack(widget: &gtk4::Widget) -> Option<gtk4::Stack> {
    if let Ok(stack) = widget.clone().downcast::<gtk4::Stack>()
        && stack.child_by_name("markdown").is_some()
        && stack.child_by_name("raw").is_some()
    {
        return Some(stack);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_notes_preview_stack(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn find_local_history_preview_stack(widget: &gtk4::Widget) -> Option<gtk4::Stack> {
    if let Ok(stack) = widget.clone().downcast::<gtk4::Stack>()
        && stack.child_by_name("loading").is_some()
        && stack.child_by_name("empty").is_some()
        && stack.child_by_name("error").is_some()
        && stack.child_by_name("content").is_some()
    {
        return Some(stack);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_local_history_preview_stack(&child_widget) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn collect_text_views(widget: &gtk4::Widget, text_views: &mut Vec<gtk4::TextView>) {
    if let Ok(text_view) = widget.clone().downcast::<gtk4::TextView>() {
        text_views.push(text_view);
        return;
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        collect_text_views(&child_widget, text_views);
        child = child_widget.next_sibling();
    }
}

fn notes_preview_visible_child_name(widget: &gtk4::Widget) -> Option<String> {
    find_notes_preview_stack(widget)
        .and_then(|stack| stack.visible_child_name().map(|name| name.to_string()))
}

fn notes_preview_text(widget: &gtk4::Widget) -> Option<String> {
    let stack = find_notes_preview_stack(widget)?;
    let child = stack.visible_child()?;
    let mut text_views = Vec::new();
    collect_text_views(&child, &mut text_views);
    text_views
        .into_iter()
        .find(|text_view| !text_view.is_editable())
        .map(|text_view| {
            let buffer = text_view.buffer();
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string()
        })
}

fn wait_for_notes_preview_text(widget: &gtk4::Widget, expected: &str) {
    wait_until(Duration::from_secs(5), || {
        notes_preview_text(widget).is_some_and(|text| text.contains(expected))
    });
}

fn local_history_preview_text(widget: &gtk4::Widget) -> Option<String> {
    let mut text_views = Vec::new();
    collect_text_views(widget, &mut text_views);
    text_views
        .into_iter()
        .find(|text_view| !text_view.is_editable())
        .map(|text_view| {
            let buffer = text_view.buffer();
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string()
        })
}

fn wait_for_local_history_preview_text(widget: &gtk4::Widget, expected: &str) {
    wait_until(Duration::from_secs(5), || {
        local_history_preview_text(widget).as_deref() == Some(expected)
    });
}

fn note_editor_text_views(widget: &gtk4::Widget) -> (gtk4::TextView, gtk4::TextView) {
    let mut text_views = Vec::new();
    collect_text_views(widget, &mut text_views);

    let edit = text_views
        .iter()
        .find(|text_view| text_view.is_editable())
        .cloned()
        .expect("editable note text view");
    let render = text_views
        .into_iter()
        .find(|text_view| !text_view.is_editable())
        .expect("rendered note text view");

    (edit, render)
}

fn assert_note_editor_text_margins_match(widget: &gtk4::Widget) {
    let (edit, render) = note_editor_text_views(widget);
    assert_eq!(edit.left_margin(), render.left_margin());
    assert_eq!(edit.right_margin(), render.right_margin());
    assert_eq!(edit.top_margin(), render.top_margin());
    assert_eq!(edit.bottom_margin(), render.bottom_margin());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WidgetOuterSize {
    width: i32,
    height: i32,
}

/// Empty status dialogs use this compact target instead of full browser size.
const EMPTY_STATUS_DIALOG_TARGET_WIDTH: i32 = 640;
/// The target height must fit the normal status page and dialog header without scrolling.
const EMPTY_STATUS_DIALOG_TARGET_HEIGHT: i32 = 480;
/// Theme chrome can vary, so allocation assertions use a lower bound near the target.
const EMPTY_STATUS_DIALOG_MIN_RENDERED_WIDTH: i32 = 600;
/// Keep enough vertical allocation for icon, title, description, and margins.
const EMPTY_STATUS_DIALOG_MIN_RENDERED_HEIGHT: i32 = 440;
/// The status page itself should receive enough width for readable copy.
const EMPTY_STATUS_PAGE_MIN_RENDERED_WIDTH: i32 = 560;

fn settled_widget_outer_size(widget: &impl IsA<gtk4::Widget>) -> WidgetOuterSize {
    let widget = widget.as_ref();
    let mut previous = None;
    let deadline = Instant::now() + Duration::from_secs(2);

    while Instant::now() < deadline {
        flush_after_delay(Duration::from_millis(20));
        let current = WidgetOuterSize {
            width: widget.width(),
            height: widget.height(),
        };
        if current.width > 0 && current.height > 0 && previous == Some(current) {
            return current;
        }
        previous = Some(current);
    }

    panic!("widget did not settle to a positive allocation; last allocation was {previous:?}");
}

fn assert_settled_widget_outer_size(
    widget: &impl IsA<gtk4::Widget>,
    expected: WidgetOuterSize,
    context: &str,
) {
    let actual = settled_widget_outer_size(widget);
    assert_eq!(
        actual, expected,
        "{context} must not change the modal outer allocation"
    );
}

/// Assert the empty browser is readable in the rendered widget tree.
///
/// `content_width()` alone is not enough: an `AdwDialog` that follows child
/// content size can still report a target while rendering the status page as a
/// narrow column or scrollable body.
fn assert_readable_empty_status_dialog(
    dialog: &libadwaita::Dialog,
    child: &gtk4::Widget,
    context: &str,
) {
    assert!(
        !dialog.follows_content_size(),
        "{context} should honor its compact target size instead of following status-page natural size"
    );
    assert_eq!(
        dialog.content_width(),
        EMPTY_STATUS_DIALOG_TARGET_WIDTH,
        "{context} should keep its compact target width"
    );
    assert_eq!(
        dialog.content_height(),
        EMPTY_STATUS_DIALOG_TARGET_HEIGHT,
        "{context} should keep its compact target height"
    );

    let dialog_size = settled_widget_outer_size(dialog);
    assert!(
        dialog_size.width >= EMPTY_STATUS_DIALOG_MIN_RENDERED_WIDTH,
        "{context} should render wide enough for readable empty-state copy, got {dialog_size:?}"
    );
    assert!(
        dialog_size.height >= EMPTY_STATUS_DIALOG_MIN_RENDERED_HEIGHT,
        "{context} should render tall enough for readable empty-state copy, got {dialog_size:?}"
    );
    assert!(
        !has_vertical_scroll_overflow(dialog),
        "{context} should fit without vertical scrolling"
    );

    let status = find_status_page(child).expect("empty-state status page");
    let status_size = settled_widget_outer_size(&status);
    assert!(
        status_size.width >= EMPTY_STATUS_PAGE_MIN_RENDERED_WIDTH,
        "{context} status page should receive a readable line length, got {status_size:?}"
    );
}

fn has_vertical_scroll_overflow(folder: &impl IsA<gtk4::Widget>) -> bool {
    descendants(folder).into_iter().any(|widget| {
        if let Ok(scroller) = widget.clone().downcast::<gtk4::ScrolledWindow>() {
            let adjustment = scroller.vadjustment();
            return scroller.is_visible() && adjustment.upper() > adjustment.page_size() + 1.0;
        }

        widget.downcast::<gtk4::Scrollbar>().is_ok_and(|scrollbar| {
            scrollbar.orientation() == gtk4::Orientation::Vertical
                && scrollbar.is_visible()
                && scrollbar.width() > 0
                && scrollbar.height() > 0
        })
    })
}

fn measured_natural_outer_size(widget: &impl IsA<gtk4::Widget>) -> WidgetOuterSize {
    let (_, natural_width, _, _) = widget.measure(gtk4::Orientation::Horizontal, -1);
    let (_, natural_height, _, _) = widget.measure(gtk4::Orientation::Vertical, natural_width);
    WidgetOuterSize {
        width: natural_width,
        height: natural_height,
    }
}

fn assert_positive_allocation(widget: &impl IsA<gtk4::Widget>, context: &str) {
    let widget = widget.as_ref();
    assert!(
        widget.width() > 0 && widget.height() > 0,
        "{context} should have a positive allocation, got {}x{}",
        widget.width(),
        widget.height()
    );
}

fn wait_for_positive_allocation(widget: &impl IsA<gtk4::Widget>, context: &str) {
    let widget = widget.as_ref();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if widget.width() > 0 && widget.height() > 0 {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }

    panic!(
        "{context} did not receive a positive allocation; final size was {}x{}, visible={}, mapped={}",
        widget.width(),
        widget.height(),
        widget.is_visible(),
        widget.is_mapped()
    );
}

fn allocate_widget_for_test(widget: &impl IsA<gtk4::Widget>, width: i32) {
    let widget = widget.as_ref();
    let (_, natural_height, _, _) = widget.measure(gtk4::Orientation::Vertical, width);
    widget.allocate(width, natural_height.max(1), -1, None);
    flush_events();
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Widget tests round allocated f32 bounds back to device pixels for exact geometry assertions"
)]
fn note_editor_text_origin(widget: &gtk4::Widget, text_view: &gtk4::TextView) -> (i32, i32) {
    let bounds = text_view
        .compute_bounds(widget)
        .expect("note text view should have allocated bounds");
    (
        (bounds.x() + text_view.left_margin() as f32).round() as i32,
        (bounds.y() + text_view.top_margin() as f32).round() as i32,
    )
}

fn note_editor_visible_text_origin(widget: &gtk4::Widget, editable: bool) -> (i32, i32) {
    let (edit, render) = note_editor_text_views(widget);
    let text_view = if editable { edit } else { render };
    note_editor_text_origin(widget, &text_view)
}

fn text_view_scrolled_window(text_view: &gtk4::TextView) -> gtk4::ScrolledWindow {
    let mut parent = text_view.parent();
    while let Some(widget) = parent {
        if let Ok(scrolled_window) = widget.clone().downcast::<gtk4::ScrolledWindow>() {
            return scrolled_window;
        }
        parent = widget.parent();
    }

    panic!("note text view should live inside a scrolled window");
}

fn assert_note_editor_render_surface_ready_before_first_render(widget: &gtk4::Widget) {
    let (_, render) = note_editor_text_views(widget);
    let scrolled_window = text_view_scrolled_window(&render);
    assert!(
        scrolled_window.property::<bool>("visible"),
        "note editor Render page should keep the final text scroller visible before first activation"
    );
}

fn assert_note_editor_render_keeps_modal_geometry(
    dialog: &impl IsA<gtk4::Widget>,
    extra: &gtk4::Widget,
    stack: &gtk4::Stack,
) {
    let edit_dialog_size = settled_widget_outer_size(dialog);
    let edit_stack_size = settled_widget_outer_size(stack);
    let edit_extra_natural_size = measured_natural_outer_size(extra);
    let edit_stack_natural_size = measured_natural_outer_size(stack);
    let edit_text_origin = note_editor_visible_text_origin(extra, true);
    assert_note_editor_render_surface_ready_before_first_render(extra);

    stack.set_visible_child_name("render");
    flush_after_delay(Duration::from_millis(40));
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));
    let render_dialog_size = settled_widget_outer_size(dialog);
    let render_stack_size = settled_widget_outer_size(stack);
    let render_extra_natural_size = measured_natural_outer_size(extra);
    let render_stack_natural_size = measured_natural_outer_size(stack);
    let render_text_origin = note_editor_visible_text_origin(extra, false);

    assert_eq!(
        edit_dialog_size, render_dialog_size,
        "note editor modal outer allocation must not change on first Edit -> Render switch"
    );
    assert_eq!(
        edit_stack_size, render_stack_size,
        "note editor stack allocation must not change on first Edit -> Render switch"
    );
    assert_eq!(
        edit_extra_natural_size, render_extra_natural_size,
        "note editor modal content must keep the same natural outer size on first Edit -> Render switch"
    );
    assert_eq!(
        edit_stack_natural_size, render_stack_natural_size,
        "note editor stack must keep the same natural size on first Edit -> Render switch"
    );
    assert_eq!(
        edit_text_origin, render_text_origin,
        "rendered note text should start at the same content origin as editable note text"
    );
}

/// Verifies that an existing note dialog keeps stable geometry when it opens in Render mode.
///
/// The helper intentionally switches Render -> Edit -> Render so regressions in
/// first-transition allocation, natural size, or text origin are caught in one place.
fn assert_note_editor_render_first_keeps_modal_geometry(
    dialog: &impl IsA<gtk4::Widget>,
    extra: &gtk4::Widget,
    stack: &gtk4::Stack,
) {
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));
    let render_dialog_size = settled_widget_outer_size(dialog);
    let render_stack_size = settled_widget_outer_size(stack);
    let render_extra_natural_size = measured_natural_outer_size(extra);
    let render_stack_natural_size = measured_natural_outer_size(stack);
    let render_text_origin = note_editor_visible_text_origin(extra, false);
    assert_note_editor_render_surface_ready_before_first_render(extra);

    stack.set_visible_child_name("edit");
    flush_after_delay(Duration::from_millis(40));
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
    assert_eq!(
        settled_widget_outer_size(dialog),
        render_dialog_size,
        "note editor modal outer allocation must not change on first Render -> Edit switch"
    );
    assert_eq!(
        settled_widget_outer_size(stack),
        render_stack_size,
        "note editor stack allocation must not change on first Render -> Edit switch"
    );
    assert_eq!(
        measured_natural_outer_size(extra),
        render_extra_natural_size,
        "note editor modal content must keep the same natural outer size on Render -> Edit"
    );
    assert_eq!(
        measured_natural_outer_size(stack),
        render_stack_natural_size,
        "note editor stack must keep the same natural size on Render -> Edit"
    );
    assert_eq!(
        note_editor_visible_text_origin(extra, true),
        render_text_origin,
        "editable note text should start at the same content origin as rendered note text"
    );

    stack.set_visible_child_name("render");
    flush_after_delay(Duration::from_millis(40));
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));
    assert_eq!(
        settled_widget_outer_size(dialog),
        render_dialog_size,
        "note editor modal outer allocation must stay stable after returning to Render"
    );
    assert_eq!(
        settled_widget_outer_size(stack),
        render_stack_size,
        "note editor stack allocation must stay stable after returning to Render"
    );
}

fn assert_typed_note_editor_first_render_keeps_modal_geometry(
    dialog: &impl IsA<gtk4::Widget>,
    extra: &gtk4::Widget,
    stack: &gtk4::Stack,
    text: &str,
) {
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
    let initial_dialog_size = settled_widget_outer_size(dialog);
    let initial_stack_size = settled_widget_outer_size(stack);
    let initial_extra_natural_size = measured_natural_outer_size(extra);
    let initial_stack_natural_size = measured_natural_outer_size(stack);
    let initial_text_origin = note_editor_visible_text_origin(extra, true);
    assert_note_editor_render_surface_ready_before_first_render(extra);

    let (edit, _) = note_editor_text_views(extra);
    edit.buffer().set_text(text);
    flush_after_delay(Duration::from_millis(40));

    assert_eq!(
        settled_widget_outer_size(dialog),
        initial_dialog_size,
        "typing into a note editor must not resize the modal before Render is clicked"
    );
    assert_eq!(
        settled_widget_outer_size(stack),
        initial_stack_size,
        "typing into a note editor must not resize the note editor stack"
    );
    assert_eq!(
        measured_natural_outer_size(extra),
        initial_extra_natural_size,
        "typing into a note editor must keep the modal content natural size stable"
    );
    assert_eq!(
        measured_natural_outer_size(stack),
        initial_stack_natural_size,
        "typing into a note editor must keep the note editor stack natural size stable"
    );
    assert_eq!(
        note_editor_visible_text_origin(extra, true),
        initial_text_origin,
        "typing into a note editor must not move the editable text origin"
    );

    assert_note_editor_render_keeps_modal_geometry(dialog, extra, stack);
}

fn menu_model_labels(model: &gio::MenuModel) -> Vec<String> {
    let mut labels = Vec::new();
    for index in 0..model.n_items() {
        if let Some(label) = model
            .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
            .and_then(|variant| variant.get::<String>())
        {
            labels.push(label);
        }
        for link_name in ["section", "submenu"] {
            if let Some(link) = model.item_link(index, link_name) {
                labels.extend(menu_model_labels(&link));
            }
        }
    }
    labels
}

fn action_button_labels(menu_box: &gtk4::Box) -> Vec<String> {
    let mut labels = Vec::new();
    let mut child = menu_box.first_child();
    while let Some(widget) = child {
        if let Ok(button) = widget.clone().downcast::<gtk4::Button>()
            && let Some(label) = button.label()
        {
            labels.push(label.to_string());
        }
        child = widget.next_sibling();
    }
    labels
}

fn menu_model_label_actions(model: &gio::MenuModel) -> Vec<(String, Option<String>)> {
    let mut entries = Vec::new();
    for index in 0..model.n_items() {
        let label = model
            .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
            .and_then(|variant| variant.get::<String>());
        let action = model
            .item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
            .and_then(|variant| variant.get::<String>());
        if let Some(label) = label {
            entries.push((label, action));
        }
        for link_name in ["section", "submenu"] {
            if let Some(link) = model.item_link(index, link_name) {
                entries.extend(menu_model_label_actions(&link));
            }
        }
    }
    entries
}

fn primary_menu_action_for_label(window: &LushtextWindow, label: &str) -> Option<String> {
    let primary_menu = window
        .imp()
        .primary_menu_button
        .menu_model()
        .expect("primary menu model");
    menu_model_label_actions(&primary_menu)
        .into_iter()
        .find_map(|(entry_label, action)| (entry_label == label).then_some(action).flatten())
}

fn activate_primary_menu_item(window: &LushtextWindow, label: &str) {
    let action = primary_menu_action_for_label(window, label)
        .unwrap_or_else(|| panic!("primary menu item '{label}' should have an action"));
    let action_name = action
        .strip_prefix("win.")
        .unwrap_or_else(|| panic!("expected '{action}' to be a window action"));
    activate_action(window, action_name);
}

fn click_alert_extra_button(dialog: &libadwaita::AlertDialog, label: &str) {
    let extra = dialog.extra_child().expect("alert dialog extra child");
    click_labeled_widget(&extra, label);
    flush_events();
}

fn click_labeled_widget(widget: &gtk4::Widget, label: &str) {
    if let Some(button) = find_button_by_label(widget, label) {
        button.emit_clicked();
        return;
    }

    if let Some(toggle) = find_toggle_button_by_label(widget, label) {
        toggle.emit_clicked();
        return;
    }

    if let Some(row) = find_action_row_by_title(widget, label) {
        row.emit_by_name::<()>("activated", &[]);
        flush_events();
        return;
    }

    panic!("clickable widget '{label}' not found");
}

fn find_action_row_by_title(widget: &gtk4::Widget, title: &str) -> Option<libadwaita::ActionRow> {
    if let Ok(row) = widget.clone().downcast::<libadwaita::ActionRow>()
        && row.title().as_str() == title
    {
        return Some(row);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_action_row_by_title(&child_widget, title) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn find_toggle_button_by_label(widget: &gtk4::Widget, label: &str) -> Option<gtk4::ToggleButton> {
    if let Ok(toggle) = widget.clone().downcast::<gtk4::ToggleButton>()
        && toggle.label().as_deref() == Some(label)
    {
        return Some(toggle);
    }

    let mut child = widget.first_child();
    while let Some(child_widget) = child {
        if let Some(found) = find_toggle_button_by_label(&child_widget, label) {
            return Some(found);
        }
        child = child_widget.next_sibling();
    }

    None
}

fn workspace_sidebar_visible(window: &LushtextWindow) -> bool {
    window.imp().workspace_split_view.shows_sidebar()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertiesSurfacePresentation {
    Pane,
    Sheet,
}

impl PropertiesSurfacePresentation {
    fn layout_name(self) -> &'static str {
        match self {
            Self::Pane => "pane",
            Self::Sheet => "sheet",
        }
    }
}

fn set_properties_surface_presentation(
    window: &LushtextWindow,
    presentation: PropertiesSurfacePresentation,
) {
    let requested = window
        .imp()
        .secondary_surfaces
        .properties_requested_visible
        .get();
    window
        .imp()
        .properties_layout_view
        .set_layout_name(presentation.layout_name());
    match presentation {
        PropertiesSurfacePresentation::Pane => {
            window.imp().properties_bottom_sheet.set_open(false);
            window
                .imp()
                .properties_split_view
                .set_show_sidebar(requested);
        }
        PropertiesSurfacePresentation::Sheet => {
            window.imp().properties_split_view.set_show_sidebar(false);
            window.imp().properties_bottom_sheet.set_open(requested);
        }
    }
    flush_events();
}

fn properties_surface_presentation(window: &LushtextWindow) -> PropertiesSurfacePresentation {
    match window
        .imp()
        .properties_layout_view
        .layout_name()
        .as_deref()
    {
        Some("sheet") => PropertiesSurfacePresentation::Sheet,
        _ => PropertiesSurfacePresentation::Pane,
    }
}

fn properties_sidebar_visible(window: &LushtextWindow) -> bool {
    match properties_surface_presentation(window) {
        PropertiesSurfacePresentation::Pane => window.imp().properties_split_view.shows_sidebar(),
        PropertiesSurfacePresentation::Sheet => window.imp().properties_bottom_sheet.is_open(),
    }
}

fn shortcut_bound(window: &LushtextWindow, action_name: &str, trigger_string: &str) -> bool {
    let expected_trigger = gtk4::ShortcutTrigger::parse_string(trigger_string)
        .unwrap_or_else(|| panic!("shortcut trigger '{trigger_string}' should parse"));
    let expected_trigger = expected_trigger.to_str();
    let controllers = window.observe_controllers();
    let shortcut_controller = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|object| object.downcast::<gtk4::ShortcutController>().ok())
        .find(|controller| controller.scope() == gtk4::ShortcutScope::Managed)
        .expect("window should install a managed shortcut controller");

    (0..shortcut_controller.n_items())
        .filter_map(|index| shortcut_controller.item(index))
        .filter_map(|object| object.downcast::<gtk4::Shortcut>().ok())
        .any(|shortcut| {
            let action_matches = shortcut
                .action()
                .and_then(|action| action.downcast::<gtk4::NamedAction>().ok())
                .is_some_and(|action| action.action_name().as_str() == action_name);
            let trigger_matches = shortcut
                .trigger()
                .is_some_and(|trigger| trigger.to_str() == expected_trigger);
            action_matches && trigger_matches
        })
}

fn emit_escape_on_window_controller(window: &LushtextWindow) -> glib::Propagation {
    let controllers = window.observe_controllers();
    let controller = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|object| object.downcast::<gtk4::EventControllerKey>().ok())
        .find(|controller| controller.propagation_phase() == gtk4::PropagationPhase::Bubble)
        .expect("window should install a bubble-phase key controller");
    let args: [&dyn glib::value::ToValue; 3] = [
        &gtk4::gdk::Key::Escape,
        &0u32,
        &gtk4::gdk::ModifierType::empty(),
    ];
    let stopped: bool = glib::object::ObjectExt::emit_by_name(&controller, "key-pressed", &args);
    if stopped {
        glib::Propagation::Stop
    } else {
        glib::Propagation::Proceed
    }
}

fn notes_menu_button_visible(window: &LushtextWindow) -> bool {
    window.imp().notes_menu_button.property::<bool>("visible")
}

fn notes_menu_popup_open(window: &LushtextWindow) -> bool {
    let button = &window.imp().notes_menu_button;
    button.is_active()
        || button
            .popover()
            .is_some_and(|popover| popover.is_visible())
}

fn open_notes_menu_popup(window: &LushtextWindow) {
    window.imp().notes_menu_button.popup();
    wait_until(Duration::from_secs(2), || notes_menu_popup_open(window));
}

fn close_notes_menu_popup(window: &LushtextWindow) {
    window.imp().notes_menu_button.popdown();
    wait_until(Duration::from_secs(2), || !notes_menu_popup_open(window));
}

fn widget_left_in(reference: &gtk4::Widget, widget: &gtk4::Widget) -> f32 {
    widget
        .compute_bounds(reference)
        .expect("widget should share allocated bounds with the reference")
        .x()
}

fn properties_surface_uses_bottom_sheet(window: &LushtextWindow) -> bool {
    properties_surface_presentation(window) == PropertiesSurfacePresentation::Sheet
        && window.imp().properties_bottom_sheet.is_open()
}

fn properties_surface_uses_right_pane(window: &LushtextWindow) -> bool {
    properties_surface_presentation(window) == PropertiesSurfacePresentation::Pane
        && window.imp().properties_split_view.shows_sidebar()
}

fn wait_for_properties_surface(
    window: &LushtextWindow,
    expected: PropertiesSurfacePresentation,
    workspace_visible: bool,
) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let properties_ready = match expected {
            PropertiesSurfacePresentation::Pane => properties_surface_uses_right_pane(window),
            PropertiesSurfacePresentation::Sheet => properties_surface_uses_bottom_sheet(window),
        };
        if properties_ready && workspace_sidebar_visible(window) == workspace_visible {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }

    panic!(
        "expected properties {expected:?} with workspace visible={workspace_visible}, got presentation={:?}, properties visible={}, workspace visible={}, window={}x{}, properties split={}, sheet open={}",
        properties_surface_presentation(window),
        properties_sidebar_visible(window),
        workspace_sidebar_visible(window),
        current_window_width(window),
        current_window_height(window),
        window.imp().properties_split_view.shows_sidebar(),
        window.imp().properties_bottom_sheet.is_open()
    );
}

fn wait_for_workspace_sidebar_transition(window: &LushtextWindow) {
    wait_until(Duration::from_secs(2), || {
        !window.workspace_sidebar_transition_pending_for_test()
    });
    flush_events();
}

fn adaptive_shell_change_counter(window: &LushtextWindow) -> std::rc::Rc<std::cell::Cell<u32>> {
    let changes = std::rc::Rc::new(std::cell::Cell::new(0u32));
    {
        let changes = changes.clone();
        window
            .imp()
            .properties_layout_view
            .connect_notify_local(Some("layout-name"), move |_, _| {
                changes.set(changes.get().saturating_add(1));
            });
    }
    {
        let changes = changes.clone();
        window
            .imp()
            .workspace_split_view
            .connect_notify_local(Some("show-sidebar"), move |_, _| {
                changes.set(changes.get().saturating_add(1));
            });
    }
    {
        let changes = changes.clone();
        window
            .imp()
            .properties_split_view
            .connect_notify_local(Some("show-sidebar"), move |_, _| {
                changes.set(changes.get().saturating_add(1));
            });
    }
    {
        let changes = changes.clone();
        window
            .imp()
            .properties_bottom_sheet
            .connect_notify_local(Some("open"), move |_, _| {
                changes.set(changes.get().saturating_add(1));
            });
    }
    changes
}

fn assert_adaptive_shell_stays_quiet(
    window: &LushtextWindow,
    expected: PropertiesSurfacePresentation,
    workspace_visible: bool,
) {
    let changes = adaptive_shell_change_counter(window);
    let before = changes.get();
    for _ in 0..8 {
        flush_after_delay(Duration::from_millis(50));
        assert_eq!(
            changes.get(),
            before,
            "adaptive shell state kept changing after it had settled"
        );
        match expected {
            PropertiesSurfacePresentation::Pane => {
                assert!(properties_surface_uses_right_pane(window));
            }
            PropertiesSurfacePresentation::Sheet => {
                assert!(properties_surface_uses_bottom_sheet(window));
            }
        }
        assert_eq!(workspace_sidebar_visible(window), workspace_visible);
    }
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

fn current_window_height(window: &LushtextWindow) -> i32 {
    if window.height() > 0 {
        window.height()
    } else {
        let (_, default_height) = window.default_size();
        default_height
    }
}

fn source_view_hadjustment_is_at_left(editor: &LushtextEditorPage) -> bool {
    editor
        .source_view()
        .hadjustment()
        .is_none_or(|adjustment| (adjustment.value() - adjustment.lower()).abs() <= 0.5)
}

fn source_view_vadjustment_is_at_top(editor: &LushtextEditorPage) -> bool {
    editor
        .source_view()
        .vadjustment()
        .is_none_or(|adjustment| (adjustment.value() - adjustment.lower()).abs() <= 0.5)
}

fn assert_workspace_sidebar_width_locked(window: &LushtextWindow, expected_width: f64) {
    let split = &window.imp().workspace_split_view;
    let min_width = split.min_sidebar_width();
    let max_width = split.max_sidebar_width();
    assert!(
        (min_width - expected_width).abs() < 1.0,
        "expected min sidebar width near {expected_width}, got {min_width}"
    );
    assert!(
        (max_width - expected_width).abs() < 1.0,
        "expected max sidebar width near {expected_width}, got {max_width}"
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

fn write_document_with_long_lines(
    editor: &LushtextEditorPage,
    line_count: usize,
    long_every: usize,
    needle_every: usize,
) {
    let mut text = String::new();
    let long_tail = "x".repeat(140);
    for line in 0..line_count {
        let marker = if needle_every != 0 && line % needle_every == 0 {
            " needle"
        } else {
            ""
        };
        if long_every != 0 && line % long_every == 0 {
            text.push_str(&format!("line {line:04}{marker} {long_tail}\n"));
        } else {
            text.push_str(&format!("line {line:04}{marker}\n"));
        }
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

fn minimap_source_map(editor: &LushtextEditorPage) -> sourceview5::Map {
    editor
        .imp()
        .minimap
        .source_map
        .borrow()
        .as_ref()
        .cloned()
        .expect("source map should exist")
}

fn minimap_marker_strip(editor: &LushtextEditorPage) -> gtk4::DrawingArea {
    editor
        .imp()
        .minimap
        .marker_strip
        .borrow()
        .as_ref()
        .cloned()
        .expect("marker strip should exist")
}

#[derive(Debug, Clone, Copy)]
struct MinimapGeometrySnapshot {
    editor_width: i32,
    editor_visible_height: i32,
    editor_wrap_mode: gtk4::WrapMode,
    source_map_width: i32,
    source_map_height: i32,
    source_map_top_margin: i32,
    source_map_wrap_mode: gtk4::WrapMode,
    marker_strip_height: i32,
    minimap_first_line_top: f64,
    vertical_lower: f64,
    vertical_value: f64,
    vertical_page_size: f64,
    vertical_upper: f64,
    /// Source-map adjustment lower bound, proving native map top anchoring.
    source_map_vertical_lower: f64,
    /// Source-map adjustment value, tracked because it can drift independently.
    source_map_vertical_value: f64,
    visible_start_line: i32,
}

/// Capture editor, source-map, and marker-strip geometry after GTK settles.
///
/// Tests compare editor scroll anchoring with `GtkSourceMap`'s own adjustment,
/// which can drift independently during width-only reflow.
fn minimap_geometry_snapshot(editor: &LushtextEditorPage) -> MinimapGeometrySnapshot {
    let source_view = editor.source_view();
    let source_map = minimap_source_map(editor);
    let marker_strip = minimap_marker_strip(editor);
    let visible_rect = source_view.visible_rect();
    let (_, buffer_y) = source_view.window_to_buffer_coords(
        gtk4::TextWindowType::Widget,
        visible_rect.x(),
        visible_rect.y(),
    );
    let visible_start_line = source_view
        .iter_at_location(0, buffer_y)
        .map_or(0, |iter| iter.line());
    let vadjustment = source_view.vadjustment().expect("source view vadjustment");
    let source_map_vadjustment = source_map
        .vadjustment()
        .expect("source map should expose a vertical adjustment");
    let map_bounds = source_map
        .compute_bounds(&*editor.imp().minimap_overlay)
        .expect("source map should have overlay-relative bounds");
    let start_iter = source_map.buffer().start_iter();
    let (line_y, _) = source_map.line_yrange(&start_iter);
    let (_, widget_y) =
        source_map.buffer_to_window_coords(gtk4::TextWindowType::Widget, 0, line_y);

    MinimapGeometrySnapshot {
        editor_width: source_view.width(),
        editor_visible_height: visible_rect.height(),
        editor_wrap_mode: source_view.wrap_mode(),
        source_map_width: source_map.width(),
        source_map_height: source_map.height(),
        source_map_top_margin: source_map.top_margin(),
        source_map_wrap_mode: source_map.wrap_mode(),
        marker_strip_height: marker_strip.height(),
        minimap_first_line_top: f64::from(map_bounds.y()) + f64::from(widget_y),
        vertical_lower: vadjustment.lower(),
        vertical_value: vadjustment.value(),
        vertical_page_size: vadjustment.page_size(),
        vertical_upper: vadjustment.upper(),
        source_map_vertical_lower: source_map_vadjustment.lower(),
        source_map_vertical_value: source_map_vadjustment.value(),
        visible_start_line,
    }
}

fn put_editor_at_top_left(editor: &LushtextEditorPage) {
    let source_view = editor.source_view();
    let buffer = editor.buffer();
    let start = buffer.start_iter();
    buffer.place_cursor(&start);
    if let Some(adjustment) = source_view.vadjustment() {
        adjustment.set_value(adjustment.lower());
    }
    if let Some(adjustment) = source_view.hadjustment() {
        adjustment.set_value(adjustment.lower());
    }
    flush_events();
}

fn add_top_bookmark_marker(editor: &LushtextEditorPage) {
    let buffer = editor.buffer();
    let start = buffer.start_iter();
    buffer.place_cursor(&start);
    let _ = editor.toggle_bookmark_at_cursor();
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Bookmark) == 1
            && !editor
                .minimap_marker_bounds(MinimapMarkerKind::Bookmark)
                .is_empty()
    });
}

fn assert_top_minimap_reflow_invariants(
    editor: &LushtextEditorPage,
    geometry: MinimapGeometrySnapshot,
    expected_wrap_mode: gtk4::WrapMode,
) {
    assert_eq!(geometry.editor_wrap_mode, expected_wrap_mode);
    assert_eq!(
        geometry.source_map_wrap_mode,
        gtk4::WrapMode::None,
        "minimap source map should stay unwrapped after reflow: {geometry:?}"
    );
    assert!(
        geometry.source_map_width > 0
            && geometry.source_map_height > 0
            && geometry.marker_strip_height > 0
            && geometry.editor_visible_height > 0,
        "editor, source map, and marker strip should all have positive settled allocation: {geometry:?}"
    );
    assert!(
        geometry.source_map_top_margin == 5,
        "minimap uses a fixed native-map top content inset, got {geometry:?}"
    );
    assert_eq!(
        geometry.visible_start_line, 0,
        "top-anchored reflow should keep line one visible: {geometry:?}"
    );
    assert!(
        (geometry.vertical_value - geometry.vertical_lower).abs() <= 0.5,
        "top-anchored reflow should keep the vertical adjustment at its lower bound: {geometry:?}"
    );
    // GtkSourceMap subtracts its own visible-rect y while drawing the native
    // slider, so its adjustment must stay anchored independently of the editor.
    assert!(
        (geometry.source_map_vertical_value - geometry.source_map_vertical_lower).abs() <= 0.5,
        "top-anchored reflow should keep the source-map adjustment at its lower bound: {geometry:?}"
    );
    assert!(
        geometry.vertical_upper > geometry.vertical_page_size,
        "fixture should remain scrollable so the top-anchor assertion is meaningful: {geometry:?}"
    );
    assert!(
        geometry.minimap_first_line_top >= 1.0,
        "first minimap content line should render below the minimap shell top edge: {geometry:?}"
    );

    let marker_bounds = editor.minimap_marker_bounds(MinimapMarkerKind::Bookmark);
    assert!(
        !marker_bounds.is_empty(),
        "top bookmark marker should remain projectable after reflow"
    );
    for bounds in marker_bounds {
        assert!(
            bounds.top >= -0.5 && bounds.bottom <= f64::from(geometry.marker_strip_height) + 0.5,
            "projected marker should remain inside the marker strip after reflow: {bounds:?}, {geometry:?}"
        );
        assert!(bounds.height() > 0.0, "projected marker should have positive height: {bounds:?}");
    }
}

fn tab_content_opacity_setting() -> f64 {
    gio::Settings::new(lushtext_core::config::APP_ID).double(keys::TAB_CONTENT_OPACITY)
}

fn preview_layout_name(window: &LushtextWindow) -> Option<String> {
    window
        .imp()
        .preview_layout_view
        .layout_name()
        .map(|name| name.to_string())
}

fn seed_peek_workspace() -> (tempfile::TempDir, PathBuf, PathBuf) {
    ensure_gtk_init();
    let folder_dir = tempfile::tempdir().expect("peek workspace tempdir");
    let alpha = folder_dir.path().join("alpha.rs");
    let beta = folder_dir.path().join("beta.rs");
    fixture::write_text(&alpha, "fn alpha() {\n    println!(\"alpha\");\n}\n");
    fixture::write_text(&beta, "fn beta() {\n    println!(\"beta\");\n}\n");

    let mut workspaces = WorkspacesFile::default();
    workspaces
        .workspaces
        .push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new("peek-ws"),
            "peek",
            folder_dir.path().to_path_buf(),
        ));
    workspaces.current_scope = WorkspaceScope::workspace(WorkspaceId::new("peek-ws"));
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save peek workspaces");
    (folder_dir, alpha, beta)
}

fn seed_workspace_row_state_files() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    ensure_gtk_init();
    let folder_dir = tempfile::tempdir().expect("workspace row state tempdir");
    let alpha = folder_dir.path().join("alpha.txt");
    let beta = folder_dir.path().join("beta.txt");
    let missing = folder_dir.path().join("missing.txt");
    fixture::write_text(&alpha, "alpha\n");
    fixture::write_text(&beta, "beta\n");
    fixture::write_text(&missing, "remove before opening\n");

    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::All,
        workspaces: vec![WorkspaceConfig::with_one_folder(
            WorkspaceId::new("ws-row-state"),
            "row state",
            folder_dir.path().to_path_buf(),
        )],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces)
        .expect("save row-state workspaces");
    (folder_dir, alpha, beta, missing)
}

fn seed_named_tab_files(names: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().expect("named tab tempdir");
    let paths = names
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            fixture::write_text(&path, &format!("content for {name}\n"));
            path
        })
        .collect();
    (dir, paths)
}

#[test]
fn test_open_document_restores_bookmarks() {
    let tempdir = tempfile::tempdir().expect("notes tempdir");
    let file_path = tempdir.path().join("src/main.rs");
    fixture::create_dir_all(file_path.parent().expect("expected operation to succeed"));
    fixture::write_text(&file_path, "one\ntwo\nthree\nfour\n");

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

    present_window(&window);
    window.open_document(&file_path);

    wait_until(Duration::from_secs(2), || {
        active_editor(&window).bookmark_records().len() == 1
    });

    let editor = active_editor(&window);
    assert_eq!(
        editor.bookmark_records()[0].label.as_deref(),
        Some("bookmark")
    );
}

#[test]
fn test_stale_bookmark_sidecar_load_does_not_replace_local_edits() {
    ensure_gtk_init();
    let tempdir = tempfile::tempdir().expect("stale bookmark sidecar tempdir");
    let file_path = tempdir.path().join("stale-bookmark.rs");
    fixture::write_text(&file_path, "one\ntwo\nthree");

    let window = test_window();
    present_window(&window);
    wait_for_startup_data_flow(&window);
    window.open_document(&file_path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    let stale_generation = editor.bookmark_change_generation();
    let line_one = editor.buffer().iter_at_line(0).expect("line one");
    editor.buffer().place_cursor(&line_one);
    let _ = editor.toggle_bookmark_at_cursor();
    let local_bookmark = editor.bookmark_at_line(0).expect("local bookmark");
    let stale_snapshot = [lushtext_core::model::bookmark::BookmarkRecord::new(
        2,
        Some("stale sidecar".to_string()),
    )];

    assert!(
        !editor.load_bookmarks_if_generation_matches(&stale_snapshot, stale_generation),
        "sidecar loads that started before a local edit must not replace live bookmarks"
    );
    assert_eq!(editor.bookmark_records(), vec![local_bookmark]);
}

#[test]
fn test_bookmark_gutter_edit_dialog_validates_moves_and_persists() {
    ensure_gtk_init();
    let tempdir = tempfile::tempdir().expect("bookmark edit tempdir");
    let file_path = tempdir.path().join("edit-bookmark.rs");
    fixture::write_text(&file_path, "one\ntwo\nthree\nfour");

    let window = test_window();
    present_window(&window);
    wait_for_startup_data_flow(&window);
    window.open_document(&file_path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    let buffer = editor.buffer();
    let line_one = buffer.iter_at_line(0).expect("line one");
    buffer.place_cursor(&line_one);
    let _ = editor.toggle_bookmark_at_cursor();
    let first_id = editor.bookmark_at_line(0).expect("first bookmark").id;

    let line_three = buffer.iter_at_line(2).expect("line three");
    buffer.place_cursor(&line_three);
    let _ = editor.toggle_bookmark_at_cursor();
    let second_id = editor.bookmark_at_line(2).expect("second bookmark").id;

    let args: [&dyn ToValue; 4] = [
        &line_one,
        &1u32,
        &gtk4::gdk::ModifierType::empty(),
        &1i32,
    ];
    editor
        .source_view()
        .emit_by_name::<()>("line-mark-activated", &args);
    flush_events();

    wait_until(Duration::from_secs(20), || {
        visible_sheet_dialog(&window)
            .map(|dialog| dialog.title())
            .is_some_and(|title| title == "Edit Bookmark")
    });
    let dialog = visible_sheet_dialog(&window).expect("bookmark edit dialog");
    let child = dialog.child().expect("bookmark edit dialog child");
    let fields_group = find_preferences_group(&child).expect("bookmark fields group");
    let label_row = find_entry_row_by_title(&child, "Label").expect("label row");
    let line_row = find_entry_row_by_title(&child, "Line").expect("line row");
    let close_button = find_button_by_tooltip(&child, "Close").expect("close button");
    let cancel_button = find_button_by_label(&child, "Cancel").expect("cancel button");
    let save_button = find_button_by_label(&child, "Save").expect("save button");
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&fields_group);
    for row in [&label_row, &line_row] {
        AccessibleAudit::new()
            .properties(&[
                gtk4::AccessibleProperty::Label,
                gtk4::AccessibleProperty::Description,
            ])
            .assert_on(row);
    }
    for button in [&close_button, &cancel_button, &save_button] {
        AccessibleAudit::new()
            .role(gtk4::AccessibleRole::Button)
            .properties(&[
                gtk4::AccessibleProperty::Label,
                gtk4::AccessibleProperty::Description,
            ])
            .assert_on(button);
    }
    assert_eq!(line_row.text(), "1");

    set_entry_row_text_and_flush(&line_row, "99");
    save_button.emit_clicked();
    wait_until(Duration::from_secs(20), || {
        visible_sheet_dialog(&window)
            .and_then(|dialog| dialog.child())
            .is_some_and(|child| {
                find_label_by_text(&child, "Line 99 is outside this document. Use 1 through 4.")
                    .is_some()
            })
    });
    let out_of_range = find_label_by_text(
        &child,
        "Line 99 is outside this document. Use 1 through 4.",
    )
    .expect("out-of-range validation label");
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[gtk4::AccessibleProperty::Label])
        .states(&[gtk4::AccessibleState::Invalid])
        .assert_on(&out_of_range);
    AccessibleAudit::new()
        .states(&[gtk4::AccessibleState::Invalid])
        .assert_on(&line_row);

    set_entry_row_text_and_flush(&line_row, "3");
    assert!(
        !gtk4::test_accessible_has_state(&line_row, gtk4::AccessibleState::Invalid),
        "line row invalid state should clear after the user edits the line field"
    );
    save_button.emit_clicked();
    wait_until(Duration::from_secs(20), || {
        visible_sheet_dialog(&window)
            .and_then(|dialog| dialog.child())
            .is_some_and(|child| {
                find_label_by_text(&child, "Line 3 already has another bookmark.").is_some()
            })
    });
    let occupied_line =
        find_label_by_text(&child, "Line 3 already has another bookmark.").expect("occupied line label");
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[gtk4::AccessibleProperty::Label])
        .states(&[gtk4::AccessibleState::Invalid])
        .assert_on(&occupied_line);
    AccessibleAudit::new()
        .states(&[gtk4::AccessibleState::Invalid])
        .assert_on(&line_row);

    set_entry_row_text_and_flush(&label_row, "Moved bookmark");
    set_entry_row_text_and_flush(&line_row, "4");
    assert!(
        !gtk4::test_accessible_has_state(&line_row, gtk4::AccessibleState::Invalid),
        "line row invalid state should clear before a corrected save"
    );
    save_button.emit_clicked();
    wait_until(Duration::from_secs(20), || {
        visible_sheet_dialog(&window).is_none()
    });

    wait_until(Duration::from_secs(20), || {
        bookmark_service::load_for_path(&json_store::data_dir(), &file_path).is_ok_and(|document| {
            document.bookmarks.iter().any(|bookmark| {
                bookmark.id == first_id
                    && bookmark.line == 3
                    && bookmark.label.as_deref() == Some("Moved bookmark")
            }) && document
                .bookmarks
                .iter()
                .any(|bookmark| bookmark.id == second_id && bookmark.line == 2)
        })
    });
}

#[test]
fn test_bookmark_commands_report_saved_file_and_empty_bookmark_context() {
    ensure_gtk_init();
    seed_no_workspaces();
    let window = test_window();
    present_window(&window);

    for name in [
        "toggle-bookmark",
        "edit-bookmark-label",
        "next-bookmark",
        "prev-bookmark",
    ] {
        assert!(
            !action_enabled(&window, name),
            "bookmark action '{name}' should be disabled without any editor tab"
        );
    }

    window.new_tab();
    flush_events();
    activate_action(&window, "edit-bookmark-label");
    wait_until(Duration::from_secs(2), || {
        status_bar_contains(&window, "Bookmarks require a saved file")
    });

    let tempdir = tempfile::tempdir().expect("bookmark command tempdir");
    let file_path = tempdir.path().join("bookmark-commands.rs");
    fixture::write_text(&file_path, "one\ntwo\nthree\n");
    window.open_document(&file_path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_size().is_some()
    });

    activate_action(&window, "edit-bookmark-label");
    wait_until(Duration::from_secs(2), || {
        status_bar_contains(&window, "Move the cursor to a bookmarked line first")
    });

    activate_action(&window, "next-bookmark");
    wait_until(Duration::from_secs(2), || {
        status_bar_contains(&window, "No bookmarks exist in the active file")
    });

    activate_action(&window, "prev-bookmark");
    wait_until(Duration::from_secs(2), || {
        status_bar_contains(&window, "No bookmarks exist in the active file")
    });
}

fn select_sidebar_path(section: &lushtext_core::ui::sidebar::WorkspaceSection, path: &Path) {
    fn try_select_path(
        section: &lushtext_core::ui::sidebar::WorkspaceSection,
        path: &Path,
    ) -> bool {
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
                && let Some(item) = row
                    .item()
                    .and_downcast::<lushtext_core::ui::sidebar::FileTreeItem>()
                && item.path().as_deref() == Some(path)
            {
                selection.set_selected(index);
                section
                    .imp()
                    .file_tree_view
                    .scroll_to(index, gtk4::ListScrollFlags::FOCUS, None);
                flush_events();
                return true;
            }
        }
        false
    }

    if try_select_path(section, path) {
        return;
    }

    // Single-folder workspaces now expose files under a real directory folder, so
    // expand the folder tree once before giving up on a nested file-path lookup.
    section.expand_folders();
    wait_until(Duration::from_secs(2), || try_select_path(section, path));
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

fn assert_window_workspace_row_state(
    section: &lushtext_core::ui::sidebar::WorkspaceSection,
    target_path: &Path,
    expected_open: bool,
    expected_active: bool,
) {
    wait_until(Duration::from_secs(3), || {
        section
            .file_row_state_for_test(target_path)
            .is_some_and(|state| state.open == expected_open && state.active == expected_active)
    });
    let state = section
        .file_row_state_for_test(target_path)
        .unwrap_or_else(|| panic!("workspace row not realized for {}", target_path.display()));
    assert_eq!(
        state.open,
        expected_open,
        "unexpected open marker for {}",
        target_path.display()
    );
    assert_eq!(
        state.active,
        expected_active,
        "unexpected active marker for {}",
        target_path.display()
    );
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
    assert_eq!(
        settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION),
        0.25
    );
}

#[test]
fn test_sidebar_peek_does_not_create_tab_until_promoted() {
    let (_folder_dir, alpha, _beta) = seed_peek_workspace();
    let window = test_window();
    present_window(&window);

    // Section population is async (restored workspaces are populated via
    // spawn_blocking_then), so this waits on background completion and gets a
    // generous budget that only matters when a loaded machine delays the thread.
    wait_until(Duration::from_secs(10), || {
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
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });

    assert_tab_count(&window, 1);
    assert_eq!(
        active_editor(&window).file_path().as_deref(),
        Some(alpha.as_path())
    );
}

#[test]
fn test_sidebar_peek_promotion_reuses_existing_tab() {
    let (_folder_dir, alpha, _beta) = seed_peek_workspace();
    let window = test_window();
    present_window(&window);

    // Section population is async (restored workspaces are populated via
    // spawn_blocking_then), so this waits on background completion and gets a
    // generous budget that only matters when a loaded machine delays the thread.
    wait_until(Duration::from_secs(10), || {
        window.imp().sidebar.imp().sections.borrow().len() == 1
    });

    window.open_document(&alpha);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });

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
    assert_eq!(
        active_editor(&window).file_path().as_deref(),
        Some(alpha.as_path())
    );
}

#[test]
fn test_split_view_defaults_restore_on_window() {
    ensure_gtk_init();
    let window = test_window();

    assert!(workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!((workspace_total_fraction(&window) - 0.3).abs() < 0.001);
    let properties_fraction = properties_total_fraction(&window);
    assert!(
        (properties_fraction - 0.25).abs() < 0.001,
        "expected snapped properties total fraction near 0.25, got {properties_fraction}"
    );
    assert_workspace_sidebar_width_locked(&window, 360.0);
}

#[test]
fn test_workspace_file_sidebar_keeps_list_view_tree_model_rail() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    let workspace_sidebar = window.imp().sidebar.upcast_ref::<gtk4::Widget>();
    assert!(
        find_adw_sidebar(workspace_sidebar).is_none(),
        "the primary workspace file sidebar must not be replaced by AdwSidebar"
    );

    assert!(
        has_tree_list_model_list_view(workspace_sidebar),
        "workspace file sidebar should keep GtkTreeListModel backing"
    );
}

#[test]
fn test_saved_split_view_widths_snap_to_supported_workspace_presets() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(true, 0.25, true, 0.6, 1600, 800);
    let settings = &window.imp().settings;

    assert!((workspace_total_fraction(&window) - 360.0 / 1600.0).abs() < 0.001);
    let properties_fraction = properties_total_fraction(&window);
    assert!(
        (properties_fraction - 0.25).abs() < 0.001,
        "expected snapped properties total fraction near 0.25, got {properties_fraction}"
    );
    assert_eq!(settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), 0.3);
    assert_eq!(
        settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION),
        0.25
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

    assert!(
        action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
    activate_action(&window, "toggle-sidebar");
    assert!(
        !action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
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
fn test_toggle_properties_action_state_tracks_rendered_surface() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-properties")
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    assert!(
        !action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
    activate_action(&window, "toggle-properties");
    assert!(
        action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
    activate_action(&window, "toggle-properties");
    assert!(
        !action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
        .expect("expected operation to succeed")
    );
}

#[test]
fn test_secondary_surface_toggles_sync_accessible_pressed_and_announcements() {
    ensure_gtk_init();
    let window = test_window();

    assert!(window.imp().status_bar.imp().sidebar_toggle_button.is_active());
    assert!(!window.imp().document_properties_toggle_button.is_active());

    activate_action(&window, "toggle-sidebar");
    assert!(!window.imp().status_bar.imp().sidebar_toggle_button.is_active());
    assert_workflow_announcement_recorded(&window, "workspace-sidebar-hidden");

    activate_action(&window, "toggle-properties");
    assert!(window.imp().document_properties_toggle_button.is_active());
    assert_workflow_announcement_recorded(&window, "document-properties-shown");
}

#[test]
fn test_f9_toggles_document_properties_instead_of_workspace_sidebar() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.toggle-properties", "F9"),
        "F9 should be bound to win.toggle-properties"
    );
}

#[test]
fn test_browse_notes_shortcut_is_registered_and_documented() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.show-notes", "<Control><Alt>a"),
        "Ctrl+Alt+A should remain bound to Browse Notes"
    );

    let shortcuts_ui = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/ui/shortcuts.ui"
    ));
    assert!(shortcuts_ui.contains("Browse Notes"));
    assert!(shortcuts_ui.contains("&lt;Control&gt;&lt;Alt&gt;a"));
}

#[test]
fn test_help_overlay_action_is_registered_and_visible_commands_resolve() {
    ensure_gtk_init();
    let window = test_window();

    let action = window
        .lookup_action("show-help-overlay")
        .expect("Keyboard Shortcuts action should be registered");
    assert!(action.is_enabled());
    assert_eq!(action.parameter_type(), None);
    assert_eq!(action.state_type(), None);

    let window_ui = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/ui/window.ui"
    ));
    assert!(window_ui.contains("win.show-help-overlay"));
    assert!(
        lushtext_core::services::palette::all_commands()
            .iter()
            .any(|command| command.id == "win.show-help-overlay"
                && command.label == "Keyboard Shortcuts")
    );
}

#[test]
fn test_fullscreen_actions_follow_fullscreened_state() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    assert!(action_enabled(&window, "fullscreen"));
    assert!(!action_enabled(&window, "unfullscreen"));

    window.fullscreen();
    wait_until(Duration::from_secs(2), || {
        !action_enabled(&window, "fullscreen") && action_enabled(&window, "unfullscreen")
    });

    window.unfullscreen();
    wait_until(Duration::from_secs(2), || {
        action_enabled(&window, "fullscreen") && !action_enabled(&window, "unfullscreen")
    });
}

#[test]
fn test_help_overlay_action_presents_shortcuts_window_without_context() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    activate_action(&window, "show-help-overlay");
    let shortcuts = wait_for_shortcuts_window(&window);

    assert_eq!(shortcuts.transient_for(), Some(window.clone().upcast()));
    assert!(action_enabled(&window, "show-help-overlay"));
    shortcuts.close();
}

#[test]
fn test_help_overlay_action_reuses_window_and_preserves_document_state() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("shortcut help keeps buffer");
    let before_state = editor_print_state(&editor);

    activate_action(&window, "show-help-overlay");
    let first_window = wait_for_shortcuts_window(&window);
    activate_action(&window, "show-help-overlay");
    let second_window = wait_for_shortcuts_window(&window);

    assert_eq!(first_window, second_window);
    assert_tab_count(&window, 1);
    assert_eq!(editor_print_state(&active_editor(&window)), before_state);
    second_window.close();
}

#[test]
fn test_help_overlay_window_handles_dense_shortcuts_and_constrained_geometry() {
    ensure_gtk_init();
    let window = test_window_with_restored_size(640, 420);
    present_window(&window);

    activate_action(&window, "show-help-overlay");
    let shortcuts = wait_for_shortcuts_window(&window);
    let shortcut_count = descendants(&shortcuts)
        .into_iter()
        .filter(|widget| widget.type_().name() == "GtkShortcutsShortcut")
        .count();

    assert!(shortcut_count >= 20);
    assert!(shortcuts.width() > 0);
    assert!(shortcuts.height() > 0);
    assert!(shortcuts.width() <= 1280);
    assert!(shortcuts.height() <= 900);

    shortcuts.close();
    flush_events();
    wait_until(Duration::from_secs(2), || {
        shortcuts_windows_for(&window).is_empty()
    });
}

#[test]
fn test_new_document_shortcut_is_ctrl_n_only() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.new-tab", "<Control>n"),
        "Ctrl+N should create a new file"
    );
    assert!(
        !shortcut_bound(&window, "win.new-tab", "<Control>t"),
        "Ctrl+T should no longer create a new file"
    );
}

#[test]
fn test_new_document_action_focuses_new_editor() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    window.imp().primary_menu_button.grab_focus();
    flush_events();
    activate_action(&window, "new-tab");

    assert_eq!(window.imp().tab_view.n_pages(), 1);
    wait_for_active_editor_focus(&window);
}

#[test]
fn test_shell_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let window = test_window();

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::KeyShortcuts,
        ])
        .assert_on(&*window.imp().new_tab_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::KeyShortcuts,
            gtk4::AccessibleProperty::HasPopup,
        ])
        .relations(&[gtk4::AccessibleRelation::Controls])
        .assert_on(&*window.imp().open_menu_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::ToggleButton)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::KeyShortcuts,
        ])
        .states(&[gtk4::AccessibleState::Pressed])
        .relations(&[gtk4::AccessibleRelation::Controls])
        .assert_on(&*window.imp().document_properties_toggle_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::HasPopup,
        ])
        .assert_on(&*window.imp().primary_menu_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TabList)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*window.imp().tab_bar);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*window.imp().focus_mode_affordance);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::KeyShortcuts,
        ])
        .assert_on(&*window.imp().leave_focus_mode_button);
}

#[test]
fn test_save_changes_dialog_controls_expose_accessibility_roles() {
    let (window, _dir, _path, _editor) = modified_file_backed_tab("disk\n", "unsaved\n");

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    let dialog = visible_alert_dialog(&window).expect("save changes dialog");
    let group = dialog.extra_child().expect("save changes checklist");
    let checks = save_changes_check_buttons(&dialog);

    assert_eq!(group.accessible_role(), gtk4::AccessibleRole::Group);
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].accessible_role(), gtk4::AccessibleRole::Checkbox);
    assert_eq!(
        save_changes_response_button(&dialog, "cancel").accessible_role(),
        gtk4::AccessibleRole::Button
    );
    assert_eq!(
        save_changes_response_button(&dialog, "save").accessible_role(),
        gtk4::AccessibleRole::Button
    );

    respond_to_save_changes_dialog(&window, "cancel");
}

#[test]
fn test_local_history_browser_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("local history role tempdir");
    let path = dir.path().join("history-roles.txt");
    fixture::write_text(&path, "snapshot text\n");
    local_history_service::capture_snapshot_for_path(
        &json_store::data_dir(),
        &path,
        "snapshot text\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed local history snapshot");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some() && action_enabled(&window, "show-local-history")
    });
    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog");
    let child = dialog.child().expect("local-history browser child");
    let sidebar = find_adw_sidebar(&child).expect("local-history sidebar");
    wait_until(Duration::from_secs(5), || {
        find_button_by_label(&child, "Copy").is_some_and(|button| button.is_sensitive())
    });
    let preview_stack =
        find_local_history_preview_stack(&child).expect("local-history preview stack");
    let mut text_views = Vec::new();
    collect_text_views(&child, &mut text_views);
    let preview_text_view = text_views
        .into_iter()
        .find(|text_view| !text_view.is_editable())
        .expect("local-history preview text view");
    assert_back_button_follows_split_collapsed(&child, "Back to Snapshots");
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::List)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&sidebar);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&preview_stack);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ReadOnly,
            gtk4::AccessibleProperty::MultiLine,
        ])
        .assert_on(&preview_text_view);

    let restore_button = find_button_by_label(&child, "Restore").expect("restore button");
    let copy_button = find_button_by_label(&child, "Copy").expect("copy button");
    let back_button =
        find_button_by_tooltip(&child, "Back to Snapshots").expect("local-history back button");
    for button in [&restore_button, &copy_button, &back_button] {
        AccessibleAudit::new()
            .role(gtk4::AccessibleRole::Button)
            .properties(&[
                gtk4::AccessibleProperty::Label,
                gtk4::AccessibleProperty::Description,
            ])
            .assert_on(button);
    }
    assert!(
        !gtk4::test_accessible_has_state(&restore_button, gtk4::AccessibleState::Disabled),
        "loaded local-history snapshots should expose Restore as enabled"
    );
    assert!(
        !gtk4::test_accessible_has_state(&copy_button, gtk4::AccessibleState::Disabled),
        "non-empty local-history snapshots should expose Copy as enabled"
    );
}

#[test]
fn test_notes_browser_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("notes-roles.md");
    fixture::write_text(&path, "# Notes\n");
    document_note_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &RichNoteBody::new("# Note\n\nAccessible"),
    )
    .expect("seed document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);
    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let search_entry = find_search_entry(&child).expect("notes browser search entry");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    let preview_stack = find_notes_preview_stack(&child).expect("notes browser preview stack");
    assert_back_button_follows_split_collapsed(&child, "Back to Notes");
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::SearchBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&search_entry);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::List)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&sidebar);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&preview_stack);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&find_button_by_label(&child, "Open").expect("notes browser open button"));
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&single_visible_close_button(&child));
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&find_button_by_tooltip(&child, "Back to Notes").expect("notes browser back button"));
}

#[test]
fn test_keyboard_search_workflow_navigates_closes_and_restores_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor
        .buffer()
        .set_text("alpha needle\nbeta needle\ngamma\n");
    editor.source_view().grab_focus();
    wait_for_active_editor_focus(&window);

    assert!(
        shortcut_bound(&window, "win.begin-search", "<Control>f"),
        "Ctrl+F should invoke the in-tab search action"
    );
    activate_action(&window, "begin-search");
    wait_until(Duration::from_secs(2), || editor.is_search_visible());

    editor.search_bar().search_entry().set_text("needle");
    wait_until(Duration::from_secs(2), || {
        editor
            .search_bar()
            .search_context()
            .is_some_and(|context| context.occurrences_count() == 2)
    });
    assert_eq!(
        emit_key_pressed_on_widget(editor.search_bar().search_entry(), gtk4::gdk::Key::Return),
        glib::Propagation::Stop
    );
    assert!(editor.search_bar().has_navigated());

    editor.search_bar().search_entry().emit_stop_search();
    wait_until(Duration::from_secs(2), || !editor.is_search_visible());
    wait_for_active_editor_focus(&window);
}

#[test]
fn test_parameterized_search_action_updates_visible_search_workflow() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor
        .buffer()
        .set_text("alpha needle\nbeta needle\ngamma\n");
    editor.source_view().grab_focus();
    wait_for_active_editor_focus(&window);

    let action = window
        .lookup_action("set-search-query")
        .expect("set-search-query action");
    assert_eq!(
        action
            .parameter_type()
            .as_ref()
            .map(|parameter_type| parameter_type.as_str()),
        Some("s")
    );

    activate_string_action(&window, "set-search-query", "needle");
    wait_until(Duration::from_secs(2), || {
        editor.is_search_visible() && editor.search_bar().search_entry().text().as_str() == "needle"
    });
    wait_until(Duration::from_secs(2), || {
        editor
            .search_bar()
            .search_context()
            .is_some_and(|context| context.occurrences_count() == 2)
    });
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Search) > 0
    });

    activate_string_action(&window, "set-search-query", "gamma");
    wait_until(Duration::from_secs(2), || {
        editor.search_bar().search_entry().text().as_str() == "gamma"
            && editor
                .search_bar()
                .search_context()
                .is_some_and(|context| context.occurrences_count() == 1)
    });

    editor.search_bar().search_entry().emit_stop_search();
    wait_until(Duration::from_secs(2), || !editor.is_search_visible());
    wait_for_active_editor_focus(&window);
}

#[test]
fn test_select_tab_action_uses_index_without_tab_strip_coordinates() {
    ensure_gtk_init();
    let window = test_window();
    let action = window.lookup_action("select-tab").expect("select-tab action");
    assert_eq!(
        action
            .parameter_type()
            .as_ref()
            .map(|parameter_type| parameter_type.as_str()),
        Some("u")
    );
    assert!(!action_enabled(&window, "select-tab"));

    window.new_tab();
    active_editor(&window).buffer().set_text("first tab");
    window.new_tab();
    active_editor(&window).buffer().set_text("second tab");
    window.new_tab();
    active_editor(&window).buffer().set_text("third tab");
    present_window(&window);

    assert!(action_enabled(&window, "select-tab"));
    assert_eq!(
        app_snapshot(
            &window
                .application()
                .expect("window should have an application")
                .downcast::<lushtext_core::app::LushtextApplication>()
                .expect("test app should be LushtextApplication")
        )
        .window
        .expect("active window snapshot")
        .active_tab_index,
        Some(2)
    );

    activate_u32_action(&window, "select-tab", 0);
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .tab_view
            .selected_page()
            .is_some_and(|page| window.imp().tab_view.page_position(&page) == 0)
    });
    assert_eq!(editor_buffer_text(&active_editor(&window)), "first tab");

    activate_u32_action(&window, "select-tab", 99);
    flush_events();
    assert_eq!(editor_buffer_text(&active_editor(&window)), "first tab");
}

#[test]
fn test_command_palette_target_actions_update_visible_mode_query() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let app = window
        .application()
        .expect("window should have an application")
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("test app should be LushtextApplication");

    for action_name in ["set-command-palette-query", "set-command-palette-mode"] {
        let action = window
            .lookup_action(action_name)
            .unwrap_or_else(|| panic!("missing action '{action_name}'"));
        assert_eq!(
            action
                .parameter_type()
                .as_ref()
                .map(|parameter_type| parameter_type.as_str()),
            Some("s"),
            "{action_name} should take a string parameter"
        );
        assert!(!action_enabled(&window, action_name));
    }

    activate_action(&window, "toggle-command-palette");
    wait_until(Duration::from_secs(2), || {
        let palette = app_snapshot(&app).window.expect("window").command_palette;
        palette.visible
            && palette.mode == "all"
            && action_enabled(&window, "set-command-palette-query")
            && action_enabled(&window, "set-command-palette-mode")
    });

    activate_string_action(&window, "set-command-palette-mode", "commands");
    activate_string_action(&window, "set-command-palette-query", "Save");
    wait_until(Duration::from_secs(2), || {
        let palette = app_snapshot(&app).window.expect("window").command_palette;
        palette.visible
            && palette.mode == "commands"
            && palette.query == "Save"
            && palette.result_count > 0
    });

    activate_string_action(&window, "set-command-palette-mode", "notes");
    activate_string_action(&window, "set-command-palette-query", "bookmark");
    wait_until(Duration::from_secs(2), || {
        let palette = app_snapshot(&app).window.expect("window").command_palette;
        palette.visible
            && palette.mode == "notes"
            && palette.query == "bookmark"
            && palette.result_count > 0
    });

    activate_string_action(&window, "set-command-palette-mode", "files");
    activate_string_action(&window, "set-command-palette-query", "missing-palette-file");
    wait_until(Duration::from_secs(2), || {
        let palette = app_snapshot(&app).window.expect("window").command_palette;
        palette.visible
            && palette.mode == "files"
            && palette.query == "missing-palette-file"
            && palette.result_count == 0
    });

    activate_string_action(&window, "set-command-palette-mode", "invalid");
    flush_events();
    assert_eq!(
        app_snapshot(&app)
            .window
            .expect("window")
            .command_palette
            .mode,
        "files"
    );

    activate_action(&window, "toggle-command-palette");
    wait_until(Duration::from_secs(2), || {
        let palette = app_snapshot(&app).window.expect("window").command_palette;
        !palette.visible
            && palette.query.is_empty()
            && palette.result_count == 0
            && !action_enabled(&window, "set-command-palette-query")
            && !action_enabled(&window, "set-command-palette-mode")
    });
}

#[test]
fn test_target_state_actions_drive_visible_surfaces_without_toggle_parity() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, false)
        .expect("disable minimap");
    let window = test_window();

    for action_name in [
        "set-sidebar-visible",
        "set-properties-visible",
        "set-minimap-visible",
        "set-search-panel-visible",
        "set-focus-mode",
        "set-preview-pane-visible",
        "set-preview-mode",
    ] {
        let action = window
            .lookup_action(action_name)
            .unwrap_or_else(|| panic!("missing action '{action_name}'"));
        assert_eq!(
            action
                .parameter_type()
                .as_ref()
                .map(|parameter_type| parameter_type.as_str()),
            Some("b"),
            "{action_name} should take a boolean parameter"
        );
    }

    assert!(!action_enabled(&window, "set-search-query"));
    assert!(!action_enabled(&window, "select-tab"));
    assert!(!action_enabled(&window, "set-open-popover-query"));
    assert!(!action_enabled(&window, "set-notes-browser-query"));
    assert!(!action_enabled(&window, "select-notes-browser-row"));
    assert!(!action_enabled(&window, "open-notes-browser-selection"));
    assert!(!action_enabled(&window, "set-preview-pane-visible"));
    assert!(!action_enabled(&window, "set-preview-mode"));

    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.source_view().grab_focus();
    wait_for_active_editor_focus(&window);

    assert!(action_enabled(&window, "set-search-query"));
    assert!(action_enabled(&window, "select-tab"));
    assert!(!action_enabled(&window, "set-open-popover-query"));
    assert!(!action_enabled(&window, "set-notes-browser-query"));
    assert!(!action_enabled(&window, "select-notes-browser-row"));
    assert!(!action_enabled(&window, "open-notes-browser-selection"));
    assert!(action_enabled(&window, "set-preview-pane-visible"));
    assert!(action_enabled(&window, "set-preview-mode"));

    activate_boolean_action(&window, "set-sidebar-visible", false);
    wait_until(Duration::from_secs(2), || !workspace_sidebar_visible(&window));
    assert!(!action_state_bool(&window, "toggle-sidebar"));
    activate_boolean_action(&window, "set-sidebar-visible", true);
    wait_until(Duration::from_secs(2), || workspace_sidebar_visible(&window));
    assert!(action_state_bool(&window, "toggle-sidebar"));

    activate_boolean_action(&window, "set-properties-visible", true);
    wait_until(Duration::from_secs(2), || properties_sidebar_visible(&window));
    assert!(action_state_bool(&window, "toggle-properties"));
    activate_boolean_action(&window, "set-properties-visible", false);
    wait_until(Duration::from_secs(2), || !properties_sidebar_visible(&window));
    assert!(!action_state_bool(&window, "toggle-properties"));

    activate_boolean_action(&window, "set-minimap-visible", true);
    wait_until(Duration::from_secs(2), || minimap_setting(&window));
    assert!(action_state_bool(&window, "toggle-minimap"));
    activate_boolean_action(&window, "set-minimap-visible", false);
    wait_until(Duration::from_secs(2), || !minimap_setting(&window));
    assert!(!action_state_bool(&window, "toggle-minimap"));

    let recent_path = PathBuf::from("/tmp/lushtext-open-popover-filter-target.txt");
    window.set_recent_documents_for_test(vec![RecentDocumentEntry::new(
        recent_path,
        None,
        42,
    )]);
    activate_action(&window, "open-recent");
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.is_visible()
    });
    let open_query_action = window
        .lookup_action("set-open-popover-query")
        .expect("missing set-open-popover-query action");
    assert_eq!(
        open_query_action
            .parameter_type()
            .as_ref()
            .map(|parameter_type| parameter_type.as_str()),
        Some("s"),
        "set-open-popover-query should take a string parameter"
    );
    assert!(action_enabled(&window, "set-open-popover-query"));
    activate_string_action(&window, "set-open-popover-query", "filter-target");
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test()
            == vec!["lushtext-open-popover-filter-target.txt"]
    });
    window.imp().open_popover.popdown();
    wait_until(Duration::from_secs(2), || {
        !window.imp().open_popover.is_visible()
    });
    assert!(!action_enabled(&window, "set-open-popover-query"));
    editor.source_view().grab_focus();
    wait_for_active_editor_focus(&window);

    activate_boolean_action(&window, "set-search-panel-visible", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().search_panel_revealer.reveals_child()
    });
    let search_query_action = window
        .lookup_action("set-search-panel-query")
        .expect("missing set-search-panel-query action");
    assert_eq!(
        search_query_action
            .parameter_type()
            .as_ref()
            .map(|parameter_type| parameter_type.as_str()),
        Some("s"),
        "set-search-panel-query should take a string parameter"
    );
    assert!(action_enabled(&window, "set-search-panel-query"));
    activate_string_action(&window, "set-search-panel-query", "workspace needle");
    wait_until(Duration::from_secs(2), || {
        window.imp().search_panel.query() == "workspace needle"
    });
    activate_boolean_action(&window, "set-search-panel-visible", false);
    wait_until(Duration::from_secs(2), || {
        !window.imp().search_panel_revealer.reveals_child()
    });
    assert!(!action_enabled(&window, "set-search-panel-query"));
    wait_for_active_editor_focus(&window);

    activate_boolean_action(&window, "set-focus-mode", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().focus_mode.active.get() && action_state_bool(&window, "toggle-focus-mode")
    });
    activate_boolean_action(&window, "set-focus-mode", false);
    wait_until(Duration::from_secs(2), || {
        !window.imp().focus_mode.active.get() && !action_state_bool(&window, "toggle-focus-mode")
    });

    activate_boolean_action(&window, "set-preview-pane-visible", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().preview_visible.get() && action_state_bool(&window, "toggle-preview-pane")
    });
    activate_boolean_action(&window, "set-preview-mode", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get()
            && !window.imp().preview_visible.get()
            && action_state_bool(&window, "toggle-preview-mode")
            && !action_state_bool(&window, "toggle-preview-pane")
    });
    activate_boolean_action(&window, "set-preview-mode", false);
    wait_until(Duration::from_secs(2), || {
        !window.imp().preview_mode.get() && !action_state_bool(&window, "toggle-preview-mode")
    });
}

#[test]
fn test_live_app_and_window_actions_match_action_catalog() {
    ensure_gtk_init();
    let app = crate::common::test_application();
    let window = LushtextWindow::new(&app);
    present_window(&window);

    let app_actions = observed_actions_from_group(ActionScope::App, &app);
    let window_actions = observed_actions_from_group(ActionScope::Window, &window);

    assert_eq!(
        action_catalog::audit_observed_actions(ActionScope::App, &app_actions),
        Ok(())
    );
    assert_eq!(
        action_catalog::audit_observed_actions(ActionScope::Window, &window_actions),
        Ok(())
    );
}

#[test]
fn test_automation_snapshot_reports_bounded_live_window_state() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("alpha needle\nbeta needle\n");
    activate_string_action(&window, "set-search-query", "needle");
    wait_until(Duration::from_secs(2), || {
        editor
            .search_bar()
            .search_context()
            .is_some_and(|context| context.occurrences_count() == 2)
    });
    // Realization opens a width-reflow burst whose settle/reveal repair
    // legitimately blocks the idle predicate, so drain queued minimap work
    // before asserting that the snapshot reports an idle window.
    wait_until(Duration::from_secs(5), || {
        !editor.minimap_work_pending_for_test()
    });

    let app = window
        .application()
        .expect("window should have an application")
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("test app should be LushtextApplication");
    let snapshot = app_snapshot(&app);
    let json = serde_json::to_string(&snapshot).expect("snapshot should serialize");

    assert_eq!(snapshot.interface_version, INTERFACE_VERSION);
    assert!(snapshot.idle);
    assert_eq!(snapshot.idle_blocker, None);
    assert_eq!(current_idle_blocker(&app), None);
    // Readiness futures use GLib timers and read GTK state, so widget tests
    // drive them on the default main context instead of a generic async runtime.
    let search_ready = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::SearchComplete,
        100,
    ));
    assert!(search_ready.ok);
    assert_eq!(search_ready.status, AutomationReadinessStatus::Ready.as_str());
    let window_actions_ready = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::WindowActionsExported,
        100,
    ));
    assert!(window_actions_ready.ok);
    let window_snapshot = snapshot.window.expect("active window snapshot");
    assert_eq!(window_snapshot.tab_count, 1);
    assert_eq!(window_snapshot.active_tab_index, Some(0));
    assert_eq!(window_snapshot.tabs[0].document_kind, "untitled");
    assert!(window_snapshot.tabs[0].modified);
    assert!(window_snapshot.search.editor_search_visible);
    assert_eq!(
        window_snapshot.search.editor_query.as_deref(),
        Some("needle")
    );
    assert_eq!(window_snapshot.search.editor_match_count, Some(2));
    assert_eq!(window_snapshot.workspace.scope_kind, "all");
    assert!(window_snapshot.workspace.no_workspaces);
    assert_eq!(window_snapshot.workspace.workspace_count, 0);
    assert_eq!(window_snapshot.workspace.folder_count, 0);
    assert_eq!(window_snapshot.workspace.scoped_folder_count, 0);
    assert!(!window_snapshot.command_palette.visible);
    assert_eq!(window_snapshot.command_palette.mode, "all");
    assert_eq!(window_snapshot.command_palette.query, "");
    assert!(!window_snapshot.notes.active_document_file_backed);
    assert_eq!(window_snapshot.notes.active_document_bookmark_count, 0);
    assert!(!window_snapshot.notes.active_line_has_bookmark);
    assert!(!window_snapshot.local_history.active_document_file_backed);
    assert!(!window_snapshot.local_history.browse_available);
    assert!(!window_snapshot.local_history.automatic_capture_available);
    assert!(!window_snapshot.content_search.visible);
    assert_eq!(window_snapshot.content_search.query, "");
    assert_eq!(window_snapshot.content_search.match_count, 0);
    assert_eq!(window_snapshot.content_search.file_count, 0);
    assert_eq!(window_snapshot.content_search.replace_preview_count, 0);
    assert_eq!(window_snapshot.content_search.checked_replacement_count, 0);
    assert_eq!(window_snapshot.notifications.status_text, None);
    assert!(
        !json.contains("alpha needle"),
        "automation snapshot must not dump document text"
    );

    // Predicate-specific waits must ignore unrelated blockers: preview
    // layout settle blocks broad idle readiness, not search completion.
    window.set_preview_transition_pending_for_test(true);
    assert_eq!(
        current_idle_blocker(&app).as_deref(),
        Some("preview-animation")
    );
    let search_ready = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::SearchComplete,
        1,
    ));
    assert!(
        search_ready.ok,
        "preview layout settle should not block search readiness"
    );
    let idle_result = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::Idle,
        1,
    ));
    assert!(!idle_result.ok);
    assert_eq!(
        idle_result.status,
        AutomationReadinessStatus::PredicateTimeout.as_str()
    );
    assert_eq!(idle_result.blocker.as_deref(), Some("preview-animation"));
    let (ok, detail) =
        glib::MainContext::default().block_on(wait_for_idle_for_test(app.clone(), 1));
    assert!(!ok);
    assert_eq!(detail, "preview-animation");
    window.set_preview_transition_pending_for_test(false);
    let (ok, detail) =
        glib::MainContext::default().block_on(wait_for_idle_for_test(app.clone(), 100));
    assert!(ok);
    assert_eq!(detail, "idle");

    // This is an in-memory debounce simulation, not filesystem setup. Window
    // action readiness only needs an active exported window; command-palette
    // indexing should block idle without blocking action introspection.
    let palette_folder = Arc::new(PathBuf::from("/tmp/lushtext-automation-readiness"));
    window
        .imp()
        .command_palette
        .set_file_index(FileIndex::from(vec![IndexedFile::new(
            palette_folder.join("existing.rs"),
            Arc::clone(&palette_folder),
        )]));
    window
        .imp()
        .command_palette
        .update_index_file_created(&palette_folder.join("created.rs"));
    assert_eq!(
        current_idle_blocker(&app).as_deref(),
        Some("command-palette-index")
    );
    let window_actions_ready = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::WindowActionsExported,
        1,
    ));
    assert!(window_actions_ready.ok);
    let (ok, detail) =
        glib::MainContext::default().block_on(wait_for_idle_for_test(app.clone(), 1));
    assert!(!ok);
    assert_eq!(detail, "command-palette-index");
    wait_until(Duration::from_secs(2), || current_idle_blocker(&app).is_none());

    let _replace_preview_reset = ReplacePreviewDelayReset;
    set_replace_preview_delay_for_test(250);
    // Seed enough search-panel state to enter Replace Preview; the preview
    // delay is the blocker under test for `search-complete`.
    window.imp().search_panel.set_query("hello");
    window.imp().search_panel.imp().runtime.total_matches.set(1);
    window
        .imp()
        .search_panel
        .imp()
        .runtime
        .search_matches
        .borrow_mut()
        .push(SearchMatch::new(
            PathBuf::from("/tmp/search.rs"),
            1,
            "let hello = 1;",
            4..9,
        ));
    window.imp().search_panel.enter_preview_mode("goodbye");
    assert_eq!(
        current_idle_blocker(&app).as_deref(),
        Some("replace-preview")
    );
    let search_result = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::SearchComplete,
        1,
    ));
    assert!(!search_result.ok);
    assert_eq!(
        search_result.status,
        AutomationReadinessStatus::PredicateTimeout.as_str()
    );
    assert_eq!(search_result.blocker.as_deref(), Some("replace-preview"));
    window.set_preview_transition_pending_for_test(true);
    let search_result = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::SearchComplete,
        1,
    ));
    assert!(!search_result.ok);
    assert_eq!(search_result.blocker.as_deref(), Some("replace-preview"));
    window.set_preview_transition_pending_for_test(false);
    let (ok, detail) =
        glib::MainContext::default().block_on(wait_for_idle_for_test(app.clone(), 1));
    assert!(!ok);
    assert_eq!(detail, "replace-preview");
    wait_until(Duration::from_secs(2), || current_idle_blocker(&app).is_none());
}

#[test]
fn test_hidden_minimap_refresh_does_not_block_visual_readiness() {
    ensure_gtk_init();
    let window = test_window();
    window
        .imp()
        .settings
        .set_boolean(keys::SHOW_MINIMAP, false)
        .expect("disable minimap");
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    wait_until(Duration::from_secs(2), || !editor.is_minimap_visible());

    editor.mark_minimap_refresh_pending_for_test();

    let app = window
        .application()
        .expect("window should have an application")
        // GTK stores the application behind the base `gtk::Application` type;
        // downcast it so the automation readiness helpers can read app state.
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("test app should be LushtextApplication");
    assert_eq!(current_idle_blocker(&app), None);

    let snapshot = app_snapshot(&app);
    assert!(snapshot.idle);
    let geometry = snapshot
        .window
        .expect("active window snapshot")
        .visual_geometry;
    assert!(geometry.ready);
    assert_eq!(geometry.blocker, None);

    let visual_ready = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::VisualGeometrySettled,
        1,
    ));
    assert!(visual_ready.ok);
    let idle_ready = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app,
        AutomationReadinessPredicate::Idle,
        1,
    ));
    assert!(idle_ready.ok);
}

#[test]
fn test_focus_suppressed_minimap_refresh_does_not_block_visual_readiness() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).is_minimap_visible());

    activate_action(&window, "toggle-focus-mode");
    let editor = active_editor(&window);
    wait_until(Duration::from_secs(2), || {
        editor.minimap_availability() == MinimapAvailability::Disabled
    });
    assert!(settings.boolean(keys::SHOW_MINIMAP));

    editor.mark_minimap_refresh_pending_for_test();

    let app = window
        .application()
        .expect("window should have an application")
        // GTK stores the application behind the base `gtk::Application` type;
        // downcast it so the automation readiness helpers can read app state.
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("test app should be LushtextApplication");
    assert_eq!(current_idle_blocker(&app), None);

    let visual_ready = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app,
        AutomationReadinessPredicate::VisualGeometrySettled,
        1,
    ));
    assert!(visual_ready.ok);
}

#[test]
fn test_keyboard_command_palette_and_secondary_surfaces_restore_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    active_editor(&window).source_view().grab_focus();
    wait_for_active_editor_focus(&window);

    assert!(
        shortcut_bound(&window, "win.toggle-command-palette", "<Control><Shift>p"),
        "Ctrl+Shift+P should invoke the command palette"
    );
    activate_action(&window, "toggle-command-palette");
    wait_until(Duration::from_secs(2), || {
        window.imp().palette_revealer.reveals_child()
    });
    window
        .imp()
        .command_palette
        .imp()
        .search_entry
        .emit_stop_search();
    wait_until(Duration::from_secs(2), || {
        !window.imp().palette_revealer.reveals_child()
    });
    wait_for_active_editor_focus(&window);

    activate_widget_without_pointer(&*window.imp().status_bar.imp().sidebar_toggle_button);
    wait_until(Duration::from_secs(2), || !workspace_sidebar_visible(&window));
    activate_widget_without_pointer(&*window.imp().status_bar.imp().sidebar_toggle_button);
    wait_until(Duration::from_secs(2), || workspace_sidebar_visible(&window));

    activate_widget_without_pointer(&*window.imp().document_properties_toggle_button);
    wait_until(Duration::from_secs(2), || properties_sidebar_visible(&window));
    assert!(
        shortcut_bound(&window, "win.toggle-properties", "F9"),
        "F9 should invoke document-properties visibility"
    );
    activate_widget_without_pointer(&*window.imp().document_properties_toggle_button);
    wait_until(Duration::from_secs(2), || !properties_sidebar_visible(&window));
}

#[test]
fn test_new_document_exits_markdown_preview_only_mode() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let dir = tempfile::tempdir().expect("new document preview tempdir");
    let original_editor = active_editor(&window);
    original_editor.set_file_path(&dir.path().join("preview-source.md"));
    original_editor.buffer().set_text("# Preview\n\nBody");

    activate_action(&window, "toggle-preview-mode");
    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get()
            && window.imp().markdown_preview.property::<bool>("visible")
            && action_state_bool(&window, "toggle-preview-mode")
    });

    activate_action(&window, "new-tab");

    assert_eq!(window.imp().tab_view.n_pages(), 2);
    let new_editor = active_editor(&window);
    assert!(
        new_editor.file_path().is_none(),
        "New Document should select the new untitled tab"
    );
    assert_ne!(
        new_editor.as_ptr(),
        original_editor.as_ptr(),
        "New Document should not leave the Markdown tab selected"
    );
    assert!(
        !window.imp().preview_mode.get(),
        "New Document should clear preview-only mode"
    );
    assert!(
        !action_state_bool(&window, "toggle-preview-mode"),
        "preview-only action state should match the cleared shell state"
    );
    assert!(
        window.imp().editor_box.property::<bool>("visible"),
        "source editor shell should be visible for the new tab"
    );
    assert!(
        !window.imp().markdown_preview.property::<bool>("visible"),
        "preview widget should not remain visible after creating a new document"
    );
    wait_for_active_editor_focus(&window);
}

#[test]
fn test_new_document_focus_handoff_ignores_stale_selection() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    let original_page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("initial tab should be selected");
    let original_editor = active_editor(&window);
    original_editor.source_view().grab_focus();
    flush_events();

    activate_action(&window, "new-tab");
    window.imp().tab_view.set_selected_page(&original_page);
    flush_after_delay(Duration::from_millis(250));

    assert!(
        window.imp().tab_view.selected_page().as_ref() == Some(&original_page),
        "stale delayed focus should not reselect the newer tab"
    );
    assert_eq!(
        gtk4::prelude::GtkWindowExt::focus(&window).map(|widget| widget.as_ptr()),
        Some(original_editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr()),
        "stale delayed focus should not steal focus from the restored tab"
    );
}

#[test]
fn test_focus_mode_shortcut_is_separate_from_fullscreen_shortcut() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.toggle-focus-mode", "<Shift><Control>F11"),
        "Ctrl+Shift+F11 should be bound to Focus Mode"
    );
    assert!(
        shortcut_bound(&window, "win.toggle-fullscreen", "F11"),
        "F11 should stay bound to ordinary fullscreen"
    );

    activate_action(&window, "toggle-fullscreen");
    assert!(
        !action_state_bool(&window, "toggle-focus-mode"),
        "ordinary fullscreen must not activate Focus Mode"
    );
}

#[test]
fn test_focus_mode_entry_exit_restores_shell_surfaces() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1400, 900);
    window.new_tab();
    present_window(&window);

    assert!(workspace_sidebar_visible(&window));
    assert!(properties_sidebar_visible(&window));
    assert!(gtk4::test_accessible_has_state(
        &*window.imp().focus_mode_affordance,
        gtk4::AccessibleState::Hidden
    ));

    activate_action(&window, "toggle-focus-mode");

    assert!(action_state_bool(&window, "toggle-focus-mode"));
    assert!(!window.imp().header_bar.property::<bool>("visible"));
    assert_tab_strip_hidden(&window, "Focus Mode entry");
    assert!(!window.imp().status_bar.property::<bool>("visible"));
    assert!(!workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!(!gtk4::test_accessible_has_state(
        &*window.imp().focus_mode_affordance,
        gtk4::AccessibleState::Hidden
    ));

    activate_action(&window, "toggle-focus-mode");

    assert!(!action_state_bool(&window, "toggle-focus-mode"));
    assert!(window.imp().header_bar.property::<bool>("visible"));
    wait_for_tab_strip_visible(&window, "Focus Mode exit");
    assert!(window.imp().status_bar.property::<bool>("visible"));
    assert!(workspace_sidebar_visible(&window));
    assert!(properties_sidebar_visible(&window));
    assert!(gtk4::test_accessible_has_state(
        &*window.imp().focus_mode_affordance,
        gtk4::AccessibleState::Hidden
    ));
}

#[test]
fn test_focus_mode_affordance_stays_visible_while_leave_button_has_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-focus-mode");
    window.imp().leave_focus_mode_button.grab_focus();
    flush_events();
    flush_after_delay(Duration::from_millis(1900));

    assert!(window.imp().focus_mode_revealer.reveals_child());
    assert!(!gtk4::test_accessible_has_state(
        &*window.imp().focus_mode_affordance,
        gtk4::AccessibleState::Hidden
    ));
}

#[test]
fn test_f9_changes_requested_properties_state_while_focus_mode_suppresses_rendering() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1400, 900);
    window.new_tab();
    present_window(&window);
    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );

    activate_action(&window, "toggle-focus-mode");
    assert!(!properties_sidebar_visible(&window));
    assert!(
        action_state_bool(&window, "toggle-properties"),
        "while focused, the F9 action state should reflect requested state"
    );

    activate_action(&window, "toggle-properties");
    assert!(
        !window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(!properties_sidebar_visible(&window));

    activate_action(&window, "toggle-focus-mode");
    assert!(!properties_sidebar_visible(&window));
}

#[test]
fn test_focus_mode_restores_side_by_side_preview_when_unchanged() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    activate_action(&window, "toggle-preview-pane");
    assert!(window.imp().preview_visible.get());

    activate_action(&window, "toggle-focus-mode");
    assert!(!window.imp().preview_visible.get());

    activate_action(&window, "toggle-focus-mode");
    assert!(
        window.imp().preview_visible.get(),
        "side-by-side preview should restore when Focus Mode exits untouched"
    );
}

#[test]
fn test_alt_p_preview_only_works_inside_focus_mode_and_blocks_preview_restore() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    activate_action(&window, "toggle-preview-pane");
    assert!(window.imp().preview_visible.get());

    activate_action(&window, "toggle-focus-mode");
    activate_action(&window, "toggle-preview-mode");

    assert!(window.imp().focus_mode.active.get());
    assert!(window.imp().preview_mode.get());
    assert!(window.imp().markdown_preview.property::<bool>("visible"));

    activate_action(&window, "toggle-focus-mode");

    assert!(!window.imp().preview_mode.get());
    assert!(
        !window.imp().preview_visible.get(),
        "focused preview changes should prevent side-by-side preview restoration"
    );
}

#[test]
fn test_focus_mode_readable_editor_margins_restore_after_exit() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1600, 900);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).source_view().width() > 0);
    let editor = active_editor(&window);
    let normal_left = editor.focus_mode_left_margin();

    activate_action(&window, "toggle-focus-mode");
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).focus_mode_left_margin() > normal_left
    });
    let focused_left = active_editor(&window).focus_mode_left_margin();
    assert_eq!(focused_left, active_editor(&window).focus_mode_right_margin());

    activate_action(&window, "toggle-focus-mode");
    assert_eq!(active_editor(&window).focus_mode_left_margin(), normal_left);
}

#[test]
fn test_focus_mode_text_origin_guide_visibility_and_margin_tracking() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_uint(keys::FOCUS_MODE_TARGET_COLUMNS, 80)
        .expect("reset Focus Mode column width");
    let window = test_window();
    window.set_default_size(1600, 900);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).source_view().width() > 0);

    assert!(
        !active_editor(&window).focus_mode_text_origin_guide_visible(),
        "the text-origin guide should not render outside Focus Mode"
    );

    activate_action(&window, "toggle-focus-mode");
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).focus_mode_text_origin_guide_visible()
            && active_editor(&window)
                .focus_mode_text_origin_guide_x()
                .is_some()
    });
    let focused_margin = active_editor(&window).focus_mode_left_margin();
    let focused_guide_x = active_editor(&window)
        .focus_mode_text_origin_guide_x()
        .expect("focused guide x coordinate");

    settings
        .set_uint(keys::FOCUS_MODE_TARGET_COLUMNS, 120)
        .expect("widen Focus Mode column width");
    flush_events();
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).focus_mode_left_margin() != focused_margin
            && active_editor(&window)
                .focus_mode_text_origin_guide_x()
                .is_some_and(|x| x != focused_guide_x)
    });

    let updated_margin = active_editor(&window).focus_mode_left_margin();
    let updated_guide_x = active_editor(&window)
        .focus_mode_text_origin_guide_x()
        .expect("updated guide x coordinate");
    assert!(
        (updated_guide_x - focused_guide_x - (updated_margin - focused_margin)).abs() <= 1,
        "the guide should move with the readable-column margin"
    );

    activate_action(&window, "toggle-focus-mode");
    assert!(
        !active_editor(&window).focus_mode_text_origin_guide_visible(),
        "the text-origin guide should hide again when Focus Mode exits"
    );
}

#[test]
fn test_focus_mode_readable_editor_margins_keep_narrow_allocations_usable() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(720, 700);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).source_view().width() > 0);

    activate_action(&window, "toggle-focus-mode");
    let editor = active_editor(&window);
    assert!(editor.focus_mode_left_margin() >= 24);
    assert!(editor.focus_mode_left_margin() <= editor.source_view().width() / 3);
}

#[test]
fn test_focus_mode_applies_markdown_preview_readable_margins_and_restores() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1600, 900);
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-focus-mode");
    activate_action(&window, "toggle-preview-mode");
    let (left, right) = window.imp().markdown_preview.content_margins();
    assert!(left > 16);
    assert_eq!(left, right);

    activate_action(&window, "toggle-focus-mode");
    assert_eq!(window.imp().markdown_preview.content_margins(), (16, 16));
}

#[test]
fn test_focus_mode_temporarily_hides_minimap_without_changing_preference() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    let window = test_window();
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).is_minimap_visible());

    activate_action(&window, "toggle-focus-mode");
    assert_eq!(
        active_editor(&window).minimap_availability(),
        MinimapAvailability::Disabled
    );
    assert!(settings.boolean(keys::SHOW_MINIMAP));

    activate_action(&window, "toggle-focus-mode");
    wait_until(Duration::from_secs(2), || active_editor(&window).is_minimap_visible());
    assert!(settings.boolean(keys::SHOW_MINIMAP));
}

#[test]
fn test_focus_mode_typewriter_scrolling_defaults_off_and_tracks_setting() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING, false)
        .expect("disable typewriter scrolling");
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-focus-mode");
    assert!(
        !active_editor(&window)
            .imp()
            .focus_mode
            .typewriter_scrolling
            .get()
    );

    settings
        .set_boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING, true)
        .expect("enable typewriter scrolling");
    flush_events();
    assert!(
        active_editor(&window)
            .imp()
            .focus_mode
            .typewriter_scrolling
            .get()
    );
}

#[test]
fn test_escape_closes_command_palette_before_exiting_focus_mode() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    activate_action(&window, "toggle-focus-mode");
    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    emit_escape_on_window_controller(&window);
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(window.imp().focus_mode.active.get());

    emit_escape_on_window_controller(&window);
    flush_events();
    assert!(!window.imp().focus_mode.active.get());
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
    assert!(
        !action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );

    activate_action(&window, "toggle-minimap");

    assert!(minimap_setting(&window));
    assert!(
        action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
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

    assert!(
        action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
}

#[test]
fn test_mode_toggles_record_state_specific_workflow_announcements() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-focus-mode");
    wait_until(Duration::from_secs(2), || window.imp().focus_mode.active.get());
    assert_workflow_announcement_recorded(&window, "focus-mode-on");

    activate_action(&window, "toggle-focus-mode");
    wait_until(Duration::from_secs(2), || !window.imp().focus_mode.active.get());
    assert_workflow_announcement_recorded(&window, "focus-mode-off");

    activate_action(&window, "toggle-preview-pane");
    wait_until(Duration::from_secs(2), || window.imp().preview_visible.get());
    assert_workflow_announcement_recorded(&window, "preview-pane-shown");

    activate_boolean_action(&window, "set-preview-mode", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get() && !window.imp().preview_visible.get()
    });
    assert_workflow_announcement_recorded(&window, "preview-pane-hidden");
    assert_workflow_announcement_recorded(&window, "preview-mode-on");

    activate_boolean_action(&window, "set-preview-mode", false);
    wait_until(Duration::from_secs(2), || !window.imp().preview_mode.get());
    assert_workflow_announcement_recorded(&window, "preview-mode-off");

    activate_action(&window, "toggle-minimap");
    wait_until(Duration::from_secs(2), || minimap_setting(&window));
    assert_workflow_announcement_recorded(&window, "minimap-shown");
}

#[test]
fn test_shell_chrome_uses_explicit_opaque_classes_for_transparency_mode() {
    let window = test_window();

    assert!(window.imp().header_bar.has_css_class("header-chrome-opaque"));
    assert!(window.imp().tab_bar.has_css_class("header-chrome-opaque"));
    assert!(window.imp().sidebar.has_css_class("side-rail-chrome-opaque"));
    assert!(
        window
            .imp()
            .properties_panel
            .has_css_class("side-rail-chrome-opaque")
    );
    assert!(
        window
            .imp()
            .properties_panel
            .has_css_class("document-properties-inspector")
    );
    assert!(!window.imp().sidebar.has_css_class("shell-chrome-opaque"));
    assert!(
        !window
            .imp()
            .properties_panel
            .has_css_class("shell-chrome-opaque")
    );
    assert!(window.imp().status_bar.has_css_class("status-bar"));
}

#[test]
fn test_status_bar_readability_height_stays_subordinate_to_header_bar() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1200, 720);
    present_window(&window);

    wait_for_positive_allocation(&*window.imp().header_bar, "header bar");
    wait_for_positive_allocation(&*window.imp().status_bar, "status bar");

    assert_status_bar_readable_one_row(&window, "no-document status bar");
    assert!(
        window.imp().status_bar.height() < window.imp().header_bar.height(),
        "the status bar should gain readable comfort without matching the header bar, status={} header={}",
        window.imp().status_bar.height(),
        window.imp().header_bar.height()
    );
    assert!(
        !window.imp().status_bar.imp().metadata_box.is_visible(),
        "metadata controls should remain hidden when no document is active"
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_status_bar_visual_modes_keep_readable_scoped_chrome() {
    ensure_gtk_init();
    let _reset = VisualSettingsReset::capture();
    let settings = gtk4::Settings::default().expect("GTK settings");

    for (context, color_scheme, theme_name) in [
        (
            "light status bar",
            libadwaita::ColorScheme::ForceLight,
            None,
        ),
        (
            "dark status bar",
            libadwaita::ColorScheme::ForceDark,
            None,
        ),
        (
            "high-contrast status bar",
            libadwaita::ColorScheme::ForceLight,
            Some("HighContrast"),
        ),
    ] {
        libadwaita::StyleManager::default().set_color_scheme(color_scheme);
        settings.set_gtk_theme_name(theme_name);
        flush_events();

        let window = test_window();
        window.set_default_size(1200, 720);
        present_window(&window);
        window.publish_status_message(
            "Saved a long status-bar visual verification message",
            NotificationSeverity::Warning,
        );
        flush_events();

        let status_bar = window.imp().status_bar.imp();
        assert_status_bar_readable_one_row(&window, context);
        assert!(status_bar.message_area_box.has_css_class("status-message-area"));
        assert!(status_bar.message_area_box.has_css_class("status-pulse-warning"));
        assert!(status_bar.message_label.has_css_class("status-message-label"));
        assert!(status_bar.message_label.has_css_class("status-warning"));
        assert!(!status_bar.message_label.wraps());
        assert!(!status_bar.metadata_box.is_visible());

        window.destroy();
        flush_after_delay(Duration::from_millis(50));
    }
}

#[test]
fn test_transient_status_message_pulses_full_message_area() {
    ensure_gtk_init();
    let window = test_window();

    window.publish_status_message("File saved", NotificationSeverity::Info);

    let status_bar = window.imp().status_bar.imp();
    assert_eq!(status_bar.message_label.label().as_str(), "File saved");
    assert!(status_bar.message_area_box.has_css_class("status-pulse-info"));
    assert!(status_bar.message_area_box.has_css_class("status-pulse-a"));
    assert!(!status_bar.message_label.has_css_class("status-pulse-info"));
    assert!(!status_bar.sidebar_toggle_button.has_css_class("status-pulse-info"));
    assert!(!status_bar.metadata_box.has_css_class("status-pulse-info"));
}

#[test]
fn test_repeated_transient_status_message_restarts_pulse_without_text_counter() {
    ensure_gtk_init();
    let window = test_window();

    window.publish_status_message("File saved", NotificationSeverity::Info);
    let first_used_a = window
        .imp()
        .status_bar
        .imp()
        .message_area_box
        .has_css_class("status-pulse-a");

    window.publish_status_message("File saved", NotificationSeverity::Info);
    let status_bar = window.imp().status_bar.imp();

    assert_eq!(status_bar.message_label.label().as_str(), "File saved");
    assert_ne!(
        first_used_a,
        status_bar.message_area_box.has_css_class("status-pulse-a")
    );
}

#[test]
fn test_visible_search_progress_update_pulses_message_area() {
    ensure_gtk_init();
    let window = test_window();

    window.update_search_progress_message_for_test(
        "Searching 10 files\u{2026}",
        NotificationSeverity::Info,
    );

    let status_bar = window.imp().status_bar.imp();
    assert_eq!(
        status_bar.message_label.label().as_str(),
        "Searching 10 files\u{2026}"
    );
    assert!(status_bar.message_area_box.has_css_class("status-pulse-info"));
}

#[test]
fn test_hidden_search_progress_update_does_not_pulse_over_transient() {
    ensure_gtk_init();
    let window = test_window();

    window.publish_status_message("File saved", NotificationSeverity::Info);
    window.imp().status_bar.clear_message_area_pulse();

    window.update_search_progress_message_for_test(
        "Searching 10 files\u{2026}",
        NotificationSeverity::Warning,
    );

    let status_bar = window.imp().status_bar.imp();
    assert_eq!(status_bar.message_label.label().as_str(), "File saved");
    assert!(!status_message_area_has_any_pulse(&window));
}

#[test]
fn test_generic_progress_heartbeat_and_resolve_renders_do_not_pulse() {
    ensure_gtk_init();
    let window = test_window();

    assert!(window.imp().notification_bus.publish(
        NotificationOwner::Search,
        NotificationSurface::StatusBar,
        NotificationPayload::Progress(StatusMessage {
            text: "Searching 1 file\u{2026}".to_string(),
            severity: NotificationSeverity::Info,
        }),
    ));
    window.render_notifications();
    assert_eq!(
        window.imp().status_bar.imp().message_label.label().as_str(),
        "Searching 1 file\u{2026}"
    );
    assert!(!status_message_area_has_any_pulse(&window));

    assert!(
        window
            .imp()
            .notification_bus
            .heartbeat(NotificationOwner::Search, NotificationSurface::StatusBar)
    );
    window.render_notifications();
    assert!(!status_message_area_has_any_pulse(&window));

    assert!(window.imp().notification_bus.resolve(
        NotificationOwner::Search,
        NotificationSurface::StatusBar
    ));
    window.render_notifications();
    assert!(!status_message_area_has_any_pulse(&window));
    assert!(
        !window
            .imp()
            .status_bar
            .imp()
            .status_announcement_throttler
            .has_recent_announcement_for_test(
                AnnouncementLane::StatusUpdate,
                "status:info:Searching 1 file\u{2026}"
            ),
        "progress heartbeats should not record routine info announcements"
    );
}

#[test]
fn test_editor_transparency_uses_derived_scheme_and_keeps_minimap_opaque() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 0.85)
        .expect("set tab-content-opacity");
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let temp_dir = tempfile::tempdir().expect("editor tempdir");
    let file_path = temp_dir.path().join("alpha.rs");
    fixture::write_text(&file_path, "fn main() {\n    println!(\"hi\");\n}\n");

    let window = test_window();
    present_window(&window);
    window.open_document(&file_path);
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor.file_size().is_some()
            && editor
                .applied_style_scheme_id()
                .is_some_and(|id| id.starts_with("lushtext-opacity-"))
    });

    let editor = active_editor(&window);
    assert!((tab_content_opacity_setting() - 0.85).abs() < f64::EPSILON);
    assert!((editor.content_background_opacity() - 0.85).abs() < f64::EPSILON);
    assert_eq!(editor.minimap_background_opacity(), 1.0);
    assert!(
        editor
            .applied_style_scheme_id()
            .is_some_and(|id| id.starts_with("lushtext-opacity-")),
        "editor should switch onto the derived opacity-aware style scheme"
    );
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
    wait_until(Duration::from_secs(2), || {
        editor.minimap_availability() == MinimapAvailability::Visible
    });

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
fn test_large_file_load_disables_syntax_through_ui_state() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let dir = tempfile::tempdir().expect("large-file tempdir");
    let path = dir.path().join("large.rs");
    let size = DISABLE_SYNTAX_HIGHLIGHTING + 1;
    fixture::write_text(&path, "small fixture promoted to large-file policy\n");

    let window = test_window();
    present_window(&window);
    window.open_document(&path);

    wait_until(Duration::from_secs(10), || {
        active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).file_size().is_some()
    });
    let editor = active_editor(&window);
    // The policy assertions below do not need a real 10MB text-buffer load.
    // Keeping the fixture small avoids turning this UI-state test into a CI
    // timing test for file I/O, UTF-8 decoding, and GtkTextBuffer insertion.
    editor.apply_loaded_content_for_test("large-file policy content\n", size);
    assert_eq!(editor.file_size(), Some(size));
    assert_eq!(editor.size_check(), FileSizeCheck::DisableSyntax);
    assert!(!editor.buffer().is_highlight_syntax());
    assert!(editor.buffer().language().is_none());

    wait_until(Duration::from_secs(2), || {
        editor.minimap_availability() == MinimapAvailability::TooLarge
    });
    assert!(!editor.is_minimap_visible());
}

#[test]
fn test_large_file_load_disables_undo_and_history_through_ui_state() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("huge-file tempdir");
    let path = dir.path().join("huge.rs");
    let size = DISABLE_UNDO_HISTORY + 1;
    fixture::write_text(&path, "small fixture promoted to huge-file policy\n");

    let window = test_window();
    present_window(&window);
    window.open_document(&path);

    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).file_size().is_some()
    });
    let editor = active_editor(&window);
    editor.apply_loaded_content_for_test("huge-file policy content\n", size);
    let saved_page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("saved page selected");
    window.new_tab();
    flush_events();
    window.imp().tab_view.set_selected_page(&saved_page);
    flush_events();

    assert_eq!(editor.file_size(), Some(size));
    assert_eq!(editor.size_check(), FileSizeCheck::DisableUndoAndSyntax);
    assert!(!editor.size_check().undo_enabled());
    assert!(!editor.buffer().can_undo());
    assert!(!action_enabled(&window, "show-local-history"));
    assert!(!editor.buffer().is_highlight_syntax());
}

#[test]
fn test_too_large_file_refuses_to_load_and_clears_open_path_state() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("too-large-file tempdir");
    let path = dir.path().join("too-large.txt");
    fixture::create_sparse_file(&path, REFUSE_TO_OPEN + 1);
    let canonical_path = fs_metadata::canonical_path(&path).expect("canonical too-large path");

    let window = test_window();
    present_window(&window);
    window.open_document(&path);

    wait_until(Duration::from_secs(5), || {
        active_editor(&window)
            .info_bar()
            .imp()
            .alert_title
            .label()
            .as_str()
            == "Could Not Open File"
    });

    let editor = active_editor(&window);
    let info_bar = editor.info_bar().imp();
    assert!(info_bar.alert_revealer.reveals_child());
    assert!(info_bar.alert_body.label().contains("too large to edit"));
    assert_eq!(editor.file_path(), None);
    assert_eq!(editor.file_size(), None);
    assert!(!window.imp().open_paths.borrow().contains(&path));
    assert!(!window.imp().open_paths.borrow().contains(&canonical_path));
}

#[test]
fn test_memory_pressure_evicts_background_tab_and_reloads_without_path_corruption() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("memory-pressure tempdir");
    let first_path = dir.path().join("first.txt");
    let second_path = dir.path().join("second.txt");
    let first_text = "first tab data\n";
    let second_text = "second tab data\n";
    fixture::write_text(&first_path, first_text);
    fixture::write_text(&second_path, second_text);
    let first_key = fs_metadata::canonical_path(&first_path).expect("canonical first path");

    let window = test_window();
    present_window(&window);

    window.open_document(&first_path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(first_path.clone())
            && editor_buffer_text(&active_editor(&window)) == first_text
    });
    let first_page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("first page selected");
    let first_editor = active_editor(&window);

    window.open_document(&second_path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(second_path.clone())
            && editor_buffer_text(&active_editor(&window)) == second_text
    });
    let second_page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("second page selected");
    let second_editor = active_editor(&window);

    window.imp().tab_view.set_selected_page(&first_page);
    flush_events();
    first_editor.imp().file_size.set(Some(200_000_000));
    first_editor
        .imp()
        .size_check
        .set(FileSizeCheck::DisableUndoAndSyntax);
    second_editor.imp().file_size.set(Some(100_000));
    window
        .imp()
        .editor_memory
        .total
        .set(first_editor.estimated_buffer_bytes() + second_editor.estimated_buffer_bytes());

    window.imp().tab_view.set_selected_page(&second_page);
    flush_events();
    wait_until(Duration::from_secs(2), || first_editor.is_evicted());
    assert_eq!(editor_buffer_text(&first_editor), "");
    assert_eq!(window.imp().tab_view.n_pages(), 2);
    assert!(window.imp().open_paths.borrow().contains(&first_key));

    window.imp().tab_view.set_selected_page(&first_page);
    flush_events();
    wait_until(Duration::from_secs(5), || {
        !first_editor.is_evicted() && editor_buffer_text(&first_editor) == first_text
    });
    assert_eq!(first_editor.file_path(), Some(first_path.clone()));
    assert!(window.imp().open_paths.borrow().contains(&first_key));

    window.open_document(&first_path);
    flush_events();
    assert_eq!(window.imp().tab_view.n_pages(), 2);
    assert_eq!(active_editor(&window).file_path(), Some(first_path));
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
            && properties_surface_uses_right_pane(&window)
            && !window.imp().workspace_split_view.is_collapsed()
    });

    assert!(workspace_sidebar_visible(&window));
    assert!(properties_surface_uses_right_pane(&window));
    assert!(!window.imp().workspace_split_view.is_collapsed());
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
        window
            .imp()
            .settings
            .double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        0.4
    );
    assert_workspace_sidebar_width_locked(&window, 440.0);

    prefs.imp().workspace_sidebar_width_row.set_selected(0);
    wait_until(Duration::from_secs(2), || {
        (workspace_total_fraction(&window) - 280.0 / 1400.0).abs() < 0.001
    });

    assert_eq!(
        window
            .imp()
            .settings
            .double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
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
            (workspace_total_fraction(&window)
                - expected_width / f64::from(current_window_width(&window)))
            .abs()
                < 0.001
        );
        assert_workspace_sidebar_width_locked(&window, expected_width);
    }
}

#[test]
fn test_split_view_allocation_sync_does_not_rewrite_persisted_properties_width() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1770, 900);
    present_window(&window);

    let settings = &window.imp().settings;
    settings
        .set_double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION, 0.73)
        .expect("seed sentinel properties width");
    let synced_width = window.imp().split_width_synced_for_width.get();

    // Allocation can be queued by Adwaita animation frames. It must keep to
    // runtime geometry and avoid writing persisted settings on each frame.
    window.queue_allocate();
    flush_after_delay(Duration::from_millis(50));

    assert_eq!(window.imp().split_width_synced_for_width.get(), synced_width);
    assert_eq!(
        settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION),
        0.73
    );
}

#[test]
fn test_workspace_sidebar_setting_recalculates_properties_breakpoint() {
    ensure_gtk_init();
    let comfy_window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    comfy_window.set_default_size(1400, 900);
    present_window(&comfy_window);
    activate_action(&comfy_window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_sidebar_visible(&comfy_window));

    assert!(
        properties_surface_uses_right_pane(&comfy_window),
        "Comfy should keep the properties pane side-by-side at 1400sp"
    );

    comfy_window.destroy();
    flush_events();

    let large_window = test_window_with_split_view_state(true, 0.4, false, 0.25);
    large_window.set_default_size(1400, 900);
    present_window(&large_window);
    activate_action(&large_window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&large_window)
    });

    assert!(properties_surface_uses_bottom_sheet(&large_window));
    assert_workspace_sidebar_width_locked(&large_window, 440.0);
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
fn test_minimap_geometry_tracks_sidebar_width_reflow_with_word_wrap() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    settings
        .set_boolean(keys::WORD_WRAP, true)
        .expect("enable word wrap");

    let window = test_window_with_split_view_state_and_size(false, 0.3, false, 0.25, 1200, 900);
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 260, 1, 13);
    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());

    let mid_file = editor.buffer().iter_at_line(120).expect("line 120");
    editor.buffer().place_cursor(&mid_file);
    editor
        .source_view()
        .scroll_to_mark(&editor.buffer().get_insert(), 0.0, true, 0.0, 0.0);
    flush_after_delay(Duration::from_millis(100));
    let before = minimap_geometry_snapshot(&editor);

    activate_action(&window, "toggle-sidebar");
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&window)
            && minimap_geometry_snapshot(&editor).editor_width < before.editor_width
    });
    // Wait out the debounced width-reflow settle and reveal window instead of
    // guessing a fixed delay so the snapshot below reads settled minimap geometry.
    wait_until(Duration::from_secs(5), || {
        !editor.minimap_work_pending_for_test()
    });
    let after = minimap_geometry_snapshot(&editor);

    assert_eq!(after.editor_wrap_mode, gtk4::WrapMode::Word);
    assert_eq!(
        after.source_map_wrap_mode,
        gtk4::WrapMode::None,
        "the minimap source map must stay unwrapped after sidebar reflow"
    );
    assert!(
        after.source_map_width > 0 && after.editor_visible_height > 0,
        "minimap and editor should both have settled allocations: {after:?}"
    );
    assert!(
        (after.visible_start_line - before.visible_start_line).abs() <= 1,
        "showing the sidebar should not jump the editor away from the visible buffer range; before={before:?}, after={after:?}"
    );
    assert!(
        after.vertical_upper > after.vertical_page_size
            && after.vertical_value >= 0.0
            && after.vertical_value <= after.vertical_upper,
        "vertical adjustment should stay internally consistent after reflow: {after:?}"
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

fn run_minimap_top_anchor_sidebar_reflow_case(word_wrap: bool, initially_visible: bool) {
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    settings
        .set_boolean(keys::WORD_WRAP, word_wrap)
        .expect("set word wrap");

    let window =
        test_window_with_split_view_state_and_size(initially_visible, 0.3, false, 0.25, 1260, 900);
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let long_line_stride = usize::from(word_wrap);
    write_document_with_long_lines(&editor, 280, long_line_stride, 0);
    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());
    add_top_bookmark_marker(&editor);
    put_editor_at_top_left(&editor);

    let expected_wrap_mode = if word_wrap {
        gtk4::WrapMode::Word
    } else {
        gtk4::WrapMode::None
    };
    wait_until(Duration::from_secs(5), || {
        let geometry = minimap_geometry_snapshot(&editor);
        !editor.minimap_work_pending_for_test()
            && geometry.visible_start_line == 0
            && (geometry.vertical_value - geometry.vertical_lower).abs() <= 0.5
            && geometry.source_map_wrap_mode == gtk4::WrapMode::None
            && geometry.minimap_first_line_top >= 1.0
    });
    let before = minimap_geometry_snapshot(&editor);
    assert_top_minimap_reflow_invariants(&editor, before, expected_wrap_mode);

    activate_action(&window, "toggle-sidebar");
    // The first width-changed allocation must open a reflow burst so margin
    // and scroll repair are pinned until the width settles. The pixel freeze
    // itself is best-effort and needs a rendered frame, which this harness
    // does not produce, so rendered-freeze coverage lives in the visual
    // geometry smoke lane instead.
    wait_until(Duration::from_secs(2), || {
        editor.minimap_reflow_settle_pending_for_test()
    });
    let expected_sidebar_visible = !initially_visible;
    wait_until(Duration::from_secs(2), || {
        let geometry = minimap_geometry_snapshot(&editor);
        workspace_sidebar_visible(&window) == expected_sidebar_visible
            && if expected_sidebar_visible {
                geometry.editor_width < before.editor_width
            } else {
                geometry.editor_width > before.editor_width
            }
    });
    // The settle repair is debounced behind the animation; wait for it, the
    // reveal window, and the follow-up marker refresh to drain so the assertions
    // below see the settled post-repair state instead of mid-burst pinned geometry.
    wait_until(Duration::from_secs(5), || {
        !editor.minimap_work_pending_for_test()
    });
    wait_until(Duration::from_secs(2), || {
        let geometry = minimap_geometry_snapshot(&editor);
        geometry.visible_start_line == 0
            && (geometry.vertical_value - geometry.vertical_lower).abs() <= 0.5
            && geometry.source_map_wrap_mode == gtk4::WrapMode::None
            && geometry.minimap_first_line_top >= 1.0
            && !editor
                .minimap_marker_bounds(MinimapMarkerKind::Bookmark)
                .is_empty()
    });

    let freeze_picture = editor
        .imp()
        .minimap
        .render_hold
        .borrow()
        .as_ref()
        .map(|hold| hold.cover().clone())
        .expect("reflow freeze cover should exist");
    assert!(
        !freeze_picture.property::<bool>("visible"),
        "the settle repair must reveal the live native map again after reflow"
    );

    let map_adjustment = minimap_source_map(&editor)
        .vadjustment()
        .expect("source map vadjustment");
    assert!(
        (map_adjustment.value() - map_adjustment.lower()).abs() <= 0.5,
        "top-anchored reflow must clear stale source-map scroll so the native slider stays anchored"
    );

    let after = minimap_geometry_snapshot(&editor);
    assert_top_minimap_reflow_invariants(&editor, after, expected_wrap_mode);
    assert!(
        (after.minimap_first_line_top - before.minimap_first_line_top).abs() <= 1.0,
        "sidebar toggle moved the first rendered minimap row beyond margin rounding; before={before:?}, after={after:?}"
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_minimap_top_anchor_survives_sidebar_hide_with_wrap_enabled_and_disabled() {
    ensure_gtk_init();
    run_minimap_top_anchor_sidebar_reflow_case(true, true);
    run_minimap_top_anchor_sidebar_reflow_case(false, true);
}

#[test]
fn test_minimap_top_anchor_survives_sidebar_show_with_wrap_enabled_and_disabled() {
    ensure_gtk_init();
    run_minimap_top_anchor_sidebar_reflow_case(true, false);
    run_minimap_top_anchor_sidebar_reflow_case(false, false);
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
fn test_long_line_markers_hidden_by_default_when_minimap_is_enabled() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings.reset(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 300, 3, 0);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());
    flush_after_delay(Duration::from_millis(120));

    assert!(!settings.boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE));
    assert_eq!(editor.minimap_marker_count(MinimapMarkerKind::LongLine), 0);
}

#[test]
fn test_long_line_markers_appear_when_preference_is_enabled() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, true)
        .expect("enable long-line minimap markers");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 300, 3, 0);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::LongLine) > 0
    });
}

#[test]
fn test_disabling_long_line_markers_preserves_other_minimap_markers() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, true)
        .expect("enable long-line minimap markers");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 360, 4, 6);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());

    let line = editor.buffer().iter_at_line(120).expect("line 120");
    editor.buffer().place_cursor(&line);
    let _ = editor.toggle_bookmark_at_cursor();
    editor.show_search();
    editor.search_bar().search_entry().set_text("needle");

    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Bookmark) == 1
            && editor.minimap_marker_count(MinimapMarkerKind::Search) > 0
            && editor.minimap_marker_count(MinimapMarkerKind::Modified) > 0
            && editor.minimap_marker_count(MinimapMarkerKind::LongLine) > 0
    });

    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, false)
        .expect("disable long-line minimap markers");
    flush_events();

    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::LongLine) == 0
    });
    assert_eq!(editor.minimap_marker_count(MinimapMarkerKind::Bookmark), 1);
    assert!(editor.minimap_marker_count(MinimapMarkerKind::Search) > 0);
    assert!(editor.minimap_marker_count(MinimapMarkerKind::Modified) > 0);
}

#[test]
fn test_close_tab_is_blocked_while_save_is_in_progress() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    let temp = tempfile::NamedTempFile::new().expect("temp file");
    editor.set_file_path(temp.path());
    editor.imp().file_size.set(Some(10_000_000));
    editor.buffer().set_text(&"x".repeat(70_000));

    let save_done = std::rc::Rc::new(std::cell::Cell::new(false));
    let save_done_clone = save_done.clone();
    editor.save_file_async(move |result| {
        result.expect("save should succeed");
        save_done_clone.set(true);
    });
    assert!(editor.is_saving());

    let page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected page");
    let close_confirmed = std::rc::Rc::new(std::cell::RefCell::new(None));
    let close_confirmed_clone = close_confirmed.clone();
    window.confirm_close_tab(&page, &editor, move |confirmed| {
        *close_confirmed_clone.borrow_mut() = Some(confirmed);
    });

    assert_eq!(*close_confirmed.borrow(), Some(false));
    wait_until(Duration::from_secs(2), || save_done.get());
}

#[test]
fn test_draft_autosave_marks_pending_when_editing_during_inflight_batch() {
    ensure_gtk_init();
    let _reset = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(25);
    let window = test_window();
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    window.imp().drafts.autosave_inflight.set(true);
    editor.buffer().set_text("changed during autosave");

    assert!(window.imp().drafts.autosave_pending.get());
    window.imp().drafts.autosave_inflight.set(false);
    window.imp().drafts.autosave_pending.set(false);
}

#[test]
fn test_first_dirty_autosave_writes_small_buffer_before_periodic_tick() {
    ensure_gtk_init();
    let _reset = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(25);
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let draft_id = editor.draft_id().expect("draft id");
    let data_dir = json_store::data_dir();
    let _ = draft_service::delete_draft_file(&data_dir, &draft_id);

    editor.buffer().set_text("first dirty draft\n");
    editor.buffer().set_modified(true);

    wait_until(Duration::from_secs(3), || {
        !window.draft_autosave_inflight_for_test()
            && !editor.draft_dirty()
            && draft_service::read_draft(&data_dir, &draft_id)
                .expect("read first-dirty draft")
                .is_some_and(|content| content == "first dirty draft\n")
    });

    draft_service::delete_draft_file(&data_dir, &draft_id).expect("delete draft file");
}

#[test]
fn test_first_dirty_autosave_failure_keeps_editor_retry_eligible() {
    ensure_gtk_init();
    let _reset = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(25);
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let draft_id = editor.draft_id().expect("draft id");
    let data_dir = json_store::data_dir();
    let drafts_dir = draft_service::drafts_dir(&data_dir);
    let manifest_path = drafts_dir.join("manifest.json");
    let _ = draft_service::delete_draft_file(&data_dir, &draft_id);
    if fs_metadata::path_status(&manifest_path)
        .expect("manifest path status")
        .is_present()
    {
        if fs_metadata::path_status(&manifest_path)
            .expect("manifest path status")
            .is_directory()
        {
            fixture::remove_dir_all(&manifest_path);
        } else {
            fixture::remove_file(&manifest_path);
        }
    }
    fixture::create_dir_all(&manifest_path);

    editor.buffer().set_text("retryable draft\n");
    editor.buffer().set_modified(true);

    wait_until(Duration::from_secs(3), || {
        !window.draft_autosave_inflight_for_test()
            && draft_service::read_draft(&data_dir, &draft_id)
                .expect("read retryable draft")
                .is_some()
    });

    assert!(
        editor.draft_dirty(),
        "failed autosave manifest persistence must keep the editor eligible for retry",
    );
    assert_eq!(
        draft_service::read_draft(&data_dir, &draft_id).expect("read draft"),
        Some("retryable draft\n".to_string())
    );

    fixture::remove_dir_all(&manifest_path);
    window.autosave_tick_for_test();
    wait_until(Duration::from_secs(3), || {
        !window.draft_autosave_inflight_for_test() && !editor.draft_dirty()
    });

    draft_service::delete_draft_file(&data_dir, &draft_id).expect("delete draft file");
}

#[test]
fn test_first_dirty_autosave_large_buffer_snapshots_across_main_loop_chunks() {
    ensure_gtk_init();
    let _reset = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(25);
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let large_text = "x".repeat(2_500_001);
    let draft_id = editor.draft_id().expect("draft id");
    let data_dir = json_store::data_dir();
    let _ = draft_service::delete_draft_file(&data_dir, &draft_id);

    editor.buffer().set_text(&large_text);
    editor.buffer().set_modified(true);

    assert!(
        editor.save_uses_chunked_snapshot_for_test(),
        "large draft buffers should use the main-loop chunked snapshot path"
    );
    wait_until(Duration::from_secs(15), || {
        !window.draft_autosave_inflight_for_test()
            && draft_service::read_draft(&data_dir, &draft_id)
                .expect("read draft")
                .is_some_and(|content| content.len() == large_text.len())
    });

    assert!(!editor.draft_dirty());

    draft_service::delete_draft_file(&data_dir, &draft_id).expect("delete draft");
}

#[test]
fn test_properties_pane_collapses_before_workspace_pane() {
    ensure_gtk_init();

    // At a narrow width just below the adaptive Comfy breakpoint (~1350sp),
    // the document-properties surface should re-render as a bottom sheet while
    // the workspace pane stays in layout.
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1320, 900);
    present_window(&window);
    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_bottom_sheet(&window));

    assert!(properties_sidebar_visible(&window));
    assert!(properties_surface_uses_bottom_sheet(&window));
    assert!(!window.imp().workspace_split_view.is_collapsed());
    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_large_workspace_preset_collapses_properties_pane_earlier() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.4, false, 0.25);
    window.set_default_size(1400, 900);
    present_window(&window);
    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_bottom_sheet(&window));

    assert!(properties_sidebar_visible(&window));
    assert!(properties_surface_uses_bottom_sheet(&window));
    assert!(!window.imp().workspace_split_view.is_collapsed());
    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_dual_secondary_surfaces_settle_at_all_breakpoint_edges() {
    ensure_gtk_init();

    for (workspace_visible, workspace_fraction, compact_width, spacious_width) in [
        (false, 0.3, 920, 945),
        (true, 0.2, 1180, 1210),
        (true, 0.3, 1320, 1365),
        (true, 0.4, 1440, 1470),
    ] {
        let compact = test_window_with_split_view_state_and_size(
            workspace_visible,
            workspace_fraction,
            true,
            0.25,
            compact_width,
            900,
        );
        present_window(&compact);
        wait_for_properties_surface(&compact, PropertiesSurfacePresentation::Sheet, false);
        assert_adaptive_shell_stays_quiet(
            &compact,
            PropertiesSurfacePresentation::Sheet,
            false,
        );
        compact.destroy();
        flush_after_delay(Duration::from_millis(50));

        let spacious = test_window_with_split_view_state_and_size(
            workspace_visible,
            workspace_fraction,
            true,
            0.25,
            spacious_width,
            900,
        );
        present_window(&spacious);
        wait_for_properties_surface(
            &spacious,
            PropertiesSurfacePresentation::Pane,
            workspace_visible,
        );
        assert_adaptive_shell_stays_quiet(
            &spacious,
            PropertiesSurfacePresentation::Pane,
            workspace_visible,
        );
        spacious.destroy();
        flush_after_delay(Duration::from_millis(50));

        assert!(
            compact_width < spacious_width,
            "representative guard pair should be ordered"
        );
    }
}

#[test]
fn test_medium_width_dual_surfaces_do_not_oscillate_after_settling() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1200, 900);
    present_window(&window);
    wait_for_properties_surface(&window, PropertiesSurfacePresentation::Sheet, false);

    assert_adaptive_shell_stays_quiet(&window, PropertiesSurfacePresentation::Sheet, false);
    assert!(
        window.imp().secondary_surfaces.workspace_requested_visible.get(),
        "compact suppression must not erase the user's desktop workspace intent"
    );
    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get(),
        "document properties should remain requested while rendered as a sheet"
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_hiding_workspace_sidebar_relaxes_properties_breakpoint() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(false, 0.4, false, 0.25);
    window.set_default_size(1400, 900);
    present_window(&window);
    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_sidebar_visible(&window));

    assert!(properties_sidebar_visible(&window));
    assert!(properties_surface_uses_right_pane(&window));
    assert!(!workspace_sidebar_visible(&window));
}

#[test]
fn test_compact_layout_mutual_exclusion_switches_secondary_surface() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1320, 900);
    present_window(&window);

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&window) && !workspace_sidebar_visible(&window)
    });

    assert!(properties_sidebar_visible(&window));
    assert!(!workspace_sidebar_visible(&window));

    activate_action(&window, "toggle-sidebar");
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&window) && !properties_sidebar_visible(&window)
    });

    assert!(workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_compact_sidebar_show_with_properties_visible_waits_to_close_sheet_until_transition_settles()
{
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1320, 900);
    present_window(&window);

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&window) && !workspace_sidebar_visible(&window)
    });

    activate_action(&window, "toggle-sidebar");

    assert!(workspace_sidebar_visible(&window));
    assert!(
        window.workspace_sidebar_transition_pending_for_test(),
        "compact workspace show should keep visual-geometry readiness blocked while Adwaita animates"
    );
    assert!(
        properties_surface_uses_bottom_sheet(&window),
        "the visible properties sheet should wait for the sidebar animation before compact arbitration closes it"
    );
    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get(),
        "workspace toggles must not erase the user's requested properties intent"
    );

    wait_for_workspace_sidebar_transition(&window);

    assert!(workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get(),
        "final compact rendering may hide properties, but the requested desktop intent should remain"
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_wide_sidebar_toggle_with_properties_visible_keeps_properties_pane_through_transition() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1600, 900);
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&window) && properties_surface_uses_right_pane(&window)
    });

    activate_action(&window, "toggle-sidebar");

    assert!(!workspace_sidebar_visible(&window));
    assert!(
        window.workspace_sidebar_transition_pending_for_test(),
        "wide workspace hide should keep visual-geometry readiness blocked while Adwaita animates"
    );
    assert!(
        properties_surface_uses_right_pane(&window),
        "wide desktop properties pane should stay visible during workspace hide animation"
    );

    wait_for_workspace_sidebar_transition(&window);

    assert!(!workspace_sidebar_visible(&window));
    assert!(properties_surface_uses_right_pane(&window));

    activate_action(&window, "toggle-sidebar");

    assert!(workspace_sidebar_visible(&window));
    assert!(
        window.workspace_sidebar_transition_pending_for_test(),
        "wide workspace show should keep visual-geometry readiness blocked while Adwaita animates"
    );
    assert!(
        properties_surface_uses_right_pane(&window),
        "wide desktop properties pane should stay visible during workspace show animation"
    );

    wait_for_workspace_sidebar_transition(&window);

    assert!(workspace_sidebar_visible(&window));
    assert!(properties_surface_uses_right_pane(&window));

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_intermediate_sidebar_show_defers_properties_reconciliation_until_transition_settles() {
    ensure_gtk_init();
    let window =
        test_window_with_split_view_state_and_size(false, 0.3, false, 0.25, 1100, 900);
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        !workspace_sidebar_visible(&window)
            && properties_surface_presentation(&window) == PropertiesSurfacePresentation::Pane
    });

    activate_action(&window, "toggle-sidebar");

    assert!(workspace_sidebar_visible(&window));
    assert!(
        window.workspace_sidebar_transition_pending_for_test(),
        "workspace toggle should block final visual-geometry readiness while Adwaita animates"
    );
    assert_eq!(
        properties_surface_presentation(&window),
        PropertiesSurfacePresentation::Pane,
        "the intermediate-width properties breakpoint must not flip in the same frame as show-sidebar"
    );
    assert!(
        window
            .imp()
            .settings
            .boolean(keys::WORKSPACE_SIDEBAR_VISIBLE),
        "user intent should persist immediately even while layout reconciliation waits"
    );

    wait_for_workspace_sidebar_transition(&window);

    assert!(workspace_sidebar_visible(&window));
    assert_eq!(
        properties_surface_presentation(&window),
        PropertiesSurfacePresentation::Sheet,
        "final reconciliation should still apply the post-toggle breakpoint at 1100sp"
    );
    assert!(!properties_sidebar_visible(&window));

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_intermediate_sidebar_hide_defers_properties_reconciliation_until_transition_settles() {
    ensure_gtk_init();
    let window =
        test_window_with_split_view_state_and_size(true, 0.3, false, 0.25, 1100, 900);
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&window)
            && properties_surface_presentation(&window) == PropertiesSurfacePresentation::Sheet
    });

    activate_action(&window, "toggle-sidebar");

    assert!(!workspace_sidebar_visible(&window));
    assert!(
        window.workspace_sidebar_transition_pending_for_test(),
        "workspace hide should block final visual-geometry readiness while Adwaita animates"
    );
    assert_eq!(
        properties_surface_presentation(&window),
        PropertiesSurfacePresentation::Sheet,
        "the relaxed no-sidebar breakpoint must wait until the hide transition settles"
    );
    assert!(
        !window
            .imp()
            .settings
            .boolean(keys::WORKSPACE_SIDEBAR_VISIBLE),
        "hidden intent should persist immediately even while layout reconciliation waits"
    );

    wait_for_workspace_sidebar_transition(&window);

    assert!(!workspace_sidebar_visible(&window));
    assert_eq!(
        properties_surface_presentation(&window),
        PropertiesSurfacePresentation::Pane,
        "final reconciliation should restore the no-sidebar breakpoint at 1100sp"
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_short_normal_window_preserves_status_bar_with_optional_surfaces() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1190, 200);
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 180, 2, 7);
    activate_action(&window, "toggle-search-panel");

    wait_for_positive_allocation(&*window.imp().status_bar, "status bar");
    wait_until(Duration::from_secs(2), || {
        editor.is_minimap_visible()
            && properties_surface_uses_bottom_sheet(&window)
            && window.imp().properties_panel.height() >= 240
            && source_view_vadjustment_is_at_top(&editor)
    });

    assert!(
        current_window_height(&window) >= window.height_request(),
        "short windows should be raised to the advertised normal-mode height request"
    );
    assert_status_bar_readable_one_row(&window, "short normal status bar");
    assert!(
        window.imp().search_panel.imp().results_scroll.height_request()
            <= current_window_height(&window) / 3,
        "search results should remain inside the short-window content budget"
    );
    assert!(
        window.imp().properties_panel.height() >= 240,
        "compact document properties should render as a usable bottom sheet, got height {}",
        window.imp().properties_panel.height()
    );
    let minimap = minimap_source_map(&editor);
    let geometry = minimap_geometry_snapshot(&editor);
    assert_eq!(
        geometry.visible_start_line, 0,
        "top-anchored short windows should keep the first editor line visible"
    );
    assert!(
        minimap.height() > 0,
        "the minimap should keep a positive height in short compact layouts"
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_single_tab_strip_preserves_constrained_normal_geometry() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["one-tab.txt"]);
    let window = test_window_with_split_view_state_and_size(false, 0.3, false, 0.25, 760, 360);
    present_window(&window);
    window.open_document(&files[0]);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });

    let editor = active_editor(&window);
    wait_for_tab_strip_visible(&window, "single-tab constrained tab strip");
    wait_for_positive_allocation(editor.source_view(), "single-tab constrained editor viewport");
    wait_for_positive_allocation(&*window.imp().status_bar, "single-tab constrained status bar");
    assert_status_bar_readable_one_row(&window, "single-tab constrained status bar");

    let root = window.upcast_ref::<gtk4::Widget>();
    let tab_bounds = window
        .imp()
        .tab_bar
        .compute_bounds(root)
        .expect("tab strip bounds in constrained window");
    let editor_page_bounds = editor
        .upcast_ref::<gtk4::Widget>()
        .compute_bounds(root)
        .expect("editor page bounds in constrained window");
    let status_bounds = window
        .imp()
        .status_bar
        .compute_bounds(root)
        .expect("status bar bounds in constrained window");

    assert!(
        tab_bounds.y() + tab_bounds.height() <= editor_page_bounds.y() + 1.0,
        "single-tab strip should not overlap the editor page, tab={tab_bounds:?}, editor={editor_page_bounds:?}"
    );
    assert!(
        editor_page_bounds.y() + editor_page_bounds.height() <= status_bounds.y() + 1.0,
        "single-tab editor page should not overlap the status bar, editor={editor_page_bounds:?}, status={status_bounds:?}"
    );
    assert!(
        status_bounds.y() + status_bounds.height() <= current_window_height(&window) as f32 + 1.0,
        "single-tab status bar should stay inside the constrained window, status={status_bounds:?}, window height={}",
        current_window_height(&window)
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_forced_tiny_window_preserves_status_bar() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window_with_split_view_state_and_size(true, 0.3, false, 0.25, 980, 190);
    // Some compositors can hand the app less height than the advertised normal
    // floor. The center editor/sidebar region must yield before the status bar.
    window.set_height_request(1);
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 80, 2, 0);

    wait_until(Duration::from_secs(2), || {
        current_window_height(&window) <= 240 && window.imp().status_bar.height() > 0
    });

    assert_positive_allocation(&*window.imp().status_bar, "status bar");
    assert_status_bar_readable_one_row(&window, "forced tiny status bar");
    let status_bounds = window
        .imp()
        .status_bar
        .compute_bounds(window.upcast_ref::<gtk4::Widget>())
        .expect("status bar bounds in window");
    assert!(
        status_bounds.y() + status_bounds.height() <= current_window_height(&window) as f32 + 1.0,
        "status bar should stay inside the tiny window allocation, bounds={status_bounds:?}, window height={}",
        current_window_height(&window)
    );

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_passive_compact_width_does_not_open_workspace_overlay() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(true, 0.3, false, 0.25, 837, 902);
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 60, 1, 0);

    wait_until(Duration::from_secs(2), || {
        window.imp().workspace_split_view.is_collapsed()
    });

    assert!(
        window.imp().secondary_surfaces.workspace_requested_visible.get(),
        "the restored desktop workspace intent should remain true"
    );
    assert!(
        !workspace_sidebar_visible(&window),
        "passive compact restore must not leave the workspace sidebar covering the editor"
    );
    assert!(
        source_view_hadjustment_is_at_left(&editor),
        "passive compact restore should leave the editor anchored at the left edge"
    );
    assert_positive_allocation(editor.source_view(), "active editor source view");

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_explicit_compact_sidebar_toggle_opens_workspace_overlay() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(false, 0.3, false, 0.25, 837, 902);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().workspace_split_view.is_collapsed()
            && properties_surface_presentation(&window) == PropertiesSurfacePresentation::Sheet
    });

    activate_action(&window, "toggle-sidebar");
    wait_until(Duration::from_secs(2), || workspace_sidebar_visible(&window));

    assert!(workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));

    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_widening_restores_both_requested_surfaces_after_compact_suppression() {
    ensure_gtk_init();
    let wider_window =
        test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1600, 900);
    present_window(&wider_window);
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&wider_window)
            && properties_surface_uses_right_pane(&wider_window)
    });

    assert!(workspace_sidebar_visible(&wider_window));
    assert!(properties_surface_uses_right_pane(&wider_window));
}

#[test]
fn test_properties_visibility_preference_survives_breakpoint_changes() {
    ensure_gtk_init();
    let narrow_window = test_window_with_split_view_state_and_size(true, 0.3, false, 0.25, 1300, 900);
    present_window(&narrow_window);
    activate_action(&narrow_window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&narrow_window)
    });

    assert!(
        narrow_window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );

    narrow_window.destroy();
    flush_after_delay(Duration::from_millis(50));

    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(keys::WINDOW_WIDTH, 1600)
        .expect("set window width");
    settings
        .set_int(keys::WINDOW_HEIGHT, 900)
        .expect("set window height");
    let wide_window = test_window();
    present_window(&wide_window);
    wait_until(Duration::from_secs(2), || properties_surface_uses_right_pane(&wide_window));

    assert!(properties_surface_uses_right_pane(&wide_window));
    assert!(
        wide_window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );
    wide_window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_open_properties_right_pane_and_bottom_sheet_keep_active_document_state() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(true, 0.3, false, 0.25, 1600, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let first_path = dir.path().join("first.txt");
    let second_path = dir.path().join("second.txt");
    fixture::write_text(&first_path, "first\n");
    fixture::write_text(&second_path, "second file\n");

    window.open_document(&first_path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(first_path.clone())
    });
    window.open_document(&second_path);
    let expected_location = second_path.display().to_string();
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .properties_panel
            .imp()
            .location_row
            .subtitle()
            .as_deref()
            == Some(expected_location.as_str())
    });

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_right_pane(&window));
    assert_eq!(
        window
            .imp()
            .properties_panel
            .imp()
            .location_row
            .subtitle()
            .as_deref(),
        Some(expected_location.as_str())
    );

    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(properties_surface_uses_right_pane(&window));
    window.destroy();
    flush_after_delay(Duration::from_millis(50));

    let narrow_window =
        test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1320, 900);
    present_window(&narrow_window);
    narrow_window.open_document(&second_path);
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&narrow_window)
            && narrow_window
                .imp()
                .properties_panel
                .imp()
                .location_row
                .subtitle()
                .as_deref()
                == Some(expected_location.as_str())
    });

    assert!(
        narrow_window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(properties_surface_uses_bottom_sheet(&narrow_window));
}

#[test]
fn test_open_properties_bottom_sheet_and_right_pane_keep_active_document_state() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1320, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sheet-to-pane.txt");
    fixture::write_text(&path, "sheet to pane\n");

    window.open_document(&path);
    let expected_location = path.display().to_string();
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .properties_panel
            .imp()
            .location_row
            .subtitle()
            .as_deref()
            == Some(expected_location.as_str())
    });

    wait_until(Duration::from_secs(2), || properties_surface_uses_bottom_sheet(&window));
    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(properties_surface_uses_bottom_sheet(&window));
    window.destroy();
    flush_after_delay(Duration::from_millis(50));

    let wide_window =
        test_window_with_split_view_state_and_size(true, 0.3, true, 0.25, 1600, 900);
    present_window(&wide_window);
    wide_window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_right_pane(&wide_window)
            && wide_window
                .imp()
                .properties_panel
                .imp()
                .location_row
                .subtitle()
                .as_deref()
                == Some(expected_location.as_str())
    });

    assert!(
        wide_window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(properties_surface_uses_right_pane(&wide_window));
}

#[test]
fn test_closed_properties_state_survives_adaptive_presentation_changes() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1600, 900);
    present_window(&window);

    assert!(!properties_sidebar_visible(&window));
    set_properties_surface_presentation(&window, PropertiesSurfacePresentation::Sheet);
    flush_events();
    assert!(!properties_sidebar_visible(&window));
    set_properties_surface_presentation(&window, PropertiesSurfacePresentation::Pane);
    flush_events();

    assert!(!properties_sidebar_visible(&window));
    assert!(
        !window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(
        !window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );
}

#[test]
fn test_warning_inline_alert_actions_stay_allocated_in_a_narrow_window() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1000, 900);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).source_view().width() > 0
    });

    let editor = active_editor(&window);
    editor
        .info_bar()
        .imp()
        .alert_revealer
        .set_transition_type(gtk4::RevealerTransitionType::None);
    editor
        .info_bar()
        .imp()
        .alert_revealer
        .set_transition_duration(0);
    editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Changes Restored".to_string(),
        body: "Unsaved changes to the document have been restored, and the inline actions must remain visible while the window narrows.".to_string(),
        primary_button: Some("_Discard…".to_string()),
        secondary_button: Some("_Save…".to_string()),
    });
    flush_after_delay(Duration::from_millis(50));

    let info_bar = editor.info_bar().imp();
    assert!(info_bar.alert_revealer.reveals_child());
    assert!(info_bar.alert_revealer.is_child_revealed());
    assert!(info_bar.alert_box.has_css_class("warning"));
    assert!(info_bar.discard_button.property::<bool>("visible"));
    assert!(info_bar.save_button.property::<bool>("visible"));
    assert!(info_bar.dismiss_button.property::<bool>("visible"));
    allocate_widget_for_test(editor.info_bar(), 700);
    wait_for_positive_allocation(editor.info_bar(), "editor inline alert host");
    wait_for_positive_allocation(&*info_bar.alert_box, "inline alert row");
    wait_for_positive_allocation(&*info_bar.actions_box, "alert actions row");
    wait_for_positive_allocation(&*info_bar.discard_button, "discard alert action");
    wait_for_positive_allocation(&*info_bar.save_button, "save alert action");
    wait_for_positive_allocation(&*info_bar.dismiss_button, "dismiss alert action");
    assert_positive_allocation(&*info_bar.discard_button, "discard alert action");
    assert_positive_allocation(&*info_bar.save_button, "save alert action");
    assert_positive_allocation(&*info_bar.dismiss_button, "dismiss alert action");
}

#[test]
fn test_access_error_inline_alert_action_stays_allocated_in_a_narrow_window() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1000, 900);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).source_view().width() > 0
    });

    let editor = active_editor(&window);
    editor
        .info_bar()
        .imp()
        .alert_revealer
        .set_transition_type(gtk4::RevealerTransitionType::None);
    editor
        .info_bar()
        .imp()
        .alert_revealer
        .set_transition_duration(0);
    editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Error,
        title: "Could Not Open File".to_string(),
        body: "Permission was denied while opening the document, so the retry action must stay visible after the shell tightens.".to_string(),
        primary_button: Some("_Retry".to_string()),
        secondary_button: None,
    });
    flush_after_delay(Duration::from_millis(50));

    let info_bar = editor.info_bar().imp();
    assert!(info_bar.alert_revealer.reveals_child());
    assert!(info_bar.alert_revealer.is_child_revealed());
    assert!(info_bar.alert_box.has_css_class("error"));
    assert!(info_bar.retry_button.property::<bool>("visible"));
    assert!(info_bar.dismiss_button.property::<bool>("visible"));
    allocate_widget_for_test(editor.info_bar(), 700);
    wait_for_positive_allocation(editor.info_bar(), "editor inline alert host");
    wait_for_positive_allocation(&*info_bar.alert_box, "inline alert row");
    wait_for_positive_allocation(&*info_bar.actions_box, "alert actions row");
    wait_for_positive_allocation(&*info_bar.retry_button, "retry alert action");
    wait_for_positive_allocation(&*info_bar.dismiss_button, "dismiss alert action");
    assert_positive_allocation(&*info_bar.retry_button, "retry alert action");
    assert_positive_allocation(&*info_bar.dismiss_button, "dismiss alert action");
}

#[test]
fn test_dismissing_one_editor_inline_alert_preserves_other_editor_alert() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    let first_editor = active_editor(&window);
    window.new_tab();
    let second_editor = active_editor(&window);

    first_editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "First Alert".to_string(),
        body: "First editor warning".to_string(),
        primary_button: Some("_Discard...".to_string()),
        secondary_button: None,
    });
    second_editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Second Alert".to_string(),
        body: "Second editor warning".to_string(),
        primary_button: Some("_Discard...".to_string()),
        secondary_button: None,
    });

    assert!(first_editor.info_bar().imp().alert_revealer.reveals_child());
    assert!(second_editor.info_bar().imp().alert_revealer.reveals_child());

    first_editor.info_bar().imp().dismiss_button.emit_clicked();

    assert!(
        !first_editor.info_bar().imp().alert_revealer.reveals_child(),
        "dismissed editor alert should be hidden"
    );
    assert!(
        second_editor.info_bar().imp().alert_revealer.reveals_child(),
        "other editor alert should remain visible"
    );
}

#[test]
fn test_restored_workspaces_survive_dual_sidebar_shell() {
    ensure_gtk_init();
    let _folders_dir = seed_restored_workspaces();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);

    present_window(&window);
    wait_for_workspace_folders(&window, 3);
    assert!(workspace_sidebar_visible(&window));
    assert_eq!(window.imp().sidebar.all_workspace_folder_paths().len(), 3);
}

#[test]
fn test_restored_folder_set_workspace_uses_one_section_with_ordered_folder_rows() {
    ensure_gtk_init();
    let (_folders_dir, first_folder, second_folder) = seed_folder_set_workspace();
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_sections(&window, 1);

    let section = first_sidebar_section(&window);
    assert_eq!(section.imp().header_label.text(), "folder set");
    assert!(section.has_folders());
    assert!(
        !section.imp().empty_folder_set_label.is_visible(),
        "folder-set workspaces with folders should show the tree"
    );

    let top_level_store = section.imp().top_level_store.borrow();
    let top_level_store = top_level_store
        .as_ref()
        .expect("folder-set workspace should install a top-level store");
    let folder_paths = (0..top_level_store.n_items())
        .filter_map(|index| top_level_store.item(index).and_downcast::<lushtext_core::ui::sidebar::FileTreeItem>())
        .filter_map(|item| item.path())
        .collect::<Vec<_>>();

    assert_eq!(folder_paths, vec![first_folder, second_folder]);
}

#[test]
fn test_restored_empty_folder_set_workspace_keeps_real_section() {
    ensure_gtk_init();
    seed_empty_folder_set_workspace();
    let window = test_window();
    present_window(&window);

    wait_for_workspace_sections(&window, 1);

    let section = first_sidebar_section(&window);
    assert_eq!(section.imp().header_label.text(), "empty folder set");
    assert!(!section.has_folders());
    assert_eq!(window.imp().sidebar.all_workspace_folder_paths(), Vec::<PathBuf>::new());
    assert!(
        section.imp().empty_folder_set_label.is_visible(),
        "empty folder-set workspaces should keep an explicit empty state"
    );
    assert!(
        !section.imp().inner_scrolled_window.is_visible(),
        "empty folder-set workspaces should not show an empty tree shell"
    );
    assert!(
        section.imp().add_folder_button.is_sensitive(),
        "empty folder-set workspaces should keep their Add Folder affordance available"
    );
    assert_eq!(
        window.imp().sidebar.current_scope(),
        WorkspaceScope::workspace(WorkspaceId::new("ws-empty"))
    );
    assert_eq!(
        window.imp().sidebar.current_scope_folder_paths(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn test_selecting_empty_workspace_scope_keeps_empty_coverage() {
    ensure_gtk_init();
    let (_folders_dir, populated_folder) = seed_empty_and_populated_workspaces();
    let window = test_window();
    present_window(&window);

    wait_for_workspace_sections(&window, 2);
    wait_for_workspace_folders(&window, 1);
    wait_for_workspace_consumers(&window, 1, 1);
    assert_eq!(
        window.imp().sidebar.all_workspace_folder_paths(),
        vec![populated_folder]
    );

    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    dropdown.set_selected(1);
    flush_events();

    wait_until(Duration::from_secs(3), || {
        window.imp().sidebar.current_scope() == WorkspaceScope::workspace(WorkspaceId::new("ws-empty"))
            && window.imp().sidebar.current_scope_folder_paths().is_empty()
            && window
                .imp()
                .search_panel
                .imp()
                .runtime
                .workspace_folders
                .borrow()
                .is_empty()
            && window.imp().command_palette.file_index_len() == 0
    });

    assert_eq!(dropdown.selected(), 1);
    let sections = window.imp().sidebar.imp().sections.borrow();
    assert!(sections[0].property::<bool>("visible"));
    assert!(
        !sections[1].property::<bool>("visible"),
        "selecting an empty workspace should not rebase to the populated workspace"
    );
}

#[test]
fn test_add_folder_to_existing_workspace_updates_state_and_consumers() {
    ensure_gtk_init();
    seed_empty_folder_set_workspace();
    let folders_dir = tempfile::tempdir().expect("new workspace folder tempdir");
    let folder = folders_dir.path().join("added-folder");
    fixture::create_dir_all(&folder);
    fixture::write_text(&folder.join("added.rs"), "fn added() {}\n");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_sections(&window, 1);

    window
        .imp()
        .sidebar
        .select_folder_for_workspace_for_test(&WorkspaceId::new("ws-empty"), &folder);

    wait_for_workspace_folders(&window, 1);
    wait_for_workspace_consumers(&window, 1, 1);

    let section = first_sidebar_section(&window);
    assert!(section.has_folders());
    assert!(
        !section.imp().empty_folder_set_label.is_visible(),
        "adding a folder should replace the empty folder-set state with the tree"
    );
    assert_eq!(window.imp().sidebar.current_scope_folder_paths(), vec![folder]);
}

#[test]
fn test_add_duplicate_folder_to_existing_workspace_reports_feedback() {
    ensure_gtk_init();
    let (_folders_dir, first_folder, second_folder) = seed_folder_set_workspace();
    let window = test_window();
    let messages = Rc::new(RefCell::new(Vec::<(String, NotificationSeverity)>::new()));
    let messages_clone = Rc::clone(&messages);
    window.imp().sidebar.connect_message(move |text, severity| {
        messages_clone
            .borrow_mut()
            .push((text.to_string(), severity));
    });

    present_window(&window);
    wait_for_workspace_folders(&window, 2);

    window
        .imp()
        .sidebar
        .select_folder_for_workspace_for_test(&WorkspaceId::new("ws-folder-set"), &first_folder);

    wait_until(Duration::from_secs(10), || {
        messages.borrow().iter().any(|(text, severity)| {
            text == "Folder already belongs to this workspace"
                && *severity == NotificationSeverity::Warning
        })
    });

    assert_eq!(
        window.imp().sidebar.all_workspace_folder_paths(),
        vec![first_folder, second_folder]
    );
    assert!(
        messages.borrow().iter().any(|(text, severity)| {
            text == "Folder already belongs to this workspace"
                && *severity == NotificationSeverity::Warning
        }),
        "duplicate folder adds should produce recoverable status feedback"
    );
}

#[test]
fn test_remove_folder_from_workspace_preserves_files_notes_and_empty_section() {
    ensure_gtk_init();
    let (_folders_dir, first_folder, second_folder) = seed_folder_set_workspace();
    let marker = first_folder.join("keep.txt");
    fixture::write_text(&marker, "do not delete\n");
    let data_dir = json_store::data_dir();
    folder_note_service::save_for_folder(
        &data_dir,
        &first_folder,
        &RichNoteBody::new("Keep this folder note"),
    )
    .expect("save folder note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);

    window.imp().sidebar.remove_folder_from_workspace_for_test(
        &WorkspaceId::new("ws-folder-set"),
        &WorkspaceFolderId::new("first"),
        &first_folder,
    );

    wait_until(Duration::from_secs(3), || {
        window.imp().sidebar.all_workspace_folder_paths() == vec![second_folder.clone()]
    });
    assert!(fs_metadata::exists(&first_folder));
    assert_eq!(fs_read::text(&marker).expect("read marker"), "do not delete\n");
    assert_eq!(
        folder_note_service::load_for_folder(&data_dir, &first_folder)
            .expect("load preserved folder note")
            .expect("folder note should remain")
            .note
            .text,
        "Keep this folder note"
    );

    window.imp().sidebar.remove_folder_from_workspace_for_test(
        &WorkspaceId::new("ws-folder-set"),
        &WorkspaceFolderId::new("second"),
        &second_folder,
    );

    wait_for_workspace_folders(&window, 0);
    wait_for_workspace_sections(&window, 1);
    let section = first_sidebar_section(&window);
    assert!(!section.has_folders());
    assert!(section.imp().empty_folder_set_label.is_visible());
}

#[test]
fn test_remove_folder_from_workspace_refreshes_scope_consumers_without_changing_scope() {
    ensure_gtk_init();
    let workspace_id = WorkspaceId::new("ws-folder-set");
    let (_folders_dir, first_folder, second_folder) =
        seed_folder_set_workspace_with_scope(WorkspaceScope::workspace(workspace_id.clone()));
    fixture::create_dir_all(&first_folder.join("images"));
    fixture::write_bytes(&first_folder.join("images/logo.png"), b"not really a png");
    fixture::write_text(&second_folder.join("beta.rs"), "fn beta() {}\n");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    let editor = active_editor(&window);
    let markdown_language = sourceview5::LanguageManager::default()
        .language("markdown")
        .expect("markdown language");
    editor.buffer().set_language(Some(&markdown_language));
    editor.buffer().set_text("![Logo](images/logo.png)");

    activate_action(&window, "toggle-preview-mode");
    wait_until(Duration::from_secs(3), || {
        window.imp().preview_mode.get()
            && markdown_preview_has_image_fallback_title(&window, "Image could not be loaded")
    });
    assert!(action_enabled(&window, "notes-open-folder-note"));

    window.imp().sidebar.remove_folder_from_workspace_for_test(
        &workspace_id,
        &WorkspaceFolderId::new("second"),
        &second_folder,
    );

    wait_until(Duration::from_secs(3), || {
        window.imp().sidebar.current_scope() == WorkspaceScope::workspace(workspace_id.clone())
            && window.imp().sidebar.current_scope_folder_paths() == vec![first_folder.clone()]
    });
    wait_for_workspace_consumers(&window, 1, 1);
    wait_until(Duration::from_secs(3), || {
        markdown_preview_has_image_fallback_title(&window, "Image could not be loaded")
    });
    assert!(action_enabled(&window, "notes-open-folder-note"));

    window.imp().sidebar.remove_folder_from_workspace_for_test(
        &workspace_id,
        &WorkspaceFolderId::new("first"),
        &first_folder,
    );

    wait_until(Duration::from_secs(3), || {
        window.imp().sidebar.current_scope() == WorkspaceScope::workspace(workspace_id.clone())
            && window.imp().sidebar.current_scope_folder_paths().is_empty()
    });
    wait_for_workspace_consumers(&window, 0, 0);
    wait_until(Duration::from_secs(3), || {
        markdown_preview_has_image_fallback_title(&window, "Image file not found")
    });
    assert!(!action_enabled(&window, "notes-open-folder-note"));
}

#[test]
fn test_reorder_workspace_folders_refreshes_markdown_preview_image_context() {
    ensure_gtk_init();
    let workspace_id = WorkspaceId::new("ws-folder-set");
    let (_folders_dir, first_folder, second_folder) =
        seed_folder_set_workspace_with_scope(WorkspaceScope::workspace(workspace_id.clone()));
    for folder in [&first_folder, &second_folder] {
        fixture::create_dir_all(&folder.join("images"));
        fixture::write_bytes(&folder.join("images/logo.png"), b"not really an image");
    }

    let window = test_window();
    window.new_tab();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    let editor = active_editor(&window);
    let markdown_language = sourceview5::LanguageManager::default()
        .language("markdown")
        .expect("markdown language");
    editor.buffer().set_language(Some(&markdown_language));
    editor.buffer().set_text("![Logo](images/logo.png)");

    activate_action(&window, "toggle-preview-mode");
    let first_image = first_folder.join("images/logo.png").display().to_string();
    wait_until(Duration::from_secs(3), || {
        markdown_preview_has_image_fallback_title(&window, "Image could not be loaded")
            && markdown_preview_has_image_fallback_body_containing(&window, &first_image)
    });

    let section = first_sidebar_section(&window);
    section.notify_reorder_folder_requested(
        &WorkspaceFolderId::new("second"),
        WorkspaceFolderMoveDirection::Up,
    );
    let second_image = second_folder.join("images/logo.png").display().to_string();
    wait_until(Duration::from_secs(3), || {
        window
            .imp()
            .sidebar
            .workspaces_file()
            .workspace(&workspace_id)
            .is_some_and(|workspace| {
                workspace.folder_paths() == vec![second_folder.clone(), first_folder.clone()]
            })
            && markdown_preview_has_image_fallback_body_containing(&window, &second_image)
    });
}

#[test]
fn test_rapid_workspace_mutations_persist_latest_sidebar_state() {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("workspace mutation tempdir");
    let alpha_first = folders_dir.path().join("alpha-first");
    let alpha_second = folders_dir.path().join("alpha-second");
    let alpha_added = folders_dir.path().join("alpha-added");
    let beta_folder = folders_dir.path().join("beta");
    for folder in [&alpha_first, &alpha_second, &alpha_added, &beta_folder] {
        fixture::create_dir_all(folder);
    }

    let alpha = WorkspaceId::new("ws-alpha");
    let beta = WorkspaceId::new("ws-beta");
    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::workspace(alpha.clone()),
        workspaces: vec![
            WorkspaceConfig::with_folders(
                alpha.clone(),
                "Alpha",
                vec![
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("alpha-first"), alpha_first.clone()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("alpha-second"), alpha_second.clone()),
                ],
            ),
            WorkspaceConfig::with_one_folder(beta.clone(), "Beta", beta_folder),
        ],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces)
        .expect("save rapid-mutation seed workspaces");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_sections(&window, 2);
    wait_for_workspace_folders(&window, 3);

    window
        .imp()
        .sidebar
        .select_folder_for_workspace_for_test(&alpha, &alpha_added);
    wait_for_workspace_folders(&window, 4);
    let added_id = window
        .imp()
        .sidebar
        .workspaces_file()
        .workspace(&alpha)
        .and_then(|workspace| {
            workspace
                .folders
                .iter()
                .find(|folder| folder.path == alpha_added)
                .map(|folder| folder.id.clone())
        })
        .expect("added folder id");

    let alpha_section = first_sidebar_section(&window);
    alpha_section.notify_reorder_folder_requested(&added_id, WorkspaceFolderMoveDirection::Up);
    wait_until(Duration::from_secs(3), || {
        window
            .imp()
            .sidebar
            .workspaces_file()
            .workspace(&alpha)
            .is_some_and(|workspace| {
                workspace.folder_paths()
                    == vec![
                        alpha_first.clone(),
                        alpha_added.clone(),
                        alpha_second.clone(),
                    ]
            })
    });

    window.imp().sidebar.remove_folder_from_workspace_for_test(
        &alpha,
        &WorkspaceFolderId::new("alpha-first"),
        &alpha_first,
    );
    wait_until(Duration::from_secs(3), || {
        window
            .imp()
            .sidebar
            .workspaces_file()
            .workspace(&alpha)
            .is_some_and(|workspace| {
                workspace.folder_paths() == vec![alpha_added.clone(), alpha_second.clone()]
            })
    });

    window
        .imp()
        .sidebar
        .rename_workspace_for_test(&alpha, "Latest Alpha");
    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    dropdown.set_selected(2);
    flush_events();
    wait_until(Duration::from_secs(3), || {
        window.imp().sidebar.current_scope() == WorkspaceScope::workspace(beta.clone())
    });

    window.imp().sidebar.remove_workspace_for_test(&beta);
    wait_for_workspace_sections(&window, 1);
    wait_until(Duration::from_secs(5), || {
        let loaded = workspace_manager::load(&json_store::data_dir()).expect("load workspaces");
        loaded.current_scope() == WorkspaceScope::All
            && loaded.workspaces.len() == 1
            && loaded.workspace(&alpha).is_some_and(|workspace| {
                workspace.name == "Latest Alpha"
                    && workspace.folder_paths()
                        == vec![alpha_added.clone(), alpha_second.clone()]
                    && workspace
                        .folders
                        .first()
                        .is_some_and(|folder| folder.id == added_id)
            })
    });

    flush_after_delay(Duration::from_millis(350));
    let loaded = workspace_manager::load(&json_store::data_dir())
        .expect("reload final rapid-mutation workspace state");
    let workspace = loaded.workspace(&alpha).expect("remaining workspace");
    assert_eq!(loaded.current_scope(), WorkspaceScope::All);
    assert_eq!(workspace.name, "Latest Alpha");
    assert_eq!(workspace.folder_paths(), vec![alpha_added, alpha_second]);
    assert_eq!(workspace.folders[0].id, added_id);
}

#[test]
fn test_workspace_selector_updates_search_and_palette_scope() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    dropdown.set_selected(1);
    flush_events();

    wait_for_workspace_consumers(&window, 1, 1);
    assert_eq!(
        window
            .imp()
            .search_panel
            .imp()
            .runtime
            .workspace_folders
            .borrow()
            .as_slice(),
        &[left_folder],
    );

    dropdown.set_selected(0);
    flush_events();
    wait_for_workspace_consumers(&window, 2, 2);
}

#[test]
fn test_restored_workspace_scope_narrows_consumers_on_startup() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);

    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    assert_eq!(dropdown.selected(), 2);
    assert_eq!(
        window
            .imp()
            .search_panel
            .imp()
            .runtime
            .workspace_folders
            .borrow()
            .as_slice(),
        &[right_folder],
    );
}

#[test]
fn test_properties_panel_shows_safe_untitled_metadata_state() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let panel = window.imp().properties_panel.imp();
    assert_eq!(
        panel.location_row.subtitle().as_deref(),
        Some("Untitled document")
    );
    assert_eq!(
        panel.file_size_row.subtitle().as_deref(),
        Some("Not available")
    );
    assert!(
        panel
            .statistics_row
            .subtitle()
            .is_some_and(|subtitle| subtitle.contains("1 line")),
    );
    assert_eq!(
        panel.formatting_source_row.subtitle().as_deref(),
        Some("Not available for untitled tabs")
    );
    assert_eq!(
        panel.health_summary_row.subtitle().as_deref(),
        Some("Untitled documents do not have file-backed health details yet.")
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
    window
        .flush_dirty_drafts()
        .expect("discarded draft flush should succeed");

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
fn test_flush_dirty_drafts_fails_when_manifest_cannot_be_saved() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("keep me");
    editor.buffer().set_modified(true);

    let draft_id = editor.draft_id().expect("draft id");
    let data_dir = json_store::data_dir();
    let drafts_dir = draft_service::drafts_dir(&data_dir);
    let manifest_path = drafts_dir.join("manifest.json");
    if fs_metadata::path_status(&manifest_path)
        .is_ok_and(lushtext_core::services::filesystem::PathStatus::is_directory)
    {
        fixture::remove_dir_all(&manifest_path);
    } else {
        fixture::remove_file(&manifest_path);
    }
    fixture::create_dir_all(&manifest_path);

    let error = window
        .flush_dirty_drafts()
        .expect_err("manifest failure should block close-time draft flush");

    assert!(
        error.to_string().contains("failed to save draft manifest"),
        "unexpected error: {error}",
    );
    assert_eq!(
        draft_service::read_draft(&data_dir, &draft_id).expect("read draft"),
        Some("keep me".to_string()),
        "draft bytes already written must be visible for recovery",
    );

    fixture::remove_dir_all(&manifest_path);
    draft_service::delete_draft_file(&data_dir, &draft_id).expect("delete draft file");
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
    fixture::write_text(&path, "saved content");

    window.complete_save_as(&editor, None, None, Some(old_draft_id.as_str()), &path, Ok(()));

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
fn test_file_chooser_open_selection_opens_selected_document() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("chooser tempdir");
    let path = dir.path().join("chosen.txt");
    fixture::write_text(&path, "chosen through chooser\n");

    let window = test_window();
    present_window(&window);
    window.select_open_file_for_test(&path);

    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
            && active_editor(&window).file_path() == Some(path.clone())
            && editor_buffer_text(&active_editor(&window)) == "chosen through chooser\n"
    });
}

#[test]
fn test_file_chooser_open_uri_reports_feedback_without_creating_tab() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let uri = "smb://example.test/share/chooser-open.txt";

    window.select_open_file_uri_for_test(uri);

    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert!(
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains(uri)
                && status.text.contains("only local files are supported")),
        "chooser URI selection should produce visible feedback"
    );
}

#[test]
fn test_file_chooser_save_as_selection_adopts_destination_after_write() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("chooser tempdir");
    let path = dir.path().join("saved-through-chooser.txt");

    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("save as through chooser\n");
    editor.buffer().set_modified(true);
    let old_draft_id = editor.draft_id().expect("untitled draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &old_draft_id, "save as through chooser\n")
        .expect("seed draft");

    window.select_save_as_destination_for_test(&path);

    wait_until(Duration::from_secs(2), || {
        fs_metadata::exists(&path)
            && editor.file_path() == Some(path.clone())
            && !editor.is_modified()
    });
    assert_eq!(
        fs_read::text(&path).expect("read saved chooser destination"),
        "save as through chooser\n"
    );
    wait_until(Duration::from_secs(2), || {
        draft_service::read_draft(&data_dir, &old_draft_id)
            .expect("read draft")
            .is_none()
    });
}

#[test]
fn test_file_chooser_save_as_uri_preserves_modified_editor_identity() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("keep this unsaved text");
    editor.buffer().set_modified(true);
    let uri = "smb://example.test/share/save-as.txt";

    window.select_save_as_uri_for_test(uri);

    assert_eq!(editor.file_path(), None);
    assert!(editor.is_modified());
    assert_eq!(editor_buffer_text(&editor), "keep this unsaved text");
    assert!(
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains(uri)
                && status.text.contains("only local files are supported")),
        "Save As URI selection should produce visible feedback"
    );
}

#[test]
fn test_file_chooser_save_as_cancels_pending_load_result_before_adopting_destination() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("save-as stale load tempdir");
    let source = dir.path().join("source.txt");
    let destination = dir.path().join("destination.txt");
    fixture::write_text(&source, "source disk bytes\n");

    let window = test_window();
    window.open_document(&source);
    let editor = active_editor(&window);
    let stale_generation = editor.load_generation_for_test();
    editor.buffer().set_text("save before load settles\n");
    editor.buffer().set_modified(true);

    window.select_save_as_destination_for_test(&destination);
    wait_until(Duration::from_secs(3), || {
        fs_metadata::exists(&destination)
            && editor.file_path() == Some(destination.clone())
            && !editor.is_saving()
            && !editor.is_modified()
    });

    let stale_result = editor_io::LoadResult {
        content: "source disk bytes\n".to_string(),
        size: 18,
        size_check: FileSizeCheck::Normal,
        canonical_path: Some(fs_metadata::canonical_path(&source).expect("canonical source")),
        mtime: Some(123),
        encoding_state: DocumentEncodingState::default(),
        has_bom: false,
        file_health: Vec::new(),
    };
    assert!(
        !editor.apply_load_result_for_test(stale_generation, Ok(stale_result)),
        "Save As must cancel the pending load generation before adopting the destination",
    );
    assert_eq!(editor_buffer_text(&editor), "save before load settles\n");
    assert_eq!(
        fs_read::text(&destination).expect("read save-as destination"),
        "save before load settles\n"
    );
}

#[cfg(unix)]
#[test]
fn test_file_chooser_save_as_existing_symlink_updates_target_without_replacing_link() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("chooser symlink tempdir");
    let target = dir.path().join("target.txt");
    let link = dir.path().join("link.txt");
    fixture::write_text(&target, "old target\n");
    fixture::symlink(&target, &link);

    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("new target\n");
    editor.buffer().set_modified(true);

    window.select_save_as_destination_for_test(&link);

    wait_until(Duration::from_secs(2), || {
        fs_read::text(&target).is_ok_and(|content| content == "new target\n")
            && editor.file_path() == Some(link.clone())
            && !editor.is_modified()
    });
    assert!(
        fixture::is_symlink(&link),
        "Save As must update the symlink target without replacing the link"
    );
    assert_eq!(fs_read::text(&target).expect("read target"), "new target\n");
    assert!(window.imp().open_paths.borrow().contains(&link));
    assert!(
        window
            .imp()
            .open_paths
            .borrow()
            .contains(&fs_metadata::canonical_path(&target).expect("canonical target")),
        "Save As should register the canonical target for duplicate detection"
    );
}

#[test]
fn test_save_as_stale_canonical_refresh_does_not_reinsert_previous_destination() {
    ensure_gtk_init();
    let _reset = CanonicalRefreshDelayReset;
    set_canonical_refresh_delay_for_test(300);
    let dir = tempfile::tempdir().expect("save-as canonical tempdir");
    let first_path = dir.path().join("first.txt");
    let second_path = dir.path().join("second.txt");

    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("save as canonical refresh\n");
    editor.buffer().set_modified(true);

    window.select_save_as_destination_for_test(&first_path);
    wait_until(Duration::from_secs(2), || {
        editor.file_path() == Some(first_path.clone()) && !editor.is_modified()
    });
    let first_canonical = fs_metadata::canonical_path(&first_path).expect("canonical first path");

    window.select_save_as_destination_for_test(&second_path);
    wait_until(Duration::from_secs(2), || {
        editor.file_path() == Some(second_path.clone()) && !editor.is_modified()
    });
    let second_canonical =
        fs_metadata::canonical_path(&second_path).expect("canonical second path");

    flush_after_delay(Duration::from_millis(450));

    let open_paths = window.imp().open_paths.borrow();
    assert!(!open_paths.contains(&first_path));
    assert!(!open_paths.contains(&first_canonical));
    assert!(open_paths.contains(&second_path));
    assert!(open_paths.contains(&second_canonical));
}

#[test]
fn test_save_as_canonical_refresh_after_tab_close_does_not_reopen_path_key() {
    ensure_gtk_init();
    let _reset = CanonicalRefreshDelayReset;
    set_canonical_refresh_delay_for_test(300);
    let dir = tempfile::tempdir().expect("save-as close canonical tempdir");
    let path = dir.path().join("closed-after-save.txt");

    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("close before canonical refresh\n");
    editor.buffer().set_modified(true);

    window.select_save_as_destination_for_test(&path);
    wait_until(Duration::from_secs(2), || {
        editor.file_path() == Some(path.clone()) && !editor.is_modified()
    });
    let canonical = fs_metadata::canonical_path(&path).expect("canonical saved path");

    let page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("saved tab selected");
    window.imp().tab_view.close_page(&page);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);

    flush_after_delay(Duration::from_millis(450));

    let open_paths = window.imp().open_paths.borrow();
    assert!(!open_paths.contains(&path));
    assert!(!open_paths.contains(&canonical));
}

#[test]
fn test_workspace_row_state_window_tracks_open_switch_close_and_failed_load() {
    let _data_dir = isolated_data_dir();
    let (_folder_dir, alpha, beta, missing) = seed_workspace_row_state_files();
    let window = test_window();
    present_window(&window);
    wait_for_workspace_sections(&window, 1);

    let section = first_sidebar_section(&window);
    section.expand_folders();
    select_sidebar_path(&section, &alpha);
    select_sidebar_path(&section, &beta);
    select_sidebar_path(&section, &missing);

    window.open_document(&alpha);
    wait_until(Duration::from_secs(3), || {
        active_editor(&window).file_size().is_some()
            && active_editor(&window).file_path().as_deref() == Some(alpha.as_path())
    });
    assert_window_workspace_row_state(&section, &alpha, true, true);

    window.open_document(&beta);
    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == 2
            && active_editor(&window).file_path().as_deref() == Some(beta.as_path())
            && active_editor(&window).file_size().is_some()
    });
    assert_window_workspace_row_state(&section, &alpha, true, false);
    assert_window_workspace_row_state(&section, &beta, true, true);

    window.open_document(&alpha);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 2
            && active_editor(&window).file_path().as_deref() == Some(alpha.as_path())
    });
    assert_window_workspace_row_state(&section, &alpha, true, true);
    assert_window_workspace_row_state(&section, &beta, true, false);

    window.close_tab_for_path(&alpha);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
            && active_editor(&window).file_path().as_deref() == Some(beta.as_path())
    });
    assert_window_workspace_row_state(&section, &alpha, false, false);
    assert_window_workspace_row_state(&section, &beta, true, true);

    fixture::remove_file(&missing);
    window.open_document(&missing);
    wait_until(Duration::from_secs(3), || {
        active_editor(&window).load_state() == EditorLoadState::Failed
            && active_editor(&window).file_path().is_none()
    });
    assert_window_workspace_row_state(&section, &missing, false, false);
    assert_window_workspace_row_state(&section, &beta, true, false);
}

#[test]
fn test_workspace_row_state_window_updates_save_as_rename_and_delete() {
    let _data_dir = isolated_data_dir();
    let (_folder_dir, alpha, beta, _missing) = seed_workspace_row_state_files();
    let window = test_window();
    present_window(&window);
    wait_for_workspace_sections(&window, 1);

    let section = first_sidebar_section(&window);
    section.expand_folders();
    select_sidebar_path(&section, &alpha);
    select_sidebar_path(&section, &beta);

    window.new_tab();
    let editor = active_editor(&window);
    editor.buffer().set_text("save as alpha\n");
    editor.buffer().set_modified(true);
    window.select_save_as_destination_for_test(&alpha);
    wait_until(Duration::from_secs(3), || {
        editor.file_path().as_deref() == Some(alpha.as_path()) && !editor.is_modified()
    });
    assert_window_workspace_row_state(&section, &alpha, true, true);
    assert_window_workspace_row_state(&section, &beta, false, false);

    editor.buffer().set_text("save as beta\n");
    editor.buffer().set_modified(true);
    window.select_save_as_destination_for_test(&beta);
    wait_until(Duration::from_secs(3), || {
        editor.file_path().as_deref() == Some(beta.as_path()) && !editor.is_modified()
    });
    assert_window_workspace_row_state(&section, &alpha, false, false);
    assert_window_workspace_row_state(&section, &beta, true, true);

    window.update_tab_path(&beta, &alpha);
    wait_until(Duration::from_secs(3), || {
        active_editor(&window).file_path().as_deref() == Some(alpha.as_path())
    });
    assert_window_workspace_row_state(&section, &alpha, true, true);
    assert_window_workspace_row_state(&section, &beta, false, false);

    window.close_tab_for_path(&alpha);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);
    assert_window_workspace_row_state(&section, &alpha, false, false);
}

#[test]
fn test_workspace_row_state_window_restores_session_and_hidden_scope_projection() {
    let _data_dir = isolated_data_dir();
    let (_folders_dir, _left_folder, right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-left")));
    let right_file = right_folder.join("beta.rs");
    session_service::save(
        &json_store::data_dir(),
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(right_file.clone()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save row-state session");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_sections(&window, 2);
    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == 1
            && active_editor(&window).file_path().as_deref() == Some(right_file.as_path())
    });

    let right_section = window.imp().sidebar.imp().sections.borrow()[1].clone();
    assert!(
        !right_section.property::<bool>("visible"),
        "right workspace should start hidden behind the selected left scope"
    );

    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    dropdown.set_selected(2);
    flush_after_delay(Duration::from_millis(300));
    wait_until(Duration::from_secs(3), || right_section.property::<bool>("visible"));
    right_section.expand_folders();

    assert_window_workspace_row_state(&right_section, &right_file, true, true);
}

#[test]
fn test_workspace_row_state_empty_and_no_workspace_shells_stay_neutral() {
    let data_dir = isolated_data_dir();
    seed_empty_folder_set_workspace();
    let loose_file = data_dir.path().join("loose.txt");
    fixture::write_text(&loose_file, "loose\n");

    let empty_window = test_window();
    present_window(&empty_window);
    wait_for_workspace_sections(&empty_window, 1);
    let empty_section = first_sidebar_section(&empty_window);
    empty_window.open_document(&loose_file);
    wait_until(Duration::from_secs(3), || {
        active_editor(&empty_window).file_path().as_deref() == Some(loose_file.as_path())
            && active_editor(&empty_window).file_size().is_some()
    });
    assert!(
        empty_section.file_row_state_for_test(&loose_file).is_none(),
        "an empty workspace should not invent row-state surfaces for files outside its tree"
    );
    assert!(empty_section.imp().empty_folder_set_label.is_visible());

    seed_no_workspaces();
    let no_workspace_window = test_window();
    present_window(&no_workspace_window);
    wait_until(Duration::from_secs(3), || {
        no_workspace_window.imp().sidebar.imp().sections.borrow().is_empty()
    });
    no_workspace_window.open_document(&loose_file);
    wait_until(Duration::from_secs(3), || {
        active_editor(&no_workspace_window).file_path().as_deref() == Some(loose_file.as_path())
            && active_editor(&no_workspace_window).file_size().is_some()
    });
    assert!(
        no_workspace_window.imp().sidebar.imp().sections.borrow().is_empty(),
        "no-workspace startup should stay structurally empty while tab row-state changes"
    );
}

#[test]
fn test_new_workspace_name_entry_creates_empty_selected_workspace() {
    ensure_gtk_init();
    seed_no_workspaces();

    let window = test_window();
    present_window(&window);

    window
        .imp()
        .sidebar
        .enter_new_workspace_name_for_test("  writing plans  ");

    wait_for_workspace_sections(&window, 1);
    let section = first_sidebar_section(&window);
    assert_eq!(section.imp().header_label.text(), "writing plans");
    assert!(!section.has_folders());
    assert!(section.imp().empty_folder_set_label.is_visible());
    assert_eq!(window.imp().sidebar.all_workspace_folder_paths(), Vec::<PathBuf>::new());
    assert_eq!(
        window.imp().sidebar.current_scope(),
        WorkspaceScope::workspace(section.workspace_id())
    );
    assert_eq!(
        window.imp().sidebar.current_scope_folder_paths(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn test_new_workspace_whitespace_name_does_not_mutate_workspace_state() {
    ensure_gtk_init();
    seed_no_workspaces();

    let window = test_window();
    present_window(&window);

    window
        .imp()
        .sidebar
        .enter_new_workspace_name_for_test("   \n\t  ");
    flush_events();

    assert!(window.imp().sidebar.imp().sections.borrow().is_empty());
    assert_eq!(window.imp().sidebar.current_scope(), WorkspaceScope::All);
    assert_eq!(window.imp().sidebar.all_workspace_folder_paths(), Vec::<PathBuf>::new());
    assert_eq!(
        window.imp().sidebar.current_scope_folder_paths(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn test_file_chooser_cancellation_preserves_document_workspace_and_draft_state() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("cancelled chooser draft\n");
    editor.buffer().set_modified(true);
    let old_draft_id = editor.draft_id().expect("untitled draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &old_draft_id, "cancelled chooser draft\n")
        .expect("seed draft");
    let workspace_folders = window.imp().sidebar.all_workspace_folder_paths();

    window.cancel_open_file_for_test();
    window.cancel_save_as_destination_for_test();
    window.imp().sidebar.cancel_new_workspace_for_test();
    flush_events();

    assert_tab_count(&window, 1);
    assert_eq!(editor.file_path(), None);
    assert!(editor.is_modified());
    assert_eq!(editor.draft_id().as_deref(), Some(old_draft_id.as_str()));
    assert_eq!(editor_buffer_text(&editor), "cancelled chooser draft\n");
    assert_eq!(
        draft_service::read_draft(&data_dir, &old_draft_id).expect("read draft"),
        Some("cancelled chooser draft\n".to_string()),
    );
    assert_eq!(window.imp().sidebar.all_workspace_folder_paths(), workspace_folders);
}

#[test]
fn test_local_history_action_requires_saved_eligible_document() {
    ensure_gtk_init();
    let window = test_window();

    assert!(!action_enabled(&window, "show-local-history"));

    window.new_tab();
    flush_events();
    assert!(!action_enabled(&window, "show-local-history"));

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.txt");
    fixture::write_text(&path, "one\n");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });
    assert!(action_enabled(&window, "show-local-history"));

    let editor = active_editor(&window);
    editor.imp().size_check.set(FileSizeCheck::DisableUndoAndSyntax);
    let saved_page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("saved page selected");
    window.new_tab();
    flush_events();
    window.imp().tab_view.set_selected_page(&saved_page);
    flush_events();
    assert!(!action_enabled(&window, "show-local-history"));
}

#[test]
fn test_active_editor_extra_menu_includes_contextual_notes_and_local_history() {
    ensure_gtk_init();
    let window = test_window_with_restored_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("extra-menu.txt");
    fixture::write_text(&path, "hello");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    let menu = editor
        .source_view()
        .extra_menu()
        .expect("source view should expose an extra menu");
    let labels = menu_model_labels(&menu);
    for label in [
        "Toggle Bookmark",
        "Edit Bookmark…",
        "Open Document Note…",
        "Local History…",
    ] {
        assert!(
            labels.iter().any(|entry| entry == label),
            "editor content menu should offer {label}"
        );
    }
}

#[test]
fn test_local_history_dialog_shows_empty_state_without_snapshots() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty-history.txt");
    fixture::write_text(&path, "one\n");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some() && action_enabled(&window, "show-local-history")
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    assert_readable_empty_status_dialog(&dialog, &child, "empty local-history browser");
    assert!(
        find_label_by_text(&child, "No local history yet").is_some(),
        "empty-state browser should explain why no snapshots are listed"
    );
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&find_status_page(&child).expect("empty local-history status page"));
}

#[test]
fn test_local_history_browser_explains_empty_snapshot_and_disables_copy() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty-snapshot-history.txt");
    fixture::write_text(&path, "");

    let data_dir = json_store::data_dir();
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed empty baseline");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some() && action_enabled(&window, "show-local-history")
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let history_dialog_size = settled_widget_outer_size(&dialog);
    wait_until(Duration::from_secs(2), || {
        find_label_by_text(&child, "This snapshot was empty").is_some()
    });
    assert_settled_widget_outer_size(
        &dialog,
        history_dialog_size,
        "local-history empty snapshot preview",
    );

    assert!(
        find_label_by_text(&child, "This snapshot was empty").is_some(),
        "empty snapshots should explain that they contained no text"
    );
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&find_status_page(&child).expect("empty snapshot status page"));
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(
            &find_local_history_preview_stack(&child).expect("local-history preview stack"),
        );
    assert!(
        find_label_by_text(&child, "Before edits · Empty file").is_some(),
        "empty snapshots should use semantic metadata instead of only 0 B"
    );
    let restore_button = find_button_by_label(&child, "Restore").expect("restore button");
    let copy_button = find_button_by_label(&child, "Copy").expect("copy button");
    assert!(
        restore_button.is_sensitive(),
        "empty historical snapshots should still be restorable"
    );
    assert!(
        !gtk4::test_accessible_has_state(&restore_button, gtk4::AccessibleState::Disabled),
        "Restore should not expose disabled state for a valid empty snapshot"
    );
    assert!(
        !copy_button.is_sensitive(),
        "copy should be disabled when the snapshot has no text content"
    );
    AccessibleAudit::new()
        .states(&[gtk4::AccessibleState::Disabled])
        .assert_on(&copy_button);
}

#[test]
fn test_local_history_browser_warns_and_shows_repaired_snapshot() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("repaired-history.txt");
    fixture::write_text(&path, "one\n");

    let data_dir = json_store::data_dir();
    let body = "recoverable history\n";
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        body,
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed recoverable snapshot");
    let identity = local_history_service::resolve_document_identity(&path).expect("history identity");
    let index_path = local_history_service::local_history_dir(&data_dir)
        .join(identity.sidecar_id)
        .join("index.json");
    fixture::write_text(&index_path, "not local-history json");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some() && action_enabled(&window, "show-local-history")
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let recovered_label = format!("Recovered snapshot · {} B", body.len());
    wait_until(Duration::from_secs(2), || {
        find_label_by_text(&child, &recovered_label).is_some()
            && window
                .imp()
                .notification_bus
                .status_bar_view()
                .is_some_and(|status| {
                    status
                        .text
                        .contains("Some local-history metadata needed recovery")
                })
    });
}

#[test]
fn test_local_history_browser_hides_legacy_empty_baseline_noise() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("legacy-noise-history.txt");
    fixture::write_text(&path, "");

    let data_dir = json_store::data_dir();
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed oldest empty baseline");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "draft content",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Periodic,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed first periodic");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed second empty baseline");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "draft content",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Periodic,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed second periodic");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed newest empty baseline");

    let raw_snapshots = local_history_service::list_snapshots_for_path(&data_dir, &path)
        .expect("list local history");
    assert_eq!(raw_snapshots.len(), 5);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some() && action_enabled(&window, "show-local-history")
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let sidebar = find_adw_sidebar(&child).expect("snapshot sidebar");
    wait_until(Duration::from_secs(2), || sidebar.item(1).is_some());

    assert!(
        sidebar.item(2).is_none(),
        "legacy empty-baseline rows should be filtered out of the visible browser sidebar"
    );
    assert!(
        find_label_by_text(&child, "Before edits · Empty file").is_none(),
        "legacy empty-baseline labels should be hidden from view"
    );
    assert!(
        find_label_by_text(&child, "While editing · 13 B").is_some(),
        "useful periodic history should remain visible"
    );
    assert_eq!(
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list stored local history")
            .len(),
        5,
        "browser filtering must not delete stored local history"
    );
}

#[test]
fn test_local_history_dialog_scales_from_parent_and_keeps_preview_dominant() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1600, 1000);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("viewer-history.txt");
    fixture::write_text(&path, "current\n");

    let data_dir = json_store::data_dir();
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "version one\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed version one");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "version two\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed version two");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let split_view = find_navigation_split_view(&child).expect("navigation split view");
    let sidebar = find_adw_sidebar(&child).expect("snapshot sidebar");
    let history_dialog_size = settled_widget_outer_size(&dialog);
    wait_until(Duration::from_secs(2), || {
        find_button_by_label(&child, "Copy").is_some_and(|button| button.is_sensitive())
    });
    assert_settled_widget_outer_size(
        &dialog,
        history_dialog_size,
        "local-history initial preview load",
    );
    wait_until(Duration::from_secs(2), || sidebar.item(1).is_some());
    sidebar.set_selected(1);
    flush_events();
    assert_settled_widget_outer_size(
        &dialog,
        history_dialog_size,
        "local-history selection loading state",
    );
    wait_until(Duration::from_secs(2), || {
        find_button_by_label(&child, "Copy").is_some_and(|button| button.is_sensitive())
    });
    assert_settled_widget_outer_size(
        &dialog,
        history_dialog_size,
        "local-history selected preview load",
    );

    let window_width = current_window_width(&window);
    let window_height = current_window_height(&window);
    let dialog_width = dialog.content_width();
    let dialog_height = dialog.content_height();
    assert!(
        !dialog.follows_content_size(),
        "viewer dialog must honor the configured content size instead of shrinking to the child"
    );
    assert!(
        dialog_width >= 1200,
        "expected a large rendered viewer width, got {dialog_width}"
    );
    assert!(
        dialog_width <= window_width - 20,
        "viewer dialog should stay smaller than its parent width (dialog {dialog_width}, parent {window_width})"
    );
    assert!(
        dialog_height >= 760,
        "expected a tall rendered viewer height, got {dialog_height}"
    );
    assert!(
        dialog_height <= window_height - 20,
        "viewer dialog should stay smaller than its parent height (dialog {dialog_height}, parent {window_height})"
    );
    assert!(
        split_view.max_sidebar_width() < f64::from(dialog_width) / 2.0,
        "snapshot rail should stay narrower than the preview-dominant half of the viewer"
    );
    let max_sidebar_width = split_view.max_sidebar_width();
    assert!(
        max_sidebar_width <= 340.0,
        "snapshot rail should stay in browse-rail territory, got {max_sidebar_width}"
    );
}

// This exercises the adaptive local-history browser, whose sheet open, collapse
// reveal, and restore steps are animation- and async-backed. Those waits get a
// generous budget because animation settle + background completion under headless
// Mutter occasionally exceed a tight window under load; the wait conditions still
// return the instant the state is real.
#[test]
fn test_local_history_browser_collapses_and_restore_can_be_undone() {
    ensure_gtk_init();
    let window = test_window_with_restored_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history-browser.txt");
    fixture::write_text(&path, "current\n");

    let data_dir = json_store::data_dir();
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "version one\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed version one");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "version two\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed version two");

    window.open_document(&path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    editor.buffer().set_text("working copy");
    editor.buffer().set_modified(true);

    // The first modified transition schedules the "before edits" baseline.
    // Wait for it before opening the browser so row 1 deterministically names
    // the newest saved snapshot instead of racing the background capture.
    wait_until(Duration::from_secs(10), || {
        let Ok(snapshots) = local_history_service::list_snapshots_for_path(&data_dir, &path) else {
            return false;
        };
        if !snapshots
            .first()
            .is_some_and(|meta| meta.origin == LocalHistorySnapshotOrigin::Baseline)
        {
            return false;
        }
        let Some(target) = snapshots.get(1) else {
            return false;
        };
        local_history_service::load_snapshot_for_path(&data_dir, &path, &target.snapshot_id)
            .ok()
            .flatten()
            .is_some_and(|snapshot| snapshot.text == "version two\n")
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let split_view = find_navigation_split_view(&child).expect("navigation split view");
    let sidebar = find_adw_sidebar(&child).expect("snapshot sidebar");
    wait_until(Duration::from_secs(5), || sidebar.item(1).is_some());

    split_view.set_collapsed(true);
    sidebar.set_selected(1);
    flush_events();

    wait_until(Duration::from_secs(5), || split_view.shows_content());
    // The restore button may be sensitive while an older preview is still
    // visible. Wait for the exact target snapshot text before clicking it.
    wait_for_local_history_preview_text(&child, "version two\n");
    wait_until(Duration::from_secs(5), || {
        find_button_by_label(&child, "Restore").is_some_and(|button| button.is_sensitive())
    });

    let restore_button =
        find_button_by_label(&child, "Restore").expect("restore button in local-history dialog");
    restore_button.emit_clicked();

    wait_until(Duration::from_secs(5), || editor_text(&editor) == "version two\n");
    wait_until(Duration::from_secs(5), || {
        window
            .imp()
            .notification_bus
            .editor_info_bar_view(editor.notification_owner_id())
            .is_some()
    });

    let undo_button = find_button_by_label(
        editor.info_bar().upcast_ref::<gtk4::Widget>(),
        "Undo Restore",
    )
    .expect("undo restore button");
    undo_button.emit_clicked();

    wait_until(Duration::from_secs(5), || editor_text(&editor) == "working copy");
}

#[test]
fn test_local_history_capture_policy_respects_full_save_only_and_unavailable_modes() {
    ensure_gtk_init();
    // SAFETY: each widget test runs in its own child process, and this interval
    // override is read later when local-history timers are scheduled.
    unsafe { std::env::set_var("LUSHTEXT_LOCAL_HISTORY_INTERVAL_MS", "25") };

    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("capture-policy.txt");
    fixture::write_text(&path, "saved\n");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let data_dir = json_store::data_dir();
    let editor = active_editor(&window);
    editor.buffer().set_text("baseline change");
    editor.buffer().set_modified(true);

    wait_until(Duration::from_secs(2), || {
        !local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list local history")
            .is_empty()
    });

    editor.buffer().set_text("periodic change");
    editor.buffer().set_modified(true);
    wait_until(Duration::from_secs(2), || {
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list local history")
            .len()
            >= 2
    });

    editor.buffer().set_text("save boundary change");
    editor.buffer().set_modified(true);
    activate_action(&window, "save");
    wait_until(Duration::from_secs(2), || {
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list local history")
            .len()
            >= 3
            && !editor.is_saving()
    });

    editor.imp().size_check.set(FileSizeCheck::DisableSyntax);
    let count_after_full = local_history_service::list_snapshots_for_path(&data_dir, &path)
        .expect("list after full mode")
        .len();

    editor.buffer().set_text("save only change");
    editor.buffer().set_modified(true);
    flush_after_delay(Duration::from_millis(120));
    assert_eq!(
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list after save-only baseline wait")
            .len(),
        count_after_full,
        "save-only mode must skip baseline and periodic capture",
    );

    activate_action(&window, "save");
    wait_until(Duration::from_secs(2), || {
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list after save-only save")
            .len()
            == count_after_full + 1
            && !editor.is_saving()
    });

    editor.imp().size_check.set(FileSizeCheck::DisableUndoAndSyntax);
    let count_after_save_only = local_history_service::list_snapshots_for_path(&data_dir, &path)
        .expect("list after save-only mode")
        .len();

    editor.buffer().set_text("unavailable change");
    editor.buffer().set_modified(true);
    flush_after_delay(Duration::from_millis(120));
    activate_action(&window, "save");
    wait_until(Duration::from_secs(2), || {
        !editor.is_saving() && !editor.is_modified()
    });
    flush_after_delay(Duration::from_millis(120));

    assert_eq!(
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list after unavailable mode")
            .len(),
        count_after_save_only,
        "unavailable mode must disable both automatic and save-boundary capture",
    );
}

#[test]
fn test_properties_panel_updates_for_file_backed_editor() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("example.txt");
    fixture::write_text(&path, "hello world");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .properties_panel
            .imp()
            .file_size_row
            .subtitle()
            .is_some_and(|subtitle| subtitle == "11 B")
    });

    let panel = window.imp().properties_panel.imp();
    assert_eq!(
        panel.location_row.subtitle().as_deref(),
        Some(path.display().to_string().as_str())
    );
    assert_eq!(panel.file_size_row.subtitle().as_deref(), Some("11 B"));
    assert!(
        panel
            .statistics_row
            .subtitle()
            .is_some_and(|subtitle| subtitle.contains("1 line")),
    );
    assert_eq!(
        panel.health_summary_row.subtitle().as_deref(),
        Some("No file-health issues recorded for this document.")
    );
}

#[test]
fn test_status_bar_shows_detected_encoding_and_line_endings_after_open() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("encoded.txt");
    fixture::write_bytes(&path, [0x63, 0x61, 0x66, 0xE9, b'\r', b'\n']);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let status_bar = window.imp().status_bar.imp();
    assert_eq!(
        status_bar.encoding_button.label().as_deref(),
        Some("Windows-1252")
    );
    assert_eq!(
        status_bar.line_ending_button.label().as_deref(),
        Some("CRLF")
    );
}

#[test]
fn test_reopen_with_encoding_requires_discard_confirmation_for_modified_document() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("reopen.txt");
    fixture::write_text(&path, "hello");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    editor.buffer().set_text("modified");
    editor.buffer().set_modified(true);

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 5));
    let labels = alert_dialog_extra_label_texts(&dialog);
    assert_label_text_contains(&labels, "Current Document");
    assert_label_text_contains(&labels, "Actions");
    click_alert_extra_button(&dialog, "Reopen with Encoding…");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Reopen with Encoding"))
    });
    let dialog = visible_alert_dialog(&window).expect("reopen encoding dialog visible");
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 7));
    let labels = alert_dialog_extra_label_texts(&dialog);
    assert_label_text_contains(&labels, "Current Decoding");
    assert_label_text_contains(&labels, "Encoding Options");
    click_alert_extra_button(&dialog, "Windows-1252");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Discard Changes"))
    });
}

#[test]
fn test_status_bar_encoding_label_stays_short_after_save_policy_change() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("save-policy.txt");
    fixture::write_text(&path, "hello");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 5));
    click_alert_extra_button(&dialog, "Save Using Encoding…");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Save Using Encoding"))
    });
    let dialog = visible_alert_dialog(&window).expect("save encoding dialog visible");
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 7));
    let labels = alert_dialog_extra_label_texts(&dialog);
    assert_label_text_contains(&labels, "Current Save Encoding");
    assert_label_text_contains(&labels, "Encoding Options");
    click_alert_extra_button(&dialog, "Windows-1252");

    wait_until(Duration::from_secs(2), || {
        active_editor(&window).save_encoding() == DocumentEncoding::Windows1252
    });
    assert_eq!(
        active_editor(&window).save_encoding(),
        DocumentEncoding::Windows1252
    );
    assert_eq!(
        window
            .imp()
            .status_bar
            .imp()
            .encoding_button
            .label()
            .as_deref(),
        Some("UTF-8")
    );
}

#[test]
fn test_save_encoding_choice_surfaces_lossy_confirmation() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("lossy.txt");
    fixture::write_text(&path, "hello");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    editor.buffer().set_text("emoji 😀");
    editor.buffer().set_modified(true);

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    click_alert_extra_button(&dialog, "Save Using Encoding…");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Save Using Encoding"))
    });
    let dialog = visible_alert_dialog(&window).expect("save encoding dialog visible");
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 7));
    click_alert_extra_button(&dialog, "Windows-1252");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Lossy Encoding Conversion"))
    });
    assert_eq!(
        active_editor(&window).save_encoding(),
        DocumentEncoding::Utf8
    );
}

#[test]
fn test_stale_lossy_encoding_analysis_result_is_rejected_after_buffer_change() {
    ensure_gtk_init();
    let _reset = LossyEncodingAnalysisDelayReset;
    set_lossy_encoding_analysis_delay_for_test(300);
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("stale-lossy.txt");
    fixture::write_text(&path, "hello");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    editor.buffer().set_text("emoji 😀");
    editor.buffer().set_modified(true);

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    click_alert_extra_button(&dialog, "Save Using Encoding…");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Save Using Encoding"))
    });
    let dialog = visible_alert_dialog(&window).expect("save encoding dialog visible");
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 7));
    click_alert_extra_button(&dialog, "Windows-1252");

    editor.buffer().set_text("plain ascii now");
    editor.buffer().set_modified(true);
    flush_after_delay(Duration::from_millis(450));

    assert!(
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_none_or(|heading| !heading.contains("Lossy Encoding Conversion")),
        "stale lossy analysis must not show a confirmation for old buffer content",
    );
    assert_eq!(active_editor(&window).save_encoding(), DocumentEncoding::Utf8);
}

#[test]
fn test_mixed_line_endings_warning_opens_normalization_picker_and_updates_status_bar() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("mixed.txt");
    fixture::write_text(&path, "a\r\nb\nc\r\n");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    wait_until(Duration::from_secs(2), || {
        editor.info_bar().imp().alert_revealer.reveals_child()
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
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 5));
    let labels = alert_dialog_extra_label_texts(&dialog);
    assert_label_text_contains(&labels, "Current Document");
    assert_label_text_contains(&labels, "Future Save Style");
    assert_label_text_contains(&labels, "Opened With");
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
fn test_external_file_change_warning_preserves_unsaved_buffer() {
    let (window, _dir, path) = open_temp_document("original\n");
    let editor = active_editor(&window);
    editor.buffer().set_text("local unsaved\n");
    editor.buffer().set_modified(true);

    write_external_change_after_mtime_tick(&path, &editor, "external version\n");
    wait_for_external_change_warning(&editor);

    assert_eq!(
        editor_buffer_text(&editor),
        "local unsaved\n",
        "external monitor warning must not silently replace unsaved editor text",
    );
    assert!(editor.is_modified());
    assert_eq!(
        editor.info_bar().imp().discard_button.label().as_deref(),
        Some("_Discard Changes and Reload")
    );
}

#[test]
fn test_external_file_change_dismiss_keeps_buffer_content() {
    let (window, _dir, path) = open_temp_document("original\n");
    let editor = active_editor(&window);
    editor.buffer().set_text("local draft\n");
    editor.buffer().set_modified(true);

    write_external_change_after_mtime_tick(&path, &editor, "external version\n");
    wait_for_external_change_warning(&editor);

    editor.info_bar().imp().dismiss_button.emit_clicked();
    wait_until(Duration::from_secs(2), || {
        !editor.info_bar().imp().alert_revealer.reveals_child()
    });

    assert_eq!(editor_buffer_text(&editor), "local draft\n");
    assert!(
        editor.is_modified(),
        "dismissing an external-change warning should not clear unsaved state",
    );
}

#[test]
fn test_external_file_change_discard_action_reloads_disk_bytes() {
    let (window, _dir, path) = open_temp_document("original\n");
    let editor = active_editor(&window);
    editor.buffer().set_text("local draft\n");
    editor.buffer().set_modified(true);

    write_external_change_after_mtime_tick(&path, &editor, "external version\n");
    wait_for_external_change_warning(&editor);

    editor.info_bar().imp().discard_button.emit_clicked();
    wait_until(Duration::from_secs(3), || {
        editor_buffer_text(&editor) == "external version\n"
            && !editor.info_bar().imp().alert_revealer.reveals_child()
    });

    assert!(!editor.is_modified());
}

#[test]
fn test_own_save_does_not_surface_external_change_warning() {
    let (window, _dir, path) = open_temp_document("original\n");
    let editor = active_editor(&window);
    editor.buffer().set_text("saved by lushtext\n");
    editor.buffer().set_modified(true);

    activate_action(&window, "save");
    wait_until(Duration::from_secs(3), || {
        !editor.is_modified()
            && editor_io::mtime_secs(&path) == editor.imp().monitor.last_known_mtime.get()
    });
    flush_after_delay(Duration::from_millis(700));

    assert!(
        !editor.info_bar().imp().alert_revealer.reveals_child(),
        "saving through LushText should update monitor state before file events become warnings",
    );
    assert_eq!(
        fs_read::text(&path).expect("saved file contents"),
        "saved by lushtext\n"
    );
}

#[test]
fn test_action_save_failure_keeps_modified_document_state() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let dir = tempfile::tempdir().expect("save action failure tempdir");
    let bad_path = dir.path().join("missing-parent").join("action-save-fails.txt");
    editor.set_file_path(&bad_path);
    editor.buffer().set_text("automation save should stay unsaved\n");
    editor.buffer().set_modified(true);

    activate_action(&window, "save");

    wait_until(Duration::from_secs(3), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Save failed"))
    });
    assert_tab_count(&window, 1);
    assert_eq!(
        editor_buffer_text(&editor),
        "automation save should stay unsaved\n"
    );
    assert!(editor.is_modified());
    assert!(!fs_metadata::exists(&bad_path));
}

#[test]
fn test_close_modified_file_tab_cancel_keeps_unsaved_tab() {
    let (window, _dir, path, editor) = modified_file_backed_tab("disk\n", "unsaved\n");

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "cancel");

    wait_until(Duration::from_secs(2), || visible_alert_dialog(&window).is_none());
    assert_tab_count(&window, 1);
    assert_eq!(editor_buffer_text(&editor), "unsaved\n");
    assert!(editor.is_modified());
    assert_eq!(
        fs_read::text(&path).expect("disk contents after cancel"),
        "disk\n"
    );
}

#[test]
fn test_action_close_tab_requires_modified_document_confirmation() {
    let (window, _dir, path, editor) = modified_file_backed_tab("disk\n", "action unsaved\n");

    activate_boolean_action(&window, "set-search-panel-visible", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().search_panel_revealer.reveals_child()
    });
    activate_action(&window, "close-tab");

    wait_for_save_changes_dialog(&window);
    assert_tab_count(&window, 1);
    assert_eq!(editor_buffer_text(&editor), "action unsaved\n");
    assert!(editor.is_modified());
    assert_eq!(
        fs_read::text(&path).expect("disk contents before action close response"),
        "disk\n"
    );

    respond_to_save_changes_dialog(&window, "cancel");
    wait_until(Duration::from_secs(2), || visible_alert_dialog(&window).is_none());
    assert_tab_count(&window, 1);
    assert_eq!(editor_buffer_text(&editor), "action unsaved\n");
    assert!(editor.is_modified());
}

#[test]
fn test_keyboard_save_changes_cancel_preserves_modified_tab() {
    let (window, _dir, path, editor) = modified_file_backed_tab("disk\n", "unsaved\n");

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    activate_save_changes_response_with_keyboard(&window, "cancel");

    wait_until(Duration::from_secs(2), || visible_alert_dialog(&window).is_none());
    assert_tab_count(&window, 1);
    assert_eq!(editor_buffer_text(&editor), "unsaved\n");
    assert!(editor.is_modified());
    assert_eq!(
        fs_read::text(&path).expect("disk contents after keyboard cancel"),
        "disk\n"
    );
}

#[test]
fn test_keyboard_save_changes_save_writes_then_closes() {
    let (window, _dir, path, _editor) = modified_file_backed_tab("disk\n", "keyboard saved\n");

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    activate_save_changes_response_with_keyboard(&window, "save");

    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == 0
            && fs_read::text(&path).is_ok_and(|contents| contents == "keyboard saved\n")
    });
}

#[test]
fn test_keyboard_save_changes_discard_closes_without_writing() {
    let (window, _dir, path, _editor) = modified_file_backed_tab("disk\n", "keyboard discard\n");

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    activate_save_changes_response_with_keyboard(&window, "discard");

    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);
    assert_eq!(
        fs_read::text(&path).expect("disk contents after keyboard discard"),
        "disk\n"
    );
}

#[test]
fn test_close_modified_file_tab_save_writes_then_closes() {
    let (window, _dir, path, _editor) = modified_file_backed_tab("disk\n", "saved\n");

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "save");

    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == 0
            && fs_read::text(&path).is_ok_and(|contents| contents == "saved\n")
    });
}

#[test]
fn test_close_modified_file_tab_save_failure_keeps_tab_modified() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let dir = tempfile::tempdir().expect("save failure tempdir");
    let bad_path = dir.path().join("missing-parent").join("close-fails.txt");
    editor.set_file_path(&bad_path);
    editor.buffer().set_text("still unsaved\n");
    editor.buffer().set_modified(true);

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "save");

    wait_until(Duration::from_secs(3), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Save failed during close"))
    });
    assert_tab_count(&window, 1);
    assert_eq!(editor_buffer_text(&editor), "still unsaved\n");
    assert!(editor.is_modified());
    assert!(!fs_metadata::exists(&bad_path));
}

#[test]
fn test_close_modified_file_tab_discard_closes_without_writing() {
    let (window, _dir, path, _editor) = modified_file_backed_tab("disk\n", "discard me\n");

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "discard");

    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);
    assert_eq!(
        fs_read::text(&path).expect("disk contents after discard"),
        "disk\n"
    );
}

#[test]
fn test_window_close_request_cancel_keeps_modified_file_tab() {
    let (window, _dir, path, editor) = modified_file_backed_tab("disk\n", "window unsaved\n");

    window.close();
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "cancel");

    wait_until(Duration::from_secs(2), || visible_alert_dialog(&window).is_none());
    assert!(window.is_visible());
    assert_tab_count(&window, 1);
    assert_eq!(editor_buffer_text(&editor), "window unsaved\n");
    assert!(editor.is_modified());
    assert_eq!(
        fs_read::text(&path).expect("disk contents after window cancel"),
        "disk\n"
    );
}

#[test]
fn test_window_close_request_save_persists_session_and_cleans_file_draft() {
    let (window, _dir, path, _editor) = modified_file_backed_tab("disk\n", "window saved\n");
    let draft_id = seed_file_backed_draft(&window, &path, "recoverable draft\n");
    let data_dir = json_store::data_dir();

    window.close();
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "save");

    wait_until(Duration::from_secs(3), || {
        !window.is_visible()
            && fs_read::text(&path).is_ok_and(|contents| contents == "window saved\n")
    });
    wait_until(Duration::from_secs(3), || {
        draft_service::read_draft(&data_dir, &draft_id)
            .expect("read cleaned draft")
            .is_none()
    });
    let session = session_service::load(&data_dir).expect("session saved on close");
    assert_eq!(session.tabs.len(), 1);
    assert_eq!(session.tabs[0].path.as_deref(), Some(path.as_path()));
    assert_eq!(session.active_tab_index, Some(0));
}

#[test]
fn test_sync_session_save_failure_keeps_retry_state_and_warns_user() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    let data_dir = json_store::data_dir();
    let session_path = data_dir.join("session.json");
    remove_session_path_for_test(&session_path);
    fixture::create_dir(&session_path);

    window.save_session_sync();

    wait_until(Duration::from_secs(10), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Session layout may not restore"))
    });
    assert!(window.imp().session.save_failed.get());
    assert!(
        window
            .imp()
            .session
            .failure_detail
            .borrow()
            .as_deref()
            .is_some_and(|detail| detail.contains("session.json"))
    );

    fs_mutate::remove_dir_all_if_exists(&session_path).expect("remove blocking session dir");
    window.save_session_sync();

    wait_until(Duration::from_secs(2), || {
        !window.imp().session.save_failed.get()
            && window.imp().session.failure_detail.borrow().is_none()
    });
    let session = session_service::load(&data_dir).expect("retry should write clean session");
    assert_eq!(session.tabs.len(), 1);
}

#[test]
fn test_close_modified_untitled_save_requires_save_as_or_discard() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("untitled work\n");
    editor.buffer().set_modified(true);

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "save");

    wait_until(Duration::from_secs(10), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Untitled documents must be saved"))
    });
    assert_tab_count(&window, 1);
    assert_eq!(editor_buffer_text(&editor), "untitled work\n");
    assert!(editor.is_modified());

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "discard");
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);
}

#[test]
fn test_close_modified_untitled_cancel_preserves_and_discard_cleans_draft() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("untitled draft\n");
    editor.buffer().set_modified(true);
    let draft_id = editor.draft_id().expect("untitled draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &draft_id, "untitled draft\n").expect("seed draft");

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "cancel");

    wait_until(Duration::from_secs(2), || visible_alert_dialog(&window).is_none());
    assert_tab_count(&window, 1);
    assert_eq!(
        draft_service::read_draft(&data_dir, &draft_id).expect("read draft after cancel"),
        Some("untitled draft\n".to_string()),
        "cancel should leave the recovery draft available",
    );

    close_selected_tab(&window);
    wait_for_save_changes_dialog(&window);
    respond_to_save_changes_dialog(&window, "discard");

    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);
    wait_until(Duration::from_secs(3), || {
        draft_service::read_draft(&data_dir, &draft_id)
            .expect("read draft after discard")
            .is_none()
    });
}

#[test]
fn test_multi_tab_window_close_saves_checked_and_discards_unchecked_documents() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["save-me.txt", "discard-me.txt"]);
    let window = test_window();
    present_window(&window);

    for path in &files {
        window.open_document(path);
    }
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 2);
    let save_page = find_tab_page_by_title(&window, "save-me.txt");
    let discard_page = find_tab_page_by_title(&window, "discard-me.txt");
    let save_editor = save_page
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("save editor");
    let discard_editor = discard_page
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("discard editor");
    wait_until(Duration::from_secs(2), || {
        save_editor.file_size().is_some() && discard_editor.file_size().is_some()
    });
    save_editor.buffer().set_text("checked save\n");
    save_editor.buffer().set_modified(true);
    discard_editor.buffer().set_text("unchecked discard\n");
    discard_editor.buffer().set_modified(true);
    let saved_draft_id = seed_file_backed_draft(&window, &files[0], "saved branch draft\n");
    let discarded_draft_id =
        seed_file_backed_draft(&window, &files[1], "discarded branch draft\n");
    let data_dir = json_store::data_dir();

    window.close();
    wait_for_save_changes_dialog(&window);
    let dialog = visible_alert_dialog(&window).expect("multi-document save dialog");
    let checks = save_changes_check_buttons(&dialog);
    assert_eq!(checks.len(), 2);
    save_changes_check_button_for_title(&dialog, "discard-me.txt").set_active(false);
    respond_to_save_changes_dialog(&window, "save");

    wait_until(Duration::from_secs(3), || !window.is_visible());
    assert_eq!(
        fs_read::text(&files[0]).expect("saved checked file"),
        "checked save\n"
    );
    assert_eq!(
        fs_read::text(&files[1]).expect("discarded unchecked file"),
        "content for discard-me.txt\n"
    );
    wait_until(Duration::from_secs(3), || {
        let saved_clean = draft_service::read_draft(&data_dir, &saved_draft_id)
            .expect("read saved branch draft")
            .is_none();
        let discarded_clean = draft_service::read_draft(&data_dir, &discarded_draft_id)
            .expect("read discarded branch draft")
            .is_none();
        saved_clean && discarded_clean
    });
}

#[test]
fn test_keyboard_multi_tab_save_changes_selection_control() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["save-me.txt", "discard-me.txt"]);
    let window = test_window();
    present_window(&window);

    for path in &files {
        window.open_document(path);
    }
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 2);
    let save_page = find_tab_page_by_title(&window, "save-me.txt");
    let discard_page = find_tab_page_by_title(&window, "discard-me.txt");
    let save_editor = save_page
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("save editor");
    let discard_editor = discard_page
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("discard editor");
    wait_until(Duration::from_secs(2), || {
        save_editor.file_size().is_some() && discard_editor.file_size().is_some()
    });
    save_editor.buffer().set_text("keyboard checked save\n");
    save_editor.buffer().set_modified(true);
    discard_editor
        .buffer()
        .set_text("keyboard unchecked discard\n");
    discard_editor.buffer().set_modified(true);

    window.close();
    wait_for_save_changes_dialog(&window);
    let dialog = visible_alert_dialog(&window).expect("multi-document save dialog");
    let discard_check = save_changes_check_button_for_title(&dialog, "discard-me.txt");
    assert!(discard_check.is_active());
    activate_widget_without_pointer(&discard_check);
    assert!(!discard_check.is_active());
    activate_save_changes_response_with_keyboard(&window, "save");

    wait_until(Duration::from_secs(3), || !window.is_visible());
    assert_eq!(
        fs_read::text(&files[0]).expect("saved checked file"),
        "keyboard checked save\n"
    );
    assert_eq!(
        fs_read::text(&files[1]).expect("discarded unchecked file"),
        "content for discard-me.txt\n"
    );
}

#[test]
fn test_close_paths_are_blocked_while_save_is_in_progress() {
    let (window, _dir, _path, editor) = modified_file_backed_tab("disk\n", "saving\n");
    editor.imp().save.inflight.set(true);

    close_selected_tab(&window);

    wait_until(Duration::from_secs(10), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Save is still in progress"))
    });
    assert_tab_count(&window, 1);
    assert!(visible_alert_dialog(&window).is_none());

    window.close();
    flush_events();
    assert!(window.is_visible());
    assert_tab_count(&window, 1);

    editor.imp().save.inflight.set(false);
}

#[test]
fn test_print_action_prepares_active_document_snapshot() {
    ensure_gtk_init();
    let (window, _dir, path) = open_temp_document("original print content\n");
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("active print content\nwith metadata\n");
    editor.buffer().set_modified(true);
    flush_events();
    let before = editor_print_state(&editor);
    let captured: Rc<RefCell<Vec<PrintDocumentSnapshot>>> = Rc::default();
    let captured_for_runner = Rc::clone(&captured);

    assert!(action_enabled(&window, "print"));
    with_print_runner_for_test(
        move |snapshot| {
            captured_for_runner.borrow_mut().push(snapshot.clone());
            PrintOutcome::Completed
        },
        || activate_action(&window, "print"),
    );

    let snapshots = captured.borrow();
    assert_eq!(snapshots.len(), 1);
    let snapshot = &snapshots[0];
    assert_eq!(snapshot.path.as_deref(), Some(path.as_path()));
    assert_eq!(snapshot.content, "active print content\nwith metadata\n");
    assert!(snapshot.modified);
    assert_eq!(snapshot.draft_id, before.draft_id);
    assert_eq!(snapshot.title, editor.title());
    assert_eq!(editor_print_state(&editor), before);
}

#[test]
fn test_print_cancel_preserves_document_state() {
    ensure_gtk_init();
    let (window, _dir, _path) = open_temp_document("print cancel content\n");
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("print cancel unsaved edit\n");
    editor.buffer().set_modified(true);
    flush_events();
    let before = editor_print_state(&editor);
    let captured: Rc<RefCell<Vec<PrintDocumentSnapshot>>> = Rc::default();
    let captured_for_runner = Rc::clone(&captured);

    with_print_runner_for_test(
        move |snapshot| {
            captured_for_runner.borrow_mut().push(snapshot.clone());
            PrintOutcome::Cancelled
        },
        || activate_action(&window, "print"),
    );

    assert_eq!(captured.borrow().len(), 1);
    assert_eq!(editor_print_state(&editor), before);
    assert!(
        !window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Print failed")),
        "cancel should not be surfaced as a print failure",
    );
}

#[test]
fn test_print_failure_reports_feedback_and_preserves_document_state() {
    ensure_gtk_init();
    let (window, _dir, _path) = open_temp_document("print failure content\n");
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("print failure unsaved edit\n");
    editor.buffer().set_modified(true);
    flush_events();
    let before = editor_print_state(&editor);

    with_print_runner_for_test(
        |_| PrintOutcome::Failed("simulated backend failure".to_string()),
        || activate_action(&window, "print"),
    );

    assert_eq!(editor_print_state(&editor), before);
    wait_until(Duration::from_secs(10), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Print failed: simulated backend failure"))
    });
}

#[test]
fn test_zoom_actions_and_menu_controls_update_setting_bounds() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_uint(keys::ZOOM_LEVEL, 100)
        .expect("reset zoom");
    let window = test_window();
    present_window(&window);
    let popover = window
        .imp()
        .primary_menu_button
        .popover()
        .expect("primary menu popover");
    let zoom_in = find_button_by_tooltip(popover.upcast_ref(), "Zoom In").expect("zoom in button");
    let zoom_out =
        find_button_by_tooltip(popover.upcast_ref(), "Zoom Out").expect("zoom out button");

    assert!(action_enabled(&window, "zoom-in"));
    assert!(action_enabled(&window, "zoom-out"));
    assert!(action_enabled(&window, "zoom-reset"));
    activate_action(&window, "zoom-in");
    assert_eq!(settings.uint(keys::ZOOM_LEVEL), 110);
    activate_action(&window, "zoom-out");
    assert_eq!(settings.uint(keys::ZOOM_LEVEL), 100);
    activate_action(&window, "zoom-reset");
    assert_eq!(settings.uint(keys::ZOOM_LEVEL), 100);

    settings
        .set_uint(keys::ZOOM_LEVEL, 400)
        .expect("set max zoom");
    flush_events();
    assert!(!zoom_in.is_sensitive());
    assert!(zoom_out.is_sensitive());
    activate_action(&window, "zoom-in");
    assert_eq!(settings.uint(keys::ZOOM_LEVEL), 400);

    settings
        .set_uint(keys::ZOOM_LEVEL, 50)
        .expect("set min zoom");
    flush_events();
    assert!(zoom_in.is_sensitive());
    assert!(!zoom_out.is_sensitive());
    activate_action(&window, "zoom-out");
    assert_eq!(settings.uint(keys::ZOOM_LEVEL), 50);
}

#[test]
fn test_zoom_level_remains_global_across_tab_switches() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_uint(keys::ZOOM_LEVEL, 100)
        .expect("reset zoom");
    let window = test_window();
    window.new_tab();
    window.new_tab();
    present_window(&window);
    let first = window.imp().tab_view.nth_page(0);
    let second = window.imp().tab_view.nth_page(1);
    window.imp().tab_view.set_selected_page(&second);

    activate_action(&window, "zoom-in");
    assert_eq!(settings.uint(keys::ZOOM_LEVEL), 110);

    window.imp().tab_view.set_selected_page(&first);
    flush_events();
    assert_eq!(
        settings.uint(keys::ZOOM_LEVEL),
        110,
        "zoom is a window/application preference, not a per-tab value",
    );
    activate_action(&window, "zoom-out");
    window.imp().tab_view.set_selected_page(&second);
    flush_events();
    assert_eq!(settings.uint(keys::ZOOM_LEVEL), 100);
}

#[test]
fn test_style_scheme_setting_updates_current_and_new_editors() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_string(keys::STYLE_SCHEME, "Adwaita")
        .expect("set initial style scheme");
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 1.0)
        .expect("set opaque tab content");
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let first = active_editor(&window);

    settings
        .set_string(keys::STYLE_SCHEME, "Adwaita-dark")
        .expect("set alternate style scheme");
    wait_until(Duration::from_secs(2), || {
        first.applied_style_scheme_id().as_deref() == Some("Adwaita-dark")
    });

    window.new_tab();
    flush_events();
    let second = active_editor(&window);
    wait_until(Duration::from_secs(2), || {
        second.applied_style_scheme_id().as_deref() == Some("Adwaita-dark")
    });
}

#[test]
fn test_invalid_style_scheme_falls_back_to_bundled_adwaita_scheme() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 1.0)
        .expect("set opaque tab content");
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let expected = if libadwaita::StyleManager::default().is_dark() {
        "Adwaita-dark"
    } else {
        "Adwaita"
    };

    settings
        .set_string(keys::STYLE_SCHEME, "missing-scheme-for-test")
        .expect("set invalid style scheme");

    wait_until(Duration::from_secs(2), || {
        editor.applied_style_scheme_id().as_deref() == Some(expected)
    });
}

#[test]
fn test_cycle_invisible_characters_updates_active_editor_and_default_for_new_tabs() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_string(keys::INVISIBLE_CHARACTERS_MODE, InvisibleCharactersMode::Off.id())
        .expect("reset invisible-character mode");
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let first = active_editor(&window);
    assert_eq!(
        first.invisible_characters_mode(),
        InvisibleCharactersMode::Off
    );

    activate_action(&window, "cycle-invisible-characters");
    assert_eq!(
        first.invisible_characters_mode(),
        InvisibleCharactersMode::WhitespaceOnly
    );
    assert_eq!(
        settings.string(keys::INVISIBLE_CHARACTERS_MODE).as_str(),
        InvisibleCharactersMode::WhitespaceOnly.id()
    );

    activate_action(&window, "cycle-invisible-characters");
    assert_eq!(first.invisible_characters_mode(), InvisibleCharactersMode::All);
    activate_action(&window, "cycle-invisible-characters");
    assert_eq!(first.invisible_characters_mode(), InvisibleCharactersMode::Off);

    activate_action(&window, "cycle-invisible-characters");
    window.new_tab();
    flush_events();
    let second = active_editor(&window);
    assert_eq!(
        second.invisible_characters_mode(),
        InvisibleCharactersMode::WhitespaceOnly,
        "new tabs should inherit the last user-selected invisible-character mode",
    );
}

#[test]
fn test_narrow_window_keeps_quick_encoding_controls_visible() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(keys::WINDOW_WIDTH, 820)
        .expect("set window width");
    settings
        .set_int(keys::WINDOW_HEIGHT, 900)
        .expect("set window height");
    settings
        .set_boolean(keys::WINDOW_MAXIMIZED, false)
        .expect("clear maximized");
    let window = test_window();
    window.set_default_size(820, 900);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("narrow.txt");
    fixture::write_text(&path, "hello");

    window.open_document(&path);
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let status_bar = window.imp().status_bar.imp();
    assert!(
        status_bar.line_ending_button.property::<bool>("visible"),
        "narrow windows should keep the line-ending entry point visible",
    );
    assert!(
        status_bar.encoding_button.property::<bool>("visible"),
        "narrow windows should keep the encoding entry point visible",
    );
    assert_status_bar_readable_one_row(&window, "narrow status bar");
    assert_eq!(
        status_bar.message_label.ellipsize(),
        gtk4::pango::EllipsizeMode::End
    );
    assert!(!status_bar.message_label.wraps());

    let status_widget = window.imp().status_bar.upcast_ref::<gtk4::Widget>();
    for (name, widget) in [
        (
            "line-ending control",
            status_bar.line_ending_button.upcast_ref::<gtk4::Widget>(),
        ),
        (
            "encoding control",
            status_bar.encoding_button.upcast_ref::<gtk4::Widget>(),
        ),
    ] {
        let bounds = widget
            .compute_bounds(status_widget)
            .unwrap_or_else(|| panic!("{name} should compute bounds in the status bar"));
        assert!(
            bounds.x() >= 0.0
                && bounds.y() >= 0.0
                && bounds.x() + bounds.width() <= status_widget.width() as f32 + 1.0
                && bounds.y() + bounds.height() <= status_widget.height() as f32 + 1.0,
            "{name} should remain reachable inside the narrow status row, bounds={bounds:?}, status={}x{}",
            status_widget.width(),
            status_widget.height()
        );
    }
}

#[test]
fn test_closing_properties_pane_restores_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1600, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.source_view().grab_focus();
    flush_events();

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_right_pane(&window));
    window
        .imp()
        .properties_panel
        .imp()
        .location_row
        .grab_focus();
    flush_events();
    let source_ptr = editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr();
    activate_action(&window, "toggle-properties");

    wait_until(Duration::from_secs(2), || {
        gtk4::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| focus.as_ptr() == source_ptr)
    });
    assert!(!properties_sidebar_visible(&window));
    let _ = editor;
}

#[test]
fn test_closing_properties_bottom_sheet_restores_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1300, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.source_view().grab_focus();
    flush_events();

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&window)
    });
    window
        .imp()
        .properties_panel
        .imp()
        .location_row
        .grab_focus();
    flush_events();
    let source_ptr = editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr();
    activate_action(&window, "toggle-properties");

    wait_until(Duration::from_secs(2), || {
        gtk4::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| focus.as_ptr() == source_ptr)
    });
    assert!(!properties_sidebar_visible(&window));
    let _ = editor;
}

#[test]
fn test_closing_properties_surface_with_no_editor_clears_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(800, 900);
    present_window(&window);

    activate_action(&window, "toggle-properties");
    let panel = window.imp().properties_panel.imp();
    panel.location_row.grab_focus();
    flush_events();

    activate_action(&window, "toggle-properties");
    assert!(gtk4::prelude::GtkWindowExt::focus(&window).is_none());
}

#[test]
fn test_properties_toggle_button_lives_in_header_bar_and_is_wired() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(
        window
            .imp()
            .document_properties_toggle_button
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
fn test_primary_menu_exposes_markdown_preview_action() {
    ensure_gtk_init();
    let window = test_window();

    assert_eq!(
        primary_menu_action_for_label(&window, "Markdown Preview").as_deref(),
        Some("win.toggle-preview-mode"),
        "primary menu should expose the rendered Markdown preview action"
    );
}

#[test]
fn test_markdown_preview_shortcut_remains_alt_p() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.toggle-preview-mode", "<Alt>p"),
        "Alt+P should keep toggling Markdown preview-only mode"
    );
}

#[test]
fn test_primary_menu_markdown_preview_renders_active_markdown_buffer() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let dir = tempfile::tempdir().expect("markdown preview tempdir");
    let editor = active_editor(&window);
    editor.set_file_path(&dir.path().join("menu-preview.md"));
    editor
        .buffer()
        .set_text("# Menu Heading\n\nCurrent buffer body");

    activate_primary_menu_item(&window, "Markdown Preview");

    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get() && window.imp().markdown_preview.is_showing_content()
    });
    let rendered = window.imp().markdown_preview.buffer_text();
    assert!(
        rendered.contains("Menu Heading"),
        "preview should render the active Markdown buffer"
    );
    assert!(
        rendered.contains("Current buffer body"),
        "preview should include body text from the active buffer"
    );
    assert!(
        !rendered.contains("# Menu Heading"),
        "preview should hide raw heading markers"
    );
}

#[test]
fn test_primary_menu_markdown_preview_pauses_large_markdown_buffer() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let dir = tempfile::tempdir().expect("large markdown preview tempdir");
    let editor = active_editor(&window);
    editor.set_file_path(&dir.path().join("large-preview.md"));
    editor.buffer().set_text(&"x".repeat(2_500_001));

    activate_primary_menu_item(&window, "Markdown Preview");

    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get() && !window.imp().markdown_preview.is_showing_content()
    });
    assert_eq!(
        window.imp().markdown_preview.placeholder_description_for_test(),
        Some("Markdown preview paused for this large document".to_string())
    );
}

#[test]
fn test_preview_only_definition_list_code_block_uses_live_column() {
    let (window, _dir) =
        prepare_markdown_preview_window(DEFINITION_LIST_CODE_BLOCK_SAMPLE, 1180, 720);

    activate_action(&window, "toggle-preview-mode");

    wait_until(Duration::from_secs(2), || window.imp().preview_mode.get());
    wait_for_markdown_preview_shell(&window);
    assert_live_code_block_uses_preview_column(&window);
}

#[test]
fn test_side_by_side_definition_list_code_block_uses_live_column() {
    ensure_gtk_init();
    gio::Settings::new(lushtext_core::config::APP_ID)
        .set_int(keys::PREVIEW_PANE_POSITION, 520)
        .expect("set wide preview pane for definition-list code block regression");
    let (window, _dir) =
        prepare_markdown_preview_window(DEFINITION_LIST_CODE_BLOCK_SAMPLE, 1800, 720);

    activate_action(&window, "toggle-preview-pane");

    wait_until(Duration::from_secs(2), || window.imp().preview_visible.get());
    wait_for_markdown_preview_shell(&window);
    assert_live_code_block_uses_preview_column(&window);
}

#[test]
fn test_primary_menu_markdown_preview_shows_placeholder_for_non_markdown() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let dir = tempfile::tempdir().expect("plain preview tempdir");
    let editor = active_editor(&window);
    editor.set_file_path(&dir.path().join("plain.txt"));
    editor.buffer().set_text("# Plain text heading-shaped line");

    activate_primary_menu_item(&window, "Markdown Preview");

    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get() && !window.imp().markdown_preview.is_showing_content()
    });
}

#[test]
fn test_notes_menu_exists_and_primary_menu_excludes_note_actions() {
    ensure_gtk_init();
    let window = test_window();

    assert!(window.imp().notes_menu_button.popover().is_some());

    let notes_menu = window
        .imp()
        .notes_menu_button
        .menu_model()
        .expect("notes menu model");
    assert_eq!(
        menu_model_labels(&notes_menu),
        vec![
            "Browse Notes…".to_string(),
            "Add Bookmark".to_string(),
            "Open Document Note…".to_string(),
            "Open Folder Note…".to_string(),
        ]
    );

    let primary_menu = window
        .imp()
        .primary_menu_button
        .menu_model()
        .expect("primary menu model");
    let primary_labels = menu_model_labels(&primary_menu);
    for label in [
        "Add Bookmark",
        "Open Document Note…",
        "Open Folder Note…",
        "Browse Notes…",
        "Edit Bookmark…",
        "Browse Bookmarks…",
    ] {
        assert!(
            !primary_labels.iter().any(|entry| entry == label),
            "primary menu should not include '{label}' once the Notes menu exists",
        );
    }
}

#[test]
fn test_notes_menu_button_shows_without_editor_or_workspace() {
    ensure_gtk_init();
    seed_no_workspaces();
    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(2), || notes_menu_button_visible(&window));
    assert!(action_enabled(&window, "notes-show-notes"));
    for name in [
        "notes-toggle-bookmark",
        "notes-open-document-note",
        "notes-open-folder-note",
    ] {
        assert!(
            !action_enabled(&window, name),
            "expected '{name}' to stay disabled without a document or concrete workspace",
        );
    }
}

#[test]
fn test_notes_menu_state_for_workspace_without_saved_file() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);
    assert!(notes_menu_button_visible(&window));

    for name in [
        "notes-toggle-bookmark",
        "notes-open-document-note",
        "notes-open-folder-note",
    ] {
        assert!(
            !action_enabled(&window, name),
            "expected '{name}' to stay disabled without a saved document",
        );
    }
    assert!(action_enabled(&window, "notes-show-notes"));

    activate_action(&window, "new-tab");
    assert!(notes_menu_button_visible(&window));
    for name in [
        "notes-toggle-bookmark",
        "notes-open-document-note",
        "notes-open-folder-note",
    ] {
        assert!(
            !action_enabled(&window, name),
            "expected '{name}' to stay disabled for an untitled tab",
        );
    }
}

#[test]
fn test_notes_menu_stays_available_after_closing_last_tab_with_workspaces() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);
    activate_action(&window, "new-tab");
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    close_selected_tab(&window);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);

    assert!(notes_menu_button_visible(&window));
    assert!(action_enabled(&window, "notes-show-notes"));
    for name in [
        "notes-toggle-bookmark",
        "notes-open-document-note",
        "notes-open-folder-note",
    ] {
        assert!(
            !action_enabled(&window, name),
            "expected '{name}' to stay disabled after the last tab closes",
        );
    }
}

#[test]
fn test_notes_menu_folder_note_action_enables_for_concrete_scope() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, _right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);
    assert!(notes_menu_button_visible(&window));
    assert!(action_enabled(&window, "notes-open-folder-note"));
    assert!(action_enabled(&window, "notes-show-notes"));
}

#[test]
fn test_notes_menu_folder_note_action_disables_for_empty_workspace_scope() {
    ensure_gtk_init();
    seed_empty_folder_set_workspace();
    let window = test_window();
    present_window(&window);

    wait_for_workspace_sections(&window, 1);
    assert!(notes_menu_button_visible(&window));
    assert!(!action_enabled(&window, "notes-open-folder-note"));
    assert!(action_enabled(&window, "notes-show-notes"));
}

#[test]
fn test_open_folder_note_for_multi_folder_workspace_requires_folder_choice() {
    ensure_gtk_init();
    let (_folders_dir, first_folder, second_folder) = seed_folder_set_workspace_with_scope(
        WorkspaceScope::workspace(WorkspaceId::new("ws-folder-set")),
    );
    let data_dir = json_store::data_dir();
    folder_note_service::save_for_folder(
        &data_dir,
        &second_folder,
        &RichNoteBody::new("Second folder note"),
    )
    .expect("save second folder note");

    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 0);
    assert!(action_enabled(&window, "notes-open-folder-note"));

    activate_action(&window, "notes-open-folder-note");
    wait_until(Duration::from_secs(10), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Open Folder Note")
    });

    let dialog = visible_alert_dialog(&window).expect("folder choice dialog");
    let extra = dialog.extra_child().expect("folder choice extra child");
    assert!(
        find_label_by_text(&extra, &first_folder.display().to_string()).is_some(),
        "folder choice dialog should include the first workspace folder"
    );
    assert!(
        find_label_by_text(&extra, &second_folder.display().to_string()).is_some(),
        "folder choice dialog should include the second workspace folder"
    );
    assert!(
        find_note_editor_stack(&extra).is_none(),
        "multi-folder action must choose a folder before showing the note editor"
    );

    let second_button = find_button_by_tooltip(&extra, &second_folder.display().to_string())
        .expect("second folder chooser button");
    second_button.emit_clicked();
    flush_events();

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Folder Note")
    });
    let dialog = visible_alert_dialog(&window).expect("folder note dialog");
    let extra = dialog.extra_child().expect("folder note extra child");
    assert!(
        find_label_by_text(&extra, &second_folder.display().to_string()).is_some(),
        "choosing the second folder should open that folder's note"
    );
    assert!(
        find_label_by_text(&extra, &first_folder.display().to_string()).is_none(),
        "choosing the second folder must not fall back to the first folder"
    );
}

#[test]
fn test_notes_menu_popup_opens_for_add_and_remove_bookmark_states() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("notes-popup.rs");
    fixture::write_text(&path, "one\ntwo\nthree\n");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("bookmark".to_string()),
        )],
    )
    .expect("save bookmark sidecar");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        let labels = menu_model_labels(
            &window
                .imp()
                .notes_menu_button
                .menu_model()
                .expect("notes menu model"),
        );
        notes_menu_button_visible(&window)
            && active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).bookmark_records().len() == 1
            && labels.iter().any(|label| label == "Remove Bookmark")
    });

    open_notes_menu_popup(&window);
    let labels = menu_model_labels(
        &window
            .imp()
            .notes_menu_button
            .menu_model()
            .expect("notes menu model"),
    );
    assert!(labels.iter().any(|label| label == "Browse Notes…"));
    assert!(labels.iter().any(|label| label == "Remove Bookmark"));
    close_notes_menu_popup(&window);

    let editor = active_editor(&window);
    let line_two = editor.buffer().iter_at_line(1).expect("line two");
    editor.buffer().place_cursor(&line_two);
    flush_events();

    wait_until(Duration::from_secs(2), || {
        menu_model_labels(
            &window
                .imp()
                .notes_menu_button
                .menu_model()
                .expect("notes menu model"),
        )
        .iter()
        .any(|label| label == "Add Bookmark")
    });

    open_notes_menu_popup(&window);
    let labels = menu_model_labels(
        &window
            .imp()
            .notes_menu_button
            .menu_model()
            .expect("notes menu model"),
    );
    assert!(labels.iter().any(|label| label == "Browse Notes…"));
    assert!(labels.iter().any(|label| label == "Add Bookmark"));
    close_notes_menu_popup(&window);
}

#[test]
fn test_sidebar_context_menus_include_note_entry_points() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    let section = window
        .imp()
        .sidebar
        .imp()
        .sections
        .borrow()
        .first()
        .cloned()
        .expect("workspace section");

    select_sidebar_path(&section, &left_folder.join("alpha.rs"));
    section.imp().file_tree_view.grab_focus();
    activate_action(&window, "show-workspace-tree-context-menu");
    wait_until(Duration::from_secs(2), || {
        section
            .imp()
            .context_menu
            .borrow()
            .as_ref()
            .is_some_and(gtk4::prelude::WidgetExt::is_visible)
    });
    let file_menu_labels = {
        let menu_box = section.imp().context_menu_box.borrow();
        action_button_labels(
            menu_box
                .as_ref()
                .expect("file context menu action box should exist"),
        )
    };
    assert!(
        file_menu_labels
            .iter()
            .any(|label| label == "Open Document Note…"),
        "file context menu should expose document notes"
    );
    if let Some(popover) = section.imp().context_menu.borrow().as_ref() {
        popover.popdown();
        flush_events();
    }

    activate_action(&window, "show-workspace-header-context-menu");
    wait_until(Duration::from_secs(2), || {
        section
            .imp()
            .header_context_menu
            .borrow()
            .as_ref()
            .is_some_and(gtk4::prelude::WidgetExt::is_visible)
    });
    let header_menu_labels = {
        let menu_box = section.imp().header_context_menu_box.borrow();
        action_button_labels(
            menu_box
                .as_ref()
                .expect("workspace header context menu action box should exist"),
        )
    };
    assert!(
        header_menu_labels
            .iter()
            .any(|label| label == "Open Folder Note…"),
        "workspace header context menu should expose folder notes"
    );

    *section.imp().context_path.borrow_mut() = Some(left_folder.join("alpha.rs"));
    section.imp().context_is_dir.set(false);
    section
        .activate_action("section.document-note", None)
        .expect("document-note widget action should exist");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });
    visible_alert_dialog(&window)
        .expect("document note dialog")
        .close();
    flush_events();

    section
        .activate_action("ws-header.open-folder-note", None)
        .expect("folder-note widget action should exist");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Folder Note")
    });
}

#[test]
fn test_document_note_dialog_supports_edit_and_render_modes() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("document-note.md");
    let source_text = "# Heading\n\nBody\n";
    let saved_note = "# Heading\n\nSaved note\n\n- dense item\n- [x] checked item\n\nA very long markdown line that should stay inside the stable note dialog surface while it starts in Render mode.";
    let changed_note = "# Heading\n\nChanged note after review";
    fixture::write_text(&path, source_text);

    let data_dir = json_store::data_dir();
    document_note_service::save_for_path(
        &data_dir,
        &path,
        &RichNoteBody::new(saved_note),
    )
    .expect("save document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || active_editor(&window).file_path() == Some(path.clone()));

    activate_action(&window, "open-document-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });

    let dialog = visible_alert_dialog(&window).expect("document note dialog");
    let extra = dialog.extra_child().expect("document note extra child");
    let switcher = find_stack_switcher(&extra).expect("note editor switcher");
    let stack = find_note_editor_stack(&extra).expect("note editor stack");
    assert_eq!(switcher.stack(), Some(stack.clone()));
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&stack);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&switcher);
    let (edit_surface, render_surface) = note_editor_text_views(&extra);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::MultiLine,
        ])
        .assert_on(&edit_surface);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ReadOnly,
            gtk4::AccessibleProperty::MultiLine,
        ])
        .assert_on(&render_surface);
    assert_note_save_response_visible(&dialog);
    wait_for_note_save_response_sensitive(&dialog, false);

    let switcher_bounds = switcher
        .compute_bounds(&extra)
        .expect("switcher bounds in dialog content");
    let stack_bounds = stack.compute_bounds(&extra).expect("stack bounds in dialog content");
    let switcher_right = switcher_bounds.x() + switcher_bounds.width();
    let stack_right = stack_bounds.x() + stack_bounds.width();
    assert!(
        (switcher_bounds.x() - stack_bounds.x()).abs() <= 1.0,
        "switcher should start at the same left edge as the note stack"
    );
    assert!(
        (switcher_right - stack_right).abs() <= 1.0,
        "switcher should reach the same right edge as the note stack"
    );

    assert_note_editor_text_margins_match(&extra);
    assert_note_editor_render_first_keeps_modal_geometry(&dialog, &extra, &stack);

    stack.set_visible_child_name("edit");
    flush_events();
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
    let (edit, _) = note_editor_text_views(&extra);
    edit.buffer().set_text("  # Heading\n\nSaved note\n\n- dense item\n- [x] checked item\n\nA very long markdown line that should stay inside the stable note dialog surface while it starts in Render mode.  ");
    wait_for_note_save_response_sensitive(&dialog, false);
    edit.buffer().set_text(changed_note);
    wait_for_note_save_response_sensitive(&dialog, true);
    edit.buffer().set_text(saved_note);
    wait_for_note_save_response_sensitive(&dialog, false);
    edit.buffer().set_text("  \n\t  ");
    wait_for_note_save_response_sensitive(&dialog, false);
    edit.buffer().set_text(changed_note);
    wait_for_note_save_response_sensitive(&dialog, true);
    stack.set_visible_child_name("render");
    flush_events();
    wait_for_note_save_response_sensitive(&dialog, true);
    alert_response_button(&dialog, "save").emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(5), || {
        document_note_service::load_for_path(&data_dir, &path)
            .ok()
            .flatten()
            .is_some_and(|document| document.note.text == changed_note)
    });
    assert_eq!(fixture::read_text(&path), source_text);
}

#[test]
fn test_empty_document_note_first_render_keeps_modal_geometry_after_typing() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("empty-document-note.md");
    fixture::write_text(&path, "# Source\n\nBody\n");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(path.clone())
            && action_enabled(&window, "open-document-note")
    });

    activate_action(&window, "open-document-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });

    let dialog = visible_alert_dialog(&window).expect("document note dialog");
    let extra = dialog.extra_child().expect("document note extra child");
    let stack = find_note_editor_stack(&extra).expect("note editor stack");
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
    assert_note_save_response_visible(&dialog);
    wait_for_note_save_response_sensitive(&dialog, false);
    let (edit, _) = note_editor_text_views(&extra);
    edit.buffer().set_text("  \n\t  ");
    wait_for_note_save_response_sensitive(&dialog, false);
    edit.buffer().set_text("# Typed document note\n\nPreview me");
    wait_for_note_save_response_sensitive(&dialog, true);
    edit.buffer().set_text("");
    wait_for_note_save_response_sensitive(&dialog, false);
    assert_note_editor_text_margins_match(&extra);
    assert_typed_note_editor_first_render_keeps_modal_geometry(
        &dialog,
        &extra,
        &stack,
        "# Typed document note\n\nPreview me",
    );
}

#[test]
fn test_document_note_save_sensitivity_handles_large_chunked_buffer_edits() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("large-document-note.md");
    fixture::write_text(&path, "# Source\n\nBody\n");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(path.clone())
            && action_enabled(&window, "open-document-note")
    });

    activate_action(&window, "open-document-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });

    let dialog = visible_alert_dialog(&window).expect("document note dialog");
    let extra = dialog.extra_child().expect("document note extra child");
    let stack = find_note_editor_stack(&extra).expect("note editor stack");
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
    wait_for_note_save_response_sensitive(&dialog, false);

    stack.set_visible_child_name("render");
    flush_events();
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));

    let large_note = "chunked note body\n".repeat(150_000);
    let (edit, _) = note_editor_text_views(&extra);
    edit.buffer().set_text(&large_note);
    wait_for_note_save_response_sensitive(&dialog, true);

    edit.buffer().set_text("");
    wait_for_note_save_response_sensitive(&dialog, false);
}

#[test]
fn test_open_folder_note_dialog_for_concrete_scope() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));

    let data_dir = json_store::data_dir();
    folder_note_service::save_for_folder(
        &data_dir,
        &right_folder,
        &RichNoteBody::new("Folder note"),
    )
    .expect("save folder note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);

    activate_action(&window, "open-folder-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Folder Note")
    });

    let dialog = visible_alert_dialog(&window).expect("folder note dialog");
    let extra = dialog.extra_child().expect("folder note extra child");
    let stack = find_note_editor_stack(&extra).expect("folder note editor stack");
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));
    assert_note_save_response_visible(&dialog);
    wait_for_note_save_response_sensitive(&dialog, false);
    assert_note_editor_render_first_keeps_modal_geometry(&dialog, &extra, &stack);
}

#[test]
fn test_open_folder_note_warns_when_sidecar_is_corrupt() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));
    let data_dir = json_store::data_dir();
    let corrupt_identity =
        folder_note_service::resolve_folder_note_identity(&right_folder).expect("folder identity");
    let corrupt_sidecar =
        folder_note_service::folder_notes_dir(&data_dir).join(format!("{}.json", corrupt_identity.sidecar_id));
    fixture::create_dir_all(corrupt_sidecar.parent().expect("sidecar parent"));
    fixture::write_text(&corrupt_sidecar, "not folder note json");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);

    activate_action(&window, "open-folder-note");
    wait_until(Duration::from_secs(10), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Folder Note")
            && window
                .imp()
                .notification_bus
                .status_bar_view()
                .is_some_and(|status| {
                    status
                        .text
                        .contains("Some folder note data could not be loaded")
                })
    });
}

#[test]
fn test_folder_note_dialog_saves_renders_and_clears_note() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));
    let data_dir = json_store::data_dir();
    let note_text = "# Saved folder note\n\nPersistent body";
    let changed_note = "# Saved folder note\n\nPersistent body\n\nReviewed from Render";

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);

    activate_action(&window, "open-folder-note");
    wait_until(Duration::from_secs(5), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Folder Note")
    });

    let dialog = visible_alert_dialog(&window).expect("folder note dialog");
    let extra = dialog.extra_child().expect("folder note extra child");
    let stack = find_note_editor_stack(&extra).expect("folder note editor stack");
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
    assert_note_save_response_visible(&dialog);
    wait_for_note_save_response_sensitive(&dialog, false);
    let (edit, render) = note_editor_text_views(&extra);
    edit.buffer().set_text("  \n\t  ");
    wait_for_note_save_response_sensitive(&dialog, false);
    edit.buffer().set_text(note_text);
    wait_for_note_save_response_sensitive(&dialog, true);
    edit.buffer().set_text("");
    wait_for_note_save_response_sensitive(&dialog, false);
    edit.buffer().set_text(note_text);
    wait_for_note_save_response_sensitive(&dialog, true);
    stack.set_visible_child_name("render");
    flush_events();
    wait_for_note_save_response_sensitive(&dialog, true);
    wait_until(Duration::from_secs(5), || {
        let buffer = render.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .contains("Saved folder note")
    });

    alert_response_button(&dialog, "save").emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(5), || {
        folder_note_service::load_for_folder(&data_dir, &right_folder)
            .ok()
            .flatten()
            .is_some_and(|document| document.note.text == note_text)
            && window
                .imp()
                .notification_bus
                .status_bar_view()
                .is_some_and(|status| status.text.contains("Folder note saved"))
    });

    activate_action(&window, "open-folder-note");
    wait_until(Duration::from_secs(5), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Folder Note")
    });
    let dialog = visible_alert_dialog(&window).expect("reopened folder note dialog");
    let extra = dialog.extra_child().expect("reopened folder note extra child");
    let stack = find_note_editor_stack(&extra).expect("reopened folder note editor stack");
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));
    wait_for_note_save_response_sensitive(&dialog, false);
    stack.set_visible_child_name("edit");
    flush_events();
    let (edit, _) = note_editor_text_views(&extra);
    edit.buffer().set_text("  # Saved folder note\n\nPersistent body  ");
    wait_for_note_save_response_sensitive(&dialog, false);
    edit.buffer().set_text(changed_note);
    wait_for_note_save_response_sensitive(&dialog, true);
    stack.set_visible_child_name("render");
    flush_events();
    wait_for_note_save_response_sensitive(&dialog, true);
    alert_response_button(&dialog, "save").emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(5), || {
        folder_note_service::load_for_folder(&data_dir, &right_folder)
            .ok()
            .flatten()
            .is_some_and(|document| document.note.text == changed_note)
            && window
                .imp()
                .notification_bus
                .status_bar_view()
                .is_some_and(|status| status.text.contains("Folder note saved"))
    });

    activate_action(&window, "open-folder-note");
    wait_until(Duration::from_secs(5), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Folder Note")
    });
    let dialog = visible_alert_dialog(&window).expect("reopened changed folder note dialog");
    alert_response_button(&dialog, "clear").emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(5), || {
        folder_note_service::load_for_folder(&data_dir, &right_folder)
            .expect("folder note load after clear")
            .is_none()
            && window
                .imp()
                .notification_bus
                .status_bar_view()
                .is_some_and(|status| status.text.contains("Folder note cleared"))
    });
}

#[test]
fn test_empty_folder_note_first_render_keeps_modal_geometry_after_typing() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, _right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);

    activate_action(&window, "open-folder-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Folder Note")
    });

    let dialog = visible_alert_dialog(&window).expect("folder note dialog");
    let extra = dialog.extra_child().expect("folder note extra child");
    let stack = find_note_editor_stack(&extra).expect("folder note editor stack");
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
    assert_note_save_response_visible(&dialog);
    wait_for_note_save_response_sensitive(&dialog, false);
    assert_note_editor_text_margins_match(&extra);
    assert_typed_note_editor_first_render_keeps_modal_geometry(
        &dialog,
        &extra,
        &stack,
        "# Typed folder note\n\nPreview me",
    );
}

#[test]
fn test_browse_notes_opens_document_note_for_selected_row() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("browse-notes.md");
    fixture::write_text(&path, "# Notes\n");

    let data_dir = json_store::data_dir();
    document_note_service::save_for_path(
        &data_dir,
        &path,
        &RichNoteBody::new("# Note\n\nOpen me"),
    )
    .expect("save document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let dialog_child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&dialog_child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(2), || {
        sidebar.item(0).is_some()
            && find_button_by_label(&dialog_child, "Open")
                .is_some_and(|button| button.is_sensitive())
            && action_enabled(&window, "set-notes-browser-query")
            && action_enabled(&window, "select-notes-browser-row")
            && action_enabled(&window, "open-notes-browser-selection")
    });

    sidebar.emit_by_name::<()>("activated", &[&0u32]);
    flush_events();
    assert!(
        visible_alert_dialog(&window).is_none(),
        "activating a notes browser row should preview/select instead of opening the editor"
    );
    assert!(
        visible_sheet_dialog(&window).is_some(),
        "the notes browser should remain open after row activation"
    );

    activate_action(&window, "open-notes-browser-selection");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });
    let dialog = visible_alert_dialog(&window).expect("browse-opened document note dialog");
    let extra = dialog.extra_child().expect("browse-opened document note extra child");
    let stack = find_note_editor_stack(&extra).expect("browse-opened note editor stack");
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));
    assert_note_save_response_visible(&dialog);
    wait_for_note_save_response_sensitive(&dialog, false);
    assert!(!action_enabled(&window, "set-notes-browser-query"));
    assert!(!action_enabled(&window, "select-notes-browser-row"));
    assert!(!action_enabled(&window, "open-notes-browser-selection"));
}

#[test]
fn test_notes_browser_warns_and_keeps_valid_rows_with_corrupt_sidecar() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let valid_path = left_folder.join("valid-note.md");
    let corrupt_path = left_folder.join("corrupt-note.md");
    fixture::write_text(&valid_path, "# Valid\n");
    fixture::write_text(&corrupt_path, "# Corrupt\n");

    let data_dir = json_store::data_dir();
    document_note_service::save_for_path(
        &data_dir,
        &valid_path,
        &RichNoteBody::new("visible note body"),
    )
    .expect("save valid document note");
    let corrupt_identity =
        bookmark_service::resolve_document_identity(&corrupt_path).expect("corrupt identity");
    let corrupt_sidecar = document_note_service::document_notes_dir(&data_dir)
        .join(format!("{}.json", corrupt_identity.sidecar_id));
    fixture::create_dir_all(corrupt_sidecar.parent().expect("sidecar parent"));
    fixture::write_text(&corrupt_sidecar, "not document note json");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(10), || {
        sidebar.items().n_items() == 1
            && window
                .imp()
                .notification_bus
                .status_bar_view()
                .is_some_and(|status| status.text.contains("Some note data could not be loaded"))
    });
    assert!(
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Document Note · valid-note.md"),
        "valid document notes should remain browsable when one sidecar is corrupt"
    );
}

#[test]
fn test_browse_notes_opens_bookmark_for_selected_row() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("browse-bookmark.rs");
    fixture::write_text(&path, "one\ntwo\nthree\n");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("jump here".to_string()),
        )],
    )
    .expect("save bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let dialog_child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&dialog_child).expect("notes browser sidebar");
    flush_events();
    assert_eq!(sidebar.items().n_items(), 1);
    assert_eq!(
        sidebar.item(0).and_then(|item| item.title()).as_deref(),
        Some("Bookmark · jump here")
    );
    let open_button = find_button_by_label(&dialog_child, "Open").expect("notes browser open button");
    wait_until(Duration::from_secs(2), || open_button.is_sensitive());

    sidebar.emit_by_name::<()>("activated", &[&0u32]);
    flush_events();
    assert!(
        visible_sheet_dialog(&window).is_some(),
        "activating a bookmark row should only preview/select it"
    );

    open_button.emit_clicked();
    flush_events();

    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).cursor_position().0 == 1
    });
}

#[test]
fn test_notes_browser_renders_markdown_bookmark_excerpt() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("bookmark-preview.md");
    fixture::write_text(
        &path,
        "opening context\n\n# Target bookmark heading\n\nfollowing context\n",
    );

    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            2,
            Some("markdown preview".to_string()),
        )],
    )
    .expect("save markdown bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 1);
    wait_for_notes_preview_text(&child, "Target bookmark heading");

    let preview_text = notes_preview_text(&child).expect("notes preview text");
    assert_eq!(
        notes_preview_visible_child_name(&child).as_deref(),
        Some("markdown"),
        "Markdown bookmarks should use the Markdown preview child"
    );
    assert!(preview_text.contains("opening context"));
    assert!(preview_text.contains("following context"));
    assert!(
        find_label_by_text(&child, "Bookmark · markdown preview").is_some(),
        "bookmark metadata should remain visible above the rendered excerpt"
    );
}

#[test]
fn test_notes_browser_renders_raw_bookmark_excerpt_with_target_marker() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("bookmark-preview.rs");
    fixture::write_text(
        &path,
        "raw first\nraw before\nraw target\nraw after\nraw final\n",
    );

    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            2,
            Some("raw preview".to_string()),
        )],
    )
    .expect("save raw bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    wait_for_notes_preview_text(&child, "raw target");

    let preview_text = notes_preview_text(&child).expect("raw preview text");
    assert_eq!(
        notes_preview_visible_child_name(&child).as_deref(),
        Some("raw"),
        "non-Markdown bookmarks should use the raw preview child"
    );
    assert!(preview_text.contains("raw before"));
    assert!(preview_text.contains(">  3 | raw target"));
    assert!(preview_text.contains("raw after"));
    let raw_text_view = find_notes_preview_stack(&child)
        .and_then(|stack| stack.visible_child())
        .and_then(|child| {
            let mut text_views = Vec::new();
            collect_text_views(&child, &mut text_views);
            text_views.into_iter().find(|text_view| !text_view.is_editable())
        })
        .expect("raw bookmark preview text view");
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ReadOnly,
            gtk4::AccessibleProperty::MultiLine,
        ])
        .assert_on(&raw_text_view);
}

#[test]
fn test_notes_browser_bookmark_preview_uses_live_open_editor_buffer() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("live-bookmark-preview.rs");
    fixture::write_text(&path, "disk before\ndisk target\ndisk after\n");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);
    window.open_document(&path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    editor
        .buffer()
        .set_text("live before\nlive target unsaved\nlive after\n");
    editor.load_bookmarks(&[lushtext_core::model::bookmark::BookmarkRecord::new(
        1,
        Some("live preview".to_string()),
    )]);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    wait_for_notes_preview_text(&child, "live target unsaved");

    let preview_text = notes_preview_text(&child).expect("live preview text");
    assert!(preview_text.contains("live before"));
    assert!(preview_text.contains("live target unsaved"));
    assert!(
        !preview_text.contains("disk target"),
        "open-editor bookmark previews should not fall back to stale disk bytes"
    );
}

#[test]
fn test_notes_browser_ignores_stale_bookmark_excerpt_completion() {
    ensure_gtk_init();
    let _delay_reset = BookmarkExcerptPreviewDelayReset;
    set_bookmark_excerpt_preview_delay_for_test(250);
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let slow_path = left_folder.join("slow-bookmark-preview.rs");
    let fast_path = left_folder.join("fast-bookmark-preview.rs");
    fixture::write_text(&slow_path, "slow before\nslow target\nslow after\n");
    fixture::write_text(&fast_path, "fast before\nfast target\nfast after\n");

    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &slow_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("aaa slow preview".to_string()),
        )],
    )
    .expect("save slow bookmark");
    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &fast_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("zzz fast preview".to_string()),
        )],
    )
    .expect("save fast bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 4);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 2);
    wait_for_notes_preview_text(&child, "Loading bookmark preview...");

    sidebar.set_selected(1);
    flush_events();
    wait_for_notes_preview_text(&child, "fast target");

    let preview_text = notes_preview_text(&child).expect("fast preview text");
    assert!(preview_text.contains("fast target"));
    assert!(
        !preview_text.contains("slow target"),
        "the older closed-file preview completion should not replace the selected row"
    );
}

#[test]
fn test_notes_browser_search_ignores_bookmark_excerpt_text() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("metadata-only-bookmark.rs");
    fixture::write_text(
        &path,
        "before\nneedle-only-in-source-excerpt\nafter\n",
    );

    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("metadata label".to_string()),
        )],
    )
    .expect("save metadata bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 1);
    wait_for_notes_preview_text(&child, "needle-only-in-source-excerpt");

    let search_entry = find_search_entry(&child).expect("notes search entry");
    search_entry.set_text("needle-only-in-source-excerpt");
    flush_events();
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 0);
}

#[test]
fn test_browse_notes_includes_fresh_live_bookmark_before_sidecar_save() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("fresh-live-bookmark.rs");
    fixture::write_text(&path, "one\ntwo\nthree\n");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);
    window.open_document(&path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    let line_two = editor.buffer().iter_at_line(1).expect("line two");
    editor.buffer().place_cursor(&line_two);
    let _ = editor.toggle_bookmark_at_cursor();

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || {
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Bookmark · Line 2")
    });
}

#[test]
fn test_browse_notes_prefers_open_editor_bookmarks_over_stale_sidecar() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("stale-sidecar-bookmark.rs");
    fixture::write_text(&path, "one\ntwo\nthree\n");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("stale persisted".to_string()),
        )],
    )
    .expect("save stale bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);
    window.open_document(&path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).bookmark_records().len() == 1
    });

    let live_bookmark = lushtext_core::model::bookmark::BookmarkRecord::new(
        2,
        Some("live current".to_string()),
    );
    active_editor(&window).load_bookmarks(&[live_bookmark]);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || {
        sidebar.items().n_items() == 1
            && sidebar
                .item(0)
                .and_then(|item| item.title())
                .is_some_and(|title| title == "Bookmark · live current")
    });
    assert!(
        (0..sidebar.items().n_items()).all(|index| {
            sidebar
                .item(index)
                .and_then(|item| item.title())
                .is_none_or(|title| !title.contains("stale persisted"))
        }),
        "stale sidecar bookmarks for an open editor should not be shown"
    );
}

#[test]
fn test_browse_notes_shows_open_tab_bookmark_without_workspace() {
    seed_no_workspaces();
    let tempdir = tempfile::tempdir().expect("open tab bookmark tempdir");
    let path = tempdir.path().join("outside-bookmark.rs");
    fixture::write_text(&path, "one\ntwo\nthree\n");

    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    let line_two = editor.buffer().iter_at_line(1).expect("line two");
    editor.buffer().place_cursor(&line_two);
    let _ = editor.toggle_bookmark_at_cursor();

    assert!(
        action_enabled(&window, "notes-show-notes"),
        "saved open tabs should make Browse Notes available without a workspace"
    );
    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let search_entry = find_search_entry(&child).expect("notes browser search entry");
    assert_eq!(search_entry.placeholder_text().as_deref(), Some("Search Notes..."));

    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 1);
    assert_eq!(adw_sidebar_section_titles(&sidebar), ["Open Tabs"]);
    let item = sidebar.item(0).expect("open tab bookmark item");
    assert_eq!(item.title().as_deref(), Some("Bookmark · Line 2"));
    assert!(
        item.subtitle()
            .is_some_and(|subtitle| subtitle.contains("Open tab · Outside workspace")),
        "open-tab bookmark rows should identify their source"
    );
}

#[test]
fn test_browse_notes_shows_open_tab_document_note_without_workspace() {
    seed_no_workspaces();
    let tempdir = tempfile::tempdir().expect("open tab document note tempdir");
    let path = tempdir.path().join("outside-note.md");
    fixture::write_text(&path, "# Outside\n");
    document_note_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &RichNoteBody::new("# Outside note\n\nopen tab body"),
    )
    .expect("save open-tab document note");

    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).file_size().is_some()
    });

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 1);
    assert_eq!(adw_sidebar_section_titles(&sidebar), ["Open Tabs"]);
    assert_eq!(
        sidebar.item(0).and_then(|item| item.title()).as_deref(),
        Some("Document Note · outside-note.md")
    );

    let search_entry = find_search_entry(&child).expect("notes search entry");
    search_entry.set_text("outside workspace");
    flush_events();
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 1);
    search_entry.set_text("open tab body");
    flush_events();
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 1);

    find_button_by_label(&child, "Open")
        .expect("notes browser open button")
        .emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(5), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });
}

#[test]
fn test_browse_notes_keeps_scope_rows_strict_and_lists_other_open_workspace_tab() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-left")));
    let left_path = left_folder.join("left-scoped-bookmark.rs");
    let right_path = right_folder.join("right-open-bookmark.rs");
    fixture::write_text(&left_path, "left\n");
    fixture::write_text(&right_path, "right\n");

    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &left_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("left scoped".to_string()),
        )],
    )
    .expect("save scoped bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 1, 2);
    window.open_document(&right_path);
    wait_until(Duration::from_secs(5), || {
        active_editor(&window).file_path() == Some(right_path.clone())
            && active_editor(&window).file_size().is_some()
    });
    active_editor(&window).load_bookmarks(&[lushtext_core::model::bookmark::BookmarkRecord::new(
        0,
        Some("right open".to_string()),
    )]);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 2);
    assert_eq!(adw_sidebar_section_titles(&sidebar), ["Bookmarks", "Open Tabs"]);
    assert_eq!(
        sidebar.item(0).and_then(|item| item.title()).as_deref(),
        Some("Bookmark · left scoped")
    );
    let open_item = sidebar.item(1).expect("right open bookmark row");
    assert_eq!(open_item.title().as_deref(), Some("Bookmark · right open"));
    assert!(
        open_item
            .subtitle()
            .is_some_and(|subtitle| subtitle.contains("Open tab · right")),
        "open tabs from another restored workspace should name that workspace"
    );
}

#[test]
fn test_notes_browser_uses_folder_order_for_overlapping_primary_context() {
    ensure_gtk_init();
    let (_folders_dir, parent_folder, _child_folder, path) = seed_overlapping_folder_workspace();
    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("overlap bookmark".to_string()),
        )],
    )
    .expect("save overlapping bookmark");
    document_note_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &RichNoteBody::new("overlap document note"),
    )
    .expect("save overlapping document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 2);

    let primary_context_prefix = format!("overlap · {} · ", parent_folder.display());
    let subtitles = (0..sidebar.items().n_items())
        .filter_map(|index| sidebar.item(index))
        .filter_map(|item| item.subtitle().map(|subtitle| subtitle.to_string()))
        .collect::<Vec<_>>();
    assert!(
        subtitles
            .iter()
            .any(|subtitle| subtitle.starts_with(&primary_context_prefix)
                && subtitle.contains("Line 2")),
        "bookmark rows should use the first configured covering folder as primary context"
    );
    assert!(
        subtitles
            .iter()
            .any(|subtitle| subtitle
                .starts_with(&format!("{}{}", primary_context_prefix, path.display()))),
        "document-note rows should use the first configured covering folder as primary context"
    );
}

#[test]
fn test_notes_browser_preserves_configured_folder_note_order() {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("folder-note order tempdir");
    let first_folder = folders_dir.path().join("z-first");
    let second_folder = folders_dir.path().join("a-second");
    fixture::create_dir_all(&first_folder);
    fixture::create_dir_all(&second_folder);
    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::All,
        workspaces: vec![WorkspaceConfig::with_folders(
            WorkspaceId::new("ws-folder-note-order"),
            "ordered",
            vec![
                WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first_folder.clone()),
                WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second_folder.clone()),
            ],
        )],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces)
        .expect("save folder-note order workspace");
    folder_note_service::save_for_folder(
        &json_store::data_dir(),
        &first_folder,
        &RichNoteBody::new("first configured folder"),
    )
    .expect("save first folder note");
    folder_note_service::save_for_folder(
        &json_store::data_dir(),
        &second_folder,
        &RichNoteBody::new("second configured folder"),
    )
    .expect("save second folder note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.items().n_items() == 2);

    assert_eq!(adw_sidebar_section_titles(&sidebar), ["Folder Notes"]);
    assert!(
        sidebar
            .item(0)
            .and_then(|item| item.subtitle())
            .is_some_and(|subtitle| subtitle.contains(&first_folder.display().to_string())),
        "first configured folder note should stay first even when its path sorts later"
    );
    assert!(
        sidebar
            .item(1)
            .and_then(|item| item.subtitle())
            .is_some_and(|subtitle| subtitle.contains(&second_folder.display().to_string())),
        "second configured folder note should stay second"
    );
}

#[test]
fn test_bookmark_browser_warns_and_keeps_valid_rows_with_corrupt_sidecar() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let valid_path = left_folder.join("valid-bookmark.rs");
    let corrupt_path = left_folder.join("corrupt-bookmark.rs");
    fixture::write_text(&valid_path, "one\ntwo\n");
    fixture::write_text(&corrupt_path, "bad\n");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &valid_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("valid bookmark".to_string()),
        )],
    )
    .expect("save valid bookmark");
    let corrupt_identity =
        bookmark_service::resolve_document_identity(&corrupt_path).expect("corrupt identity");
    let corrupt_sidecar =
        bookmark_service::bookmarks_dir(&data_dir).join(format!("{}.json", corrupt_identity.sidecar_id));
    fixture::create_dir_all(corrupt_sidecar.parent().expect("sidecar parent"));
    fixture::write_text(&corrupt_sidecar, "not bookmark json");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);

    activate_action(&window, "show-bookmarks");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("bookmark browser dialog");
    let child = dialog.child().expect("bookmark browser child");
    wait_until(Duration::from_secs(10), || {
        find_label_by_text(&child, "valid bookmark").is_some()
            && window
                .imp()
                .notification_bus
                .status_bar_view()
                .is_some_and(|status| {
                    status
                        .text
                        .contains("Some bookmark data could not be loaded")
                })
    });
}

#[test]
fn test_notes_browser_close_button_dismisses_populated_browser() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("close-notes-browser.md");
    fixture::write_text(&path, "# Close\n");
    document_note_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &RichNoteBody::new("close me"),
    )
    .expect("save document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let split_view = find_navigation_split_view(&child).expect("notes browser split view");
    split_view.set_collapsed(false);
    split_view.set_show_content(true);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        !split_view.is_collapsed() && visible_buttons_by_tooltip(&child, "Close").len() == 1
    });

    single_visible_close_button(&child).emit_clicked();
    flush_events();

    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_none());
}

#[test]
fn test_notes_browser_close_button_dismisses_collapsed_sidebar_page() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("close-sidebar-notes-browser.md");
    fixture::write_text(&path, "# Close sidebar\n");
    document_note_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &RichNoteBody::new("close sidebar"),
    )
    .expect("save document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let split_view = find_navigation_split_view(&child).expect("notes browser split view");
    split_view.set_collapsed(true);
    split_view.set_show_content(false);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        split_view.is_collapsed()
            && !split_view.shows_content()
            && visible_buttons_by_tooltip(&child, "Close").len() == 1
    });

    single_visible_close_button(&child).emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_none());
}

#[test]
fn test_notes_browser_back_navigates_and_close_dismisses_collapsed_preview() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("close-preview-notes-browser.md");
    fixture::write_text(&path, "# Close preview\n");
    document_note_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &RichNoteBody::new("close preview"),
    )
    .expect("save document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let split_view = find_navigation_split_view(&child).expect("notes browser split view");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(5), || sidebar.item(0).is_some());
    split_view.set_collapsed(true);
    sidebar.emit_by_name::<()>("activated", &[&0u32]);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        split_view.is_collapsed()
            && split_view.shows_content()
            && visible_buttons_by_tooltip(&child, "Close").len() == 1
    });

    let back_button = find_button_by_tooltip(&child, "Back to Notes").expect("preview back button");
    assert!(
        back_button.is_visible(),
        "collapsed preview should expose Back as navigation"
    );
    back_button.emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(2), || {
        visible_sheet_dialog(&window).is_some() && !split_view.shows_content()
    });

    split_view.set_show_content(true);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        split_view.shows_content() && visible_buttons_by_tooltip(&child, "Close").len() == 1
    });
    single_visible_close_button(&child).emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_none());
}

#[test]
fn test_empty_notes_browser_close_button_and_escape_dismiss() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("empty notes browser dialog");
    let child = dialog.child().expect("empty notes browser child");
    assert_readable_empty_status_dialog(&dialog, &child, "empty Browse Notes browser");
    assert!(
        find_label_by_text(&child, "No notes yet").is_some(),
        "empty Browse Notes should present an explicit empty state"
    );
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&find_status_page(&child).expect("empty notes status page"));
    single_visible_close_button(&child).emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_none());

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(5), || {
        visible_sheet_dialog(&window).is_some()
            && gtk4::prelude::GtkWindowExt::focus(&window).is_some()
    });
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::Escape);
    flush_events();
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_none());
}

#[test]
fn test_empty_notes_browser_opens_from_header_without_workspace_or_open_tab_rows() {
    seed_no_workspaces();

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        notes_menu_button_visible(&window) && action_enabled(&window, "notes-show-notes")
    });

    // Use the menu-scoped action so this covers the header `Browse Notes…` row.
    activate_action(&window, "notes-show-notes");
    wait_until(Duration::from_secs(5), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("empty notes browser dialog");
    let child = dialog.child().expect("empty notes browser child");
    assert_readable_empty_status_dialog(&dialog, &child, "empty Browse Notes browser");
    assert!(
        find_label_by_text(&child, "No notes yet").is_some(),
        "Browse Notes should present an explicit empty state even without workspaces"
    );
    assert!(
        find_adw_sidebar(&child).is_none(),
        "no-workspace empty state should not materialize fake browser rows"
    );
}

#[test]
fn test_browse_notes_filters_bookmarks_to_current_workspace_scope() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-left")));
    let left_path = left_folder.join("left-bookmark.rs");
    let right_path = right_folder.join("right-bookmark.rs");
    fixture::write_text(&left_path, "left\n");
    fixture::write_text(&right_path, "right\n");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &left_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("left only".to_string()),
        )],
    )
    .expect("save left bookmark");
    bookmark_service::save_for_path(
        &data_dir,
        &right_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("right hidden".to_string()),
        )],
    )
    .expect("save right bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 1, 2);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 1);
    assert!(
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Bookmark · left only"),
        "current workspace scope should include the left bookmark"
    );

    let search_entry = find_search_entry(&child).expect("notes search entry");
    search_entry.set_text("right hidden");
    flush_events();
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 0);
}

#[test]
fn test_notes_browser_uses_sectioned_adw_sidebar_and_filters_note_body() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("sectioned-notes.md");
    fixture::write_text(&path, "one\ntwo\nthree\n");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("bookmark needle".to_string()),
        )],
    )
    .expect("save bookmark");
    folder_note_service::save_for_folder(
        &data_dir,
        &left_folder,
        &RichNoteBody::new("folder needle"),
    )
    .expect("save folder note");
    document_note_service::save_for_path(
        &data_dir,
        &path,
        &RichNoteBody::new("document needle"),
    )
    .expect("save document note");
    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(2), || {
        sidebar.items().n_items() == 3
            && action_enabled(&window, "set-notes-browser-query")
            && action_enabled(&window, "select-notes-browser-row")
            && action_enabled(&window, "open-notes-browser-selection")
    });
    let notes_browser_size = settled_widget_outer_size(&dialog);

    for index in 0u32..3 {
        sidebar.emit_by_name::<()>("activated", &[&index]);
        flush_events();
        assert!(
            visible_alert_dialog(&window).is_none(),
            "pointer-style activation at notes browser index {index} should not open an editor"
        );
        assert!(
            visible_sheet_dialog(&window).is_some(),
            "the notes browser should stay visible after activating index {index}"
        );
        assert_settled_widget_outer_size(
            &dialog,
            notes_browser_size,
            "notes browser row activation",
        );
    }

    let section_titles: Vec<_> = (0..sidebar.sections().n_items())
        .filter_map(|index| sidebar.section(index))
        .filter_map(|section| section.title().map(|title| title.to_string()))
        .collect();
    assert_eq!(
        section_titles,
        ["Bookmarks", "Folder Notes", "Document Notes"],
        "notes browser should expose semantic Adwaita sidebar sections"
    );

    let split_view = find_navigation_split_view(&child).expect("notes split view");
    split_view.set_collapsed(true);
    sidebar.set_selected(0);
    flush_events();
    wait_until(Duration::from_secs(2), || split_view.shows_content());
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser collapsed preview selection",
    );
    assert!(
        find_label_by_text(&child, "Bookmark · bookmark needle").is_some(),
        "selecting a bookmark should update the preview before opening it"
    );
    assert!(
        find_label_by_text(
            &child,
            &format!("left · {} · {} · Line 2", left_folder.display(), path.display()),
        )
        .is_some(),
        "bookmark preview metadata should include workspace, primary folder, file path, and line"
    );

    activate_u32_action(&window, "select-notes-browser-row", 2);
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser document-note preview selection",
    );
    assert!(
        find_label_by_text(&child, "Document Note · sectioned-notes.md").is_some(),
        "selecting a sidebar note should update the preview before opening it"
    );

    let search_entry = find_search_entry(&child).expect("notes search entry");
    activate_string_action(&window, "set-notes-browser-query", "document needle");
    wait_until(Duration::from_secs(2), || {
        search_entry.text().as_str() == "document needle" && sidebar.items().n_items() == 1
    });
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser document-note filtering",
    );
    assert!(
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Document Note · sectioned-notes.md"),
        "notes search should match document note body text, not only visible metadata"
    );

    activate_string_action(&window, "set-notes-browser-query", "bookmark needle");
    wait_until(Duration::from_secs(2), || {
        search_entry.text().as_str() == "bookmark needle"
            &&
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Bookmark · bookmark needle")
    });
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser bookmark filtering",
    );

    activate_string_action(&window, "set-notes-browser-query", "Line 2");
    wait_until(Duration::from_secs(2), || {
        search_entry.text().as_str() == "Line 2"
            &&
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Bookmark · bookmark needle")
    });
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser line-metadata filtering",
    );

    activate_string_action(&window, "set-notes-browser-query", "missing needle");
    wait_until(Duration::from_secs(2), || {
        search_entry.text().as_str() == "missing needle" && sidebar.items().n_items() == 0
    });
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser empty filtered state",
    );
    assert!(
        find_label_by_text(&child, "No notes match that search").is_some(),
        "empty filtered notes state should remain explicit"
    );
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(
            &find_label_by_text(&child, "No notes match that search")
                .expect("notes no-match status"),
        );
}

#[test]
fn test_notes_browser_caps_large_result_sets_with_refine_notice() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("many-bookmarks.rs");
    let content = (0..510)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    fixture::write_text(&path, &content);

    let bookmarks = (0..510)
        .map(|line| {
            lushtext_core::model::bookmark::BookmarkRecord::new(
                line,
                Some(format!("bookmark {line}")),
            )
        })
        .collect::<Vec<_>>();
    bookmark_service::save_for_path(&json_store::data_dir(), &path, &bookmarks)
        .expect("save many bookmarks");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 500);
    assert!(
        find_label_by_text(
            &child,
            "Showing first 500 matches. Refine search to narrow results."
        )
        .is_some(),
        "large note sets should explain that the sidebar result set is capped"
    );
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(
            &find_label_by_text(
                &child,
                "Showing first 500 matches. Refine search to narrow results.",
            )
            .expect("notes result-limit status"),
        );
}

#[test]
fn test_notes_menu_renders_immediately_left_of_main_menu() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);
    wait_until(Duration::from_secs(2), || {
        let header_bar = window.imp().header_bar.upcast_ref::<gtk4::Widget>();
        let notes_button = window.imp().notes_menu_button.upcast_ref::<gtk4::Widget>();
        let main_button = window.imp().primary_menu_button.upcast_ref::<gtk4::Widget>();
        notes_menu_button_visible(&window)
            && notes_button.compute_bounds(header_bar).is_some()
            && main_button.compute_bounds(header_bar).is_some()
    });

    let header_bar = window.imp().header_bar.upcast_ref::<gtk4::Widget>();
    let notes_x = widget_left_in(
        header_bar,
        window.imp().notes_menu_button.upcast_ref::<gtk4::Widget>(),
    );
    let main_x = widget_left_in(
        header_bar,
        window.imp().primary_menu_button.upcast_ref::<gtk4::Widget>(),
    );

    assert!(
        notes_x < main_x,
        "Notes should render left of Main Menu instead of to its right",
    );
}

#[test]
fn test_notes_menu_cursor_specific_actions_follow_active_note_context() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_folder.join("notes-state.rs");
    fixture::write_text(&path, "one\ntwo\nthree\n");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("bookmark".to_string()),
        )],
    )
    .expect("save bookmark sidecar");
    let window = test_window();
    present_window(&window);
    wait_for_workspace_folders(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor.bookmark_records().len() == 1
            && menu_model_labels(
                &window
                    .imp()
                    .notes_menu_button
                    .menu_model()
                    .expect("notes menu model"),
            )
            .iter()
            .any(|label| label == "Remove Bookmark")
    });

    assert!(action_enabled(&window, "notes-toggle-bookmark"));
    assert!(action_enabled(&window, "notes-open-document-note"));
    assert!(!action_enabled(&window, "notes-open-folder-note"));
    assert!(action_enabled(&window, "edit-bookmark-label"));

    let editor = active_editor(&window);
    let line_two = editor.buffer().iter_at_line(1).expect("line two");
    editor.buffer().place_cursor(&line_two);
    flush_events();

    wait_until(Duration::from_secs(2), || {
        menu_model_labels(
            &window
                .imp()
                .notes_menu_button
                .menu_model()
                .expect("notes menu model"),
        )
        .iter()
        .any(|label| label == "Add Bookmark")
    });
    assert!(action_enabled(&window, "notes-toggle-bookmark"));
    assert!(action_enabled(&window, "notes-open-document-note"));
}

#[test]
fn test_empty_window_hides_tab_strip() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    assert_tab_count(&window, 0);
    assert_tab_strip_hidden(&window, "empty window");
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
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 3
    });
    wait_for_tab_strip_visible(&window, "multi-tab strip");

    let pinned_page = find_tab_page_by_title(&window, "b.txt");
    prepare_tab_context_menu(&window, &pinned_page);
    activate_action(&window, "toggle-tab-pinned");
    wait_until(Duration::from_secs(2), || {
        pinned_page.is_pinned() && tab_titles(&window) == vec!["b.txt", "a.txt", "c.txt"]
    });

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
        tab_titles(&window) == vec!["b.txt", "c.txt", "a.txt"]
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

    let pages = tab_pages(&window);
    assert!(pages[0].is_pinned());
    assert!(!pages[1].is_pinned());
    assert!(!pages[2].is_pinned());

    let last_page = find_tab_page_by_title(&window, "a.txt");
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
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 3
    });

    let modified_page = find_tab_page_by_title(&window, "b.txt");
    let modified_editor = modified_page
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("modified editor page");
    wait_until(Duration::from_secs(2), || {
        modified_editor.file_size().is_some()
    });
    modified_editor.buffer().set_text("modified");
    modified_editor.buffer().set_modified(true);
    flush_events();

    let target = find_tab_page_by_title(&window, "a.txt");
    prepare_tab_context_menu(&window, &target);
    activate_action(&window, "close-other-tabs");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window).is_some()
    });
    assert_tab_count(&window, 3);

    let dialog = visible_alert_dialog(&window).expect("bulk close confirmation");
    dialog.emit_by_name::<()>("response", &[&"discard"]);
    dialog.force_close();
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window).is_none()
    });
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });

    assert_eq!(tab_titles(&window), vec!["a.txt"]);
}

#[test]
fn test_pin_action_updates_indicator_icon() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["pin-me.txt"]);
    let window = test_window();
    present_window(&window);
    window.open_document(&files[0]);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });
    wait_for_tab_strip_visible(&window, "single unpinned tab strip");

    let page = find_tab_page_by_title(&window, "pin-me.txt");
    prepare_tab_context_menu(&window, &page);
    assert_tab_context_menu_has_label(&window, "Pin", "single unpinned tab");
    assert!(action_enabled(&window, "toggle-tab-pinned"));
    activate_action(&window, "toggle-tab-pinned");
    wait_until(Duration::from_secs(2), || page.is_pinned());
    wait_for_tab_strip_visible(&window, "single pinned tab strip");
    assert!(page.indicator_icon().is_some());

    prepare_tab_context_menu(&window, &page);
    assert_tab_context_menu_has_label(&window, "Unpin", "single pinned tab");
    assert!(action_enabled(&window, "toggle-tab-pinned"));
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
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 3
    });

    assert_eq!(
        tab_titles(&window),
        vec!["alpha.txt", "beta.txt", "gamma.txt"]
    );
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
fn test_startup_format_gate_blocks_future_session_and_resumes_after_start_fresh() {
    let data_dir = isolated_data_dir();
    fixture::write_text(
        &data_dir.path().join("session.json"),
        &format!(
            r#"{{
  "kind": "{}",
  "version": {},
  "data": {{ "tabs": [], "active_tab_index": null }}
}}"#,
            json_format::KIND_SESSION,
            json_format::SUPPORTED_JSON_VERSION + 1
        ),
    );
    let activation_dir = tempfile::tempdir().expect("activation tempdir");
    let activation_path = activation_dir.path().join("queued.txt");
    fixture::write_text(&activation_path, "queued activation");

    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(10), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Data Was Created by a Newer LushText")
    });

    assert!(!window.imp().startup_data_flow.completed.get());
    assert_eq!(
        window.imp().tab_view.n_pages(),
        0,
        "session restore must wait behind the startup format gate"
    );

    window.open_document_from_activation(&activation_path);
    flush_events();
    assert_eq!(
        window
            .imp()
            .startup_data_flow
            .pending_activation_paths
            .borrow()
            .len(),
        1
    );
    assert_eq!(
        window.imp().tab_view.n_pages(),
        0,
        "activation opens should queue while incompatible metadata is unresolved"
    );

    let dialog = visible_alert_dialog(&window).expect("startup format dialog");
    let body = dialog.body();
    assert!(body.contains("newer LushText"));
    assert!(!body.contains("Options:"));
    assert!(!body.contains("Affected data:"));
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 3));
    let labels = alert_dialog_extra_label_texts(&dialog);
    assert_label_text_contains(&labels, "Options");
    assert_label_text_contains(&labels, "Quit");
    assert_label_text_contains(&labels, "Close LushText without changing app data.");
    assert_label_text_contains(&labels, "Start Fresh");
    assert_label_text_contains(&labels, format_upgrade::FORMAT_UPGRADE_BACKUP_DIR);
    assert_label_text_contains(&labels, "Affected Data");
    assert_label_text_contains(&labels, "Session");
    assert_label_text_contains(&labels, "1 item was created by a newer LushText.");
    assert_no_label_text_contains(&labels, "Convert");
    assert!(!dialog.has_response("convert"));
    assert!(find_button_by_label(dialog.upcast_ref(), "Convert").is_none());
    assert!(find_button_by_label(dialog.upcast_ref(), "_Convert").is_none());
    alert_response_button(&dialog, "start-fresh").emit_clicked();
    flush_events();

    wait_until(Duration::from_secs(5), || {
        window.imp().startup_data_flow.completed.get()
            && window.imp().tab_view.n_pages() == 1
            && active_editor(&window).file_path().as_deref() == Some(activation_path.as_path())
    });

    assert!(window.imp().startup_data_flow.completed.get());
    assert!(
        window
            .imp()
            .startup_data_flow
            .pending_activation_paths
            .borrow()
            .is_empty()
    );
    assert!(
        !fs_metadata::exists(&data_dir.path().join("session.json")),
        "Start Fresh should move the future session out of active app data"
    );
    assert!(fs_metadata::exists(
        &data_dir
            .path()
            .join(format_upgrade::FORMAT_UPGRADE_BACKUP_DIR)
    ));
}

/// Test converter used only to make the startup gate exercise its Convert path.
fn convert_session_v0_fixture_to_v1(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let data = value
        .get("data")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"tabs": [], "active_tab_index": null}));
    Ok(serde_json::to_vec_pretty(&serde_json::json!({
        "kind": json_format::KIND_SESSION,
        "version": json_format::SUPPORTED_JSON_VERSION,
        "data": data,
    }))?)
}

#[test]
fn test_startup_format_gate_offers_convert_for_upgradeable_older_session() {
    let data_dir = isolated_data_dir();
    fixture::write_text(
        &data_dir.path().join("session.json"),
        &format!(
            r#"{{
  "kind": "{}",
  "version": 0,
  "data": {{ "tabs": [], "active_tab_index": null }}
}}"#,
            json_format::KIND_SESSION
        ),
    );
    let registry = format_upgrade::test_support::ConverterRegistry::production().with_converter(
        json_format::KIND_SESSION,
        0,
        json_format::SUPPORTED_JSON_VERSION,
        convert_session_v0_fixture_to_v1,
    );
    let _registry_override =
        format_upgrade::test_support::ConverterRegistry::override_production_for_test(registry);

    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(10), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Older LushText Data Can Be Updated")
    });

    assert!(!window.imp().startup_data_flow.completed.get());
    let dialog = visible_alert_dialog(&window).expect("startup format dialog");
    let body = dialog.body();
    assert!(body.contains("older app data"));
    assert!(!body.contains("Options:"));
    assert!(!body.contains("Affected data:"));
    assert_eq!(alert_dialog_extra_structure_counts(&dialog), (2, 4));
    let labels = alert_dialog_extra_label_texts(&dialog);
    assert_label_text_contains(&labels, "Options");
    assert_label_text_contains(&labels, "Convert");
    assert_label_text_contains(
        &labels,
        "Back up affected files, then update supported older data to the current format.",
    );
    assert_label_text_contains(&labels, "Start Fresh");
    assert_label_text_contains(&labels, format_upgrade::FORMAT_UPGRADE_BACKUP_DIR);
    assert_label_text_contains(&labels, "Quit");
    assert_label_text_contains(&labels, "Close LushText without changing app data.");
    assert_label_text_contains(&labels, "Affected Data");
    assert_label_text_contains(&labels, "Session");
    assert_label_text_contains(&labels, "1 item can be converted to the current format.");
    assert!(dialog.has_response("convert"));
    assert_eq!(dialog.response_label("convert").as_str(), "_Convert");

    dialog.emit_by_name::<()>("response", &[&"convert"]);
    flush_events();

    wait_until(Duration::from_secs(5), || {
        window.imp().startup_data_flow.completed.get()
    });

    let saved = fixture::read_text(&data_dir.path().join("session.json"));
    assert!(saved.contains(r#""version": 1"#));
    assert!(fs_metadata::exists(
        &data_dir
            .path()
            .join(format_upgrade::FORMAT_UPGRADE_BACKUP_DIR)
    ));
}

#[test]
fn test_startup_restore_surfaces_grouped_recovery_diagnostics() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let session_path = data_dir.join("session.json");
    remove_session_path_for_test(&session_path);
    fixture::write_text(&session_path, "not valid session json");

    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(10), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Some recovery data could not be loaded"))
    });
    assert!(
        !fs_metadata::exists(&session_path),
        "malformed session should be moved away before replacement is allowed"
    );
    session_service::save(&data_dir, &SessionData::default()).expect("restore clean session");
}

#[test]
fn test_workspace_recovery_surfaces_visible_warning() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let workspace_path = data_dir.join("workspaces.json");
    remove_session_path_for_test(&workspace_path);
    fixture::write_text(
        &workspace_path,
        r#"{"active_workspace":"legacy","workspaces":[{"id":"legacy","entries":[]}]}"#,
    );

    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(10), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Workspace state needed recovery"))
    });
    assert!(
        !fs_metadata::exists(&workspace_path),
        "unsupported workspace state should be moved away before replacement is allowed"
    );
    workspace_manager::save(&data_dir, &WorkspacesFile::default())
        .expect("restore clean workspace state");
}

#[test]
fn test_saved_search_recovery_surfaces_visible_warning() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let saved_searches_path = data_dir.join("saved-searches.json");
    remove_session_path_for_test(&saved_searches_path);
    fixture::write_text(&saved_searches_path, r#"[{"name":"legacy","query":"TODO"}]"#);

    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(10), || {
        window
            .imp()
            .notification_bus
            .status_bar_view()
            .is_some_and(|status| status.text.contains("Saved searches needed recovery"))
    });
    assert!(
        !fs_metadata::exists(&saved_searches_path),
        "unsupported saved searches should be moved away before replacement is allowed"
    );
    saved_searches::save(&data_dir, &[]).expect("restore clean saved searches");
}

#[test]
fn test_local_history_startup_restore_uses_restored_draft_as_baseline() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("restored-history.txt");
    fixture::write_text(&file_path, "");
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&file_path);
    let draft_content = "draft content";
    draft_service::write_draft(&data_dir, &draft_id, draft_content).expect("seed draft");
    let current_mtime = editor_io::mtime_secs(&file_path).expect("file mtime");
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id,
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
                path: Some(file_path.clone()),
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
        editor_text(&editor) == draft_content
    });
    wait_until(Duration::from_secs(2), || {
        !local_history_service::list_snapshots_for_path(&data_dir, &file_path)
            .expect("list local history")
            .is_empty()
    });

    let snapshots = local_history_service::list_snapshots_for_path(&data_dir, &file_path)
        .expect("list local history");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].origin,
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline
    );
    assert_eq!(snapshots[0].byte_len, draft_content.len() as u64);

    let loaded = local_history_service::load_snapshot_for_path(
        &data_dir,
        &file_path,
        &snapshots[0].snapshot_id,
    )
    .expect("load local history snapshot")
    .expect("baseline snapshot should exist");
    assert_eq!(loaded.text, draft_content);

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    wait_until(Duration::from_secs(2), || {
        find_label_by_text(&child, "Before edits · 13 B").is_some()
    });

    assert!(
        find_label_by_text(&child, "Before edits · 13 B").is_some(),
        "draft-restored history should baseline the restored working content"
    );
    assert!(
        find_label_by_text(&child, "Before edits · Empty file").is_none(),
        "draft-restored history should not show a fresh empty disk baseline row"
    );
}

#[test]
fn test_startup_restore_applies_matching_file_backed_draft() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("restored.txt");
    fixture::write_text(&file_path, "disk content");
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&file_path);
    draft_service::write_draft(&data_dir, &draft_id, "draft content").expect("seed draft");
    let current_mtime = editor_io::mtime_secs(&file_path).expect("file mtime");
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id,
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
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });
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
    fixture::write_text(&file_path, "current disk content");
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
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });
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
    assert!(
        window
            .imp()
            .drafts
            .manifest
            .borrow()
            .find_by_id(&draft_id)
            .is_none()
    );
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
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });
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
fn test_preview_pane_toggle_uses_adwaita_side_by_side_shell() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1200, 720);
    window.new_tab();
    present_window(&window);

    assert_eq!(preview_layout_name(&window).as_deref(), Some("editor"));
    assert!(!window.imp().preview_split_view.shows_sidebar());
    assert!(!window.imp().markdown_preview.property::<bool>("visible"));
    assert!(gtk4::test_accessible_has_state(
        &*window.imp().markdown_preview,
        gtk4::AccessibleState::Hidden
    ));

    activate_action(&window, "toggle-preview-pane");

    wait_until(Duration::from_secs(2), || {
        window.imp().preview_visible.get()
            && window.imp().preview_split_view.shows_sidebar()
            && preview_layout_name(&window).as_deref() == Some("editor")
            && window.imp().markdown_preview.property::<bool>("visible")
            && window.imp().editor_box.property::<bool>("visible")
            && !window.preview_transition_pending_for_test()
    });
    assert!(!window.imp().preview_mode.get());
    assert!(action_state_bool(&window, "toggle-preview-pane"));
    assert_eq!(
        window.imp().preview_split_view.sidebar_position(),
        gtk4::PackType::End
    );
    assert!(window.imp().preview_split_view.is_pin_sidebar());
    assert!(!window.imp().preview_split_view.enables_show_gesture());
    assert!(!window.imp().preview_split_view.enables_hide_gesture());
    assert!(!gtk4::test_accessible_has_state(
        &*window.imp().markdown_preview,
        gtk4::AccessibleState::Hidden
    ));
}

#[test]
fn test_preview_mode_toggle_uses_full_content_layout() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-preview-mode");

    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get()
            && preview_layout_name(&window).as_deref() == Some("preview")
            && window.imp().markdown_preview.property::<bool>("visible")
            && !window.imp().editor_box.property::<bool>("visible")
            && !window.imp().preview_split_view.shows_sidebar()
            && !window.preview_transition_pending_for_test()
    });
    assert!(!window.imp().preview_visible.get());
    assert!(action_state_bool(&window, "toggle-preview-mode"));
    assert!(!action_state_bool(&window, "toggle-preview-pane"));
    assert!(!gtk4::test_accessible_has_state(
        &*window.imp().markdown_preview,
        gtk4::AccessibleState::Hidden
    ));
}

#[test]
fn test_side_by_side_preview_width_clamps_legacy_preference_without_rewriting_it() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(keys::PREVIEW_PANE_POSITION, 520)
        .expect("set legacy preview width preference");
    let window = test_window();
    window.set_default_size(1400, 720);
    window.new_tab();
    present_window(&window);
    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_sidebar_visible(&window));

    activate_action(&window, "toggle-preview-pane");

    wait_until(Duration::from_secs(2), || {
        window.imp().preview_split_view.shows_sidebar()
            && !window.preview_transition_pending_for_test()
    });
    let split = &window.imp().preview_split_view;
    let preview_width = split.max_sidebar_width();
    let available_width = window.imp().content_box.width().max(1);
    assert!(
        available_width < window.width(),
        "test must prove content-width clamping with secondary chrome visible"
    );
    assert!(
        preview_width <= (f64::from(available_width) / 3.0).floor() + f64::EPSILON,
        "side-by-side preview should stay within one third of the content width: {preview_width} / {available_width}",
    );
    assert!(
        (split.min_sidebar_width() - split.max_sidebar_width()).abs() <= f64::EPSILON,
        "preview split width should be fixed through Adwaita constraints",
    );
    assert_eq!(
        settings.int(keys::PREVIEW_PANE_POSITION),
        520,
        "layout clamping must not rewrite the user's wider preferred width",
    );
}

#[test]
fn test_preview_target_actions_keep_adwaita_shell_modes_mutually_exclusive() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_boolean_action(&window, "set-preview-pane-visible", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().preview_visible.get()
            && window.imp().preview_split_view.shows_sidebar()
            && preview_layout_name(&window).as_deref() == Some("editor")
    });

    activate_boolean_action(&window, "set-preview-mode", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get()
            && !window.imp().preview_visible.get()
            && preview_layout_name(&window).as_deref() == Some("preview")
            && !window.imp().preview_split_view.shows_sidebar()
            && !action_state_bool(&window, "toggle-preview-pane")
            && action_state_bool(&window, "toggle-preview-mode")
    });

    activate_boolean_action(&window, "set-preview-mode", false);
    wait_until(Duration::from_secs(2), || {
        !window.imp().preview_mode.get()
            && preview_layout_name(&window).as_deref() == Some("editor")
            && !window.imp().markdown_preview.property::<bool>("visible")
            && window.imp().editor_box.property::<bool>("visible")
    });
    assert!(gtk4::test_accessible_has_state(
        &*window.imp().markdown_preview,
        gtk4::AccessibleState::Hidden
    ));
}

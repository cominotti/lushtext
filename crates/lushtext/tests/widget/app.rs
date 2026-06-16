// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for LushtextApplication.

use crate::common::{
    ensure_gtk_init, fixture, flush_events, fs_metadata, fs_read, present_window, wait_until,
};
use gio::prelude::*;
use glib::prelude::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{IconTheme, gdk};
use gtk4::prelude::{GtkApplicationExt, TextBufferExt};
use lushtext_core::app::LushtextApplication;
use lushtext_core::config;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::services::{editor_io, json_store, session_service};
use lushtext_core::ui::automation::app_snapshot;
use lushtext_core::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use lushtext_core::ui::window::LushtextWindow;
use sourceview5::StyleSchemeManager;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[test]
fn test_new() {
    ensure_gtk_init();
    let _app = LushtextApplication::new();
}

#[test]
fn test_app_id() {
    ensure_gtk_init();
    let app = LushtextApplication::new();
    assert_eq!(app.application_id().expect("expected operation to succeed").as_str(), config::APP_ID);
}

#[test]
fn test_handles_open_flag() {
    ensure_gtk_init();
    let app = LushtextApplication::new();
    assert!(app.flags().contains(gio::ApplicationFlags::HANDLES_OPEN));
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    let _app: LushtextApplication = LushtextApplication::default();
}

#[test]
fn test_startup_registers_bundled_sourceview_scheme_path() {
    ensure_gtk_init();
    let app = LushtextApplication::new();
    app.register(gio::Cancellable::NONE)
        .expect("test application registration");
    app.emit_by_name::<()>("startup", &[]);

    let manager = StyleSchemeManager::default();
    let expected = "resource:///dev/cominotti/lushtext/gtksourceview/styles";
    assert!(
        manager.search_path().iter().any(|path| path.as_str() == expected),
        "expected bundled sourceview style search path {expected} to be registered"
    );
    assert!(manager.scheme("Adwaita").is_some());
    assert!(manager.scheme("Adwaita-dark").is_some());
}

#[test]
fn test_startup_registers_bundled_app_icon_path() {
    ensure_gtk_init();
    let app = LushtextApplication::new();
    app.register(gio::Cancellable::NONE)
        .expect("test application registration");
    app.emit_by_name::<()>("startup", &[]);

    let display = gdk::Display::default().expect("display");
    let theme = IconTheme::for_display(&display);
    let expected = config::RESOURCE_ICON_PATH;
    assert!(
        theme.resource_path().iter().any(|path| path.as_str() == expected),
        "expected bundled icon resource path {expected} to be registered"
    );
    assert!(
        theme.has_icon(config::APP_ID),
        "expected icon theme to expose {} after startup",
        config::APP_ID
    );
}

fn test_lushtext_application() -> LushtextApplication {
    crate::common::test_application()
        .downcast::<LushtextApplication>()
        .expect("test application should be LushtextApplication")
}

fn active_window(app: &LushtextApplication) -> LushtextWindow {
    app.active_window()
        .expect("active application window")
        .downcast::<LushtextWindow>()
        .expect("active window should be LushtextWindow")
}

fn active_editor(window: &LushtextWindow) -> LushtextEditorPage {
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.selected_page().is_some()
    });
    window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected page")
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("selected page should be an editor")
}

fn editor_text(editor: &LushtextEditorPage) -> String {
    let buffer = editor.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn tab_paths(window: &LushtextWindow) -> Vec<PathBuf> {
    (0..window.imp().tab_view.n_pages())
        .filter_map(|index| {
            window
                .imp()
                .tab_view
                .nth_page(index)
                .child()
                .downcast::<LushtextEditorPage>()
                .ok()
                .and_then(|editor| editor.file_path())
        })
        .collect()
}

fn status_text_contains(window: &LushtextWindow, needle: &str) -> bool {
    window
        .imp()
        .notification_bus
        .status_bar_view()
        .is_some_and(|status| status.text.contains(needle))
}

/// Fetch a named Open popover surface from the automation snapshot.
fn open_recent_surface<'a>(
    snapshot: &'a lushtext_core::model::automation::AutomationVisualGeometrySnapshot,
    name: &str,
) -> &'a lushtext_core::model::automation::AutomationVisualSurfaceSnapshot {
    snapshot
        .surfaces
        .iter()
        .find(|surface| surface.name == name)
        .unwrap_or_else(|| panic!("snapshot should include {name} surface"))
}

fn clear_session() {
    session_service::save(&json_store::data_dir(), &SessionData::default())
        .expect("clear test session");
}

struct EditorLoadDelayReset;

impl Drop for EditorLoadDelayReset {
    fn drop(&mut self) {
        editor_io::set_load_delay_for_test(0);
    }
}

fn delay_editor_loads_for_test(delay: Duration) -> EditorLoadDelayReset {
    let delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
    editor_io::set_load_delay_for_test(delay_ms);
    EditorLoadDelayReset
}

fn open_files(app: &LushtextApplication, paths: &[&Path]) {
    let files: Vec<_> = paths.iter().map(gio::File::for_path).collect();
    app.open(&files, "");
    flush_events();
}

fn open_gfiles(app: &LushtextApplication, files: &[gio::File]) {
    app.open(files, "");
    flush_events();
}

fn wait_for_loaded_tabs(window: &LushtextWindow, expected: i32) {
    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == expected
            && (0..window.imp().tab_view.n_pages()).all(|index| {
                window
                    .imp()
                    .tab_view
                    .nth_page(index)
                    .child()
                    .downcast::<LushtextEditorPage>()
                    .is_ok_and(|editor| editor.file_size().is_some())
            })
    });
}

fn wait_for_active_loaded_path(window: &LushtextWindow, path: &Path) {
    wait_until(Duration::from_secs(3), || {
        let editor = active_editor(window);
        editor.file_path().as_deref() == Some(path) && editor.file_size().is_some()
    });
}

#[test]
fn test_open_activation_opens_single_file_tab() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let path = dir.path().join("activated.txt");
    fixture::write_text(&path, "opened from desktop\n");
    let app = test_lushtext_application();

    open_files(&app, &[path.as_path()]);
    let window = active_window(&app);
    wait_for_loaded_tabs(&window, 1);

    let editor = active_editor(&window);
    assert_eq!(editor.file_path().as_deref(), Some(path.as_path()));
    assert_eq!(editor_text(&editor), "opened from desktop\n");
}

#[test]
fn test_open_activation_deduplicates_canonical_paths_and_focuses_duplicate() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let alpha = dir.path().join("alpha.txt");
    let beta = dir.path().join("beta.txt");
    let alpha_link = dir.path().join("alpha-link.txt");
    fixture::write_text(&alpha, "alpha\n");
    fixture::write_text(&beta, "beta\n");
    fixture::symlink(&alpha, &alpha_link);
    let app = test_lushtext_application();

    open_files(&app, &[alpha.as_path(), beta.as_path(), alpha_link.as_path()]);
    let window = active_window(&app);
    wait_for_loaded_tabs(&window, 2);

    let canonical_alpha = fs_metadata::canonical_path(&alpha).expect("canonical alpha");
    let canonical_beta = fs_metadata::canonical_path(&beta).expect("canonical beta");
    let canonical_paths: Vec<_> = tab_paths(&window)
        .into_iter()
        .map(|path| fs_metadata::canonical_path(&path).expect("canonical tab path"))
        .collect();
    assert!(canonical_paths.contains(&canonical_alpha));
    assert!(canonical_paths.contains(&canonical_beta));
    assert_eq!(
        active_editor(&window)
            .file_path()
            .as_deref()
            .and_then(|path| fs_metadata::canonical_path(path).ok()),
        Some(canonical_alpha.clone()),
        "the duplicate activation should focus the already-open canonical file",
    );
    assert!(
        window.imp().open_paths.borrow().contains(&canonical_alpha),
        "canonical duplicate close must leave the surviving owner keyed by canonical path",
    );
}

#[test]
fn test_open_activation_reuses_existing_window() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    fixture::write_text(&first, "first\n");
    fixture::write_text(&second, "second\n");
    let app = test_lushtext_application();

    open_files(&app, &[first.as_path()]);
    let window = active_window(&app);
    wait_for_loaded_tabs(&window, 1);
    let original_window = window.as_ptr();

    open_files(&app, &[second.as_path()]);
    let reused = active_window(&app);
    wait_for_loaded_tabs(&reused, 2);

    assert_eq!(reused.as_ptr(), original_window);
    assert_eq!(active_editor(&reused).file_path().as_deref(), Some(second.as_path()));
}

#[test]
fn test_open_activation_close_updates_recent_popover_automation_snapshot() {
    ensure_gtk_init();
    clear_session();
    let dir = tempfile::tempdir().expect("activation recent tempdir");
    let path = dir.path().join("activated-recent.txt");
    fixture::write_text(&path, "recent from activation\n");
    let app = test_lushtext_application();

    open_files(&app, &[path.as_path()]);
    let window = active_window(&app);
    present_window(&window);
    wait_for_loaded_tabs(&window, 1);
    wait_until(Duration::from_secs(3), || {
        window
            .recent_documents_for_test()
            .iter()
            .any(|entry| entry.matches_path(&path))
    });

    window.activate_action("close-tab", None);
    flush_events();
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);

    window.activate_action("open-recent", None);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().open_menu_button.is_active()
            || gtk4::prelude::WidgetExt::is_visible(&*window.imp().open_popover)
    });

    let snapshot = app_snapshot(&app);
    let window_snapshot = snapshot.window.expect("active window snapshot");
    assert!(window_snapshot.surfaces.open_popover_visible);
    assert_eq!(
        window_snapshot.surfaces.active_transient_surface.as_deref(),
        Some("open-popover")
    );
    let list = open_recent_surface(&window_snapshot.visual_geometry, "open-popover-recent-list");
    let empty = open_recent_surface(&window_snapshot.visual_geometry, "open-popover-empty-state");
    assert!(
        list.visible,
        "automation snapshot should expose the recent list after closing the activated tab"
    );
    assert!(
        !empty.visible,
        "automation snapshot should not report the empty state when a closed recent row exists"
    );
}

#[test]
fn test_open_activation_reports_failed_paths_without_stale_bookkeeping() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let missing = dir.path().join("missing.txt");
    let app = test_lushtext_application();

    open_files(&app, &[missing.as_path()]);
    let window = active_window(&app);
    let missing_status = missing.display().to_string();
    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == 1
            && active_editor(&window).file_path().is_none()
            && active_editor(&window).load_state() == EditorLoadState::Failed
            && status_text_contains(&window, &missing_status)
    });
    assert!(
        !window.imp().open_paths.borrow().contains(&missing),
        "a missing activation target should not poison duplicate-tab bookkeeping",
    );

    fixture::write_text(&missing, "created after failed activation\n");
    open_files(&app, &[missing.as_path()]);
    wait_for_active_loaded_path(&window, &missing);
    assert_eq!(editor_text(&active_editor(&window)), "created after failed activation\n");

    let unreadable = dir.path().join("directory-target.txt");
    fixture::create_dir(&unreadable);
    let unreadable_key = fs_metadata::canonical_path(&unreadable).expect("canonical unreadable target");
    open_files(&app, &[unreadable.as_path()]);
    let unreadable_status = unreadable.display().to_string();
    wait_until(Duration::from_secs(3), || {
        !window.imp().open_paths.borrow().contains(&unreadable_key)
            && status_text_contains(&window, &unreadable_status)
    });
    assert!(
        !window.imp().open_paths.borrow().contains(&unreadable_key),
        "an unreadable activation target should not leave a canonical open-path key",
    );

    fixture::remove_dir_all(&unreadable);
    fixture::write_text(&unreadable, "readable after cleanup\n");
    open_files(&app, &[unreadable.as_path()]);
    wait_for_active_loaded_path(&window, &unreadable);
    assert_eq!(active_editor(&window).file_path().as_deref(), Some(unreadable.as_path()));
    assert_eq!(editor_text(&active_editor(&window)), "readable after cleanup\n");
}

#[test]
fn test_desktop_exec_forwards_documents_and_matches_open_activation() {
    ensure_gtk_init();
    let desktop_entry = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/dev.cominotti.lushtext.desktop.in"
    ));
    let exec_line = desktop_entry
        .lines()
        .find(|line| line.starts_with("Exec="))
        .expect("desktop Exec line");
    assert!(
        exec_line.contains("%U") || exec_line.contains("%F") || exec_line.contains("%f"),
        "desktop Exec line should forward document arguments: {exec_line}",
    );

    let dir = tempfile::tempdir().expect("activation tempdir");
    let path = dir.path().join("desktop-forwarded.txt");
    fixture::write_text(&path, "desktop metadata activation\n");
    let app = test_lushtext_application();

    open_files(&app, &[path.as_path()]);
    let window = active_window(&app);
    wait_for_loaded_tabs(&window, 1);
    assert_eq!(editor_text(&active_editor(&window)), "desktop metadata activation\n");
}

#[test]
fn test_open_activation_keeps_explicit_file_active_after_session_restore() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let restored = dir.path().join("restored.txt");
    let explicit = dir.path().join("explicit.txt");
    fixture::write_text(&restored, "restored session\n");
    fixture::write_text(&explicit, "explicit activation\n");
    session_service::save(
        &json_store::data_dir(),
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(restored),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("seed session");
    let app = test_lushtext_application();

    open_files(&app, &[explicit.as_path()]);
    let window = active_window(&app);
    wait_for_loaded_tabs(&window, 2);

    assert_eq!(
        active_editor(&window).file_path().as_deref(),
        Some(explicit.as_path()),
        "explicit desktop or CLI activation should remain selected after session restore",
    );
}

#[test]
fn test_open_activation_bypasses_restored_failed_placeholder_after_file_becomes_readable() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let restored_missing = dir.path().join("restored-missing.txt");
    session_service::save(
        &json_store::data_dir(),
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(restored_missing.clone()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("seed missing restored session");
    let app = test_lushtext_application();

    app.activate();
    flush_events();
    let window = active_window(&app);
    let missing_status = restored_missing.display().to_string();
    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == 1
            && active_editor(&window).load_state() == EditorLoadState::Failed
            && status_text_contains(&window, &missing_status)
    });

    fixture::write_text(&restored_missing, "now readable\n");
    open_files(&app, &[restored_missing.as_path()]);
    wait_for_active_loaded_path(&window, &restored_missing);

    assert_eq!(window.imp().tab_view.n_pages(), 2);
    assert_eq!(editor_text(&active_editor(&window)), "now readable\n");
    assert!(
        (0..window.imp().tab_view.n_pages()).any(|index| window
            .imp()
            .tab_view
            .nth_page(index)
            .child()
            .downcast_ref::<LushtextEditorPage>()
            .is_some_and(|editor| editor.load_state() == EditorLoadState::Failed)),
        "the restored failed placeholder should remain visible",
    );
}

#[test]
fn test_open_activation_modified_failed_placeholder_remains_recoverable_without_blocking_reopen() {
    ensure_gtk_init();
    clear_session();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let missing = dir.path().join("modified-failed.txt");
    let app = test_lushtext_application();
    let files = [gio::File::for_path(&missing)];
    let load_delay = delay_editor_loads_for_test(Duration::from_millis(250));

    app.open(&files, "");
    let window = active_window(&app);
    let failed_editor = active_editor(&window);
    failed_editor.buffer().set_text("typed into failed placeholder");
    failed_editor.buffer().set_modified(true);
    let missing_status = missing.display().to_string();
    wait_until(Duration::from_secs(3), || {
        failed_editor.load_state() == EditorLoadState::Failed
            && failed_editor.file_path().as_deref() == Some(missing.as_path())
            && !window.imp().open_paths.borrow().contains(&missing)
            && status_text_contains(&window, &missing_status)
    });
    drop(load_delay);

    fixture::write_text(&missing, "fresh file from desktop\n");
    open_files(&app, &[missing.as_path()]);
    wait_for_active_loaded_path(&window, &missing);
    let active = active_editor(&window);

    assert_eq!(window.imp().tab_view.n_pages(), 2);
    assert_ne!(active.as_ptr(), failed_editor.as_ptr());
    assert_eq!(editor_text(&active), "fresh file from desktop\n");
    assert_eq!(editor_text(&failed_editor), "typed into failed placeholder");
    assert!(failed_editor.is_modified());
    assert_eq!(failed_editor.load_state(), EditorLoadState::Failed);
}

#[test]
fn test_open_activation_modified_failed_placeholder_restores_draft_after_restart() {
    ensure_gtk_init();
    clear_session();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let missing = dir.path().join("restart-missing.txt");
    let app = test_lushtext_application();
    let files = [gio::File::for_path(&missing)];
    let load_delay = delay_editor_loads_for_test(Duration::from_millis(250));

    app.open(&files, "");
    let window = active_window(&app);
    let failed_editor = active_editor(&window);
    failed_editor
        .buffer()
        .set_text("recover this failed placeholder");
    failed_editor.buffer().set_modified(true);
    wait_until(Duration::from_secs(3), || {
        failed_editor.load_state() == EditorLoadState::Failed
            && failed_editor.file_path().as_deref() == Some(missing.as_path())
    });
    drop(load_delay);
    window.flush_dirty_drafts().expect("flush failed-placeholder draft");
    window.save_session_sync();

    let restored_app = test_lushtext_application();
    restored_app.activate();
    flush_events();
    let restored_window = active_window(&restored_app);
    wait_until(Duration::from_secs(3), || {
        let editor = active_editor(&restored_window);
        editor.load_state() == EditorLoadState::Failed
            && editor.file_path().as_deref() == Some(missing.as_path())
            && editor_text(&editor) == "recover this failed placeholder"
            && editor.is_modified()
    });
}

#[test]
fn test_save_after_modified_failed_placeholder_restores_duplicate_bookkeeping() {
    ensure_gtk_init();
    clear_session();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let missing = dir.path().join("save-after-failed.txt");
    let app = test_lushtext_application();
    let files = [gio::File::for_path(&missing)];
    let load_delay = delay_editor_loads_for_test(Duration::from_millis(250));

    app.open(&files, "");
    let window = active_window(&app);
    let failed_editor = active_editor(&window);
    failed_editor.buffer().set_text("save from failed tab\n");
    failed_editor.buffer().set_modified(true);
    wait_until(Duration::from_secs(3), || {
        failed_editor.load_state() == EditorLoadState::Failed
            && failed_editor.file_path().as_deref() == Some(missing.as_path())
            && !window.imp().open_paths.borrow().contains(&missing)
    });
    drop(load_delay);

    window.activate_action("save", None);
    flush_events();
    wait_until(Duration::from_secs(3), || {
        !failed_editor.is_saving()
            && !failed_editor.is_modified()
            && window.imp().open_paths.borrow().contains(&missing)
    });

    assert_eq!(
        fs_read::text(&missing).expect("read saved failed placeholder"),
        "save from failed tab\n"
    );
    open_files(&app, &[missing.as_path()]);
    wait_until(Duration::from_secs(3), || window.imp().tab_view.n_pages() == 1);
    assert_eq!(active_editor(&window).as_ptr(), failed_editor.as_ptr());
}

#[test]
fn test_reload_failure_keeps_loaded_tab_file_backed_for_session_restore() {
    ensure_gtk_init();
    clear_session();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let path = dir.path().join("reload-removed.txt");
    fixture::write_text(&path, "loaded before reload failure\n");
    let app = test_lushtext_application();

    open_files(&app, &[path.as_path()]);
    let window = active_window(&app);
    wait_for_active_loaded_path(&window, &path);
    let editor = active_editor(&window);
    assert_eq!(editor.load_state(), EditorLoadState::Loaded);

    fixture::remove_file(&path);
    editor.load_file_async(&path);
    wait_until(Duration::from_secs(3), || {
        editor.load_state() == EditorLoadState::Loaded
            && editor
                .info_bar()
                .imp()
                .alert_revealer
                .property::<bool>("reveal-child")
    });

    assert_eq!(editor.file_path().as_deref(), Some(path.as_path()));
    assert_eq!(editor_text(&editor), "loaded before reload failure\n");
    window.save_session_sync();
    let restored_session = session_service::load(&json_store::data_dir()).expect("load session");
    assert_eq!(
        restored_session.tabs.first().and_then(|tab| tab.path.as_ref()),
        Some(&path),
        "a reload failure on an already-loaded tab must not turn the clean tab into an untitled session entry",
    );
}

#[test]
fn test_open_activation_stays_selected_after_session_restore_failure_settles() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let restored_missing = dir.path().join("restored-missing-late.txt");
    let explicit = dir.path().join("explicit-after-restore-failure.txt");
    fixture::write_text(&explicit, "explicit activation survives\n");
    session_service::save(
        &json_store::data_dir(),
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(restored_missing.clone()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("seed failing restored session");
    let app = test_lushtext_application();

    open_files(&app, &[explicit.as_path()]);
    let window = active_window(&app);
    let missing_status = restored_missing.display().to_string();
    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == 2
            && active_editor(&window).file_path().as_deref() == Some(explicit.as_path())
            && active_editor(&window).file_size().is_some()
            && status_text_contains(&window, &missing_status)
    });

    assert_eq!(
        editor_text(&active_editor(&window)),
        "explicit activation survives\n"
    );
}

#[test]
fn test_repeated_open_activation_focuses_existing_loaded_file_without_duplication() {
    ensure_gtk_init();
    clear_session();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let path = dir.path().join("repeat.txt");
    fixture::write_text(&path, "repeat activation\n");
    let app = test_lushtext_application();

    open_files(&app, &[path.as_path()]);
    let window = active_window(&app);
    wait_for_loaded_tabs(&window, 1);
    open_files(&app, &[path.as_path()]);
    wait_for_loaded_tabs(&window, 1);

    assert_eq!(
        active_editor(&window).file_path().as_deref(),
        Some(path.as_path())
    );
    assert_eq!(editor_text(&active_editor(&window)), "repeat activation\n");
}

#[test]
fn test_non_path_uri_activation_reports_feedback_without_fake_document_tab() {
    ensure_gtk_init();
    clear_session();
    let app = test_lushtext_application();
    let remote_uri = "smb://example.test/share/remote.txt";
    let remote = gio::File::for_uri(remote_uri);
    assert!(remote.path().is_none());

    open_gfiles(&app, &[remote]);
    let window = active_window(&app);
    wait_until(Duration::from_secs(3), || {
        status_text_contains(&window, remote_uri)
    });

    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert!(status_text_contains(
        &window,
        "only local files are supported"
    ));
}

#[test]
fn test_mixed_uri_and_local_activation_reports_uri_while_opening_local_file() {
    ensure_gtk_init();
    clear_session();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let local = dir.path().join("local-after-uri.txt");
    fixture::write_text(&local, "local still opens\n");
    let remote_uri = "smb://example.test/share/mixed.txt";
    let files = [gio::File::for_uri(remote_uri), gio::File::for_path(&local)];
    let app = test_lushtext_application();

    open_gfiles(&app, &files);
    let window = active_window(&app);
    wait_for_active_loaded_path(&window, &local);

    assert_eq!(window.imp().tab_view.n_pages(), 1);
    assert_eq!(editor_text(&active_editor(&window)), "local still opens\n");
    assert!(status_text_contains(&window, remote_uri));
}

#[test]
fn test_existing_window_receives_non_path_uri_feedback_without_losing_active_tab() {
    ensure_gtk_init();
    clear_session();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let local = dir.path().join("already-open.txt");
    fixture::write_text(&local, "already open\n");
    let remote_uri = "smb://example.test/share/existing-window.txt";
    let app = test_lushtext_application();

    open_files(&app, &[local.as_path()]);
    let window = active_window(&app);
    wait_for_loaded_tabs(&window, 1);
    open_gfiles(&app, &[gio::File::for_uri(remote_uri)]);
    wait_until(Duration::from_secs(3), || {
        status_text_contains(&window, remote_uri)
    });

    assert_eq!(window.imp().tab_view.n_pages(), 1);
    assert_eq!(
        active_editor(&window).file_path().as_deref(),
        Some(local.as_path())
    );
    assert_eq!(editor_text(&active_editor(&window)), "already open\n");
}

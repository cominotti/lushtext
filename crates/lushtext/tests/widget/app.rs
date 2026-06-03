// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for LushtextApplication.

use crate::common::{ensure_gtk_init, flush_events, wait_until};
use gio::prelude::*;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{IconTheme, gdk};
use gtk4::prelude::{GtkApplicationExt, TextBufferExt};
use lushtext_core::app::LushtextApplication;
use lushtext_core::config;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::services::{json_store, session_service};
use lushtext_core::ui::editor_page::LushtextEditorPage;
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

fn open_files(app: &LushtextApplication, paths: &[&Path]) {
    let files: Vec<_> = paths.iter().map(gio::File::for_path).collect();
    app.open(&files, "");
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
    std::fs::write(&path, "opened from desktop\n").expect("write activation file");
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
    std::fs::write(&alpha, "alpha\n").expect("write alpha");
    std::fs::write(&beta, "beta\n").expect("write beta");
    std::os::unix::fs::symlink(&alpha, &alpha_link).expect("symlink alpha");
    let app = test_lushtext_application();

    open_files(&app, &[alpha.as_path(), beta.as_path(), alpha_link.as_path()]);
    let window = active_window(&app);
    wait_for_loaded_tabs(&window, 2);

    let canonical_paths: Vec<_> = tab_paths(&window)
        .into_iter()
        .map(|path| path.canonicalize().expect("canonical tab path"))
        .collect();
    assert!(canonical_paths.contains(&alpha.canonicalize().expect("canonical alpha")));
    assert!(canonical_paths.contains(&beta.canonicalize().expect("canonical beta")));
    assert_eq!(
        active_editor(&window)
            .file_path()
            .as_deref()
            .and_then(|path| path.canonicalize().ok()),
        Some(alpha.canonicalize().expect("canonical alpha")),
        "the duplicate activation should focus the already-open canonical file",
    );
}

#[test]
fn test_open_activation_reuses_existing_window() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    std::fs::write(&first, "first\n").expect("write first");
    std::fs::write(&second, "second\n").expect("write second");
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
            && status_text_contains(&window, &missing_status)
    });
    assert!(
        !window.imp().open_paths.borrow().contains(&missing),
        "a missing activation target should not poison duplicate-tab bookkeeping",
    );

    std::fs::write(&missing, "created after failed activation\n").expect("create retry file");
    open_files(&app, &[missing.as_path()]);
    wait_for_active_loaded_path(&window, &missing);
    assert_eq!(editor_text(&active_editor(&window)), "created after failed activation\n");

    let unreadable = dir.path().join("directory-target.txt");
    std::fs::create_dir(&unreadable).expect("create unreadable document target");
    let unreadable_key = unreadable.canonicalize().expect("canonical unreadable target");
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

    std::fs::remove_dir(&unreadable).expect("remove unreadable target directory");
    std::fs::write(&unreadable, "readable after cleanup\n").expect("replace target with file");
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
    std::fs::write(&path, "desktop metadata activation\n").expect("write activation file");
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
    std::fs::write(&restored, "restored session\n").expect("write restored");
    std::fs::write(&explicit, "explicit activation\n").expect("write explicit");
    session_service::save(
        &json_store::data_dir(),
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(restored.clone()),
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

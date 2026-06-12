// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget and window-integration tests for command palette template wiring,
//! grouped results, keyboard flow, click-away dismissal, and focus restoration.

use crate::common::{ensure_gtk_init, fixture, flush_events, wait_until};
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::prelude::ToValue;
use gtk4::prelude::*;
use lushtext_core::model::palette::{
    CommandCategory, CommandDef, IndexedFile, PaletteFileEntry, SearchMode,
};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::{json_store, workspace_manager};
use lushtext_core::services::palette::FileIndex;
use lushtext_core::ui::command_palette::LushtextCommandPalette;
use lushtext_core::ui::command_palette::item::PaletteItem;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Keep older command-palette assertions on the shared harness wait semantics.
fn spin_until(predicate: impl Fn() -> bool) {
    wait_until(Duration::from_secs(2), predicate);
}

/// Present a test window and wait until the headless compositor allocates it.
fn present_window(window: &LushtextWindow) {
    window.present();
    // Realization is a precondition: give the headless compositor a generous
    // budget to allocate the window before tests interact with it.
    wait_until(Duration::from_secs(5), || {
        window.width() > 0 && window.height() > 0
    });
    flush_events();
}

/// Seed two persisted workspaces so window construction observes real app state.
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

/// Wait until the window's async command-palette index rebuild reaches a size.
fn wait_for_palette_index(window: &LushtextWindow, expected_index: usize) {
    wait_until(Duration::from_secs(3), || {
        window.imp().command_palette.file_index_len() == expected_index
    });
}

fn emit_key(widget: &gtk4::Widget, key: gtk4::gdk::Key) -> glib::Propagation {
    let controllers = widget.observe_controllers();
    for index in 0..controllers.n_items() {
        // Controller lists store generic GObjects, so the test downcasts to the
        // key controller before emitting GTK's "key-pressed" signal by name.
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

fn palette_rows(palette: &LushtextCommandPalette) -> Vec<PaletteItem> {
    let store = palette.imp().results_store.clone();
    (0..store.n_items())
        .filter_map(|index| store.item(index).and_downcast::<PaletteItem>())
        .collect()
}

fn palette_labels(palette: &LushtextCommandPalette) -> Vec<String> {
    palette_rows(palette)
        .iter()
        .map(PaletteItem::display_name)
        .collect()
}

fn row_position(labels: &[String], label: &str) -> usize {
    labels
        .iter()
        .position(|candidate| candidate == label)
        .unwrap_or_else(|| panic!("expected label '{label}' in {labels:?}"))
}

fn row_with_label(palette: &LushtextCommandPalette, label: &str) -> PaletteItem {
    palette_rows(palette)
        .into_iter()
        .find(|item| item.display_name() == label)
        .unwrap_or_else(|| panic!("expected row '{label}' in {:?}", palette_labels(palette)))
}

fn row_subtitle(palette: &LushtextCommandPalette, label: &str) -> Option<String> {
    palette_rows(palette)
        .into_iter()
        .find(|item| item.display_name() == label)
        .map(|item| item.subtitle())
}

fn rebuild_and_wait_for_label(palette: &LushtextCommandPalette, query: &str, label: &str) {
    palette.imp().rebuild_results(query);
    spin_until(|| palette_labels(palette).iter().any(|item| item == label));
}

fn rebuild_and_wait_until(
    palette: &LushtextCommandPalette,
    query: &str,
    predicate: impl Fn(&[String]) -> bool,
) {
    palette.imp().rebuild_results(query);
    wait_until(Duration::from_secs(5), || {
        let labels = palette_labels(palette);
        predicate(&labels)
    });
}

// ---------------------------------------------------------------------------
// PaletteItem GObject adapter
// ---------------------------------------------------------------------------

#[test]
fn test_palette_item_from_indexed_file() {
    ensure_gtk_init();
    let file = IndexedFile {
        path: "/home/user/project/src/main.rs".into(),
        name: "main.rs".to_string(),
        workspace_folder: std::sync::Arc::new("/home/user/project".into()),
    };
    let item = PaletteItem::from_indexed_file(&file);
    assert_eq!(item.display_name(), "main.rs");
    assert_eq!(item.subtitle(), "src/main.rs");
    assert!(item.is_file());
    assert!(!item.is_command());
    assert_eq!(
        item.file_path(),
        Some(PathBuf::from("/home/user/project/src/main.rs"))
    );
    assert!(item.action_id().is_empty());
}

#[test]
fn test_palette_item_from_command_def() {
    ensure_gtk_init();
    let cmd = CommandDef {
        id: "win.save",
        label: "Save File",
        category: CommandCategory::File,
        shortcut: Some("Ctrl+S"),
    };
    let item = PaletteItem::from_command_def(&cmd);
    assert_eq!(item.display_name(), "Save File");
    assert_eq!(item.subtitle(), "File · Ctrl+S");
    assert!(item.is_command());
    assert!(!item.is_file());
    assert_eq!(item.action_id(), "win.save");
    assert!(item.file_path().is_none());
}

#[test]
fn test_palette_item_command_without_shortcut() {
    ensure_gtk_init();
    let cmd = CommandDef {
        id: "app.preferences",
        label: "Preferences",
        category: CommandCategory::App,
        shortcut: None,
    };
    let item = PaletteItem::from_command_def(&cmd);
    assert_eq!(item.subtitle(), "App");
}

#[test]
fn test_palette_item_file_at_workspace_folder_top_level() {
    ensure_gtk_init();
    let file = IndexedFile {
        path: "/home/user/project/Cargo.toml".into(),
        name: "Cargo.toml".to_string(),
        workspace_folder: std::sync::Arc::new("/home/user/project".into()),
    };
    let item = PaletteItem::from_indexed_file(&file);
    assert_eq!(item.subtitle(), "Cargo.toml");
}

#[test]
fn test_palette_item_header_is_not_activatable() {
    ensure_gtk_init();
    let item = PaletteItem::new_header_raw("Open Tabs");
    assert_eq!(item.display_name(), "Open Tabs");
    assert!(item.is_header());
    assert!(!item.is_file());
    assert!(!item.is_command());
    assert!(!item.is_activatable());
}

// ---------------------------------------------------------------------------
// LushtextCommandPalette widget
// ---------------------------------------------------------------------------

#[test]
fn test_command_palette_new() {
    ensure_gtk_init();
    let _palette = LushtextCommandPalette::new();
}

#[test]
fn test_command_palette_default() {
    ensure_gtk_init();
    let _palette: LushtextCommandPalette = LushtextCommandPalette::default();
}

#[test]
fn test_command_palette_starts_with_all_mode() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    assert_eq!(palette.mode(), SearchMode::All);
}

#[test]
fn test_command_palette_mode_dropdown_initial() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let dropdown = &palette.imp().mode_dropdown;
    let model = dropdown
        .model()
        .and_downcast::<gtk4::StringList>()
        .expect("mode dropdown should use a StringList model");
    assert_eq!(model.n_items(), 4);
    assert_eq!(model.string(0).as_deref(), Some("All"));
    assert_eq!(model.string(1).as_deref(), Some("Files"));
    assert_eq!(model.string(2).as_deref(), Some("Notes"));
    assert_eq!(model.string(3).as_deref(), Some("Commands"));
    assert_eq!(dropdown.selected(), SearchMode::All.position());
}

#[test]
fn test_command_palette_mode_dropdown_changes_mode() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();
    flush_events();

    palette.imp().mode_dropdown.set_selected(SearchMode::Files.position());
    flush_events();

    assert_eq!(palette.mode(), SearchMode::Files);
    assert_eq!(
        palette
            .imp()
            .search_entry
            .placeholder_text()
            .expect("expected operation to succeed")
            .as_str(),
        SearchMode::Files.placeholder(),
    );
}

#[test]
fn test_command_palette_tab_syncs_mode_dropdown() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();
    flush_events();

    assert_eq!(
        emit_key(
            palette.imp().search_entry.upcast_ref::<gtk4::Widget>(),
            gtk4::gdk::Key::Tab,
        ),
        glib::Propagation::Stop,
    );
    assert_eq!(palette.mode(), SearchMode::Files);
    assert_eq!(palette.imp().mode_dropdown.selected(), SearchMode::Files.position());

    assert_eq!(
        emit_key(
            palette.imp().search_entry.upcast_ref::<gtk4::Widget>(),
            gtk4::gdk::Key::Tab,
        ),
        glib::Propagation::Stop,
    );
    assert_eq!(palette.mode(), SearchMode::Notes);
    assert_eq!(palette.imp().mode_dropdown.selected(), SearchMode::Notes.position());

    assert_eq!(
        emit_key(
            palette.imp().search_entry.upcast_ref::<gtk4::Widget>(),
            gtk4::gdk::Key::ISO_Left_Tab,
        ),
        glib::Propagation::Stop,
    );
    assert_eq!(palette.mode(), SearchMode::Files);
    assert_eq!(palette.imp().mode_dropdown.selected(), SearchMode::Files.position());
}

#[test]
fn test_command_palette_open_sets_placeholder() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();
    flush_events();
    assert_eq!(
        palette
            .imp()
            .search_entry
            .placeholder_text()
            .expect("expected operation to succeed")
            .as_str(),
        SearchMode::All.placeholder(),
    );
}

#[test]
fn test_command_palette_placeholder_changes_with_mode() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();
    flush_events();

    let imp = palette.imp();

    imp.set_mode(imp.mode.get().next());
    assert_eq!(palette.mode(), SearchMode::Files);
    assert_eq!(imp.mode_dropdown.selected(), SearchMode::Files.position());
    assert_eq!(
        imp.search_entry.placeholder_text().expect("expected operation to succeed").as_str(),
        SearchMode::Files.placeholder(),
    );

    imp.set_mode(imp.mode.get().next());
    assert_eq!(palette.mode(), SearchMode::Notes);
    assert_eq!(imp.mode_dropdown.selected(), SearchMode::Notes.position());
    assert_eq!(
        imp.search_entry.placeholder_text().expect("expected operation to succeed").as_str(),
        SearchMode::Notes.placeholder(),
    );

    imp.set_mode(imp.mode.get().next());
    assert_eq!(palette.mode(), SearchMode::Commands);
    assert_eq!(imp.mode_dropdown.selected(), SearchMode::Commands.position());
    assert_eq!(
        imp.search_entry.placeholder_text().expect("expected operation to succeed").as_str(),
        SearchMode::Commands.placeholder(),
    );

    imp.set_mode(imp.mode.get().next());
    assert_eq!(palette.mode(), SearchMode::All);
    assert_eq!(imp.mode_dropdown.selected(), SearchMode::All.position());
    assert_eq!(
        imp.search_entry.placeholder_text().expect("expected operation to succeed").as_str(),
        SearchMode::All.placeholder(),
    );
}

#[test]
fn test_command_palette_open_populates_results() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();

    // rebuild_results runs on a background thread — spin until results arrive
    let store = palette.imp().results_store.clone();
    spin_until(|| store.n_items() > 0);
    assert!(
        store.n_items() > 0,
        "open() should populate results with commands"
    );
}

#[test]
fn test_command_palette_open_focuses_entry() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();
    flush_events();
    // Entry text should be cleared on open
    assert_eq!(palette.imp().search_entry.text().as_str(), "");
}

#[test]
fn test_command_palette_close_clears_results() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();

    let store = palette.imp().results_store.clone();
    spin_until(|| store.n_items() > 0);
    assert!(store.n_items() > 0);

    palette.close();
    assert_eq!(store.n_items(), 0);
}

#[test]
fn test_command_palette_close_clears_entry() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.imp().search_entry.set_text("test query");
    palette.close();
    assert_eq!(palette.imp().search_entry.text().as_str(), "");
}

#[test]
fn test_command_palette_set_file_index() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("hello.rs"), "");
    fixture::write_text(&dir.path().join("world.txt"), "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    palette.set_file_index(index);

    // Open and verify files appear in results
    palette.open();
    let store = palette.imp().results_store.clone();
    spin_until(|| {
        (0..store.n_items()).any(|i| {
            store
                .item(i)
                .and_downcast_ref::<PaletteItem>()
                .is_some_and(PaletteItem::is_file)
        })
    });
}

#[test]
fn test_command_palette_search_filters_results() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("main.rs"), "");
    fixture::write_text(&dir.path().join("Cargo.toml"), "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    palette.set_file_index(index);

    // Search for "main" — should match main.rs but not Cargo.toml
    palette.imp().rebuild_results("main");

    let store = palette.imp().results_store.clone();
    spin_until(|| {
        (0..store.n_items()).any(|i| {
            store
                .item(i)
                .and_downcast_ref::<PaletteItem>()
                .is_some_and(PaletteItem::is_file)
        })
    });

    let file_items: Vec<String> = (0..store.n_items())
        .filter_map(|i| {
            store
                .item(i)
                .and_downcast_ref::<PaletteItem>()
                .filter(|item| item.is_file())
                .map(PaletteItem::display_name)
        })
        .collect();
    assert!(file_items.contains(&"main.rs".to_string()));
    assert!(
        !file_items.contains(&"Cargo.toml".to_string()),
        "Cargo.toml should not match 'main' query, got: {file_items:?}"
    );
}

#[test]
fn test_command_palette_files_mode_groups_open_tabs_before_workspace_files() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let duplicate = dir.path().join("alpha.rs");
    let workspace_only = dir.path().join("workspace_alpha.rs");
    fixture::write_text(&duplicate, "");
    fixture::write_text(&workspace_only, "");

    palette.set_workspace_group_label("Selected Workspace");
    palette.set_open_tabs(vec![PaletteFileEntry::new(
        "alpha.rs".to_string(),
        duplicate.display().to_string(),
        duplicate.clone(),
    )]);
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::Files);

    rebuild_and_wait_for_label(&palette, "alpha", "Selected Workspace");
    let labels = palette_labels(&palette);

    assert!(
        row_position(&labels, "Open Tabs") < row_position(&labels, "Selected Workspace"),
        "Open Tabs should precede workspace files: {labels:?}",
    );
    assert_eq!(
        labels.iter().filter(|label| label.as_str() == "alpha.rs").count(),
        1,
        "duplicate open/workspace file should only appear once: {labels:?}",
    );
    assert!(
        labels.iter().any(|label| label == "workspace_alpha.rs"),
        "workspace-only file should remain visible: {labels:?}",
    );
}

#[test]
fn test_command_palette_files_mode_uses_all_workspaces_label() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("alpha.rs"), "");

    palette.set_workspace_group_label("All Workspaces");
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::Files);

    rebuild_and_wait_for_label(&palette, "alpha", "All Workspaces");
    let labels = palette_labels(&palette);
    assert!(labels.iter().any(|label| label == "All Workspaces"));
    assert!(!labels.iter().any(|label| label == "Selected Workspace"));
}

#[test]
fn test_command_palette_files_mode_empty_workspace_index_clears_workspace_group() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("alpha.rs"), "");

    palette.set_workspace_group_label("Selected Workspace");
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::Files);
    rebuild_and_wait_for_label(&palette, "alpha", "Selected Workspace");

    palette.set_file_index(FileIndex::default());
    rebuild_and_wait_until(&palette, "alpha", |labels| {
        labels.is_empty() && palette.imp().no_results_label.property::<bool>("visible")
    });
    let labels = palette_labels(&palette);
    assert!(!labels.iter().any(|label| label == "Selected Workspace"));
    assert!(!labels.iter().any(|label| label == "alpha.rs"));
}

#[test]
fn test_command_palette_all_mode_groups_sources_by_priority() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("open_workspace.rs"), "");

    palette.set_workspace_group_label("Selected Workspace");
    palette.set_open_tabs(vec![PaletteFileEntry::new(
        "open_tab.rs".to_string(),
        "/tmp/open_tab.rs".to_string(),
        PathBuf::from("/tmp/open_tab.rs"),
    )]);
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::All);

    rebuild_and_wait_for_label(&palette, "open", "Commands");
    let labels = palette_labels(&palette);

    let open_tabs = row_position(&labels, "Open Tabs");
    let workspace = row_position(&labels, "Selected Workspace");
    let notes = row_position(&labels, "Notes");
    let commands = row_position(&labels, "Commands");
    assert!(
        open_tabs < workspace && workspace < notes && notes < commands,
        "All mode groups should preserve priority: {labels:?}",
    );
    assert!(labels.iter().any(|label| label == "open_tab.rs"));
    assert!(labels.iter().any(|label| label == "open_workspace.rs"));
    assert!(labels.iter().any(|label| label == "Open Document Note"));
    assert!(labels.iter().any(|label| label == "Open File"));
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "Open Document Note")
            .count(),
        1,
        "note commands should not be duplicated under Commands: {labels:?}",
    );
}

#[test]
fn test_command_palette_all_mode_uses_all_workspaces_label() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("open_workspace.rs"), "");

    palette.set_workspace_group_label("All Workspaces");
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::All);

    rebuild_and_wait_for_label(&palette, "open", "Commands");
    let labels = palette_labels(&palette);
    assert!(labels.iter().any(|label| label == "All Workspaces"));
    assert!(!labels.iter().any(|label| label == "Selected Workspace"));
    assert!(
        row_position(&labels, "All Workspaces") < row_position(&labels, "Commands"),
        "workspace files should appear before command rows in All mode: {labels:?}",
    );
}

#[test]
fn test_command_palette_workspace_file_group_deduplicates_overlapping_folder_rows() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let workspace_folder = dir.path().to_path_buf();
    let nested_folder = workspace_folder.join("src");
    let nested_file = nested_folder.join("main.rs");
    fixture::create_dir_all(&nested_folder);
    fixture::write_text(&workspace_folder.join("README.md"), "");
    fixture::write_text(&nested_file, "");

    palette.set_workspace_group_label("Selected Workspace");
    palette.set_file_index(FileIndex::rebuild(&[
        workspace_folder.clone(),
        nested_folder.clone(),
    ]));
    palette.imp().set_mode(SearchMode::Files);
    rebuild_and_wait_for_label(&palette, "main", "main.rs");

    let labels = palette_labels(&palette);
    assert_eq!(
        labels.iter().filter(|label| label.as_str() == "main.rs").count(),
        1,
        "overlapping workspace folders should show one palette row: {labels:?}",
    );
    assert_eq!(row_with_label(&palette, "main.rs").subtitle(), "src/main.rs");

    palette.set_file_index(FileIndex::rebuild(&[nested_folder, workspace_folder]));
    palette.imp().rebuild_results("main");
    wait_until(Duration::from_secs(5), || {
        row_subtitle(&palette, "main.rs").as_deref() == Some("main.rs")
    });
    assert_eq!(
        row_with_label(&palette, "main.rs").subtitle(),
        "main.rs",
        "folder order should choose the primary display context",
    );
}

#[test]
fn test_command_palette_aggregate_scope_deduplicates_duplicate_workspace_files() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("alpha.rs"), "");

    palette.set_workspace_group_label("All Workspaces");
    palette.set_file_index(FileIndex::rebuild(&[
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
    ]));
    palette.imp().set_mode(SearchMode::Files);
    rebuild_and_wait_for_label(&palette, "alpha", "All Workspaces");

    let labels = palette_labels(&palette);
    assert_eq!(
        labels.iter().filter(|label| label.as_str() == "alpha.rs").count(),
        1,
        "aggregate scope should show one row for the same canonical file: {labels:?}",
    );
}

#[test]
fn test_command_palette_notes_mode_groups_note_commands_by_intent() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.imp().set_mode(SearchMode::Notes);

    rebuild_and_wait_for_label(&palette, "", "Workspace");
    let labels = palette_labels(&palette);

    let browse = row_position(&labels, "Browse");
    let current_document = row_position(&labels, "Current Document");
    let bookmark_navigation = row_position(&labels, "Bookmark Navigation");
    let workspace = row_position(&labels, "Workspace");
    assert!(
        browse < current_document
            && current_document < bookmark_navigation
            && bookmark_navigation < workspace,
        "Notes mode groups should preserve intent order: {labels:?}",
    );
    assert!(labels.iter().any(|label| label == "Browse Notes"));
    assert!(labels.iter().any(|label| label == "Browse Bookmarks"));
    assert!(labels.iter().any(|label| label == "Toggle Bookmark"));
    assert!(labels.iter().any(|label| label == "Edit Bookmark"));
    assert!(labels.iter().any(|label| label == "Open Document Note"));
    assert!(labels.iter().any(|label| label == "Next Bookmark"));
    assert!(labels.iter().any(|label| label == "Previous Bookmark"));
    assert!(labels.iter().any(|label| label == "Open Folder Note"));
    assert!(!labels.iter().any(|label| label == "Commands"));
}

#[test]
fn test_command_palette_notes_mode_excludes_files_and_non_note_commands() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("open_notes.rs"), "");

    palette.set_workspace_group_label("Selected Workspace");
    palette.set_open_tabs(vec![PaletteFileEntry::new(
        "open_notes_tab.rs".to_string(),
        "/tmp/open_notes_tab.rs".to_string(),
        PathBuf::from("/tmp/open_notes_tab.rs"),
    )]);
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::Notes);

    rebuild_and_wait_for_label(&palette, "open", "Open Document Note");
    let labels = palette_labels(&palette);

    assert!(labels.iter().any(|label| label == "Open Document Note"));
    assert!(labels.iter().any(|label| label == "Open Folder Note"));
    assert!(!labels.iter().any(|label| label == "Open Tabs"));
    assert!(!labels.iter().any(|label| label == "Selected Workspace"));
    assert!(!labels.iter().any(|label| label == "open_notes.rs"));
    assert!(!labels.iter().any(|label| label == "open_notes_tab.rs"));
    assert!(!labels.iter().any(|label| label == "Open File"));
}

#[test]
fn test_command_palette_commands_mode_keeps_note_commands_with_notes_subtitle() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.imp().set_mode(SearchMode::Commands);

    rebuild_and_wait_for_label(&palette, "browse notes", "Browse Notes");

    let row = row_with_label(&palette, "Browse Notes");
    assert!(row.is_command());
    assert_eq!(row.action_id(), "win.show-notes");
    assert_eq!(row.subtitle(), "Notes · Ctrl+Alt+A");
}

#[test]
fn test_command_palette_headers_do_not_activate() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    palette.set_open_tabs(vec![PaletteFileEntry::new(
        "alpha.rs".to_string(),
        "/tmp/alpha.rs".to_string(),
        PathBuf::from("/tmp/alpha.rs"),
    )]);
    palette.imp().set_mode(SearchMode::Files);
    rebuild_and_wait_for_label(&palette, "alpha", "Open Tabs");

    let activated = Rc::new(Cell::new(false));
    let activated_clone = activated.clone();
    palette.connect_item_activated(move |_| {
        activated_clone.set(true);
    });

    let selection = palette
        .imp()
        .results_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .expect("results should use a SingleSelection model");
    selection.set_selected(0);
    palette.imp().activate_selected();
    assert!(!activated.get(), "source header must not activate");

    selection.set_selected(1);
    palette.imp().activate_selected();
    assert!(activated.get(), "file row should still activate");
}

#[test]
fn test_command_palette_connect_item_activated() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let activated = Rc::new(Cell::new(false));
    let activated_clone = activated.clone();
    palette.connect_item_activated(move |_| {
        activated_clone.set(true);
    });

    // Populate with commands and activate first item
    palette.open();
    let store = palette.imp().results_store.clone();
    spin_until(|| store.n_items() > 0);

    palette.imp().activate_selected();
    assert!(activated.get(), "activate callback should have fired");
}

#[test]
fn test_command_palette_connect_close_requested() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let closed = Rc::new(Cell::new(false));
    let closed_clone = closed.clone();
    palette.connect_close_requested(move || {
        closed_clone.set(true);
    });

    // Trigger stop-search (Escape)
    palette.imp().search_entry.emit_stop_search();
    assert!(closed.get(), "close callback should have fired");
}

#[test]
fn test_command_palette_no_results_label_hidden_initially() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    // The no_results_label should not be visible when nothing is searched
    assert!(!palette.imp().no_results_label.property::<bool>("visible"));
}

#[test]
fn test_command_palette_no_results_label_on_no_match() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();
    let store = palette.imp().results_store.clone();
    spin_until(|| store.n_items() > 0);

    // Search for something that won't match anything
    palette.imp().rebuild_results("xyzzynonexistent");

    let no_results_label = palette.imp().no_results_label.clone();
    spin_until(|| no_results_label.property::<bool>("visible"));

    assert!(
        palette.imp().no_results_label.property::<bool>("visible"),
        "no_results_label should be visible when search has no matches"
    );
}

#[test]
fn test_command_palette_results_view_has_model() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    assert!(
        palette.imp().results_view.model().is_some(),
        "results_view should have a selection model"
    );
}

#[test]
fn test_command_palette_results_view_single_click_disabled() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    assert!(
        !palette.imp().results_view.is_single_click_activate(),
        "single-click-activate should be false"
    );
}

// ---------------------------------------------------------------------------
// Window integration tests for command palette
// ---------------------------------------------------------------------------

use gio::prelude::{ActionExt, ActionGroupExt, ActionMapExt};
use lushtext_core::ui::editor_page::LushtextEditorPage;
use lushtext_core::ui::window::LushtextWindow;

fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

fn action_enabled(window: &LushtextWindow, name: &str) -> bool {
    window
        .lookup_action(name)
        .unwrap_or_else(|| panic!("action '{name}' not found"))
        .is_enabled()
}

fn activate_action(window: &LushtextWindow, name: &str) {
    ActionGroupExt::activate_action(window, name, None);
    flush_events();
}

fn active_editor(window: &LushtextWindow) -> Option<LushtextEditorPage> {
    window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
}

fn active_editor_has_focus(window: &LushtextWindow) -> bool {
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return false;
    };
    active_editor(window).is_some_and(|editor| {
        focus.as_ptr() == editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr()
    })
}

/// Wait for the palette allocation and return its bounds in window coordinates.
///
/// Click-away tests use window-relative points because the production handler is
/// attached to the top-level window rather than to the palette widget.
fn command_palette_bounds_in_window(window: &LushtextWindow) -> gtk4::graphene::Rect {
    let mut bounds = None;
    wait_until(Duration::from_secs(2), || {
        bounds = window
            .imp()
            .command_palette
            .compute_bounds(window.upcast_ref::<gtk4::Widget>());
        bounds.is_some()
    });
    bounds.expect("command palette should be allocated inside the window")
}

/// Return a stable point near the center of the allocated palette.
fn point_inside_command_palette(window: &LushtextWindow) -> (f64, f64) {
    let bounds = command_palette_bounds_in_window(window);
    (
        f64::from(bounds.x() + bounds.width() / 2.0),
        f64::from(bounds.y() + bounds.height() / 2.0),
    )
}

/// Return a window-relative point outside the palette for click-away tests.
///
/// Corner candidates avoid assuming there is always room beside the palette in
/// constrained windows.
fn point_outside_command_palette(window: &LushtextWindow) -> (f64, f64) {
    let bounds = command_palette_bounds_in_window(window);
    let width = f64::from(window.width());
    let height = f64::from(window.height());
    let candidates = [
        (8.0, height - 8.0),
        (width - 8.0, height - 8.0),
        (8.0, 8.0),
        (width - 8.0, 8.0),
    ];
    candidates
        .into_iter()
        .find(|(x, y)| {
            *x >= 0.0
                && *x <= width
                && *y >= 0.0
                && *y <= height
                && !bounds.contains_point(&test_graphene_point(*x, *y))
        })
        .expect("test window should expose a point outside the command palette")
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Graphene uses f32 coordinates while GTK window dimensions are f64 in this helper."
)]
fn test_graphene_point(x: f64, y: f64) -> gtk4::graphene::Point {
    gtk4::graphene::Point::new(x as f32, y as f32)
}

#[test]
fn test_toggle_command_palette_action_exists() {
    ensure_gtk_init();
    let window = test_window();
    assert!(window.lookup_action("toggle-command-palette").is_some());
}

#[test]
fn test_toggle_command_palette_always_enabled() {
    ensure_gtk_init();
    let window = test_window();
    // Should be enabled even with no tabs
    assert!(action_enabled(&window, "toggle-command-palette"));
}

#[test]
fn test_toggle_command_palette_enabled_with_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert!(action_enabled(&window, "toggle-command-palette"));
}

#[test]
fn test_toggle_command_palette_enabled_after_closing_all_tabs() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    activate_action(&window, "close-tab");
    // Must remain enabled unlike begin-search
    assert!(action_enabled(&window, "toggle-command-palette"));
}

#[test]
fn test_palette_revealer_starts_hidden() {
    ensure_gtk_init();
    let window = test_window();
    assert!(!window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_toggle_reveals_palette() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_toggle_twice_hides_palette() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    activate_action(&window, "toggle-command-palette");
    assert!(!window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_palette_close_callback_hides_revealer() {
    ensure_gtk_init();
    let window = test_window();

    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    window
        .imp()
        .command_palette
        .imp()
        .search_entry
        .emit_stop_search();
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_palette_activation_closes_palette() {
    ensure_gtk_init();
    let window = test_window();

    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    // Wait for background search results (same race as activate_selected tests)
    let palette = window.imp().command_palette.clone();
    spin_until(move || palette.imp().results_store.n_items() > 0);

    window.imp().command_palette.imp().activate_selected();
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_escape_closes_palette_after_focus_moves_to_editor() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window).expect("active editor");
    editor.source_view().grab_focus();
    wait_until(Duration::from_secs(2), || active_editor_has_focus(&window));

    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    editor.source_view().grab_focus();
    wait_until(Duration::from_secs(2), || active_editor_has_focus(&window));
    assert!(window.handle_transient_escape_for_test());
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(active_editor_has_focus(&window));
}

#[test]
fn test_escape_closes_palette_after_focus_leaves_palette() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    active_editor(&window)
        .expect("active editor")
        .source_view()
        .grab_focus();
    wait_until(Duration::from_secs(2), || active_editor_has_focus(&window));

    activate_action(&window, "toggle-command-palette");
    // Regression path: Escape must still close the palette when no widget
    // inside it owns focus anymore.
    gtk4::prelude::GtkWindowExt::set_focus(&window, gtk4::Widget::NONE);
    flush_events();
    assert!(
        gtk4::prelude::GtkWindowExt::focus(&window).is_none(),
        "test precondition: focus should no longer belong to the palette"
    );

    assert!(window.handle_transient_escape_for_test());
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(active_editor_has_focus(&window));
}

#[test]
fn test_click_outside_command_palette_closes_and_restores_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    active_editor(&window)
        .expect("active editor")
        .source_view()
        .grab_focus();
    wait_until(Duration::from_secs(2), || active_editor_has_focus(&window));

    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().saved_focus.borrow().is_some());
    let (x, y) = point_outside_command_palette(&window);

    assert!(window.handle_command_palette_pointer_press_for_test(x, y));
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(window.imp().saved_focus.borrow().is_none());
    assert!(active_editor_has_focus(&window));
}

#[test]
fn test_click_inside_command_palette_keeps_it_open() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    activate_action(&window, "toggle-command-palette");
    let (x, y) = point_inside_command_palette(&window);

    assert!(!window.handle_command_palette_pointer_press_for_test(x, y));
    flush_events();

    assert!(window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_escape_closes_only_palette_above_search_panel() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-search-panel");
    wait_until(Duration::from_secs(2), || {
        window.imp().search_panel_revealer.reveals_child()
    });
    activate_action(&window, "toggle-command-palette");

    assert!(window.handle_transient_escape_for_test());
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(window.imp().search_panel_revealer.reveals_child());
}

#[test]
fn test_palette_stop_search_escape_does_not_cascade_to_search_panel() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-search-panel");
    wait_until(Duration::from_secs(2), || {
        window.imp().search_panel_revealer.reveals_child()
    });
    activate_action(&window, "toggle-command-palette");

    window
        .imp()
        .command_palette
        .imp()
        .search_entry
        .emit_stop_search();
    assert!(window.handle_transient_escape_for_test());
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(window.imp().search_panel_revealer.reveals_child());
}

#[test]
fn test_escape_closes_palette_without_tabs_or_workspaces() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    assert!(window.handle_transient_escape_for_test());
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_palette_dismissal_handles_dense_results_in_constrained_window() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("dense palette tempdir");
    // Eighty files exceed the palette's visible rows, and 720x360 keeps the
    // shell small enough to exercise constrained click/Escape geometry.
    for index in 0..80 {
        fixture::write_text(&dir.path().join(format!("palette-file-{index:02}.rs")), "");
    }
    let window = test_window();
    window.set_default_size(720, 360);
    present_window(&window);
    let palette = window.imp().command_palette.clone();
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::Files);

    activate_action(&window, "toggle-command-palette");
    // Twenty rows prove the result list is dense without depending on the exact
    // capped maximum returned by fuzzy search.
    rebuild_and_wait_until(&palette, "palette", |labels| labels.len() >= 20);

    assert!(window.handle_transient_escape_for_test());
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_palette_new_file_command_focuses_new_editor_after_close() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    rebuild_and_wait_for_label(&palette, "new file", "New File");

    let labels = palette_labels(&palette);
    let position = u32::try_from(row_position(&labels, "New File"))
        .expect("palette row position should fit GTK selection index");
    let selection = palette
        .imp()
        .results_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .expect("results should use a SingleSelection model");
    selection.set_selected(position);

    palette.imp().activate_selected();
    wait_until(Duration::from_secs(2), || {
        !window.imp().palette_revealer.reveals_child() && active_editor_has_focus(&window)
    });

    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

#[test]
fn test_palette_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    assert_eq!(
        palette.imp().search_entry.accessible_role(),
        gtk4::AccessibleRole::SearchBox
    );
    assert_eq!(
        palette.imp().mode_dropdown.accessible_role(),
        gtk4::AccessibleRole::ComboBox
    );
    assert_eq!(
        palette.imp().results_view.accessible_role(),
        gtk4::AccessibleRole::List
    );
}

#[test]
fn test_palette_workspace_group_label_follows_concrete_workspace_scope() {
    ensure_gtk_init();
    let (_folders_dir, left_folder, _right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-left")));
    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.imp().set_mode(SearchMode::Files);
    rebuild_and_wait_for_label(&palette, "alpha", "Selected Workspace");

    let labels = palette_labels(&palette);
    assert!(labels.iter().any(|label| label == "Selected Workspace"));
    assert!(labels.iter().any(|label| label == "alpha.rs"));
    assert_eq!(
        window
            .imp()
            .sidebar
            .folder_paths_for_scope(&WorkspaceScope::workspace(WorkspaceId::new("ws-left"))),
        vec![left_folder],
    );
}

#[test]
fn test_palette_workspace_group_label_follows_aggregate_workspace_scope() {
    ensure_gtk_init();
    let (_folders_dir, _left_folder, _right_folder) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 2);

    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.imp().set_mode(SearchMode::Files);
    rebuild_and_wait_for_label(&palette, "alpha", "All Workspaces");

    let labels = palette_labels(&palette);
    assert!(labels.iter().any(|label| label == "All Workspaces"));
    assert!(!labels.iter().any(|label| label == "Selected Workspace"));
}

#[test]
fn test_palette_empty_selected_workspace_scope_has_no_workspace_file_rows() {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("scoped workspace folders tempdir");
    let other_folder = folders_dir.path().join("other");
    fixture::create_dir_all(&other_folder);
    fixture::write_text(&other_folder.join("beta.rs"), "fn beta() {}\n");

    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws-empty")),
        workspaces: vec![
            WorkspaceConfig::with_folders(WorkspaceId::new("ws-empty"), "empty", Vec::new()),
            WorkspaceConfig::with_one_folder(
                WorkspaceId::new("ws-other"),
                "other",
                other_folder,
            ),
        ],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save scoped workspaces");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 0);

    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.imp().set_mode(SearchMode::Files);
    palette.imp().rebuild_results("beta");
    wait_until(Duration::from_secs(5), || {
        palette.imp().no_results_label.property::<bool>("visible")
    });

    let labels = palette_labels(&palette);
    assert!(!labels.iter().any(|label| label == "Selected Workspace"));
    assert!(!labels.iter().any(|label| label == "All Workspaces"));
    assert!(!labels.iter().any(|label| label == "beta.rs"));
}

#[test]
fn test_palette_open_tabs_can_appear_outside_selected_workspace() {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("outer workspace folders tempdir");
    let outside_file = folders_dir.path().join("beta.rs");
    fixture::write_text(&outside_file, "fn beta() {}\n");

    let (_workspace_folders_dir, _left_folder, _right_folder) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-left")));
    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);
    window.open_document(&outside_file);
    flush_events();

    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.imp().set_mode(SearchMode::Files);
    palette.imp().rebuild_results("beta");
    spin_until(|| {
        let labels = palette_labels(&palette);
        labels.iter().any(|label| label == "beta.rs")
            && !labels.iter().any(|label| label == "Commands")
    });

    let labels = palette_labels(&palette);
    assert!(labels.iter().any(|label| label == "Open Tabs"));
    assert!(labels.iter().any(|label| label == "beta.rs"));
    assert!(
        !labels.iter().any(|label| label == "Selected Workspace"),
        "selected workspace should not claim an out-of-scope open tab: {labels:?}",
    );
}

// ---------------------------------------------------------------------------
// Focus restoration state tracking
// ---------------------------------------------------------------------------

#[test]
fn test_open_palette_saves_focus_state() {
    ensure_gtk_init();
    let window = test_window();
    assert!(
        window.imp().saved_focus.borrow().is_none(),
        "saved_focus should be None before opening palette"
    );

    activate_action(&window, "toggle-command-palette");
    assert!(
        window.imp().saved_focus.borrow().is_some(),
        "saved_focus should be Some after opening palette"
    );
}

#[test]
fn test_close_palette_clears_saved_focus() {
    ensure_gtk_init();
    let window = test_window();

    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().saved_focus.borrow().is_some());

    activate_action(&window, "toggle-command-palette");
    assert!(
        window.imp().saved_focus.borrow().is_none(),
        "saved_focus should be consumed (None) after closing palette"
    );
}

#[test]
fn test_escape_clears_saved_focus() {
    ensure_gtk_init();
    let window = test_window();

    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().saved_focus.borrow().is_some());

    // Trigger stop-search (Escape)
    window
        .imp()
        .command_palette
        .imp()
        .search_entry
        .emit_stop_search();
    flush_events();

    assert!(
        window.imp().saved_focus.borrow().is_none(),
        "saved_focus should be consumed after Escape close"
    );
}

#[test]
fn test_activation_clears_saved_focus() {
    ensure_gtk_init();
    let window = test_window();

    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().saved_focus.borrow().is_some());

    // Wait for background search results to populate the results store.
    // rebuild_results uses spawn_blocking_then, so results arrive via
    // idle_add_once — flush_events alone may miss them if the background
    // thread hasn't finished yet.
    let palette = window.imp().command_palette.clone();
    spin_until(move || palette.imp().results_store.n_items() > 0);

    // Activate first result (closes palette)
    window.imp().command_palette.imp().activate_selected();
    flush_events();

    assert!(
        window.imp().saved_focus.borrow().is_none(),
        "saved_focus should be consumed after item activation close"
    );
}

// --- Command registry completeness ---

#[test]
fn test_all_commands_contains_fullscreen() {
    ensure_gtk_init();
    let commands = lushtext_core::services::palette::all_commands();
    let fullscreen = commands.iter().find(|c| c.id == "win.toggle-fullscreen");
    assert!(
        fullscreen.is_some(),
        "all_commands() should include Fullscreen"
    );
    let cmd = fullscreen.expect("expected operation to succeed");
    assert_eq!(cmd.label, "Fullscreen");
    assert_eq!(cmd.shortcut, Some("F11"));
    assert_eq!(cmd.category, CommandCategory::View);
}

#[test]
fn test_all_commands_new_file_uses_ctrl_n() {
    ensure_gtk_init();
    let commands = lushtext_core::services::palette::all_commands();
    let cmd = commands.iter().find(|c| c.id == "win.new-tab");
    assert!(cmd.is_some(), "all_commands() should include New File");
    let cmd = cmd.expect("expected operation to succeed");
    assert_eq!(cmd.label, "New File");
    assert_eq!(cmd.shortcut, Some("Ctrl+N"));
    assert_eq!(cmd.category, CommandCategory::File);
}

#[test]
fn test_all_commands_contains_focus_mode() {
    ensure_gtk_init();
    let commands = lushtext_core::services::palette::all_commands();
    let cmd = commands.iter().find(|c| c.id == "win.toggle-focus-mode");
    assert!(cmd.is_some(), "all_commands() should include Focus Mode");
    let cmd = cmd.expect("expected operation to succeed");
    assert_eq!(cmd.label, "Focus Mode");
    assert_eq!(cmd.shortcut, Some("Ctrl+Shift+F11"));
    assert_eq!(cmd.category, CommandCategory::View);
}

#[test]
fn test_all_commands_contains_zoom_in() {
    ensure_gtk_init();
    let commands = lushtext_core::services::palette::all_commands();
    let cmd = commands.iter().find(|c| c.id == "win.zoom-in");
    assert!(cmd.is_some(), "all_commands() should include Zoom In");
    let cmd = cmd.expect("expected operation to succeed");
    assert_eq!(cmd.label, "Zoom In");
    assert_eq!(cmd.shortcut, Some("Ctrl+="));
    assert_eq!(cmd.category, CommandCategory::View);
}

#[test]
fn test_all_commands_contains_zoom_out() {
    ensure_gtk_init();
    let commands = lushtext_core::services::palette::all_commands();
    let cmd = commands.iter().find(|c| c.id == "win.zoom-out");
    assert!(cmd.is_some(), "all_commands() should include Zoom Out");
    let cmd = cmd.expect("expected operation to succeed");
    assert_eq!(cmd.label, "Zoom Out");
    assert_eq!(cmd.shortcut, Some("Ctrl+-"));
    assert_eq!(cmd.category, CommandCategory::View);
}

#[test]
fn test_all_commands_contains_zoom_reset() {
    ensure_gtk_init();
    let commands = lushtext_core::services::palette::all_commands();
    let cmd = commands.iter().find(|c| c.id == "win.zoom-reset");
    assert!(cmd.is_some(), "all_commands() should include Reset Zoom");
    let cmd = cmd.expect("expected operation to succeed");
    assert_eq!(cmd.label, "Reset Zoom");
    assert_eq!(cmd.shortcut, Some("Ctrl+0"));
    assert_eq!(cmd.category, CommandCategory::View);
}

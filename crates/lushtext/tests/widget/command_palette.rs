// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget and window-integration tests for command palette template wiring,
//! grouped results, keyboard flow, click-away dismissal, and focus restoration.

use crate::common::{
    ensure_gtk_init, fixture, flush_after_delay, flush_events, isolated_data_dir, present_window,
    wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::prelude::ToValue;
use gtk4::prelude::*;
use lushtext_core::model::palette::{
    CommandCategory, CommandDef, IndexedFile, PaletteFileEntry, PaletteFileIdentity,
    PaletteNoteCategory, PaletteNoteEntry, PaletteNoteTarget, SearchMode,
};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::{json_store, workspace_manager};
use lushtext_core::services::palette::{
    FileIndex, MAX_INDEXED_FILES, NoteSourceRefreshRequest,
};
use lushtext_core::ui::accessibility::{self, test_audit::AccessibleAudit};
use lushtext_core::ui::command_palette::{
    LushtextCommandPalette, apply_palette_row_accessibility_for_test,
    file_index_retirement_snapshot_for_test, set_index_update_delay_for_test,
    set_search_delay_for_test,
};
use lushtext_core::ui::command_palette::item::PaletteItem;
use lushtext_core::ui::plain_disposal::{
    hold_disposal_capacity_for_test, lane_snapshot_for_test,
};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

struct PaletteSearchDelayReset;

impl Drop for PaletteSearchDelayReset {
    fn drop(&mut self) {
        set_search_delay_for_test(0);
    }
}

struct IndexUpdateDelayReset;

impl Drop for IndexUpdateDelayReset {
    fn drop(&mut self) {
        set_index_update_delay_for_test(0);
    }
}

fn in_memory_palette_index(prefix: &str, count: usize) -> FileIndex {
    let root = Arc::new(PathBuf::from(format!("/synthetic/{prefix}")));
    FileIndex::from(
        (0..count)
            .map(|index| {
                let path = root.join(format!("{prefix}-{index:05}.rs"));
                IndexedFile::new(
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                    Arc::clone(&root),
                )
            })
            .collect::<Vec<_>>(),
    )
}

/// Keep older command-palette assertions on the shared harness wait semantics.
fn spin_until(predicate: impl Fn() -> bool) {
    wait_until(Duration::from_secs(2), predicate);
}

/// Present a test window and wait until the headless compositor allocates it.
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

/// Wait until persisted workspaces have loaded and the selected empty scope is reflected.
fn wait_for_empty_selected_workspace_index(window: &LushtextWindow, expected_total_folders: usize) {
    wait_until(Duration::from_secs(10), || {
        window.imp().sidebar.all_workspace_folder_paths().len() == expected_total_folders
            && window.imp().sidebar.current_scope_folder_paths().is_empty()
            && window.imp().command_palette.file_index_len() == 0
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

fn bookmark_note(title: &str, subtitle: &str, path: PathBuf, line: u32) -> PaletteNoteEntry {
    PaletteNoteEntry {
        category: PaletteNoteCategory::Bookmarks,
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        detail: None,
        note_text: None,
        target: PaletteNoteTarget::Bookmark {
            path,
            line,
            workspace_folders: Vec::new(),
        },
    }
}

fn open_tab_bookmark_note(title: &str, subtitle: &str, path: PathBuf, line: u32) -> PaletteNoteEntry {
    PaletteNoteEntry {
        category: PaletteNoteCategory::OpenTabs,
        ..bookmark_note(title, subtitle, path, line)
    }
}

fn text_note(
    category: PaletteNoteCategory,
    title: &str,
    subtitle: &str,
    detail: &str,
    body: &str,
    target: PaletteNoteTarget,
) -> PaletteNoteEntry {
    PaletteNoteEntry {
        category,
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        detail: Some(detail.to_string()),
        note_text: Some(body.to_string()),
        target,
    }
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
        identity: PaletteFileIdentity::canonical("/home/user/project/src/main.rs".into()),
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
        identity: PaletteFileIdentity::canonical("/home/user/project/Cargo.toml".into()),
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

#[test]
fn test_palette_item_note_is_activatable_without_body_text() {
    ensure_gtk_init();
    let path = PathBuf::from("/workspace/src/main.rs");
    let target = PaletteNoteTarget::Bookmark {
        path,
        line: 4,
        workspace_folders: vec![PathBuf::from("/workspace")],
    };
    let item = PaletteItem::new_note_raw(
        "Bookmark · Review",
        "/workspace/src/main.rs · Line 5",
        target.clone(),
    );

    assert_eq!(item.display_name(), "Bookmark · Review");
    assert_eq!(item.subtitle(), "/workspace/src/main.rs · Line 5");
    assert!(item.is_note());
    assert!(item.is_activatable());
    assert!(!item.is_file());
    assert!(!item.is_command());
    assert_eq!(item.note_target(), Some(target));
    assert!(item.file_path().is_none());
    assert!(item.action_id().is_empty());
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
fn test_command_palette_keyboard_mode_cycle_then_escape_restores_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window).expect("active editor");
    editor.source_view().grab_focus();
    wait_until(Duration::from_secs(2), || active_editor_has_focus(&window));

    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    assert!(window.imp().palette_revealer.reveals_child());

    assert_eq!(
        emit_key(
            palette.imp().search_entry.upcast_ref::<gtk4::Widget>(),
            gtk4::gdk::Key::Tab,
        ),
        glib::Propagation::Stop,
    );
    assert_eq!(
        emit_key(
            palette.imp().search_entry.upcast_ref::<gtk4::Widget>(),
            gtk4::gdk::Key::Tab,
        ),
        glib::Propagation::Stop,
    );
    assert_eq!(palette.mode(), SearchMode::Notes);
    assert_eq!(palette.imp().mode_dropdown.selected(), SearchMode::Notes.position());

    // Escape arrives as SearchEntry's stop-search signal, so this keeps the
    // test on GTK's keyboard path while avoiding compositor-level key injection.
    palette.imp().search_entry.emit_stop_search();
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(active_editor_has_focus(&window));
    assert!(window.imp().saved_focus.borrow().is_none());
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
fn test_command_palette_rapid_queries_keep_one_active_one_latest_and_final_accessibility() {
    ensure_gtk_init();
    let _delay_reset = PaletteSearchDelayReset;
    set_search_delay_for_test(150);
    let palette = LushtextCommandPalette::new();
    palette.set_file_index(in_memory_palette_index("rapid-final", 2_000));
    palette.open();

    palette.set_search_mode(SearchMode::Files);
    palette.set_query("rapid-intermediate");
    palette.set_query("rapid-final-01999");

    let pressure = palette.search_runtime_snapshot_for_test();
    assert_eq!(pressure.active, 1);
    assert_eq!(pressure.pending, 1);
    assert_eq!(pressure.active_high_water, 1);
    assert_eq!(pressure.pending_high_water, 1);
    assert!(palette.is_searching());
    assert!(gtk4::test_accessible_has_state(
        &*palette.imp().search_entry,
        gtk4::AccessibleState::Busy,
    ));

    wait_until(Duration::from_secs(10), || {
        !palette.is_searching()
            && palette_labels(&palette)
                .iter()
                .any(|label| label == "rapid-final-01999.rs")
    });

    assert!(palette.observed_search_cancellations_for_test() > 0);
    assert!(palette.last_cancelled_search_examined_for_test() <= 2_000);
    assert!(!palette_labels(&palette)
        .iter()
        .any(|label| label.contains("intermediate")));
    assert!(!gtk4::test_accessible_has_state(
        &*palette.imp().search_entry,
        gtk4::AccessibleState::Busy,
    ));
    AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::ValueText])
        .assert_on(&*palette.imp().results_view);
}

#[test]
fn test_command_palette_latest_mode_index_and_scope_snapshot_wins() {
    ensure_gtk_init();
    let _delay_reset = PaletteSearchDelayReset;
    set_search_delay_for_test(150);
    let palette = LushtextCommandPalette::new();
    palette.set_file_index(in_memory_palette_index("old-scope", 512));
    palette.set_workspace_group_label("Old Scope");
    palette.open();
    palette.set_query("old-scope-00511");

    palette.set_search_mode(SearchMode::Files);
    palette.set_file_index(in_memory_palette_index("latest-scope", 512));
    palette.set_workspace_group_label("Latest Scope");
    palette.set_query("latest-scope-00511");

    wait_until(Duration::from_secs(10), || {
        let labels = palette_labels(&palette);
        !palette.is_searching()
            && labels.iter().any(|label| label == "Latest Scope")
            && labels.iter().any(|label| label == "latest-scope-00511.rs")
    });
    let labels = palette_labels(&palette);
    assert!(!labels.iter().any(|label| label == "Old Scope"));
    assert!(!labels.iter().any(|label| label == "old-scope-00511.rs"));
    assert_eq!(palette.mode(), SearchMode::Files);
}

#[test]
fn test_command_palette_close_cancels_active_and_pending_without_stale_projection() {
    ensure_gtk_init();
    let _delay_reset = PaletteSearchDelayReset;
    set_search_delay_for_test(200);
    let palette = LushtextCommandPalette::new();
    palette.set_file_index(in_memory_palette_index("close-search", 2_000));
    palette.open();
    palette.set_query("close-search-00001");
    palette.set_query("close-search-01999");
    assert_eq!(palette.search_runtime_snapshot_for_test().pending, 1);

    palette.close();
    assert!(!palette.is_searching());
    assert_eq!(palette.result_count(), 0);
    assert_eq!(palette.search_runtime_snapshot_for_test().pending, 0);

    wait_until(Duration::from_secs(10), || {
        palette.search_runtime_snapshot_for_test().active == 0
    });
    assert_eq!(palette.result_count(), 0);
    assert!(!palette.imp().no_results_label.property::<bool>("visible"));
    assert!(palette.observed_search_cancellations_for_test() > 0);
}

#[test]
fn test_command_palette_incremental_index_worker_publishes_then_clears_readiness() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let root = Arc::new(PathBuf::from("/synthetic/incremental-index"));
    let existing = root.join("existing.rs");
    palette.set_file_index(FileIndex::from(vec![IndexedFile::new(
        existing.clone(),
        PaletteFileIdentity::canonical(existing),
        Arc::clone(&root),
    )]));
    palette.open();
    palette.set_search_mode(SearchMode::Files);
    palette.set_query("created-latest");

    palette.update_index_file_created(&root.join("created-latest.rs"));
    assert!(palette.pending_index_update_count() > 0);
    wait_until(Duration::from_secs(10), || {
        palette.pending_index_update_count() == 0
            && palette_labels(&palette)
                .iter()
                .any(|label| label == "created-latest.rs")
    });

    assert_eq!(palette.pending_index_update_count(), 0);
    assert!(palette_labels(&palette)
        .iter()
        .any(|label| label == "created-latest.rs"));
}

#[test]
fn test_incremental_index_capacity_retry_is_paced_and_resumes_after_release() {
    ensure_gtk_init();
    wait_until(Duration::from_secs(5), || {
        let snapshot = lane_snapshot_for_test();
        snapshot.running_jobs == 0 && snapshot.queued_jobs == 0
    });
    let capacity_hold = hold_disposal_capacity_for_test();
    let full_before = lane_snapshot_for_test().full_outcomes;
    let palette = LushtextCommandPalette::new();

    palette.update_index_file_deleted(Path::new("/synthetic/deferred-delete"));
    flush_after_delay(Duration::from_millis(200));

    assert_eq!(palette.pending_index_update_count(), 1);
    assert!(!palette.index_update_worker_running_for_test());
    let full_after_first_attempt = lane_snapshot_for_test().full_outcomes;
    assert_eq!(full_after_first_attempt, full_before + 1);
    flush_after_delay(Duration::from_millis(200));
    assert_eq!(
        lane_snapshot_for_test().full_outcomes,
        full_after_first_attempt,
        "capacity polling must not rerun the whole index mutation in a tight loop"
    );

    drop(capacity_hold);
    wait_until(Duration::from_secs(10), || {
        palette.pending_index_update_count() == 0
    });
    assert!(!palette.index_update_worker_running_for_test());
}

#[test]
fn test_incremental_index_update_queue_coalesces_overflow_to_one_rebuild() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let long_segment = "x".repeat(8 * 1024);

    for index in 0..2_000 {
        palette.update_index_file_deleted(&PathBuf::from(format!(
            "/synthetic/{index:05}-{long_segment}"
        )));
    }

    let (queued, bytes, rebuild_pending, count_limit, byte_limit) =
        palette.index_update_queue_snapshot_for_test();
    assert!(queued <= count_limit);
    assert!(bytes <= byte_limit);
    assert!(rebuild_pending);
    assert_eq!(palette.pending_index_update_count(), queued + 1);

    wait_until(Duration::from_secs(10), || {
        palette.pending_index_update_count() == 0
    });
    assert_eq!(
        palette.index_update_queue_snapshot_for_test(),
        (0, 0, false, count_limit, byte_limit)
    );
}

#[test]
fn test_command_palette_retires_last_owned_full_index_off_gtk() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.set_file_index(in_memory_palette_index("retire-full", MAX_INDEXED_FILES));

    palette.set_file_index(FileIndex::default());

    wait_until(Duration::from_secs(10), || {
        file_index_retirement_snapshot_for_test().full_replacements == 1
    });
    assert_eq!(
        file_index_retirement_snapshot_for_test().full_replacements,
        1
    );
}

#[test]
fn test_command_palette_retires_last_owned_accepted_incremental_index_off_gtk() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.set_file_index(in_memory_palette_index(
        "retire-accepted",
        MAX_INDEXED_FILES,
    ));

    palette.update_index_file_deleted(&PathBuf::from(
        "/synthetic/retire-accepted/retire-accepted-00000.rs",
    ));

    wait_until(Duration::from_secs(10), || {
        palette.pending_index_update_count() == 0
            && file_index_retirement_snapshot_for_test().accepted_incremental == 1
    });
    assert_eq!(
        file_index_retirement_snapshot_for_test().accepted_incremental,
        1
    );
}

#[test]
fn test_command_palette_retires_last_owned_rejected_incremental_index_off_gtk() {
    ensure_gtk_init();
    let _delay_reset = IndexUpdateDelayReset;
    set_index_update_delay_for_test(200);
    let palette = LushtextCommandPalette::new();
    palette.set_file_index(in_memory_palette_index(
        "retire-rejected",
        MAX_INDEXED_FILES,
    ));
    palette.update_index_file_deleted(&PathBuf::from(
        "/synthetic/retire-rejected/missing.rs",
    ));
    wait_until(Duration::from_secs(10), || {
        palette.index_update_worker_running_for_test()
    });

    palette.set_file_index(in_memory_palette_index("replacement", 1));

    wait_until(Duration::from_secs(10), || {
        palette.pending_index_update_count() == 0
            && file_index_retirement_snapshot_for_test().rejected_incremental == 1
    });
    assert_eq!(
        file_index_retirement_snapshot_for_test().rejected_incremental,
        1
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
        PaletteFileIdentity::canonical(duplicate.clone()),
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
        PaletteFileIdentity::canonical(PathBuf::from("/tmp/open_tab.rs")),
    )]);
    palette.set_note_entries(vec![
        bookmark_note(
            "Bookmark · Open task",
            "Project · /tmp/open_note.rs · Line 2",
            PathBuf::from("/tmp/open_note.rs"),
            1,
        ),
        text_note(
            PaletteNoteCategory::DocumentNotes,
            "Document Note · open_note.rs",
            "Project · /workspace/open_note.rs",
            "Open document note",
            "Open document note body",
            PaletteNoteTarget::DocumentNote {
                path: PathBuf::from("/workspace/open_note.rs"),
                workspace_folders: vec![PathBuf::from("/workspace")],
            },
        ),
        open_tab_bookmark_note(
            "Bookmark · Open tab note",
            "Open tab · Outside workspace · /tmp/open_tab_note.rs · Line 3",
            PathBuf::from("/tmp/open_tab_note.rs"),
            2,
        ),
    ]);
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::All);

    rebuild_and_wait_for_label(&palette, "open", "Commands");
    let labels = palette_labels(&palette);

    let open_tabs = row_position(&labels, "Open Tabs");
    let workspace = row_position(&labels, "Selected Workspace");
    let bookmarks = row_position(&labels, "Bookmarks");
    let document_notes = row_position(&labels, "Document Notes");
    let open_tab_notes = row_position(&labels, "Open Tab Notes");
    let commands = row_position(&labels, "Commands");
    assert!(
        open_tabs < workspace
            && workspace < bookmarks
            && bookmarks < document_notes
            && document_notes < open_tab_notes
            && open_tab_notes < commands,
        "All mode groups should preserve priority: {labels:?}",
    );
    assert!(labels.iter().any(|label| label == "open_tab.rs"));
    assert!(labels.iter().any(|label| label == "open_workspace.rs"));
    assert!(labels.iter().any(|label| label == "Bookmark · Open task"));
    assert!(labels.iter().any(|label| label == "Document Note · open_note.rs"));
    assert!(labels.iter().any(|label| label == "Bookmark · Open tab note"));
    assert!(labels.iter().any(|label| label == "Open Document Note"));
    assert!(labels.iter().any(|label| label == "Open File"));
    assert!(!labels.iter().any(|label| label == "Notes"));
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
fn test_command_palette_notes_mode_groups_note_records_by_category() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.set_note_entries(vec![
        bookmark_note(
            "Bookmark · Review parser",
            "Core · /workspace/src/parser.rs · Line 8",
            PathBuf::from("/workspace/src/parser.rs"),
            7,
        ),
        text_note(
            PaletteNoteCategory::FolderNotes,
            "Folder Note · Core",
            "Core · /workspace",
            "Folder planning",
            "Folder planning body",
            PaletteNoteTarget::FolderNote {
                workspace_name: "Core".to_string(),
                folder: PathBuf::from("/workspace"),
            },
        ),
        text_note(
            PaletteNoteCategory::DocumentNotes,
            "Document Note · parser.rs",
            "Core · /workspace · /workspace/src/parser.rs",
            "Document planning",
            "Document planning body",
            PaletteNoteTarget::DocumentNote {
                path: PathBuf::from("/workspace/src/parser.rs"),
                workspace_folders: vec![PathBuf::from("/workspace")],
            },
        ),
        open_tab_bookmark_note(
            "Bookmark · Outside tab",
            "Open tab · Outside workspace · /tmp/outside.md · Line 3",
            PathBuf::from("/tmp/outside.md"),
            2,
        ),
    ]);
    palette.imp().set_mode(SearchMode::Notes);

    rebuild_and_wait_for_label(&palette, "", "Open Tabs");
    let labels = palette_labels(&palette);

    let bookmarks = row_position(&labels, "Bookmarks");
    let folder_notes = row_position(&labels, "Folder Notes");
    let document_notes = row_position(&labels, "Document Notes");
    let open_tabs = row_position(&labels, "Open Tabs");
    assert!(
        bookmarks < folder_notes && folder_notes < document_notes && document_notes < open_tabs,
        "Notes mode groups should preserve note category order: {labels:?}",
    );
    assert!(labels.iter().any(|label| label == "Bookmark · Review parser"));
    assert!(labels.iter().any(|label| label == "Folder Note · Core"));
    assert!(labels.iter().any(|label| label == "Document Note · parser.rs"));
    assert!(labels.iter().any(|label| label == "Bookmark · Outside tab"));
    assert!(!labels.iter().any(|label| label == "Commands"));
    assert!(!labels.iter().any(|label| label == "Browse Notes"));
}

#[test]
fn test_command_palette_notes_mode_excludes_files_and_commands() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("open_notes.rs"), "");

    palette.set_workspace_group_label("Selected Workspace");
    palette.set_open_tabs(vec![PaletteFileEntry::new(
        "open_notes_tab.rs".to_string(),
        "/tmp/open_notes_tab.rs".to_string(),
        PathBuf::from("/tmp/open_notes_tab.rs"),
        PaletteFileIdentity::canonical(PathBuf::from("/tmp/open_notes_tab.rs")),
    )]);
    palette.set_note_entries(vec![text_note(
        PaletteNoteCategory::DocumentNotes,
        "Document Note · open_notes.rs",
        "Project · /workspace/open_notes.rs",
        "Open note detail",
        "Open note body",
        PaletteNoteTarget::DocumentNote {
            path: PathBuf::from("/workspace/open_notes.rs"),
            workspace_folders: vec![PathBuf::from("/workspace")],
        },
    )]);
    palette.set_file_index(FileIndex::rebuild(&[dir.path().to_path_buf()]));
    palette.imp().set_mode(SearchMode::Notes);

    rebuild_and_wait_for_label(&palette, "open", "Document Note · open_notes.rs");
    let labels = palette_labels(&palette);

    assert!(labels.iter().any(|label| label == "Document Note · open_notes.rs"));
    assert!(!labels.iter().any(|label| label == "Selected Workspace"));
    assert!(!labels.iter().any(|label| label == "open_notes.rs"));
    assert!(!labels.iter().any(|label| label == "open_notes_tab.rs"));
    assert!(!labels.iter().any(|label| label == "Open File"));
    assert!(!labels.iter().any(|label| label == "Open Document Note"));
}

#[test]
fn test_command_palette_notes_mode_empty_source_has_no_fake_rows() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.imp().set_mode(SearchMode::Notes);

    palette.imp().rebuild_results("");
    wait_until(Duration::from_secs(5), || palette_labels(&palette).is_empty());

    assert!(palette_labels(&palette).is_empty());
    assert!(
        !palette.imp().no_results_label.property::<bool>("visible"),
        "empty default Notes mode should not show a no-results warning"
    );

    palette.imp().rebuild_results("missing-note");
    wait_until(Duration::from_secs(5), || {
        palette.imp().no_results_label.property::<bool>("visible")
    });
    assert!(palette_labels(&palette).is_empty());
}

#[test]
fn test_command_palette_notes_mode_handles_dense_awkward_rows() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let entries = (0..80)
        .map(|index| {
            bookmark_note(
                &format!(
                    "Bookmark · Dense Awkward Label {index:02} With Extra Long Searchable Text"
                ),
                &format!(
                    "Dense Workspace · /workspace/deeply/nested/path/{index:02}/file.rs · Line {}",
                    index + 1
                ),
                PathBuf::from(format!("/workspace/deeply/nested/path/{index:02}/file.rs")),
                u32::try_from(index).expect("dense test index should fit u32"),
            )
        })
        .collect();
    palette.set_note_entries(entries);
    palette.imp().set_mode(SearchMode::Notes);

    rebuild_and_wait_until(&palette, "dense awkward", |labels| labels.len() == 51);
    let labels = palette_labels(&palette);
    assert_eq!(labels.first().map(String::as_str), Some("Bookmarks"));
    assert_eq!(
        labels
            .iter()
            .filter(|label| label.starts_with("Bookmark · Dense Awkward Label"))
            .count(),
        50,
        "Notes mode should cap dense category rows without adding unrelated groups: {labels:?}"
    );
    assert!(!labels.iter().any(|label| label == "Commands"));
}

#[test]
fn test_command_palette_notes_mode_matches_note_content_and_metadata() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.set_note_entries(vec![
        bookmark_note(
            "Bookmark · Ship checkpoint",
            "Release Workspace · /workspace/src/lib.rs · Line 42",
            PathBuf::from("/workspace/src/lib.rs"),
            41,
        ),
        text_note(
            PaletteNoteCategory::FolderNotes,
            "Folder Note · Release Workspace",
            "Release Workspace · /workspace/docs",
            "Folder note detail",
            "Folder body mentions migration checklist",
            PaletteNoteTarget::FolderNote {
                workspace_name: "Release Workspace".to_string(),
                folder: PathBuf::from("/workspace/docs"),
            },
        ),
        text_note(
            PaletteNoteCategory::DocumentNotes,
            "Document Note · lib.rs",
            "Release Workspace · /workspace · /workspace/src/lib.rs",
            "Document note detail",
            "Document body mentions rollout proof",
            PaletteNoteTarget::DocumentNote {
                path: PathBuf::from("/workspace/src/lib.rs"),
                workspace_folders: vec![PathBuf::from("/workspace")],
            },
        ),
    ]);
    palette.imp().set_mode(SearchMode::Notes);

    rebuild_and_wait_for_label(&palette, "rollout proof", "Document Note · lib.rs");
    assert_eq!(palette_labels(&palette), vec!["Document Notes", "Document Note · lib.rs"]);

    rebuild_and_wait_for_label(&palette, "migration checklist", "Folder Note · Release Workspace");
    assert_eq!(
        palette_labels(&palette),
        vec!["Folder Notes", "Folder Note · Release Workspace"]
    );

    rebuild_and_wait_for_label(&palette, "Ship checkpoint", "Bookmark · Ship checkpoint");
    assert_eq!(
        palette_labels(&palette),
        vec!["Bookmarks", "Bookmark · Ship checkpoint"]
    );

    rebuild_and_wait_for_label(&palette, "Line 42", "Bookmark · Ship checkpoint");
    assert!(palette_labels(&palette).iter().any(|label| label == "Bookmark · Ship checkpoint"));

    rebuild_and_wait_for_label(&palette, "/workspace/docs", "Folder Note · Release Workspace");
    assert!(palette_labels(&palette)
        .iter()
        .any(|label| label == "Folder Note · Release Workspace"));

    rebuild_and_wait_for_label(&palette, "/workspace/src/lib.rs", "Document Note · lib.rs");
    let labels = palette_labels(&palette);
    assert!(labels.iter().any(|label| label == "Bookmark · Ship checkpoint"));
    assert!(labels.iter().any(|label| label == "Document Note · lib.rs"));
}

#[test]
fn test_command_palette_note_rows_activate_all_target_variants() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let bookmark_path = PathBuf::from("/workspace/src/lib.rs");
    let folder_path = PathBuf::from("/workspace/docs");
    let document_path = PathBuf::from("/workspace/README.md");
    palette.set_note_entries(vec![
        bookmark_note(
            "Bookmark · Activate bookmark",
            "Project · /workspace/src/lib.rs · Line 7",
            bookmark_path.clone(),
            6,
        ),
        text_note(
            PaletteNoteCategory::FolderNotes,
            "Folder Note · Project",
            "Project · /workspace/docs",
            "Folder activation",
            "Folder activation body",
            PaletteNoteTarget::FolderNote {
                workspace_name: "Project".to_string(),
                folder: folder_path.clone(),
            },
        ),
        text_note(
            PaletteNoteCategory::DocumentNotes,
            "Document Note · README.md",
            "Project · /workspace · /workspace/README.md",
            "Document activation",
            "Document activation body",
            PaletteNoteTarget::DocumentNote {
                path: document_path.clone(),
                workspace_folders: vec![PathBuf::from("/workspace")],
            },
        ),
    ]);
    palette.imp().set_mode(SearchMode::Notes);
    rebuild_and_wait_for_label(&palette, "", "Document Note · README.md");

    let activated = Rc::new(RefCell::new(Vec::new()));
    let activated_clone = activated.clone();
    palette.connect_item_activated(move |item| {
        if let Some(target) = item.note_target() {
            activated_clone.borrow_mut().push(target);
        }
    });

    let selection = palette
        .imp()
        .results_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .expect("results should use a SingleSelection model");
    for label in [
        "Bookmark · Activate bookmark",
        "Folder Note · Project",
        "Document Note · README.md",
    ] {
        let position = u32::try_from(row_position(&palette_labels(&palette), label))
            .expect("palette row position should fit GTK selection index");
        selection.set_selected(position);
        palette.imp().activate_selected();
    }

    let targets = activated.borrow();
    assert!(matches!(
        &targets[0],
        PaletteNoteTarget::Bookmark { path, line, .. }
            if path == &bookmark_path && *line == 6
    ));
    assert!(matches!(
        &targets[1],
        PaletteNoteTarget::FolderNote {
            workspace_name,
            folder,
        } if workspace_name == "Project" && folder == &folder_path
    ));
    assert!(matches!(
        &targets[2],
        PaletteNoteTarget::DocumentNote {
            path,
            workspace_folders,
        } if path == &document_path && workspace_folders == &vec![PathBuf::from("/workspace")]
    ));
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
        PaletteFileIdentity::canonical(PathBuf::from("/tmp/alpha.rs")),
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
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*palette.imp().no_results_label);
    assert!(gtk4::test_accessible_has_state(
        &*palette.imp().results_view,
        gtk4::AccessibleState::Hidden
    ));
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
use lushtext_core::model::automation::AutomationReadinessPredicate;
use lushtext_core::ui::automation::{current_idle_blocker, wait_for_ready_for_test};
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

fn command_palette_entry_has_focus(window: &LushtextWindow) -> bool {
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return false;
    };
    let search_entry = window
        .imp()
        .command_palette
        .imp()
        .search_entry
        .upcast_ref::<gtk4::Widget>();
    focus.as_ptr() == search_entry.as_ptr() || focus.is_ancestor(search_entry)
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
fn test_command_palette_current_query_blocks_search_readiness_until_final_completion() {
    ensure_gtk_init();
    let _delay_reset = PaletteSearchDelayReset;
    set_search_delay_for_test(250);
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window).expect("active editor");
    editor.source_view().grab_focus();
    wait_until(Duration::from_secs(2), || active_editor_has_focus(&window));
    window
        .imp()
        .command_palette
        .set_file_index(in_memory_palette_index("readiness-final", 2_000));
    activate_action(&window, "toggle-command-palette");
    window
        .imp()
        .command_palette
        .set_search_mode(SearchMode::Files);
    window
        .imp()
        .command_palette
        .set_query("readiness-final-01999");
    let app = window
        .application()
        .expect("window application")
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("LushText application");

    assert_eq!(
        current_idle_blocker(&app).as_deref(),
        Some("command-palette-search")
    );
    let pending = glib::MainContext::default().block_on(wait_for_ready_for_test(
        app.clone(),
        AutomationReadinessPredicate::SearchComplete,
        1,
    ));
    assert!(!pending.ok);
    assert_eq!(pending.blocker.as_deref(), Some("command-palette-search"));

    wait_until(Duration::from_secs(10), || {
        !window.imp().command_palette.is_searching()
    });
    assert_eq!(current_idle_blocker(&app), None);
    assert!(command_palette_entry_has_focus(&window));
}

#[test]
fn test_command_palette_note_source_refresh_blocks_idle_until_terminal_finish() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let app = window
        .application()
        .expect("window application")
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("LushText application");
    wait_until(Duration::from_secs(5), || {
        current_idle_blocker(&app).is_none()
    });
    let scope = WorkspacesFile {
        current_scope: WorkspaceScope::All,
        workspaces: Vec::new(),
    }
    .current_scope_snapshot();
    let start = window
        .imp()
        .command_palette_note_refreshes
        .borrow_mut()
        .submit(NoteSourceRefreshRequest {
            data_dir: PathBuf::from("/synthetic/note-refresh"),
            scope_snapshot: scope,
            open_editor_snapshots: Arc::from([]),
            open_editor_snapshots_truncated: false,
            mode: lushtext_core::services::palette::NotesBrowserMode::AllNotes,
            limits: lushtext_core::services::palette::PALETTE_NOTE_SOURCE_LIMITS,
        })
        .expect("first note-source request starts");

    assert_eq!(
        current_idle_blocker(&app).as_deref(),
        Some("command-palette-index")
    );

    assert!(
        window
            .imp()
            .command_palette_note_refreshes
            .borrow_mut()
            .finish(start.generation)
            .is_none()
    );
    assert_eq!(current_idle_blocker(&app), None);
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

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::SearchBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*palette.imp().search_entry);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::ComboBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*palette.imp().mode_dropdown);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::List)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*palette.imp().results_view);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .states(&[gtk4::AccessibleState::Hidden])
        .assert_on(&*palette.imp().no_results_label);
}

#[test]
fn test_palette_accessibility_tracks_busy_and_selected_result_value() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    palette.imp().searching.set(true);
    palette.refresh_accessibility_state_for_test();
    assert!(gtk4::test_accessible_has_state(
        &*palette.imp().search_entry,
        gtk4::AccessibleState::Busy
    ));
    assert!(gtk4::test_accessible_has_state(
        &*palette.imp().results_view,
        gtk4::AccessibleState::Busy
    ));

    palette.imp().searching.set(false);
    palette.open();
    spin_until(|| palette.result_count() > 0);
    AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::ValueText])
        .assert_on(&*palette.imp().results_view);
}

#[test]
fn test_palette_row_accessibility_metadata_is_positioned_selected_and_clearable() {
    ensure_gtk_init();
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let item = PaletteItem::new_file_raw(
        "main.rs".to_string(),
        "src/main.rs".to_string(),
        PathBuf::from("/workspace/src/main.rs"),
    );

    apply_palette_row_accessibility_for_test(&row, &item, true, 2, 4);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .states(&[gtk4::AccessibleState::Selected])
        .relations(&[
            gtk4::AccessibleRelation::PosInSet,
            gtk4::AccessibleRelation::SetSize,
        ])
        .assert_on(&row);

    accessibility::clear_row_accessibility(&row);
    assert!(!gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Label
    ));
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
    let _data_dir = isolated_data_dir();
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
    wait_for_empty_selected_workspace_index(&window, 1);

    activate_action(&window, "toggle-command-palette");
    wait_until(Duration::from_secs(5), || {
        window.imp().palette_revealer.reveals_child()
    });
    let palette = window.imp().command_palette.clone();
    palette.imp().set_mode(SearchMode::Files);
    palette.imp().rebuild_results("beta");
    wait_until(Duration::from_secs(10), || {
        palette.imp().no_results_label.property::<bool>("visible")
            && palette_labels(&palette).is_empty()
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
    wait_until(Duration::from_secs(5), || {
        active_editor(&window)
            .and_then(|editor| editor.file_path())
            .as_deref()
            == Some(outside_file.as_path())
    });

    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.imp().set_mode(SearchMode::Files);
    palette.imp().rebuild_results("beta");
    wait_until(Duration::from_secs(5), || {
        let labels = palette_labels(&palette);
        labels.iter().any(|label| label == "beta.rs")
            && labels.iter().any(|label| label == "Open Tabs")
            && !labels.iter().any(|label| label == "Selected Workspace")
            && !labels.iter().any(|label| label == "alpha.rs")
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

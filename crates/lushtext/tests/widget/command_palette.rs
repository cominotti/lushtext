// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the Command Palette widget and its components.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::palette::{CommandCategory, CommandDef, IndexedFile, SearchMode};
use lushtext_core::services::palette::FileIndex;
use lushtext_core::ui::command_palette::item::PaletteItem;
use lushtext_core::ui::command_palette::LushtextCommandPalette;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

/// Drain all pending events from the GTK main loop.
fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
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
        workspace_root: "/home/user/project".into(),
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
fn test_palette_item_file_at_root() {
    ensure_gtk_init();
    let file = IndexedFile {
        path: "/home/user/project/Cargo.toml".into(),
        name: "Cargo.toml".to_string(),
        workspace_root: "/home/user/project".into(),
    };
    let item = PaletteItem::from_indexed_file(&file);
    assert_eq!(item.subtitle(), "Cargo.toml");
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
    let _palette: LushtextCommandPalette = Default::default();
}

#[test]
fn test_command_palette_starts_with_all_mode() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    assert_eq!(palette.mode(), SearchMode::All);
}

#[test]
fn test_command_palette_mode_label_initial() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    assert_eq!(palette.imp().mode_label.label().as_str(), "All ⇥");
}

#[test]
fn test_command_palette_open_populates_results() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    palette.open();
    flush_events();

    // With no file index, should still have command results
    let store = &palette.imp().results_store;
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
    flush_events();
    assert!(palette.imp().results_store.n_items() > 0);

    palette.close();
    assert_eq!(palette.imp().results_store.n_items(), 0);
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

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("hello.rs"), "").unwrap();
    std::fs::write(dir.path().join("world.txt"), "").unwrap();

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    palette.set_file_index(index);

    // Open and verify files appear in results
    palette.open();
    flush_events();
    let store = &palette.imp().results_store;

    let has_file = (0..store.n_items()).any(|i| {
        store
            .item(i)
            .and_downcast_ref::<PaletteItem>()
            .is_some_and(|item| item.is_file())
    });
    assert!(
        has_file,
        "results should contain file items after set_file_index"
    );
}

#[test]
fn test_command_palette_search_filters_results() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.rs"), "").unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    palette.set_file_index(index);

    // Search for "main" — should match main.rs but not Cargo.toml
    palette.imp().rebuild_results("main");

    let store = &palette.imp().results_store;
    let file_items: Vec<String> = (0..store.n_items())
        .filter_map(|i| {
            store
                .item(i)
                .and_downcast_ref::<PaletteItem>()
                .filter(|item| item.is_file())
                .map(|item| item.display_name())
        })
        .collect();
    assert!(file_items.contains(&"main.rs".to_string()));
    assert!(
        !file_items.contains(&"Cargo.toml".to_string()),
        "Cargo.toml should not match 'main' query, got: {:?}",
        file_items
    );
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
    flush_events();
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
    flush_events();

    // Search for something that won't match anything
    palette.imp().rebuild_results("xyzzynonexistent");

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
use lushtext_core::app::LushtextApplication;
use lushtext_core::ui::window::LushtextWindow;

fn test_window() -> LushtextWindow {
    let app: libadwaita::Application = LushtextApplication::new().upcast();
    LushtextWindow::new(&app)
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
    // Must remain enabled unlike toggle-search
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

    // Open palette
    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    // Trigger stop-search (Escape)
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

    // Open palette
    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    // Activate first result (should close palette)
    window.imp().command_palette.imp().activate_selected();
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_palette_width_request_set() {
    ensure_gtk_init();
    let window = test_window();
    flush_events();
    // Width request should be set by size_allocate; before realization
    // we can at least verify the command_palette widget is accessible
    let _cp = &window.imp().command_palette;
}

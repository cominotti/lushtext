// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextSidebar context menu, rename, delete, and model manipulation.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use lushtext_core::app::LushtextApplication;
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::sidebar::LushtextSidebar;
use lushtext_core::ui::window::LushtextWindow;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

/// Create a window attached to a test application.
fn test_window() -> LushtextWindow {
    let app: libadwaita::Application = LushtextApplication::new().upcast();
    LushtextWindow::new(&app)
}

/// Drain all pending events from the GTK main loop.
fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

// --- Context menu construction ---

#[test]
fn test_context_menu_created() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert!(sidebar.imp().context_menu.borrow().is_some());
}

#[test]
fn test_context_menu_popover_has_no_arrow() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    let popover = sidebar.imp().context_menu.borrow();
    let popover = popover.as_ref().unwrap();
    assert!(!popover.has_arrow());
}

// --- Model manipulation: remove_from_model ---

#[test]
fn test_remove_from_model_root_item() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    // Set up root store manually (bypasses async scan)
    let root_store = gio::ListStore::new::<FileTreeItem>();
    root_store.append(&FileTreeItem::new(PathBuf::from("/tmp/test/a.txt"), false));
    root_store.append(&FileTreeItem::new(PathBuf::from("/tmp/test/b.txt"), false));
    *sidebar.imp().root_store.borrow_mut() = Some(root_store.clone());

    assert_eq!(root_store.n_items(), 2);
    sidebar.remove_from_model(std::path::Path::new("/tmp/test/a.txt"));
    assert_eq!(root_store.n_items(), 1);

    // Remaining item should be b.txt
    let remaining = root_store.item(0).and_downcast::<FileTreeItem>().unwrap();
    assert_eq!(remaining.path(), PathBuf::from("/tmp/test/b.txt"));
}

#[test]
fn test_remove_from_model_nonexistent_is_noop() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let root_store = gio::ListStore::new::<FileTreeItem>();
    root_store.append(&FileTreeItem::new(PathBuf::from("/tmp/test/a.txt"), false));
    *sidebar.imp().root_store.borrow_mut() = Some(root_store.clone());

    sidebar.remove_from_model(std::path::Path::new("/tmp/test/does_not_exist.txt"));
    assert_eq!(root_store.n_items(), 1);
}

#[test]
fn test_remove_from_model_child_item() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    // Set up a root store with a directory
    let root_store = gio::ListStore::new::<FileTreeItem>();
    let dir_item = FileTreeItem::new(PathBuf::from("/tmp/test/src"), true);
    root_store.append(&dir_item);
    *sidebar.imp().root_store.borrow_mut() = Some(root_store.clone());

    // Create a child store for the directory
    let child_store = gio::ListStore::new::<FileTreeItem>();
    child_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/src/main.rs"),
        false,
    ));
    child_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/src/lib.rs"),
        false,
    ));

    // Build TreeListModel that returns our child store for the directory
    let child_store_c = child_store.clone();
    let tree_model = gtk4::TreeListModel::new(root_store.clone(), false, false, move |item| {
        let fi = item.downcast_ref::<FileTreeItem>()?;
        if fi.is_dir() && fi.path() == std::path::Path::new("/tmp/test/src") {
            Some(child_store_c.clone().upcast::<gio::ListModel>())
        } else {
            None
        }
    });
    *sidebar.imp().tree_model.borrow_mut() = Some(tree_model.clone());

    // Expand the directory row so children() is available
    if let Some(row) = tree_model.item(0).and_downcast::<gtk4::TreeListRow>() {
        row.set_expanded(true);
    }

    // Verify child store has 2 items
    assert_eq!(child_store.n_items(), 2);

    // Remove one child
    sidebar.remove_from_model(std::path::Path::new("/tmp/test/src/main.rs"));
    assert_eq!(child_store.n_items(), 1);

    let remaining = child_store.item(0).and_downcast::<FileTreeItem>().unwrap();
    assert_eq!(remaining.path(), PathBuf::from("/tmp/test/src/lib.rs"));
}

// --- Callback wiring ---

#[test]
fn test_rename_callback_fires() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let called = Rc::new(Cell::new(false));
    let old_seen = Rc::new(Cell::new(PathBuf::new()));
    let new_seen = Rc::new(Cell::new(PathBuf::new()));

    let called_c = called.clone();
    let old_c = old_seen.clone();
    let new_c = new_seen.clone();
    sidebar.connect_file_renamed(move |old, new| {
        called_c.set(true);
        old_c.set(old.to_path_buf());
        new_c.set(new.to_path_buf());
    });

    // Simulate rename callback invocation (as done in confirm_rename)
    if let Some(ref cb) = *sidebar.imp().rename_callback.borrow() {
        cb(
            std::path::Path::new("/tmp/old.txt"),
            std::path::Path::new("/tmp/new.txt"),
        );
    }

    assert!(called.get());
    assert_eq!(old_seen.take(), PathBuf::from("/tmp/old.txt"));
    assert_eq!(new_seen.take(), PathBuf::from("/tmp/new.txt"));
}

#[test]
fn test_delete_callback_fires() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let called = Rc::new(Cell::new(false));
    let path_seen = Rc::new(Cell::new(PathBuf::new()));

    let called_c = called.clone();
    let path_c = path_seen.clone();
    sidebar.connect_file_deleted(move |path| {
        called_c.set(true);
        path_c.set(path.to_path_buf());
    });

    if let Some(ref cb) = *sidebar.imp().delete_callback.borrow() {
        cb(std::path::Path::new("/tmp/deleted.txt"));
    }

    assert!(called.get());
    assert_eq!(path_seen.take(), PathBuf::from("/tmp/deleted.txt"));
}

// --- Window integration: tab path updates ---

#[test]
fn test_update_tab_path_exact_match() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.rs");
    std::fs::write(&old_path, "fn main() {}").unwrap();
    window.open_document(&old_path);

    let new_path = dir.path().join("new.rs");
    window.update_tab_path(&old_path, &new_path);

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "new.rs");
}

#[test]
fn test_update_tab_path_directory_prefix_rewrite() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let old_dir = dir.path().join("old_dir");
    std::fs::create_dir(&old_dir).unwrap();
    let file_path = old_dir.join("file.rs");
    std::fs::write(&file_path, "content").unwrap();
    window.open_document(&file_path);

    // Rename the parent directory
    let new_dir = dir.path().join("new_dir");
    window.update_tab_path(&old_dir, &new_dir);

    // The tab should now point to new_dir/file.rs
    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "file.rs");

    let editor = page
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .unwrap();
    assert_eq!(editor.file_path().unwrap(), new_dir.join("file.rs"));
}

#[test]
fn test_update_tab_path_no_match_is_noop() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("keep.rs");
    std::fs::write(&file_path, "content").unwrap();
    window.open_document(&file_path);

    // Rename a different file — should not affect this tab
    window.update_tab_path(
        std::path::Path::new("/tmp/other.rs"),
        std::path::Path::new("/tmp/renamed.rs"),
    );

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "keep.rs");
}

// --- Window integration: close tabs ---

#[test]
fn test_close_tab_for_path_exact() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("doomed.rs");
    std::fs::write(&file_path, "").unwrap();
    window.open_document(&file_path);
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    window.close_tab_for_path(&file_path);
    flush_events();
    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_close_tab_for_path_directory_closes_children() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let f1 = sub.join("a.rs");
    let f2 = sub.join("b.rs");
    let f3 = dir.path().join("outside.rs");
    std::fs::write(&f1, "").unwrap();
    std::fs::write(&f2, "").unwrap();
    std::fs::write(&f3, "").unwrap();

    window.open_document(&f1);
    window.open_document(&f2);
    window.open_document(&f3);
    assert_eq!(window.imp().tab_view.n_pages(), 3);

    // Deleting the "sub" directory should close f1 and f2 but not f3
    window.close_tab_for_path(&sub);
    flush_events();
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    let remaining = window
        .imp()
        .tab_view
        .nth_page(0)
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .unwrap();
    assert_eq!(remaining.file_path().unwrap(), f3);
}

#[test]
fn test_close_tab_for_path_nonexistent_is_noop() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert_eq!(window.imp().tab_view.n_pages(), 1);

    window.close_tab_for_path(std::path::Path::new("/does/not/exist"));
    flush_events();
    assert_eq!(window.imp().tab_view.n_pages(), 1);
}

// --- FileTreeItem pending_rename ---

#[test]
fn test_file_tree_item_pending_rename_default_false() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/tmp/test.txt"), false);
    assert!(!item.is_pending_rename());
}

#[test]
fn test_file_tree_item_pending_rename_set_and_clear() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/tmp/test.txt"), false);
    item.set_pending_rename(true);
    assert!(item.is_pending_rename());
    item.set_pending_rename(false);
    assert!(!item.is_pending_rename());
}

// --- New file/folder creation callback ---

#[test]
fn test_create_callback_fires() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let called = Rc::new(Cell::new(false));
    let path_seen = Rc::new(Cell::new(PathBuf::new()));

    let called_c = called.clone();
    let path_c = path_seen.clone();
    sidebar.connect_file_created(move |path| {
        called_c.set(true);
        path_c.set(path.to_path_buf());
    });

    if let Some(ref cb) = *sidebar.imp().create_callback.borrow() {
        cb(std::path::Path::new("/tmp/new_file.txt"));
    }

    assert!(called.get());
    assert_eq!(path_seen.take(), PathBuf::from("/tmp/new_file.txt"));
}

// --- ListView properties ---

#[test]
fn test_sidebar_double_click_activate_property() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert!(!sidebar.imp().file_tree_view.is_single_click_activate());
}

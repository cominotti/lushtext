// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextWorkspaceSection widget.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use lushtext_core::model::workspace::WorkspaceId;
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::sidebar::workspace_section::LushtextWorkspaceSection;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

// --- Construction ---

#[test]
fn test_workspace_section_new() {
    ensure_gtk_init();
    let _section = LushtextWorkspaceSection::new(WorkspaceId::new("test-id"));
}

#[test]
fn test_workspace_section_stores_id() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("my-ws-id"));
    assert_eq!(section.workspace_id(), WorkspaceId::new("my-ws-id"));
}

#[test]
fn test_workspace_section_default_name() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert_eq!(section.workspace_name(), "New Workspace");
}

#[test]
fn test_workspace_section_set_name() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    section.set_workspace_name("my project");
    assert_eq!(section.workspace_name(), "my project");
}

#[test]
fn test_workspace_section_header_label_does_not_ellipsize() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert_eq!(
        section.imp().header_label.ellipsize(),
        gtk4::pango::EllipsizeMode::None
    );
}

#[test]
fn test_workspace_section_inner_scroller_does_not_propagate_natural_width() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert!(!section.imp().inner_scrolled_window.propagates_natural_width());
}

// --- Context menu ---

#[test]
fn test_workspace_section_file_context_menu_created() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert!(section.imp().context_menu.borrow().is_some());
}

#[test]
fn test_workspace_section_context_menu_no_arrow() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    let popover = section.imp().context_menu.borrow();
    let popover = popover.as_ref().unwrap();
    assert!(!popover.has_arrow());
}

// --- Model manipulation: remove_from_model ---

#[test]
fn test_remove_from_model_root_item() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let root_store = gio::ListStore::new::<FileTreeItem>();
    root_store.append(&FileTreeItem::new(PathBuf::from("/tmp/test/a.txt"), false, None));
    root_store.append(&FileTreeItem::new(PathBuf::from("/tmp/test/b.txt"), false, None));
    *section.imp().root_store.borrow_mut() = Some(root_store.clone());

    assert_eq!(root_store.n_items(), 2);
    section.remove_from_model(std::path::Path::new("/tmp/test/a.txt"));
    assert_eq!(root_store.n_items(), 1);

    let remaining = root_store.item(0).and_downcast::<FileTreeItem>().unwrap();
    assert_eq!(remaining.path(), Some(PathBuf::from("/tmp/test/b.txt")));
}

#[test]
fn test_remove_from_model_nonexistent_is_noop() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let root_store = gio::ListStore::new::<FileTreeItem>();
    root_store.append(&FileTreeItem::new(PathBuf::from("/tmp/test/a.txt"), false, None));
    *section.imp().root_store.borrow_mut() = Some(root_store.clone());

    section.remove_from_model(std::path::Path::new("/tmp/test/does_not_exist.txt"));
    assert_eq!(root_store.n_items(), 1);
}

#[test]
fn test_remove_from_model_child_item() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let root_store = gio::ListStore::new::<FileTreeItem>();
    let dir_item = FileTreeItem::new(PathBuf::from("/tmp/test/src"), true, None);
    root_store.append(&dir_item);
    *section.imp().root_store.borrow_mut() = Some(root_store.clone());

    let child_store = gio::ListStore::new::<FileTreeItem>();
    child_store.append(&FileTreeItem::new(
PathBuf::from("/tmp/test/src/main.rs"),
false,
None
));
    child_store.append(&FileTreeItem::new(
PathBuf::from("/tmp/test/src/lib.rs"),
false,
None
));

    let child_store_c = child_store.clone();
    let tree_model = gtk4::TreeListModel::new(root_store.clone(), false, false, move |item| {
        let fi = item.downcast_ref::<FileTreeItem>()?;
        if fi.is_dir() && fi.path().as_deref() == Some(std::path::Path::new("/tmp/test/src")) {
            Some(child_store_c.clone().upcast::<gio::ListModel>())
        } else {
            None
        }
    });
    *section.imp().tree_model.borrow_mut() = Some(tree_model.clone());

    if let Some(row) = tree_model.item(0).and_downcast::<gtk4::TreeListRow>() {
        row.set_expanded(true);
    }

    assert_eq!(child_store.n_items(), 2);
    section.remove_from_model(std::path::Path::new("/tmp/test/src/main.rs"));
    assert_eq!(child_store.n_items(), 1);

    let remaining = child_store.item(0).and_downcast::<FileTreeItem>().unwrap();
    assert_eq!(
        remaining.path(),
        Some(PathBuf::from("/tmp/test/src/lib.rs"))
    );
}

// --- Callback wiring ---

#[test]
fn test_rename_callback_fires() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let called = Rc::new(Cell::new(false));
    let old_seen = Rc::new(Cell::new(PathBuf::new()));
    let new_seen = Rc::new(Cell::new(PathBuf::new()));

    let called_c = called.clone();
    let old_c = old_seen.clone();
    let new_c = new_seen.clone();
    section.connect_file_renamed(move |old, new| {
        called_c.set(true);
        old_c.set(old.to_path_buf());
        new_c.set(new.to_path_buf());
    });

    if let Some(ref cb) = *section.imp().rename_callback.borrow() {
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
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let called = Rc::new(Cell::new(false));
    let path_seen = Rc::new(Cell::new(PathBuf::new()));

    let called_c = called.clone();
    let path_c = path_seen.clone();
    section.connect_file_deleted(move |path| {
        called_c.set(true);
        path_c.set(path.to_path_buf());
    });

    if let Some(ref cb) = *section.imp().delete_callback.borrow() {
        cb(std::path::Path::new("/tmp/deleted.txt"));
    }

    assert!(called.get());
    assert_eq!(path_seen.take(), PathBuf::from("/tmp/deleted.txt"));
}

#[test]
fn test_create_callback_fires() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let called = Rc::new(Cell::new(false));
    let path_seen = Rc::new(Cell::new(PathBuf::new()));

    let called_c = called.clone();
    let path_c = path_seen.clone();
    section.connect_file_created(move |path| {
        called_c.set(true);
        path_c.set(path.to_path_buf());
    });

    if let Some(ref cb) = *section.imp().create_callback.borrow() {
        cb(std::path::Path::new("/tmp/new_file.txt"));
    }

    assert!(called.get());
    assert_eq!(path_seen.take(), PathBuf::from("/tmp/new_file.txt"));
}

// --- Add folder callback ---

#[test]
fn test_add_folder_callback_fires() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("test-ws"));

    let called = Rc::new(Cell::new(false));
    let id_seen = Rc::new(Cell::new(String::new()));

    let called_c = called.clone();
    let id_c = id_seen.clone();
    section.connect_add_folder_requested(move |ws_id| {
        called_c.set(true);
        id_c.set(ws_id.as_str().to_string());
    });

    section.notify_add_folder_requested();

    assert!(called.get());
    assert_eq!(id_seen.take(), "test-ws");
}

// --- Workspace header callbacks ---

#[test]
fn test_rename_workspace_callback_fires() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-123"));

    let called = Rc::new(Cell::new(false));
    let id_seen = Rc::new(Cell::new(String::new()));

    let called_c = called.clone();
    let id_c = id_seen.clone();
    section.connect_rename_workspace_requested(move |ws_id| {
        called_c.set(true);
        id_c.set(ws_id.as_str().to_string());
    });

    section.notify_rename_workspace_requested();

    assert!(called.get());
    assert_eq!(id_seen.take(), "ws-123");
}

#[test]
fn test_unlist_workspace_callback_fires() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-456"));

    let called = Rc::new(Cell::new(false));
    let id_seen = Rc::new(Cell::new(String::new()));

    let called_c = called.clone();
    let id_c = id_seen.clone();
    section.connect_unlist_workspace_requested(move |ws_id| {
        called_c.set(true);
        id_c.set(ws_id.as_str().to_string());
    });

    section.notify_unlist_workspace_requested();

    assert!(called.get());
    assert_eq!(id_seen.take(), "ws-456");
}

// --- FileTreeItem pending_rename ---

#[test]
fn test_file_tree_item_pending_rename_default_false() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/tmp/test.txt"), false, None);
    assert!(!item.is_pending_rename());
}

#[test]
fn test_file_tree_item_pending_rename_set_and_clear() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/tmp/test.txt"), false, None);
    item.set_pending_rename(true);
    assert!(item.is_pending_rename());
    item.set_pending_rename(false);
    assert!(!item.is_pending_rename());
}

// --- ListView property ---

#[test]
fn test_workspace_section_double_click_activate_property() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert!(!section.imp().file_tree_view.is_single_click_activate());
}

// --- add_root ---

#[test]
fn test_add_root_initializes_tree() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().unwrap();
    section.add_root(dir.path(), true);

    assert!(section.imp().root_store.borrow().is_some());
    let root_store = section.imp().root_store.borrow();
    let root_store = root_store.as_ref().unwrap();
    assert_eq!(root_store.n_items(), 1);
}

#[test]
fn test_add_root_deduplicates() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().unwrap();
    section.add_root(dir.path(), true);
    section.add_root(dir.path(), true); // duplicate

    let root_store = section.imp().root_store.borrow();
    let root_store = root_store.as_ref().unwrap();
    assert_eq!(root_store.n_items(), 1);
}

#[test]
fn test_add_root_appends_multiple() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir1 = tempfile::tempdir().unwrap();
    let dir2 = tempfile::tempdir().unwrap();
    section.add_root(dir1.path(), true);
    section.add_root(dir2.path(), true);

    let root_store = section.imp().root_store.borrow();
    let root_store = root_store.as_ref().unwrap();
    assert_eq!(root_store.n_items(), 2);
}

// --- Button state toggle ---

#[test]
fn test_button_shows_add_icon_when_no_roots() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert_eq!(
        section
            .imp()
            .add_folder_button
            .icon_name()
            .unwrap()
            .as_str(),
        "folder-new-symbolic"
    );
    assert_eq!(
        section
            .imp()
            .add_folder_button
            .tooltip_text()
            .unwrap()
            .as_str(),
        "Add Folder to Workspace"
    );
}

#[test]
fn test_button_switches_to_replace_icon_after_load_roots() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().unwrap();
    section.load_roots(&[(dir.path().to_path_buf(), true)]);

    assert_eq!(
        section
            .imp()
            .add_folder_button
            .icon_name()
            .unwrap()
            .as_str(),
        "folder-open-symbolic"
    );
    assert_eq!(
        section
            .imp()
            .add_folder_button
            .tooltip_text()
            .unwrap()
            .as_str(),
        "Replace Workspace Root"
    );
}

#[test]
fn test_button_switches_to_replace_icon_after_add_root() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().unwrap();
    section.add_root(dir.path(), true);

    assert_eq!(
        section
            .imp()
            .add_folder_button
            .icon_name()
            .unwrap()
            .as_str(),
        "folder-open-symbolic"
    );
}

#[test]
fn test_has_roots_false_initially() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert!(!section.has_roots());
}

#[test]
fn test_has_roots_true_after_load() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().unwrap();
    section.load_roots(&[(dir.path().to_path_buf(), true)]);
    assert!(section.has_roots());
}

#[test]
fn test_workspace_section_expand_roots() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().unwrap();
    // Must have at least one visible entry to not be detected as empty
    std::fs::write(dir.path().join("file.txt"), "content").unwrap();
    
    section.load_roots(&[(dir.path().to_path_buf(), true)]);

    let tree_model = section.imp().tree_model.borrow();
    let tree_model = tree_model.as_ref().unwrap();
    
    // Wait for the peeking logic to finish if it's async (but it's sync in _load_roots)
    let row = tree_model.item(0).and_downcast::<gtk4::TreeListRow>().unwrap();
    
    // Explicitly collapse to test expand_roots
    row.set_expanded(false);
    assert!(!row.is_expanded());

    section.expand_roots();
    assert!(row.is_expanded());
}

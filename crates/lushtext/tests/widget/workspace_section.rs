// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextWorkspaceSection widget.

use crate::common::{
    emit_key_pressed_on_focus, ensure_gtk_init, flush_events, present_window, wait_until,
};
use gio::prelude::MenuModelExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use lushtext_core::model::workspace::{WorkspaceEntry, WorkspaceId};
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::sidebar::workspace_section::LushtextWorkspaceSection;
use std::cell::Cell;
use std::cell::RefCell;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

fn present_section_window(section: &LushtextWorkspaceSection) -> gtk4::ApplicationWindow {
    let app = crate::common::test_application();
    let window = gtk4::ApplicationWindow::builder()
        .application(&app)
        .default_width(320)
        .default_height(420)
        .build();
    window.set_child(Some(section));
    present_window(&window);
    window
}

fn realized_root_row_widgets(
    section: &LushtextWorkspaceSection,
) -> (gtk4::ApplicationWindow, gtk4::Image, gtk4::Label) {
    let window = present_section_window(section);
    wait_until(Duration::from_secs(2), || {
        section.imp().file_tree_view.first_child().is_some()
    });

    let row_widget = section
        .imp()
        .file_tree_view
        .first_child()
        .expect("list view should realize the first row");
    let overlay = row_widget
        .first_child()
        .and_downcast::<gtk4::Overlay>()
        .expect("row child should be the factory overlay");
    let expander = overlay
        .child()
        .and_downcast::<gtk4::TreeExpander>()
        .expect("overlay child should be the tree expander");
    let content_box = expander
        .child()
        .and_downcast::<gtk4::Box>()
        .expect("tree expander child should be the content box");
    let icon = content_box
        .first_child()
        .and_downcast::<gtk4::Image>()
        .expect("content box should start with the row icon");
    let label = icon
        .next_sibling()
        .and_downcast::<gtk4::Label>()
        .expect("row icon should be followed by the row label");

    (window, icon, label)
}

struct PeekFixture {
    _dir: tempfile::TempDir,
    section: LushtextWorkspaceSection,
    text_a: PathBuf,
    text_b: PathBuf,
    binary: PathBuf,
    directory: PathBuf,
}

fn make_peek_fixture() -> PeekFixture {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let text_a = dir.path().join("alpha.rs");
    let text_b = dir.path().join("beta.rs");
    let binary = dir.path().join("binary.bin");
    let directory = dir.path().join("nested");

    std::fs::write(&text_a, "fn alpha() {\n    println!(\"alpha\");\n}\n").expect("expected operation to succeed");
    std::fs::write(&text_b, "fn beta() {\n    println!(\"beta\");\n}\n").expect("expected operation to succeed");
    std::fs::write(&binary, [0xff, 0xfe, 0xfd]).expect("expected operation to succeed");
    std::fs::create_dir_all(&directory).expect("expected operation to succeed");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("peek-ws"));
    section.load_roots(&[
        WorkspaceEntry::File {
            path: text_a.clone(),
        },
        WorkspaceEntry::File {
            path: text_b.clone(),
        },
        WorkspaceEntry::File {
            path: binary.clone(),
        },
        WorkspaceEntry::Directory {
            path: directory.clone(),
        },
    ]);

    PeekFixture {
        _dir: dir,
        section,
        text_a,
        text_b,
        binary,
        directory,
    }
}

fn select_path(section: &LushtextWorkspaceSection, target_path: &Path) {
    let selection = section
        .imp()
        .file_tree_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .expect("file tree should use a SingleSelection");
    let tree_model = section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .cloned()
        .expect("tree model should be loaded");

    for index in 0..tree_model.n_items() {
        if let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
            && let Some(item) = row.item().and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
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

    panic!("path {} was not found in the tree model", target_path.display());
}

fn tree_contains_path(section: &LushtextWorkspaceSection, target_path: &Path) -> bool {
    let Some(tree_model) = section.imp().tree_model.borrow().as_ref().cloned() else {
        return false;
    };

    for index in 0..tree_model.n_items() {
        if let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
            && let Some(item) = row.item().and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
        {
            return true;
        }
    }

    false
}

fn row_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::TreeListRow> {
    let tree_model = section.imp().tree_model.borrow().as_ref()?.clone();
    for index in 0..tree_model.n_items() {
        if let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
            && let Some(item) = row.item().and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
        {
            return Some(row);
        }
    }
    None
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

fn selected_path(section: &LushtextWorkspaceSection) -> Option<PathBuf> {
    section
        .imp()
        .file_tree_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .and_then(|selection| selection.selected_item())
        .and_then(|row| row.downcast::<gtk4::TreeListRow>().ok())
        .and_then(|row| row.item())
        .and_then(|item| item.downcast::<FileTreeItem>().ok())
        .and_then(|item| item.path())
}

fn peek_body_text(section: &LushtextWorkspaceSection) -> String {
    let binding = section.imp().peek_widgets.text_buffer.borrow();
    let buffer = binding.as_ref().expect("peek buffer should exist");
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, false).to_string()
}

fn peek_fallback_text(section: &LushtextWorkspaceSection) -> (String, String) {
    let title = section
        .imp()
        .peek_widgets
        .fallback_title_label
        .borrow()
        .as_ref()
        .expect("fallback title should exist")
        .label()
        .to_string();
    let body = section
        .imp()
        .peek_widgets
        .fallback_body_label
        .borrow()
        .as_ref()
        .expect("fallback body should exist")
        .label()
        .to_string();
    (title, body)
}

fn tree_view(section: &LushtextWorkspaceSection) -> &gtk4::ListView {
    &section.imp().file_tree_view
}

fn assert_tree_focus(window: &gtk4::ApplicationWindow, section: &LushtextWorkspaceSection) {
    let focus = gtk4::prelude::GtkWindowExt::focus(window).expect("focused widget");
    let target = tree_view(section).upcast_ref::<gtk4::Widget>();
    let mut current = Some(focus);
    while let Some(widget) = current {
        if widget.as_ptr() == target.as_ptr() {
            return;
        }
        current = widget.parent();
    }
    panic!("focus should remain inside the sidebar tree view");
}

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
    assert!(
        !section
            .imp()
            .inner_scrolled_window
            .propagates_natural_width()
    );
}

#[test]
fn test_workspace_section_header_button_carries_vertical_spacing() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert_eq!(section.imp().inner_scrolled_window.margin_top(), 0);
    assert_eq!(section.imp().refresh_button.valign(), gtk4::Align::Center);
    assert_eq!(section.imp().refresh_button.margin_top(), 6);
    assert_eq!(section.imp().refresh_button.margin_bottom(), 6);
    assert_eq!(section.imp().add_folder_button.valign(), gtk4::Align::Center);
    assert_eq!(section.imp().add_folder_button.margin_top(), 6);
    assert_eq!(section.imp().add_folder_button.margin_bottom(), 6);
}

#[test]
fn test_workspace_section_refresh_button_sits_left_of_replace_root_button() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let second_child = section
        .imp()
        .header_box
        .first_child()
        .and_then(|child| child.next_sibling())
        .and_downcast::<gtk4::Button>()
        .expect("second header child should be the refresh button");
    let third_child = second_child
        .next_sibling()
        .and_downcast::<gtk4::Button>()
        .expect("third header child should be the replace-root button");

    assert_eq!(second_child.as_ptr(), section.imp().refresh_button.as_ptr());
    assert_eq!(third_child.as_ptr(), section.imp().add_folder_button.as_ptr());
}

#[test]
fn test_single_directory_root_row_matches_builder_files_presentation() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    std::fs::write(dir.path().join("main.rs"), "fn main() {}\n").expect("expected operation to succeed");
    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let (_window, icon, label) = realized_root_row_widgets(&section);
    assert_eq!(icon.icon_name().as_deref(), Some("view-list-symbolic"));
    assert_eq!(label.label().as_str(), "Files");
}

#[test]
fn test_drilldown_root_row_keeps_actual_folder_presentation() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).expect("expected operation to succeed");
    std::fs::write(nested.join("lib.rs"), "pub fn demo() {}\n").expect("expected operation to succeed");
    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);
    section.focus_folder(&nested);

    let (_window, icon, label) = realized_root_row_widgets(&section);
    assert_eq!(icon.icon_name().as_deref(), Some("folder-symbolic"));
    assert_eq!(label.label().as_str(), "nested");
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
    let popover = popover.as_ref().expect("expected operation to succeed");
    assert!(!popover.has_arrow());
}

#[test]
fn test_workspace_section_context_menu_lists_local_history() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    let popover = section.imp().context_menu.borrow();
    let popover = popover.as_ref().expect("context menu should exist");
    let menu = popover.menu_model().expect("context menu should expose a menu model");

    let labels = menu_model_labels(&menu);
    assert!(
        labels.iter().any(|label| label == "Local History…"),
        "file context menu should advertise Local History"
    );
}

#[test]
fn test_workspace_section_local_history_action_emits_requested_path() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    let path = PathBuf::from("/tmp/local-history.txt");
    let requested = Rc::new(RefCell::new(None::<PathBuf>));

    {
        let requested = requested.clone();
        section.connect_local_history_requested(move |requested_path| {
            *requested.borrow_mut() = Some(requested_path.to_path_buf());
        });
    }

    *section.imp().context_path.borrow_mut() = Some(path.clone());
    section.imp().context_is_dir.set(false);
    section
        .activate_action("section.local-history", None)
        .expect("local-history widget action should exist");

    assert_eq!(*requested.borrow(), Some(path));
}

#[test]
fn test_workspace_section_peek_popover_uses_horizontal_offset() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    let binding = section.imp().peek_widgets.popover.borrow();
    let popover = binding.as_ref().expect("peek popover should exist");
    assert_eq!(popover.offset(), (15, 0));
}

// --- Model manipulation: remove_from_model ---

#[test]
fn test_remove_from_model_root_item() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let root_store = gio::ListStore::new::<FileTreeItem>();
    root_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/a.txt"),
        false,
        None,
    ));
    root_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/b.txt"),
        false,
        None,
    ));
    *section.imp().root_store.borrow_mut() = Some(root_store.clone());

    assert_eq!(root_store.n_items(), 2);
    section.remove_from_model(std::path::Path::new("/tmp/test/a.txt"));
    assert_eq!(root_store.n_items(), 1);

    let remaining = root_store.item(0).and_downcast::<FileTreeItem>().expect("expected operation to succeed");
    assert_eq!(remaining.path(), Some(PathBuf::from("/tmp/test/b.txt")));
}

#[test]
fn test_remove_from_model_nonexistent_is_noop() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let root_store = gio::ListStore::new::<FileTreeItem>();
    root_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/a.txt"),
        false,
        None,
    ));
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
        None,
    ));
    child_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/src/lib.rs"),
        false,
        None,
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

    let remaining = child_store.item(0).and_downcast::<FileTreeItem>().expect("expected operation to succeed");
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

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.add_root(dir.path(), true);

    assert!(section.imp().root_store.borrow().is_some());
    let root_store = section.imp().root_store.borrow();
    let root_store = root_store.as_ref().expect("expected operation to succeed");
    assert_eq!(root_store.n_items(), 1);
}

#[test]
fn test_add_root_deduplicates() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.add_root(dir.path(), true);
    section.add_root(dir.path(), true); // duplicate

    let root_store = section.imp().root_store.borrow();
    let root_store = root_store.as_ref().expect("expected operation to succeed");
    assert_eq!(root_store.n_items(), 1);
}

#[test]
fn test_add_root_appends_multiple() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir1 = tempfile::tempdir().expect("expected operation to succeed");
    let dir2 = tempfile::tempdir().expect("expected operation to succeed");
    section.add_root(dir1.path(), true);
    section.add_root(dir2.path(), true);

    let root_store = section.imp().root_store.borrow();
    let root_store = root_store.as_ref().expect("expected operation to succeed");
    assert_eq!(root_store.n_items(), 2);
}

// --- Button state toggle ---

#[test]
fn test_button_shows_add_icon_when_no_roots() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert!(
        !section.imp().refresh_button.is_sensitive(),
        "refresh should stay disabled until the section has roots"
    );
    assert_eq!(
        section
            .imp()
            .add_folder_button
            .icon_name()
            .expect("expected operation to succeed")
            .as_str(),
        "folder-new-symbolic"
    );
    assert_eq!(
        section
            .imp()
            .add_folder_button
            .tooltip_text()
            .expect("expected operation to succeed")
            .as_str(),
        "Add Folder to Workspace"
    );
}

#[test]
fn test_button_switches_to_replace_icon_after_load_roots() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    assert!(
        section.imp().refresh_button.is_sensitive(),
        "refresh should become available once the section has roots"
    );
    assert_eq!(
        section
            .imp()
            .add_folder_button
            .icon_name()
            .expect("expected operation to succeed")
            .as_str(),
        "folder-open-symbolic"
    );
    assert_eq!(
        section
            .imp()
            .add_folder_button
            .tooltip_text()
            .expect("expected operation to succeed")
            .as_str(),
        "Replace Workspace Root"
    );
}

#[test]
fn test_button_switches_to_replace_icon_after_add_root() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.add_root(dir.path(), true);

    assert!(section.imp().refresh_button.is_sensitive());
    assert_eq!(
        section
            .imp()
            .add_folder_button
            .icon_name()
            .expect("expected operation to succeed")
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

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);
    assert!(section.has_roots());
}

#[test]
fn test_manual_refresh_keeps_selection_and_expansion() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).expect("expected operation to succeed");
    let existing = nested.join("alpha.txt");
    std::fs::write(&existing, "alpha").expect("expected operation to succeed");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("refresh-ws"));
    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    section.expand_roots();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should exist")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &existing));
    select_path(&section, &existing);

    let created = nested.join("beta.txt");
    std::fs::write(&created, "beta").expect("expected operation to succeed");
    section.imp().refresh_button.emit_clicked();

    wait_until(Duration::from_secs(5), || {
        tree_contains_path(&section, &created) && selected_path(&section) == Some(existing.clone())
    });
    assert_eq!(selected_path(&section), Some(existing.clone()));
    assert!(
        row_for_path(&section, &nested)
            .expect("nested directory should still exist")
            .is_expanded(),
        "expanded directories should stay expanded after refresh"
    );
}

#[test]
fn test_refresh_updates_tree_after_external_rename() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).expect("expected operation to succeed");
    let original = nested.join("before.txt");
    std::fs::write(&original, "before").expect("expected operation to succeed");
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("refresh-flow-ws"));
    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    section.expand_roots();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should exist")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &original));

    let renamed = nested.join("renamed.txt");
    std::fs::rename(&original, &renamed).expect("expected operation to succeed");
    section.imp().refresh_button.emit_clicked();
    wait_until(Duration::from_secs(5), || {
        !tree_contains_path(&section, &original) && tree_contains_path(&section, &renamed)
    });
}

#[test]
fn test_refresh_updates_tree_after_external_delete() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).expect("expected operation to succeed");
    let deleted = nested.join("delete-me.txt");
    std::fs::write(&deleted, "delete").expect("expected operation to succeed");
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("refresh-delete-ws"));
    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    section.expand_roots();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should exist")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &deleted));

    std::fs::remove_file(&deleted).expect("expected operation to succeed");
    section.imp().refresh_button.emit_clicked();
    wait_until(Duration::from_secs(5), || !tree_contains_path(&section, &deleted));
}

#[test]
fn test_workspace_section_toggle_roots() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    // Must have at least one visible entry to not be detected as empty
    std::fs::write(dir.path().join("file.txt"), "content").expect("expected operation to succeed");

    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let tree_model = section.imp().tree_model.borrow();
    let tree_model = tree_model.as_ref().expect("expected operation to succeed");

    let row = tree_model
        .item(0)
        .and_downcast::<gtk4::TreeListRow>()
        .expect("expected operation to succeed");

    // Initial state is collapsed (new default behavior)
    assert!(!row.is_expanded());

    // Toggle should expand
    section.toggle_roots();
    assert!(row.is_expanded());

    // Toggle should collapse
    section.toggle_roots();
    assert!(!row.is_expanded());
}

#[test]
fn test_manual_refresh_keeps_collapsed_root_collapsed() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    std::fs::write(dir.path().join("file.txt"), "content").expect("expected operation to succeed");

    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    {
        let tree_model = section.imp().tree_model.borrow();
        let tree_model = tree_model.as_ref().expect("expected operation to succeed");
        let row = tree_model
            .item(0)
            .and_downcast::<gtk4::TreeListRow>()
            .expect("expected operation to succeed");

        row.set_expanded(true);
        row.set_expanded(false);
        assert!(!row.is_expanded(), "root should start collapsed before refresh");
    }

    section.imp().refresh_button.emit_clicked();
    wait_until(Duration::from_secs(2), || {
        section
            .imp()
            .tree_model
            .borrow()
            .as_ref()
            .and_then(|tree_model| tree_model.item(0))
            .and_downcast::<gtk4::TreeListRow>()
            .is_some_and(|row| !row.is_expanded())
    });

    let tree_model = section.imp().tree_model.borrow();
    let tree_model = tree_model.as_ref().expect("expected operation to succeed");
    let row = tree_model
        .item(0)
        .and_downcast::<gtk4::TreeListRow>()
        .expect("expected operation to succeed");
    assert!(
        !row.is_expanded(),
        "manual refresh should not re-expand a root the user collapsed"
    );
}

#[test]
fn test_manual_refresh_keeps_root_models_mounted() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).expect("expected operation to succeed");
    let existing = nested.join("alpha.txt");
    std::fs::write(&existing, "alpha").expect("expected operation to succeed");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("manual-model-stability"));
    section.load_roots(&[WorkspaceEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    section.expand_roots();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should exist")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &existing));

    let root_store_ptr = section
        .imp()
        .root_store
        .borrow()
        .as_ref()
        .expect("root store should exist")
        .as_ptr();
    let tree_model_ptr = section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .expect("tree model should exist")
        .as_ptr();

    let created = nested.join("beta.txt");
    std::fs::write(&created, "beta").expect("expected operation to succeed");
    section.imp().refresh_button.emit_clicked();

    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &created));

    assert_eq!(
        section
            .imp()
            .root_store
            .borrow()
            .as_ref()
            .expect("root store should still exist")
            .as_ptr(),
        root_store_ptr,
        "manual refresh should keep the existing root store mounted",
    );
    assert_eq!(
        section
            .imp()
            .tree_model
            .borrow()
            .as_ref()
            .expect("tree model should still exist")
            .as_ptr(),
        tree_model_ptr,
        "manual refresh should keep the existing tree model mounted",
    );
}

#[test]
fn test_file_peek_space_opens_for_selected_file_and_keeps_sidebar_focus() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    select_path(&fixture.section, &fixture.text_a);
    tree_view(&fixture.section).grab_focus();
    flush_events();

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || {
        fixture.section.peek_visible()
            && fixture.section.peeked_path().as_deref() == Some(fixture.text_a.as_path())
            && peek_body_text(&fixture.section).contains("alpha")
    });

    assert_tree_focus(&window, &fixture.section);
}

#[test]
fn test_file_peek_selection_change_refreshes_preview_in_place() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    select_path(&fixture.section, &fixture.text_a);
    tree_view(&fixture.section).grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || fixture.section.peek_visible());

    select_path(&fixture.section, &fixture.text_b);
    wait_until(Duration::from_secs(2), || {
        fixture.section.peeked_path().as_deref() == Some(fixture.text_b.as_path())
            && peek_body_text(&fixture.section).contains("beta")
    });

    assert_tree_focus(&window, &fixture.section);
}

#[test]
fn test_file_peek_escape_dismisses_and_restores_sidebar_focus() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    select_path(&fixture.section, &fixture.text_a);
    tree_view(&fixture.section).grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || fixture.section.peek_visible());

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::Escape);
    wait_until(Duration::from_secs(2), || !fixture.section.peek_visible());

    assert_tree_focus(&window, &fixture.section);
}

#[test]
fn test_file_peek_repeated_space_dismisses_current_preview() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    select_path(&fixture.section, &fixture.text_a);
    tree_view(&fixture.section).grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || fixture.section.peek_visible());

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || !fixture.section.peek_visible());

    assert_tree_focus(&window, &fixture.section);
}

#[test]
fn test_file_peek_click_away_close_restores_sidebar_focus() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    select_path(&fixture.section, &fixture.text_a);
    tree_view(&fixture.section).grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || fixture.section.peek_visible());

    fixture
        .section
        .imp()
        .peek_widgets
        .popover
        .borrow()
        .as_ref()
        .expect("peek popover should exist")
        .popdown();
    wait_until(Duration::from_secs(2), || !fixture.section.peek_visible());

    assert_tree_focus(&window, &fixture.section);
}

#[test]
fn test_file_peek_enter_promotes_selected_file() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    let promoted_path = Rc::new(RefCell::new(None::<PathBuf>));
    let promoted_path_clone = promoted_path.clone();
    fixture.section.connect_peek_promoted(move |path| {
        promoted_path_clone.replace(Some(path.to_path_buf()));
    });

    select_path(&fixture.section, &fixture.text_a);
    tree_view(&fixture.section).grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || {
        fixture.section.peek_visible()
            && fixture
                .section
                .imp()
                .peek_widgets
                .open_button
                .borrow()
                .as_ref()
                .is_some_and(gtk4::Button::is_sensitive)
    });

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::Return);
    wait_until(Duration::from_secs(2), || !fixture.section.peek_visible());

    assert_eq!(promoted_path.take(), Some(fixture.text_a));
}

#[test]
fn test_file_peek_open_button_promotes_selected_file() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    let promoted_path = Rc::new(RefCell::new(None::<PathBuf>));
    let promoted_path_clone = promoted_path.clone();
    fixture.section.connect_peek_promoted(move |path| {
        promoted_path_clone.replace(Some(path.to_path_buf()));
    });

    select_path(&fixture.section, &fixture.text_b);
    tree_view(&fixture.section).grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || {
        fixture.section.peek_visible()
            && fixture
                .section
                .imp()
                .peek_widgets
                .open_button
                .borrow()
                .as_ref()
                .is_some_and(gtk4::Button::is_sensitive)
    });

    fixture
        .section
        .imp()
        .peek_widgets
        .open_button
        .borrow()
        .as_ref()
        .expect("peek open button should exist")
        .emit_clicked();
    wait_until(Duration::from_secs(2), || !fixture.section.peek_visible());

    assert_eq!(promoted_path.take(), Some(fixture.text_b));
}

#[test]
fn test_file_peek_binary_fallback_disables_open() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    select_path(&fixture.section, &fixture.binary);
    tree_view(&fixture.section).grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || {
        fixture.section.peek_visible()
            && fixture.section.peeked_path().as_deref() == Some(fixture.binary.as_path())
            && !peek_fallback_text(&fixture.section).0.is_empty()
    });

    let (title, body) = peek_fallback_text(&fixture.section);
    assert!(title.contains("Inline preview unavailable"));
    assert!(body.contains("UTF-8 text"));
    assert!(
        !fixture
            .section
            .imp()
            .peek_widgets
            .open_button
            .borrow()
            .as_ref()
            .expect("peek open button should exist")
            .is_sensitive()
    );
}

#[test]
fn test_file_peek_selecting_directory_dismisses_preview() {
    let fixture = make_peek_fixture();
    let window = present_section_window(&fixture.section);

    select_path(&fixture.section, &fixture.text_a);
    tree_view(&fixture.section).grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || fixture.section.peek_visible());

    select_path(&fixture.section, &fixture.directory);
    wait_until(Duration::from_secs(2), || !fixture.section.peek_visible());

    assert_eq!(selected_path(&fixture.section).as_deref(), Some(fixture.directory.as_path()));
    assert_tree_focus(&window, &fixture.section);
}

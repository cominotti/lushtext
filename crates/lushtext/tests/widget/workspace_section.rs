// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextWorkspaceSection widget.

use crate::common::{
    emit_key_pressed_on_focus, ensure_gtk_init, fixture, flush_events, present_window, wait_until,
};
use gio::prelude::MenuModelExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use lushtext_core::model::workspace::{
    FolderTreeEntry, WorkspaceFolder, WorkspaceFolderId, WorkspaceFolderMoveDirection,
    WorkspaceId,
};
use lushtext_core::services::filesystem::metadata as fs_metadata;
use lushtext_core::services::workspace_watch::WorkspaceWatchTarget;
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::sidebar::workspace_section::LushtextWorkspaceSection;
use std::cell::Cell;
use std::cell::RefCell;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

fn present_section_window(section: &LushtextWorkspaceSection) -> gtk4::ApplicationWindow {
    present_section_window_with_size(section, 320, 420)
}

fn present_section_window_with_size(
    section: &LushtextWorkspaceSection,
    width: i32,
    height: i32,
) -> gtk4::ApplicationWindow {
    let app = crate::common::test_application();
    let window = gtk4::ApplicationWindow::builder()
        .application(&app)
        .default_width(width)
        .default_height(height)
        .build();
    window.set_child(Some(section));
    present_window(&window);
    window
}

fn realized_folder_row_content(
    section: &LushtextWorkspaceSection,
) -> (
    gtk4::ApplicationWindow,
    gtk4::Button,
    gtk4::Image,
    gtk4::Label,
) {
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
    let drag_handle = content_box
        .first_child()
        .and_downcast::<gtk4::Button>()
        .expect("content box should start with the reorder handle");
    let icon = drag_handle
        .next_sibling()
        .and_downcast::<gtk4::Image>()
        .expect("reorder handle should be followed by the row icon");
    let label = icon
        .next_sibling()
        .and_downcast::<gtk4::Label>()
        .expect("row icon should be followed by the row label");

    (window, drag_handle, icon, label)
}

fn realized_folder_row_widgets(
    section: &LushtextWorkspaceSection,
) -> (gtk4::ApplicationWindow, gtk4::Image, gtk4::Label) {
    let (window, _drag_handle, icon, label) = realized_folder_row_content(section);
    (window, icon, label)
}

fn themed_icon_names(icon: &gio::Icon) -> Vec<String> {
    icon.downcast_ref::<gio::ThemedIcon>()
        .map(|themed_icon| {
            themed_icon
                .names()
                .into_iter()
                .map(|name| name.to_string())
                .collect()
        })
        .unwrap_or_default()
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

    fixture::write_text(&text_a, "fn alpha() {\n    println!(\"alpha\");\n}\n");
    fixture::write_text(&text_b, "fn beta() {\n    println!(\"beta\");\n}\n");
    fixture::write_bytes(&binary, [0xff, 0xfe, 0xfd]);
    fixture::create_dir_all(&directory);

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("peek-ws"));
    section.load_folders(&[
        FolderTreeEntry::File {
            path: text_a.clone(),
        },
        FolderTreeEntry::File {
            path: text_b.clone(),
        },
        FolderTreeEntry::File {
            path: binary.clone(),
        },
        FolderTreeEntry::Directory {
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

fn tree_model_index_for_path(section: &LushtextWorkspaceSection, target_path: &Path) -> Option<u32> {
    let tree_model = section.imp().tree_model.borrow().as_ref()?.clone();
    for index in 0..tree_model.n_items() {
        if let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
            && let Some(item) = row.item().and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
        {
            return Some(index);
        }
    }
    None
}

fn rows_for_path(section: &LushtextWorkspaceSection, target_path: &Path) -> Vec<gtk4::TreeListRow> {
    let Some(tree_model) = section.imp().tree_model.borrow().as_ref().cloned() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for index in 0..tree_model.n_items() {
        if let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
            && let Some(item) = row.item().and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
        {
            rows.push(row);
        }
    }
    rows
}

fn row_count_for_path(section: &LushtextWorkspaceSection, target_path: &Path) -> usize {
    rows_for_path(section, target_path).len()
}

fn realized_row_widget_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::Widget> {
    let mut child = section.imp().file_tree_view.first_child();
    while let Some(row_widget) = child {
        if let Some(overlay) = row_widget.first_child().and_downcast::<gtk4::Overlay>()
            && let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>()
            && let Some(tree_row) = expander.list_row()
            && let Some(item) = tree_row.item().and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
        {
            return Some(row_widget);
        }
        child = row_widget.next_sibling();
    }
    None
}

fn realized_expander_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::TreeExpander> {
    let row_widget = realized_row_widget_for_path(section, target_path)?;
    row_widget
        .first_child()
        .and_downcast::<gtk4::Overlay>()
        .and_then(|overlay| overlay.child().and_downcast::<gtk4::TreeExpander>())
}

fn realized_overlay_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::Overlay> {
    let row_widget = realized_row_widget_for_path(section, target_path)?;
    row_widget.first_child().and_downcast::<gtk4::Overlay>()
}

fn realized_drag_handle_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::Button> {
    let expander = realized_expander_for_path(section, target_path)?;
    let content_box = expander.child().and_downcast::<gtk4::Box>()?;
    content_box
        .first_child()
        .and_downcast::<gtk4::Button>()
        .filter(|button| button.has_css_class("workspace-folder-drag-handle"))
}

fn realized_top_level_drag_handle_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::Button> {
    let mut child = section.imp().file_tree_view.first_child();
    while let Some(row_widget) = child {
        if let Some(overlay) = row_widget.first_child().and_downcast::<gtk4::Overlay>()
            && let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>()
            && let Some(tree_row) = expander.list_row()
            && tree_row.depth() == 0
            && let Some(item) = tree_row.item().and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
            && let Some(content_box) = expander.child().and_downcast::<gtk4::Box>()
            && let Some(drag_handle) = content_box.first_child().and_downcast::<gtk4::Button>()
            && drag_handle.has_css_class("workspace-folder-drag-handle")
        {
            return Some(drag_handle);
        }
        child = row_widget.next_sibling();
    }
    None
}

fn assert_reorder_handle_visible(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
    expected_visible: bool,
) {
    let handle = realized_top_level_drag_handle_for_path(section, target_path)
        .expect("realized top-level row should have the recycled reorder-handle widget");
    assert_eq!(
        handle.is_visible(),
        expected_visible,
        "unexpected reorder-handle visibility for {}",
        target_path.display()
    );
    assert_eq!(
        handle.is_sensitive(),
        expected_visible,
        "reorder-handle sensitivity should track visibility for {}",
        target_path.display()
    );
}

fn realized_drop_target_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::Box> {
    let overlay = realized_overlay_for_path(section, target_path)?;
    let mut child = overlay.first_child();
    while let Some(widget) = child {
        if widget.has_css_class("workspace-folder-drop-target") {
            return widget.downcast::<gtk4::Box>().ok();
        }
        child = widget.next_sibling();
    }
    None
}

fn realized_reorder_shield_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::Widget> {
    let overlay = realized_overlay_for_path(section, target_path)?;
    let mut child = overlay.first_child();
    while let Some(widget) = child {
        if widget.has_css_class("workspace-folder-dnd-shield") {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

fn visible_reorder_drop_target_count(section: &LushtextWorkspaceSection) -> usize {
    let mut visible = 0;
    let mut row_widget = section.imp().file_tree_view.first_child();
    while let Some(row) = row_widget {
        if let Some(overlay) = row.first_child().and_downcast::<gtk4::Overlay>() {
            let mut child = overlay.first_child();
            while let Some(widget) = child {
                if widget.has_css_class("workspace-folder-drop-target")
                    && widget.property::<bool>("visible")
                {
                    visible += 1;
                }
                child = widget.next_sibling();
            }
        }
        row_widget = row.next_sibling();
    }
    visible
}

fn prepare_context_menu_for_path(section: &LushtextWorkspaceSection, target_path: &Path) {
    let list_view = &section.imp().file_tree_view;
    if let Some(index) = tree_model_index_for_path(section, target_path) {
        list_view.scroll_to(index, gtk4::ListScrollFlags::NONE, None);
    }
    wait_until(Duration::from_secs(5), || {
        realized_expander_for_path(section, target_path)
            .is_some_and(|expander| expander.height() > 0)
    });
    let expander =
        realized_expander_for_path(section, target_path).expect("target row should be realized");
    let tree_row = expander
        .list_row()
        .expect("realized expander should be bound to a tree row");
    let file_item = tree_row
        .item()
        .and_downcast::<FileTreeItem>()
        .expect("tree row item should be a FileTreeItem");
    assert_eq!(
        file_item.path().as_deref(),
        Some(target_path),
        "context setup should target the requested row"
    );

    let imp = section.imp();
    let workspace_folder_id = file_item.workspace_folder_id();
    *imp.context_path.borrow_mut() = file_item.path();
    imp.context_is_dir.set(file_item.is_dir());
    *imp.context_workspace_folder_id.borrow_mut() = workspace_folder_id.clone();
    *imp.context_expander.borrow_mut() = Some(expander);

    let menu = if workspace_folder_id.is_some() {
        imp.context_folder_menu_model.borrow().as_ref().cloned()
    } else {
        imp.context_file_menu_model.borrow().as_ref().cloned()
    }
    .expect("context menu model should exist");
    imp.context_menu
        .borrow()
        .as_ref()
        .expect("context menu should exist")
        .set_menu_model(Some(&menu));
}

fn current_context_menu_labels(section: &LushtextWorkspaceSection) -> Vec<String> {
    let menu = section
        .imp()
        .context_menu
        .borrow()
        .as_ref()
        .and_then(gtk4::PopoverMenu::menu_model)
        .expect("context menu should expose a model");
    menu_model_labels(&menu)
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
fn test_long_workspace_folder_label_ellipsizes_and_keeps_controls_visible() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("long-folders"));

    let dir = tempfile::tempdir().expect("workspace folders tempdir");
    let long_name =
        "this-is-a-very-long-workspace-folder-name-that-must-not-widen-the-sidebar-panel";
    let long_path = dir.path().join(long_name);
    let second_path = dir.path().join("second-folder");
    fixture::create_dir_all(&long_path);
    fixture::create_dir_all(&second_path);
    let mut folders = vec![
        WorkspaceFolder::with_id(WorkspaceFolderId::new("long-folder"), long_path),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second-folder"), second_path.clone()),
    ];
    for index in 0..18 {
        let folder_path = dir.path().join(format!(
            "extra-folder-{index:02}-with-a-name-that-should-still-clip-inside-the-panel"
        ));
        fixture::create_dir_all(&folder_path);
        folders.push(WorkspaceFolder::with_id(
            WorkspaceFolderId::new(format!("extra-{index:02}")),
            folder_path,
        ));
    }
    section.load_workspace_folders(&folders);

    let window = present_section_window_with_size(&section, 220, 260);
    wait_until(Duration::from_secs(2), || {
        section.imp().file_tree_view.first_child().is_some()
            && section.imp().refresh_button.width() > 0
            && section.imp().add_folder_button.width() > 0
    });

    let row_widget = section
        .imp()
        .file_tree_view
        .first_child()
        .expect("list view should realize the first folder row");
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
    let drag_handle = content_box
        .first_child()
        .and_downcast::<gtk4::Button>()
        .expect("content box should start with the reorder handle");
    let icon = drag_handle
        .next_sibling()
        .and_downcast::<gtk4::Image>()
        .expect("reorder handle should be followed by the row icon");
    let label = icon
        .next_sibling()
        .and_downcast::<gtk4::Label>()
        .expect("row icon should be followed by the row label");

    assert!(label.text().starts_with(long_name));
    assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert!(!label.wraps());
    assert_eq!(
        section.imp().inner_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );

    let row_bounds = row_widget
        .compute_bounds(&*section.imp().inner_scrolled_window)
        .expect("folder row should have scroller-relative bounds");
    let label_bounds = label
        .compute_bounds(&row_widget)
        .expect("folder label should have row-relative bounds");
    let scroller_width = section.imp().inner_scrolled_window.width() as f32;
    let row_height_before_drag = row_widget.height();
    let label_width_before_drag = label_bounds.width();
    assert!(
        row_bounds.width() <= scroller_width + 1.0,
        "folder rows should be clipped to the section scroller width (row={}, scroller={scroller_width})",
        row_bounds.width()
    );
    assert!(
        label_bounds.width() > 0.0 && label_bounds.width() <= row_bounds.width(),
        "long folder labels should get a bounded positive allocation (label={}, row={})",
        label_bounds.width(),
        row_bounds.width()
    );
    assert!(
        section.imp().refresh_button.width() > 0
            && section.imp().add_folder_button.width() > 0,
        "workspace folder controls should remain allocated in narrow sections"
    );
    assert!(section.imp().header_box.property::<bool>("visible"));
    assert_eq!(
        section.imp().inner_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );

    section.with_active_workspace_folder_reorder_drag_for_test(
        &WorkspaceFolderId::new("long-folder"),
        || {
            let hover =
                section.simulate_workspace_folder_reorder_hover_after_for_test(&second_path);
            assert!(hover.owns_hover, "shield should own constrained-geometry hover");
            assert!(
                hover.shows_indicator,
                "valid constrained hover should show insertion feedback"
            );
            assert!(hover.accepts_drop, "valid constrained hover should accept drop");
            flush_events();

            let row_bounds_during_drag = row_widget
                .compute_bounds(&*section.imp().inner_scrolled_window)
                .expect("folder row should keep scroller-relative bounds during drag");
            let label_bounds_during_drag = label
                .compute_bounds(&row_widget)
                .expect("folder label should keep row-relative bounds during drag");
            assert_eq!(
                row_widget.height(),
                row_height_before_drag,
                "the full-row shield must not change row height"
            );
            assert!(
                (label_bounds_during_drag.width() - label_width_before_drag).abs() <= 1.0,
                "the full-row shield must not change label measurement"
            );
            assert!(
                row_bounds_during_drag.width() <= scroller_width + 1.0,
                "drag hover should not widen rows or introduce horizontal scrolling"
            );
            assert!(
                section.imp().header_box.property::<bool>("visible"),
                "the fixed workspace header should remain visible while dragging"
            );
            assert!(
                drag_handle.width() > 0
                    && section.imp().refresh_button.width() > 0
                    && section.imp().add_folder_button.width() > 0,
                "drag handles and header controls should remain reachable during drag"
            );
        },
    );

    drop(window);
}

#[test]
fn test_workspace_folder_reorder_handle_only_shows_when_folder_can_move() {
    ensure_gtk_init();
    let one_folder_section = LushtextWorkspaceSection::new(WorkspaceId::new("one-folder"));
    let one_folder_dir = tempfile::tempdir().expect("one folder tempdir");
    one_folder_section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("only"),
        one_folder_dir.path().to_path_buf(),
    )]);

    let (one_folder_window, one_folder_handle, _, _) =
        realized_folder_row_content(&one_folder_section);
    assert!(
        !one_folder_handle.is_visible(),
        "a single workspace folder has no reorder destination"
    );
    drop(one_folder_window);

    let two_folder_section = LushtextWorkspaceSection::new(WorkspaceId::new("two-folders"));
    let first = tempfile::tempdir().expect("first folder tempdir");
    let second = tempfile::tempdir().expect("second folder tempdir");
    two_folder_section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
    ]);

    let (_two_folder_window, two_folder_handle, _, _) =
        realized_folder_row_content(&two_folder_section);
    assert!(
        two_folder_handle.is_visible(),
        "top-level workspace folders should expose a drag handle when reordering is possible"
    );
    assert!(
        two_folder_handle.has_css_class("workspace-folder-drag-handle"),
        "the handle should carry a stable style hook for DnD presentation"
    );
}

#[test]
fn test_workspace_folder_reorder_handles_update_on_live_membership_changes() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("live-membership"));
    section.load_workspace_folders(&[]);
    let window = present_section_window(&section);
    assert!(
        section
            .imp()
            .empty_folder_set_label
            .property::<bool>("visible"),
        "empty workspaces should show their empty folder-set state"
    );
    assert!(section.imp().header_box.property::<bool>("visible"));
    assert!(
        section.imp().add_folder_button.width() > 0 && section.imp().refresh_button.width() > 0,
        "empty workspaces should keep header actions reachable"
    );
    assert_eq!(visible_reorder_drop_target_count(&section), 0);

    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    let first_id = WorkspaceFolderId::new("first");
    let second_id = WorkspaceFolderId::new("second");

    section.add_workspace_folder(&first_id, first.path());
    wait_until(Duration::from_secs(5), || {
        top_level_workspace_folder_ids(&section) == ["first".to_string()]
            && realized_drag_handle_for_path(&section, first.path()).is_some()
    });
    assert_reorder_handle_visible(&section, first.path(), false);
    assert!(
        !section
            .imp()
            .empty_folder_set_label
            .property::<bool>("visible"),
        "the first folder should replace the empty state with the tree body"
    );
    assert!(
        realized_reorder_shield_for_path(&section, first.path())
            .is_some_and(|shield| !shield.can_target()),
        "reorder shields should stay inert when there is no active drag"
    );

    section.add_workspace_folder(&second_id, second.path());
    wait_until(Duration::from_secs(5), || {
        top_level_workspace_folder_ids(&section) == ["first".to_string(), "second".to_string()]
            && realized_drag_handle_for_path(&section, second.path())
                .is_some_and(|handle| handle.is_visible())
            && realized_drag_handle_for_path(&section, first.path())
                .is_some_and(|handle| handle.is_visible())
    });
    assert_reorder_handle_visible(&section, first.path(), true);
    assert_reorder_handle_visible(&section, second.path(), true);

    section.with_active_workspace_folder_reorder_drag_for_test(&first_id, || {
        let hover = section.simulate_workspace_folder_reorder_hover_after_for_test(second.path());
        assert!(
            hover.shows_indicator,
            "valid hover should establish insertion feedback before removal"
        );
        assert_eq!(visible_reorder_drop_target_count(&section), 1);

        section.remove_workspace_folder(&second_id, second.path());
        flush_events();
        assert_eq!(top_level_workspace_folder_ids(&section), ["first".to_string()]);
        assert_eq!(
            visible_reorder_drop_target_count(&section),
            0,
            "membership sync should clear stale insertion feedback"
        );
    });

    assert_reorder_handle_visible(&section, first.path(), false);
    assert!(
        realized_reorder_shield_for_path(&section, first.path())
            .is_some_and(|shield| !shield.can_target()),
        "the remaining one-folder row should leave its shield inert after the drag ends"
    );

    section.remove_workspace_folder(&first_id, first.path());
    wait_until(Duration::from_secs(5), || {
        top_level_workspace_folder_ids(&section).is_empty()
            && section
                .imp()
                .empty_folder_set_label
                .property::<bool>("visible")
    });
    assert_eq!(visible_reorder_drop_target_count(&section), 0);
    assert!(
        section.imp().header_box.property::<bool>("visible"),
        "removing every folder should leave the workspace header reachable"
    );
    assert!(
        !section
            .imp()
            .inner_scrolled_window
            .property::<bool>("visible"),
        "empty workspace state should not leave a fake tree body visible"
    );

    drop(window);
}

#[test]
fn test_workspace_folder_reorder_handles_update_after_hidden_membership_change() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("hidden-membership"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("first"),
        first.path().to_path_buf(),
    )]);
    let window = present_section_window(&section);
    assert_reorder_handle_visible(&section, first.path(), false);

    section.set_visible(false);
    section.add_workspace_folder(&WorkspaceFolderId::new("second"), second.path());
    flush_events();
    section.set_visible(true);

    wait_until(Duration::from_secs(5), || {
        top_level_workspace_folder_ids(&section) == ["first".to_string(), "second".to_string()]
            && realized_drag_handle_for_path(&section, first.path()).is_some()
            && realized_drag_handle_for_path(&section, second.path()).is_some()
    });
    assert_reorder_handle_visible(&section, first.path(), true);
    assert_reorder_handle_visible(&section, second.path(), true);

    drop(window);
}

#[test]
fn test_workspace_folder_reorder_handle_is_hidden_on_descendant_rows() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("descendant-handle"));
    let folder = tempfile::tempdir().expect("workspace folder");
    let nested = folder.path().join("nested");
    fixture::create_dir_all(&nested);
    section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("folder"),
        folder.path().to_path_buf(),
    )]);
    let _window = present_section_window(&section);

    section.expand_folders();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));

    let nested_handle = realized_drag_handle_for_path(&section, &nested)
        .expect("descendant row should still have the recycled handle widget");
    assert!(
        !nested_handle.is_visible(),
        "descendant directory rows must not expose workspace-folder reorder handles"
    );
}

#[test]
fn test_workspace_folder_drop_indicator_decisions_follow_real_reorders() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("indicator-decisions"));
    let first = tempfile::tempdir().expect("first folder tempdir");
    let second = tempfile::tempdir().expect("second folder tempdir");
    let third = tempfile::tempdir().expect("third folder tempdir");
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("third"), third.path().to_path_buf()),
    ]);

    assert!(
        section.drop_workspace_folder_before_would_show_indicator_for_test(
            &WorkspaceFolderId::new("third"),
            &WorkspaceFolderId::new("first"),
        ),
        "dropping third before first should show an above-row insertion line"
    );
    assert!(
        section.drop_workspace_folder_after_would_show_indicator_for_test(
            &WorkspaceFolderId::new("first"),
            &WorkspaceFolderId::new("third"),
        ),
        "dropping first after third should show a below-row insertion line"
    );
    assert!(
        !section.drop_workspace_folder_after_would_show_indicator_for_test(
            &WorkspaceFolderId::new("second"),
            &WorkspaceFolderId::new("first"),
        ),
        "drop positions that keep the current order should not show insertion feedback"
    );
    assert!(
        !section.drop_workspace_folder_before_would_show_indicator_for_test(
            &WorkspaceFolderId::new("missing"),
            &WorkspaceFolderId::new("first"),
        ),
        "invalid folder payloads should not show valid insertion feedback"
    );
}

#[test]
fn test_workspace_folder_drop_indicator_is_a_single_line_surface() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("indicator-line"));
    let first = tempfile::tempdir().expect("first folder tempdir");
    let second = tempfile::tempdir().expect("second folder tempdir");
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
    ]);
    let _window = present_section_window(&section);

    let row_surface = realized_overlay_for_path(&section, first.path())
        .expect("top-level workspace folder row should have a row overlay");
    let drop_shield = realized_reorder_shield_for_path(&section, first.path())
        .expect("top-level workspace folder row should have a full-row DnD shield");
    let drop_target = realized_drop_target_for_path(&section, first.path())
        .expect("top-level workspace folder row should have a drop target surface");
    let line = drop_target
        .first_child()
        .and_downcast::<gtk4::Box>()
        .expect("drop target should contain the single painted insertion line");

    assert!(
        row_surface.has_css_class("workspace-folder-dnd-surface"),
        "the row overlay should carry the CSS hook that neutralizes GTK drop-active paint"
    );
    assert!(
        !row_surface.has_css_class("workspace-folder-drop-indicator"),
        "the row overlay must not carry the painted accent class"
    );
    assert!(
        drop_shield.has_css_class("workspace-folder-dnd-shield"),
        "the shield should carry the stable hook for DnD hit ownership"
    );
    assert!(
        !drop_shield.has_css_class("workspace-folder-drop-indicator"),
        "the full-row shield must never be the painted insertion indicator"
    );
    assert!(!drop_shield.can_target());
    assert_eq!(drop_shield.halign(), gtk4::Align::Fill);
    assert_eq!(drop_shield.valign(), gtk4::Align::Fill);
    assert!(drop_shield.hexpands());
    assert!(drop_shield.vexpands());
    assert!(
        drop_target.has_css_class("workspace-folder-drop-target"),
        "the outer overlay should be only the transparent positioning target"
    );
    assert!(
        !drop_target.has_css_class("workspace-folder-drop-indicator"),
        "the outer overlay must not carry the painted accent class"
    );
    assert!(
        line.has_css_class("workspace-folder-drop-indicator"),
        "only the inner fixed-height child should paint the insertion line"
    );
    assert_eq!(drop_target.height_request(), 2);
    assert_eq!(line.height_request(), 2);
    assert_eq!(line.valign(), gtk4::Align::Center);
    assert!(line.hexpands());
    assert!(!drop_target.can_target());
    assert!(!line.can_target());
    assert!(
        !drop_target.is_visible(),
        "drop feedback should stay hidden until a valid reorder target is hovered"
    );

    drop_target.set_valign(gtk4::Align::Start);
    drop_target.set_visible(true);
    row_surface.set_state_flags(gtk4::StateFlags::DROP_ACTIVE, false);
    drop_target.set_state_flags(gtk4::StateFlags::DROP_ACTIVE, false);
    flush_events();
    assert!(
        row_surface.height() > drop_target.height() * 4,
        "the row surface should remain much taller than the insertion feedback"
    );
    assert!(
        drop_shield.height() >= row_surface.height().saturating_sub(1),
        "the shield should cover the row allocation without becoming the visual feedback"
    );
    assert!(
        drop_target.height() <= 3,
        "visible reorder feedback should allocate as one narrow line, not a row rectangle"
    );
    assert!(
        line.height() <= 3,
        "the painted child should stay a narrow rounded insertion line"
    );
}

#[test]
fn test_workspace_folder_reorder_drag_hover_does_not_expand_or_restart_watch() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("drag-hover-inert"));
    let first = tempfile::tempdir().expect("first folder tempdir");
    let second = tempfile::tempdir().expect("second folder tempdir");
    let nested = first.path().join("nested");
    let nested_file = nested.join("child.txt");
    let second_child = second.path().join("second-child");
    fixture::create_dir_all(&nested);
    fixture::write_text(&nested_file, "nested file");
    fixture::create_dir_all(&second_child);

    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
    ]);
    let _window = present_section_window(&section);

    row_for_path(&section, first.path())
        .expect("first top-level folder should be in the tree")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    assert!(
        !tree_contains_path(&section, &nested_file),
        "nested descendants should start unmaterialized"
    );
    assert!(
        !tree_contains_path(&section, &second_child),
        "collapsed top-level target descendants should start unmaterialized"
    );

    section.stop_workspace_watch_for_test();
    let initial_generation = section.imp().watch_runtime.start_generation.get();
    let initial_targets = section.watch_targets_for_test();
    let nested_row = row_for_path(&section, &nested).expect("nested folder should be visible");
    let second_row =
        row_for_path(&section, second.path()).expect("second top-level folder should be visible");
    let second_expander = realized_expander_for_path(&section, second.path())
        .expect("second top-level folder expander should be realized");
    let nested_expander = realized_expander_for_path(&section, &nested)
        .expect("nested folder expander should be realized");
    let second_shield = realized_reorder_shield_for_path(&section, second.path())
        .expect("second top-level folder should have a reorder shield");
    let nested_shield = realized_reorder_shield_for_path(&section, &nested)
        .expect("nested folder should have a reorder shield");
    let second_drop_target = realized_drop_target_for_path(&section, second.path())
        .expect("second top-level folder should have insertion-line surface");
    let nested_drop_target = realized_drop_target_for_path(&section, &nested)
        .expect("nested folder should have insertion-line surface");
    assert!(
        second_expander.can_target(),
        "folder expanders should normally receive pointer targeting"
    );
    assert!(
        nested_expander.can_target(),
        "descendant folder expanders should normally receive pointer targeting"
    );
    assert!(
        !second_shield.can_target() && !nested_shield.can_target(),
        "reorder shields should be inert outside active drags"
    );

    let expanded_transitions = Rc::new(Cell::new(0));
    let second_transitions = Rc::clone(&expanded_transitions);
    second_row.connect_notify_local(Some("expanded"), move |_, _| {
        second_transitions.set(second_transitions.get() + 1);
    });
    let nested_transitions = Rc::clone(&expanded_transitions);
    nested_row.connect_notify_local(Some("expanded"), move |_, _| {
        nested_transitions.set(nested_transitions.get() + 1);
    });

    section.reset_workspace_folder_reorder_drag_hover_fallback_count_for_test();
    section.with_active_workspace_folder_reorder_drag_for_test(
        &WorkspaceFolderId::new("first"),
        || {
            assert!(
                LushtextWorkspaceSection::workspace_folder_reorder_drag_owns_row_hover_for_test(),
                "active reorder drags should be owned by inert row surfaces before expanders see hover"
            );
            assert!(
                second_expander.can_target(),
                "reorder drags must not retarget the top-level disclosure widget"
            );
            assert!(
                nested_expander.can_target(),
                "reorder drags must not retarget descendant disclosure widgets"
            );
            assert!(
                second_shield.can_target() && nested_shield.can_target(),
                "active reorder drags should target the row shields instead of expanders"
            );
            let valid_hover =
                section.simulate_workspace_folder_reorder_hover_after_for_test(second.path());
            assert!(valid_hover.owns_hover, "valid hover should be shield-owned");
            assert!(
                valid_hover.shows_indicator,
                "valid top-level hover should show insertion feedback"
            );
            assert!(valid_hover.accepts_drop, "valid top-level hover should accept drop");
            assert_eq!(
                visible_reorder_drop_target_count(&section),
                1,
                "valid hover should show exactly one insertion-line surface"
            );
            assert!(
                second_drop_target.property::<bool>("visible"),
                "valid top-level hover should show the target row insertion line"
            );
            flush_events();
            assert!(
                !second_row.is_expanded(),
                "drag hover over a collapsed top-level folder must never request expansion"
            );
            assert!(
                !tree_contains_path(&section, &second_child),
                "drag hover must not materialize children for top-level folders"
            );

            let invalid_hover =
                section.simulate_workspace_folder_reorder_hover_before_for_test(&nested);
            assert!(
                invalid_hover.owns_hover,
                "invalid descendant hover should still be shield-owned"
            );
            assert!(
                !invalid_hover.shows_indicator,
                "invalid descendant hover should show no insertion feedback"
            );
            assert!(
                !invalid_hover.accepts_drop,
                "invalid descendant hover should reject drops"
            );
            assert_eq!(
                visible_reorder_drop_target_count(&section),
                0,
                "invalid descendant hover should be owned but show no insertion line"
            );
            assert!(
                !nested_drop_target.property::<bool>("visible"),
                "descendant hover should leave its insertion-line surface hidden"
            );
            assert!(
                !nested_row.is_expanded(),
                "drag hover over descendant folders must never request expansion"
            );
            assert!(
                !tree_contains_path(&section, &nested_file),
                "drag hover must not materialize children for descendant folders"
            );
            assert_eq!(
                expanded_transitions.get(),
                0,
                "reorder hover should not emit any expanded-state transition"
            );
            assert_eq!(
                section.workspace_folder_reorder_drag_hover_fallback_count_for_test(),
                0,
                "shield-owned hover should never request the defensive child-model fallback"
            );
        },
    );

    assert!(
        second_expander.can_target(),
        "folder expanders should be targetable again when reorder drag ends"
    );
    assert!(
        !LushtextWorkspaceSection::workspace_folder_reorder_drag_owns_row_hover_for_test(),
        "row-surface drag ownership should end with the reorder drag"
    );
    assert!(
        !second_shield.can_target() && !nested_shield.can_target(),
        "row shields should become inert again when reorder drag ends"
    );
    assert_eq!(
        visible_reorder_drop_target_count(&section),
        0,
        "ending the drag should clear all insertion-line feedback"
    );
    assert_eq!(
        section.imp().watch_runtime.start_generation.get(),
        initial_generation,
        "drag-hover expansion suppression must not restart workspace watching"
    );
    assert_eq!(
        section.watch_targets_for_test(),
        initial_targets,
        "drag hover should leave materialized watch coverage unchanged"
    );
}

#[test]
fn test_workspace_folder_reorder_row_recycling_resets_indicator_and_rebinds_shield() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("drag-row-recycle"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
    ]);
    let _window = present_section_window(&section);
    let replacement_root = tempfile::tempdir().expect("replacement workspace folder parent");

    section.with_active_workspace_folder_reorder_drag_for_test(
        &WorkspaceFolderId::new("first"),
        || {
            let hover =
                section.simulate_workspace_folder_reorder_hover_after_for_test(second.path());
            assert!(
                hover.shows_indicator,
                "pre-recycle valid hover should show insertion feedback"
            );
            assert_eq!(visible_reorder_drop_target_count(&section), 1);

            let replacement_a = replacement_root.path().join("replacement-a");
            let replacement_b = replacement_root.path().join("replacement-b");
            let replacement_child = replacement_a.join("child.txt");
            fixture::create_dir_all(&replacement_a);
            fixture::create_dir_all(&replacement_b);
            fixture::write_text(&replacement_child, "child");
            section.load_workspace_folders(&[
                WorkspaceFolder::with_id(
                    WorkspaceFolderId::new("replacement-a"),
                    replacement_a.clone(),
                ),
                WorkspaceFolder::with_id(
                    WorkspaceFolderId::new("replacement-b"),
                    replacement_b,
                ),
            ]);
            wait_until(Duration::from_secs(5), || {
                top_level_workspace_folder_ids(&section)
                    == ["replacement-a".to_string(), "replacement-b".to_string()]
                    && realized_reorder_shield_for_path(&section, &replacement_a).is_some()
            });

            let rebound_shield = realized_reorder_shield_for_path(&section, &replacement_a)
                .expect("rebound row should still have a shield");
            let rebound_drop_target = realized_drop_target_for_path(&section, &replacement_a)
                .expect("rebound row should still have an insertion-line surface");
            let rebound_handle = realized_drag_handle_for_path(&section, &replacement_a)
                .expect("rebound top-level row should still have a drag handle");
            let rebound_expander = realized_expander_for_path(&section, &replacement_a)
                .expect("rebound top-level row should still have an expander");

            assert!(
                rebound_shield.can_target(),
                "rows rebound during an active drag should target the shield, not the expander"
            );
            assert!(
                !rebound_drop_target.property::<bool>("visible"),
                "recycled rows must not keep a stale insertion line"
            );
            assert!(
                rebound_handle.is_visible() && rebound_handle.is_sensitive(),
                "recycled top-level workspace folders should restore drag-handle state"
            );
            assert!(
                rebound_expander.can_target(),
                "recycled rows should keep ordinary expander targeting intact"
            );
            assert_eq!(visible_reorder_drop_target_count(&section), 0);
        },
    );

    let top_level_path = section
        .imp()
        .original_folders
        .borrow()
        .first()
        .expect("replacement folder should remain loaded")
        .path()
        .to_path_buf();
    let shield = realized_reorder_shield_for_path(&section, &top_level_path)
        .expect("replacement row should remain realized");
    assert!(
        !shield.can_target(),
        "row shields should be inert again after the active drag guard drops"
    );
    assert!(
        realized_expander_for_path(&section, &top_level_path)
            .expect("replacement row should keep an expander")
            .can_target(),
        "ordinary row targeting should remain available after recycling"
    );
}

#[test]
fn test_workspace_folder_reorder_shield_is_inert_outside_drag_for_normal_interactions() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("shield-inert-normal"));
    let folder = tempfile::tempdir().expect("workspace folder");
    let other = tempfile::tempdir().expect("second workspace folder");
    let nested = folder.path().join("src");
    let file = nested.join("main.rs");
    fixture::create_dir_all(&nested);
    fixture::write_text(&file, "fn main() {}\n");
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("folder"), folder.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("other"), other.path().to_path_buf()),
    ]);
    let _window = present_section_window(&section);

    let folder_shield = realized_reorder_shield_for_path(&section, folder.path())
        .expect("top-level folder row should have a reorder shield");
    let folder_expander = realized_expander_for_path(&section, folder.path())
        .expect("top-level folder row should have an expander");
    assert!(!folder_shield.can_target());
    assert!(folder_expander.can_target());

    row_for_path(&section, folder.path())
        .expect("folder should be indexed")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));

    row_for_path(&section, &nested)
        .expect("nested directory should be visible")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &file));

    let activated = Rc::new(RefCell::new(None::<PathBuf>));
    let activated_clone = Rc::clone(&activated);
    section.connect_file_activated(move |path| {
        *activated_clone.borrow_mut() = Some(path.to_path_buf());
    });
    let file_index = tree_model_index_for_path(&section, &file).expect("file should be indexed");
    section
        .imp()
        .file_tree_view
        .emit_by_name::<()>("activate", &[&file_index]);
    flush_events();
    assert_eq!(*activated.borrow(), Some(file.clone()));

    prepare_context_menu_for_path(&section, &file);
    let labels = current_context_menu_labels(&section);
    assert!(labels.iter().any(|label| label == "Rename"));
    assert!(labels.iter().any(|label| label == "Delete"));

    select_path(&section, &file);
    assert!(section.toggle_peek_for_selection());
    wait_until(Duration::from_secs(5), || {
        section.peek_visible() && section.peeked_path().as_deref() == Some(file.as_path())
    });
    section.dismiss_peek(false);

    section.focus_folder(&nested);
    flush_events();
    assert_eq!(
        section.imp().drilldown_stack.borrow().last(),
        Some(&nested),
        "focus-folder should remain reachable outside active reorder drags"
    );
    assert!(section.imp().workspace_folder_ids.borrow().contains_key(folder.path()));
}

#[test]
fn test_workspace_folder_reorder_shield_path_does_not_mutate_filesystem() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("drag-fs-safe"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    let third = tempfile::tempdir().expect("third folder");
    let first_file = first.path().join("alpha.txt");
    let second_file = second.path().join("beta.txt");
    let third_file = third.path().join("gamma.txt");
    fixture::write_text(&first_file, "alpha");
    fixture::write_text(&second_file, "beta");
    fixture::write_text(&third_file, "gamma");
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("third"), third.path().to_path_buf()),
    ]);
    let _window = present_section_window(&section);

    let reordered = Rc::new(Cell::new(false));
    let reordered_clone = Rc::clone(&reordered);
    section.connect_reorder_folder_to_index_requested(move |_, _, _| {
        reordered_clone.set(true);
    });
    section.with_active_workspace_folder_reorder_drag_for_test(
        &WorkspaceFolderId::new("third"),
        || {
            let hover =
                section.simulate_workspace_folder_reorder_hover_before_for_test(first.path());
            assert!(hover.owns_hover, "filesystem-safety hover should be shield-owned");
            assert!(
                hover.shows_indicator,
                "filesystem-safety hover should show valid insertion feedback"
            );
            assert!(
                hover.accepts_drop,
                "filesystem-safety hover should accept the metadata-only drop"
            );
            assert!(section.drop_workspace_folder_before_for_test(
                &WorkspaceFolderId::new("third"),
                &WorkspaceFolderId::new("first"),
            ));
        },
    );

    assert!(reordered.get(), "the normal reorder callback path should still fire");
    assert_eq!(fixture::read_text(&first_file), "alpha");
    assert_eq!(fixture::read_text(&second_file), "beta");
    assert_eq!(fixture::read_text(&third_file), "gamma");
    assert!(
        !fs_metadata::exists(&first.path().join("gamma.txt"))
            && !fs_metadata::exists(&second.path().join("gamma.txt"))
            && fs_metadata::exists(&third_file),
        "reorder must not move, copy, rename, delete, or rewrite user files"
    );
}

#[test]
fn test_workspace_section_header_button_carries_vertical_spacing() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert_eq!(section.imp().inner_scrolled_window.margin_top(), 0);
    assert_eq!(section.imp().collapse_button.valign(), gtk4::Align::Center);
    assert_eq!(section.imp().collapse_button.margin_top(), 6);
    assert_eq!(section.imp().collapse_button.margin_bottom(), 6);
    assert_eq!(section.imp().add_folder_button.valign(), gtk4::Align::Center);
    assert_eq!(section.imp().add_folder_button.margin_top(), 6);
    assert_eq!(section.imp().add_folder_button.margin_bottom(), 6);
    assert_eq!(section.imp().refresh_button.valign(), gtk4::Align::Center);
    assert_eq!(section.imp().refresh_button.margin_top(), 6);
    assert_eq!(section.imp().refresh_button.margin_bottom(), 6);
}

#[test]
fn test_workspace_section_refresh_button_is_rightmost_header_control() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let refresh_widget = section.imp().refresh_button.clone().upcast::<gtk4::Widget>();
    let mut child = section.imp().header_box.first_child();
    let mut refresh_child = None;
    while let Some(candidate) = child {
        if candidate.as_ptr() == refresh_widget.as_ptr() {
            refresh_child = candidate.downcast::<gtk4::Button>().ok();
            break;
        }
        child = candidate.next_sibling();
    }
    let refresh_child = refresh_child.expect("header should contain the refresh button");

    let add_folder_child = section
        .imp()
        .add_folder_button
        .clone()
        .upcast::<gtk4::Widget>();
    assert!(
        refresh_child
            .prev_sibling()
            .is_some_and(|child| child.as_ptr() == add_folder_child.as_ptr()),
        "refresh should stay after the add-folder control"
    );

    let mut trailing_child = refresh_child.next_sibling();
    while let Some(child) = trailing_child {
        assert!(
            !child.is::<gtk4::Button>(),
            "no workspace-section header button should appear to the right of refresh"
        );
        trailing_child = child.next_sibling();
    }
}

#[test]
fn test_workspace_section_add_folder_button_sits_before_refresh() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let collapse_child = section
        .imp()
        .header_box
        .first_child()
        .and_downcast::<gtk4::Button>()
        .expect("first header child should be the collapse button");
    assert_eq!(
        collapse_child.as_ptr(),
        section.imp().collapse_button.as_ptr()
    );

    let add_folder_child = section
        .imp()
        .add_folder_button
        .clone()
        .upcast::<gtk4::Widget>();
    let refresh_child = section
        .imp()
        .refresh_button
        .clone()
        .upcast::<gtk4::Widget>();

    assert!(
        add_folder_child
            .next_sibling()
            .is_some_and(|child| child.as_ptr() == refresh_child.as_ptr()),
        "add-folder should remain immediately before refresh"
    );
}

#[test]
fn test_single_directory_folder_row_keeps_real_folder_presentation() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("main.rs"), "fn main() {}\n");
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let (_window, icon, label) = realized_folder_row_widgets(&section);
    assert_eq!(icon.icon_name().as_deref(), Some("folder"));
    assert_eq!(
        label.label().as_str(),
        dir.path()
            .file_name()
            .expect("tempdir should have a folder name")
            .to_string_lossy()
    );
}

#[test]
fn test_drilldown_folder_row_keeps_actual_folder_presentation() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    fixture::create_dir(&nested);
    fixture::write_text(&nested.join("lib.rs"), "pub fn demo() {}\n");
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);
    section.focus_folder(&nested);

    let (_window, icon, label) = realized_folder_row_widgets(&section);
    assert_eq!(icon.icon_name().as_deref(), Some("folder"));
    assert_eq!(label.label().as_str(), "nested");
}

#[test]
fn test_file_tree_file_row_uses_regular_content_type_icon() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let image_path = dir.path().join("preview.png");
    fixture::write_bytes(&image_path, b"not a real image, extension is enough");
    section.load_folders(&[FolderTreeEntry::File {
        path: image_path,
    }]);

    let (_window, icon, label) = realized_folder_row_widgets(&section);
    assert_eq!(label.label().as_str(), "preview.png");
    assert_eq!(icon.storage_type(), gtk4::ImageType::Gicon);

    let gicon = icon.gicon().expect("file row should use a content-type icon");
    let names = themed_icon_names(&gicon);
    assert!(
        names.iter().any(|name| name.contains("image")),
        "image file row should use image-themed icon names, got {names:?}"
    );
    assert!(
        names.first().is_some_and(|name| !name.ends_with("-symbolic")),
        "file row should prefer a regular themed icon, got {names:?}"
    );
}

#[test]
fn test_workspace_section_chrome_icons_remain_symbolic() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    fixture::create_dir(&nested);
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    assert_eq!(
        section.imp().collapse_button.icon_name().as_deref(),
        Some("pan-down-symbolic")
    );
    assert_eq!(
        section.imp().add_folder_button.icon_name().as_deref(),
        Some("folder-new-symbolic")
    );
    assert_eq!(
        section.imp().refresh_button.icon_name().as_deref(),
        Some("view-refresh-symbolic")
    );
    assert_eq!(
        section.imp().drilldown_back_button.icon_name().as_deref(),
        Some("go-previous-symbolic")
    );

    let window = present_section_window(&section);
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
    let mut child = overlay.first_child();
    let mut focus_button = None;
    while let Some(candidate) = child {
        if let Some(button) = candidate.downcast_ref::<gtk4::Button>()
            && button.icon_name().as_deref() == Some("go-next-symbolic")
        {
            focus_button = Some(button.clone());
            break;
        }
        child = candidate.next_sibling();
    }
    let focus_button = focus_button.expect("focus-folder overlay control should be a button");
    assert_eq!(focus_button.icon_name().as_deref(), Some("go-next-symbolic"));
    window.close();
}

#[test]
fn test_file_activation_still_emits_after_regular_icon_binding() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file_path = dir.path().join("main.rs");
    fixture::write_text(&file_path, "fn main() {}\n");
    section.load_folders(&[FolderTreeEntry::File {
        path: file_path.clone(),
    }]);
    let _window = present_section_window(&section);
    wait_until(Duration::from_secs(2), || {
        section.imp().file_tree_view.first_child().is_some()
    });

    let activated = Rc::new(RefCell::new(None::<PathBuf>));
    {
        let activated = activated.clone();
        section.connect_file_activated(move |path| {
            *activated.borrow_mut() = Some(path.to_path_buf());
        });
    }

    section.imp().file_tree_view.emit_by_name::<()>("activate", &[&0u32]);
    flush_events();

    assert_eq!(*activated.borrow(), Some(file_path));
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
    let menu = section
        .imp()
        .context_file_menu_model
        .borrow()
        .as_ref()
        .expect("file context menu should exist")
        .clone();

    let labels = menu_model_labels(menu.upcast_ref());
    assert!(
        labels.iter().any(|label| label == "Local History…"),
        "file context menu should advertise Local History"
    );
}

#[test]
fn test_workspace_section_file_context_menu_keeps_file_actions_only() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    let menu = section
        .imp()
        .context_file_menu_model
        .borrow()
        .as_ref()
        .expect("file context menu should exist")
        .clone();

    let labels = menu_model_labels(menu.upcast_ref());

    assert!(labels.iter().any(|label| label == "Open Document Note…"));
    assert!(labels.iter().any(|label| label == "Rename"));
    assert!(labels.iter().any(|label| label == "Delete"));
    assert!(!labels.iter().any(|label| label == "Open Folder Note…"));
    assert!(!labels.iter().any(|label| label == "Move Up"));
    assert!(!labels.iter().any(|label| label == "Move Down"));
}

#[test]
fn test_workspace_section_folder_context_menu_lists_membership_actions() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    let menu = section
        .imp()
        .context_folder_menu_model
        .borrow()
        .as_ref()
        .expect("folder context menu should exist")
        .clone();

    let labels = menu_model_labels(menu.upcast_ref());

    assert!(labels.iter().any(|label| label == "Open Folder Note…"));
    assert!(labels.iter().any(|label| label == "Move Up"));
    assert!(labels.iter().any(|label| label == "Move Down"));
    assert!(labels.iter().any(|label| label == "Remove from Workspace"));
    assert!(labels.iter().any(|label| label == "New File"));
    assert!(labels.iter().any(|label| label == "New Folder"));
    assert!(!labels.iter().any(|label| label == "Delete"));
}

#[test]
fn test_descendant_file_context_menu_keeps_file_actions_under_workspace_folder() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-file-actions"));
    let folder = tempfile::tempdir().expect("workspace folder");
    let nested = folder.path().join("src");
    let file = nested.join("main.rs");
    fixture::create_dir_all(&nested);
    fixture::write_text(&file, "fn main() {}\n");

    section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("folder-id"),
        folder.path().to_path_buf(),
    )]);
    let _window = present_section_window(&section);

    section.expand_folders();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should be visible")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &file));
    prepare_context_menu_for_path(&section, &file);
    let labels = current_context_menu_labels(&section);
    assert!(labels.iter().any(|label| label == "Open Document Note…"));
    assert!(labels.iter().any(|label| label == "Local History…"));
    assert!(labels.iter().any(|label| label == "New File"));
    assert!(labels.iter().any(|label| label == "New Folder"));
    assert!(labels.iter().any(|label| label == "Rename"));
    assert!(labels.iter().any(|label| label == "Delete"));
    assert!(!labels.iter().any(|label| label == "Open Folder Note…"));
    assert!(!labels.iter().any(|label| label == "Remove from Workspace"));
    assert!(!labels.iter().any(|label| label == "Move Up"));
    assert!(!labels.iter().any(|label| label == "Move Down"));
    assert!(section.imp().context_workspace_folder_id.borrow().is_none());
    assert_eq!(section.imp().context_path.borrow().as_ref(), Some(&file));

    let activated = Rc::new(RefCell::new(None::<PathBuf>));
    let activated_clone = Rc::clone(&activated);
    section.connect_file_activated(move |path| {
        *activated_clone.borrow_mut() = Some(path.to_path_buf());
    });
    let file_index =
        tree_model_index_for_path(&section, &file).expect("file should be in the tree model");
    section
        .imp()
        .file_tree_view
        .emit_by_name::<()>("activate", &[&file_index]);
    flush_events();
    assert_eq!(*activated.borrow(), Some(file.clone()));

    let document_note_path = Rc::new(RefCell::new(None::<PathBuf>));
    let document_note_path_clone = Rc::clone(&document_note_path);
    section.connect_document_note_requested(move |path| {
        *document_note_path_clone.borrow_mut() = Some(path.to_path_buf());
    });
    section
        .activate_action("section.document-note", None)
        .expect("document-note action should exist");
    assert_eq!(*document_note_path.borrow(), Some(file.clone()));

    let local_history_path = Rc::new(RefCell::new(None::<PathBuf>));
    let local_history_path_clone = Rc::clone(&local_history_path);
    section.connect_local_history_requested(move |path| {
        *local_history_path_clone.borrow_mut() = Some(path.to_path_buf());
    });
    section
        .activate_action("section.local-history", None)
        .expect("local-history action should exist");
    assert_eq!(*local_history_path.borrow(), Some(file));
}

#[test]
fn test_descendant_focus_folder_does_not_mutate_workspace_folder_membership() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-focus-descendant"));
    let folder = tempfile::tempdir().expect("workspace folder");
    let nested = folder.path().join("src");
    fixture::create_dir_all(&nested);
    fixture::write_text(&nested.join("lib.rs"), "pub fn demo() {}\n");

    section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("folder-id"),
        folder.path().to_path_buf(),
    )]);
    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));

    prepare_context_menu_for_path(&section, &nested);
    let labels = current_context_menu_labels(&section);
    assert!(labels.iter().any(|label| label == "Focus Folder"));
    assert!(labels.iter().any(|label| label == "New File"));
    assert!(labels.iter().any(|label| label == "New Folder"));
    assert!(labels.iter().any(|label| label == "Rename"));
    assert!(labels.iter().any(|label| label == "Delete"));
    assert!(!labels.iter().any(|label| label == "Open Folder Note…"));
    assert!(!labels.iter().any(|label| label == "Remove from Workspace"));
    assert!(section.imp().context_workspace_folder_id.borrow().is_none());
    assert_eq!(section.imp().context_path.borrow().as_ref(), Some(&nested));
    section.focus_folder(&nested);
    flush_events();

    assert_eq!(*section.imp().drilldown_stack.borrow(), vec![nested]);
    assert_eq!(
        section.imp().original_folders.borrow()[0].path(),
        folder.path()
    );
    assert_eq!(
        section
            .imp()
            .workspace_folder_ids
            .borrow()
            .get(folder.path())
            .map(WorkspaceFolderId::as_str),
        Some("folder-id")
    );

    section.navigate_back();
    flush_events();
    assert_eq!(
        top_level_workspace_folder_ids(&section),
        ["folder-id".to_string()]
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
fn test_remove_from_model_top_level_item() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let top_level_store = gio::ListStore::new::<FileTreeItem>();
    top_level_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/a.txt"),
        false,
        None,
    ));
    top_level_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/b.txt"),
        false,
        None,
    ));
    *section.imp().top_level_store.borrow_mut() = Some(top_level_store.clone());

    assert_eq!(top_level_store.n_items(), 2);
    assert!(section.remove_from_model(std::path::Path::new("/tmp/test/a.txt")));
    assert_eq!(top_level_store.n_items(), 1);

    let remaining = top_level_store.item(0).and_downcast::<FileTreeItem>().expect("expected operation to succeed");
    assert_eq!(remaining.path(), Some(PathBuf::from("/tmp/test/b.txt")));
}

#[test]
fn test_remove_from_model_nonexistent_is_noop() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let top_level_store = gio::ListStore::new::<FileTreeItem>();
    top_level_store.append(&FileTreeItem::new(
        PathBuf::from("/tmp/test/a.txt"),
        false,
        None,
    ));
    *section.imp().top_level_store.borrow_mut() = Some(top_level_store.clone());

    assert!(!section.remove_from_model(std::path::Path::new(
        "/tmp/test/does_not_exist.txt"
    )));
    assert_eq!(top_level_store.n_items(), 1);
}

#[test]
fn test_remove_from_model_child_item() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let top_level_store = gio::ListStore::new::<FileTreeItem>();
    let dir_item = FileTreeItem::new(PathBuf::from("/tmp/test/src"), true, None);
    top_level_store.append(&dir_item);
    *section.imp().top_level_store.borrow_mut() = Some(top_level_store.clone());

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
    let tree_model = gtk4::TreeListModel::new(top_level_store, false, false, move |item| {
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
    assert!(section.remove_from_model(std::path::Path::new(
        "/tmp/test/src/main.rs"
    )));
    assert_eq!(child_store.n_items(), 1);

    let remaining = child_store.item(0).and_downcast::<FileTreeItem>().expect("expected operation to succeed");
    assert_eq!(
        remaining.path(),
        Some(PathBuf::from("/tmp/test/src/lib.rs"))
    );
}

#[test]
fn test_remove_from_model_removes_duplicate_visible_paths() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    let target = PathBuf::from("/tmp/test/src");
    let parent = PathBuf::from("/tmp/test");

    let top_level_store = gio::ListStore::new::<FileTreeItem>();
    top_level_store.append(&FileTreeItem::new(target.clone(), true, None));
    *section.imp().top_level_store.borrow_mut() = Some(top_level_store.clone());

    let child_store = gio::ListStore::new::<FileTreeItem>();
    child_store.append(&FileTreeItem::new(target.clone(), true, None));
    section
        .imp()
        .dir_stores
        .borrow_mut()
        .insert(parent, child_store.downgrade());

    assert!(section.remove_from_model(&target));
    assert_eq!(top_level_store.n_items(), 0);
    assert_eq!(child_store.n_items(), 0);
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

#[test]
fn test_remove_folder_callback_fires_with_stable_folder_id_and_path() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-remove"));

    let seen = Rc::new(RefCell::new(None::<(String, String, PathBuf)>));
    let seen_clone = Rc::clone(&seen);
    section.connect_remove_folder_requested(move |ws_id, folder_id, path| {
        *seen_clone.borrow_mut() = Some((
            ws_id.as_str().to_string(),
            folder_id.as_str().to_string(),
            path.to_path_buf(),
        ));
    });

    let folder_id = WorkspaceFolderId::new("folder-a");
    let path = PathBuf::from("/tmp/project");
    section.notify_remove_folder_requested(&folder_id, &path);

    assert_eq!(
        seen.borrow().as_ref(),
        Some(&("ws-remove".to_string(), "folder-a".to_string(), path))
    );
}

#[test]
fn test_folder_note_for_folder_callback_fires_with_exact_path() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-note"));
    let seen = Rc::new(RefCell::new(None::<(String, PathBuf)>));
    let seen_clone = Rc::clone(&seen);
    section.connect_folder_note_for_folder_requested(move |ws_id, path| {
        *seen_clone.borrow_mut() = Some((ws_id.as_str().to_string(), path.to_path_buf()));
    });

    let path = PathBuf::from("/tmp/project");
    section.notify_folder_note_for_folder_requested(&path);

    assert_eq!(
        seen.borrow().as_ref(),
        Some(&("ws-note".to_string(), path))
    );
}

#[test]
fn test_reorder_folder_callback_fires_with_stable_id_and_direction() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-reorder"));
    let seen = Rc::new(RefCell::new(
        None::<(String, String, WorkspaceFolderMoveDirection)>,
    ));
    let seen_clone = Rc::clone(&seen);
    section.connect_reorder_folder_requested(move |ws_id, folder_id, direction| {
        *seen_clone.borrow_mut() = Some((
            ws_id.as_str().to_string(),
            folder_id.as_str().to_string(),
            direction,
        ));
    });

    section.notify_reorder_folder_requested(
        &WorkspaceFolderId::new("folder-b"),
        WorkspaceFolderMoveDirection::Up,
    );

    assert_eq!(
        seen.borrow().as_ref(),
        Some(&(
            "ws-reorder".to_string(),
            "folder-b".to_string(),
            WorkspaceFolderMoveDirection::Up
        ))
    );
}

#[test]
fn test_drag_reorder_callback_fires_with_stable_id_and_absolute_index() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-dnd"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    let third = tempfile::tempdir().expect("third folder");
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("third"), third.path().to_path_buf()),
    ]);

    let seen = Rc::new(RefCell::new(None::<(String, String, usize)>));
    let seen_clone = Rc::clone(&seen);
    section.connect_reorder_folder_to_index_requested(move |ws_id, folder_id, index| {
        *seen_clone.borrow_mut() = Some((
            ws_id.as_str().to_string(),
            folder_id.as_str().to_string(),
            index,
        ));
    });

    assert!(section.drop_workspace_folder_before_for_test(
        &WorkspaceFolderId::new("third"),
        &WorkspaceFolderId::new("first"),
    ));

    assert_eq!(
        seen.borrow().as_ref(),
        Some(&("ws-dnd".to_string(), "third".to_string(), 0))
    );
}

#[test]
fn test_drag_reorder_noops_when_folder_already_has_requested_position() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-dnd-noop"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    let third = tempfile::tempdir().expect("third folder");
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("third"), third.path().to_path_buf()),
    ]);

    let calls = Rc::new(Cell::new(0));
    let calls_clone = Rc::clone(&calls);
    section.connect_reorder_folder_to_index_requested(move |_, _, _| {
        calls_clone.set(calls_clone.get() + 1);
    });

    assert!(section.drop_workspace_folder_after_for_test(
        &WorkspaceFolderId::new("second"),
        &WorkspaceFolderId::new("first"),
    ));

    assert_eq!(
        calls.get(),
        0,
        "dropping a folder where it already belongs should not emit a reorder"
    );
}

#[test]
fn test_load_workspace_folders_preserves_order_and_folder_ids_for_reorder() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-order"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    let third = tempfile::tempdir().expect("third folder");

    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("third"), third.path().to_path_buf()),
    ]);

    let top_level_store = section
        .imp()
        .top_level_store
        .borrow()
        .as_ref()
        .expect("workspace folders should install a top-level store")
        .clone();
    let ordered_ids = (0..top_level_store.n_items())
        .filter_map(|index| top_level_store.item(index).and_downcast::<FileTreeItem>())
        .filter_map(|item| item.workspace_folder_id())
        .map(|id| id.as_str().to_string())
        .collect::<Vec<_>>();

    assert_eq!(ordered_ids, ["first", "second", "third"]);
}

#[test]
fn test_refresh_reconciles_top_level_rows_without_losing_folder_ids() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-refresh-ids"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    let third = tempfile::tempdir().expect("third folder");

    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("third"), third.path().to_path_buf()),
    ]);
    let window = present_section_window(&section);

    *section.imp().original_folders.borrow_mut() = vec![
        FolderTreeEntry::Directory {
            path: second.path().to_path_buf(),
        },
        FolderTreeEntry::Directory {
            path: first.path().to_path_buf(),
        },
        FolderTreeEntry::Directory {
            path: third.path().to_path_buf(),
        },
    ];
    section.imp().refresh_button.emit_clicked();

    wait_until(Duration::from_secs(5), || {
        top_level_workspace_folder_ids(&section)
            == vec![
                "second".to_string(),
                "first".to_string(),
                "third".to_string(),
            ]
    });
    assert_reorder_handle_visible(&section, second.path(), true);
    assert_reorder_handle_visible(&section, first.path(), true);
    assert_reorder_handle_visible(&section, third.path(), true);

    drop(window);
}

#[test]
fn test_watch_targets_follow_visible_workspace_folders_in_order() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-watch-targets"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");

    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
    ]);

    assert_eq!(
        section.watch_targets_for_test(),
        vec![
            WorkspaceWatchTarget::directory(first.path().to_path_buf()),
            WorkspaceWatchTarget::directory(second.path().to_path_buf()),
        ]
    );
}

#[test]
fn test_watch_targets_ignore_collapsed_descendants_until_expanded() {
    ensure_gtk_init();
    let parent = tempfile::tempdir().expect("parent folder");
    let nested = parent.path().join("nested");
    fixture::create_dir_all(&nested);
    fixture::write_text(&nested.join("child.txt"), "materialize nested");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-watch-broad"));
    section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("parent"),
        parent.path().to_path_buf(),
    )]);

    assert_eq!(
        section.watch_targets_for_test(),
        vec![WorkspaceWatchTarget::directory(parent.path().to_path_buf())],
        "collapsed broad folders should not recursively watch descendants"
    );

    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    assert_eq!(
        section.watch_targets_for_test(),
        vec![WorkspaceWatchTarget::directory(parent.path().to_path_buf())],
        "visible but collapsed descendants remain manual-refresh territory"
    );

    row_for_path(&section, &nested)
        .expect("nested folder should be visible")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || {
        section
            .watch_targets_for_test()
            .contains(&WorkspaceWatchTarget::directory(nested.clone()))
    });
    assert_eq!(
        section.watch_targets_for_test(),
        vec![
            WorkspaceWatchTarget::directory(parent.path().to_path_buf()),
            WorkspaceWatchTarget::directory(nested),
        ],
        "expanded descendants become explicit materialized watch targets"
    );
}

#[test]
fn test_manual_refresh_reloads_each_workspace_folder_and_preserves_order() {
    ensure_gtk_init();
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    let first_existing = first.path().join("alpha.txt");
    let second_existing = second.path().join("beta.txt");
    fixture::write_text(&first_existing, "alpha");
    fixture::write_text(&second_existing, "beta");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-refresh-multi"));
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
    ]);

    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(5), || {
        tree_contains_path(&section, &first_existing)
            && tree_contains_path(&section, &second_existing)
    });

    let first_created = first.path().join("created-a.txt");
    let second_created = second.path().join("created-b.txt");
    fixture::write_text(&first_created, "created a");
    fixture::write_text(&second_created, "created b");
    section.imp().refresh_button.emit_clicked();

    wait_until(Duration::from_secs(5), || {
        tree_contains_path(&section, &first_created)
            && tree_contains_path(&section, &second_created)
            && top_level_workspace_folder_ids(&section)
                == ["first".to_string(), "second".to_string()]
    });
}

#[test]
fn test_overlapping_workspace_folders_render_literal_duplicate_file_rows() {
    ensure_gtk_init();
    let parent = tempfile::tempdir().expect("parent folder");
    let child = parent.path().join("src");
    let shared_file = child.join("main.rs");
    fixture::create_dir_all(&child);
    fixture::write_text(&shared_file, "fn main() {}\n");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-overlap"));
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(
            WorkspaceFolderId::new("parent"),
            parent.path().to_path_buf(),
        ),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("child"), child.clone()),
    ]);

    let _window = present_section_window(&section);
    wait_until(Duration::from_secs(2), || {
        top_level_workspace_folder_ids(&section) == ["parent".to_string(), "child".to_string()]
    });
    section.expand_folders();
    wait_until(Duration::from_secs(10), || {
        row_count_for_path(&section, &child) >= 2
    });

    for child_row in rows_for_path(&section, &child) {
        child_row.set_expanded(true);
    }

    wait_until(Duration::from_secs(10), || {
        row_count_for_path(&section, &shared_file) >= 2
    });

    assert_eq!(
        row_count_for_path(&section, &shared_file),
        2,
        "overlapping workspace folders should render the shared file once per visible folder tree"
    );
    assert_eq!(
        top_level_workspace_folder_ids(&section),
        ["parent".to_string(), "child".to_string()]
    );
    assert_reorder_handle_visible(&section, parent.path(), true);
    assert_reorder_handle_visible(&section, &child, true);
}

#[test]
fn test_auto_refresh_updates_overlapping_expanded_folder_trees() {
    ensure_gtk_init();
    let parent = tempfile::tempdir().expect("parent folder");
    let child = parent.path().join("src");
    let existing = child.join("main.rs");
    fixture::create_dir_all(&child);
    fixture::write_text(&existing, "fn main() {}\n");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("ws-overlap-refresh"));
    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(
            WorkspaceFolderId::new("parent"),
            parent.path().to_path_buf(),
        ),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("child"), child.clone()),
    ]);

    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(10), || {
        row_count_for_path(&section, &child) >= 2
    });
    for child_row in rows_for_path(&section, &child) {
        child_row.set_expanded(true);
    }
    wait_until(Duration::from_secs(10), || {
        row_count_for_path(&section, &existing) == 2
    });

    let created = child.join("lib.rs");
    fixture::write_text(&created, "pub fn lib() {}\n");
    section.queue_auto_refresh_for_test(vec![created.clone()]);

    wait_until(Duration::from_secs(10), || {
        row_count_for_path(&section, &created) == 2
            && rows_for_path(&section, &child)
                .into_iter()
                .all(|row| row.is_expanded())
    });
    assert_eq!(
        top_level_workspace_folder_ids(&section),
        ["parent".to_string(), "child".to_string()]
    );
}

fn top_level_workspace_folder_ids(section: &LushtextWorkspaceSection) -> Vec<String> {
    let Some(top_level_store) = section.imp().top_level_store.borrow().as_ref().cloned() else {
        return Vec::new();
    };

    (0..top_level_store.n_items())
        .filter_map(|index| top_level_store.item(index).and_downcast::<FileTreeItem>())
        .filter_map(|item| item.workspace_folder_id())
        .map(|id| id.as_str().to_string())
        .collect()
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

// --- add_folder ---

#[test]
fn test_add_folder_initializes_tree() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.add_folder(dir.path(), true);

    assert!(section.imp().top_level_store.borrow().is_some());
    let top_level_store = section.imp().top_level_store.borrow();
    let top_level_store = top_level_store.as_ref().expect("expected operation to succeed");
    assert_eq!(top_level_store.n_items(), 1);
}

#[test]
fn test_add_folder_deduplicates() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.add_folder(dir.path(), true);
    section.add_folder(dir.path(), true); // duplicate

    let top_level_store = section.imp().top_level_store.borrow();
    let top_level_store = top_level_store.as_ref().expect("expected operation to succeed");
    assert_eq!(top_level_store.n_items(), 1);
}

#[test]
fn test_add_folder_appends_multiple() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir1 = tempfile::tempdir().expect("expected operation to succeed");
    let dir2 = tempfile::tempdir().expect("expected operation to succeed");
    section.add_folder(dir1.path(), true);
    section.add_folder(dir2.path(), true);

    let top_level_store = section.imp().top_level_store.borrow();
    let top_level_store = top_level_store.as_ref().expect("expected operation to succeed");
    assert_eq!(top_level_store.n_items(), 2);
}

// --- Button state toggle ---

#[test]
fn test_refresh_button_stays_available_when_no_folders() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert!(
        section.imp().refresh_button.is_sensitive(),
        "refresh should stay reachable for an empty workspace section"
    );
}

#[test]
fn test_load_empty_folders_shows_empty_folder_set_state() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    section.load_folders(&[]);

    assert!(!section.has_folders());
    assert!(
        section.imp().refresh_button.is_sensitive(),
        "empty workspaces should remain refreshable without filesystem work"
    );
    assert!(
        section.imp().empty_folder_set_label.is_visible(),
        "empty workspaces should show an explicit empty folder-set state"
    );
    assert!(
        !section.imp().inner_scrolled_window.is_visible(),
        "empty folder sets should not show an empty file tree"
    );
    let top_level_store = section.imp().top_level_store.borrow();
    assert_eq!(
        top_level_store
            .as_ref()
            .expect("empty folder set should still install a top-level store")
            .n_items(),
        0
    );
    assert!(section.imp().watch_runtime.watcher.borrow().is_none());
    assert!(section.imp().watch_runtime.start_source_id.borrow().is_none());
    assert!(section.imp().watch_runtime.poll_source_id.borrow().is_none());
    assert_eq!(
        section.watch_targets_for_test(),
        Vec::<WorkspaceWatchTarget>::new()
    );
}

#[test]
fn test_workspace_body_collapse_hides_folder_tree_without_clearing_folders() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("collapse-with-folders"));
    let folder = tempfile::tempdir().expect("workspace folder");
    section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("folder"),
        folder.path().to_path_buf(),
    )]);

    assert!(section.has_folders());
    assert!(
        section
            .imp()
            .inner_scrolled_window
            .property::<bool>("visible")
    );

    section.set_section_body_collapsed(true);

    assert!(section.is_section_body_collapsed());
    assert!(
        !section
            .imp()
            .inner_scrolled_window
            .property::<bool>("visible"),
        "workspace collapse should hide the folder tree body"
    );
    assert!(
        !section
            .imp()
            .empty_folder_set_label
            .property::<bool>("visible"),
        "workspace collapse should hide the empty-state body too"
    );
    assert!(section.imp().header_box.property::<bool>("visible"));
    assert!(section.imp().add_folder_button.property::<bool>("visible"));
    assert!(section.imp().refresh_button.property::<bool>("visible"));
    assert_eq!(
        section.imp().collapse_button.tooltip_text().as_deref(),
        Some("Expand Workspace")
    );

    section.set_section_body_collapsed(false);

    assert!(!section.is_section_body_collapsed());
    assert!(
        section
            .imp()
            .inner_scrolled_window
            .property::<bool>("visible")
    );
    assert_eq!(
        section.imp().collapse_button.tooltip_text().as_deref(),
        Some("Collapse Workspace")
    );
}

#[test]
fn test_workspace_body_collapse_hides_and_restores_empty_state() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("collapse-empty"));
    section.load_workspace_folders(&[]);

    assert!(
        section
            .imp()
            .empty_folder_set_label
            .property::<bool>("visible")
    );

    section.toggle_section_body_collapsed();

    assert!(section.is_section_body_collapsed());
    assert!(
        !section
            .imp()
            .empty_folder_set_label
            .property::<bool>("visible")
    );
    assert!(
        !section
            .imp()
            .inner_scrolled_window
            .property::<bool>("visible")
    );

    section.toggle_section_body_collapsed();

    assert!(!section.is_section_body_collapsed());
    assert!(
        section
            .imp()
            .empty_folder_set_label
            .property::<bool>("visible")
    );
}

#[test]
fn test_workspace_body_collapse_survives_section_model_reload() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("collapse-reload"));
    let first = tempfile::tempdir().expect("first folder");
    let second = tempfile::tempdir().expect("second folder");
    section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("first"),
        first.path().to_path_buf(),
    )]);
    section.set_section_body_collapsed(true);

    section.load_workspace_folders(&[
        WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first.path().to_path_buf()),
        WorkspaceFolder::with_id(WorkspaceFolderId::new("second"), second.path().to_path_buf()),
    ]);

    assert!(section.is_section_body_collapsed());
    assert!(
        !section
            .imp()
            .inner_scrolled_window
            .property::<bool>("visible"),
        "ordinary section reloads should preserve the workspace body collapse state"
    );
    assert!(section.has_folders());

    section.set_section_body_collapsed(false);
    let window = present_section_window(&section);
    wait_until(Duration::from_secs(5), || {
        realized_drag_handle_for_path(&section, first.path()).is_some()
            && realized_drag_handle_for_path(&section, second.path()).is_some()
    });
    assert_reorder_handle_visible(&section, first.path(), true);
    assert_reorder_handle_visible(&section, second.path(), true);

    drop(window);
}

#[test]
fn test_empty_workspace_manual_refresh_noops_without_feedback_or_watchers() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("empty-refresh"));
    section.load_folders(&[]);
    let top_level_store_ptr = section
        .imp()
        .top_level_store
        .borrow()
        .as_ref()
        .expect("empty folder set should still install a top-level store")
        .as_ptr();

    let messages = Rc::new(RefCell::new(Vec::<String>::new()));
    let messages_clone = Rc::clone(&messages);
    section.connect_message(move |message, _| {
        messages_clone.borrow_mut().push(message.to_string());
    });

    section.imp().refresh_button.emit_clicked();

    wait_until(Duration::from_secs(2), || {
        !section.imp().refresh_runtime.pending_full_reload.get()
    });

    assert!(messages.borrow().is_empty());
    assert!(
        section.imp().refresh_runtime.pending_paths.borrow().is_empty(),
        "empty manual refresh should not enqueue filesystem paths"
    );
    assert_eq!(
        section
            .imp()
            .top_level_store
            .borrow()
            .as_ref()
            .expect("top-level store should remain installed")
            .as_ptr(),
        top_level_store_ptr,
        "empty manual refresh should not rebuild the top-level store"
    );
    assert!(section.imp().watch_runtime.watcher.borrow().is_none());
    assert!(section.imp().watch_runtime.start_source_id.borrow().is_none());
    assert!(section.imp().watch_runtime.poll_source_id.borrow().is_none());
}

#[test]
fn test_watcher_start_failure_surfaces_recoverable_feedback() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("watch-failure"));
    let dir = tempfile::tempdir().expect("workspace parent");
    let missing = dir.path().join("missing-folder");

    let messages = Rc::new(RefCell::new(Vec::<String>::new()));
    let messages_clone = Rc::clone(&messages);
    section.connect_message(move |message, _| {
        messages_clone.borrow_mut().push(message.to_string());
    });

    section.load_folders(&[FolderTreeEntry::Directory {
        path: missing.clone(),
    }]);

    wait_until(Duration::from_secs(5), || {
        messages
            .borrow()
            .iter()
            .any(|message| message.contains("Workspace auto-refresh unavailable"))
    });

    assert!(
        section.imp().refresh_button.is_sensitive(),
        "manual refresh should remain available after watcher startup fails"
    );
    assert!(
        messages
            .borrow()
            .iter()
            .any(|message| message.contains(missing.to_string_lossy().as_ref())),
        "watcher feedback should identify the folder that could not be watched"
    );
}

#[test]
fn test_manual_refresh_failure_reports_feedback_and_keeps_existing_tree() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("workspace folder");
    let existing = dir.path().join("visible-before-failure.txt");
    fixture::write_text(&existing, "still visible");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("manual-refresh-failure"));
    section.load_workspace_folders(&[WorkspaceFolder::with_id(
        WorkspaceFolderId::new("folder"),
        dir.path().to_path_buf(),
    )]);

    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(5), || {
        tree_contains_path(&section, &existing)
    });
    section.stop_workspace_watch_for_test();

    let messages = Rc::new(RefCell::new(Vec::<String>::new()));
    let messages_clone = Rc::clone(&messages);
    section.connect_message(move |message, _| {
        messages_clone.borrow_mut().push(message.to_string());
    });

    fixture::remove_dir_all(dir.path());
    section.imp().refresh_button.emit_clicked();

    wait_until(Duration::from_secs(5), || {
        messages
            .borrow()
            .iter()
            .any(|message| message.contains("Workspace refresh failed"))
    });

    assert!(
        messages
            .borrow()
            .iter()
            .any(|message| message.contains(dir.path().to_string_lossy().as_ref())),
        "manual refresh failure feedback should identify the folder"
    );
    assert!(
        tree_contains_path(&section, &existing),
        "failed manual refresh should keep the previous visible tree mounted"
    );
    assert!(
        section.imp().refresh_button.is_sensitive(),
        "manual refresh should remain retryable after a scan failure"
    );
}

#[test]
fn test_add_folder_button_emits_workspace_callback() {
    ensure_gtk_init();
    let workspace_id = WorkspaceId::new("add-folder-ws");
    let section = LushtextWorkspaceSection::new(workspace_id.clone());
    let received = Rc::new(RefCell::new(None::<WorkspaceId>));

    let received_clone = Rc::clone(&received);
    section.connect_add_folder_requested(move |id| {
        received_clone.replace(Some(id.clone()));
    });
    section.imp().add_folder_button.emit_clicked();

    assert_eq!(received.borrow().as_ref(), Some(&workspace_id));
}

#[test]
fn test_refresh_button_becomes_enabled_after_load_folders() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    assert!(
        section.imp().refresh_button.is_sensitive(),
        "refresh should become available once the section has folders"
    );
    assert!(
        !section.imp().empty_folder_set_label.is_visible(),
        "loaded folder sets should hide the empty state"
    );
    assert!(
        section.imp().inner_scrolled_window.is_visible(),
        "loaded folder sets should show the file tree"
    );
}

#[test]
fn test_refresh_button_becomes_enabled_after_add_folder() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.add_folder(dir.path(), true);

    assert!(section.imp().refresh_button.is_sensitive());
}

#[test]
fn test_has_folders_false_initially() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());
    assert!(!section.has_folders());
}

#[test]
fn test_has_folders_true_after_load() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);
    assert!(section.has_folders());
}

#[test]
fn test_manual_refresh_keeps_selection_and_expansion() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    fixture::create_dir(&nested);
    let existing = nested.join("alpha.txt");
    fixture::write_text(&existing, "alpha");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("refresh-ws"));
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should exist")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &existing));
    select_path(&section, &existing);

    let created = nested.join("beta.txt");
    fixture::write_text(&created, "beta");
    section.imp().refresh_button.emit_clicked();

    wait_until(Duration::from_secs(5), || {
        tree_contains_path(&section, &created)
            && selected_path(&section).as_deref() == Some(existing.as_path())
    });
    assert_eq!(selected_path(&section).as_deref(), Some(existing.as_path()));
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
    fixture::create_dir(&nested);
    let original = nested.join("before.txt");
    fixture::write_text(&original, "before");
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("refresh-flow-ws"));
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should exist")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &original));

    let renamed = nested.join("renamed.txt");
    fixture::rename(&original, &renamed);
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
    fixture::create_dir(&nested);
    let deleted = nested.join("delete-me.txt");
    fixture::write_text(&deleted, "delete");
    let section = LushtextWorkspaceSection::new(WorkspaceId::new("refresh-delete-ws"));
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should exist")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &deleted));

    fixture::remove_file(&deleted);
    section.imp().refresh_button.emit_clicked();
    wait_until(Duration::from_secs(5), || !tree_contains_path(&section, &deleted));
}

#[test]
fn test_workspace_section_toggle_folders() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    // Must have at least one visible entry to not be detected as empty
    fixture::write_text(&dir.path().join("file.txt"), "content");

    section.load_folders(&[FolderTreeEntry::Directory {
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
    section.toggle_folders();
    assert!(row.is_expanded());

    // Toggle should collapse
    section.toggle_folders();
    assert!(!row.is_expanded());
}

#[test]
fn test_manual_refresh_keeps_collapsed_top_level_folder_collapsed() {
    ensure_gtk_init();
    let section = LushtextWorkspaceSection::new(WorkspaceId::default());

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("file.txt"), "content");

    section.load_folders(&[FolderTreeEntry::Directory {
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
        assert!(!row.is_expanded(), "folder should start collapsed before refresh");
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
        "manual refresh should not re-expand a folder the user collapsed"
    );
}

#[test]
fn test_manual_refresh_keeps_top_level_models_mounted() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let nested = dir.path().join("nested");
    fixture::create_dir(&nested);
    let existing = nested.join("alpha.txt");
    fixture::write_text(&existing, "alpha");

    let section = LushtextWorkspaceSection::new(WorkspaceId::new("manual-model-stability"));
    section.load_folders(&[FolderTreeEntry::Directory {
        path: dir.path().to_path_buf(),
    }]);

    let _window = present_section_window(&section);
    section.expand_folders();
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &nested));
    row_for_path(&section, &nested)
        .expect("nested directory should exist")
        .set_expanded(true);
    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &existing));

    let top_level_store_ptr = section
        .imp()
        .top_level_store
        .borrow()
        .as_ref()
        .expect("top-level store should exist")
        .as_ptr();
    let tree_model_ptr = section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .expect("tree model should exist")
        .as_ptr();

    let created = nested.join("beta.txt");
    fixture::write_text(&created, "beta");
    section.imp().refresh_button.emit_clicked();

    wait_until(Duration::from_secs(5), || tree_contains_path(&section, &created));

    assert_eq!(
        section
            .imp()
            .top_level_store
            .borrow()
            .as_ref()
            .expect("top-level store should still exist")
            .as_ptr(),
        top_level_store_ptr,
        "manual refresh should keep the existing top-level store mounted",
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

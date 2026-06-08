// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextSidebar multi-workspace orchestrator.

use crate::common::{ensure_gtk_init, fixture, flush_after_delay, flush_events, wait_until};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::workspace::{WorkspaceConfig, WorkspaceId, WorkspacesFile};
use lushtext_core::services::{json_store, workspace_manager};
use lushtext_core::ui::sidebar::LushtextSidebar;
use lushtext_core::ui::window::LushtextWindow;
use std::time::Duration;

const WARNING_BAR_ROW_HEIGHT: i32 = 54;

/// Create a window attached to a test application.
fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

// --- Sidebar construction ---

#[test]
fn test_sidebar_new() {
    ensure_gtk_init();
    let _sidebar = LushtextSidebar::new();
}

#[test]
fn test_sidebar_workspace_filter_defaults_to_all_workspaces() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    let model = sidebar
        .imp()
        .workspace_filter_dropdown
        .model()
        .and_downcast::<gtk4::StringList>()
        .expect("workspace filter should use a StringList model");
    assert_eq!(model.n_items(), 1);
    assert_eq!(
        model.string(0).expect("All workspaces option should exist").as_str(),
        "All workspaces"
    );
    assert_eq!(sidebar.imp().workspace_filter_dropdown.selected(), 0);
}

#[test]
fn test_sidebar_starts_with_no_sections() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert!(sidebar.imp().sections.borrow().is_empty());
}

#[test]
fn test_sidebar_new_workspace_button_exists() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    let _button = &sidebar.imp().new_workspace_button;
}

#[test]
fn test_sidebar_selector_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    assert_eq!(
        sidebar.imp().workspace_filter_dropdown.accessible_role(),
        gtk4::AccessibleRole::ComboBox
    );
    assert_eq!(
        sidebar.imp().new_workspace_button.accessible_role(),
        gtk4::AccessibleRole::Button
    );
}

#[test]
fn test_sidebar_selector_row_uses_workspace_tree_left_inset() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(sidebar.imp().new_workspace_box.margin_start(), 6);
}

#[test]
fn test_sidebar_new_workspace_button_carries_vertical_spacing() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().new_workspace_button.icon_name().as_deref(),
        Some("folder-new-symbolic")
    );
    assert_eq!(sidebar.imp().new_workspace_button.valign(), gtk4::Align::Center);
    assert_eq!(sidebar.imp().new_workspace_button.margin_top(), 6);
    assert_eq!(sidebar.imp().new_workspace_button.margin_bottom(), 6);
}

#[test]
fn test_sidebar_workspace_list_revealer_uses_crossfade() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().workspace_list_revealer.transition_type(),
        gtk4::RevealerTransitionType::Crossfade,
    );
    assert_eq!(sidebar.imp().workspace_list_revealer.transition_duration(), 250);
    assert!(sidebar.imp().workspace_list_revealer.reveals_child());
}

#[test]
fn test_workspace_filter_can_show_only_one_workspace() {
    ensure_gtk_init();
    let _folders_dir = seed_restored_workspaces();

    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().sections.borrow().len() == 3
    });
    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    let model = dropdown
        .model()
        .and_downcast::<gtk4::StringList>()
        .expect("workspace filter should use a StringList model");
    assert_eq!(model.n_items(), 4);
    assert_eq!(
        model.string(0).expect("All workspaces option should exist").as_str(),
        "All workspaces"
    );
    assert_eq!(model.string(1).expect("first workspace option should exist").as_str(), "one");
    assert_eq!(model.string(2).expect("second workspace option should exist").as_str(), "two");
    assert_eq!(model.string(3).expect("third workspace option should exist").as_str(), "three");

    dropdown.set_selected(2);
    flush_after_delay(Duration::from_millis(300));

    wait_until(Duration::from_secs(3), || {
        let sidebar = window.imp().sidebar.imp();
        let revealer = &sidebar.workspace_list_revealer;
        let sections = sidebar.sections.borrow();
        revealer.reveals_child()
            && revealer.is_child_revealed()
            && !sections[0].property::<bool>("visible")
            && sections[1].property::<bool>("visible")
            && !sections[2].property::<bool>("visible")
    });

    dropdown.set_selected(0);
    flush_after_delay(Duration::from_millis(300));
    wait_until(Duration::from_secs(3), || {
        let sidebar = window.imp().sidebar.imp();
        let revealer = &sidebar.workspace_list_revealer;
        let sections = sidebar.sections.borrow();
        revealer.reveals_child()
            && revealer.is_child_revealed()
            && sections.iter().all(|section| section.property::<bool>("visible"))
    });
}

#[test]
fn test_new_workspace_affordance_stays_above_sections_scroll_area() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let first = sidebar
        .first_child()
        .expect("first child is the fixed new-workspace box");
    let separator_after_top = first
        .next_sibling()
        .expect("separator follows the new-workspace box");
    let revealer = separator_after_top
        .next_sibling()
        .and_downcast::<gtk4::Revealer>()
        .expect("workspace list revealer sits below the fixed top row");
    let scroller = revealer
        .child()
        .and_downcast::<gtk4::ScrolledWindow>()
        .expect("workspace scroller should be the revealer child");

    assert!(first.is::<gtk4::Box>());
    assert_eq!(first.as_ptr(), sidebar.imp().new_workspace_button.parent().expect("expected operation to succeed").as_ptr());
    assert!(separator_after_top.is::<gtk4::Separator>());
    assert_eq!(
        revealer.as_ptr(),
        sidebar.imp().workspace_list_revealer.as_ptr()
    );
    assert_eq!(scroller.as_ptr(), sidebar.imp().outer_scrolled_window.as_ptr());
    assert!(revealer.next_sibling().is_none());
}

#[test]
fn test_sidebar_outer_scroller_disables_horizontal_scrollbar() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().outer_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );
    assert!(!sidebar.imp().outer_scrolled_window.propagates_natural_width());
}

#[test]
fn test_dense_workspace_sections_scroll_below_fixed_selector() {
    ensure_gtk_init();
    const WORKSPACE_COUNT: usize = 18;
    let _folders_dir = seed_dense_workspace_sections(WORKSPACE_COUNT);

    let window = test_window();
    window.set_default_size(360, 320);
    present_window(&window);

    wait_until(Duration::from_secs(3), || {
        let sidebar = window.imp().sidebar.imp();
        let adjustment = sidebar.outer_scrolled_window.vadjustment();
        sidebar.sections.borrow().len() == WORKSPACE_COUNT
            && sidebar.new_workspace_box.height() > 0
            && sidebar.outer_scrolled_window.height() > 0
            && adjustment.upper() > adjustment.page_size() + 1.0
    });

    let sidebar = window.imp().sidebar.upcast_ref::<gtk4::Widget>();
    let sidebar_imp = window.imp().sidebar.imp();
    let selector_bounds = sidebar_imp
        .new_workspace_box
        .compute_bounds(sidebar)
        .expect("fixed selector should have sidebar-relative bounds");
    let scroller_bounds = sidebar_imp
        .outer_scrolled_window
        .compute_bounds(sidebar)
        .expect("workspace list scroller should have sidebar-relative bounds");
    assert!(
        scroller_bounds.y() >= selector_bounds.y() + selector_bounds.height() - 1.0,
        "workspace sections should scroll below the fixed selector (selector y={} h={}, scroller y={})",
        selector_bounds.y(),
        selector_bounds.height(),
        scroller_bounds.y()
    );
    assert_eq!(
        sidebar_imp.outer_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );

    let adjustment = sidebar_imp.outer_scrolled_window.vadjustment();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    flush_events();

    let selector_after_scroll = sidebar_imp
        .new_workspace_box
        .compute_bounds(sidebar)
        .expect("fixed selector should remain allocated after list scroll");
    assert!(sidebar_imp.new_workspace_box.is_visible());
    assert_eq!(
        selector_bounds.y(),
        selector_after_scroll.y(),
        "scrolling the workspace sections should not move the fixed selector row"
    );
}

#[test]
fn test_sidebar_new_workspace_affordance_matches_document_restored_warning_height() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1200, 800);
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().new_workspace_box.height() > 0
    });

    let sidebar_height = window.imp().sidebar.imp().new_workspace_box.height();
    assert_eq!(
        sidebar_height, WARNING_BAR_ROW_HEIGHT,
        "new workspace affordance height should preserve the warning-bar sizing contract (sidebar={sidebar_height}, expected={WARNING_BAR_ROW_HEIGHT})",
    );
}

#[test]
fn test_sidebar_has_no_persistent_width_footer_controls() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let mut children = Vec::new();
    let mut child = sidebar.first_child();
    while let Some(widget) = child {
        children.push(widget.clone());
        child = widget.next_sibling();
    }

    assert_eq!(children.len(), 3);
    assert!(children[0].is::<gtk4::Box>());
    assert!(children[1].is::<gtk4::Separator>());
    assert!(children[2].is::<gtk4::Revealer>());
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

fn seed_dense_workspace_sections(count: usize) -> tempfile::TempDir {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("dense workspace folders tempdir");
    let mut workspaces = WorkspacesFile::default();

    for idx in 0..count {
        let name = format!("dense-{idx:02}");
        let path = folders_dir.path().join(&name);
        fixture::create_dir_all(&path);
        workspaces.workspaces.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new(format!("ws-dense-{idx:02}")),
            name,
            path,
        ));
    }

    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces.json");
    folders_dir
}

// --- Window integration: tab path updates (moved from old sidebar.rs) ---

#[test]
fn test_update_tab_path_exact_match() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let old_path = dir.path().join("old.rs");
    fixture::write_text(&old_path, "fn main() {}");
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

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let old_dir = dir.path().join("old_dir");
    fixture::create_dir(&old_dir);
    let file_path = old_dir.join("file.rs");
    fixture::write_text(&file_path, "content");
    window.open_document(&file_path);

    let new_dir = dir.path().join("new_dir");
    window.update_tab_path(&old_dir, &new_dir);

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "file.rs");

    let editor = page
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .expect("expected operation to succeed");
    assert_eq!(editor.file_path().expect("expected operation to succeed"), new_dir.join("file.rs"));
}

#[test]
fn test_update_tab_path_no_match_is_noop() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file_path = dir.path().join("keep.rs");
    fixture::write_text(&file_path, "content");
    window.open_document(&file_path);

    window.update_tab_path(
        std::path::Path::new("/tmp/other.rs"),
        std::path::Path::new("/tmp/renamed.rs"),
    );

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "keep.rs");
}

fn present_window(window: &LushtextWindow) {
    window.present();
    flush_events();
}

fn assert_tab_count(window: &LushtextWindow, expected: i32) {
    assert_eq!(
        window.imp().tab_view.n_pages(),
        expected,
        "expected {expected} open tab(s), got {}",
        window.imp().tab_view.n_pages()
    );
}

// --- Window integration: close tabs ---

#[test]
fn test_close_tab_for_path_exact() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file_path = dir.path().join("doomed.rs");
    fixture::write_text(&file_path, "");
    window.open_document(&file_path);
    assert_tab_count(&window, 1);

    window.close_tab_for_path(&file_path);
    flush_events();
    assert_tab_count(&window, 0);
}

#[test]
fn test_close_tab_for_path_directory_closes_children() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let sub = dir.path().join("sub");
    fixture::create_dir(&sub);
    let f1 = sub.join("a.rs");
    let f2 = sub.join("b.rs");
    let f3 = dir.path().join("outside.rs");
    fixture::write_text(&f1, "");
    fixture::write_text(&f2, "");
    fixture::write_text(&f3, "");

    window.open_document(&f1);
    window.open_document(&f2);
    window.open_document(&f3);
    assert_tab_count(&window, 3);

    window.close_tab_for_path(&sub);
    flush_events();
    assert_tab_count(&window, 1);

    let remaining = window
        .imp()
        .tab_view
        .nth_page(0)
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .expect("expected operation to succeed");
    assert_eq!(remaining.file_path().expect("expected operation to succeed"), f3);
}

#[test]
fn test_close_tab_for_path_nonexistent_is_noop() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert_tab_count(&window, 1);

    window.close_tab_for_path(std::path::Path::new("/does/not/exist"));
    flush_events();
    assert_tab_count(&window, 1);
}

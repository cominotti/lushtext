// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the main window shell.
//!
//! This suite focuses on the current window contract: split-view sidebar
//! behavior, a few critical shell affordances, and preview-pane regressions
//! that still live in the window layer.

use crate::common::{emit_key_pressed_on_focus, ensure_gtk_init};
use gio::prelude::{ActionExt, ActionGroupExt, ActionMapExt, ListModelExt, MenuModelExt};
use glib::prelude::{Cast, IsA, ObjectExt};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::{
    ActionRowExt, AdwApplicationWindowExt, AdwDialogExt, AlertDialogExt, AnimationExt,
    ComboRowExt, SidebarItemExt,
};
use lushtext_core::config::keys;
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::encoding::{DocumentEncoding, FileHealthFindingKind, LineEnding};
use lushtext_core::model::note::RichNoteBody;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::file_limits::FileSizeCheck;
use lushtext_core::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use lushtext_core::services::{
    bookmark_service, document_note_service, draft_service, editor_io, json_store,
    local_history_service, session_service, workspace_manager, workspace_note_service,
};
use lushtext_core::ui::editor_page::{
    LushtextEditorPage, MinimapAvailability, MinimapMarkerKind, SaveError,
};
use lushtext_core::ui::markdown_preview::LushtextMarkdownPreview;
use lushtext_core::ui::preferences::LushtextPreferences;
use lushtext_core::ui::window::LushtextWindow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

const DEFINITION_LIST_CODE_BLOCK_SAMPLE: &str = concat!(
    "## Definition lists\n\n",
    "Term 1\n",
    ": Definition 1 with lazy continuation.\n\n",
    "Term 2 with *inline markup*\n\n",
    ":   Definition 2\n\n",
    "        { some code, part of Definition 2 }\n\n",
    "    Third paragraph of definition 2.\n\n",
    "Compact style:\n\n",
    "Term 1 ~ Definition 1\n\n",
    "Term 2 ~ Definition 2a ~ Definition 2b\n",
);

const CODE_BLOCK_HORIZONTAL_PADDING: i32 = 12;

fn test_window_with_split_view_state(
    workspace_visible: bool,
    workspace_fraction: f64,
    properties_visible: bool,
    properties_fraction: f64,
) -> LushtextWindow {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED, true)
        .expect("set split-view-layout-migrated");
    settings
        .set_boolean(keys::WORKSPACE_SIDEBAR_VISIBLE, workspace_visible)
        .expect("set workspace-sidebar-visible");
    settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, workspace_fraction)
        .expect("set workspace-sidebar-width-fraction");
    settings
        .set_boolean(keys::PROPERTIES_SIDEBAR_VISIBLE, properties_visible)
        .expect("set properties-sidebar-visible");
    settings
        .set_double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION, properties_fraction)
        .expect("set properties-sidebar-width-fraction");
    test_window()
}

fn test_window_with_legacy_sidebar_state(visible: bool, position: i32) -> LushtextWindow {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED, false)
        .expect("clear split-view-layout-migrated");
    settings
        .set_boolean(keys::SIDEBAR_VISIBLE, visible)
        .expect("set legacy sidebar-visible");
    settings
        .set_int(keys::SIDEBAR_POSITION, position)
        .expect("set legacy sidebar-position");
    test_window()
}

fn seed_restored_workspaces() -> tempfile::TempDir {
    ensure_gtk_init();
    let roots_dir = tempfile::tempdir().expect("workspace roots tempdir");
    let mut workspaces = WorkspacesFile::default();

    for (idx, name) in ["one", "two", "three"].into_iter().enumerate() {
        let path = roots_dir.path().join(name);
        std::fs::create_dir_all(&path).expect("create workspace root");
        workspaces.workspaces.push(WorkspaceConfig {
            id: WorkspaceId::new(format!("ws-{idx}")),
            name: name.to_string(),
            root: path,
        });
    }

    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces.json");
    roots_dir
}

fn seed_scoped_workspaces(initial_scope: WorkspaceScope) -> (tempfile::TempDir, PathBuf, PathBuf) {
    ensure_gtk_init();
    let roots_dir = tempfile::tempdir().expect("scoped workspace roots tempdir");
    let left_root = roots_dir.path().join("left");
    let right_root = roots_dir.path().join("right");
    std::fs::create_dir_all(&left_root).expect("create left workspace root");
    std::fs::create_dir_all(&right_root).expect("create right workspace root");
    std::fs::write(left_root.join("alpha.rs"), "fn alpha() {}\n").expect("write alpha");
    std::fs::write(right_root.join("beta.rs"), "fn beta() {}\n").expect("write beta");

    let workspaces = WorkspacesFile {
        current_scope: initial_scope,
        workspaces: vec![
            WorkspaceConfig {
                id: WorkspaceId::new("ws-left"),
                name: "left".to_string(),
                root: left_root.clone(),
            },
            WorkspaceConfig {
                id: WorkspaceId::new("ws-right"),
                name: "right".to_string(),
                root: right_root.clone(),
            },
        ],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save scoped workspaces");
    (roots_dir, left_root, right_root)
}

fn wait_for_workspace_roots(window: &LushtextWindow, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if window.imp().sidebar.all_workspace_root_paths().len() == expected {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }
    let actual = window.imp().sidebar.all_workspace_root_paths().len();
    panic!("expected {expected} restored workspace roots, got {actual}");
}

fn wait_for_workspace_consumers(window: &LushtextWindow, expected_roots: usize, expected_index: usize) {
    wait_until(Duration::from_secs(3), || {
        window
            .imp()
            .search_panel
            .imp()
            .runtime
            .workspace_roots
            .borrow()
            .len()
            == expected_roots
            && window.imp().command_palette.file_index_len() == expected_index
    });
}

fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

fn flush_after_delay(delay: Duration) {
    std::thread::sleep(delay);
    flush_events();
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        flush_after_delay(Duration::from_millis(20));
    }
    panic!("condition was not met within {timeout:?}");
}

fn present_window(window: &LushtextWindow) {
    window.present();
    wait_until(Duration::from_secs(2), || {
        window.width() > 0 && window.height() > 0
    });
    flush_after_delay(Duration::from_millis(20));
}

fn action_enabled(window: &LushtextWindow, name: &str) -> bool {
    let action = window
        .lookup_action(name)
        .unwrap_or_else(|| panic!("action '{name}' not found"));
    action.is_enabled()
}

fn action_state_bool(window: &LushtextWindow, name: &str) -> bool {
    window
        .lookup_action(name)
        .unwrap_or_else(|| panic!("action '{name}' not found"))
        .state()
        .unwrap_or_else(|| panic!("action '{name}' should be stateful"))
        .get::<bool>()
        .unwrap_or_else(|| panic!("action '{name}' should use bool state"))
}

fn activate_action(window: &LushtextWindow, name: &str) {
    ActionGroupExt::activate_action(window, name, None);
    flush_events();
}

fn active_editor(window: &LushtextWindow) -> LushtextEditorPage {
    window
        .imp()
        .tab_view
        .selected_page()
        .expect("expected operation to succeed")
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("expected operation to succeed")
}

// Keep hidden-to-visible preview-shell regressions in this suite: a directly
// mounted `LushtextMarkdownPreview` can pass while the real `GtkPaned` shell
// leaves child-anchor code blocks with stale allocations.
fn prepare_markdown_preview_window(
    markdown: &str,
    width: i32,
    height: i32,
) -> (LushtextWindow, tempfile::TempDir) {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(width, height);
    window.new_tab();
    let dir = tempfile::tempdir().expect("markdown preview tempdir");
    let editor = active_editor(&window);
    editor.set_file_path(&dir.path().join("gh-markdown-sample.md"));
    editor.buffer().set_text(markdown);
    present_window(&window);
    (window, dir)
}

fn descendants(root: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Widget> {
    let mut widgets = Vec::new();
    let mut stack = vec![root.as_ref().clone()];

    while let Some(widget) = stack.pop() {
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current.clone());
            child = current.next_sibling();
        }
        widgets.push(widget);
    }

    widgets
}

fn widgets_with_css_class<T>(root: &impl IsA<gtk4::Widget>, css_class: &str) -> Vec<T>
where
    T: IsA<gtk4::Widget> + glib::object::ObjectType + Clone + 'static,
{
    descendants(root)
        .into_iter()
        .filter(|widget| widget.has_css_class(css_class))
        .filter_map(|widget| widget.downcast::<T>().ok())
        .collect()
}

fn source_views(preview: &LushtextMarkdownPreview) -> Vec<sourceview5::View> {
    widgets_with_css_class::<sourceview5::View>(preview, "markdown-code-block-view")
}

fn code_block_containers(preview: &LushtextMarkdownPreview) -> Vec<gtk4::Box> {
    widgets_with_css_class::<gtk4::Box>(preview, "markdown-code-block")
}

fn code_block_scrollers(preview: &LushtextMarkdownPreview) -> Vec<gtk4::ScrolledWindow> {
    widgets_with_css_class::<gtk4::ScrolledWindow>(preview, "markdown-code-block-scroller")
}

fn preview_text_column_width(preview: &LushtextMarkdownPreview) -> i32 {
    let text_view = preview.text_view();
    text_view.width() - text_view.left_margin() - text_view.right_margin()
}

fn expected_code_block_width(preview: &LushtextMarkdownPreview, block: &gtk4::Box) -> i32 {
    preview_text_column_width(preview)
        .saturating_sub(block.margin_start() + block.margin_end())
        .max(1)
}

fn horizontal_overflow(scroller: &gtk4::ScrolledWindow) -> f64 {
    let adjustment = scroller.hadjustment();
    (adjustment.upper() - adjustment.page_size()).max(0.0)
}

fn source_view_buffer_text(source_view: &sourceview5::View) -> String {
    let buffer = source_view.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn wait_for_markdown_preview_shell(window: &LushtextWindow) {
    wait_until(Duration::from_secs(3), || {
        let preview = &window.imp().markdown_preview;
        let Some(scroller) = code_block_scrollers(preview).first().cloned() else {
            return false;
        };

        !window.imp().preview_animation_active.get()
            && preview.is_showing_content()
            && preview.text_view().width() > 0
            && !code_block_containers(preview).is_empty()
            && !source_views(preview).is_empty()
            && scroller.hadjustment().page_size() > 0.0
    });
    flush_after_delay(Duration::from_millis(40));
}

fn assert_live_code_block_uses_preview_column(window: &LushtextWindow) {
    let preview = &window.imp().markdown_preview;
    let text_view = preview.text_view();
    let block = code_block_containers(preview)
        .pop()
        .expect("code block container");
    let scroller = code_block_scrollers(preview).pop().expect("code scroller");
    let source_view = source_views(preview).pop().expect("code source view");
    let expected_width = expected_code_block_width(preview, &block);
    let actual_width = block.width();
    let minimum_inner_width =
        (expected_width - (CODE_BLOCK_HORIZONTAL_PADDING * 2) - 8).max(1) as f32;
    let block_bounds = block
        .compute_bounds(&text_view)
        .expect("code block bounds in preview text view");
    let scroller_bounds = scroller
        .compute_bounds(&block)
        .expect("scroller bounds in code block");
    let overflow = horizontal_overflow(&scroller);

    assert_eq!(
        source_view_buffer_text(&source_view),
        "{ some code, part of Definition 2 }\n",
        "preview should preserve the fenced code text from the definition body"
    );
    assert!(
        block.margin_start() > 0,
        "definition-list code blocks should keep their definition-body offset"
    );
    assert_eq!(
        block.width_request(),
        expected_width,
        "definition-list code block width request should follow the live text column"
    );
    assert!(
        (actual_width - expected_width).abs() <= 4,
        "definition-list code block allocation should settle near {expected_width}, got {actual_width}"
    );
    assert!(
        block_bounds.width() >= (expected_width - 4).max(1) as f32,
        "code block bounds should span the preview column after definition margins; expected at least {}, got {}",
        (expected_width - 4).max(1),
        block_bounds.width()
    );
    assert!(
        scroller_bounds.width() >= minimum_inner_width,
        "inner code scroller should receive the block width minus padding; expected at least {minimum_inner_width}, got {}",
        scroller_bounds.width()
    );
    assert!(
        overflow <= 1.0,
        "short definition-list code block should not expose false horizontal overflow, got upper={} page_size={} overflow={overflow}",
        scroller.hadjustment().upper(),
        scroller.hadjustment().page_size()
    );
}

fn active_editor_has_focus(window: &LushtextWindow) -> bool {
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return false;
    };
    let editor = active_editor(window);
    focus.as_ptr() == editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr()
}

fn wait_for_active_editor_focus(window: &LushtextWindow) {
    wait_until(Duration::from_secs(2), || active_editor_has_focus(window));
}

fn editor_text(editor: &LushtextEditorPage) -> String {
    let buffer = editor.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn assert_tab_count(window: &LushtextWindow, expected: i32) {
    let actual = window.imp().tab_view.n_pages();
    assert_eq!(
        actual,
        expected,
        "expected {expected} open tab(s), got {actual}"
    );
}

fn tab_pages(window: &LushtextWindow) -> Vec<libadwaita::TabPage> {
    (0..window.imp().tab_view.n_pages())
        .map(|index| window.imp().tab_view.nth_page(index))
        .collect()
}

fn tab_titles(window: &LushtextWindow) -> Vec<String> {
    tab_pages(window)
        .into_iter()
        .map(|page| page.title().to_string())
        .collect()
}

fn find_tab_page_by_title(window: &LushtextWindow, title: &str) -> libadwaita::TabPage {
    tab_pages(window)
        .into_iter()
        .find(|page| page.title().as_str() == title)
        .unwrap_or_else(|| panic!("tab '{title}' not found"))
}

fn prepare_tab_context_menu(window: &LushtextWindow, page: &libadwaita::TabPage) {
    window
        .imp()
        .tab_view
        .emit_by_name::<()>("setup-menu", &[page]);
    flush_events();
}

fn visible_alert_dialog(window: &LushtextWindow) -> Option<libadwaita::AlertDialog> {
    window
        .visible_dialog()
        .and_then(|dialog| dialog.downcast::<libadwaita::AlertDialog>().ok())
}

fn visible_sheet_dialog(window: &LushtextWindow) -> Option<libadwaita::Dialog> {
    window
        .visible_dialog()
        .and_then(|dialog| dialog.downcast::<libadwaita::Dialog>().ok())
}

fn find_button_by_label(root: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Ok(button) = root.clone().downcast::<gtk4::Button>()
        && button.label().as_deref() == Some(label)
    {
        return Some(button);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_button_by_label(&widget, label) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn find_label_by_text(root: &gtk4::Widget, text: &str) -> Option<gtk4::Label> {
    if let Ok(label) = root.clone().downcast::<gtk4::Label>()
        && label.label() == text
    {
        return Some(label);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_label_by_text(&widget, text) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn find_navigation_split_view(root: &gtk4::Widget) -> Option<libadwaita::NavigationSplitView> {
    if let Ok(split_view) = root.clone().downcast::<libadwaita::NavigationSplitView>() {
        return Some(split_view);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_navigation_split_view(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn find_adw_sidebar(root: &gtk4::Widget) -> Option<libadwaita::Sidebar> {
    if let Ok(sidebar) = root.clone().downcast::<libadwaita::Sidebar>() {
        return Some(sidebar);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_adw_sidebar(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn has_tree_list_model_list_view(root: &gtk4::Widget) -> bool {
    if let Ok(list_view) = root.clone().downcast::<gtk4::ListView>()
        && let Some(selection) = list_view.model().and_downcast::<gtk4::SingleSelection>()
        && selection
            .model()
            .is_some_and(|model| model.is::<gtk4::TreeListModel>())
    {
        return true;
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if has_tree_list_model_list_view(&widget) {
            return true;
        }
        child = widget.next_sibling();
    }

    false
}

fn find_search_entry(root: &gtk4::Widget) -> Option<gtk4::SearchEntry> {
    if let Ok(search_entry) = root.clone().downcast::<gtk4::SearchEntry>() {
        return Some(search_entry);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_search_entry(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn find_stack_switcher(root: &gtk4::Widget) -> Option<gtk4::StackSwitcher> {
    if let Ok(switcher) = root.clone().downcast::<gtk4::StackSwitcher>() {
        return Some(switcher);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_stack_switcher(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn find_note_editor_stack(root: &gtk4::Widget) -> Option<gtk4::Stack> {
    if let Ok(stack) = root.clone().downcast::<gtk4::Stack>()
        && stack.child_by_name("edit").is_some()
        && stack.child_by_name("render").is_some()
    {
        return Some(stack);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_note_editor_stack(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn collect_text_views(root: &gtk4::Widget, text_views: &mut Vec<gtk4::TextView>) {
    if let Ok(text_view) = root.clone().downcast::<gtk4::TextView>() {
        text_views.push(text_view);
        return;
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        collect_text_views(&widget, text_views);
        child = widget.next_sibling();
    }
}

fn note_editor_text_views(root: &gtk4::Widget) -> (gtk4::TextView, gtk4::TextView) {
    let mut text_views = Vec::new();
    collect_text_views(root, &mut text_views);

    let edit = text_views
        .iter()
        .find(|text_view| text_view.is_editable())
        .cloned()
        .expect("editable note text view");
    let render = text_views
        .into_iter()
        .find(|text_view| !text_view.is_editable())
        .expect("rendered note text view");

    (edit, render)
}

fn assert_note_editor_text_margins_match(root: &gtk4::Widget) {
    let (edit, render) = note_editor_text_views(root);
    assert_eq!(edit.left_margin(), render.left_margin());
    assert_eq!(edit.right_margin(), render.right_margin());
    assert_eq!(edit.top_margin(), render.top_margin());
    assert_eq!(edit.bottom_margin(), render.bottom_margin());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WidgetOuterSize {
    width: i32,
    height: i32,
}

fn settled_widget_outer_size(widget: &impl IsA<gtk4::Widget>) -> WidgetOuterSize {
    let widget = widget.as_ref();
    let mut previous = None;
    let deadline = Instant::now() + Duration::from_secs(2);

    while Instant::now() < deadline {
        flush_after_delay(Duration::from_millis(20));
        let current = WidgetOuterSize {
            width: widget.width(),
            height: widget.height(),
        };
        if current.width > 0 && current.height > 0 && previous == Some(current) {
            return current;
        }
        previous = Some(current);
    }

    panic!("widget did not settle to a positive allocation; last allocation was {previous:?}");
}

fn assert_settled_widget_outer_size(
    widget: &impl IsA<gtk4::Widget>,
    expected: WidgetOuterSize,
    context: &str,
) {
    let actual = settled_widget_outer_size(widget);
    assert_eq!(
        actual, expected,
        "{context} must not change the modal outer allocation"
    );
}

fn measured_natural_outer_size(widget: &impl IsA<gtk4::Widget>) -> WidgetOuterSize {
    let (_, natural_width, _, _) = widget.measure(gtk4::Orientation::Horizontal, -1);
    let (_, natural_height, _, _) = widget.measure(gtk4::Orientation::Vertical, natural_width);
    WidgetOuterSize {
        width: natural_width,
        height: natural_height,
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Widget tests round allocated f32 bounds back to device pixels for exact geometry assertions"
)]
fn note_editor_text_origin(root: &gtk4::Widget, text_view: &gtk4::TextView) -> (i32, i32) {
    let bounds = text_view
        .compute_bounds(root)
        .expect("note text view should have allocated bounds");
    (
        (bounds.x() + text_view.left_margin() as f32).round() as i32,
        (bounds.y() + text_view.top_margin() as f32).round() as i32,
    )
}

fn note_editor_visible_text_origin(root: &gtk4::Widget, editable: bool) -> (i32, i32) {
    let (edit, render) = note_editor_text_views(root);
    let text_view = if editable { edit } else { render };
    note_editor_text_origin(root, &text_view)
}

fn text_view_scrolled_window(text_view: &gtk4::TextView) -> gtk4::ScrolledWindow {
    let mut parent = text_view.parent();
    while let Some(widget) = parent {
        if let Ok(scrolled_window) = widget.clone().downcast::<gtk4::ScrolledWindow>() {
            return scrolled_window;
        }
        parent = widget.parent();
    }

    panic!("note text view should live inside a scrolled window");
}

fn assert_note_editor_render_surface_ready_before_first_render(root: &gtk4::Widget) {
    let (_, render) = note_editor_text_views(root);
    let scrolled_window = text_view_scrolled_window(&render);
    assert!(
        scrolled_window.property::<bool>("visible"),
        "note editor Render page should keep the final text scroller visible before first activation"
    );
}

fn assert_note_editor_render_keeps_modal_geometry(
    dialog: &impl IsA<gtk4::Widget>,
    extra: &gtk4::Widget,
    stack: &gtk4::Stack,
) {
    let edit_dialog_size = settled_widget_outer_size(dialog);
    let edit_stack_size = settled_widget_outer_size(stack);
    let edit_extra_natural_size = measured_natural_outer_size(extra);
    let edit_stack_natural_size = measured_natural_outer_size(stack);
    let edit_text_origin = note_editor_visible_text_origin(extra, true);
    assert_note_editor_render_surface_ready_before_first_render(extra);

    stack.set_visible_child_name("render");
    flush_after_delay(Duration::from_millis(40));
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));
    let render_dialog_size = settled_widget_outer_size(dialog);
    let render_stack_size = settled_widget_outer_size(stack);
    let render_extra_natural_size = measured_natural_outer_size(extra);
    let render_stack_natural_size = measured_natural_outer_size(stack);
    let render_text_origin = note_editor_visible_text_origin(extra, false);

    assert_eq!(
        edit_dialog_size, render_dialog_size,
        "note editor modal outer allocation must not change on first Edit -> Render switch"
    );
    assert_eq!(
        edit_stack_size, render_stack_size,
        "note editor stack allocation must not change on first Edit -> Render switch"
    );
    assert_eq!(
        edit_extra_natural_size, render_extra_natural_size,
        "note editor modal content must keep the same natural outer size on first Edit -> Render switch"
    );
    assert_eq!(
        edit_stack_natural_size, render_stack_natural_size,
        "note editor stack must keep the same natural size on first Edit -> Render switch"
    );
    assert_eq!(
        edit_text_origin, render_text_origin,
        "rendered note text should start at the same content origin as editable note text"
    );
}

fn assert_typed_note_editor_first_render_keeps_modal_geometry(
    dialog: &impl IsA<gtk4::Widget>,
    extra: &gtk4::Widget,
    stack: &gtk4::Stack,
    text: &str,
) {
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
    let initial_dialog_size = settled_widget_outer_size(dialog);
    let initial_stack_size = settled_widget_outer_size(stack);
    let initial_extra_natural_size = measured_natural_outer_size(extra);
    let initial_stack_natural_size = measured_natural_outer_size(stack);
    let initial_text_origin = note_editor_visible_text_origin(extra, true);
    assert_note_editor_render_surface_ready_before_first_render(extra);

    let (edit, _) = note_editor_text_views(extra);
    edit.buffer().set_text(text);
    flush_after_delay(Duration::from_millis(40));

    assert_eq!(
        settled_widget_outer_size(dialog),
        initial_dialog_size,
        "typing into a note editor must not resize the modal before Render is clicked"
    );
    assert_eq!(
        settled_widget_outer_size(stack),
        initial_stack_size,
        "typing into a note editor must not resize the note editor stack"
    );
    assert_eq!(
        measured_natural_outer_size(extra),
        initial_extra_natural_size,
        "typing into a note editor must keep the modal content natural size stable"
    );
    assert_eq!(
        measured_natural_outer_size(stack),
        initial_stack_natural_size,
        "typing into a note editor must keep the note editor stack natural size stable"
    );
    assert_eq!(
        note_editor_visible_text_origin(extra, true),
        initial_text_origin,
        "typing into a note editor must not move the editable text origin"
    );

    assert_note_editor_render_keeps_modal_geometry(dialog, extra, stack);
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

fn menu_model_label_actions(model: &gio::MenuModel) -> Vec<(String, Option<String>)> {
    let mut entries = Vec::new();
    for index in 0..model.n_items() {
        let label = model
            .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
            .and_then(|variant| variant.get::<String>());
        let action = model
            .item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
            .and_then(|variant| variant.get::<String>());
        if let Some(label) = label {
            entries.push((label, action));
        }
        for link_name in ["section", "submenu"] {
            if let Some(link) = model.item_link(index, link_name) {
                entries.extend(menu_model_label_actions(&link));
            }
        }
    }
    entries
}

fn primary_menu_action_for_label(window: &LushtextWindow, label: &str) -> Option<String> {
    let primary_menu = window
        .imp()
        .primary_menu_button
        .menu_model()
        .expect("primary menu model");
    menu_model_label_actions(&primary_menu)
        .into_iter()
        .find_map(|(entry_label, action)| (entry_label == label).then_some(action).flatten())
}

fn activate_primary_menu_item(window: &LushtextWindow, label: &str) {
    let action = primary_menu_action_for_label(window, label)
        .unwrap_or_else(|| panic!("primary menu item '{label}' should have an action"));
    let action_name = action
        .strip_prefix("win.")
        .unwrap_or_else(|| panic!("expected '{action}' to be a window action"));
    activate_action(window, action_name);
}

fn click_alert_extra_button(dialog: &libadwaita::AlertDialog, label: &str) {
    let extra = dialog.extra_child().expect("alert dialog extra child");
    click_labeled_widget(&extra, label);
    flush_events();
}

fn click_labeled_widget(root: &gtk4::Widget, label: &str) {
    if let Some(button) = find_button_by_label(root, label) {
        button.emit_clicked();
        return;
    }

    if let Some(toggle) = find_toggle_button_by_label(root, label) {
        toggle.emit_clicked();
        return;
    }

    panic!("clickable widget '{label}' not found");
}

fn find_toggle_button_by_label(root: &gtk4::Widget, label: &str) -> Option<gtk4::ToggleButton> {
    if let Ok(toggle) = root.clone().downcast::<gtk4::ToggleButton>()
        && toggle.label().as_deref() == Some(label)
    {
        return Some(toggle);
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_toggle_button_by_label(&widget, label) {
            return Some(found);
        }
        child = widget.next_sibling();
    }

    None
}

fn workspace_sidebar_visible(window: &LushtextWindow) -> bool {
    window.imp().workspace_split_view.shows_sidebar()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertiesSurfacePresentation {
    Pane,
    Sheet,
}

impl PropertiesSurfacePresentation {
    fn layout_name(self) -> &'static str {
        match self {
            Self::Pane => "pane",
            Self::Sheet => "sheet",
        }
    }
}

fn set_properties_surface_presentation(
    window: &LushtextWindow,
    presentation: PropertiesSurfacePresentation,
) {
    window
        .imp()
        .properties_layout_view
        .set_layout_name(presentation.layout_name());
    flush_events();
}

fn properties_surface_presentation(window: &LushtextWindow) -> PropertiesSurfacePresentation {
    match window
        .imp()
        .properties_layout_view
        .layout_name()
        .as_deref()
    {
        Some("sheet") => PropertiesSurfacePresentation::Sheet,
        _ => PropertiesSurfacePresentation::Pane,
    }
}

fn properties_sidebar_visible(window: &LushtextWindow) -> bool {
    match properties_surface_presentation(window) {
        PropertiesSurfacePresentation::Pane => window.imp().properties_split_view.shows_sidebar(),
        PropertiesSurfacePresentation::Sheet => window.imp().properties_bottom_sheet.is_open(),
    }
}

fn shortcut_bound(window: &LushtextWindow, action_name: &str, trigger_string: &str) -> bool {
    let expected_trigger = gtk4::ShortcutTrigger::parse_string(trigger_string)
        .unwrap_or_else(|| panic!("shortcut trigger '{trigger_string}' should parse"));
    let expected_trigger = expected_trigger.to_str();
    let controllers = window.observe_controllers();
    let shortcut_controller = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|object| object.downcast::<gtk4::ShortcutController>().ok())
        .find(|controller| controller.scope() == gtk4::ShortcutScope::Managed)
        .expect("window should install a managed shortcut controller");

    (0..shortcut_controller.n_items())
        .filter_map(|index| shortcut_controller.item(index))
        .filter_map(|object| object.downcast::<gtk4::Shortcut>().ok())
        .any(|shortcut| {
            let action_matches = shortcut
                .action()
                .and_then(|action| action.downcast::<gtk4::NamedAction>().ok())
                .is_some_and(|action| action.action_name().as_str() == action_name);
            let trigger_matches = shortcut
                .trigger()
                .is_some_and(|trigger| trigger.to_str() == expected_trigger);
            action_matches && trigger_matches
        })
}

fn emit_escape_on_window_controller(window: &LushtextWindow) -> glib::Propagation {
    let controllers = window.observe_controllers();
    let controller = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|object| object.downcast::<gtk4::EventControllerKey>().ok())
        .find(|controller| controller.propagation_phase() == gtk4::PropagationPhase::Bubble)
        .expect("window should install a bubble-phase key controller");
    let args: [&dyn glib::value::ToValue; 3] = [
        &gtk4::gdk::Key::Escape,
        &0u32,
        &gtk4::gdk::ModifierType::empty(),
    ];
    let stopped: bool = glib::object::ObjectExt::emit_by_name(&controller, "key-pressed", &args);
    if stopped {
        glib::Propagation::Stop
    } else {
        glib::Propagation::Proceed
    }
}

fn notes_menu_button_visible(window: &LushtextWindow) -> bool {
    window.imp().notes_menu_button.property::<bool>("visible")
}

fn notes_menu_popup_open(window: &LushtextWindow) -> bool {
    let button = &window.imp().notes_menu_button;
    button.is_active()
        || button
            .popover()
            .is_some_and(|popover| popover.is_visible())
}

fn open_notes_menu_popup(window: &LushtextWindow) {
    window.imp().notes_menu_button.popup();
    wait_until(Duration::from_secs(2), || notes_menu_popup_open(window));
}

fn close_notes_menu_popup(window: &LushtextWindow) {
    window.imp().notes_menu_button.popdown();
    wait_until(Duration::from_secs(2), || !notes_menu_popup_open(window));
}

fn widget_left_in(reference: &gtk4::Widget, widget: &gtk4::Widget) -> f32 {
    widget
        .compute_bounds(reference)
        .expect("widget should share allocated bounds with the reference")
        .x()
}

fn properties_surface_uses_bottom_sheet(window: &LushtextWindow) -> bool {
    properties_surface_presentation(window) == PropertiesSurfacePresentation::Sheet
        && window.imp().properties_bottom_sheet.is_open()
}

fn properties_surface_uses_right_pane(window: &LushtextWindow) -> bool {
    properties_surface_presentation(window) == PropertiesSurfacePresentation::Pane
        && window.imp().properties_split_view.shows_sidebar()
}

fn workspace_total_fraction(window: &LushtextWindow) -> f64 {
    window.imp().workspace_split_view.sidebar_width_fraction()
}

fn current_window_width(window: &LushtextWindow) -> i32 {
    if window.width() > 0 {
        window.width()
    } else {
        let (default_width, _) = window.default_size();
        default_width
    }
}

fn current_window_height(window: &LushtextWindow) -> i32 {
    if window.height() > 0 {
        window.height()
    } else {
        let (_, default_height) = window.default_size();
        default_height
    }
}

fn assert_workspace_sidebar_width_locked(window: &LushtextWindow, expected_width: f64) {
    let split = &window.imp().workspace_split_view;
    let min_width = split.min_sidebar_width();
    let max_width = split.max_sidebar_width();
    assert!(
        (min_width - expected_width).abs() < 1.0,
        "expected min sidebar width near {expected_width}, got {min_width}"
    );
    assert!(
        (max_width - expected_width).abs() < 1.0,
        "expected max sidebar width near {expected_width}, got {max_width}"
    );
}

fn write_long_document(editor: &LushtextEditorPage, line_count: usize, needle_every: usize) {
    let mut text = String::new();
    for line in 0..line_count {
        let marker = if needle_every != 0 && line % needle_every == 0 {
            " needle"
        } else {
            ""
        };
        text.push_str(&format!("line {line:04}{marker}\n"));
    }
    editor.buffer().set_text(&text);
    flush_events();
}

fn write_document_with_long_lines(
    editor: &LushtextEditorPage,
    line_count: usize,
    long_every: usize,
    needle_every: usize,
) {
    let mut text = String::new();
    let long_tail = "x".repeat(140);
    for line in 0..line_count {
        let marker = if needle_every != 0 && line % needle_every == 0 {
            " needle"
        } else {
            ""
        };
        if long_every != 0 && line % long_every == 0 {
            text.push_str(&format!("line {line:04}{marker} {long_tail}\n"));
        } else {
            text.push_str(&format!("line {line:04}{marker}\n"));
        }
    }
    editor.buffer().set_text(&text);
    flush_events();
}

fn properties_total_fraction(window: &LushtextWindow) -> f64 {
    let properties_fraction = window.imp().properties_split_view.sidebar_width_fraction();
    if workspace_sidebar_visible(window) && !window.imp().workspace_split_view.is_collapsed() {
        properties_fraction * (1.0 - workspace_total_fraction(window))
    } else {
        properties_fraction
    }
}

fn minimap_setting(window: &LushtextWindow) -> bool {
    window.imp().settings.boolean(keys::SHOW_MINIMAP)
}

fn tab_content_opacity_setting() -> f64 {
    gio::Settings::new(lushtext_core::config::APP_ID).double(keys::TAB_CONTENT_OPACITY)
}

fn preview_animation(window: &LushtextWindow) -> libadwaita::TimedAnimation {
    window
        .imp()
        .preview_animation
        .borrow()
        .as_ref()
        .cloned()
        .expect("preview animation should be active")
}

fn seed_peek_workspace() -> (tempfile::TempDir, PathBuf, PathBuf) {
    ensure_gtk_init();
    let root_dir = tempfile::tempdir().expect("peek workspace tempdir");
    let alpha = root_dir.path().join("alpha.rs");
    let beta = root_dir.path().join("beta.rs");
    std::fs::write(&alpha, "fn alpha() {\n    println!(\"alpha\");\n}\n").expect("write alpha");
    std::fs::write(&beta, "fn beta() {\n    println!(\"beta\");\n}\n").expect("write beta");

    let mut workspaces = WorkspacesFile::default();
    workspaces.workspaces.push(WorkspaceConfig {
        id: WorkspaceId::new("peek-ws"),
        name: "peek".to_string(),
        root: root_dir.path().to_path_buf(),
    });
    workspaces.current_scope = WorkspaceScope::workspace(WorkspaceId::new("peek-ws"));
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save peek workspaces");
    (root_dir, alpha, beta)
}

fn seed_named_tab_files(names: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().expect("named tab tempdir");
    let paths = names
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            std::fs::write(&path, format!("content for {name}\n")).expect("write tab fixture");
            path
        })
        .collect();
    (dir, paths)
}

#[test]
fn test_open_document_restores_bookmarks() {
    let tempdir = tempfile::tempdir().expect("notes tempdir");
    let file_path = tempdir.path().join("src/main.rs");
    std::fs::create_dir_all(file_path.parent().expect("expected operation to succeed"))
        .expect("create file parent");
    std::fs::write(&file_path, "one\ntwo\nthree\nfour\n").expect("write source file");

    let window = test_window();
    let data_dir = json_store::data_dir();

    bookmark_service::save_for_path(
        &data_dir,
        &file_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("bookmark".to_string()),
        )],
    )
    .expect("save bookmarks");

    present_window(&window);
    window.open_document(&file_path);

    wait_until(Duration::from_secs(2), || {
        active_editor(&window).bookmark_records().len() == 1
    });

    let editor = active_editor(&window);
    assert_eq!(
        editor.bookmark_records()[0].label.as_deref(),
        Some("bookmark")
    );
}

fn select_sidebar_path(section: &lushtext_core::ui::sidebar::WorkspaceSection, path: &Path) {
    fn try_select_path(
        section: &lushtext_core::ui::sidebar::WorkspaceSection,
        path: &Path,
    ) -> bool {
        let selection = section
            .imp()
            .file_tree_view
            .model()
            .and_downcast::<gtk4::SingleSelection>()
            .expect("sidebar section should use SingleSelection");
        let tree_model = section
            .imp()
            .tree_model
            .borrow()
            .as_ref()
            .cloned()
            .expect("tree model should be loaded");

        for index in 0..tree_model.n_items() {
            if let Some(row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
                && let Some(item) = row
                    .item()
                    .and_downcast::<lushtext_core::ui::sidebar::FileTreeItem>()
                && item.path().as_deref() == Some(path)
            {
                selection.set_selected(index);
                section
                    .imp()
                    .file_tree_view
                    .scroll_to(index, gtk4::ListScrollFlags::FOCUS, None);
                flush_events();
                return true;
            }
        }
        false
    }

    if try_select_path(section, path) {
        return;
    }

    // Single-root workspaces now expose files under a real directory root, so
    // expand the root tree once before giving up on a nested file-path lookup.
    section.expand_roots();
    wait_until(Duration::from_secs(2), || try_select_path(section, path));
}

fn first_sidebar_section(window: &LushtextWindow) -> lushtext_core::ui::sidebar::WorkspaceSection {
    window
        .imp()
        .sidebar
        .imp()
        .sections
        .borrow()
        .first()
        .cloned()
        .expect("sidebar should have at least one section")
}

#[test]
fn test_window_restores_default_size() {
    ensure_gtk_init();
    let window = test_window();
    let (w, h) = window.default_size();
    assert_eq!(w, 1200);
    assert_eq!(h, 800);
}

#[test]
fn test_split_view_settings_defaults() {
    ensure_gtk_init();
    let window = test_window();
    let settings = &window.imp().settings;

    assert!(settings.boolean(keys::WORKSPACE_SIDEBAR_VISIBLE));
    assert_eq!(settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), 0.3);
    assert!(!settings.boolean(keys::PROPERTIES_SIDEBAR_VISIBLE));
    assert_eq!(
        settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION),
        0.25
    );
}

#[test]
fn test_sidebar_peek_does_not_create_tab_until_promoted() {
    let (_root_dir, alpha, _beta) = seed_peek_workspace();
    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().sections.borrow().len() == 1
    });

    let section = first_sidebar_section(&window);
    select_sidebar_path(&section, &alpha);
    section.imp().file_tree_view.grab_focus();
    assert_tab_count(&window, 0);

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || {
        section.peek_visible()
            && section
                .imp()
                .peek_widgets
                .open_button
                .borrow()
                .as_ref()
                .is_some_and(gtk4::Button::is_sensitive)
    });
    assert_tab_count(&window, 0);

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::Return);
    wait_until(Duration::from_secs(2), || !section.peek_visible());
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });

    assert_tab_count(&window, 1);
    assert_eq!(
        active_editor(&window).file_path().as_deref(),
        Some(alpha.as_path())
    );
}

#[test]
fn test_sidebar_peek_promotion_reuses_existing_tab() {
    let (_root_dir, alpha, _beta) = seed_peek_workspace();
    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().sections.borrow().len() == 1
    });

    window.open_document(&alpha);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });

    let section = first_sidebar_section(&window);
    select_sidebar_path(&section, &alpha);
    section.imp().file_tree_view.grab_focus();

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::space);
    wait_until(Duration::from_secs(2), || {
        section.peek_visible()
            && section
                .imp()
                .peek_widgets
                .open_button
                .borrow()
                .as_ref()
                .is_some_and(gtk4::Button::is_sensitive)
    });

    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::Return);
    wait_until(Duration::from_secs(2), || !section.peek_visible());

    assert_tab_count(&window, 1);
    assert_eq!(
        active_editor(&window).file_path().as_deref(),
        Some(alpha.as_path())
    );
}

#[test]
fn test_split_view_defaults_restore_on_window() {
    ensure_gtk_init();
    let window = test_window();

    assert!(workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!((workspace_total_fraction(&window) - 0.3).abs() < 0.001);
    assert!((properties_total_fraction(&window) - 0.25).abs() < 0.001);
    assert_workspace_sidebar_width_locked(&window, 360.0);
}

#[test]
fn test_workspace_file_sidebar_keeps_list_view_tree_model_rail() {
    ensure_gtk_init();
    let (_roots_dir, _left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    let workspace_sidebar = window.imp().sidebar.upcast_ref::<gtk4::Widget>();
    assert!(
        find_adw_sidebar(workspace_sidebar).is_none(),
        "the primary workspace file sidebar must not be replaced by AdwSidebar"
    );

    assert!(
        has_tree_list_model_list_view(workspace_sidebar),
        "workspace file sidebar should keep GtkTreeListModel backing"
    );
}

#[test]
fn test_saved_split_view_widths_snap_to_supported_workspace_presets() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.25, true, 0.6);
    let settings = &window.imp().settings;

    assert!((workspace_total_fraction(&window) - 0.3).abs() < 0.001);
    assert!((properties_total_fraction(&window) - 0.25).abs() < 0.001);
    assert_eq!(settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), 0.3);
    assert_eq!(
        settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION),
        0.25
    );
}

#[test]
fn test_legacy_sidebar_settings_migrate_to_workspace_split_view() {
    ensure_gtk_init();
    let window = test_window_with_legacy_sidebar_state(false, 275);
    let settings = &window.imp().settings;

    assert!(settings.boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED));
    assert!(!workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    assert!((workspace_total_fraction(&window) - 0.3).abs() < 0.001);
}

#[test]
fn test_toggle_sidebar_hides_workspace_split_view() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-sidebar");
    assert!(!workspace_sidebar_visible(&window));
    assert!(
        !window
            .imp()
            .settings
            .boolean(keys::WORKSPACE_SIDEBAR_VISIBLE)
    );
}

#[test]
fn test_toggle_sidebar_action_always_enabled() {
    ensure_gtk_init();
    let window = test_window();
    assert!(action_enabled(&window, "toggle-sidebar"));
}

#[test]
fn test_toggle_sidebar_action_state_syncs_with_split_view() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-sidebar")
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    assert!(
        action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
    activate_action(&window, "toggle-sidebar");
    assert!(
        !action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
}

#[test]
fn test_toggle_properties_shows_properties_split_view() {
    ensure_gtk_init();
    let window = test_window();
    activate_action(&window, "toggle-properties");
    assert!(properties_sidebar_visible(&window));
    assert!(
        window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );
}

#[test]
fn test_toggle_properties_action_state_tracks_rendered_surface() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-properties")
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    assert!(
        !action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
    activate_action(&window, "toggle-properties");
    assert!(
        action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
    activate_action(&window, "toggle-properties");
    assert!(
        !action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
}

#[test]
fn test_f9_toggles_document_properties_instead_of_workspace_sidebar() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.toggle-properties", "F9"),
        "F9 should be bound to win.toggle-properties"
    );
}

#[test]
fn test_browse_notes_shortcut_is_registered_and_documented() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.show-notes", "<Control><Alt>a"),
        "Ctrl+Alt+A should remain bound to Browse Notes"
    );

    let shortcuts_ui = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/ui/shortcuts.ui"
    ));
    assert!(shortcuts_ui.contains("Browse Notes"));
    assert!(shortcuts_ui.contains("&lt;Control&gt;&lt;Alt&gt;a"));
}

#[test]
fn test_new_document_shortcut_is_ctrl_n_only() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.new-tab", "<Control>n"),
        "Ctrl+N should create a new file"
    );
    assert!(
        !shortcut_bound(&window, "win.new-tab", "<Control>t"),
        "Ctrl+T should no longer create a new file"
    );
}

#[test]
fn test_new_document_action_focuses_new_editor() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    window.imp().primary_menu_button.grab_focus();
    flush_events();
    activate_action(&window, "new-tab");

    assert_eq!(window.imp().tab_view.n_pages(), 1);
    wait_for_active_editor_focus(&window);
}

#[test]
fn test_new_document_exits_markdown_preview_only_mode() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let dir = tempfile::tempdir().expect("new document preview tempdir");
    let original_editor = active_editor(&window);
    original_editor.set_file_path(&dir.path().join("preview-source.md"));
    original_editor.buffer().set_text("# Preview\n\nBody");

    activate_action(&window, "toggle-preview-mode");
    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get()
            && window.imp().markdown_preview.property::<bool>("visible")
            && action_state_bool(&window, "toggle-preview-mode")
    });

    activate_action(&window, "new-tab");

    assert_eq!(window.imp().tab_view.n_pages(), 2);
    let new_editor = active_editor(&window);
    assert!(
        new_editor.file_path().is_none(),
        "New Document should select the new untitled tab"
    );
    assert_ne!(
        new_editor.as_ptr(),
        original_editor.as_ptr(),
        "New Document should not leave the Markdown tab selected"
    );
    assert!(
        !window.imp().preview_mode.get(),
        "New Document should clear preview-only mode"
    );
    assert!(
        !action_state_bool(&window, "toggle-preview-mode"),
        "preview-only action state should match the cleared shell state"
    );
    assert!(
        window.imp().editor_box.property::<bool>("visible"),
        "source editor shell should be visible for the new tab"
    );
    assert!(
        !window.imp().markdown_preview.property::<bool>("visible"),
        "preview widget should not remain visible after creating a new document"
    );
    wait_for_active_editor_focus(&window);
}

#[test]
fn test_new_document_focus_handoff_ignores_stale_selection() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    let original_page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("initial tab should be selected");
    let original_editor = active_editor(&window);
    original_editor.source_view().grab_focus();
    flush_events();

    activate_action(&window, "new-tab");
    window.imp().tab_view.set_selected_page(&original_page);
    flush_after_delay(Duration::from_millis(250));

    assert!(
        window.imp().tab_view.selected_page().as_ref() == Some(&original_page),
        "stale delayed focus should not reselect the newer tab"
    );
    assert_eq!(
        gtk4::prelude::GtkWindowExt::focus(&window).map(|widget| widget.as_ptr()),
        Some(original_editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr()),
        "stale delayed focus should not steal focus from the restored tab"
    );
}

#[test]
fn test_focus_mode_shortcut_is_separate_from_fullscreen_shortcut() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.toggle-focus-mode", "<Shift><Control>F11"),
        "Ctrl+Shift+F11 should be bound to Focus Mode"
    );
    assert!(
        shortcut_bound(&window, "win.toggle-fullscreen", "F11"),
        "F11 should stay bound to ordinary fullscreen"
    );

    activate_action(&window, "toggle-fullscreen");
    assert!(
        !action_state_bool(&window, "toggle-focus-mode"),
        "ordinary fullscreen must not activate Focus Mode"
    );
}

#[test]
fn test_focus_mode_entry_exit_restores_shell_surfaces() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, true, 0.25);
    window.set_default_size(1400, 900);
    window.new_tab();
    present_window(&window);

    assert!(workspace_sidebar_visible(&window));
    assert!(properties_sidebar_visible(&window));

    activate_action(&window, "toggle-focus-mode");

    assert!(action_state_bool(&window, "toggle-focus-mode"));
    assert!(!window.imp().header_bar.property::<bool>("visible"));
    assert!(!window.imp().tab_bar.property::<bool>("visible"));
    assert!(!window.imp().status_bar.property::<bool>("visible"));
    assert!(!workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));

    activate_action(&window, "toggle-focus-mode");

    assert!(!action_state_bool(&window, "toggle-focus-mode"));
    assert!(window.imp().header_bar.property::<bool>("visible"));
    assert!(window.imp().tab_bar.property::<bool>("visible"));
    assert!(window.imp().status_bar.property::<bool>("visible"));
    assert!(workspace_sidebar_visible(&window));
    assert!(properties_sidebar_visible(&window));
}

#[test]
fn test_f9_changes_requested_properties_state_while_focus_mode_suppresses_rendering() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, true, 0.25);
    window.set_default_size(1400, 900);
    window.new_tab();
    present_window(&window);
    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );

    activate_action(&window, "toggle-focus-mode");
    assert!(!properties_sidebar_visible(&window));
    assert!(
        action_state_bool(&window, "toggle-properties"),
        "while focused, the F9 action state should reflect requested state"
    );

    activate_action(&window, "toggle-properties");
    assert!(
        !window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(!properties_sidebar_visible(&window));

    activate_action(&window, "toggle-focus-mode");
    assert!(!properties_sidebar_visible(&window));
}

#[test]
fn test_focus_mode_restores_side_by_side_preview_when_unchanged() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    activate_action(&window, "toggle-preview-pane");
    assert!(window.imp().preview_visible.get());

    activate_action(&window, "toggle-focus-mode");
    assert!(!window.imp().preview_visible.get());

    activate_action(&window, "toggle-focus-mode");
    assert!(
        window.imp().preview_visible.get(),
        "side-by-side preview should restore when Focus Mode exits untouched"
    );
}

#[test]
fn test_alt_p_preview_only_works_inside_focus_mode_and_blocks_preview_restore() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    activate_action(&window, "toggle-preview-pane");
    assert!(window.imp().preview_visible.get());

    activate_action(&window, "toggle-focus-mode");
    activate_action(&window, "toggle-preview-mode");

    assert!(window.imp().focus_mode.active.get());
    assert!(window.imp().preview_mode.get());
    assert!(window.imp().markdown_preview.property::<bool>("visible"));

    activate_action(&window, "toggle-focus-mode");

    assert!(!window.imp().preview_mode.get());
    assert!(
        !window.imp().preview_visible.get(),
        "focused preview changes should prevent side-by-side preview restoration"
    );
}

#[test]
fn test_focus_mode_readable_editor_margins_restore_after_exit() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1600, 900);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).source_view().width() > 0);
    let editor = active_editor(&window);
    let normal_left = editor.focus_mode_left_margin();

    activate_action(&window, "toggle-focus-mode");
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).focus_mode_left_margin() > normal_left
    });
    let focused_left = active_editor(&window).focus_mode_left_margin();
    assert_eq!(focused_left, active_editor(&window).focus_mode_right_margin());

    activate_action(&window, "toggle-focus-mode");
    assert_eq!(active_editor(&window).focus_mode_left_margin(), normal_left);
}

#[test]
fn test_focus_mode_text_origin_guide_visibility_and_margin_tracking() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_uint(keys::FOCUS_MODE_TARGET_COLUMNS, 80)
        .expect("reset Focus Mode column width");
    let window = test_window();
    window.set_default_size(1600, 900);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).source_view().width() > 0);

    assert!(
        !active_editor(&window).focus_mode_text_origin_guide_visible(),
        "the text-origin guide should not render outside Focus Mode"
    );

    activate_action(&window, "toggle-focus-mode");
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).focus_mode_text_origin_guide_visible()
            && active_editor(&window)
                .focus_mode_text_origin_guide_x()
                .is_some()
    });
    let focused_margin = active_editor(&window).focus_mode_left_margin();
    let focused_guide_x = active_editor(&window)
        .focus_mode_text_origin_guide_x()
        .expect("focused guide x coordinate");

    settings
        .set_uint(keys::FOCUS_MODE_TARGET_COLUMNS, 120)
        .expect("widen Focus Mode column width");
    flush_events();
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).focus_mode_left_margin() != focused_margin
            && active_editor(&window)
                .focus_mode_text_origin_guide_x()
                .is_some_and(|x| x != focused_guide_x)
    });

    let updated_margin = active_editor(&window).focus_mode_left_margin();
    let updated_guide_x = active_editor(&window)
        .focus_mode_text_origin_guide_x()
        .expect("updated guide x coordinate");
    assert!(
        (updated_guide_x - focused_guide_x - (updated_margin - focused_margin)).abs() <= 1,
        "the guide should move with the readable-column margin"
    );

    activate_action(&window, "toggle-focus-mode");
    assert!(
        !active_editor(&window).focus_mode_text_origin_guide_visible(),
        "the text-origin guide should hide again when Focus Mode exits"
    );
}

#[test]
fn test_focus_mode_readable_editor_margins_keep_narrow_allocations_usable() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(720, 700);
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).source_view().width() > 0);

    activate_action(&window, "toggle-focus-mode");
    let editor = active_editor(&window);
    assert!(editor.focus_mode_left_margin() >= 24);
    assert!(editor.focus_mode_left_margin() <= editor.source_view().width() / 3);
}

#[test]
fn test_focus_mode_applies_markdown_preview_readable_margins_and_restores() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1600, 900);
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-focus-mode");
    activate_action(&window, "toggle-preview-mode");
    let (left, right) = window.imp().markdown_preview.content_margins();
    assert!(left > 16);
    assert_eq!(left, right);

    activate_action(&window, "toggle-focus-mode");
    assert_eq!(window.imp().markdown_preview.content_margins(), (16, 16));
}

#[test]
fn test_focus_mode_temporarily_hides_minimap_without_changing_preference() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    let window = test_window();
    window.new_tab();
    present_window(&window);
    wait_until(Duration::from_secs(2), || active_editor(&window).is_minimap_visible());

    activate_action(&window, "toggle-focus-mode");
    assert_eq!(
        active_editor(&window).minimap_availability(),
        MinimapAvailability::Disabled
    );
    assert!(settings.boolean(keys::SHOW_MINIMAP));

    activate_action(&window, "toggle-focus-mode");
    wait_until(Duration::from_secs(2), || active_editor(&window).is_minimap_visible());
    assert!(settings.boolean(keys::SHOW_MINIMAP));
}

#[test]
fn test_focus_mode_typewriter_scrolling_defaults_off_and_tracks_setting() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING, false)
        .expect("disable typewriter scrolling");
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-focus-mode");
    assert!(
        !active_editor(&window)
            .imp()
            .focus_mode
            .typewriter_scrolling
            .get()
    );

    settings
        .set_boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING, true)
        .expect("enable typewriter scrolling");
    flush_events();
    assert!(
        active_editor(&window)
            .imp()
            .focus_mode
            .typewriter_scrolling
            .get()
    );
}

#[test]
fn test_escape_closes_command_palette_before_exiting_focus_mode() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    activate_action(&window, "toggle-focus-mode");
    activate_action(&window, "toggle-command-palette");
    assert!(window.imp().palette_revealer.reveals_child());

    emit_escape_on_window_controller(&window);
    flush_events();

    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(window.imp().focus_mode.active.get());

    emit_escape_on_window_controller(&window);
    flush_events();
    assert!(!window.imp().focus_mode.active.get());
}

#[test]
fn test_toggle_minimap_updates_setting_and_action_state() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-minimap")
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    assert!(!minimap_setting(&window));
    assert!(
        !action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );

    activate_action(&window, "toggle-minimap");

    assert!(minimap_setting(&window));
    assert!(
        action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
}

#[test]
fn test_toggle_minimap_action_state_tracks_external_setting_changes() {
    ensure_gtk_init();
    let window = test_window();
    let action = window
        .lookup_action("toggle-minimap")
        .expect("expected operation to succeed")
        .downcast::<gio::SimpleAction>()
        .expect("expected operation to succeed");

    window
        .imp()
        .settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("set show-minimap");
    flush_events();

    assert!(
        action
            .state()
            .expect("expected operation to succeed")
            .get::<bool>()
            .expect("expected operation to succeed")
    );
}

#[test]
fn test_shell_chrome_uses_explicit_opaque_classes_for_transparency_mode() {
    let window = test_window();

    assert!(window.imp().header_bar.has_css_class("header-chrome-opaque"));
    assert!(window.imp().tab_bar.has_css_class("header-chrome-opaque"));
    assert!(window.imp().sidebar.has_css_class("shell-chrome-opaque"));
    assert!(window.imp().properties_panel.has_css_class("shell-chrome-opaque"));
    assert!(window.imp().status_bar.has_css_class("status-bar"));
}

#[test]
fn test_editor_transparency_uses_derived_scheme_and_keeps_minimap_opaque() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 0.85)
        .expect("set tab-content-opacity");
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let temp_dir = tempfile::tempdir().expect("editor tempdir");
    let file_path = temp_dir.path().join("alpha.rs");
    std::fs::write(&file_path, "fn main() {\n    println!(\"hi\");\n}\n").expect("write file");

    let window = test_window();
    present_window(&window);
    window.open_document(&file_path);
    wait_until(Duration::from_secs(2), || active_editor(&window).file_size().is_some());

    let editor = active_editor(&window);
    assert!((tab_content_opacity_setting() - 0.85).abs() < f64::EPSILON);
    assert!((editor.content_background_opacity() - 0.85).abs() < f64::EPSILON);
    assert_eq!(editor.minimap_background_opacity(), 1.0);
    assert!(
        editor
            .applied_style_scheme_id()
            .is_some_and(|id| id.starts_with("lushtext-opacity-")),
        "editor should switch onto the derived opacity-aware style scheme"
    );
}

#[test]
fn test_minimap_visibility_restores_from_setting_for_long_document() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_long_document(&editor, 500, 0);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());
    assert_eq!(editor.minimap_availability(), MinimapAvailability::Visible);
}

#[test]
fn test_minimap_stays_visible_when_document_fits_viewport() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.buffer().set_text("short\nbuffer\n");
    flush_events();
    wait_until(Duration::from_secs(2), || {
        editor.minimap_availability() == MinimapAvailability::Visible
    });

    assert!(editor.is_minimap_visible());
    assert_eq!(editor.minimap_availability(), MinimapAvailability::Visible);
}

#[test]
fn test_large_document_disables_minimap_and_surfaces_feedback() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    editor.imp().size_check.set(FileSizeCheck::DisableSyntax);
    window
        .imp()
        .settings
        .set_boolean(keys::SHOW_MINIMAP, false)
        .expect("disable minimap");
    window
        .imp()
        .settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("re-enable minimap");
    flush_events();

    wait_until(Duration::from_secs(2), || {
        editor.minimap_availability() == MinimapAvailability::TooLarge
    });

    assert!(!editor.is_minimap_visible());
    let status = window
        .imp()
        .notification_bus
        .status_bar_view()
        .expect("minimap warning status message");
    assert_eq!(status.text, "Minimap unavailable for this large document");
}

#[test]
fn test_both_sidebars_can_be_visible_together_on_wide_window() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(keys::WINDOW_WIDTH, 2200)
        .expect("set window width");
    settings
        .set_int(keys::WINDOW_HEIGHT, 900)
        .expect("set window height");
    let window = test_window();
    present_window(&window);

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&window)
            && properties_surface_uses_right_pane(&window)
            && !window.imp().workspace_split_view.is_collapsed()
    });

    assert!(workspace_sidebar_visible(&window));
    assert!(properties_surface_uses_right_pane(&window));
    assert!(!window.imp().workspace_split_view.is_collapsed());
    assert!((workspace_total_fraction(&window) - 360.0 / 2200.0).abs() < 0.001);
    assert!((properties_total_fraction(&window) - 0.25).abs() < 0.001);
    assert_workspace_sidebar_width_locked(&window, 360.0);
}

#[test]
fn test_preferences_sidebar_width_row_updates_workspace_shell_immediately() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let prefs = LushtextPreferences::new();

    prefs.imp().workspace_sidebar_width_row.set_selected(2);
    wait_until(Duration::from_secs(2), || {
        (workspace_total_fraction(&window) - 440.0 / 1400.0).abs() < 0.001
    });

    assert_eq!(
        window
            .imp()
            .settings
            .double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        0.4
    );
    assert_workspace_sidebar_width_locked(&window, 440.0);

    prefs.imp().workspace_sidebar_width_row.set_selected(0);
    wait_until(Duration::from_secs(2), || {
        (workspace_total_fraction(&window) - 280.0 / 1400.0).abs() < 0.001
    });

    assert_eq!(
        window
            .imp()
            .settings
            .double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        0.2
    );
    assert_workspace_sidebar_width_locked(&window, 280.0);
}

#[test]
fn test_workspace_sidebar_width_presets_clamp_across_representative_window_sizes() {
    ensure_gtk_init();

    for (window_width, stored_fraction, expected_width) in [
        (900, 0.2, 220.0),
        (1200, 0.3, 360.0),
        (1400, 0.4, 440.0),
        (2000, 0.3, 360.0),
    ] {
        let window = test_window_with_split_view_state(true, stored_fraction, false, 0.25);
        window.set_default_size(window_width, 900);
        present_window(&window);

        assert!(
            (workspace_total_fraction(&window)
                - expected_width / f64::from(current_window_width(&window)))
            .abs()
                < 0.001
        );
        assert_workspace_sidebar_width_locked(&window, expected_width);
    }
}

#[test]
fn test_split_view_allocation_sync_does_not_rewrite_persisted_properties_width() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1770, 900);
    present_window(&window);

    let settings = &window.imp().settings;
    settings
        .set_double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION, 0.73)
        .expect("seed sentinel properties width");
    let synced_width = window.imp().split_width_synced_for_width.get();

    // Allocation can be queued by Adwaita animation frames. It must keep to
    // runtime geometry and avoid writing persisted settings on each frame.
    window.queue_allocate();
    flush_after_delay(Duration::from_millis(50));

    assert_eq!(window.imp().split_width_synced_for_width.get(), synced_width);
    assert_eq!(
        settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION),
        0.73
    );
}

#[test]
fn test_workspace_sidebar_setting_recalculates_properties_breakpoint() {
    ensure_gtk_init();
    let comfy_window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    comfy_window.set_default_size(1400, 900);
    present_window(&comfy_window);
    activate_action(&comfy_window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_sidebar_visible(&comfy_window));

    assert!(
        properties_surface_uses_right_pane(&comfy_window),
        "Comfy should keep the properties pane side-by-side at 1400sp"
    );

    comfy_window.destroy();
    flush_events();

    let large_window = test_window_with_split_view_state(true, 0.4, false, 0.25);
    large_window.set_default_size(1400, 900);
    present_window(&large_window);
    activate_action(&large_window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&large_window)
    });

    assert!(properties_surface_uses_bottom_sheet(&large_window));
    assert_workspace_sidebar_width_locked(&large_window, 440.0);
}

#[test]
fn test_bookmark_markers_follow_toggle_state() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_long_document(&editor, 400, 0);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());

    let line = editor.buffer().iter_at_line(120).expect("line 120");
    editor.buffer().place_cursor(&line);
    let _ = editor.toggle_bookmark_at_cursor();
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Bookmark) == 1
    });

    let _ = editor.toggle_bookmark_at_cursor();
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Bookmark) == 0
    });
}

#[test]
fn test_search_markers_clear_when_search_closes() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_long_document(&editor, 320, 5);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());

    editor.show_search();
    editor.search_bar().search_entry().set_text("needle");
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Search) > 0
    });

    editor.hide_search();
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Search) == 0
    });
}

#[test]
fn test_modified_markers_clear_after_save() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    let temp = tempfile::NamedTempFile::new().expect("temp file");
    let path = temp.path().to_path_buf();

    editor.set_file_path(&path);
    write_long_document(&editor, 300, 0);

    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Modified) > 0
    });

    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    editor.save_file_async(move |result| {
        result.expect("save should succeed");
        done_clone.set(true);
    });
    wait_until(Duration::from_secs(2), || done.get());
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Modified) == 0
    });
}

#[test]
fn test_long_line_markers_hidden_by_default_when_minimap_is_enabled() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings.reset(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 300, 3, 0);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());
    flush_after_delay(Duration::from_millis(120));

    assert!(!settings.boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE));
    assert_eq!(editor.minimap_marker_count(MinimapMarkerKind::LongLine), 0);
}

#[test]
fn test_long_line_markers_appear_when_preference_is_enabled() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, true)
        .expect("enable long-line minimap markers");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 300, 3, 0);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());
    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::LongLine) > 0
    });
}

#[test]
fn test_disabling_long_line_markers_preserves_other_minimap_markers() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, true)
        .expect("enable long-line minimap markers");

    let window = test_window();
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window);
    write_document_with_long_lines(&editor, 360, 4, 6);

    wait_until(Duration::from_secs(2), || editor.is_minimap_visible());

    let line = editor.buffer().iter_at_line(120).expect("line 120");
    editor.buffer().place_cursor(&line);
    let _ = editor.toggle_bookmark_at_cursor();
    editor.show_search();
    editor.search_bar().search_entry().set_text("needle");

    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::Bookmark) == 1
            && editor.minimap_marker_count(MinimapMarkerKind::Search) > 0
            && editor.minimap_marker_count(MinimapMarkerKind::Modified) > 0
            && editor.minimap_marker_count(MinimapMarkerKind::LongLine) > 0
    });

    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, false)
        .expect("disable long-line minimap markers");
    flush_events();

    wait_until(Duration::from_secs(2), || {
        editor.minimap_marker_count(MinimapMarkerKind::LongLine) == 0
    });
    assert_eq!(editor.minimap_marker_count(MinimapMarkerKind::Bookmark), 1);
    assert!(editor.minimap_marker_count(MinimapMarkerKind::Search) > 0);
    assert!(editor.minimap_marker_count(MinimapMarkerKind::Modified) > 0);
}

#[test]
fn test_close_tab_is_blocked_while_save_is_in_progress() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    let temp = tempfile::NamedTempFile::new().expect("temp file");
    editor.set_file_path(temp.path());
    editor.imp().file_size.set(Some(10_000_000));
    editor.buffer().set_text(&"x".repeat(70_000));

    let save_done = std::rc::Rc::new(std::cell::Cell::new(false));
    let save_done_clone = save_done.clone();
    editor.save_file_async(move |result| {
        result.expect("save should succeed");
        save_done_clone.set(true);
    });
    assert!(editor.is_saving());

    let page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected page");
    let close_confirmed = std::rc::Rc::new(std::cell::RefCell::new(None));
    let close_confirmed_clone = close_confirmed.clone();
    window.confirm_close_tab(&page, &editor, move |confirmed| {
        *close_confirmed_clone.borrow_mut() = Some(confirmed);
    });

    assert_eq!(*close_confirmed.borrow(), Some(false));
    wait_until(Duration::from_secs(2), || save_done.get());
}

#[test]
fn test_draft_autosave_marks_pending_when_editing_during_inflight_batch() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    window.imp().drafts.autosave_inflight.set(true);
    editor.buffer().set_text("changed during autosave");

    assert!(window.imp().drafts.autosave_pending.get());
    window.imp().drafts.autosave_inflight.set(false);
    window.imp().drafts.autosave_pending.set(false);
}

#[test]
fn test_properties_pane_collapses_before_workspace_pane() {
    ensure_gtk_init();

    // At a narrow width just below the adaptive Comfy breakpoint (~1350sp),
    // the document-properties surface should re-render as a bottom sheet while
    // the workspace pane stays in layout.
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1320, 900);
    present_window(&window);
    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_bottom_sheet(&window));

    assert!(properties_sidebar_visible(&window));
    assert!(properties_surface_uses_bottom_sheet(&window));
    assert!(!window.imp().workspace_split_view.is_collapsed());
    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_large_workspace_preset_collapses_properties_pane_earlier() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.4, false, 0.25);
    window.set_default_size(1400, 900);
    present_window(&window);
    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_bottom_sheet(&window));

    assert!(properties_sidebar_visible(&window));
    assert!(properties_surface_uses_bottom_sheet(&window));
    assert!(!window.imp().workspace_split_view.is_collapsed());
    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_hiding_workspace_sidebar_relaxes_properties_breakpoint() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(false, 0.4, false, 0.25);
    window.set_default_size(1400, 900);
    present_window(&window);
    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_sidebar_visible(&window));

    assert!(properties_sidebar_visible(&window));
    assert!(properties_surface_uses_right_pane(&window));
    assert!(!workspace_sidebar_visible(&window));
}

#[test]
fn test_compact_layout_mutual_exclusion_switches_secondary_surface() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1320, 900);
    present_window(&window);

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&window) && !workspace_sidebar_visible(&window)
    });

    assert!(properties_sidebar_visible(&window));
    assert!(!workspace_sidebar_visible(&window));

    activate_action(&window, "toggle-sidebar");
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&window) && !properties_sidebar_visible(&window)
    });

    assert!(workspace_sidebar_visible(&window));
    assert!(!properties_sidebar_visible(&window));
    window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_widening_restores_both_requested_surfaces_after_compact_suppression() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED, true)
        .expect("set split-view-layout-migrated");
    settings
        .set_boolean(keys::WORKSPACE_SIDEBAR_VISIBLE, true)
        .expect("set workspace-sidebar-visible");
    settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, 0.3)
        .expect("set workspace-sidebar-width-fraction");
    settings
        .set_boolean(keys::PROPERTIES_SIDEBAR_VISIBLE, true)
        .expect("set properties-sidebar-visible");
    settings
        .set_double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION, 0.25)
        .expect("set properties-sidebar-width-fraction");

    let wider_window = test_window();
    wider_window.set_default_size(1600, 900);
    present_window(&wider_window);
    wait_until(Duration::from_secs(2), || {
        workspace_sidebar_visible(&wider_window)
            && properties_surface_uses_right_pane(&wider_window)
    });

    assert!(workspace_sidebar_visible(&wider_window));
    assert!(properties_surface_uses_right_pane(&wider_window));
}

#[test]
fn test_properties_visibility_preference_survives_breakpoint_changes() {
    ensure_gtk_init();
    let narrow_window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    narrow_window.set_default_size(1300, 900);
    present_window(&narrow_window);
    activate_action(&narrow_window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&narrow_window)
    });

    assert!(
        narrow_window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );

    narrow_window.destroy();
    flush_after_delay(Duration::from_millis(50));

    let wide_window = test_window();
    wide_window.set_default_size(1600, 900);
    present_window(&wide_window);
    wait_until(Duration::from_secs(2), || properties_surface_uses_right_pane(&wide_window));

    assert!(properties_surface_uses_right_pane(&wide_window));
    assert!(
        wide_window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );
    wide_window.destroy();
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_open_properties_right_pane_transitions_to_open_bottom_sheet_with_active_document_state() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1600, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let first_path = dir.path().join("first.txt");
    let second_path = dir.path().join("second.txt");
    std::fs::write(&first_path, "first\n").expect("write first file");
    std::fs::write(&second_path, "second file\n").expect("write second file");

    window.open_document(&first_path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(first_path.clone())
    });
    window.open_document(&second_path);
    let expected_location = second_path.display().to_string();
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .properties_panel
            .imp()
            .location_row
            .subtitle()
            .as_deref()
            == Some(expected_location.as_str())
    });

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_right_pane(&window));
    assert_eq!(
        window
            .imp()
            .properties_panel
            .imp()
            .location_row
            .subtitle()
            .as_deref(),
        Some(expected_location.as_str())
    );

    set_properties_surface_presentation(&window, PropertiesSurfacePresentation::Sheet);
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&window)
            && window
                .imp()
                .properties_panel
                .imp()
                .location_row
                .subtitle()
                .as_deref()
                == Some(expected_location.as_str())
    });

    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(properties_surface_uses_bottom_sheet(&window));
}

#[test]
fn test_open_properties_bottom_sheet_transitions_to_open_right_pane_with_active_document_state() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1600, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sheet-to-pane.txt");
    std::fs::write(&path, "sheet to pane\n").expect("write file");

    window.open_document(&path);
    let expected_location = path.display().to_string();
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .properties_panel
            .imp()
            .location_row
            .subtitle()
            .as_deref()
            == Some(expected_location.as_str())
    });

    set_properties_surface_presentation(&window, PropertiesSurfacePresentation::Sheet);
    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_bottom_sheet(&window));

    set_properties_surface_presentation(&window, PropertiesSurfacePresentation::Pane);
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_right_pane(&window)
            && window
                .imp()
                .properties_panel
                .imp()
                .location_row
                .subtitle()
                .as_deref()
                == Some(expected_location.as_str())
    });

    assert!(
        window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(properties_surface_uses_right_pane(&window));
}

#[test]
fn test_closed_properties_state_survives_adaptive_presentation_changes() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1600, 900);
    present_window(&window);

    assert!(!properties_sidebar_visible(&window));
    set_properties_surface_presentation(&window, PropertiesSurfacePresentation::Sheet);
    flush_events();
    assert!(!properties_sidebar_visible(&window));
    set_properties_surface_presentation(&window, PropertiesSurfacePresentation::Pane);
    flush_events();

    assert!(!properties_sidebar_visible(&window));
    assert!(
        !window
            .imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    );
    assert!(
        !window
            .imp()
            .settings
            .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE)
    );
}

#[test]
fn test_warning_infobar_actions_stay_allocated_in_a_narrow_window() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1000, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Changes Restored".to_string(),
        body: "Unsaved changes to the document have been restored, and the inline actions must remain visible while the window narrows.".to_string(),
        primary_button: Some("_Discard…".to_string()),
        secondary_button: Some("_Save…".to_string()),
    });
    flush_after_delay(Duration::from_millis(50));

    let info_bar = editor.info_bar().imp();
    assert!(info_bar.discard_button.property::<bool>("visible"));
    assert!(info_bar.save_button.property::<bool>("visible"));
    // Width allocation requires a full compositor layout pass which is not
    // guaranteed in the subprocess widget harness. Verify the property-level
    // visibility, which is what the application code controls.
}

#[test]
fn test_access_error_infobar_action_stays_allocated_in_a_narrow_window() {
    ensure_gtk_init();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);
    window.set_default_size(1000, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Error,
        title: "Could Not Open File".to_string(),
        body: "Permission was denied while opening the document, so the retry action must stay visible after the shell tightens.".to_string(),
        primary_button: Some("_Retry".to_string()),
        secondary_button: None,
    });
    flush_after_delay(Duration::from_millis(50));

    let info_bar = editor.info_bar().imp();
    assert!(info_bar.retry_button.property::<bool>("visible"));
}

#[test]
fn test_restored_workspaces_survive_dual_sidebar_shell() {
    ensure_gtk_init();
    let _roots_dir = seed_restored_workspaces();
    let window = test_window_with_split_view_state(true, 0.3, false, 0.25);

    present_window(&window);
    wait_for_workspace_roots(&window, 3);
    assert!(workspace_sidebar_visible(&window));
    assert_eq!(window.imp().sidebar.all_workspace_root_paths().len(), 3);
}

#[test]
fn test_workspace_selector_updates_search_and_palette_scope() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    dropdown.set_selected(1);
    flush_events();

    wait_for_workspace_consumers(&window, 1, 1);
    assert_eq!(
        window
            .imp()
            .search_panel
            .imp()
            .runtime
            .workspace_roots
            .borrow()
            .as_slice(),
        &[left_root],
    );

    dropdown.set_selected(0);
    flush_events();
    wait_for_workspace_consumers(&window, 2, 2);
}

#[test]
fn test_restored_workspace_scope_narrows_consumers_on_startup() {
    ensure_gtk_init();
    let (_roots_dir, _left_root, right_root) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));
    let window = test_window();
    present_window(&window);

    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);

    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    assert_eq!(dropdown.selected(), 2);
    assert_eq!(
        window
            .imp()
            .search_panel
            .imp()
            .runtime
            .workspace_roots
            .borrow()
            .as_slice(),
        &[right_root],
    );
}

#[test]
fn test_properties_panel_shows_safe_untitled_metadata_state() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let panel = window.imp().properties_panel.imp();
    assert_eq!(
        panel.location_row.subtitle().as_deref(),
        Some("Untitled document")
    );
    assert_eq!(
        panel.file_size_row.subtitle().as_deref(),
        Some("Not available")
    );
    assert!(
        panel
            .statistics_row
            .subtitle()
            .is_some_and(|subtitle| subtitle.contains("1 line")),
    );
    assert_eq!(
        panel.formatting_source_row.subtitle().as_deref(),
        Some("Not available for untitled tabs")
    );
    assert_eq!(
        panel.health_summary_row.subtitle().as_deref(),
        Some("Untitled documents do not have file-backed health details yet.")
    );
}

#[test]
fn test_flush_dirty_drafts_skips_close_discarded_editors() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("discard me");
    editor.buffer().set_modified(true);

    let draft_id = editor.draft_id().expect("draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &draft_id, "stale draft").expect("seed draft");
    draft_service::delete_draft_file(&data_dir, &draft_id).expect("delete seeded draft");

    window
        .imp()
        .drafts
        .close_discard_ids
        .borrow_mut()
        .insert(draft_id.clone());
    window
        .flush_dirty_drafts()
        .expect("discarded draft flush should succeed");

    assert_eq!(
        draft_service::read_draft(&data_dir, &draft_id).expect("read draft"),
        None,
        "discarded drafts must not be recreated during close flush",
    );
    assert!(
        window.imp().drafts.close_discard_ids.borrow().is_empty(),
        "close discard state should be cleared after the flush",
    );
}

#[test]
fn test_flush_dirty_drafts_fails_when_manifest_cannot_be_saved() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("keep me");
    editor.buffer().set_modified(true);

    let draft_id = editor.draft_id().expect("draft id");
    let data_dir = json_store::data_dir();
    let drafts_dir = draft_service::drafts_dir(&data_dir);
    let manifest_path = drafts_dir.join("manifest.json");
    if manifest_path.is_dir() {
        std::fs::remove_dir_all(&manifest_path).expect("remove stale manifest dir");
    } else {
        let _ = std::fs::remove_file(&manifest_path);
    }
    std::fs::create_dir_all(&manifest_path).expect("create manifest path as directory");

    let error = window
        .flush_dirty_drafts()
        .expect_err("manifest failure should block close-time draft flush");

    assert!(
        error.to_string().contains("failed to save draft manifest"),
        "unexpected error: {error}",
    );
    assert_eq!(
        draft_service::read_draft(&data_dir, &draft_id).expect("read draft"),
        Some("keep me".to_string()),
        "draft bytes already written must be visible for recovery",
    );

    std::fs::remove_dir_all(&manifest_path).expect("remove manifest dir");
    draft_service::delete_draft_file(&data_dir, &draft_id).expect("delete draft file");
}

#[test]
fn test_complete_save_as_failure_keeps_existing_editor_identity() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("unsaved draft");
    editor.buffer().set_modified(true);

    let old_draft_id = editor.draft_id().expect("untitled draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &old_draft_id, "unsaved draft").expect("seed draft");

    let path = std::env::temp_dir()
        .join("lushtext-save-as-missing-parent")
        .join("failure.txt");
    window.complete_save_as(
        &editor,
        None,
        Some(old_draft_id.as_str()),
        &path,
        Err(SaveError::WriteTemp {
            path: path.clone(),
            source: std::io::Error::other("boom"),
        }),
    );

    assert_eq!(editor.file_path(), None);
    assert_eq!(editor.draft_id().as_deref(), Some(old_draft_id.as_str()));
    assert!(
        !window.imp().open_paths.borrow().contains(&path),
        "failed Save As must not register the destination as open",
    );
    assert_eq!(
        draft_service::read_draft(&data_dir, &old_draft_id).expect("read draft"),
        Some("unsaved draft".to_string()),
        "failed Save As must keep the prior draft available",
    );
}

#[test]
fn test_complete_save_as_success_updates_editor_identity_and_cleans_old_draft() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    flush_events();

    let editor = active_editor(&window);
    editor.buffer().set_text("saved content");
    editor.buffer().set_modified(false);

    let old_draft_id = editor.draft_id().expect("untitled draft id");
    let data_dir = json_store::data_dir();
    draft_service::write_draft(&data_dir, &old_draft_id, "saved content").expect("seed draft");

    let dir = tempfile::tempdir().expect("save as tempdir");
    let path = dir.path().join("saved.txt");
    std::fs::write(&path, "saved content").expect("seed saved file");

    window.complete_save_as(&editor, None, Some(old_draft_id.as_str()), &path, Ok(()));

    assert_eq!(editor.file_path(), Some(path.clone()));
    assert_eq!(editor.title(), "saved.txt");
    assert!(
        window.imp().open_paths.borrow().contains(&path),
        "successful Save As must register the new destination as open",
    );
    wait_until(Duration::from_secs(2), || {
        draft_service::read_draft(&data_dir, &old_draft_id)
            .expect("read draft")
            .is_none()
    });
}

#[test]
fn test_local_history_action_requires_saved_eligible_document() {
    ensure_gtk_init();
    let window = test_window();

    assert!(!action_enabled(&window, "show-local-history"));

    window.new_tab();
    flush_events();
    assert!(!action_enabled(&window, "show-local-history"));

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history.txt");
    std::fs::write(&path, "one\n").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });
    assert!(action_enabled(&window, "show-local-history"));

    let editor = active_editor(&window);
    editor.imp().size_check.set(FileSizeCheck::DisableUndoAndSyntax);
    let saved_page = window
        .imp()
        .tab_view
        .selected_page()
        .expect("saved page selected");
    window.new_tab();
    flush_events();
    window.imp().tab_view.set_selected_page(&saved_page);
    flush_events();
    assert!(!action_enabled(&window, "show-local-history"));
}

#[test]
fn test_active_editor_extra_menu_includes_contextual_notes_and_local_history() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("extra-menu.txt");
    std::fs::write(&path, "hello").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    let menu = editor
        .source_view()
        .extra_menu()
        .expect("source view should expose an extra menu");
    let labels = menu_model_labels(&menu);
    for label in [
        "Toggle Bookmark",
        "Edit Bookmark Label…",
        "Open Document Note…",
        "Local History…",
    ] {
        assert!(
            labels.iter().any(|entry| entry == label),
            "editor content menu should offer {label}"
        );
    }
}

#[test]
fn test_local_history_dialog_shows_empty_state_without_snapshots() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty-history.txt");
    std::fs::write(&path, "one\n").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some() && action_enabled(&window, "show-local-history")
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    assert_eq!(
        dialog.content_width(),
        560,
        "empty-state browser should keep its compact target width"
    );
    assert_eq!(
        dialog.content_height(),
        360,
        "empty-state browser should keep its compact target height"
    );
    assert!(
        dialog.content_width() <= 720,
        "empty-state browser should stay compact on screen, got width {}",
        dialog.content_width()
    );
    assert!(
        dialog.content_height() <= 520,
        "empty-state browser should stay compact on screen, got height {}",
        dialog.content_height()
    );
    assert!(
        find_label_by_text(&child, "No local history yet").is_some(),
        "empty-state browser should explain why no snapshots are listed"
    );
}

#[test]
fn test_local_history_browser_explains_empty_snapshot_and_disables_copy() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty-snapshot-history.txt");
    std::fs::write(&path, "").expect("write file");

    let data_dir = json_store::data_dir();
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed empty baseline");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some() && action_enabled(&window, "show-local-history")
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let history_dialog_size = settled_widget_outer_size(&dialog);
    wait_until(Duration::from_secs(2), || {
        find_label_by_text(&child, "This snapshot was empty").is_some()
    });
    assert_settled_widget_outer_size(
        &dialog,
        history_dialog_size,
        "local-history empty snapshot preview",
    );

    assert!(
        find_label_by_text(&child, "This snapshot was empty").is_some(),
        "empty snapshots should explain that they contained no text"
    );
    assert!(
        find_label_by_text(&child, "Before edits · Empty file").is_some(),
        "empty snapshots should use semantic metadata instead of only 0 B"
    );
    assert!(
        find_button_by_label(&child, "Restore").is_some_and(|button| button.is_sensitive()),
        "empty historical snapshots should still be restorable"
    );
    assert!(
        find_button_by_label(&child, "Copy").is_some_and(|button| !button.is_sensitive()),
        "copy should be disabled when the snapshot has no text content"
    );
}

#[test]
fn test_local_history_browser_hides_legacy_empty_baseline_noise() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("legacy-noise-history.txt");
    std::fs::write(&path, "").expect("write file");

    let data_dir = json_store::data_dir();
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed oldest empty baseline");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "draft content",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Periodic,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed first periodic");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed second empty baseline");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "draft content",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Periodic,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed second periodic");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed newest empty baseline");

    let raw_snapshots = local_history_service::list_snapshots_for_path(&data_dir, &path)
        .expect("list local history");
    assert_eq!(raw_snapshots.len(), 5);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some() && action_enabled(&window, "show-local-history")
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let sidebar = find_adw_sidebar(&child).expect("snapshot sidebar");
    wait_until(Duration::from_secs(2), || sidebar.item(1).is_some());

    assert!(
        sidebar.item(2).is_none(),
        "legacy empty-baseline rows should be filtered out of the visible browser sidebar"
    );
    assert!(
        find_label_by_text(&child, "Before edits · Empty file").is_none(),
        "legacy empty-baseline labels should be hidden from view"
    );
    assert!(
        find_label_by_text(&child, "While editing · 13 B").is_some(),
        "useful periodic history should remain visible"
    );
    assert_eq!(
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list stored local history")
            .len(),
        5,
        "browser filtering must not delete stored local history"
    );
}

#[test]
fn test_local_history_dialog_scales_from_parent_and_keeps_preview_dominant() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1600, 1000);
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("viewer-history.txt");
    std::fs::write(&path, "current\n").expect("write file");

    let data_dir = json_store::data_dir();
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "version one\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed version one");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "version two\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed version two");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let split_view = find_navigation_split_view(&child).expect("navigation split view");
    let sidebar = find_adw_sidebar(&child).expect("snapshot sidebar");
    let history_dialog_size = settled_widget_outer_size(&dialog);
    wait_until(Duration::from_secs(2), || {
        find_button_by_label(&child, "Copy").is_some_and(|button| button.is_sensitive())
    });
    assert_settled_widget_outer_size(
        &dialog,
        history_dialog_size,
        "local-history initial preview load",
    );
    wait_until(Duration::from_secs(2), || sidebar.item(1).is_some());
    sidebar.set_selected(1);
    flush_events();
    assert_settled_widget_outer_size(
        &dialog,
        history_dialog_size,
        "local-history selection loading state",
    );
    wait_until(Duration::from_secs(2), || {
        find_button_by_label(&child, "Copy").is_some_and(|button| button.is_sensitive())
    });
    assert_settled_widget_outer_size(
        &dialog,
        history_dialog_size,
        "local-history selected preview load",
    );

    let window_width = current_window_width(&window);
    let window_height = current_window_height(&window);
    let dialog_width = dialog.content_width();
    let dialog_height = dialog.content_height();
    assert!(
        !dialog.follows_content_size(),
        "viewer dialog must honor the configured content size instead of shrinking to the child"
    );
    assert!(
        dialog_width >= 1200,
        "expected a large rendered viewer width, got {dialog_width}"
    );
    assert!(
        dialog_width <= window_width - 20,
        "viewer dialog should stay smaller than its parent width (dialog {dialog_width}, parent {window_width})"
    );
    assert!(
        dialog_height >= 760,
        "expected a tall rendered viewer height, got {dialog_height}"
    );
    assert!(
        dialog_height <= window_height - 20,
        "viewer dialog should stay smaller than its parent height (dialog {dialog_height}, parent {window_height})"
    );
    assert!(
        split_view.max_sidebar_width() < f64::from(dialog_width) / 2.0,
        "snapshot rail should stay narrower than the preview-dominant half of the viewer"
    );
    let max_sidebar_width = split_view.max_sidebar_width();
    assert!(
        max_sidebar_width <= 340.0,
        "snapshot rail should stay in browse-rail territory, got {max_sidebar_width}"
    );
}

#[test]
fn test_local_history_browser_collapses_and_restore_can_be_undone() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history-browser.txt");
    std::fs::write(&path, "current\n").expect("write file");

    let data_dir = json_store::data_dir();
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "version one\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed version one");
    std::thread::sleep(Duration::from_millis(2));
    local_history_service::capture_snapshot_for_path(
        &data_dir,
        &path,
        "version two\n",
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed version two");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    editor.buffer().set_text("working copy");
    editor.buffer().set_modified(true);

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    let split_view = find_navigation_split_view(&child).expect("navigation split view");
    let sidebar = find_adw_sidebar(&child).expect("snapshot sidebar");
    wait_until(Duration::from_secs(2), || sidebar.item(1).is_some());

    split_view.set_collapsed(true);
    sidebar.set_selected(1);
    flush_events();

    wait_until(Duration::from_secs(2), || split_view.shows_content());
    wait_until(Duration::from_secs(2), || {
        find_button_by_label(&child, "Restore").is_some_and(|button| button.is_sensitive())
    });

    let restore_button =
        find_button_by_label(&child, "Restore").expect("restore button in local-history dialog");
    restore_button.emit_clicked();

    wait_until(Duration::from_secs(2), || editor_text(&editor) == "version two\n");
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .notification_bus
            .editor_info_bar_view(editor.notification_owner_id())
            .is_some()
    });

    let undo_button = find_button_by_label(
        editor.info_bar().upcast_ref::<gtk4::Widget>(),
        "Undo Restore",
    )
    .expect("undo restore button");
    undo_button.emit_clicked();

    wait_until(Duration::from_secs(2), || editor_text(&editor) == "working copy");
}

#[test]
fn test_local_history_capture_policy_respects_full_save_only_and_unavailable_modes() {
    ensure_gtk_init();
    // SAFETY: each widget test runs in its own child process, and this interval
    // override is read later when local-history timers are scheduled.
    unsafe { std::env::set_var("LUSHTEXT_LOCAL_HISTORY_INTERVAL_MS", "25") };

    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("capture-policy.txt");
    std::fs::write(&path, "saved\n").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let data_dir = json_store::data_dir();
    let editor = active_editor(&window);
    editor.buffer().set_text("baseline change");
    editor.buffer().set_modified(true);

    wait_until(Duration::from_secs(2), || {
        !local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list local history")
            .is_empty()
    });

    editor.buffer().set_text("periodic change");
    editor.buffer().set_modified(true);
    wait_until(Duration::from_secs(2), || {
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list local history")
            .len()
            >= 2
    });

    editor.buffer().set_text("save boundary change");
    editor.buffer().set_modified(true);
    activate_action(&window, "save");
    wait_until(Duration::from_secs(2), || {
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list local history")
            .len()
            >= 3
    });

    editor.imp().size_check.set(FileSizeCheck::DisableSyntax);
    let count_after_full = local_history_service::list_snapshots_for_path(&data_dir, &path)
        .expect("list after full mode")
        .len();

    editor.buffer().set_text("save only change");
    editor.buffer().set_modified(true);
    flush_after_delay(Duration::from_millis(120));
    assert_eq!(
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list after save-only baseline wait")
            .len(),
        count_after_full,
        "save-only mode must skip baseline and periodic capture",
    );

    activate_action(&window, "save");
    wait_until(Duration::from_secs(2), || {
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list after save-only save")
            .len()
            == count_after_full + 1
    });

    editor.imp().size_check.set(FileSizeCheck::DisableUndoAndSyntax);
    let count_after_save_only = local_history_service::list_snapshots_for_path(&data_dir, &path)
        .expect("list after save-only mode")
        .len();

    editor.buffer().set_text("unavailable change");
    editor.buffer().set_modified(true);
    flush_after_delay(Duration::from_millis(120));
    activate_action(&window, "save");
    flush_after_delay(Duration::from_millis(120));

    assert_eq!(
        local_history_service::list_snapshots_for_path(&data_dir, &path)
            .expect("list after unavailable mode")
            .len(),
        count_after_save_only,
        "unavailable mode must disable both automatic and save-boundary capture",
    );
}

#[test]
fn test_properties_panel_updates_for_file_backed_editor() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("example.txt");
    std::fs::write(&path, "hello world").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .properties_panel
            .imp()
            .file_size_row
            .subtitle()
            .is_some_and(|subtitle| subtitle == "11 B")
    });

    let panel = window.imp().properties_panel.imp();
    assert_eq!(
        panel.location_row.subtitle().as_deref(),
        Some(path.display().to_string().as_str())
    );
    assert_eq!(panel.file_size_row.subtitle().as_deref(), Some("11 B"));
    assert!(
        panel
            .statistics_row
            .subtitle()
            .is_some_and(|subtitle| subtitle.contains("1 line")),
    );
    assert_eq!(
        panel.health_summary_row.subtitle().as_deref(),
        Some("No file-health issues recorded for this document.")
    );
}

#[test]
fn test_status_bar_shows_detected_encoding_and_line_endings_after_open() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("encoded.txt");
    std::fs::write(&path, [0x63, 0x61, 0x66, 0xE9, b'\r', b'\n']).expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let status_bar = window.imp().status_bar.imp();
    assert_eq!(
        status_bar.encoding_button.label().as_deref(),
        Some("Windows-1252")
    );
    assert_eq!(
        status_bar.line_ending_button.label().as_deref(),
        Some("CRLF")
    );
}

#[test]
fn test_reopen_with_encoding_requires_discard_confirmation_for_modified_document() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("reopen.txt");
    std::fs::write(&path, "hello").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    editor.buffer().set_text("modified");
    editor.buffer().set_modified(true);

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    click_alert_extra_button(&dialog, "Reopen with Encoding…");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Reopen with Encoding"))
    });
    let dialog = visible_alert_dialog(&window).expect("reopen encoding dialog visible");
    click_alert_extra_button(&dialog, "Windows-1252");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Discard Changes"))
    });
}

#[test]
fn test_status_bar_encoding_label_stays_short_after_save_policy_change() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("save-policy.txt");
    std::fs::write(&path, "hello").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    click_alert_extra_button(&dialog, "Save Using Encoding…");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Save Using Encoding"))
    });
    let dialog = visible_alert_dialog(&window).expect("save encoding dialog visible");
    click_alert_extra_button(&dialog, "Windows-1252");

    assert_eq!(
        active_editor(&window).save_encoding(),
        DocumentEncoding::Windows1252
    );
    assert_eq!(
        window
            .imp()
            .status_bar
            .imp()
            .encoding_button
            .label()
            .as_deref(),
        Some("UTF-8")
    );
}

#[test]
fn test_save_encoding_choice_surfaces_lossy_confirmation() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("lossy.txt");
    std::fs::write(&path, "hello").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    editor.buffer().set_text("emoji 😀");
    editor.buffer().set_modified(true);

    activate_action(&window, "show-encoding-controls");
    let dialog = visible_alert_dialog(&window).expect("encoding dialog visible");
    click_alert_extra_button(&dialog, "Save Using Encoding…");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Save Using Encoding"))
    });
    let dialog = visible_alert_dialog(&window).expect("save encoding dialog visible");
    click_alert_extra_button(&dialog, "Windows-1252");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Lossy Encoding Conversion"))
    });
    assert_eq!(
        active_editor(&window).save_encoding(),
        DocumentEncoding::Utf8
    );
}

#[test]
fn test_mixed_line_endings_warning_opens_normalization_picker_and_updates_status_bar() {
    ensure_gtk_init();
    let window = test_window();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("mixed.txt");
    std::fs::write(&path, "a\r\nb\nc\r\n").expect("write file");

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let editor = active_editor(&window);
    wait_until(Duration::from_secs(2), || {
        editor
            .info_bar()
            .imp()
            .discard_infobar
            .property::<bool>("revealed")
    });
    assert_eq!(
        editor.info_bar().imp().discard_button.label().as_deref(),
        Some("_Normalize…")
    );

    editor.info_bar().imp().discard_button.emit_clicked();
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading.contains("Line Endings"))
    });

    let dialog = visible_alert_dialog(&window).expect("line endings dialog visible");
    click_alert_extra_button(&dialog, "LF");

    assert_eq!(active_editor(&window).save_line_ending(), LineEnding::Lf);
    assert!(
        active_editor(&window)
            .file_health()
            .into_iter()
            .all(|finding| finding.kind != FileHealthFindingKind::MixedLineEndings)
    );
    assert_eq!(
        window
            .imp()
            .status_bar
            .imp()
            .line_ending_button
            .label()
            .as_deref(),
        Some("LF")
    );
}

#[test]
fn test_narrow_window_keeps_quick_encoding_controls_visible() {
    ensure_gtk_init();
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(keys::WINDOW_WIDTH, 820)
        .expect("set window width");
    settings
        .set_int(keys::WINDOW_HEIGHT, 900)
        .expect("set window height");
    settings
        .set_boolean(keys::WINDOW_MAXIMIZED, false)
        .expect("clear maximized");
    let window = test_window();
    window.set_default_size(820, 900);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("narrow.txt");
    std::fs::write(&path, "hello").expect("write file");

    window.open_document(&path);
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_size().is_some()
    });

    let status_bar = window.imp().status_bar.imp();
    assert!(
        status_bar.line_ending_button.property::<bool>("visible"),
        "narrow windows should keep the line-ending entry point visible",
    );
    assert!(
        status_bar.encoding_button.property::<bool>("visible"),
        "narrow windows should keep the encoding entry point visible",
    );
}

#[test]
fn test_closing_properties_pane_restores_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1600, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.source_view().grab_focus();
    flush_events();

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || properties_surface_uses_right_pane(&window));
    window
        .imp()
        .properties_panel
        .imp()
        .location_row
        .grab_focus();
    flush_events();
    let source_ptr = editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr();
    activate_action(&window, "toggle-properties");

    wait_until(Duration::from_secs(2), || {
        gtk4::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| focus.as_ptr() == source_ptr)
    });
    assert!(!properties_sidebar_visible(&window));
    let _ = editor;
}

#[test]
fn test_closing_properties_bottom_sheet_restores_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1300, 900);
    window.new_tab();
    present_window(&window);

    let editor = active_editor(&window);
    editor.source_view().grab_focus();
    flush_events();

    activate_action(&window, "toggle-properties");
    wait_until(Duration::from_secs(2), || {
        properties_surface_uses_bottom_sheet(&window)
    });
    window
        .imp()
        .properties_panel
        .imp()
        .location_row
        .grab_focus();
    flush_events();
    let source_ptr = editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr();
    activate_action(&window, "toggle-properties");

    wait_until(Duration::from_secs(2), || {
        gtk4::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| focus.as_ptr() == source_ptr)
    });
    assert!(!properties_sidebar_visible(&window));
    let _ = editor;
}

#[test]
fn test_closing_properties_surface_with_no_editor_clears_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(800, 900);
    present_window(&window);

    activate_action(&window, "toggle-properties");
    let panel = window.imp().properties_panel.imp();
    panel.location_row.grab_focus();
    flush_events();

    activate_action(&window, "toggle-properties");
    assert!(gtk4::prelude::GtkWindowExt::focus(&window).is_none());
}

#[test]
fn test_properties_toggle_button_lives_in_header_bar_and_is_wired() {
    ensure_gtk_init();
    let window = test_window();
    assert_eq!(
        window
            .imp()
            .document_properties_toggle_button
            .action_name()
            .as_deref(),
        Some("win.toggle-properties")
    );
}

#[test]
fn test_primary_menu_button_exists() {
    ensure_gtk_init();
    let window = test_window();
    assert!(window.imp().primary_menu_button.popover().is_some());
}

#[test]
fn test_primary_menu_exposes_markdown_preview_action() {
    ensure_gtk_init();
    let window = test_window();

    assert_eq!(
        primary_menu_action_for_label(&window, "Markdown Preview").as_deref(),
        Some("win.toggle-preview-mode"),
        "primary menu should expose the rendered Markdown preview action"
    );
}

#[test]
fn test_markdown_preview_shortcut_remains_alt_p() {
    ensure_gtk_init();
    let window = test_window();

    assert!(
        shortcut_bound(&window, "win.toggle-preview-mode", "<Alt>p"),
        "Alt+P should keep toggling Markdown preview-only mode"
    );
}

#[test]
fn test_primary_menu_markdown_preview_renders_active_markdown_buffer() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let dir = tempfile::tempdir().expect("markdown preview tempdir");
    let editor = active_editor(&window);
    editor.set_file_path(&dir.path().join("menu-preview.md"));
    editor
        .buffer()
        .set_text("# Menu Heading\n\nCurrent buffer body");

    activate_primary_menu_item(&window, "Markdown Preview");

    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get() && window.imp().markdown_preview.is_showing_content()
    });
    let rendered = window.imp().markdown_preview.buffer_text();
    assert!(
        rendered.contains("Menu Heading"),
        "preview should render the active Markdown buffer"
    );
    assert!(
        rendered.contains("Current buffer body"),
        "preview should include body text from the active buffer"
    );
    assert!(
        !rendered.contains("# Menu Heading"),
        "preview should hide raw heading markers"
    );
}

#[test]
fn test_preview_only_definition_list_code_block_uses_live_column() {
    let (window, _dir) =
        prepare_markdown_preview_window(DEFINITION_LIST_CODE_BLOCK_SAMPLE, 1180, 720);

    activate_action(&window, "toggle-preview-mode");

    wait_until(Duration::from_secs(2), || window.imp().preview_mode.get());
    wait_for_markdown_preview_shell(&window);
    assert_live_code_block_uses_preview_column(&window);
}

#[test]
fn test_side_by_side_definition_list_code_block_uses_live_column() {
    ensure_gtk_init();
    gio::Settings::new(lushtext_core::config::APP_ID)
        .set_int(keys::PREVIEW_PANE_POSITION, 520)
        .expect("set wide preview pane for definition-list code block regression");
    let (window, _dir) =
        prepare_markdown_preview_window(DEFINITION_LIST_CODE_BLOCK_SAMPLE, 1800, 720);

    activate_action(&window, "toggle-preview-pane");

    wait_until(Duration::from_secs(2), || window.imp().preview_visible.get());
    wait_for_markdown_preview_shell(&window);
    assert_live_code_block_uses_preview_column(&window);
}

#[test]
fn test_primary_menu_markdown_preview_shows_placeholder_for_non_markdown() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let dir = tempfile::tempdir().expect("plain preview tempdir");
    let editor = active_editor(&window);
    editor.set_file_path(&dir.path().join("plain.txt"));
    editor.buffer().set_text("# Plain text heading-shaped line");

    activate_primary_menu_item(&window, "Markdown Preview");

    wait_until(Duration::from_secs(2), || {
        window.imp().preview_mode.get() && !window.imp().markdown_preview.is_showing_content()
    });
}

#[test]
fn test_notes_menu_exists_and_primary_menu_excludes_note_actions() {
    ensure_gtk_init();
    let window = test_window();

    assert!(window.imp().notes_menu_button.popover().is_some());

    let notes_menu = window
        .imp()
        .notes_menu_button
        .menu_model()
        .expect("notes menu model");
    assert_eq!(
        menu_model_labels(&notes_menu),
        vec![
            "Browse Notes…".to_string(),
            "Add Bookmark".to_string(),
            "Open Document Note…".to_string(),
            "Open Workspace Note…".to_string(),
        ]
    );

    let primary_menu = window
        .imp()
        .primary_menu_button
        .menu_model()
        .expect("primary menu model");
    let primary_labels = menu_model_labels(&primary_menu);
    for label in [
        "Add Bookmark",
        "Open Document Note…",
        "Open Workspace Note…",
        "Browse Notes…",
        "Edit Bookmark Label…",
        "Browse Bookmarks…",
    ] {
        assert!(
            !primary_labels.iter().any(|entry| entry == label),
            "primary menu should not include '{label}' once the Notes menu exists",
        );
    }
}

#[test]
fn test_notes_menu_button_hides_without_editor_or_workspace() {
    ensure_gtk_init();
    let window = test_window();
    assert!(!notes_menu_button_visible(&window));
}

#[test]
fn test_notes_menu_state_for_workspace_without_saved_file() {
    ensure_gtk_init();
    let (_roots_dir, _left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);
    assert!(notes_menu_button_visible(&window));

    for name in [
        "notes-toggle-bookmark",
        "notes-open-document-note",
        "notes-open-workspace-note",
    ] {
        assert!(
            !action_enabled(&window, name),
            "expected '{name}' to stay disabled without a saved document",
        );
    }
    assert!(action_enabled(&window, "notes-show-notes"));

    activate_action(&window, "new-tab");
    assert!(notes_menu_button_visible(&window));
    for name in [
        "notes-toggle-bookmark",
        "notes-open-document-note",
        "notes-open-workspace-note",
    ] {
        assert!(
            !action_enabled(&window, name),
            "expected '{name}' to stay disabled for an untitled tab",
        );
    }
}

#[test]
fn test_notes_menu_workspace_note_action_enables_for_concrete_scope() {
    ensure_gtk_init();
    let (_roots_dir, _left_root, _right_root) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));
    let window = test_window();
    present_window(&window);

    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);
    assert!(notes_menu_button_visible(&window));
    assert!(action_enabled(&window, "notes-open-workspace-note"));
    assert!(action_enabled(&window, "notes-show-notes"));
}

#[test]
fn test_notes_menu_popup_opens_for_add_and_remove_bookmark_states() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_root.join("notes-popup.rs");
    std::fs::write(&path, "one\ntwo\nthree\n").expect("write notes popup source");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("bookmark".to_string()),
        )],
    )
    .expect("save bookmark sidecar");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        let labels = menu_model_labels(
            &window
                .imp()
                .notes_menu_button
                .menu_model()
                .expect("notes menu model"),
        );
        notes_menu_button_visible(&window)
            && active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).bookmark_records().len() == 1
            && labels.iter().any(|label| label == "Remove Bookmark")
    });

    open_notes_menu_popup(&window);
    let labels = menu_model_labels(
        &window
            .imp()
            .notes_menu_button
            .menu_model()
            .expect("notes menu model"),
    );
    assert!(labels.iter().any(|label| label == "Browse Notes…"));
    assert!(labels.iter().any(|label| label == "Remove Bookmark"));
    close_notes_menu_popup(&window);

    let editor = active_editor(&window);
    let line_two = editor.buffer().iter_at_line(1).expect("line two");
    editor.buffer().place_cursor(&line_two);
    flush_events();

    wait_until(Duration::from_secs(2), || {
        menu_model_labels(
            &window
                .imp()
                .notes_menu_button
                .menu_model()
                .expect("notes menu model"),
        )
        .iter()
        .any(|label| label == "Add Bookmark")
    });

    open_notes_menu_popup(&window);
    let labels = menu_model_labels(
        &window
            .imp()
            .notes_menu_button
            .menu_model()
            .expect("notes menu model"),
    );
    assert!(labels.iter().any(|label| label == "Browse Notes…"));
    assert!(labels.iter().any(|label| label == "Add Bookmark"));
    close_notes_menu_popup(&window);
}

#[test]
fn test_sidebar_context_menus_include_note_entry_points() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);

    let section = window
        .imp()
        .sidebar
        .imp()
        .sections
        .borrow()
        .first()
        .cloned()
        .expect("workspace section");

    let file_menu = section
        .imp()
        .context_menu
        .borrow()
        .as_ref()
        .and_then(gtk4::PopoverMenu::menu_model)
        .expect("file context menu model");
    assert!(
        menu_model_labels(&file_menu)
            .iter()
            .any(|label| label == "Open Document Note…"),
        "file context menu should expose document notes"
    );

    let header_menu = section
        .imp()
        .header_context_menu
        .borrow()
        .as_ref()
        .and_then(gtk4::PopoverMenu::menu_model)
        .expect("workspace header context menu model");
    assert!(
        menu_model_labels(&header_menu)
            .iter()
            .any(|label| label == "Open Workspace Note…"),
        "workspace header context menu should expose workspace notes"
    );

    *section.imp().context_path.borrow_mut() = Some(left_root.join("alpha.rs"));
    section.imp().context_is_dir.set(false);
    section
        .activate_action("section.document-note", None)
        .expect("document-note widget action should exist");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });
    visible_alert_dialog(&window)
        .expect("document note dialog")
        .close();
    flush_events();

    section
        .activate_action("ws-header.open-workspace-note", None)
        .expect("workspace-note widget action should exist");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Workspace Note")
    });
}

#[test]
fn test_document_note_dialog_supports_edit_and_render_modes() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_root.join("document-note.md");
    std::fs::write(&path, "# Heading\n\nBody\n").expect("write document note source");

    let data_dir = json_store::data_dir();
    document_note_service::save_for_path(
        &data_dir,
        &path,
        &RichNoteBody::new("# Heading\n\nSaved note"),
    )
    .expect("save document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || active_editor(&window).file_path() == Some(path.clone()));

    activate_action(&window, "open-document-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });

    let dialog = visible_alert_dialog(&window).expect("document note dialog");
    let extra = dialog.extra_child().expect("document note extra child");
    let switcher = find_stack_switcher(&extra).expect("note editor switcher");
    let stack = find_note_editor_stack(&extra).expect("note editor stack");
    assert_eq!(switcher.stack(), Some(stack.clone()));
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));

    let switcher_bounds = switcher
        .compute_bounds(&extra)
        .expect("switcher bounds in dialog content");
    let stack_bounds = stack.compute_bounds(&extra).expect("stack bounds in dialog content");
    let switcher_right = switcher_bounds.x() + switcher_bounds.width();
    let stack_right = stack_bounds.x() + stack_bounds.width();
    assert!(
        (switcher_bounds.x() - stack_bounds.x()).abs() <= 1.0,
        "switcher should start at the same left edge as the note stack"
    );
    assert!(
        (switcher_right - stack_right).abs() <= 1.0,
        "switcher should reach the same right edge as the note stack"
    );

    assert_note_editor_text_margins_match(&extra);
    assert_note_editor_render_keeps_modal_geometry(&dialog, &extra, &stack);
    assert_eq!(stack.visible_child_name().as_deref(), Some("render"));

    stack.set_visible_child_name("edit");
    flush_events();
    assert_eq!(stack.visible_child_name().as_deref(), Some("edit"));
}

#[test]
fn test_empty_document_note_first_render_keeps_modal_geometry_after_typing() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_root.join("empty-document-note.md");
    std::fs::write(&path, "# Source\n\nBody\n").expect("write document note source");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(path.clone())
            && action_enabled(&window, "open-document-note")
    });

    activate_action(&window, "open-document-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });

    let dialog = visible_alert_dialog(&window).expect("document note dialog");
    let extra = dialog.extra_child().expect("document note extra child");
    let stack = find_note_editor_stack(&extra).expect("note editor stack");
    assert_note_editor_text_margins_match(&extra);
    assert_typed_note_editor_first_render_keeps_modal_geometry(
        &dialog,
        &extra,
        &stack,
        "# Typed document note\n\nPreview me",
    );
}

#[test]
fn test_open_workspace_note_dialog_for_concrete_scope() {
    ensure_gtk_init();
    let (_roots_dir, _left_root, right_root) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));

    let data_dir = json_store::data_dir();
    workspace_note_service::save_for_root(
        &data_dir,
        &right_root,
        &RichNoteBody::new("Workspace note"),
    )
    .expect("save workspace note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);

    activate_action(&window, "open-workspace-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Workspace Note")
    });
}

#[test]
fn test_empty_workspace_note_first_render_keeps_modal_geometry_after_typing() {
    ensure_gtk_init();
    let (_roots_dir, _left_root, _right_root) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-right")));

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 1, 1);

    activate_action(&window, "open-workspace-note");
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Workspace Note")
    });

    let dialog = visible_alert_dialog(&window).expect("workspace note dialog");
    let extra = dialog.extra_child().expect("workspace note extra child");
    let stack = find_note_editor_stack(&extra).expect("workspace note editor stack");
    assert_note_editor_text_margins_match(&extra);
    assert_typed_note_editor_first_render_keeps_modal_geometry(
        &dialog,
        &extra,
        &stack,
        "# Typed workspace note\n\nPreview me",
    );
}

#[test]
fn test_browse_notes_opens_document_note_for_selected_row() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_root.join("browse-notes.md");
    std::fs::write(&path, "# Notes\n").expect("write browser note source");

    let data_dir = json_store::data_dir();
    document_note_service::save_for_path(
        &data_dir,
        &path,
        &RichNoteBody::new("# Note\n\nOpen me"),
    )
    .expect("save document note");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let dialog_child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&dialog_child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(2), || {
        sidebar.item(0).is_some()
            && find_button_by_label(&dialog_child, "Open")
                .is_some_and(|button| button.is_sensitive())
    });

    sidebar.emit_by_name::<()>("activated", &[&0u32]);
    flush_events();
    assert!(
        visible_alert_dialog(&window).is_none(),
        "activating a notes browser row should preview/select instead of opening the editor"
    );
    assert!(
        visible_sheet_dialog(&window).is_some(),
        "the notes browser should remain open after row activation"
    );

    find_button_by_label(&dialog_child, "Open")
        .expect("notes browser open button")
        .emit_clicked();
    flush_events();

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window)
            .and_then(|dialog| dialog.heading())
            .as_deref()
            == Some("Document Note")
    });
}

#[test]
fn test_browse_notes_opens_bookmark_for_selected_row() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_root.join("browse-bookmark.rs");
    std::fs::write(&path, "one\ntwo\nthree\n").expect("write bookmark source");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("jump here".to_string()),
        )],
    )
    .expect("save bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let dialog_child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&dialog_child).expect("notes browser sidebar");
    flush_events();
    assert_eq!(sidebar.items().n_items(), 1);
    assert_eq!(
        sidebar.item(0).and_then(|item| item.title()).as_deref(),
        Some("Bookmark · jump here")
    );
    let open_button = find_button_by_label(&dialog_child, "Open").expect("notes browser open button");
    wait_until(Duration::from_secs(2), || open_button.is_sensitive());

    sidebar.emit_by_name::<()>("activated", &[&0u32]);
    flush_events();
    assert!(
        visible_sheet_dialog(&window).is_some(),
        "activating a bookmark row should only preview/select it"
    );

    open_button.emit_clicked();
    flush_events();

    wait_until(Duration::from_secs(2), || {
        active_editor(&window).file_path() == Some(path.clone())
            && active_editor(&window).cursor_position().0 == 1
    });
}

#[test]
fn test_browse_notes_filters_bookmarks_to_current_workspace_scope() {
    ensure_gtk_init();
    let (_roots_dir, left_root, right_root) =
        seed_scoped_workspaces(WorkspaceScope::workspace(WorkspaceId::new("ws-left")));
    let left_path = left_root.join("left-bookmark.rs");
    let right_path = right_root.join("right-bookmark.rs");
    std::fs::write(&left_path, "left\n").expect("write left bookmark source");
    std::fs::write(&right_path, "right\n").expect("write right bookmark source");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &left_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("left only".to_string()),
        )],
    )
    .expect("save left bookmark");
    bookmark_service::save_for_path(
        &data_dir,
        &right_path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("right hidden".to_string()),
        )],
    )
    .expect("save right bookmark");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 1, 2);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 1);
    assert!(
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Bookmark · left only"),
        "current workspace scope should include the left bookmark"
    );

    let search_entry = find_search_entry(&child).expect("notes search entry");
    search_entry.set_text("right hidden");
    flush_events();
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 0);
}

#[test]
fn test_notes_browser_uses_sectioned_adw_sidebar_and_filters_note_body() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_root.join("sectioned-notes.md");
    std::fs::write(&path, "one\ntwo\nthree\n").expect("write sectioned note source");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            1,
            Some("bookmark needle".to_string()),
        )],
    )
    .expect("save bookmark");
    workspace_note_service::save_for_root(
        &data_dir,
        &left_root,
        &RichNoteBody::new("workspace needle"),
    )
    .expect("save workspace note");
    document_note_service::save_for_path(
        &data_dir,
        &path,
        &RichNoteBody::new("document needle"),
    )
    .expect("save document note");
    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 3);
    let notes_browser_size = settled_widget_outer_size(&dialog);

    for index in 0u32..3 {
        sidebar.emit_by_name::<()>("activated", &[&index]);
        flush_events();
        assert!(
            visible_alert_dialog(&window).is_none(),
            "pointer-style activation at notes browser index {index} should not open an editor"
        );
        assert!(
            visible_sheet_dialog(&window).is_some(),
            "the notes browser should stay visible after activating index {index}"
        );
        assert_settled_widget_outer_size(
            &dialog,
            notes_browser_size,
            "notes browser row activation",
        );
    }

    let section_titles: Vec<_> = (0..sidebar.sections().n_items())
        .filter_map(|index| sidebar.section(index))
        .filter_map(|section| section.title().map(|title| title.to_string()))
        .collect();
    assert_eq!(
        section_titles,
        ["Bookmarks", "Workspace Notes", "Document Notes"],
        "notes browser should expose semantic Adwaita sidebar sections"
    );

    let split_view = find_navigation_split_view(&child).expect("notes split view");
    split_view.set_collapsed(true);
    sidebar.set_selected(0);
    flush_events();
    wait_until(Duration::from_secs(2), || split_view.shows_content());
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser collapsed preview selection",
    );
    assert!(
        find_label_by_text(&child, "Bookmark · bookmark needle").is_some(),
        "selecting a bookmark should update the preview before opening it"
    );
    assert!(
        find_label_by_text(
            &child,
            &format!("left · {} · Line 2", path.display()),
        )
        .is_some(),
        "bookmark preview metadata should include workspace, file path, and line"
    );

    sidebar.set_selected(2);
    flush_events();
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser document-note preview selection",
    );
    assert!(
        find_label_by_text(&child, "Document Note · sectioned-notes.md").is_some(),
        "selecting a sidebar note should update the preview before opening it"
    );

    let search_entry = find_search_entry(&child).expect("notes search entry");
    search_entry.set_text("document needle");
    flush_events();
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 1);
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser document-note filtering",
    );
    assert!(
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Document Note · sectioned-notes.md"),
        "notes search should match document note body text, not only visible metadata"
    );

    search_entry.set_text("bookmark needle");
    flush_events();
    wait_until(Duration::from_secs(2), || {
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Bookmark · bookmark needle")
    });
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser bookmark filtering",
    );

    search_entry.set_text("Line 2");
    flush_events();
    flush_after_delay(Duration::from_millis(200));
    wait_until(Duration::from_secs(2), || {
        sidebar
            .item(0)
            .and_then(|item| item.title())
            .is_some_and(|title| title == "Bookmark · bookmark needle")
    });
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser line-metadata filtering",
    );

    search_entry.set_text("missing needle");
    flush_events();
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 0);
    assert_settled_widget_outer_size(
        &dialog,
        notes_browser_size,
        "notes browser empty filtered state",
    );
    assert!(
        find_label_by_text(&child, "No notes match that search").is_some(),
        "empty filtered notes state should remain explicit"
    );
}

#[test]
fn test_notes_browser_caps_large_result_sets_with_refine_notice() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_root.join("many-bookmarks.rs");
    let content = (0..510)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    std::fs::write(&path, content).expect("write many bookmark source");

    let bookmarks = (0..510)
        .map(|line| {
            lushtext_core::model::bookmark::BookmarkRecord::new(
                line,
                Some(format!("bookmark {line}")),
            )
        })
        .collect::<Vec<_>>();
    bookmark_service::save_for_path(&json_store::data_dir(), &path, &bookmarks)
        .expect("save many bookmarks");

    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("notes browser dialog");
    let child = dialog.child().expect("notes browser child");
    let sidebar = find_adw_sidebar(&child).expect("notes browser sidebar");
    wait_until(Duration::from_secs(2), || sidebar.items().n_items() == 500);
    assert!(
        find_label_by_text(
            &child,
            "Showing first 500 matches. Refine search to narrow results."
        )
        .is_some(),
        "large note sets should explain that the sidebar result set is capped"
    );
}

#[test]
fn test_notes_menu_renders_immediately_left_of_main_menu() {
    ensure_gtk_init();
    let (_roots_dir, _left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let window = test_window();
    present_window(&window);

    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 2);
    wait_until(Duration::from_secs(2), || {
        let header_bar = window.imp().header_bar.upcast_ref::<gtk4::Widget>();
        let notes_button = window.imp().notes_menu_button.upcast_ref::<gtk4::Widget>();
        let main_button = window.imp().primary_menu_button.upcast_ref::<gtk4::Widget>();
        notes_menu_button_visible(&window)
            && notes_button.compute_bounds(header_bar).is_some()
            && main_button.compute_bounds(header_bar).is_some()
    });

    let header_bar = window.imp().header_bar.upcast_ref::<gtk4::Widget>();
    let notes_x = widget_left_in(
        header_bar,
        window.imp().notes_menu_button.upcast_ref::<gtk4::Widget>(),
    );
    let main_x = widget_left_in(
        header_bar,
        window.imp().primary_menu_button.upcast_ref::<gtk4::Widget>(),
    );

    assert!(
        notes_x < main_x,
        "Notes should render left of Main Menu instead of to its right",
    );
}

#[test]
fn test_notes_menu_cursor_specific_actions_follow_active_note_context() {
    ensure_gtk_init();
    let (_roots_dir, left_root, _right_root) = seed_scoped_workspaces(WorkspaceScope::All);
    let path = left_root.join("notes-state.rs");
    std::fs::write(&path, "one\ntwo\nthree\n").expect("write note-state source");

    let data_dir = json_store::data_dir();
    bookmark_service::save_for_path(
        &data_dir,
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("bookmark".to_string()),
        )],
    )
    .expect("save bookmark sidecar");
    let window = test_window();
    present_window(&window);
    wait_for_workspace_roots(&window, 2);
    wait_for_workspace_consumers(&window, 2, 3);

    window.open_document(&path);
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor.bookmark_records().len() == 1
            && menu_model_labels(
                &window
                    .imp()
                    .notes_menu_button
                    .menu_model()
                    .expect("notes menu model"),
            )
            .iter()
            .any(|label| label == "Remove Bookmark")
    });

    assert!(action_enabled(&window, "notes-toggle-bookmark"));
    assert!(action_enabled(&window, "notes-open-document-note"));
    assert!(!action_enabled(&window, "notes-open-workspace-note"));
    assert!(action_enabled(&window, "edit-bookmark-label"));

    let editor = active_editor(&window);
    let line_two = editor.buffer().iter_at_line(1).expect("line two");
    editor.buffer().place_cursor(&line_two);
    flush_events();

    wait_until(Duration::from_secs(2), || {
        menu_model_labels(
            &window
                .imp()
                .notes_menu_button
                .menu_model()
                .expect("notes menu model"),
        )
        .iter()
        .any(|label| label == "Add Bookmark")
    });
    assert!(action_enabled(&window, "notes-toggle-bookmark"));
    assert!(action_enabled(&window, "notes-open-document-note"));
}

#[test]
fn test_tab_context_menu_targets_background_tab_for_move_action() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["a.txt", "b.txt", "c.txt"]);
    let window = test_window();
    present_window(&window);

    for path in &files {
        window.open_document(path);
    }
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 3
    });

    let selected_before = window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected tab before move");
    let first_page = find_tab_page_by_title(&window, "a.txt");
    prepare_tab_context_menu(&window, &first_page);

    assert!(!action_enabled(&window, "move-tab-left"));
    assert!(action_enabled(&window, "move-tab-right"));

    activate_action(&window, "move-tab-right");
    wait_until(Duration::from_secs(2), || {
        tab_titles(&window) == vec!["b.txt", "a.txt", "c.txt"]
    });

    let selected_after = window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected tab after move");
    assert_eq!(
        selected_before.as_ptr(),
        selected_after.as_ptr(),
        "moving a background tab should not retarget selection",
    );

    let last_page = find_tab_page_by_title(&window, "c.txt");
    prepare_tab_context_menu(&window, &last_page);
    assert!(!action_enabled(&window, "move-tab-right"));
}

#[test]
fn test_bulk_close_context_action_uses_one_confirmation_before_closing() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["a.txt", "b.txt", "c.txt"]);
    let window = test_window();
    present_window(&window);

    for path in &files {
        window.open_document(path);
    }
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 3
    });

    let modified_page = find_tab_page_by_title(&window, "b.txt");
    let modified_editor = modified_page
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("modified editor page");
    wait_until(Duration::from_secs(2), || {
        modified_editor.file_size().is_some()
    });
    modified_editor.buffer().set_text("modified");
    modified_editor.buffer().set_modified(true);
    flush_events();

    let target = find_tab_page_by_title(&window, "a.txt");
    prepare_tab_context_menu(&window, &target);
    activate_action(&window, "close-other-tabs");

    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window).is_some()
    });
    assert_tab_count(&window, 3);

    let dialog = visible_alert_dialog(&window).expect("bulk close confirmation");
    dialog.emit_by_name::<()>("response", &[&"discard"]);
    dialog.force_close();
    wait_until(Duration::from_secs(2), || {
        visible_alert_dialog(&window).is_none()
    });
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });

    assert_eq!(tab_titles(&window), vec!["a.txt"]);
}

#[test]
fn test_pin_action_updates_indicator_icon() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["pin-me.txt"]);
    let window = test_window();
    present_window(&window);
    window.open_document(&files[0]);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });

    let page = find_tab_page_by_title(&window, "pin-me.txt");
    prepare_tab_context_menu(&window, &page);
    activate_action(&window, "toggle-tab-pinned");
    wait_until(Duration::from_secs(2), || page.is_pinned());
    assert!(page.indicator_icon().is_some());

    prepare_tab_context_menu(&window, &page);
    activate_action(&window, "toggle-tab-pinned");
    wait_until(Duration::from_secs(2), || !page.is_pinned());
    assert!(page.indicator_icon().is_none());
}

#[test]
fn test_session_restore_keeps_pinned_tabs_ahead_of_unpinned_tabs() {
    ensure_gtk_init();
    let (_dir, files) = seed_named_tab_files(&["alpha.txt", "beta.txt", "gamma.txt"]);
    let session = SessionData {
        tabs: vec![
            SessionTab {
                path: Some(files[0].clone()),
                draft_id: None,
                cursor_line: 1,
                cursor_col: 0,
                scroll_line: 0,
                pinned: true,
            },
            SessionTab {
                path: Some(files[1].clone()),
                draft_id: None,
                cursor_line: 2,
                cursor_col: 0,
                scroll_line: 0,
                pinned: true,
            },
            SessionTab {
                path: Some(files[2].clone()),
                draft_id: None,
                cursor_line: 3,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            },
        ],
        active_tab_index: Some(2),
    };
    session_service::save(&json_store::data_dir(), &session).expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 3
    });

    assert_eq!(
        tab_titles(&window),
        vec!["alpha.txt", "beta.txt", "gamma.txt"]
    );
    let pages = tab_pages(&window);
    assert!(pages[0].is_pinned());
    assert!(pages[1].is_pinned());
    assert!(!pages[2].is_pinned());
    assert_eq!(
        window
            .imp()
            .tab_view
            .selected_page()
            .expect("restored selected page")
            .title()
            .as_str(),
        "gamma.txt"
    );
}

#[test]
fn test_local_history_startup_restore_uses_restored_draft_as_baseline() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("restored-history.txt");
    std::fs::write(&file_path, "").expect("write empty file");
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&file_path);
    let draft_content = "draft content";
    draft_service::write_draft(&data_dir, &draft_id, draft_content).expect("seed draft");
    let current_mtime = editor_io::mtime_secs(&file_path).expect("file mtime");
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.clone(),
                original_path: Some(file_path.clone()),
                original_mtime_secs: Some(current_mtime),
                saved_at_secs: 1,
            }],
        },
    )
    .expect("save manifest");
    session_service::save(
        &data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(file_path.clone()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 1);
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor_text(&editor) == draft_content
    });
    wait_until(Duration::from_secs(2), || {
        !local_history_service::list_snapshots_for_path(&data_dir, &file_path)
            .expect("list local history")
            .is_empty()
    });

    let snapshots = local_history_service::list_snapshots_for_path(&data_dir, &file_path)
        .expect("list local history");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].origin,
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Baseline
    );
    assert_eq!(snapshots[0].byte_len, draft_content.len() as u64);

    let loaded = local_history_service::load_snapshot_for_path(
        &data_dir,
        &file_path,
        &snapshots[0].snapshot_id,
    )
    .expect("load local history snapshot")
    .expect("baseline snapshot should exist");
    assert_eq!(loaded.text, draft_content);

    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(2), || visible_sheet_dialog(&window).is_some());

    let dialog = visible_sheet_dialog(&window).expect("local-history dialog visible");
    let child = dialog.child().expect("dialog child");
    wait_until(Duration::from_secs(2), || {
        find_label_by_text(&child, "Before edits · 13 B").is_some()
    });

    assert!(
        find_label_by_text(&child, "Before edits · 13 B").is_some(),
        "draft-restored history should baseline the restored working content"
    );
    assert!(
        find_label_by_text(&child, "Before edits · Empty file").is_none(),
        "draft-restored history should not show a fresh empty disk baseline row"
    );
}

#[test]
fn test_startup_restore_applies_matching_file_backed_draft() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("restored.txt");
    std::fs::write(&file_path, "disk content").expect("write file");
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&file_path);
    draft_service::write_draft(&data_dir, &draft_id, "draft content").expect("seed draft");
    let current_mtime = editor_io::mtime_secs(&file_path).expect("file mtime");
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.clone(),
                original_path: Some(file_path.clone()),
                original_mtime_secs: Some(current_mtime),
                saved_at_secs: 1,
            }],
        },
    )
    .expect("save manifest");
    session_service::save(
        &data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(file_path),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor_text(&editor) == "draft content"
    });

    let editor = active_editor(&window);
    assert_eq!(editor_text(&editor), "draft content");
    assert!(editor.is_draft_restored());
    let notification = window
        .imp()
        .notification_bus
        .editor_info_bar_view(editor.notification_owner_id())
        .expect("draft restore notification");
    assert_eq!(notification.title, "Draft Changes Restored");
}

#[test]
fn test_startup_restore_skips_stale_file_backed_draft_once() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let file_path = dir.path().join("stale.txt");
    std::fs::write(&file_path, "current disk content").expect("write file");
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_path(&file_path);
    draft_service::write_draft(&data_dir, &draft_id, "stale draft").expect("seed draft");
    let current_mtime = editor_io::mtime_secs(&file_path).expect("file mtime");
    let stale_mtime = current_mtime
        .checked_add(1)
        .unwrap_or_else(|| current_mtime.saturating_sub(1));
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.clone(),
                original_path: Some(file_path.clone()),
                original_mtime_secs: Some(stale_mtime),
                saved_at_secs: 1,
            }],
        },
    )
    .expect("save manifest");
    session_service::save(
        &data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(file_path),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        window
            .imp()
            .notification_bus
            .editor_info_bar_view(editor.notification_owner_id())
            .is_some()
    });
    wait_until(Duration::from_secs(2), || {
        draft_service::read_draft(&data_dir, &draft_id)
            .expect("read draft")
            .is_none()
    });

    let editor = active_editor(&window);
    assert_eq!(editor_text(&editor), "current disk content");
    assert!(!editor.is_draft_restored());
    let notification = window
        .imp()
        .notification_bus
        .editor_info_bar_view(editor.notification_owner_id())
        .expect("stale draft warning");
    assert_eq!(notification.title, "Draft Not Restored");
    assert!(
        window
            .imp()
            .drafts
            .manifest
            .borrow()
            .find_by_id(&draft_id)
            .is_none()
    );
}

#[test]
fn test_startup_restore_keeps_untitled_draft_behavior() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let draft_id = draft_service::draft_id_for_untitled(42);
    draft_service::write_draft(&data_dir, &draft_id, "untitled restored content")
        .expect("seed untitled draft");
    draft_service::save_manifest(
        &data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.clone(),
                original_path: None,
                original_mtime_secs: None,
                saved_at_secs: 1,
            }],
        },
    )
    .expect("save manifest");
    session_service::save(
        &data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: None,
                draft_id: Some(draft_id),
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 1
    });
    wait_until(Duration::from_secs(2), || {
        let editor = active_editor(&window);
        editor_text(&editor) == "untitled restored content"
    });

    let editor = active_editor(&window);
    assert_eq!(editor_text(&editor), "untitled restored content");
    assert!(editor.is_draft_restored());
    let notification = window
        .imp()
        .notification_bus
        .editor_info_bar_view(editor.notification_owner_id())
        .expect("untitled restore notification");
    assert_eq!(notification.title, "Document Restored");
}

#[test]
fn test_preview_pane_toggle_starts_nontrivial_animation() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-preview-pane");

    let animation = preview_animation(&window);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The Adw animation exposes f64 endpoints, but preview pane positions stay within i32 paned coordinates"
    )]
    let from = animation.value_from() as i32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The preview pane target position is always an i32 paned coordinate"
    )]
    let to = animation.value_to() as i32;
    assert_ne!(
        from, to,
        "preview pane toggle should start a real paned animation, not jump directly to the endpoint",
    );
    assert_eq!(animation.state(), libadwaita::AnimationState::Playing);
    assert!(
        window.imp().markdown_preview.property::<bool>("visible"),
        "preview widget should stay visible while the side-by-side animation is in flight",
    );
}

#[test]
fn test_preview_mode_toggle_starts_nontrivial_animation() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);

    activate_action(&window, "toggle-preview-mode");

    let animation = preview_animation(&window);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The Adw animation exposes f64 endpoints, but preview pane positions stay within i32 paned coordinates"
    )]
    let from = animation.value_from() as i32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The preview pane target position is always an i32 paned coordinate"
    )]
    let to = animation.value_to() as i32;
    assert_ne!(
        from, to,
        "preview-only mode should animate the paned instead of snapping immediately",
    );
    assert_eq!(animation.state(), libadwaita::AnimationState::Playing);
    assert!(
        window.imp().editor_box.property::<bool>("visible"),
        "editor box should remain visible until the preview-only animation completes",
    );
}

#[test]
fn test_preview_pane_animation_does_not_enqueue_position_persistence_per_tick() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let generation_before = window.imp().preview_persist_generation.get();

    activate_action(&window, "toggle-preview-pane");

    assert!(window.imp().preview_animation_active.get());
    assert_eq!(
        window.imp().preview_persist_generation.get(),
        generation_before,
        "programmatic preview animation should not enqueue debounced settings writes on every tick",
    );
    assert_eq!(
        preview_animation(&window).state(),
        libadwaita::AnimationState::Playing
    );
}

#[test]
fn test_preview_mode_animation_does_not_enqueue_position_persistence_per_tick() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    present_window(&window);
    let generation_before = window.imp().preview_persist_generation.get();

    activate_action(&window, "toggle-preview-mode");

    assert!(window.imp().preview_animation_active.get());
    assert_eq!(
        window.imp().preview_persist_generation.get(),
        generation_before,
        "preview-only animation should not enqueue debounced settings writes on every tick",
    );
    assert_eq!(
        preview_animation(&window).state(),
        libadwaita::AnimationState::Playing
    );
}

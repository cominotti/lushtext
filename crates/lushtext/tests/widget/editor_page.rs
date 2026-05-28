// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextEditorPage widget.

use crate::common::{ensure_gtk_init, present_window, test_application, wait_until};
use gio::prelude::ListModelExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use lushtext_core::ui::editor_page::{
    BookmarkNavigationDirection, BookmarkToggleState, LushtextEditorPage,
};
use sourceview5::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

fn button_label(button: &gtk4::Button) -> gtk4::Label {
    button
        .child()
        .expect("button child")
        .downcast::<gtk4::Label>()
        .expect("button label")
}

fn same_widget(widget: &gtk4::Widget, target: &impl IsA<gtk4::Widget>) -> bool {
    widget.as_ptr() == target.as_ref().as_ptr()
}

fn visible_alert_action_order(page: &LushtextEditorPage) -> Vec<&'static str> {
    let imp = page.info_bar().imp();
    let mut order = Vec::new();
    let mut child = imp.actions_box.first_child();

    while let Some(widget) = child {
        child = widget.next_sibling();
        if !widget.is_visible() {
            continue;
        }

        if same_widget(&widget, &*imp.retry_button) {
            order.push("retry");
        } else if same_widget(&widget, &*imp.discard_button) {
            order.push("discard");
        } else if same_widget(&widget, &*imp.save_button) {
            order.push("save");
        } else if same_widget(&widget, &*imp.dismiss_button) {
            order.push("dismiss");
        } else {
            panic!("unexpected visible inline-alert action: {}", widget.type_().name());
        }
    }

    order
}

fn descendants(root: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Widget> {
    let mut result = Vec::new();
    let mut stack = vec![root.clone().upcast::<gtk4::Widget>()];
    while let Some(widget) = stack.pop() {
        let mut child = widget.first_child();
        while let Some(current) = child {
            result.push(current.clone());
            stack.push(current.clone());
            child = current.next_sibling();
        }
    }
    result
}

fn minimap_controller_types(widget: &impl IsA<gtk4::Widget>) -> Vec<String> {
    let controllers = widget.observe_controllers();
    let mut types = Vec::new();
    for index in 0..controllers.n_items() {
        let controller = controllers
            .item(index)
            .expect("controller should still exist");
        types.push(controller.type_().name().to_string());
    }
    types.sort();
    types
}

fn baseline_source_map_for_view(view: &sourceview5::View) -> sourceview5::Map {
    let map = sourceview5::Map::new();
    map.set_view(view);
    map.set_editable(false);
    map.set_cursor_visible(false);
    map.set_can_focus(false);
    map.set_wrap_mode(gtk4::WrapMode::None);
    map.set_show_line_numbers(false);
    map.set_show_line_marks(false);
    map.set_highlight_current_line(false);
    map.set_monospace(true);
    map.set_left_margin(0);
    map.set_right_margin(0);
    map.set_overflow(gtk4::Overflow::Visible);
    map.add_css_class("monospace");
    map.add_css_class("minimap-view");
    map.set_hexpand(true);
    map.set_vexpand(true);
    map
}

fn present_editor_page(page: &LushtextEditorPage) -> gtk4::ApplicationWindow {
    let app = test_application();
    let window = gtk4::ApplicationWindow::builder()
        .application(&app)
        .default_width(1000)
        .default_height(800)
        .child(page)
        .build();
    present_window(&window);
    wait_until(std::time::Duration::from_secs(2), || {
        page.source_view().is_mapped() && page.source_view().visible_rect().height() > 0
    });
    window
}

#[test]
fn test_new() {
    ensure_gtk_init();
    let _page = LushtextEditorPage::new();
}

#[test]
fn test_inline_alert_uses_supported_widgets() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let widget_types: Vec<_> = descendants(page.info_bar())
        .into_iter()
        .map(|widget| widget.type_().name().to_string())
        .collect();

    assert!(
        widget_types.iter().any(|name| name == "GtkRevealer"),
        "inline alert should use a supported GtkRevealer"
    );
    assert!(
        !widget_types.iter().any(|name| name == "GtkInfoBar"),
        "inline alert must not instantiate deprecated GtkInfoBar"
    );
    assert_eq!(
        page.info_bar().imp().alert_box.accessible_role(),
        gtk4::AccessibleRole::Alert,
        "inline alert should keep alert semantics for assistive technology"
    );
}

#[test]
fn test_inline_alert_buttons_use_scoped_contrast_class() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let imp = page.info_bar().imp();

    for (name, button) in [
        ("retry", &*imp.retry_button),
        ("discard", &*imp.discard_button),
        ("save", &*imp.save_button),
        ("dismiss", &*imp.dismiss_button),
    ] {
        assert!(
            button.has_css_class("inline-alert-button"),
            "{name} inline-alert button should opt into scoped contrast styling"
        );
    }

    assert_eq!(
        imp.dismiss_button.tooltip_text().as_deref(),
        Some("Dismiss"),
        "icon-only dismiss action should keep an accessible text affordance"
    );

    let css = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/style/style.css"
    ));
    assert!(
        css.contains(".editor-inline-alert .inline-alert-button"),
        "inline-alert contrast styling should be scoped through the alert button class"
    );
    assert!(
        !css.contains(".editor-inline-alert button {"),
        "inline-alert contrast styling should not target every nested button"
    );
}

#[test]
fn test_starts_unmodified() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert!(!page.is_modified());
}

#[test]
fn test_file_path_initially_none() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert!(page.file_path().is_none());
}

#[test]
fn test_title_untitled_when_no_path() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert_eq!(page.title(), "Untitled");
}

#[test]
fn test_buffer_accessible() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    // Buffer should be a sourceview5::Buffer
    assert!(!buffer.is_modified());
}

#[test]
fn test_source_view_accessible() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let view = page.source_view();
    // Source view should be functional
    assert!(view.is_visible());
}

#[test]
fn test_buffer_text_manipulation() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    buffer.set_text("hello world");
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    assert_eq!(text.as_str(), "hello world");
}

#[test]
fn test_buffer_modified_flag() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    assert!(!page.is_modified());

    // Setting text triggers modified
    buffer.set_text("new content");
    assert!(page.is_modified());

    // Clearing modified flag
    buffer.set_modified(false);
    assert!(!page.is_modified());
}

#[test]
fn test_save_file_no_path_returns_error() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    // save_file_async calls callback synchronously when no path is set
    let result: std::rc::Rc<
        std::cell::RefCell<Option<Result<(), lushtext_core::ui::editor_page::SaveError>>>,
    > = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result_clone = result.clone();
    page.save_file_async(move |r| {
        *result_clone.borrow_mut() = Some(r);
    });
    let result = result.borrow_mut().take().expect("expected operation to succeed");
    assert!(matches!(
        result,
        Err(lushtext_core::ui::editor_page::SaveError::NoPath)
    ));
}

#[test]
fn test_save_file_writes_content() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    // Manually set the file path (simulating load_file_async without the async part)
    let tmp = tempfile::NamedTempFile::new().expect("expected operation to succeed");
    let path = tmp.path().to_path_buf();

    // Set path via the internal RefCell (load_file_async sets this synchronously)
    page.imp().file_path.replace(Some(path.clone()));

    // Set buffer content
    buffer.set_text("saved content");

    // Save and verify — spin main loop to process the background thread callback
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    page.save_file_async(move |r| {
        r.expect("expected operation to succeed");
        done_clone.set(true);
    });
    while !done.get() {
        glib::MainContext::default().iteration(true);
    }
    let saved = std::fs::read_to_string(&path).expect("expected operation to succeed");
    assert_eq!(saved, "saved content");

    // Buffer should no longer be modified after save
    assert!(!page.is_modified());
}

#[test]
fn test_save_keeps_document_dirty_until_background_write_finishes() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    let tmp = tempfile::NamedTempFile::new().expect("expected operation to succeed");
    let path = tmp.path().to_path_buf();
    let content = "x".repeat(70_000);

    page.imp().file_path.replace(Some(path.clone()));
    page.imp().file_size.set(Some(10_000_000));
    buffer.set_text(&content);

    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    page.save_file_async(move |r| {
        r.expect("expected operation to succeed");
        done_clone.set(true);
    });

    assert!(page.is_saving());
    assert!(page.is_modified());
    assert!(!page.source_view().is_editable());

    wait_until(std::time::Duration::from_secs(2), || done.get());
    assert!(!page.is_saving());
    assert!(!page.is_modified());
    assert!(page.source_view().is_editable());
    assert_eq!(
        std::fs::read_to_string(path).expect("expected operation to succeed"),
        content
    );
}

#[test]
fn test_save_rejects_duplicate_while_first_save_is_in_progress() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let tmp = tempfile::NamedTempFile::new().expect("expected operation to succeed");

    page.imp().file_path.replace(Some(tmp.path().to_path_buf()));
    page.imp().file_size.set(Some(10_000_000));
    page.buffer().set_text(&"x".repeat(70_000));

    let first_done = std::rc::Rc::new(std::cell::Cell::new(false));
    let first_done_clone = first_done.clone();
    page.save_file_async(move |r| {
        r.expect("expected operation to succeed");
        first_done_clone.set(true);
    });

    let duplicate_result: std::rc::Rc<
        std::cell::RefCell<Option<Result<(), lushtext_core::ui::editor_page::SaveError>>>,
    > = std::rc::Rc::new(std::cell::RefCell::new(None));
    let duplicate_result_clone = duplicate_result.clone();
    page.save_file_async(move |r| {
        *duplicate_result_clone.borrow_mut() = Some(r);
    });

    let duplicate_result = duplicate_result
        .borrow_mut()
        .take()
        .expect("duplicate save should finish synchronously");
    assert!(matches!(
        duplicate_result,
        Err(lushtext_core::ui::editor_page::SaveError::SaveInProgress)
    ));

    wait_until(std::time::Duration::from_secs(2), || first_done.get());
}

#[test]
fn test_failed_save_restores_previous_modified_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let dir = tempfile::TempDir::new().expect("expected operation to succeed");

    page.imp().file_path.replace(Some(dir.path().to_path_buf()));
    page.buffer().set_text("unsaved content");

    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    page.save_file_async(move |r| {
        assert!(r.is_err());
        done_clone.set(true);
    });

    wait_until(std::time::Duration::from_secs(2), || done.get());
    assert!(!page.is_saving());
    assert!(page.is_modified());
    assert!(page.source_view().is_editable());
}

#[test]
fn test_title_with_file_path() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.imp()
        .file_path
        .replace(Some("/home/user/project/main.rs".into()));

    assert_eq!(page.title(), "main.rs");
}

#[test]
fn test_show_search_reveals_search_bar() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    let revealer = &page.imp().search_revealer;

    // Initially hidden
    assert!(!revealer.reveals_child());

    // Show search
    page.show_search();
    assert!(revealer.reveals_child());

    // show_search is NOT a toggle — calling it again keeps the bar open
    page.show_search();
    assert!(revealer.reveals_child());
}

#[test]
fn test_minimap_source_map_matches_upstream_geometry_contract() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let source_map = page
        .imp()
        .minimap
        .source_map
        .borrow()
        .as_ref()
        .cloned()
        .expect("source map should be created during construction");
    let baseline_map = baseline_source_map_for_view(page.source_view());

    assert_eq!(source_map.top_margin(), baseline_map.top_margin());
    assert_eq!(source_map.bottom_margin(), baseline_map.bottom_margin());
    assert_eq!(source_map.left_margin(), baseline_map.left_margin());
    assert_eq!(source_map.right_margin(), baseline_map.right_margin());
    assert_eq!(source_map.overflow(), baseline_map.overflow());
    assert!(!source_map.can_focus());
}

#[test]
fn test_minimap_source_map_keeps_native_navigation_controller_set() {
    ensure_gtk_init();

    let page = LushtextEditorPage::new();
    let source_map = page
        .imp()
        .minimap
        .source_map
        .borrow()
        .as_ref()
        .cloned()
        .expect("source map should be created during construction");

    let baseline_view = sourceview5::View::new();
    let baseline_map = baseline_source_map_for_view(&baseline_view);

    assert_eq!(
        minimap_controller_types(&source_map),
        minimap_controller_types(&baseline_map),
        "the editor minimap should not add app-owned click/drag controller overrides on top of GtkSourceMap"
    );
}

#[test]
fn test_minimap_slider_css_uses_neutral_viewport_colors() {
    let css = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/style/style.css"
    ));
    let selector = ".minimap-shell textview.GtkSourceMap slider,\n.minimap-view slider";
    let (_, after_selector) = css
        .split_once(selector)
        .expect("minimap slider rule should exist");
    let (rule_body, _) = after_selector
        .split_once('}')
        .expect("minimap slider rule should be closed");

    assert!(
        rule_body.contains("@view_fg_color"),
        "minimap viewport indicator should use neutral editor-chrome color tokens"
    );
    assert!(
        !rule_body.contains("@accent_color") && !rule_body.contains("@accent_bg_color"),
        "minimap viewport indicator should not use accent color tokens"
    );
}

#[test]
fn test_editor_page_adds_dynamic_eof_overscroll_after_allocation() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let _window = present_editor_page(&page);

    wait_until(std::time::Duration::from_secs(2), || {
        page.source_view().bottom_margin() > 6
    });

    let visible_rect = page.source_view().visible_rect();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The test mirrors the production overscroll rounding from a GTK-provided i32 visible height"
    )]
    let expected_margin = ((f64::from(visible_rect.height()) * 0.75).round() as i32).max(6);

    assert_eq!(page.source_view().bottom_margin(), expected_margin);
}

#[test]
fn test_minimap_source_map_inherits_dynamic_eof_tail_geometry() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let _window = present_editor_page(&page);

    let source_map = page
        .imp()
        .minimap
        .source_map
        .borrow()
        .as_ref()
        .cloned()
        .expect("source map should be created during construction");

    wait_until(std::time::Duration::from_secs(2), || {
        source_map.bottom_margin() > 6
    });

    let baseline_map = baseline_source_map_for_view(page.source_view());
    assert_eq!(source_map.bottom_margin(), baseline_map.bottom_margin());
    assert!(source_map.bottom_margin() > 6);
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    // Verify Default impl works (it delegates to new())
    let _page: LushtextEditorPage = LushtextEditorPage::default();
}

#[test]
fn test_bookmark_toggle_and_navigation() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    buffer.set_text("one\ntwo\nthree\nfour\nfive\n");

    let line_two = buffer.iter_at_line(1).expect("expected operation to succeed");
    buffer.place_cursor(&line_two);
    assert_eq!(
        page.toggle_bookmark_at_cursor(),
        BookmarkToggleState::Added(1)
    );

    let line_five = buffer.iter_at_line(4).expect("expected operation to succeed");
    buffer.place_cursor(&line_five);
    assert_eq!(
        page.toggle_bookmark_at_cursor(),
        BookmarkToggleState::Added(4)
    );

    assert_eq!(
        page.bookmark_records()
            .into_iter()
            .map(|bookmark| bookmark.line)
            .collect::<Vec<_>>(),
        vec![1, 4]
    );

    let line_one = buffer.iter_at_line(0).expect("expected operation to succeed");
    buffer.place_cursor(&line_one);
    let jumped = page
        .navigate_bookmark(BookmarkNavigationDirection::Next)
        .expect("expected operation to succeed");
    assert_eq!(jumped.line, 1);
    assert_eq!(page.cursor_position().0, 1);

    let wrapped = page
        .navigate_bookmark(BookmarkNavigationDirection::Previous)
        .expect("expected operation to succeed");
    assert_eq!(wrapped.line, 4);
    assert_eq!(page.cursor_position().0, 4);
}

// --- Search bar integration ---

#[test]
fn test_stop_search_does_not_fire_during_grab_focus() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let fired = std::rc::Rc::new(std::cell::Cell::new(false));
    let fired_clone = fired.clone();

    // Attach a spy to detect unexpected stop-search emissions
    page.imp()
        .search_bar
        .search_entry()
        .connect_stop_search(move |_| {
            fired_clone.set(true);
        });

    // Show the search bar (which calls grab_focus on the entry)
    page.show_search();
    while glib::MainContext::default().iteration(false) {}

    assert!(
        !fired.get(),
        "stop-search should NOT fire during grab_focus"
    );
    assert!(
        page.imp().search_revealer.reveals_child(),
        "search bar should remain visible"
    );
}

#[test]
fn test_search_bar_starts_hidden() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    assert!(!page.imp().search_revealer.reveals_child());
}

#[test]
fn test_close_button_hides_search() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    // Show the search bar
    page.show_search();

    assert!(page.imp().search_revealer.reveals_child());

    // Click the close button (calls hide_search internally)
    page.imp().search_bar.close_button().emit_clicked();

    // Search bar should be hidden
    assert!(!page.imp().search_revealer.reveals_child());
}

#[test]
fn test_warning_inline_alert_wraps_titles_and_action_labels() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Changes Restored".to_string(),
        body: "Unsaved changes to the document have been restored.".to_string(),
        primary_button: Some("_Discard…".to_string()),
        secondary_button: Some("_Save…".to_string()),
    });

    let imp = page.info_bar().imp();
    assert!(
        imp.alert_revealer.reveals_child(),
        "warning inline alert should be shown"
    );
    assert!(imp.alert_box.has_css_class("warning"));
    assert!(!imp.alert_box.has_css_class("error"));
    assert!(imp.alert_title.wraps(), "warning title should wrap");
    assert_eq!(imp.alert_title.wrap_mode(), gtk4::pango::WrapMode::WordChar);
    assert!(imp.alert_body.wraps(), "warning body should wrap");
    assert_eq!(
        imp.alert_body.wrap_mode(),
        gtk4::pango::WrapMode::WordChar
    );

    let discard_label = button_label(&imp.discard_button);
    assert!(discard_label.wraps(), "discard action label should wrap");
    assert_eq!(
        discard_label.wrap_mode(),
        gtk4::pango::WrapMode::WordChar
    );
    assert_eq!(
        discard_label.justify(),
        gtk4::Justification::Center,
        "discard action label should stay centered when it wraps"
    );

    let save_label = button_label(&imp.save_button);
    assert!(save_label.wraps(), "save action label should wrap");
    assert_eq!(save_label.wrap_mode(), gtk4::pango::WrapMode::WordChar);
    assert_eq!(save_label.justify(), gtk4::Justification::Center);
}

#[test]
fn test_warning_inline_alert_groups_workflow_actions_and_dismiss() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Changes Restored".to_string(),
        body: "Unsaved changes to the document have been restored.".to_string(),
        primary_button: Some("_Discard...".to_string()),
        secondary_button: Some("_Save...".to_string()),
    });

    let imp = page.info_bar().imp();
    assert!(imp.actions_box.property::<bool>("visible"));
    assert!(
        same_widget(
            &imp.dismiss_button.parent().expect("dismiss button parent"),
            &*imp.actions_box,
        ),
        "dismiss should be part of the same horizontal action row"
    );
    assert_eq!(
        visible_alert_action_order(&page),
        vec!["discard", "save", "dismiss"],
        "Draft Changes Restored should group Discard, Save, and dismiss in order"
    );
}

#[test]
fn test_error_inline_alert_groups_retry_and_dismiss() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Error,
        title: "Could Not Open File".to_string(),
        body: "Permission denied".to_string(),
        primary_button: Some("_Retry".to_string()),
        secondary_button: None,
    });

    let imp = page.info_bar().imp();
    assert!(imp.actions_box.property::<bool>("visible"));
    assert!(
        same_widget(
            &imp.dismiss_button.parent().expect("dismiss button parent"),
            &*imp.actions_box,
        ),
        "dismiss should stay in the same horizontal action row for errors"
    );
    assert_eq!(
        visible_alert_action_order(&page),
        vec!["retry", "dismiss"],
        "error alerts should group Retry and dismiss in order"
    );
}

#[test]
fn test_document_restored_inline_alert_keeps_save_as_visible() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Document Restored".to_string(),
        body: "Unsaved document has been restored.".to_string(),
        primary_button: None,
        secondary_button: Some("Save _As…".to_string()),
    });

    let imp = page.info_bar().imp();
    assert!(
        imp.alert_revealer.reveals_child(),
        "restored-document inline alert should be shown"
    );
    assert!(
        !imp.discard_button.property::<bool>("visible"),
        "untitled restore should not expose discard"
    );
    assert!(
        imp.save_button.property::<bool>("visible"),
        "Save As must stay visible"
    );

    let save_label = button_label(&imp.save_button);
    assert_eq!(save_label.label(), "Save _As…");
    assert!(
        save_label.wraps(),
        "Save As label should wrap instead of disappearing"
    );
}

#[test]
fn test_inline_alert_action_callbacks_are_routed() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let retry_clicked = Rc::new(Cell::new(false));
    let discard_clicked = Rc::new(Cell::new(false));
    let save_clicked = Rc::new(Cell::new(false));

    page.info_bar().connect_retry({
        let retry_clicked = retry_clicked.clone();
        move || retry_clicked.set(true)
    });
    page.info_bar().connect_discard({
        let discard_clicked = discard_clicked.clone();
        move || discard_clicked.set(true)
    });
    page.info_bar().connect_save({
        let save_clicked = save_clicked.clone();
        move || save_clicked.set(true)
    });

    page.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Error,
        title: "Could Not Open File".to_string(),
        body: "Permission denied".to_string(),
        primary_button: Some("_Retry".to_string()),
        secondary_button: None,
    });
    page.info_bar().imp().retry_button.emit_clicked();
    assert!(retry_clicked.get(), "retry callback should fire");

    page.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Changes Restored".to_string(),
        body: "Unsaved changes were restored.".to_string(),
        primary_button: Some("_Discard...".to_string()),
        secondary_button: Some("_Save...".to_string()),
    });
    page.info_bar().imp().discard_button.emit_clicked();
    page.info_bar().imp().save_button.emit_clicked();
    assert!(discard_clicked.get(), "discard callback should fire");
    assert!(save_clicked.get(), "save callback should fire");
}

#[test]
fn test_warning_inline_alert_without_workflow_actions_keeps_only_dismiss_action() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.emit_inline_notification(InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Not Restored".to_string(),
        body: "The file changed on disk.".to_string(),
        primary_button: None,
        secondary_button: None,
    });

    let imp = page.info_bar().imp();
    assert!(imp.alert_revealer.reveals_child());
    assert!(imp.actions_box.property::<bool>("visible"));
    assert!(!imp.retry_button.property::<bool>("visible"));
    assert!(!imp.discard_button.property::<bool>("visible"));
    assert!(!imp.save_button.property::<bool>("visible"));
    assert!(imp.dismiss_button.property::<bool>("visible"));
    assert_eq!(
        visible_alert_action_order(&page),
        vec!["dismiss"],
        "warnings without workflow actions should still expose dismiss in the action row"
    );
}

#[test]
fn test_escape_hides_search() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    // Show the search bar
    page.show_search();

    assert!(page.imp().search_revealer.reveals_child());

    // Emit stop-search (Escape key fires close callback)
    page.imp().search_bar.search_entry().emit_stop_search();

    // Search bar should be hidden
    assert!(!page.imp().search_revealer.reveals_child());
}

#[test]
fn test_search_show_hide_cycle() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    // Cycle: show → close → show → escape → show → hide
    page.show_search();
    assert!(page.imp().search_revealer.reveals_child());

    page.imp().search_bar.close_button().emit_clicked();
    assert!(!page.imp().search_revealer.reveals_child());

    page.show_search();
    assert!(page.imp().search_revealer.reveals_child());

    page.imp().search_bar.search_entry().emit_stop_search();
    assert!(!page.imp().search_revealer.reveals_child());

    page.show_search();
    assert!(page.imp().search_revealer.reveals_child());

    page.hide_search();
    assert!(!page.imp().search_revealer.reveals_child());
}

// --- GSettings integration ---

#[test]
fn test_settings_word_wrap_default_enabled() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    // Schema default: word-wrap = true → WrapMode::Word
    assert_eq!(page.source_view().wrap_mode(), gtk4::WrapMode::Word);
}

#[test]
fn test_settings_show_line_numbers_default() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert!(page.source_view().shows_line_numbers());
}

#[test]
fn test_settings_highlight_current_line_default() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert!(page.source_view().is_highlight_current_line());
}

#[test]
fn test_settings_tab_width_default() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert_eq!(page.source_view().tab_width(), 4);
}

#[test]
fn test_settings_insert_spaces_default() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert!(page.source_view().is_insert_spaces_instead_of_tabs());
}

// --- set_file_path ---

#[test]
fn test_set_file_path_updates_path() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert!(page.file_path().is_none());

    let path = std::path::PathBuf::from("/tmp/test_file.rs");
    page.set_file_path(&path);
    assert_eq!(page.file_path(), Some(path));
}

#[test]
fn test_set_file_path_updates_title() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    assert_eq!(page.title(), "Untitled");

    page.set_file_path(std::path::Path::new("/home/user/project/hello.rs"));
    assert_eq!(page.title(), "hello.rs");
}

#[test]
fn test_set_file_path_detects_language() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    // No language detected for untitled
    assert!(page.buffer().language().is_none());

    // Setting a .rs path should detect Rust language
    page.set_file_path(std::path::Path::new("/tmp/test.rs"));
    if let Some(lang) = page.buffer().language() {
        assert_eq!(lang.id().as_str(), "rust");
    }
    // If Rust language spec is not installed, just verify no panic
}

#[test]
fn test_set_file_path_overwrites_previous() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.set_file_path(std::path::Path::new("/a/first.txt"));
    assert_eq!(page.title(), "first.txt");

    page.set_file_path(std::path::Path::new("/b/second.rs"));
    assert_eq!(page.title(), "second.rs");
}

// --- Search bar cursor restore regression tests ---

#[test]
fn test_hide_search_restores_cursor_position() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    // Set up content and place cursor in the middle.
    buffer.set_text("line1\nline2\nline3\nline4\nline5");
    if let Some(mut iter) = buffer.iter_at_line(2) {
        iter.forward_chars(3);
        buffer.place_cursor(&iter);
    }
    let (pre_line, pre_col) = page.cursor_position();
    assert_eq!(pre_line, 2);
    assert_eq!(pre_col, 3);

    // Show then hide — cursor should return to (2, 3).
    page.show_search();
    page.hide_search();

    let (post_line, post_col) = page.cursor_position();
    assert_eq!(post_line, pre_line, "cursor line changed after close");
    assert_eq!(post_col, pre_col, "cursor column changed after close");
}

#[test]
fn test_hide_search_cleans_up_pre_search_mark() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    assert!(
        buffer.mark("pre-search-cursor").is_none(),
        "mark should not exist before search"
    );

    page.show_search();
    assert!(
        buffer.mark("pre-search-cursor").is_some(),
        "mark should exist while search is open"
    );

    page.hide_search();
    assert!(
        buffer.mark("pre-search-cursor").is_none(),
        "mark should be deleted after search closes"
    );
}

#[test]
fn test_close_button_restores_cursor_position() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    buffer.set_text("aaa\nbbb\nccc\nddd");
    if let Some(iter) = buffer.iter_at_line(1) {
        buffer.place_cursor(&iter);
    }
    let (pre_line, _) = page.cursor_position();

    page.show_search();
    // Close via the close button (the path that was scrolling to the end).
    page.imp().search_bar.close_button().emit_clicked();

    let (post_line, _) = page.cursor_position();
    assert_eq!(
        post_line, pre_line,
        "close button should restore cursor to pre-search line"
    );
}

#[test]
fn test_escape_restores_cursor_position() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    buffer.set_text("first\nsecond\nthird");
    if let Some(iter) = buffer.iter_at_line(2) {
        buffer.place_cursor(&iter);
    }
    let (pre_line, _) = page.cursor_position();

    page.show_search();
    // Close via Escape (stop-search signal).
    page.imp().search_bar.search_entry().emit_stop_search();

    let (post_line, _) = page.cursor_position();
    assert_eq!(
        post_line, pre_line,
        "Escape should restore cursor to pre-search line"
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextEditorPage widget.

use crate::common::{
    ensure_gtk_init, fixture, flush_after_delay, fs_read, isolated_data_dir, present_window,
    test_application, wait_until,
};
use gio::prelude::ListModelExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::config::{APP_ID, keys};
use lushtext_core::model::encoding::DocumentEncodingState;
use lushtext_core::model::editor_memory::EVICTED_EDITOR_BOOKKEEPING_BYTES;
use lushtext_core::services::editor_io::LoadResult;
use lushtext_core::services::file_limits::FileSizeCheck;
use lushtext_core::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use lushtext_core::services::local_history_service;
use lushtext_core::ui::accessibility::{AnnouncementLane, test_audit::AccessibleAudit};
use lushtext_core::ui::editor_page::{
    BookmarkEditError, BookmarkNavigationDirection, BookmarkToggleState, EditorLoadState,
    LushtextEditorPage, MinimapAvailability, MinimapMarkerKind,
};
use lushtext_core::ui::info_bar::inline_alert_announcement_key_for_test;
use sourceview5::prelude::*;
use std::assert_matches;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

fn button_label(button: &gtk4::Button) -> gtk4::Label {
    button
        .child()
        .expect("button child")
        .downcast::<gtk4::Label>()
        .expect("button label")
}

fn editor_buffer_text(page: &LushtextEditorPage) -> String {
    let buffer = page.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
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
    map.set_top_margin(5);
    map.set_bottom_margin(view.bottom_margin());
    map.set_left_margin(0);
    map.set_right_margin(0);
    map.set_overflow(gtk4::Overflow::Visible);
    map.add_css_class("monospace");
    map.add_css_class("minimap-view");
    map.set_hexpand(true);
    map.set_vexpand(true);
    map
}

fn enable_minimap_for_tests(long_line_markers: bool) -> gio::Settings {
    let settings = gio::Settings::new(APP_ID);
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("enable minimap");
    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, long_line_markers)
        .expect("set long-line minimap markers");
    settings
}

fn minimap_source_map(page: &LushtextEditorPage) -> sourceview5::Map {
    page.imp()
        .minimap
        .source_map
        .borrow()
        .as_ref()
        .cloned()
        .expect("source map should be created during construction")
}

fn minimap_marker_strip(page: &LushtextEditorPage) -> gtk4::DrawingArea {
    page.imp()
        .minimap
        .marker_strip
        .borrow()
        .as_ref()
        .cloned()
        .expect("marker strip should be created during construction")
}

fn present_editor_page(page: &LushtextEditorPage) -> gtk4::ApplicationWindow {
    present_editor_page_with_size(page, 1000, 800)
}

fn present_editor_page_with_size(
    page: &LushtextEditorPage,
    width: i32,
    height: i32,
) -> gtk4::ApplicationWindow {
    let app = test_application();
    let window = gtk4::ApplicationWindow::builder()
        .application(&app)
        .default_width(width)
        .default_height(height)
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
        gtk4::AccessibleRole::Group,
        "inline alert surface should keep sibling actions reachable to assistive technology"
    );
    assert_eq!(
        page.info_bar().imp().alert_title.accessible_role(),
        gtk4::AccessibleRole::Alert,
        "inline alert title should keep high-priority alert semantics for assistive technology"
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
fn test_inline_alert_uses_balanced_compact_padding() {
    let css = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/style/style.css"
    ));
    let alert_block = css
        .split(".editor-inline-alert {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .expect("inline-alert CSS block should exist");

    assert!(
        alert_block.contains("padding: 6px 12px;"),
        "inline alert should keep balanced compact vertical padding"
    );
    assert!(
        !alert_block.contains("padding: 8px 12px 4px;"),
        "inline alert should not use asymmetric one-off alignment padding"
    );
    assert!(
        alert_block.contains("border-bottom: 1px solid @borders;"),
        "inline alert should preserve the existing bottom divider"
    );
}

// --- Adaptive inline-alert layout (AdwWrapBox) -----------------------------

fn warning_notification() -> InlineActionNotification {
    InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Changes Restored".to_string(),
        body: "Unsaved changes from a previous session have been restored.".to_string(),
        primary_button: Some("_Discard…".to_string()),
        secondary_button: Some("_Save…".to_string()),
    }
}

fn present_editor_page_sized(page: &LushtextEditorPage, width: i32) -> gtk4::ApplicationWindow {
    let app = test_application();
    let window = gtk4::ApplicationWindow::builder()
        .application(&app)
        .default_width(width)
        .default_height(700)
        .child(page)
        .build();
    present_window(&window);
    wait_until(std::time::Duration::from_secs(2), || {
        page.source_view().is_mapped() && page.source_view().visible_rect().height() > 0
    });
    window
}

/// Bounds of `widget` expressed in the coordinate space of `ancestor`.
fn rect_within(
    widget: &impl IsA<gtk4::Widget>,
    ancestor: &impl IsA<gtk4::Widget>,
) -> gtk4::graphene::Rect {
    widget
        .compute_bounds(ancestor)
        .expect("widget should have computable bounds within the ancestor")
}

fn wait_for_minimap_ready(page: &LushtextEditorPage) {
    wait_until(std::time::Duration::from_secs(2), || {
        let source_map = minimap_source_map(page);
        let marker_strip = minimap_marker_strip(page);
        page.is_minimap_visible()
            && source_map.is_mapped()
            && marker_strip.is_mapped()
            && marker_strip.height() > 0
            && source_map.height() > 0
            && source_map.bottom_margin() > 6
    });
}

fn minimap_test_document(
    line_count: usize,
    needle_lines: &[usize],
    long_line_lines: &[usize],
) -> String {
    let long_tail = "x".repeat(150);
    (0..line_count)
        .map(|line| {
            let mut text = format!("line {line:03}");
            if needle_lines.contains(&line) {
                text.push_str(" needle");
            }
            if long_line_lines.contains(&line) {
                text.push(' ');
                text.push_str(&long_tail);
            }
            text
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn show_search_and_wait_for_minimap_marker(page: &LushtextEditorPage, needle: &str) {
    page.show_search();
    page.search_bar().search_entry().set_text(needle);
    wait_until(std::time::Duration::from_secs(2), || {
        page.minimap_marker_count(MinimapMarkerKind::Search) > 0
            && !page
                .minimap_marker_bounds(MinimapMarkerKind::Search)
                .is_empty()
    });
}

fn source_map_content_bottom(page: &LushtextEditorPage) -> f64 {
    let source_map = minimap_source_map(page);
    let marker_strip = minimap_marker_strip(page);
    let map_bounds = source_map
        .compute_bounds(&marker_strip)
        .expect("source map should have marker-strip-relative bounds");
    let buffer = source_map.buffer();
    let end_iter = buffer.end_iter();
    let last_line = buffer.iter_at_line(end_iter.line()).unwrap_or(end_iter);
    let (line_y, line_height) = source_map.line_yrange(&last_line);
    let (_, widget_y) = source_map.buffer_to_window_coords(
        gtk4::TextWindowType::Widget,
        0,
        line_y.saturating_add(line_height.max(0)),
    );

    f64::from(map_bounds.y()) + f64::from(widget_y)
}

fn assert_marker_bounds_within_source_content(
    page: &LushtextEditorPage,
    kind: MinimapMarkerKind,
) -> Vec<lushtext_core::ui::editor_page::MinimapMarkerBounds> {
    let bounds = page.minimap_marker_bounds(kind);
    assert!(!bounds.is_empty(), "expected projected {kind:?} marker bounds");

    let content_bottom = source_map_content_bottom(page);
    let strip_height = f64::from(minimap_marker_strip(page).height());
    for bound in &bounds {
        assert!(
            bound.top >= -0.5,
            "{kind:?} marker should not start above the marker strip: {bound:?}"
        );
        assert!(
            bound.bottom <= content_bottom + 0.5,
            "{kind:?} marker should stop at rendered content bottom {content_bottom}, got {bound:?}"
        );
        assert!(
            bound.bottom <= strip_height + 0.5,
            "{kind:?} marker should remain inside strip height {strip_height}, got {bound:?}"
        );
        assert!(
            bound.height() > 0.0,
            "{kind:?} marker should have positive height: {bound:?}"
        );
    }

    bounds
}

#[test]
fn test_inline_alert_uses_adw_wrap_box_container() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let imp = page.info_bar().imp();

    // The message and action group are the two children of one AdwWrapBox.
    assert_eq!(
        imp.content_wrap.get().type_().name(),
        "AdwWrapBox",
        "inline alert should host its content in an AdwWrapBox"
    );
    assert!(
        same_widget(
            &imp.actions_box.parent().expect("actions parent"),
            &imp.content_wrap.get(),
        ),
        "action group should be a single direct (atomic) child of the wrap box"
    );

    let names: Vec<_> = descendants(page.info_bar())
        .into_iter()
        .map(|w| w.type_().name().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n == "AdwWrapBox"),
        "adaptive layout should use the AdwWrapBox container"
    );
    assert!(
        !names.iter().any(|n| n == "GtkInfoBar"),
        "adaptive layout must not reintroduce GtkInfoBar"
    );
}

#[test]
fn test_inline_alert_message_and_actions_share_row_when_wide() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let _window = present_editor_page_sized(&page, 1000);

    let imp = page.info_bar().imp();
    // The widget harness does not advance the revealer's slide-animation frame
    // clock, so reveal instantly to obtain a real allocation for geometry checks.
    imp.alert_revealer.set_transition_duration(0);
    page.emit_inline_notification(warning_notification());
    wait_until(std::time::Duration::from_secs(2), || {
        imp.actions_box.width() > 0 && imp.discard_button.width() > 0
    });

    let wrap = imp.content_wrap.get();
    let message_box = wrap.first_child().expect("message box");
    let msg = rect_within(&message_box, &wrap);
    let act = rect_within(&*imp.actions_box, &wrap);

    // Same row: the action group overlaps the message vertically instead of
    // sitting on its own line beneath it.
    assert!(
        act.y() < msg.y() + msg.height(),
        "wide editor: actions should share the message row (msg y={} h={}, act y={})",
        msg.y(),
        msg.height(),
        act.y()
    );
    // Trailing: the action group sits to the right of the message.
    assert!(
        act.x() >= msg.x() + msg.width() - 1.0,
        "wide editor: actions should trail the message (msg x={} w={}, act x={})",
        msg.x(),
        msg.width(),
        act.x()
    );

    // Action buttons stay grouped with positive allocation.
    for (name, button) in [
        ("discard", &*imp.discard_button),
        ("save", &*imp.save_button),
        ("dismiss", &*imp.dismiss_button),
    ] {
        assert!(button.property::<bool>("visible"), "{name} should be visible");
        assert!(
            button.width() > 0 && button.height() > 0,
            "{name} should have a positive allocation"
        );
        assert!(
            same_widget(&button.parent().expect("button parent"), &*imp.actions_box),
            "{name} should stay in the single horizontal action group"
        );
    }
}

#[test]
fn test_inline_alert_actions_wrap_below_message_when_narrow() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let _window = present_editor_page_sized(&page, 360);

    let imp = page.info_bar().imp();
    // The widget harness does not advance the revealer's slide-animation frame
    // clock, so reveal instantly to obtain a real allocation for geometry checks.
    imp.alert_revealer.set_transition_duration(0);
    page.emit_inline_notification(warning_notification());
    wait_until(std::time::Duration::from_secs(2), || {
        imp.actions_box.width() > 0 && imp.discard_button.width() > 0
    });

    let wrap = imp.content_wrap.get();
    let message_box = wrap.first_child().expect("message box");
    let msg = rect_within(&message_box, &wrap);
    let act = rect_within(&*imp.actions_box, &wrap);

    // Wrapped: the action group sits on its own row beneath the message.
    assert!(
        act.y() >= msg.y() + msg.height() - 1.0,
        "narrow editor: actions should wrap beneath the message (msg y={} h={}, act y={})",
        msg.y(),
        msg.height(),
        act.y()
    );

    // The action group stays one horizontal row with positive per-button
    // allocation even after wrapping (AdwWrapBox treats it as one atomic child).
    let discard = rect_within(&*imp.discard_button, &*imp.actions_box);
    let save = rect_within(&*imp.save_button, &*imp.actions_box);
    let dismiss = rect_within(&*imp.dismiss_button, &*imp.actions_box);
    for (name, r) in [("discard", discard), ("save", save), ("dismiss", dismiss)] {
        assert!(
            r.width() > 0.0 && r.height() > 0.0,
            "{name} should have a positive allocation"
        );
    }
    assert!(
        (discard.y() - save.y()).abs() < 1.0 && (save.y() - dismiss.y()).abs() < 1.0,
        "wrapped action buttons must stay on a single row (discard y={}, save y={}, dismiss y={})",
        discard.y(),
        save.y(),
        dismiss.y()
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

    assert!(view.is_visible());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TextBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::MultiLine,
            gtk4::AccessibleProperty::ReadOnly,
        ])
        .assert_on(view);
    assert!(!gtk4::test_accessible_has_state(
        view,
        gtk4::AccessibleState::Busy
    ));
}

#[test]
fn test_source_view_accessibility_tracks_loading_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.imp()
        .file_path
        .replace(Some("/tmp/accessibility-main.rs".into()));
    page.imp().load_state.set(EditorLoadState::Loading);
    page.refresh_accessibility_metadata_for_test();

    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ReadOnly,
        ])
        .states(&[gtk4::AccessibleState::Busy])
        .assert_on(page.source_view());

    page.imp().load_state.set(EditorLoadState::Loaded);
    page.refresh_accessibility_metadata_for_test();
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Busy
    ));
}

#[test]
fn test_source_view_accessibility_tracks_failed_load_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.imp()
        .file_path
        .replace(Some("/tmp/broken-document.txt".into()));
    page.imp().load_state.set(EditorLoadState::Failed);
    page.refresh_accessibility_metadata_for_test();

    AccessibleAudit::new()
        .states(&[
            gtk4::AccessibleState::Disabled,
            gtk4::AccessibleState::Invalid,
        ])
        .assert_on(page.source_view());

    page.imp().load_state.set(EditorLoadState::Loaded);
    page.refresh_accessibility_metadata_for_test();
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Disabled
    ));
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Invalid
    ));
}

#[test]
fn test_source_view_accessibility_tracks_preview_only_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.set_preview_only_accessibility_for_test(true);
    AccessibleAudit::new()
        .states(&[
            gtk4::AccessibleState::Disabled,
            gtk4::AccessibleState::Hidden,
        ])
        .assert_on(page.source_view());

    page.set_preview_only_accessibility_for_test(false);
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Hidden
    ));
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Disabled
    ));
}

#[test]
fn test_source_view_accessibility_keeps_large_file_editable_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.apply_loaded_content_for_test("large file marker", 50_000_001);

    assert_eq!(page.size_check(), FileSizeCheck::DisableUndoAndSyntax);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ReadOnly,
        ])
        .assert_on(page.source_view());
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Disabled
    ));
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
        std::cell::RefCell<Option<Result<(), lushtext_core::ui::editor_page::EditorSaveError>>>,
    > = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result_clone = result.clone();
    page.save_file_async(move |r| {
        *result_clone.borrow_mut() = Some(r);
    });
    let result = result.borrow_mut().take().expect("expected operation to succeed");
    assert_matches!(
        result,
        Err(lushtext_core::ui::editor_page::EditorSaveError::NoPath)
    );
}

#[test]
fn test_save_file_writes_content() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    let tmp = tempfile::NamedTempFile::new().expect("expected operation to succeed");
    let path = tmp.path().to_path_buf();

    page.imp().file_path.replace(Some(path.clone()));

    buffer.set_text("saved content");

    // Spin the main loop until the background save callback runs.
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    page.save_file_async(move |r| {
        r.expect("expected operation to succeed");
        done_clone.set(true);
    });
    while !done.get() {
        glib::MainContext::default().iteration(true);
    }
    let saved = fs_read::text(&path).expect("expected operation to succeed");
    assert_eq!(saved, "saved content");

    // Buffer should no longer be modified after save
    assert!(!page.is_modified());
}

#[test]
fn test_large_untitled_buffer_uses_chunked_save_snapshot_policy() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&"x".repeat(2_500_000));

    assert!(
        page.save_uses_chunked_snapshot_for_test(),
        "large untitled buffers should not be copied in one synchronous snapshot"
    );
}

#[test]
fn test_file_that_grew_in_memory_uses_chunked_save_snapshot_policy() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.imp().file_size.set(Some(1_024));
    page.buffer().set_text(&"x".repeat(2_500_000));

    assert!(
        page.save_uses_chunked_snapshot_for_test(),
        "snapshot policy should follow live buffer size, not only loaded file size"
    );
}

#[test]
fn test_minimap_long_line_warning_scan_preserves_small_document_markers() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&format!("short\n{}", "x".repeat(121)));

    assert_eq!(page.long_line_warning_count_for_test(), 1);
}

#[test]
fn test_minimap_long_line_warning_scan_skips_large_buffer_threshold() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&"x".repeat(2_500_001));

    assert_eq!(
        page.long_line_warning_count_for_test(),
        0,
        "large-buffer minimap warnings should skip the full text copy/scan path"
    );
}

#[test]
fn test_minimap_wrapped_budget_skips_large_buffer_line_scan() {
    ensure_gtk_init();
    let settings = enable_minimap_for_tests(false);
    settings
        .set_boolean(keys::WORD_WRAP, true)
        .expect("enable word wrap");

    let page = LushtextEditorPage::new();
    page.imp().file_size.set(Some(3 * 1024 * 1024));
    page.buffer().set_text(&"x".repeat(2_500_001));
    let _window = present_editor_page_with_size(&page, 1000, 520);

    wait_until(std::time::Duration::from_secs(2), || {
        page.minimap_availability() == MinimapAvailability::TooLarge
    });

    assert!(!page.is_minimap_visible());
}

#[test]
fn test_stale_load_generation_result_does_not_mutate_current_editor_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.buffer().set_text("current buffer\n");

    let stale_generation = page.load_generation_for_test();
    page.cancel_load();
    let stale_result = LoadResult {
        content: "stale disk bytes\n".to_string(),
        size: 17,
        size_check: FileSizeCheck::Normal,
        canonical_path: Some(std::path::PathBuf::from("/tmp/stale.txt")),
        mtime: Some(123),
        encoding_state: DocumentEncodingState::default(),
        has_bom: false,
        file_health: Vec::new(),
    };

    assert!(
        !page.apply_load_result_for_test(stale_generation, Ok(stale_result)),
        "stale load generations should be rejected before touching the editor"
    );
    assert_eq!(editor_buffer_text(&page), "current buffer\n");
    assert_eq!(page.file_size(), None);
}

#[test]
fn test_live_memory_estimate_tracks_untitled_unicode_and_growth() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    page.buffer().set_text("abc");
    assert_eq!(page.estimated_live_buffer_bytes(), 12);

    page.buffer().set_text("é🙂");
    assert_eq!(
        page.estimated_live_buffer_bytes(),
        8,
        "two Unicode scalars use the conservative four-byte bound"
    );

    page.apply_loaded_content_for_test("abc", 100);
    assert_eq!(page.estimated_live_buffer_bytes(), 100);
    page.buffer().set_text(&"x".repeat(30));
    assert_eq!(page.estimated_live_buffer_bytes(), 120);
}

#[test]
fn test_live_memory_estimate_updates_after_save_and_eviction() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let temp = tempfile::NamedTempFile::new().expect("memory estimate temp file");
    page.set_file_path(temp.path());
    page.buffer().set_text("saved unicode 🙂\n");

    let before_save = page.estimated_live_buffer_bytes();
    let done = Rc::new(Cell::new(false));
    let done_clone = done.clone();
    page.save_file_async(move |result| {
        result.expect("memory estimate save should succeed");
        done_clone.set(true);
    });
    wait_until(std::time::Duration::from_secs(5), || done.get());

    assert!(!page.is_saving());
    assert!(page.file_size().is_some());
    assert_eq!(page.estimated_live_buffer_bytes(), before_save);

    page.evict();
    assert_eq!(
        page.estimated_live_buffer_bytes(),
        EVICTED_EDITOR_BOOKKEEPING_BYTES
    );
}

#[test]
fn test_new_load_cancels_previous_token_without_reusing_identity() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("load token tempdir");
    let first_path = dir.path().join("first.txt");
    let second_path = dir.path().join("second.txt");
    fixture::write_text(&first_path, "first\n");
    fixture::write_text(&second_path, "second\n");

    let page = LushtextEditorPage::new();
    page.load_file_async(&first_path);
    let first_token = page.load_cancel_token_for_test();
    assert!(
        !first_token.load(Ordering::Acquire),
        "newly started load token should begin active"
    );

    page.load_file_async(&second_path);
    let second_token = page.load_cancel_token_for_test();

    assert!(
        first_token.load(Ordering::Acquire),
        "starting a newer load must permanently cancel the previous token"
    );
    assert!(
        !second_token.load(Ordering::Acquire),
        "the replacement token should remain active for the newer load"
    );
    assert!(
        !Arc::ptr_eq(&first_token, &second_token),
        "new loads should rotate token identity instead of clearing the old token"
    );
}

#[test]
fn test_large_save_keeps_snapshot_consistent_and_read_only_until_write_finishes() {
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
    assert!(!page.source_view().is_cursor_visible());
    assert!(gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Busy
    ));

    wait_until(std::time::Duration::from_secs(2), || done.get());
    assert!(!page.is_saving());
    assert!(!page.is_modified());
    assert!(page.source_view().is_editable());
    assert!(page.source_view().is_cursor_visible());
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Busy
    ));
    assert_eq!(fs_read::text(&path).expect("expected operation to succeed"), content);
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
    assert!(page.is_saving());
    assert!(!page.source_view().is_editable());
    assert!(!page.source_view().is_cursor_visible());
    assert!(gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Busy
    ));

    let duplicate_result: std::rc::Rc<
        std::cell::RefCell<Option<Result<(), lushtext_core::ui::editor_page::EditorSaveError>>>,
    > = std::rc::Rc::new(std::cell::RefCell::new(None));
    let duplicate_result_clone = duplicate_result.clone();
    page.save_file_async(move |r| {
        *duplicate_result_clone.borrow_mut() = Some(r);
    });

    let duplicate_result = duplicate_result
        .borrow_mut()
        .take()
        .expect("duplicate save should finish synchronously");
    assert_matches!(
        duplicate_result,
        Err(lushtext_core::ui::editor_page::EditorSaveError::SaveInProgress)
    );

    wait_until(std::time::Duration::from_secs(2), || first_done.get());
    assert!(!page.is_saving());
    assert!(page.source_view().is_editable());
    assert!(page.source_view().is_cursor_visible());
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Busy
    ));
    assert_eq!(
        fs_read::text(tmp.path()).expect("saved duplicate test file"),
        "x".repeat(70_000)
    );
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
    assert!(!gtk4::test_accessible_has_state(
        page.source_view(),
        gtk4::AccessibleState::Busy
    ));
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

fn select_buffer_chars(page: &LushtextEditorPage, start_offset: i32, end_offset: i32) {
    let buffer = page.buffer();
    let start = buffer.iter_at_offset(start_offset);
    let end = buffer.iter_at_offset(end_offset);
    buffer.select_range(&start, &end);
}

fn assert_search_query_focused_and_selected(page: &LushtextEditorPage, expected: &str) {
    let entry = page.search_bar().search_entry();
    wait_until(std::time::Duration::from_secs(2), || {
        let Some(window) = page.root().and_downcast::<gtk4::Window>() else {
            return false;
        };
        let mut focus = gtk4::prelude::GtkWindowExt::focus(&window);
        while let Some(widget) = focus {
            if same_widget(&widget, entry) {
                return true;
            }
            focus = widget.parent();
        }
        false
    });
    assert_eq!(entry.text().as_str(), expected);
    assert_eq!(
        entry.selection_bounds(),
        Some((0, i32::try_from(expected.chars().count()).expect("small query"))),
        "the complete existing query should be selected for replacement"
    );
}

#[test]
fn test_search_prefill_empty_and_exact_limit_keep_query_surface_usable() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let window = present_editor_page(&page);
    let entry = page.search_bar().search_entry();

    page.buffer().set_text("plain text");
    entry.set_text("existing");
    page.show_search();
    assert_search_query_focused_and_selected(&page, "existing");

    page.hide_search();
    flush_after_delay(Duration::from_millis(300));
    let exact = "x".repeat(1_024);
    page.buffer().set_text(&exact);
    select_buffer_chars(&page, 0, 1_024);
    page.show_search();
    assert_search_query_focused_and_selected(&page, &exact);

    window.destroy();
}

#[test]
fn test_replace_prefill_rejects_one_over_and_large_selection_without_copying() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let window = present_editor_page(&page);
    let entry = page.search_bar().search_entry();
    let one_over = "z".repeat(1_025);
    page.buffer().set_text(&one_over);
    select_buffer_chars(&page, 0, 1_025);
    entry.set_text("keep me");
    page.show_replace();
    assert!(page.search_bar().imp().replace_mode_button.is_active());
    assert_search_query_focused_and_selected(&page, "keep me");

    window.destroy();

    let large_page = LushtextEditorPage::new();
    let large_entry = large_page.search_bar().search_entry();
    large_page.buffer().set_text(&"z".repeat(100_000));
    select_buffer_chars(&large_page, 0, 100_000);
    large_entry.set_text("still bounded");
    large_page.show_replace();
    assert!(large_page.search_bar().imp().replace_mode_button.is_active());
    assert_eq!(large_entry.text().as_str(), "still bounded");
    assert_eq!(large_entry.selection_bounds(), Some((0, 13)));
}

#[test]
fn test_search_prefill_counts_unicode_scalars_and_repeated_open_does_not_reprefill() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let window = present_editor_page(&page);
    let unicode = "é🙂漢字".repeat(25);
    let unicode_chars = i32::try_from(unicode.chars().count()).expect("small Unicode fixture");
    page.buffer().set_text(&format!("{unicode}later"));
    select_buffer_chars(&page, 0, unicode_chars);

    page.show_search();
    assert_search_query_focused_and_selected(&page, &unicode);

    select_buffer_chars(&page, unicode_chars, unicode_chars + 5);
    page.show_replace();
    assert!(page.search_bar().imp().replace_mode_button.is_active());
    assert_search_query_focused_and_selected(&page, &unicode);

    window.destroy();
}

#[test]
fn test_periodic_local_history_edit_cancels_chunked_snapshot_without_persistence() {
    ensure_gtk_init();
    let data_dir = isolated_data_dir();
    let document = data_dir.path().join("periodic-edit.md");
    fixture::write_text(&document, "saved\n");
    let page = LushtextEditorPage::new();
    page.set_file_path(&document);
    let buffer = page.buffer();
    buffer.set_text(&"x".repeat(2_500_000));
    buffer.set_modified(true);

    page.run_local_history_periodic_capture_for_test();
    assert!(page.local_history_periodic_snapshot_inflight_for_test());

    let mut end = buffer.end_iter();
    buffer.insert(&mut end, "edited while chunking");
    flush_after_delay(Duration::from_millis(100));

    assert!(!page.local_history_periodic_snapshot_inflight_for_test());
    let snapshots = local_history_service::list_snapshots_for_path(data_dir.path(), &document)
        .expect("list local-history snapshots");
    assert!(
        snapshots.is_empty(),
        "an edit-stale periodic snapshot must never reach persistence"
    );
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

    assert!(
        source_map
            .view()
            .is_some_and(|view| view.as_ptr() == page.source_view().as_ptr())
    );
    assert!(!source_map.is_editable());
    assert!(!source_map.is_cursor_visible());
    assert!(!source_map.shows_line_numbers());
    assert!(!source_map.shows_line_marks());
    assert!(!source_map.is_highlight_current_line());
    assert!(source_map.is_monospace());
    assert!(source_map.has_css_class("monospace"));
    assert!(source_map.has_css_class("minimap-view"));
    assert!(source_map.hexpands());
    assert!(source_map.vexpands());
    assert_eq!(source_map.margin_start(), 13);
    assert_eq!(source_map.margin_end(), 13);
    assert_eq!(source_map.wrap_mode(), baseline_map.wrap_mode());
    assert_eq!(source_map.top_margin(), baseline_map.top_margin());
    assert_eq!(source_map.bottom_margin(), baseline_map.bottom_margin());
    assert_eq!(source_map.left_margin(), baseline_map.left_margin());
    assert_eq!(source_map.right_margin(), baseline_map.right_margin());
    assert_eq!(source_map.overflow(), baseline_map.overflow());
    assert!(!source_map.can_focus());
}

#[test]
fn test_minimap_pending_refresh_marks_work_pending() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    wait_until(std::time::Duration::from_secs(2), || {
        !page.minimap_work_pending_for_test()
    });
    assert!(!page.minimap_work_pending_for_test());
    page.mark_minimap_refresh_pending_for_test();
    assert!(page.minimap_work_pending_for_test());
}

#[test]
fn test_minimap_source_map_stays_unwrapped_across_editor_wrap_mode_changes() {
    ensure_gtk_init();
    let settings = gio::Settings::new(APP_ID);
    settings
        .set_boolean(keys::WORD_WRAP, true)
        .expect("enable word wrap");
    let page = LushtextEditorPage::new();
    let source_map = minimap_source_map(&page);

    assert_eq!(page.source_view().wrap_mode(), gtk4::WrapMode::Word);
    assert_eq!(source_map.wrap_mode(), gtk4::WrapMode::None);

    settings
        .set_boolean(keys::WORD_WRAP, false)
        .expect("disable word wrap");
    wait_until(std::time::Duration::from_secs(2), || {
        page.source_view().wrap_mode() == gtk4::WrapMode::None
            && source_map.wrap_mode() == gtk4::WrapMode::None
    });

    settings
        .set_boolean(keys::WORD_WRAP, true)
        .expect("re-enable word wrap");
    wait_until(std::time::Duration::from_secs(2), || {
        page.source_view().wrap_mode() == gtk4::WrapMode::Word
            && source_map.wrap_mode() == gtk4::WrapMode::None
    });
}

#[test]
fn test_minimap_disables_wrapped_extreme_long_line_documents() {
    ensure_gtk_init();
    let settings = enable_minimap_for_tests(false);
    settings
        .set_boolean(keys::WORD_WRAP, true)
        .expect("enable word wrap");

    let page = LushtextEditorPage::new();
    page.imp().file_size.set(Some(3 * 1024 * 1024));
    page.buffer().set_text(&format!("{}\n", "x".repeat(9_000)));
    let _window = present_editor_page_with_size(&page, 1000, 520);

    wait_until(std::time::Duration::from_secs(2), || {
        page.minimap_availability() == MinimapAvailability::TooLarge
    });

    assert!(!page.is_minimap_visible());
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
fn test_minimap_slider_css_preserves_native_viewport_effect() {
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
        rule_body.contains("background-color: alpha(@view_fg_color, 0.14);"),
        "native GtkSourceMap slider should keep the original viewport fill"
    );
    assert!(
        rule_body.contains("border: 1px solid alpha(@view_fg_color, 0.46);"),
        "native GtkSourceMap slider should keep the original viewport border"
    );
    assert!(
        !rule_body.contains("@accent_color") && !rule_body.contains("@accent_bg_color"),
        "native GtkSourceMap slider should not adopt semantic marker colors"
    );
    assert!(
        rule_body.contains("border-radius: 0;"),
        "native slider should remain a square-cornered geometry shell"
    );
}

#[test]
fn test_minimap_native_viewport_effect_projects_inside_source_map() {
    ensure_gtk_init();
    let settings = enable_minimap_for_tests(false);
    settings
        .set_boolean(keys::WORD_WRAP, false)
        .expect("disable word wrap");
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&minimap_test_document(120, &[], &[]));
    let _window = present_editor_page_with_size(&page, 1000, 700);
    wait_for_minimap_ready(&page);

    let source_map = minimap_source_map(&page);
    let map_width = f64::from(source_map.width());
    let map_height = f64::from(source_map.height());
    // GtkSourceMap draws its native slider from a private CSS node that can
    // extend a little beyond the text projection; keep the assertion about the
    // native effect's historical geometry, not an app-owned replacement.
    let native_slider_outset = 13.0;
    let viewport_bounds = page
        .minimap_viewport_bounds_for_test()
        .expect("visible minimap should project native viewport bounds");

    assert!(
        viewport_bounds.width > 0.0 && viewport_bounds.height > 0.0,
        "native viewport bounds should be positive: {viewport_bounds:?}"
    );
    assert!(
        viewport_bounds.x >= -native_slider_outset - 0.5,
        "native viewport should not start left of the source map: {viewport_bounds:?}, map_width={map_width}"
    );
    assert!(
        viewport_bounds.x + viewport_bounds.width <= map_width + native_slider_outset + 0.5,
        "native viewport should not extend right of the source map: {viewport_bounds:?}, map_width={map_width}"
    );
    assert!(
        viewport_bounds.y >= -0.5,
        "native viewport should not start above the source map: {viewport_bounds:?}, map_height={map_height}"
    );
    // Use a loose upper bound: the regression made the viewport nearly
    // full-height, while valid GTK font/theme differences stay well below this.
    assert!(
        viewport_bounds.height < map_height * 0.75,
        "long-document native viewport should not cover almost the whole minimap: {viewport_bounds:?}, map_height={map_height}"
    );
}

#[test]
fn test_minimap_viewport_top_delta_to_first_content_row_survives_width_changes() {
    fn projected_top_delta(width: i32) -> f64 {
        let settings = enable_minimap_for_tests(false);
        settings
            .set_boolean(keys::WORD_WRAP, false)
            .expect("disable word wrap");
        let page = LushtextEditorPage::new();
        page.buffer().set_text(&minimap_test_document(140, &[], &[]));
        let _window = present_editor_page_with_size(&page, width, 700);
        wait_for_minimap_ready(&page);

        let viewport = page
            .minimap_viewport_bounds_for_test()
            .expect("visible minimap should project viewport bounds");
        let content = page
            .minimap_first_content_row_bounds_for_test()
            .expect("visible minimap should project first content row");
        viewport.y - content.y
    }

    ensure_gtk_init();
    let narrow_delta = projected_top_delta(900);
    let wide_delta = projected_top_delta(1300);

    // Allow half a logical pixel for GTK's fractional allocation and rounding.
    assert!(
        (narrow_delta - wide_delta).abs() <= 0.5,
        "viewport top should keep the same content-row anchor delta across widths: narrow={narrow_delta}, wide={wide_delta}"
    );
}

#[test]
fn test_minimap_native_viewport_effect_reprojects_after_mid_file_scroll() {
    ensure_gtk_init();
    let settings = enable_minimap_for_tests(false);
    settings
        .set_boolean(keys::WORD_WRAP, false)
        .expect("disable word wrap");
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&minimap_test_document(260, &[], &[]));
    let _window = present_editor_page_with_size(&page, 1000, 700);
    wait_for_minimap_ready(&page);

    let top_bounds = page
        .minimap_viewport_bounds_for_test()
        .expect("top-of-file viewport should project");
    let buffer = page.buffer();
    let mid_iter = buffer
        .iter_at_line(180)
        .expect("mid-file line should exist in the minimap fixture");
    buffer.place_cursor(&mid_iter);
    page.source_view()
        .scroll_to_mark(&buffer.get_insert(), 0.0, true, 0.0, 0.0);
    // `scroll_to_mark` updates adjustments asynchronously through the GTK main
    // loop; give it a short frame budget before polling projected minimap bounds.
    flush_after_delay(std::time::Duration::from_millis(100));
    wait_until(std::time::Duration::from_secs(3), || {
        page.source_view().visible_rect().y() > 0
            && page
                .minimap_viewport_bounds_for_test()
                .is_some_and(|bounds| bounds.y > top_bounds.y + 1.0)
    });

    let source_map = minimap_source_map(&page);
    let map_height = f64::from(source_map.height());
    let scrolled_bounds = page
        .minimap_viewport_bounds_for_test()
        .expect("mid-file viewport should project");

    assert!(
        scrolled_bounds.y >= -0.5 && scrolled_bounds.bottom() <= map_height + 0.5,
        "mid-file native viewport should remain inside source-map bounds: {scrolled_bounds:?}, map_height={map_height}"
    );
    // Use the same loose anti-regression bound as the top-of-file projection:
    // this catches full-height projection failures without overfitting pixels.
    assert!(
        scrolled_bounds.height < map_height * 0.75,
        "mid-file native viewport should remain a visible-window slice, not the full minimap: {scrolled_bounds:?}, map_height={map_height}"
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

    // Width-reflow bursts pin minimap geometry sync until the viewport
    // settles, so wait for the map to actually mirror the editor's dynamic
    // EOF margin instead of accepting the first margin above the baseline.
    wait_until(std::time::Duration::from_secs(5), || {
        let editor_margin = page.source_view().bottom_margin();
        editor_margin > 6 && source_map.bottom_margin() == editor_margin
    });

    let baseline_map = baseline_source_map_for_view(page.source_view());
    assert_eq!(source_map.bottom_margin(), baseline_map.bottom_margin());
    assert!(source_map.bottom_margin() > 6);
}

#[test]
fn test_minimap_search_markers_stop_before_dynamic_eof_tail() {
    ensure_gtk_init();
    let _settings = enable_minimap_for_tests(false);
    let page = LushtextEditorPage::new();
    page.buffer()
        .set_text(&minimap_test_document(80, &[0, 24, 79], &[]));
    let _window = present_editor_page(&page);
    wait_for_minimap_ready(&page);

    show_search_and_wait_for_minimap_marker(&page, "needle");

    let content_bottom = source_map_content_bottom(&page);
    let strip_height = f64::from(minimap_marker_strip(&page).height());
    assert!(
        content_bottom + 1.0 < strip_height,
        "test needs a visible blank EOF tail: content bottom {content_bottom}, strip {strip_height}"
    );

    let bounds = assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Search);
    assert!(
        bounds
            .iter()
            .any(|bound| (content_bottom - bound.bottom).abs() <= 0.5),
        "a match on the last real line should project to the content bottom, not the strip bottom: {bounds:?}"
    );
}

#[test]
fn test_minimap_all_marker_kinds_share_source_map_content_boundary() {
    ensure_gtk_init();
    let _settings = enable_minimap_for_tests(true);
    let page = LushtextEditorPage::new();
    page.buffer()
        .set_text(&minimap_test_document(90, &[89], &[89]));
    let _window = present_editor_page(&page);
    wait_for_minimap_ready(&page);

    let line = page.buffer().iter_at_line(89).expect("line 89");
    page.buffer().place_cursor(&line);
    let _ = page.toggle_bookmark_at_cursor();
    show_search_and_wait_for_minimap_marker(&page, "needle");

    wait_until(std::time::Duration::from_secs(2), || {
        page.minimap_marker_count(MinimapMarkerKind::Bookmark) > 0
            && page.minimap_marker_count(MinimapMarkerKind::Modified) > 0
            && page.minimap_marker_count(MinimapMarkerKind::LongLine) > 0
    });

    for kind in [
        MinimapMarkerKind::Bookmark,
        MinimapMarkerKind::Search,
        MinimapMarkerKind::Modified,
        MinimapMarkerKind::LongLine,
    ] {
        assert_marker_bounds_within_source_content(&page, kind);
    }
}

#[test]
fn test_minimap_search_marker_bounds_clear_when_search_closes() {
    ensure_gtk_init();
    let _settings = enable_minimap_for_tests(false);
    let page = LushtextEditorPage::new();
    page.buffer()
        .set_text(&minimap_test_document(70, &[5, 35, 69], &[]));
    let _window = present_editor_page(&page);
    wait_for_minimap_ready(&page);

    show_search_and_wait_for_minimap_marker(&page, "needle");
    assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Search);

    page.hide_search();
    wait_until(std::time::Duration::from_secs(2), || {
        page.minimap_marker_count(MinimapMarkerKind::Search) == 0
            && page
                .minimap_marker_bounds(MinimapMarkerKind::Search)
                .is_empty()
    });
}

#[test]
fn test_minimap_modified_bounds_clear_after_save_and_reproject_after_later_edit() {
    ensure_gtk_init();
    let _settings = enable_minimap_for_tests(false);
    let page = LushtextEditorPage::new();
    let temp = tempfile::NamedTempFile::new().expect("temp file");
    page.set_file_path(temp.path());
    page.buffer().set_text(&minimap_test_document(75, &[], &[]));
    let _window = present_editor_page(&page);
    wait_for_minimap_ready(&page);

    wait_until(std::time::Duration::from_secs(2), || {
        !page
            .minimap_marker_bounds(MinimapMarkerKind::Modified)
            .is_empty()
    });
    assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Modified);

    let done = Rc::new(Cell::new(false));
    let done_clone = done.clone();
    page.save_file_async(move |result| {
        result.expect("save should succeed");
        done_clone.set(true);
    });
    wait_until(std::time::Duration::from_secs(2), || done.get());
    wait_until(std::time::Duration::from_secs(2), || {
        page.minimap_marker_count(MinimapMarkerKind::Modified) == 0
            && page
                .minimap_marker_bounds(MinimapMarkerKind::Modified)
                .is_empty()
    });

    let buffer = page.buffer();
    let mut iter = buffer.iter_at_line(40).expect("line 40");
    buffer.insert(&mut iter, " changed");
    wait_until(std::time::Duration::from_secs(2), || {
        !page
            .minimap_marker_bounds(MinimapMarkerKind::Modified)
            .is_empty()
    });
    assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Modified);
}

#[test]
fn test_minimap_long_line_toggle_preserves_other_projected_markers() {
    ensure_gtk_init();
    let settings = enable_minimap_for_tests(true);
    let page = LushtextEditorPage::new();
    page.buffer()
        .set_text(&minimap_test_document(85, &[42, 84], &[84]));
    let _window = present_editor_page(&page);
    wait_for_minimap_ready(&page);

    let line = page.buffer().iter_at_line(84).expect("line 84");
    page.buffer().place_cursor(&line);
    let _ = page.toggle_bookmark_at_cursor();
    show_search_and_wait_for_minimap_marker(&page, "needle");
    wait_until(std::time::Duration::from_secs(2), || {
        !page
            .minimap_marker_bounds(MinimapMarkerKind::LongLine)
            .is_empty()
    });

    let search_before =
        assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Search);
    let bookmark_before =
        assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Bookmark);

    settings
        .set_boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE, false)
        .expect("disable long-line minimap markers");
    wait_until(std::time::Duration::from_secs(2), || {
        page.minimap_marker_count(MinimapMarkerKind::LongLine) == 0
            && page
                .minimap_marker_bounds(MinimapMarkerKind::LongLine)
                .is_empty()
    });

    let search_after = assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Search);
    let bookmark_after =
        assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Bookmark);
    assert_eq!(search_before.len(), search_after.len());
    assert_eq!(bookmark_before.len(), bookmark_after.len());
    assert!(
        (search_before[0].top - search_after[0].top).abs() < 1.0,
        "search marker should keep its projected alignment after long-line toggle"
    );
    assert!(
        (bookmark_before[0].bottom - bookmark_after[0].bottom).abs() < 1.0,
        "bookmark marker should keep its projected alignment after long-line toggle"
    );
}

#[test]
fn test_minimap_marker_projection_refreshes_after_taller_allocation() {
    ensure_gtk_init();
    let _settings = enable_minimap_for_tests(false);
    let page = LushtextEditorPage::new();
    page.buffer()
        .set_text(&minimap_test_document(80, &[79], &[]));
    let window = present_editor_page_with_size(&page, 1000, 520);
    wait_for_minimap_ready(&page);

    show_search_and_wait_for_minimap_marker(&page, "needle");
    let initial_strip_height = minimap_marker_strip(&page).height();
    let initial_tail = f64::from(initial_strip_height) - source_map_content_bottom(&page);
    assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Search);

    window.set_size_request(1000, 900);
    window.queue_resize();
    wait_until(std::time::Duration::from_secs(2), || {
        minimap_marker_strip(&page).height() > initial_strip_height + 80
    });
    wait_until(std::time::Duration::from_secs(2), || {
        !page
            .minimap_marker_bounds(MinimapMarkerKind::Search)
            .is_empty()
    });

    let resized_tail =
        f64::from(minimap_marker_strip(&page).height()) - source_map_content_bottom(&page);
    assert!(
        resized_tail > initial_tail + 40.0,
        "taller allocation should expose more EOF tail while markers remain content-bound"
    );
    assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Search);
}

#[test]
fn test_minimap_markers_remain_content_bound_with_word_wrap_disabled() {
    ensure_gtk_init();
    let settings = enable_minimap_for_tests(false);
    settings
        .set_boolean(keys::WORD_WRAP, false)
        .expect("disable word wrap");
    let page = LushtextEditorPage::new();
    page.buffer()
        .set_text(&minimap_test_document(80, &[10, 40, 79], &[40]));
    let _window = present_editor_page(&page);
    wait_for_minimap_ready(&page);

    assert_eq!(page.source_view().wrap_mode(), gtk4::WrapMode::None);
    assert_eq!(minimap_source_map(&page).wrap_mode(), gtk4::WrapMode::None);
    show_search_and_wait_for_minimap_marker(&page, "needle");
    assert_marker_bounds_within_source_content(&page, MinimapMarkerKind::Search);
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

#[test]
fn test_bookmark_edit_moves_existing_id_across_lines() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    buffer.set_text("one\ntwo\nthree\nfour\nfive");

    let line_three = buffer.iter_at_line(2).expect("line three");
    buffer.place_cursor(&line_three);
    assert_eq!(
        page.toggle_bookmark_at_cursor(),
        BookmarkToggleState::Added(2)
    );

    let bookmark_id = page.bookmark_records()[0].id.clone();
    let outcome = page
        .update_bookmark(&bookmark_id, Some("  Last line  ".to_string()), 5)
        .expect("move to last line");
    assert_eq!(outcome.line, 4);
    let updated = page.bookmark_at_line(4).expect("moved bookmark");
    assert_eq!(updated.id, bookmark_id);
    assert_eq!(updated.label.as_deref(), Some("Last line"));
    assert_eq!(
        page.bookmark_at_line(4).map(|bookmark| bookmark.id),
        Some(bookmark_id.clone())
    );

    let outcome = page
        .update_bookmark(&bookmark_id, Some("First line".to_string()), 1)
        .expect("move to first line");
    assert_eq!(outcome.line, 0);
    assert_eq!(
        page.bookmark_at_line(0).map(|bookmark| bookmark.id),
        Some(bookmark_id.clone())
    );

    let outcome = page
        .update_bookmark(&bookmark_id, Some("Middle line".to_string()), 3)
        .expect("move back to middle line");
    assert_eq!(outcome.line, 2);
    assert_eq!(
        page.bookmark_records()
            .into_iter()
            .map(|bookmark| (bookmark.id, bookmark.line, bookmark.label))
            .collect::<Vec<_>>(),
        vec![(bookmark_id, 2, Some("Middle line".to_string()))]
    );
}

#[test]
fn test_bookmark_edit_rejects_invalid_lines_without_mutating() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    buffer.set_text("one\ntwo\nthree\nfour");

    let line_one = buffer.iter_at_line(0).expect("line one");
    buffer.place_cursor(&line_one);
    let _ = page.toggle_bookmark_at_cursor();
    let first_id = page.bookmark_records()[0].id.clone();

    let line_three = buffer.iter_at_line(2).expect("line three");
    buffer.place_cursor(&line_three);
    let _ = page.toggle_bookmark_at_cursor();
    let before = page.bookmark_records();

    assert_eq!(
        page.update_bookmark(&first_id, Some("changed".to_string()), 3),
        Err(BookmarkEditError::LineOccupied { line: 3 })
    );
    assert_eq!(page.bookmark_records(), before);

    assert_matches!(
        page.update_bookmark(&first_id, Some("changed".to_string()), 0),
        Err(BookmarkEditError::LineOutOfRange {
            requested_line: 0,
            max_line: 4
        })
    );
    assert_eq!(page.bookmark_records(), before);

    assert_matches!(
        page.update_bookmark(&first_id, Some("changed".to_string()), 99),
        Err(BookmarkEditError::LineOutOfRange {
            requested_line: 99,
            max_line: 4
        })
    );
    assert_eq!(page.bookmark_records(), before);
}

#[test]
fn test_bookmark_activation_callback_only_fires_for_bookmark_lines() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    buffer.set_text("one\ntwo\nthree");

    let line_two = buffer.iter_at_line(1).expect("line two");
    buffer.place_cursor(&line_two);
    let _ = page.toggle_bookmark_at_cursor();
    let bookmark_id = page.bookmark_records()[0].id.clone();

    let activated = Rc::new(RefCell::new(Vec::new()));
    let activated_for_callback = activated.clone();
    page.connect_bookmark_activated(move |bookmark| {
        activated_for_callback.borrow_mut().push(bookmark);
    });

    let activated_bookmark = page
        .activate_bookmark_at_line(1)
        .expect("bookmark activation");
    assert_eq!(activated_bookmark.id, bookmark_id);
    assert!(page.activate_bookmark_at_line(0).is_none());

    let activated = activated.borrow();
    assert_eq!(activated.len(), 1);
    assert_eq!(activated[0].id, bookmark_id);
    assert_eq!(activated[0].line, 1);
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
fn test_inline_alert_announcements_use_shared_throttling_policy() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    let warning = InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Draft Changes Restored".to_string(),
        body: "Unsaved changes to the document have been restored.".to_string(),
        primary_button: Some("_Discard...".to_string()),
        secondary_button: Some("_Save...".to_string()),
    };
    page.emit_inline_notification(warning.clone());

    let warning_key = inline_alert_announcement_key_for_test(&warning);
    assert!(
        !page
            .info_bar()
            .imp()
            .alert_announcement_throttler
            .should_announce_at(AnnouncementLane::StatusUpdate, &warning_key, Instant::now()),
        "warning inline alerts should be throttled after render announces them"
    );

    let error = InlineActionNotification {
        style: InlineNotificationStyle::Error,
        title: "Could Not Open File".to_string(),
        body: "Permission denied".to_string(),
        primary_button: Some("_Retry".to_string()),
        secondary_button: None,
    };
    page.emit_inline_notification(error.clone());

    let error_key = inline_alert_announcement_key_for_test(&error);
    assert!(
        page.info_bar()
            .imp()
            .alert_announcement_throttler
            .should_announce_at(AnnouncementLane::Alert, &error_key, Instant::now()),
        "error inline alerts should keep the high-priority alert lane unthrottled"
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
fn test_settings_minimap_width_bounds_overlay_width_request() {
    ensure_gtk_init();
    let settings = gio::Settings::new(APP_ID);
    let original_width = settings.int(keys::MINIMAP_WIDTH);
    settings
        .set_int(keys::MINIMAP_WIDTH, 88)
        .expect("set minimap width");

    // Setting before construction proves `Settings::bind(...).sync_create()`
    // initializes the overlay width from persisted state.
    let page = LushtextEditorPage::new();
    assert_eq!(
        page.imp().minimap_overlay.width_request(),
        88,
        "new editor pages should initialize minimap width from GSettings"
    );

    // Later setting changes prove the live binding updates the existing page
    // without the old imperative width setter.
    settings
        .set_int(keys::MINIMAP_WIDTH, 160)
        .expect("set maximum minimap width");
    wait_until(std::time::Duration::from_secs(2), || {
        page.imp().minimap_overlay.width_request() == 160
    });

    settings
        .set_int(keys::MINIMAP_WIDTH, 64)
        .expect("set minimum minimap width");
    wait_until(std::time::Duration::from_secs(2), || {
        page.imp().minimap_overlay.width_request() == 64
    });

    settings
        .set_int(keys::MINIMAP_WIDTH, original_width)
        .expect("restore minimap width");
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

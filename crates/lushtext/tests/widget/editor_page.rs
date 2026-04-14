// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextEditorPage widget.

use crate::common::{ensure_gtk_init, present_window, test_application, wait_until};
use gio::prelude::ListModelExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::annotation::{AnnotationRecord, AnnotationStyle};
use lushtext_core::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use lushtext_core::ui::editor_page::{
    BookmarkNavigationDirection, BookmarkToggleState, LushtextEditorPage,
};
use sourceview5::prelude::*;

fn button_label(button: &gtk4::Button) -> gtk4::Label {
    button
        .child()
        .expect("button child")
        .downcast::<gtk4::Label>()
        .expect("button label")
}

fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
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

#[test]
fn test_load_annotations_restores_current_annotation() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    buffer.set_text("one\ntwo\nthree\nfour\n");

    let annotation = AnnotationRecord::new(1, 2, "remember this", AnnotationStyle::Warning);
    page.load_annotations(std::slice::from_ref(&annotation));

    let line_three = buffer.iter_at_line(2).expect("expected operation to succeed");
    buffer.place_cursor(&line_three);

    let restored = page.current_annotation().expect("expected operation to succeed");
    assert_eq!(restored.id, annotation.id);
    assert_eq!(restored.note_text, "remember this");
    assert_eq!(restored.style, AnnotationStyle::Warning);
    assert_eq!(restored.start_line, 1);
    assert_eq!(restored.end_line, 2);
}

#[test]
fn test_annotation_range_tracks_user_edits_and_removes_deleted_ranges() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();
    buffer.set_text("one\ntwo\nthree\nfour\n");

    page.load_annotations(&[AnnotationRecord::new(
        1,
        2,
        "track me",
        AnnotationStyle::Todo,
    )]);

    let mut insert_at_start = buffer.start_iter();
    buffer.begin_user_action();
    buffer.insert(&mut insert_at_start, "zero\n");
    buffer.end_user_action();
    flush_events();

    let shifted = page.annotation_records();
    assert_eq!(shifted.len(), 1);
    assert_eq!(shifted[0].start_line, 2);
    assert_eq!(shifted[0].end_line, 3);

    let mut delete_start = buffer.iter_at_line(2).expect("expected operation to succeed");
    let mut delete_end = buffer.iter_at_line(4).expect("expected operation to succeed");
    buffer.begin_user_action();
    buffer.delete(&mut delete_start, &mut delete_end);
    buffer.end_user_action();
    flush_events();

    assert!(page.annotation_records().is_empty());
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
fn test_warning_infobar_wraps_titles_and_action_labels() {
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
        imp.discard_infobar.property::<bool>("revealed"),
        "warning infobar should be shown"
    );
    assert!(imp.discard_title.wraps(), "warning title should wrap");
    assert_eq!(
        imp.discard_title.wrap_mode(),
        gtk4::pango::WrapMode::WordChar
    );
    assert!(imp.discard_subtitle.wraps(), "warning subtitle should wrap");
    assert_eq!(
        imp.discard_subtitle.wrap_mode(),
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
fn test_document_restored_infobar_keeps_save_as_visible() {
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
        imp.discard_infobar.property::<bool>("revealed"),
        "restored-document infobar should be shown"
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

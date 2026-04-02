// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextEditorPage widget.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::editor_page::LushtextEditorPage;
use sourceview5::prelude::*;

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
    let result: std::rc::Rc<std::cell::RefCell<Option<Result<(), String>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let result_clone = result.clone();
    page.save_file_async(move |r| {
        *result_clone.borrow_mut() = Some(r);
    });
    let result = result.borrow().clone().unwrap();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("No file path set"));
}

#[test]
fn test_save_file_writes_content() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let buffer = page.buffer();

    // Manually set the file path (simulating load_file_async without the async part)
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    // Set path via the internal RefCell (load_file_async sets this synchronously)
    page.imp().file_path.replace(Some(path.clone()));

    // Set buffer content
    buffer.set_text("saved content");

    // Save and verify — spin main loop to process the background thread callback
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let done_clone = done.clone();
    page.save_file_async(move |r| {
        r.unwrap();
        done_clone.set(true);
    });
    while !done.get() {
        glib::MainContext::default().iteration(true);
    }
    let saved = std::fs::read_to_string(&path).unwrap();
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
fn test_toggle_search_changes_revealer_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    let revealer = &page.imp().search_revealer;

    // Initially hidden
    assert!(!revealer.reveals_child());

    // Toggle on
    page.toggle_search();
    assert!(revealer.reveals_child());

    // Toggle off
    page.toggle_search();
    assert!(!revealer.reveals_child());
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    // Verify Default impl works (it delegates to new())
    let _page: LushtextEditorPage = Default::default();
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
    page.toggle_search();
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
    page.toggle_search();

    assert!(page.imp().search_revealer.reveals_child());

    // Click the close button
    page.imp().search_bar.close_button().emit_clicked();

    // Search bar should be hidden
    assert!(!page.imp().search_revealer.reveals_child());
}

#[test]
fn test_escape_hides_search() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    // Show the search bar
    page.toggle_search();

    assert!(page.imp().search_revealer.reveals_child());

    // Emit stop-search (Escape key)
    page.imp().search_bar.search_entry().emit_stop_search();

    // Search bar should be hidden
    assert!(!page.imp().search_revealer.reveals_child());
}

#[test]
fn test_search_show_hide_cycle() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();

    // Cycle: show → close → show → escape → show → toggle
    page.toggle_search();
    assert!(page.imp().search_revealer.reveals_child());

    page.imp().search_bar.close_button().emit_clicked();
    assert!(!page.imp().search_revealer.reveals_child());

    page.toggle_search();
    assert!(page.imp().search_revealer.reveals_child());

    page.imp().search_bar.search_entry().emit_stop_search();
    assert!(!page.imp().search_revealer.reveals_child());

    page.toggle_search();
    assert!(page.imp().search_revealer.reveals_child());

    page.toggle_search();
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-level Markdown preview freshness.
//!
//! `markdown_preview.rs` mounts the preview widget on its own, so it cannot see
//! whether the window re-renders the preview when the selected tab's content or
//! identity is republished. These tests drive the real window: a document load,
//! a whole-buffer replacement, and a path change each suspend or bypass the
//! buffer `changed` handler, and the editor's content-republished hook is the
//! only thing that brings the preview back to the installed document.
//!
//! Assertions read `buffer_text()`, the placeholder, and evidence counters
//! through end-state waits rather than layout geometry, because presented
//! widget tests do not reliably advance the preview's timed layout animation.

use crate::common::{
    EditorLoadDelayReset, action_state_bool, activate_action, active_editor, editor_text,
    ensure_gtk_init, fixture, flush_after_delay, present_window, test_window, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::services::editor_io;
use lushtext_core::ui::editor_page::{
    BufferReplacementCancelReason, EditorLoadState, LushtextEditorPage,
};
use lushtext_core::ui::markdown_preview::LushtextMarkdownPreview;
use lushtext_core::ui::window::LushtextWindow;
use sourceview5::prelude::BufferExt as _;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

const PREPARING: &str = "Preparing Markdown preview…";
const NOT_MARKDOWN: &str = "Not a Markdown file";

fn preview(window: &LushtextWindow) -> LushtextMarkdownPreview {
    window.imp().markdown_preview.get()
}

/// A presented window with one untitled tab, because the preview actions are
/// disabled while no tab is open, with the preview `action` switched on and its
/// window-side `flag` (`preview_mode` or `preview_visible`) observed set.
fn preview_window(action: &str, flag: fn(&LushtextWindow) -> bool) -> LushtextWindow {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    window.new_tab();
    activate_action(&window, action);
    wait_until(Duration::from_secs(2), || {
        flag(&window) && action_state_bool(&window, action)
    });
    window
}

/// [`preview_window`] in preview-only mode.
fn preview_only_window() -> LushtextWindow {
    preview_window("toggle-preview-mode", |window| {
        window.imp().preview_mode.get()
    })
}

fn selected_page(window: &LushtextWindow) -> libadwaita::TabPage {
    window
        .imp()
        .tab_view
        .selected_page()
        .expect("a selected tab page")
}

fn wait_for_loaded(editor: &LushtextEditorPage) {
    wait_until(Duration::from_secs(10), || {
        editor.load_state() == EditorLoadState::Loaded
    });
}

fn wait_for_rendered(window: &LushtextWindow, expected: &str) {
    let preview = preview(window);
    wait_until(Duration::from_secs(10), || {
        preview.is_showing_content()
            && !preview.render_pending()
            && preview.buffer_text().contains(expected)
    });
}

fn write_markdown(dir: &Path, name: &str, heading: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fixture::write_text(&path, &format!("# {heading}\n\nBody paragraph.\n"));
    path
}

#[test]
fn test_opening_markdown_in_preview_only_renders_it_after_load() {
    let window = preview_only_window();
    let dir = tempfile::tempdir().expect("preview-only open tempdir");
    let path = write_markdown(dir.path(), "opened.md", "Opened Heading");

    window.open_document(&path);
    let editor = active_editor(&window);
    wait_for_loaded(&editor);
    wait_for_rendered(&window, "Opened Heading");

    assert!(
        action_state_bool(&window, "toggle-preview-mode"),
        "opening an existing document must keep preview-only mode"
    );
    assert!(window.imp().preview_mode.get());
}

#[test]
fn test_opening_markdown_with_side_by_side_preview_renders_it_after_load() {
    let window = preview_window("toggle-preview-pane", |window| {
        window.imp().preview_visible.get()
    });
    let dir = tempfile::tempdir().expect("side-by-side open tempdir");
    let path = write_markdown(dir.path(), "side.md", "Side Heading");

    window.open_document(&path);
    wait_for_loaded(&active_editor(&window));
    wait_for_rendered(&window, "Side Heading");

    assert!(
        action_state_bool(&window, "toggle-preview-pane"),
        "opening an existing document must keep the side-by-side pane"
    );
    // The pane was requested before the `tabs` page was ever allocated, which is
    // the case the shell defers by one frame; the split view must still open.
    // The harness fails the run on the NaN-progress allocation warning.
    wait_until(Duration::from_secs(5), || {
        window.imp().preview_split_view.shows_sidebar()
    });
}

#[test]
fn test_preview_names_the_installing_interval_and_non_markdown_immediately() {
    let _reset = EditorLoadDelayReset;
    let window = preview_only_window();
    let dir = tempfile::tempdir().expect("installing interval tempdir");
    let markdown = write_markdown(dir.path(), "pending.md", "Pending Heading");
    let rust = dir.path().join("pending.rs");
    fixture::write_text(&rust, "fn main() {}\n");
    // Keep the read in flight long enough that nothing below races the load.
    editor_io::set_load_delay_for_test(300);

    window.open_document(&markdown);
    let markdown_page = selected_page(&window);
    let preview = preview(&window);
    assert_eq!(
        preview.buffer_text(),
        PREPARING,
        "a selected Markdown tab whose load is in flight must name the interval"
    );
    assert!(preview.is_showing_content());
    assert_eq!(
        active_editor(&window).load_state(),
        EditorLoadState::Loading
    );

    window.open_document(&rust);
    assert_eq!(
        preview.evidence().placeholder_description.as_deref(),
        Some(NOT_MARKDOWN),
        "the language is known from the path, so a loading non-Markdown tab says so at once"
    );
    assert!(!preview.is_showing_content());

    // The Markdown tab heals once it is selected again and its load lands.
    window.imp().tab_view.set_selected_page(&markdown_page);
    editor_io::set_load_delay_for_test(0);
    wait_for_loaded(&active_editor(&window));
    wait_for_rendered(&window, "Pending Heading");
}

#[test]
fn test_buffer_replacement_terminal_refreshes_the_selected_preview() {
    let window = preview_only_window();
    let dir = tempfile::tempdir().expect("replacement tempdir");
    let path = write_markdown(dir.path(), "replaced.md", "Original Heading");
    window.open_document(&path);
    let editor = active_editor(&window);
    wait_for_loaded(&editor);
    wait_for_rendered(&window, "Original Heading");

    let current = Rc::new(Cell::new(true));
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    editor.replace_buffer_for_test(
        "# Replaced Heading\n\nNew body.\n".to_string(),
        1,
        Rc::clone(&current),
        Rc::clone(&outcomes),
    );
    wait_until(Duration::from_secs(10), || outcomes.borrow().len() == 1);
    assert!(outcomes.borrow()[0].cancel_reason.is_none());

    // The replacement ran under the projection guard, so the buffer `changed`
    // handler stood down; only the terminal republish can reach the preview.
    wait_for_rendered(&window, "Replaced Heading");
    assert!(!preview(&window).buffer_text().contains("Original Heading"));
}

#[test]
fn test_cancelled_buffer_replacement_replaces_the_preparing_placeholder() {
    let window = preview_only_window();
    let dir = tempfile::tempdir().expect("cancelled replacement tempdir");
    let path = write_markdown(dir.path(), "cancelled.md", "Before Cancel");
    window.open_document(&path);
    let editor = active_editor(&window);
    wait_for_loaded(&editor);
    wait_for_rendered(&window, "Before Cancel");

    // A body large enough to need bounded turns, so the guard is held across
    // main-loop iterations and the preview can observe it.
    let current = Rc::new(Cell::new(true));
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    editor.make_buffer_replacement_stale_after_slices_for_test(1);
    editor.replace_buffer_for_test(
        "new line\n".repeat(160_000),
        2,
        Rc::clone(&current),
        Rc::clone(&outcomes),
    );
    assert!(editor.buffer_replacement_evidence().in_progress);

    // Re-entering preview-only while the guard is held reaches the installing
    // branch of the real refresh path.
    activate_action(&window, "toggle-preview-mode");
    activate_action(&window, "toggle-preview-mode");
    let preview = preview(&window);
    if outcomes.borrow().is_empty() {
        assert_eq!(preview.buffer_text(), PREPARING);
    }

    wait_until(Duration::from_secs(10), || outcomes.borrow().len() == 1);
    assert_eq!(
        outcomes.borrow()[0].cancel_reason,
        Some(BufferReplacementCancelReason::Stale)
    );
    wait_until(Duration::from_secs(10), || {
        !preview.render_pending() && preview.buffer_text() != PREPARING
    });
    // A stale replacement clears rather than publishing a partial body, so
    // the buffer as it now stands is empty, and so is its render.
    assert_eq!(editor_text(&editor), "");
    assert!(preview.is_showing_content());
    assert_eq!(
        preview.buffer_text(),
        "",
        "the preview must render the buffer as the cancelled replacement left it"
    );
}

#[test]
fn test_background_tab_terminal_does_not_rerender_the_preview() {
    let _reset = EditorLoadDelayReset;
    let window = preview_only_window();
    let dir = tempfile::tempdir().expect("background terminal tempdir");
    let first = write_markdown(dir.path(), "first.md", "First Heading");
    let second = write_markdown(dir.path(), "second.md", "Second Heading");

    window.open_document(&first);
    let first_page = selected_page(&window);
    wait_for_loaded(&active_editor(&window));
    wait_for_rendered(&window, "First Heading");

    editor_io::set_load_delay_for_test(400);
    window.open_document(&second);
    let second_editor = active_editor(&window);
    assert_eq!(second_editor.load_state(), EditorLoadState::Loading);
    window.imp().tab_view.set_selected_page(&first_page);
    wait_for_rendered(&window, "First Heading");
    assert_eq!(
        second_editor.load_state(),
        EditorLoadState::Loading,
        "the background load must still be in flight when the baseline is taken"
    );

    let preview = preview(&window);
    let dispatch_before = preview.evidence().projection.dispatch_count;
    let text_view = preview.text_view();
    let rerenders = Rc::new(Cell::new(0u32));
    let buffer_swaps = Rc::clone(&rerenders);
    let swap_handler = text_view.connect_buffer_notify(move |_| {
        buffer_swaps.set(buffer_swaps.get() + 1);
    });
    let edits = Rc::clone(&rerenders);
    let buffer = text_view.buffer();
    let edit_handler = buffer.connect_changed(move |_| edits.set(edits.get() + 1));

    wait_for_loaded(&second_editor);
    flush_after_delay(Duration::from_millis(50));

    assert_eq!(
        rerenders.get(),
        0,
        "a background terminal must not re-render"
    );
    assert_eq!(
        preview.evidence().projection.dispatch_count,
        dispatch_before,
        "a background terminal must not dispatch a projection"
    );
    assert!(preview.buffer_text().contains("First Heading"));
    assert!(!preview.buffer_text().contains("Second Heading"));
    text_view.disconnect(swap_handler);
    buffer.disconnect(edit_handler);
}

#[test]
fn test_identity_change_follows_markdown_language_both_ways() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    window.new_tab();
    let editor = active_editor(&window);
    editor
        .buffer()
        .set_text("# Saved Heading\n\nUntitled body.\n");
    activate_action(&window, "toggle-preview-mode");
    let preview = preview(&window);
    wait_until(Duration::from_secs(2), || {
        preview.evidence().placeholder_description.as_deref() == Some(NOT_MARKDOWN)
    });
    // Let the buffer-change debounce drain so only the identity change can
    // re-render below.
    flush_after_delay(Duration::from_millis(400));
    assert!(!preview.is_showing_content());

    let dir = tempfile::tempdir().expect("identity change tempdir");
    editor.set_file_path(&dir.path().join("notes.md"));
    wait_for_rendered(&window, "Saved Heading");

    editor.set_file_path(&dir.path().join("notes.txt"));
    wait_until(Duration::from_secs(2), || {
        !preview.is_showing_content()
            && preview.evidence().placeholder_description.as_deref() == Some(NOT_MARKDOWN)
    });
    assert!(
        editor.buffer().language().is_none(),
        "a path with no known language must clear the Markdown language"
    );
}

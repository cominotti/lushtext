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
    emit_key_pressed_on_focus, ensure_gtk_init, fixture, flush_after_delay, flush_events, fs_read,
    present_window, rect_within, test_window, test_window_with_restored_size, wait_until,
    write_external_change_after_mtime_tick,
};
use glib::prelude::ToVariant;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::services::editor_io;
use lushtext_core::services::notifications::{
    InlineActionNotification, InlineNotificationStyle, NotificationSeverity,
};
use lushtext_core::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_core::ui::editor_page::{
    BufferReplacementCancelReason, EditorLoadState, InlineAlertLifetime, LushtextEditorPage,
};
use lushtext_core::ui::info_bar::LushtextInfoBar;
use lushtext_core::ui::markdown_preview::LushtextMarkdownPreview;
use lushtext_core::ui::window::LushtextWindow;
use lushtext_core::ui::window::notification_evidence;
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

// ---------------------------------------------------------------------------
// The window-hosted document alert shown above the preview in preview-only
// mode (`show-document-alerts-in-preview-mode`).
// ---------------------------------------------------------------------------

const CHANGED_ON_DISK: &str = "File Has Changed on Disk";

fn preview_alert(window: &LushtextWindow) -> LushtextInfoBar {
    window.imp().preview_document_alert.get()
}

/// Whether `bar` currently shows an alert titled `title` (own properties).
fn bar_shows(bar: &LushtextInfoBar, title: &str) -> bool {
    bar.property::<bool>("visible")
        && bar.imp().alert_revealer.reveals_child()
        && bar.imp().alert_title.label().as_str() == title
}

fn bar_hidden(bar: &LushtextInfoBar) -> bool {
    !bar.property::<bool>("visible") && !bar.imp().alert_revealer.reveals_child()
}

/// How many of the window's alert surfaces are rendered showing `title`.
///
/// `is_visible()` walks the parent chain, so an editor bar inside the hidden
/// editor column does not count.
fn visible_surfaces_showing(window: &LushtextWindow, title: &str) -> usize {
    let mut bars = vec![preview_alert(window)];
    let tab_view = &window.imp().tab_view;
    for index in 0..tab_view.n_pages() {
        if let Ok(editor) = tab_view
            .nth_page(index)
            .child()
            .downcast::<LushtextEditorPage>()
        {
            bars.push(editor.info_bar().clone());
        }
    }
    bars.iter()
        .filter(|bar| bar.is_visible() && bar.is_mapped() && bar_shows(bar, title))
        .count()
}

fn open_loaded(window: &LushtextWindow, path: &Path) -> LushtextEditorPage {
    window.open_document(path);
    let editor = active_editor(window);
    wait_for_loaded(&editor);
    editor
}

/// A presented window with `path` open and loaded, switched to preview-only.
fn markdown_in_preview_only(path: &Path) -> (LushtextWindow, LushtextEditorPage) {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let editor = open_loaded(&window, path);
    enter_preview_only(&window);
    (window, editor)
}

fn enter_preview_only(window: &LushtextWindow) {
    set_bool_action(window, "set-preview-mode", true);
    wait_until(Duration::from_secs(2), || window.imp().preview_mode.get());
}

fn leave_preview_only(window: &LushtextWindow) {
    set_bool_action(window, "set-preview-mode", false);
    wait_until(Duration::from_secs(2), || !window.imp().preview_mode.get());
}

fn set_bool_action(window: &LushtextWindow, name: &str, value: bool) {
    gio::prelude::ActionGroupExt::activate_action(window, name, Some(&value.to_variant()));
    flush_events();
}

fn warning(
    title: &str,
    primary: Option<&str>,
    secondary: Option<&str>,
) -> InlineActionNotification {
    InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: title.to_string(),
        body: format!("{title} body."),
        primary_button: primary.map(ToString::to_string),
        secondary_button: secondary.map(ToString::to_string),
    }
}

fn emit(editor: &LushtextEditorPage, notification: InlineActionNotification) {
    editor.emit_inline_notification(notification, InlineAlertLifetime::SupersededByLoad);
}

fn select(window: &LushtextWindow, editor: &LushtextEditorPage) {
    let page = window.imp().tab_view.page(editor);
    window.imp().tab_view.set_selected_page(&page);
    flush_events();
}

fn focus_widget(window: &LushtextWindow) -> Option<gtk4::Widget> {
    GtkWindowExt::focus(window)
}

/// An external write in preview-only mode shows its warning above the preview.
#[test]
fn test_external_change_in_preview_only_shows_alert_on_preview_surface() {
    let dir = tempfile::tempdir().expect("external change tempdir");
    let path = write_markdown(dir.path(), "watched.md", "Original Heading");
    let (window, editor) = markdown_in_preview_only(&path);
    wait_for_rendered(&window, "Original Heading");

    write_external_change_after_mtime_tick(&path, &editor, "# Changed Heading\n");
    let alert = preview_alert(&window);
    wait_until(Duration::from_secs(5), || {
        bar_shows(&alert, CHANGED_ON_DISK)
    });

    let imp = alert.imp();
    assert_eq!(
        imp.discard_button.label().as_deref(),
        Some("_Discard Changes and Reload")
    );
    assert!(imp.discard_button.property::<bool>("visible"));
    assert!(imp.dismiss_button.property::<bool>("visible"));
    assert!(imp.alert_box.has_css_class("warning"));
    assert!(!window.imp().editor_box.property::<bool>("visible"));
    assert_eq!(visible_surfaces_showing(&window, CHANGED_ON_DISK), 1);
    assert!(notification_evidence(&window).preview_document_alert_visible);
}

/// "Discard Changes and Reload" on the copy reloads, re-renders, and clears both.
#[test]
fn test_preview_surface_reload_refreshes_preview_and_clears_both_surfaces() {
    let dir = tempfile::tempdir().expect("reload tempdir");
    let path = write_markdown(dir.path(), "watched.md", "Original Heading");
    let (window, editor) = markdown_in_preview_only(&path);
    wait_for_rendered(&window, "Original Heading");
    write_external_change_after_mtime_tick(&path, &editor, "# Changed Heading\n");
    let alert = preview_alert(&window);
    wait_until(Duration::from_secs(5), || {
        bar_shows(&alert, CHANGED_ON_DISK)
    });

    alert.imp().discard_button.grab_focus();
    alert.imp().discard_button.emit_clicked();
    wait_until(Duration::from_secs(10), || {
        editor_text(&editor) == "# Changed Heading\n"
            && editor.load_state() == EditorLoadState::Loaded
    });
    wait_for_rendered(&window, "Changed Heading");

    assert!(bar_hidden(&alert));
    assert!(bar_hidden(editor.info_bar()));
    assert!(
        window
            .imp()
            .notification_bus
            .editor_info_bar_view(editor.notification_owner_id())
            .is_none()
    );
    let text_view = preview(&window).text_view();
    assert_eq!(
        focus_widget(&window).as_ref(),
        Some(text_view.upcast_ref::<gtk4::Widget>()),
        "focus leaves the hidden copy for the preview text"
    );
}

/// Outside preview-only the editor's own bar is the one banner.
#[test]
fn test_side_by_side_and_editor_only_show_exactly_one_alert() {
    let dir = tempfile::tempdir().expect("one banner tempdir");
    let path = write_markdown(dir.path(), "doc.md", "Heading");
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let editor = open_loaded(&window, &path);
    emit(
        &editor,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    flush_events();

    // Editor-only.
    assert!(bar_shows(editor.info_bar(), CHANGED_ON_DISK));
    assert!(bar_hidden(&preview_alert(&window)));
    assert_eq!(visible_surfaces_showing(&window, CHANGED_ON_DISK), 1);

    // Side-by-side.
    set_bool_action(&window, "set-preview-pane-visible", true);
    wait_until(Duration::from_secs(2), || {
        window.imp().preview_visible.get()
    });
    flush_events();
    assert!(bar_shows(editor.info_bar(), CHANGED_ON_DISK));
    assert!(bar_hidden(&preview_alert(&window)));
    assert_eq!(visible_surfaces_showing(&window, CHANGED_ON_DISK), 1);
    assert!(!notification_evidence(&window).preview_document_alert_visible);

    // Preview-only, then back: the copy takes over and gives it back.
    enter_preview_only(&window);
    assert!(bar_shows(&preview_alert(&window), CHANGED_ON_DISK));
    assert_eq!(visible_surfaces_showing(&window, CHANGED_ON_DISK), 1);
    leave_preview_only(&window);
    assert!(bar_hidden(&preview_alert(&window)));
    assert!(bar_shows(editor.info_bar(), CHANGED_ON_DISK));
}

/// In the narrowest supported window the side-by-side pane never covers the
/// editor's banner.
#[test]
fn test_narrow_side_by_side_keeps_the_editor_alert_uncovered() {
    let dir = tempfile::tempdir().expect("narrow tempdir");
    let path = write_markdown(dir.path(), "doc.md", "Heading");
    let window = test_window_with_restored_size(360, 640);
    present_window(&window);
    let editor = open_loaded(&window, &path);
    editor
        .info_bar()
        .imp()
        .alert_revealer
        .set_transition_duration(0);
    emit(
        &editor,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    set_bool_action(&window, "set-preview-pane-visible", true);
    let markdown = preview(&window);
    wait_until(Duration::from_secs(5), || {
        markdown.width() > 0 && editor.info_bar().width() > 0
    });
    flush_after_delay(Duration::from_millis(100));

    let bar = rect_within(editor.info_bar(), &window);
    let pane = rect_within(&markdown, &window);
    assert!(bar.width() > 0.0 && bar.height() > 0.0);
    assert!(
        bar.x() >= 0.0 && bar.x() + bar.width() <= window.width() as f32 + 0.5,
        "the editor alert stays inside the window ({bar:?})"
    );
    assert!(
        bar.intersection(&pane).is_none(),
        "the preview pane must not cover the editor alert (bar {bar:?}, pane {pane:?})"
    );
    assert!(bar_hidden(&preview_alert(&window)));
}

/// Tab switches re-project the copy, and an alert stays owned by its tab.
#[test]
fn test_tab_switch_and_close_reproject_the_preview_surface() {
    let dir = tempfile::tempdir().expect("tab switch tempdir");
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let plain = open_loaded(&window, &write_markdown(dir.path(), "a.md", "A"));
    let alerted = open_loaded(&window, &write_markdown(dir.path(), "b.md", "B"));
    let spare = open_loaded(&window, &write_markdown(dir.path(), "c.md", "C"));
    emit(
        &alerted,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    select(&window, &plain);
    enter_preview_only(&window);
    let alert = preview_alert(&window);
    assert!(bar_hidden(&alert), "tab A has no alert");

    select(&window, &alerted);
    assert!(bar_shows(&alert, CHANGED_ON_DISK));

    select(&window, &plain);
    assert!(bar_hidden(&alert));
    assert!(
        window
            .imp()
            .notification_bus
            .editor_info_bar_view(alerted.notification_owner_id())
            .is_some(),
        "tab B keeps its alert"
    );
    select(&window, &alerted);
    assert!(bar_shows(&alert, CHANGED_ON_DISK));

    // An action after an ordinary switch acts on the alert now shown.
    emit(&spare, warning("Restored from Local History", None, None));
    select(&window, &spare);
    assert!(bar_shows(&alert, "Restored from Local History"));
    alert.imp().dismiss_button.emit_clicked();
    flush_events();
    assert!(
        window
            .imp()
            .notification_bus
            .editor_info_bar_view(spare.notification_owner_id())
            .is_none(),
        "dismiss ran for the newly selected tab"
    );
    assert!(
        window
            .imp()
            .notification_bus
            .editor_info_bar_view(alerted.notification_owner_id())
            .is_some(),
        "and left the other tab's alert alone"
    );

    // Closing the selected alert tab re-projects for the next selection.
    select(&window, &alerted);
    assert!(bar_shows(&alert, CHANGED_ON_DISK));
    window
        .imp()
        .tab_view
        .close_page(&window.imp().tab_view.page(&alerted));
    flush_events();
    assert!(!window.imp().preview_mode.get() || bar_hidden(&alert));
}

/// A copy projected for one tab acts on nothing once the selection moved
/// during a deferred tab-projection batch.
#[test]
fn test_stale_preview_surface_forward_runs_no_handler() {
    let dir = tempfile::tempdir().expect("stale forward tempdir");
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let first = open_loaded(&window, &write_markdown(dir.path(), "a.md", "A"));
    let second = open_loaded(&window, &write_markdown(dir.path(), "b.md", "B"));
    first.buffer().set_text("# A\n\nlocal edits\n");
    first.buffer().set_modified(true);
    second.buffer().set_text("# B\n\nlocal edits\n");
    second.buffer().set_modified(true);
    emit(
        &first,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    emit(
        &second,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    select(&window, &first);
    enter_preview_only(&window);
    let alert = preview_alert(&window);
    assert!(bar_shows(&alert, CHANGED_ON_DISK));
    let projected_for = notification_evidence(&window)
        .preview_document_alert_projection
        .expect("projected")
        .0;
    assert_eq!(projected_for, first.notification_owner_id());

    // A session-restore batch or bulk close defers tab-derived projections.
    let depth = &window.imp().tab_projection_refresh_defer_depth;
    depth.set(depth.get() + 1);
    select(&window, &second);
    alert.imp().discard_button.emit_clicked();
    flush_after_delay(Duration::from_millis(300));
    depth.set(depth.get() - 1);

    for editor in [&first, &second] {
        assert!(editor.is_modified(), "no reload ran");
        assert!(editor_text(editor).contains("local edits"));
        assert!(
            window
                .imp()
                .notification_bus
                .editor_info_bar_view(editor.notification_owner_id())
                .is_some(),
            "no alert was resolved"
        );
    }
    assert_eq!(
        notification_evidence(&window)
            .preview_document_alert_projection
            .map(|(owner, _)| owner),
        Some(second.notification_owner_id()),
        "the copy was re-projected for the newly selected tab"
    );
}

/// Dismiss and Save on the copy run the editor's own handlers.
#[test]
fn test_preview_surface_dismiss_and_save_run_editor_handlers() {
    let dir = tempfile::tempdir().expect("dismiss save tempdir");
    let path = write_markdown(dir.path(), "doc.md", "Heading");
    let (window, editor) = markdown_in_preview_only(&path);
    editor.buffer().set_text("# Restored\n");
    editor.buffer().set_modified(true);
    emit(
        &editor,
        warning("Draft Changes Restored", Some("_Discard…"), Some("_Save…")),
    );
    let alert = preview_alert(&window);
    assert!(bar_shows(&alert, "Draft Changes Restored"));
    assert!(alert.imp().save_button.property::<bool>("visible"));

    alert.imp().save_button.emit_clicked();
    wait_until(Duration::from_secs(10), || {
        !editor.is_modified() && fs_read::text(&path).ok().as_deref() == Some("# Restored\n")
    });

    emit(
        &editor,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    assert!(bar_shows(&alert, CHANGED_ON_DISK));
    editor.buffer().set_text("# Kept\n");
    alert.imp().dismiss_button.grab_focus();
    alert.imp().dismiss_button.emit_clicked();
    flush_events();
    assert!(bar_hidden(&alert));
    assert_eq!(editor_text(&editor), "# Kept\n", "dismiss keeps the buffer");
    assert_eq!(
        focus_widget(&window).as_ref(),
        Some(preview(&window).text_view().upcast_ref::<gtk4::Widget>()),
        "focus moves to the preview text after Dismiss"
    );
    leave_preview_only(&window);
    assert!(bar_hidden(editor.info_bar()));
}

/// Leaving preview-only while focus is on the copy hands focus to the
/// document, not to the copy's now-hidden button.
#[test]
fn test_leaving_preview_only_does_not_strand_focus_on_the_copy() {
    let dir = tempfile::tempdir().expect("leave preview focus tempdir");
    let path = write_markdown(dir.path(), "doc.md", "Heading");
    let (window, editor) = markdown_in_preview_only(&path);
    emit(
        &editor,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    let alert = preview_alert(&window);
    assert!(bar_shows(&alert, CHANGED_ON_DISK));
    alert.imp().dismiss_button.grab_focus();
    flush_events();

    leave_preview_only(&window);
    assert!(bar_hidden(&alert));
    assert!(bar_shows(editor.info_bar(), CHANGED_ON_DISK));
    let focus = focus_widget(&window);
    assert!(
        focus.as_ref().is_none_or(
            |focus| !focus.is_ancestor(&alert) && focus != alert.upcast_ref::<gtk4::Widget>()
        ),
        "focus must not stay inside the hidden copy ({focus:?})"
    );
    assert_eq!(
        focus.as_ref(),
        Some(editor.source_view().upcast_ref::<gtk4::Widget>()),
        "focus returns to the document's editor"
    );
}

/// A resolve the user did not start from the copy moves no focus, even when
/// the window has no focus widget at that moment.
#[test]
fn test_background_resolve_of_the_copy_does_not_move_focus() {
    let dir = tempfile::tempdir().expect("background resolve tempdir");
    let path = write_markdown(dir.path(), "doc.md", "Heading");
    let (window, editor) = markdown_in_preview_only(&path);
    wait_for_rendered(&window, "Heading");
    emit(
        &editor,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    let alert = preview_alert(&window);
    assert!(bar_shows(&alert, CHANGED_ON_DISK));
    GtkWindowExt::set_focus(&window, None::<&gtk4::Widget>);
    flush_events();

    window.dismiss_editor_notifications(&editor);
    flush_events();
    assert!(bar_hidden(&alert));
    let focus = focus_widget(&window);
    assert!(
        focus.is_none(),
        "a resolve not started from the copy must not move focus ({focus:?})"
    );
}

/// A non-Markdown document's alert shows above the "Not a Markdown file"
/// placeholder; Escape leaves it; Dismiss leaves focus nowhere.
#[test]
fn test_non_markdown_alert_shows_above_placeholder_and_survives_escape() {
    let dir = tempfile::tempdir().expect("non markdown tempdir");
    let path = dir.path().join("notes.rs");
    fixture::write_text(&path, "fn main() {}\n");
    let (window, editor) = markdown_in_preview_only(&path);
    emit(
        &editor,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    let alert = preview_alert(&window);
    assert!(bar_shows(&alert, CHANGED_ON_DISK));
    let markdown = preview(&window);
    wait_until(Duration::from_secs(5), || {
        !markdown.is_showing_content()
            && markdown.evidence().placeholder_description.as_deref() == Some(NOT_MARKDOWN)
    });

    alert.imp().dismiss_button.grab_focus();
    emit_key_pressed_on_focus(&window, gtk4::gdk::Key::Escape);
    flush_events();
    assert!(
        bar_shows(&alert, CHANGED_ON_DISK),
        "Escape does not dismiss alerts"
    );

    alert.imp().dismiss_button.grab_focus();
    alert.imp().dismiss_button.emit_clicked();
    flush_events();
    assert!(bar_hidden(&alert));
    let focus = focus_widget(&window);
    assert!(
        focus.is_none(),
        "with no preview text shown, focus goes nowhere, not to the sidebar ({focus:?})"
    );
}

/// The copy wraps its actions beneath the message when its column is narrow.
///
/// The preview column is never narrower than about 600 px: 640 is the window's
/// minimum width. At normal text size the message label wraps its own text
/// first and the action group keeps sharing the row there, so the constrained
/// case is enlarged text in the narrowest window (a `gtk-xft-dpi` of twice the
/// default, the Text Scaling accessibility setting at 2.0).
#[test]
fn test_preview_surface_alert_wraps_actions_when_narrow_and_shares_a_row_when_wide() {
    for (width, text_scale, wraps) in [(640, 2, true), (1400, 1, false)] {
        ensure_gtk_init();
        gtk4::Settings::default()
            .expect("GTK settings")
            .set_gtk_xft_dpi(96 * 1024 * text_scale);
        let dir = tempfile::tempdir().expect("wrap tempdir");
        let path = write_markdown(dir.path(), "doc.md", "Heading");
        let window = test_window_with_restored_size(width, 700);
        present_window(&window);
        let editor = open_loaded(&window, &path);
        let alert = preview_alert(&window);
        alert.imp().alert_revealer.set_transition_duration(0);
        enter_preview_only(&window);
        emit(
            &editor,
            InlineActionNotification {
                style: InlineNotificationStyle::Warning,
                title: "Draft Changes Restored".to_string(),
                body: "Unsaved changes from a previous session have been restored.".to_string(),
                primary_button: Some("_Discard…".to_string()),
                secondary_button: Some("_Save…".to_string()),
            },
        );
        let imp = alert.imp();
        wait_until(Duration::from_secs(5), || {
            imp.actions_box.width() > 0
                && imp.discard_button.width() > 0
                && imp.save_button.width() > 0
                && imp.dismiss_button.width() > 0
        });
        flush_after_delay(Duration::from_millis(100));

        let wrap = imp.content_wrap.get();
        let message_box = wrap.first_child().expect("message box");
        let msg = rect_within(&message_box, &wrap);
        let act = rect_within(&*imp.actions_box, &wrap);
        if wraps {
            assert!(
                act.y() >= msg.y() + msg.height() - 1.0,
                "narrow: actions wrap beneath the message (msg {msg:?}, act {act:?})"
            );
            let discard = rect_within(&*imp.discard_button, &*imp.actions_box);
            let save = rect_within(&*imp.save_button, &*imp.actions_box);
            let dismiss = rect_within(&*imp.dismiss_button, &*imp.actions_box);
            assert!(discard.width() > 0.0 && save.width() > 0.0 && dismiss.width() > 0.0);
            let center = |r: &gtk4::graphene::Rect| r.y() + r.height() / 2.0;
            assert!(
                (center(&discard) - center(&dismiss)).abs() < 1.0
                    && (center(&save) - center(&dismiss)).abs() < 1.0,
                "the action buttons stay on one row (discard {discard:?}, save {save:?}, dismiss {dismiss:?})"
            );
            assert!(
                discard.x() < save.x() && save.x() < dismiss.x(),
                "Discard, Save, then dismiss, side by side"
            );
            assert!(
                act.x() + act.width() <= wrap.width() as f32 + 0.5,
                "the action group stays fully visible"
            );
        } else {
            assert!(
                act.y() < msg.y() + msg.height(),
                "wide: actions share the message row (msg {msg:?}, act {act:?})"
            );
        }
    }
}

/// Each window projects and announces only its own selected tab.
#[test]
fn test_two_windows_project_only_their_own_alerts() {
    let dir = tempfile::tempdir().expect("two windows tempdir");
    ensure_gtk_init();
    let window_a = test_window();
    present_window(&window_a);
    let editor_a = open_loaded(&window_a, &write_markdown(dir.path(), "a.md", "A"));
    let window_b = test_window();
    present_window(&window_b);
    let editor_b = open_loaded(&window_b, &write_markdown(dir.path(), "b.md", "B"));
    enter_preview_only(&window_a);
    let a_before = notification_evidence(&window_a).announcements;
    let b_before = notification_evidence(&window_b).announcements;

    emit(&editor_a, warning("Alert A", None, None));
    emit(&editor_b, warning("Alert B", None, None));

    assert!(bar_shows(&preview_alert(&window_a), "Alert A"));
    assert!(bar_hidden(&preview_alert(&window_b)));
    assert!(bar_shows(editor_b.info_bar(), "Alert B"));
    let a_after = notification_evidence(&window_a).announcements;
    let b_after = notification_evidence(&window_b).announcements;
    assert_eq!(a_after.preview_surface, a_before.preview_surface + 1);
    assert_eq!(a_after.editor_info_bar, a_before.editor_info_bar);
    assert_eq!(b_after.editor_info_bar, b_before.editor_info_bar + 1);
    assert_eq!(b_after.preview_surface, b_before.preview_surface);
    window_b.close();
}

/// Accessibility of the copy, and announcing each alert once.
#[test]
fn test_preview_surface_accessibility_and_announce_once() {
    let dir = tempfile::tempdir().expect("a11y tempdir");
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let first = open_loaded(&window, &write_markdown(dir.path(), "a.md", "A"));
    let second = open_loaded(&window, &write_markdown(dir.path(), "b.md", "B"));
    let counts = |window: &LushtextWindow| notification_evidence(window).announcements;
    let start = counts(&window);

    // Announced from the editor's bar, then preview-only: no second announcement.
    emit(
        &second,
        warning("Draft Changes Restored", Some("_Discard…"), Some("_Save…")),
    );
    assert_eq!(counts(&window).editor_info_bar, start.editor_info_bar + 1);
    enter_preview_only(&window);
    let alert = preview_alert(&window);
    assert!(bar_shows(&alert, "Draft Changes Restored"));
    assert_eq!(counts(&window).preview_surface, start.preview_surface);

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .assert_on(&*alert.imp().alert_box);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Alert)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*alert.imp().alert_title);
    for button in [
        &*alert.imp().discard_button,
        &*alert.imp().save_button,
        &*alert.imp().dismiss_button,
    ] {
        assert!(button.is_visible() && button.is_sensitive() && button.is_focusable());
    }
    assert!(
        !second.info_bar().is_mapped(),
        "the editor's bar is not exposed"
    );

    // A new alert in preview-only is announced once, from the copy.
    emit(
        &second,
        warning(CHANGED_ON_DISK, Some("_Discard Changes and Reload"), None),
    );
    assert_eq!(counts(&window).preview_surface, start.preview_surface + 1);
    window.render_notifications();
    window.publish_status_message("Unrelated", NotificationSeverity::Info);
    assert_eq!(counts(&window).preview_surface, start.preview_surface + 1);

    // A background-tab alert: silent when raised, once when selected, never again.
    emit(&first, warning("Restored from Local History", None, None));
    assert_eq!(counts(&window).preview_surface, start.preview_surface + 1);
    select(&window, &first);
    assert_eq!(counts(&window).preview_surface, start.preview_surface + 2);
    select(&window, &second);
    select(&window, &first);
    assert_eq!(counts(&window).preview_surface, start.preview_surface + 2);
    assert_eq!(counts(&window).editor_info_bar, start.editor_info_bar + 1);
}

/// Proof 1 for the preview fields: reading the surface changes nothing.
#[test]
fn test_preview_surface_evidence_reads_stay_side_effect_free() {
    let dir = tempfile::tempdir().expect("evidence tempdir");
    let path = write_markdown(dir.path(), "doc.md", "Heading");
    let (window, editor) = markdown_in_preview_only(&path);
    let read_twice = |window: &LushtextWindow| {
        let first = notification_evidence(window);
        assert_eq!(first, notification_evidence(window));
        first
    };
    assert!(!read_twice(&window).preview_document_alert_visible);
    emit(&editor, warning(CHANGED_ON_DISK, None, None));
    let shown = read_twice(&window);
    assert!(shown.preview_document_alert_visible);
    assert_eq!(
        shown
            .preview_document_alert_projection
            .map(|(owner, _)| owner),
        Some(editor.notification_owner_id())
    );
    leave_preview_only(&window);
    let left = read_twice(&window);
    assert!(!left.preview_document_alert_visible);
    assert_eq!(left.preview_document_alert_projection, None);
    // SAFETY: this test window is disposed exactly once, and the assertions
    // afterwards only read the evidence surface.
    unsafe { window.run_dispose() };
    assert!(!notification_evidence(&window).preview_document_alert_visible);
}

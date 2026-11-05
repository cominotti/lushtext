// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for the Protected View reading surfaces
//! (`protected-view-inspector-and-surfaces`): the Line Inspector, Copy Whole
//! Line, the fold gutter, the alert and notice, and navigation and restore
//! through the navigation mark.
//!
//! Every fixture is synthetic. Thresholds come from `lushtext_core::model`,
//! never restated.

use crate::common::{
    activate_action, active_editor, editor_text, ensure_gtk_init, fixture, flush_events,
    present_window, test_application, test_window, wait_until,
};
use gtk4::prelude::*;
use libadwaita::prelude::{AdwApplicationWindowExt, AdwDialogExt, AlertDialogExt};
use lushtext_core::model::document_scale::L_PROTECT_EDIT;
use lushtext_core::model::protected_fold::N_INSPECT;
use lushtext_core::ui::editor_page::{
    EditorLoadState, InspectorRequest, LushtextEditorPage, WholeLineProvider,
};
use sourceview5::prelude::ViewExt as _;
use std::time::Duration;

fn n(value: u64) -> usize {
    usize::try_from(value).expect("policy constants fit usize")
}

fn present(page: &LushtextEditorPage) -> gtk4::ApplicationWindow {
    let app = test_application();
    let window = gtk4::ApplicationWindow::builder()
        .application(&app)
        .default_width(1000)
        .default_height(600)
        .child(page)
        .build();
    present_window(&window);
    window
}

fn load_and_wait(page: &LushtextEditorPage, path: &std::path::Path) {
    page.load_file_async(path);
    wait_until(Duration::from_secs(60), || {
        page.load_evidence().load_state == EditorLoadState::Loaded && !page.is_saving()
    });
    flush_events();
}

/// A giant paragraph whose every character is determined by its column, with
/// multibyte characters, so a wrong window cut shows up as wrong text.
fn giant_paragraph(chars: usize) -> String {
    (0..chars)
        .map(|column| match column % 97 {
            0 => 'é',
            1 => '€',
            value => char::from(b'a' + u8::try_from(value % 26).expect("fits")),
        })
        .collect()
}

/// `short\n`, the giant paragraph (line 1), `tail\n`.
fn giant_body(chars: usize) -> (String, String) {
    let giant = giant_paragraph(chars);
    (format!("short head\n{giant}\ntail line\n"), giant)
}

fn protected_page(body: &str, name: &str) -> (LushtextEditorPage, gtk4::ApplicationWindow) {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("surfaces tempdir");
    let path = dir.path().join(format!("{name}.txt"));
    fixture::write_text(&path, body);
    let page = LushtextEditorPage::new();
    let window = present(&page);
    load_and_wait(&page, &path);
    assert!(page.is_protected_view(), "{name}: protected");
    std::mem::forget(dir);
    (page, window)
}

fn wait_for_window(page: &LushtextEditorPage) {
    wait_until(Duration::from_secs(30), || {
        let evidence = page.protected_view_evidence();
        evidence.inspector_visible && !evidence.inspector_read_pending
    });
}

fn inspector_text(page: &LushtextEditorPage) -> String {
    let view = page.line_inspector().expect("inspector open").window_view();
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, true).to_string()
}

fn insert_offset(page: &LushtextEditorPage) -> i32 {
    let buffer = page.buffer();
    buffer.iter_at_mark(&buffer.get_insert()).offset()
}

#[test]
fn test_the_line_inspector_reads_windows_in_the_middle_of_a_giant_paragraph() {
    let chars = 300_000;
    let (body, giant) = giant_body(chars);
    let (page, _window) = protected_page(&body, "inspect");
    let insert = insert_offset(&page);
    let columns: Vec<char> = giant.chars().collect();
    let width = n(N_INSPECT);

    page.open_line_inspector(1, 150_123);
    wait_for_window(&page);
    assert!(crate::common::assert_monospace_widgets_registered("line inspector") > 0);
    let start = 150_123 / width * width;
    let expected: String = columns[start..start + width].iter().collect();
    assert_eq!(
        inspector_text(&page),
        expected,
        "the window holds the column"
    );
    let evidence = page.protected_view_evidence();
    let (paragraph, first, end) = evidence.inspector_window.expect("a window is shown");
    assert_eq!((paragraph, n(first), n(end)), (1, start, start + width));
    let summary = page.line_inspector().expect("open").summary();
    assert!(summary.starts_with("Line 2, columns"), "{summary}");
    assert!(summary.contains(&chars.to_string()), "{summary}");

    let inspector = page.line_inspector().expect("open");
    inspector.activate(InspectorRequest::NextWindow);
    wait_for_window(&page);
    let next: String = columns[start + width..start + 2 * width].iter().collect();
    assert_eq!(inspector_text(&page), next, "Next Window");
    inspector.activate(InspectorRequest::PreviousWindow);
    inspector.activate(InspectorRequest::PreviousWindow);
    wait_for_window(&page);
    let previous: String = columns[start - width..start].iter().collect();
    assert_eq!(
        inspector_text(&page),
        previous,
        "only the latest of two requests publishes"
    );

    inspector.activate(InspectorRequest::GoTo {
        line: 2,
        column: u64::try_from(chars).expect("fits"),
    });
    wait_for_window(&page);
    let last_start = (chars - 1) / width * width;
    let last: String = columns[last_start..].iter().collect();
    assert_eq!(inspector_text(&page), last, "the last window ends the line");

    assert_eq!(editor_text(&page), body, "the inspector changed nothing");
    assert_eq!(insert_offset(&page), insert, "the insert mark did not move");
    assert!(page.protected_view_evidence().inspector_windows_published >= 4);
}

/// Read the clipboard's whole content as UTF-8 bytes, pumping the main loop.
fn clipboard_bytes(page: &LushtextEditorPage) -> Vec<u8> {
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot = std::rc::Rc::clone(&result);
    page.source_view()
        .clipboard()
        .read_text_async(None::<&gio::Cancellable>, move |text| {
            slot.replace(Some(text.ok().flatten().map(|text| text.to_string())));
        });
    wait_until(Duration::from_secs(60), || result.borrow().is_some());
    result
        .take()
        .flatten()
        .map(String::into_bytes)
        .unwrap_or_default()
}

#[test]
fn test_copy_whole_line_writes_the_exact_paragraph_in_bounded_chunks() {
    let chars = 300_000;
    let (body, giant) = giant_body(chars);
    let (page, _window) = protected_page(&body, "copy");
    page.source_view().clipboard().set_text("");
    page.copy_protected_whole_line(1);
    wait_until(Duration::from_secs(30), || {
        page.protected_view_evidence().whole_line_copies == 1
    });
    let provider = page
        .source_view()
        .clipboard()
        .content()
        .and_downcast::<WholeLineProvider>()
        .expect("the lazy whole-line provider is on the clipboard");
    assert_eq!(provider.byte_len(), giant.len());
    assert_eq!(
        provider.chunks_written(),
        0,
        "nothing is written until read"
    );

    let copied = clipboard_bytes(&page);
    assert_eq!(copied.len(), giant.len(), "every byte of the line");
    assert!(
        copied == giant.as_bytes(),
        "byte-identical to the paragraph"
    );
    let chunk = lushtext_core::ui::editor_page::WHOLE_LINE_CHUNK_BYTES;
    assert!(
        provider.chunks_written() >= u64::try_from(giant.len().div_ceil(chunk)).expect("fits"),
        "written one bounded chunk per dispatch"
    );
    assert_eq!(editor_text(&page), body);
}

#[test]
fn test_the_fold_gutter_follows_the_session() {
    let (body, _) = giant_body(n(L_PROTECT_EDIT) + 1);
    let (page, _window) = protected_page(&body, "gutter");
    assert!(page.protected_view_evidence().fold_gutter_installed);
    // A reload of a small file leaves Protected View and takes the gutter,
    // and any open inspector, with it.
    page.open_line_inspector(1, 0);
    wait_for_window(&page);
    let dir = tempfile::tempdir().expect("gutter tempdir");
    let path = dir.path().join("small.txt");
    fixture::write_text(&path, "small\n");
    load_and_wait(&page, &path);
    assert!(!page.is_protected_view());
    let evidence = page.protected_view_evidence();
    assert!(!evidence.fold_gutter_installed);
    assert!(!evidence.inspector_visible);
}

#[test]
fn test_find_navigation_to_a_hidden_match_opens_the_inspector_on_it() {
    let giant = format!(
        "{}zqzhidden{}",
        "h".repeat(n(L_PROTECT_EDIT) + 100),
        "t".repeat(1_000)
    );
    let body = format!("short zqzhidden line\n{giant}\nshort\n");
    let (page, _window) = protected_page(&body, "find");
    page.show_search();
    flush_events();
    page.search_bar().search_entry().set_text("zqzhidden");
    wait_until(Duration::from_secs(30), || {
        page.protected_view_evidence().find_matches == 2
    });
    assert!(
        !page.protected_view_evidence().inspector_visible,
        "the automatic first match does not pop the inspector"
    );
    page.search_bar().move_next();
    wait_for_window(&page);
    let (paragraph, start, end) = page
        .protected_view_evidence()
        .inspector_window
        .expect("window");
    let column = u64::try_from(n(L_PROTECT_EDIT) + 100).expect("fits");
    assert_eq!(paragraph, 1);
    assert!(
        start <= column && column < end,
        "the window holds the match"
    );
    let view = page.line_inspector().expect("open").window_view();
    let (from, to) = view
        .buffer()
        .selection_bounds()
        .expect("the match is selected");
    assert_eq!(view.buffer().text(&from, &to, true), "zqzhidden");
}

#[test]
fn test_session_restore_scrolls_without_a_caret_and_keeps_the_cursor() {
    ensure_gtk_init();
    let (body, _) = giant_body(n(L_PROTECT_EDIT) + 1);
    let dir = tempfile::tempdir().expect("restore tempdir");
    let path = dir.path().join("restore.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    page.set_restore_position(1, 40_000, 1);
    load_and_wait(&page, &path);
    assert!(page.is_protected_view());
    assert_eq!(insert_offset(&page), 0, "no caret is placed");
    assert_eq!(
        page.cursor_position(),
        (1, 40_000),
        "the persisted cursor survives for the next session save"
    );
}

#[test]
fn test_the_alert_appears_once_and_blocked_requests_are_rate_limited() {
    let (body, _) = giant_body(n(L_PROTECT_EDIT) + 1);
    let dir = tempfile::tempdir().expect("alert tempdir");
    let path = dir.path().join("alert.txt");
    fixture::write_text(&path, &body);
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let page = active_editor(&window);
    wait_until(Duration::from_secs(60), || {
        page.load_evidence().load_state == EditorLoadState::Loaded
            && page.protected_view_evidence().alerts_published == 1
    });
    assert!(
        page.info_bar().property::<bool>("visible"),
        "the alert is shown"
    );
    for _ in 0..10 {
        page.source_view()
            .emit_by_name::<()>("paste-clipboard", &[]);
    }
    flush_events();
    let evidence = page.protected_view_evidence();
    assert_eq!(evidence.notices_published, 1, "one rate-limited notice");
    assert_eq!(evidence.alerts_published, 1, "no alert is stacked");
    assert_eq!(editor_text(&page), body);

    // Bookmarks have no caret line: the toggle is refused with its notice.
    activate_action(&window, "toggle-bookmark");
    assert_eq!(page.protected_view_evidence().notices_published, 2);

    // Inspect Line from the keyboard path opens on the folded paragraph.
    activate_action(&window, "inspect-line");
    wait_for_window(&page);
    assert_eq!(
        page.protected_view_evidence()
            .inspector_window
            .map(|window| window.0),
        Some(1)
    );
    // The window's surface snapshot projects the inspector.
    let app = window
        .application()
        .expect("window application")
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("LushtextApplication");
    let snapshot = lushtext_core::ui::automation::app_snapshot(&app);
    assert!(
        snapshot
            .window
            .expect("window snapshot")
            .surfaces
            .line_inspector_visible
    );
}

#[test]
fn test_the_alert_undo_returns_a_paste_grown_document_to_editable() {
    ensure_gtk_init();
    let body = "first line\nsecond line\n";
    let dir = tempfile::tempdir().expect("undo tempdir");
    let path = dir.path().join("undo.txt");
    fixture::write_text(&path, body);
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let page = active_editor(&window);
    wait_until(Duration::from_secs(30), || {
        page.load_evidence().load_state == EditorLoadState::Loaded
    });
    let buffer = page.buffer();
    let mut end = buffer.iter_at_line(0).expect("line 0");
    end.forward_to_line_end();
    buffer.insert_interactive(&mut end, &"p".repeat(n(L_PROTECT_EDIT) + 1), true);
    assert!(page.is_protected_view());
    wait_until(Duration::from_secs(10), || {
        page.protected_view_evidence().alerts_published == 1
    });
    assert!(page.protected_view_evidence().entry_undo_available);
    let primary = crate::common::find_descendant(page.info_bar().upcast_ref(), |widget| {
        widget.buildable_id().as_deref() == Some("discard_button")
    })
    .and_downcast::<gtk4::Button>()
    .expect("the alert's primary button");
    assert_eq!(primary.label().as_deref(), Some("_Undo Edit"));
    primary.emit_clicked();
    flush_events();
    assert!(
        !page.is_protected_view(),
        "the alert's Undo Edit left Protected View"
    );
    assert_eq!(editor_text(&page), body);
    assert!(
        !page.info_bar().property::<bool>("visible"),
        "the alert is withdrawn"
    );
}

/// Open `bytes` as a protected Shift_JIS document whose decode does not round
/// trip (0xED40 decodes without errors but encodes back as another code).
fn open_inexact_shift_jis(
    name: &str,
) -> (
    lushtext_core::ui::window::LushtextWindow,
    LushtextEditorPage,
    std::path::PathBuf,
    Vec<u8>,
) {
    ensure_gtk_init();
    let mut bytes = b"head\r\n".to_vec();
    bytes.extend(std::iter::repeat_n(b'a', n(L_PROTECT_EDIT) + 1));
    bytes.extend_from_slice(&[0xED, 0x40, b'\n']);
    let dir = tempfile::tempdir().expect("save-as tempdir");
    let path = dir.path().join(format!("{name}.txt"));
    fixture::write_bytes(&path, &bytes);
    std::mem::forget(dir);
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let page = active_editor(&window);
    wait_until(Duration::from_secs(60), || {
        page.load_evidence().load_state == EditorLoadState::Loaded
    });
    page.load_file_async_with_encoding(
        &path,
        Some(lushtext_core::model::encoding::DocumentEncoding::ShiftJis),
    );
    wait_until(Duration::from_secs(60), || {
        page.load_evidence().load_state == EditorLoadState::Loaded
            && page.document_encoding_state().opened_encoding
                == lushtext_core::model::encoding::DocumentEncoding::ShiftJis
    });
    assert!(page.is_protected_view());
    assert!(page.decode_is_inexact());
    (window, page, path, bytes)
}

#[test]
fn test_save_as_of_an_unmodified_inexact_decode_copies_the_file_byte_for_byte() {
    let (window, page, path, bytes) = open_inexact_shift_jis("exact-copy");
    let destination = path.with_file_name("exact-copy-saved.txt");
    window.show_save_as_dialog();
    flush_events();
    assert_eq!(
        page.protected_view_evidence().notices_published,
        0,
        "an unmodified document's Save As is not refused"
    );
    window.select_save_as_destination_for_test(&destination);
    wait_until(Duration::from_secs(60), || {
        !page.is_saving() && page.file_path().as_deref() == Some(destination.as_path())
    });
    assert_eq!(
        fixture::read_bytes(&destination),
        bytes,
        "the copy holds exactly the loaded bytes"
    );
    assert!(
        page.decode_is_inexact(),
        "the saved file still decodes inexactly"
    );
    assert!(!page.is_modified());
}

#[test]
fn test_save_as_of_an_edited_inexact_decode_is_refused_with_a_notice() {
    let (window, page, _path, _bytes) = open_inexact_shift_jis("edited-inexact");
    // An edited protected document (as after an edit-growth entry).
    page.buffer().set_modified(true);
    let before = page.protected_view_evidence().notices_published;
    window.show_save_as_dialog();
    flush_events();
    assert_eq!(
        page.protected_view_evidence().notices_published,
        before + 1,
        "Save As is refused with its notice instead of opening the chooser"
    );
}

#[test]
fn test_every_toolkit_binding_of_the_protected_view_is_classified() {
    let (body, _) = giant_body(n(L_PROTECT_EDIT) + 1);
    let (page, _window) = protected_page(&body, "bindings");
    let view = page.source_view();
    let controllers = view.observe_controllers();
    let mut names = Vec::new();
    for index in 0..controllers.n_items() {
        let Some(controller) = controllers
            .item(index)
            .and_downcast::<gtk4::ShortcutController>()
        else {
            continue;
        };
        for shortcut in 0..controller.n_items() {
            let Some(action) = controller
                .item(shortcut)
                .and_downcast::<gtk4::Shortcut>()
                .and_then(|shortcut| shortcut.action())
            else {
                continue;
            };

            if let Some(signal) = action.downcast_ref::<gtk4::SignalAction>() {
                names.push(signal.signal_name().to_string());
            } else if let Some(named) = action.downcast_ref::<gtk4::NamedAction>() {
                names.push(named.action_name().to_string());
            }
        }
    }
    assert!(
        names.iter().any(|name| name == "select-all"),
        "the enumeration reaches the class bindings: {names:?}"
    );
    let unclassified: Vec<&String> = names
        .iter()
        .filter(|name| !lushtext_core::ui::editor_page::classifies_toolkit_binding(name))
        .collect();
    assert!(
        unclassified.is_empty(),
        "every bound signal and action has a disposition: {unclassified:?}"
    );
    // Drops are refused: the view has no drop target while protected.
    let drop_targets = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter(|item| item.is::<gtk4::DropTarget>() || item.is::<gtk4::DropTargetAsync>())
        .count();
    assert_eq!(drop_targets, 0, "no drop target while protected");
}

#[test]
fn test_the_clear_key_action_leaves_a_protected_document_unchanged() {
    // GtkTextView binds the Clear key to `text.clear`, which empties the
    // buffer; the class-shortcut enumeration found it unclassified.
    let (body, _) = giant_body(n(L_PROTECT_EDIT) + 1);
    let (page, _window) = protected_page(&body, "clear");
    let _ = page.source_view().activate_action("text.clear", None);
    flush_events();
    assert_eq!(editor_text(&page), body, "text.clear changed nothing");
    assert!(!page.is_modified());
}

#[test]
fn test_a_protected_head_stays_under_the_paragraph_wrap_gate() {
    // Decision of this change (measured): protected heads keep C5's
    // per-paragraph wrap gate, so a folded head is shown unwrapped.
    let (body, _) = giant_body(n(L_PROTECT_EDIT) + 1);
    let (page, _window) = protected_page(&body, "wrap-gate");
    let buffer = page.buffer();
    let tag = buffer
        .tag_table()
        .lookup(lushtext_core::ui::editor_page::NO_WRAP_TAG)
        .expect("the wrap gate tagged the long paragraph");
    assert!(buffer.iter_at_line(1).expect("line 1").has_tag(&tag));
    assert!(!buffer.iter_at_line(0).expect("line 0").has_tag(&tag));
}

#[test]
fn test_a_protected_periodic_capture_reads_the_source_not_the_buffer() {
    // An edit-growth entry is modified, so periodic local history captures
    // it; the capture must come from the text source on the worker (no
    // chunked GTK snapshot of the protected buffer, ledger A66).
    ensure_gtk_init();
    let data_dir = crate::common::isolated_data_dir();
    let document = data_dir.path().join("periodic-protected.txt");
    fixture::write_text(&document, "first line\nsecond line\n");
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &document);
    let buffer = page.buffer();
    let mut end = buffer.iter_at_line(0).expect("line 0");
    end.forward_to_line_end();
    buffer.insert_interactive(&mut end, &"p".repeat(n(L_PROTECT_EDIT) + 1), true);
    assert!(page.is_protected_view());
    wait_until(Duration::from_secs(30), || {
        page.protected_view_evidence().source_bytes.is_some()
    });
    let expected = editor_text(&page);
    use lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Periodic;

    page.run_local_history_periodic_capture_for_test();
    assert!(
        !page.local_history_evidence().periodic_snapshot_inflight,
        "no chunked buffer snapshot runs for a protected document"
    );
    wait_until(Duration::from_secs(30), || {
        lushtext_core::services::local_history_service::list_snapshots_for_path(
            data_dir.path(),
            &document,
        )
        .is_ok_and(|snapshots| snapshots.iter().any(|meta| meta.origin == Periodic))
    });
    let snapshots = lushtext_core::services::local_history_service::list_snapshots_for_path(
        data_dir.path(),
        &document,
    )
    .expect("list snapshots");
    let latest = snapshots
        .iter()
        .find(|meta| meta.origin == Periodic)
        .expect("a periodic snapshot");
    let loaded = lushtext_core::services::local_history_service::load_snapshot_for_path(
        data_dir.path(),
        &document,
        &latest.snapshot_id,
    )
    .expect("load snapshot")
    .expect("the snapshot exists");
    assert_eq!(loaded.text, expected, "the snapshot is the whole text");
}

/// Every `GtkTextView` under `root`, in tree order.
fn text_views_under(root: &gtk4::Widget) -> Vec<gtk4::TextView> {
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        if let Ok(view) = widget.clone().downcast::<gtk4::TextView>() {
            found.push(view);
        }
        let mut child = widget.last_child();
        while let Some(next) = child {
            child = next.prev_sibling();
            stack.push(next);
        }
    }
    found
}

/// Whether `buffer` holds the fold tag (it was installed folded).
fn buffer_is_folded(buffer: &gtk4::TextBuffer) -> bool {
    buffer
        .tag_table()
        .lookup(lushtext_core::ui::protected_fold::FOLD_TAG_NAME)
        .is_some()
}

/// Read a clipboard's whole text, pumping the main loop.
fn read_clipboard(clipboard: &gtk4::gdk::Clipboard) -> String {
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot = std::rc::Rc::clone(&result);
    clipboard.read_text_async(None::<&gio::Cancellable>, move |text| {
        slot.replace(Some(
            text.ok()
                .flatten()
                .map(|text| text.to_string())
                .unwrap_or_default(),
        ));
    });
    wait_until(Duration::from_secs(60), || result.borrow().is_some());
    result.take().unwrap_or_default()
}

/// Copy `view`'s selection the way Ctrl+C and the context menu do (the
/// `copy-clipboard` signal), and return what the clipboard then holds.
fn copy_selection_and_read(view: &gtk4::TextView) -> String {
    let clipboard = view.clipboard();
    clipboard.set_text("sentinel");
    let sentinel = clipboard.content();
    view.emit_by_name::<()>("copy-clipboard", &[]);
    wait_until(Duration::from_secs(60), || clipboard.content() != sentinel);
    read_clipboard(&clipboard)
}

/// Select characters `start..end` of `view`'s buffer.
fn select_chars(view: &gtk4::TextView, start: usize, end: usize) {
    let buffer = view.buffer();
    let at = |chars: usize| buffer.iter_at_offset(i32::try_from(chars).expect("fits"));
    buffer.select_range(&at(start), &at(end));
}

#[test]
fn test_copy_across_a_folded_paragraph_in_the_history_preview_copies_the_full_text() {
    ensure_gtk_init();
    let window = crate::common::test_window_with_restored_size(1400, 900);
    present_window(&window);
    let dir = tempfile::tempdir().expect("preview-copy tempdir");
    let path = dir.path().join("history-folded.txt");
    fixture::write_text(&path, "current\n");
    let (body, _) = giant_body(n(L_PROTECT_EDIT) + 5_000);
    lushtext_core::services::local_history_service::capture_snapshot_for_path(
        &lushtext_core::services::json_store::data_dir(),
        &path,
        &body,
        lushtext_core::model::local_history::LocalHistorySnapshotOrigin::Save,
        lushtext_core::services::local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed a folded snapshot");
    window.open_document(&path);
    wait_until(Duration::from_secs(10), || {
        active_editor(&window).file_size().is_some()
    });
    activate_action(&window, "show-local-history");
    wait_until(Duration::from_secs(10), || {
        window.visible_dialog().is_some()
    });
    let dialog = window.visible_dialog().expect("local-history dialog");
    let chars = body.chars().count();
    let view = {
        let mut found = None;
        wait_until(Duration::from_secs(60), || {
            found = text_views_under(dialog.upcast_ref())
                .into_iter()
                .find(|view| usize::try_from(view.buffer().char_count()).ok() == Some(chars));
            found.is_some()
        });
        found.expect("the installed preview")
    };
    assert!(buffer_is_folded(&view.buffer()), "the preview is folded");

    // From inside the head line, across the folded paragraph, into the tail
    // line: every hidden character is copied.
    let start = 2;
    let end = "short head\n".chars().count() + n(L_PROTECT_EDIT) + 5_000 + 1 + 4;
    select_chars(&view, start, end);
    let expected: String = body.chars().skip(start).take(end - start).collect();
    let copied = copy_selection_and_read(&view);
    assert_eq!(copied.len(), expected.len(), "every byte of the selection");
    assert!(copied == expected, "the copy is the selected source text");

    // The primary selection serves the same text.
    let primary = read_clipboard(&view.primary_clipboard());
    assert!(
        primary == expected,
        "the primary selection is the source text"
    );
}

/// A document note whose body holds many ordinary lines and one paragraph
/// over `L_PROTECT_EDIT`: the editor profiles it on a worker and installs it
/// in several turns.
#[test]
fn test_a_long_line_note_installs_folded_in_bounded_turns_and_copies_from_its_text() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("note tempdir");
    let path = dir.path().join("noted.txt");
    fixture::write_text(&path, "source\n");
    let giant = giant_paragraph(n(L_PROTECT_EDIT) + 10_000);
    let raw = format!(
        "{}{giant}\nlast line\n",
        "ordinary note line\n".repeat(40_000)
    );
    let note = lushtext_core::model::note::RichNoteBody::new(&raw);
    // The stored body is the normalized text the editor opens with.
    let note_text = note.text.clone();
    lushtext_core::services::document_note_service::save_for_path(
        &lushtext_core::services::json_store::data_dir(),
        &path,
        &note,
    )
    .expect("seed the note");
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    wait_until(Duration::from_secs(10), || {
        active_editor(&window).file_path().as_deref() == Some(path.as_path())
    });
    let slices_before = window.notes_evidence().note_body_install_slices;
    activate_action(&window, "open-document-note");
    wait_until(Duration::from_secs(10), || {
        window.visible_dialog().is_some()
    });
    let dialog = window
        .visible_dialog()
        .and_downcast::<libadwaita::AlertDialog>()
        .expect("note dialog");
    let extra = dialog.extra_child().expect("note editor");
    let chars = note_text.chars().count();
    let view = {
        let mut found = None;
        wait_until(Duration::from_secs(120), || {
            found = text_views_under(&extra)
                .into_iter()
                .find(|view| usize::try_from(view.buffer().char_count()).ok() == Some(chars));
            found.is_some()
        });
        found.expect("the installed note body")
    };
    let evidence = window.notes_evidence();
    assert!(
        evidence.note_body_install_slices > slices_before + 1,
        "the body went in over several turns"
    );
    eprintln!(
        "note-body-install-evidence slices={} largest_slice_us={}",
        evidence.note_body_install_slices - slices_before,
        evidence.largest_note_body_install_slice_us
    );
    assert!(buffer_is_folded(&view.buffer()), "the note is folded");
    assert!(!view.is_editable(), "a folded note is read-only");
    assert!(
        !dialog.is_response_enabled("save"),
        "an unchanged note has nothing to save"
    );

    let lines = "ordinary note line\n".chars().count() * 40_000;
    select_chars(&view, lines - 3, lines + n(L_PROTECT_EDIT) + 10_000 + 3);
    let expected: String = note_text
        .chars()
        .skip(lines - 3)
        .take(n(L_PROTECT_EDIT) + 10_000 + 6)
        .collect();
    assert!(
        copy_selection_and_read(&view) == expected,
        "a copy across the folded paragraph is its whole text"
    );
    dialog.close();
}

#[test]
fn test_copy_whole_line_says_which_line_it_copied() {
    let chars = n(L_PROTECT_EDIT) + 7;
    let (body, _) = giant_body(chars);
    let dir = tempfile::tempdir().expect("announce tempdir");
    let path = dir.path().join("announce.txt");
    fixture::write_text(&path, &body);
    let window = test_window();
    present_window(&window);
    window.open_document(&path);
    let page = active_editor(&window);
    wait_until(Duration::from_secs(60), || {
        page.load_evidence().load_state == EditorLoadState::Loaded
    });
    page.copy_protected_whole_line(1);
    wait_until(Duration::from_secs(30), || {
        page.protected_view_evidence().whole_line_copies == 1
    });
    assert_eq!(
        status_message_text(&window).as_deref(),
        Some(format!("Line 2 copied ({chars} characters)").as_str())
    );
}

#[test]
fn test_a_paste_longer_than_the_entry_cap_is_cut_with_a_notice() {
    use lushtext_core::ui::entry_paste_cap::{ENTRY_INSERT_CAP_BYTES, ENTRY_INSERT_CAPPED_NOTICE};
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("paste-cap tempdir");
    let path = dir.path().join("paste-cap.txt");
    fixture::write_text(&path, "text\n");
    window.open_document(&path);
    let page = active_editor(&window);
    wait_until(Duration::from_secs(30), || {
        page.load_evidence().load_state == EditorLoadState::Loaded
    });
    let entry = page.search_bar().search_entry().clone();
    // A two-byte character straddles the cap, so the cut moves back to its
    // start.
    let long = format!(
        "{}é{}",
        "x".repeat(ENTRY_INSERT_CAP_BYTES - 1),
        "y".repeat(4096)
    );
    let mut position = 0;
    entry.insert_text(&long, &mut position);
    flush_events();
    assert_eq!(entry.text().len(), ENTRY_INSERT_CAP_BYTES - 1);
    assert_eq!(
        status_message_text(&window).as_deref(),
        Some(ENTRY_INSERT_CAPPED_NOTICE)
    );
    // An ordinary insertion is untouched.
    entry.set_text("");
    let mut position = 0;
    entry.insert_text("needle", &mut position);
    assert_eq!(entry.text().as_str(), "needle");
}

#[test]
fn test_the_gutter_keeps_its_width_across_an_edit_entry_and_its_undo() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("gutter-width tempdir");
    let path = dir.path().join("gutter-width.txt");
    fixture::write_text(&path, "first line\nsecond line\n");
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    let view = page.source_view();
    let gutter = sourceview5::prelude::ViewExt::gutter(view, gtk4::TextWindowType::Left);
    crate::common::wait_for_painted_frame(view);
    let before = gutter.width();
    assert!(before > 0, "the gutter shows line numbers");
    let buffer = page.buffer();
    let mut end = buffer.iter_at_line(0).expect("line 0");
    end.forward_to_line_end();
    buffer.insert_interactive(&mut end, &"p".repeat(n(L_PROTECT_EDIT) + 1), true);
    assert!(page.is_protected_view());
    assert!(page.protected_view_evidence().fold_gutter_installed);
    crate::common::wait_for_painted_frame(view);
    assert_eq!(gutter.width(), before, "entering draws markers, no resize");
    assert!(page.undo_protected_entry());
    crate::common::wait_for_painted_frame(view);
    assert_eq!(gutter.width(), before, "leaving clears them, no resize");
    assert!(!page.protected_view_evidence().fold_gutter_installed);
}

#[test]
fn test_the_inspector_moves_between_folded_lines_and_states_what_is_hidden() {
    let first = giant_paragraph(n(L_PROTECT_EDIT) + 11);
    let second = giant_paragraph(n(L_PROTECT_EDIT) + 22);
    let body = format!("a\n{first}\nb\nc\n{second}\nd\n");
    let (page, _window) = protected_page(&body, "folded-nav");
    page.open_line_inspector(0, 0);
    wait_for_window(&page);
    let inspector = page.line_inspector().expect("open");
    inspector.activate(InspectorRequest::NextFoldedLine);
    wait_for_window(&page);
    assert_eq!(
        page.protected_view_evidence().inspector_window.map(|w| w.0),
        Some(1)
    );
    let summary = inspector.summary();
    assert!(summary.contains("hidden"), "{summary}");
    inspector.activate(InspectorRequest::NextFoldedLine);
    wait_for_window(&page);
    assert_eq!(
        page.protected_view_evidence().inspector_window.map(|w| w.0),
        Some(4)
    );
    inspector.activate(InspectorRequest::PreviousFoldedLine);
    wait_for_window(&page);
    assert_eq!(
        page.protected_view_evidence().inspector_window.map(|w| w.0),
        Some(1)
    );
}

/// The status bar's current message, from the window's notification bus.
fn status_message_text(window: &lushtext_core::ui::window::LushtextWindow) -> Option<String> {
    use glib::subclass::prelude::ObjectSubclassIsExt;
    window
        .imp()
        .notification_bus
        .status_bar_view()
        .map(|status| status.text)
}

/// Run `during` and then the main loop for `settle`, returning `during`'s own
/// microseconds and the longest gap between two ticks of a 1 ms timer over the
/// whole window (the longest main-loop dispatch, within a tick).
fn longest_dispatch_us(during: impl FnOnce(), settle: Duration) -> (u128, u128) {
    let last = std::rc::Rc::new(std::cell::Cell::new(std::time::Instant::now()));
    let worst = std::rc::Rc::new(std::cell::Cell::new(0u128));
    let (tick_last, tick_worst) = (std::rc::Rc::clone(&last), std::rc::Rc::clone(&worst));
    let source = glib::timeout_add_local(Duration::from_millis(1), move || {
        let now = std::time::Instant::now();
        tick_worst.set(
            tick_worst
                .get()
                .max(now.duration_since(tick_last.get()).as_micros()),
        );
        tick_last.set(now);
        glib::ControlFlow::Continue
    });
    flush_events();
    last.set(std::time::Instant::now());
    let started = std::time::Instant::now();
    during();
    let own = started.elapsed().as_micros();
    // Pump the main loop (the 1 ms timer wakes it) for the settle window.
    let context = glib::MainContext::default();
    let settle_started = std::time::Instant::now();
    while settle_started.elapsed() < settle {
        context.iteration(true);
    }
    source.remove();
    (own, worst.get())
}

/// Measurement (review S4), printed for a release run: an edit-growth entry
/// and its undo on a document of many long unfolded lines leave the gutter's
/// width alone, so nothing re-lays out the document; for comparison, a gutter
/// width change of the same document (what toggling the marker column did).
#[test]
fn test_entry_and_undo_leave_the_gutter_width_alone_evidence() {
    ensure_gtk_init();
    let line = "w".repeat(9) + " ";
    let long_line = line.repeat(4_000); // 40,000 characters, under L_PROTECT_EDIT
    let body = format!("{}\n", vec![long_line.as_str(); 30].join("\n"));
    let dir = tempfile::tempdir().expect("gutter-evidence tempdir");
    let path = dir.path().join("gutter-evidence.txt");
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    let view = page.source_view();
    crate::common::flush_after_delay(Duration::from_millis(500));
    let gutter = sourceview5::prelude::ViewExt::gutter(view, gtk4::TextWindowType::Left);
    let width = gutter.width();

    let buffer = page.buffer();
    let (entry_own, entry_worst) = longest_dispatch_us(
        || {
            let mut end = buffer.iter_at_line(0).expect("line 0");
            end.forward_to_line_end();
            buffer.insert_interactive(&mut end, &"p".repeat(n(L_PROTECT_EDIT)), true);
        },
        Duration::from_millis(1_500),
    );
    assert!(page.is_protected_view());
    assert_eq!(gutter.width(), width, "the entry did not resize the gutter");
    let (undo_own, undo_worst) = longest_dispatch_us(
        || assert!(page.undo_protected_entry()),
        Duration::from_millis(1_500),
    );
    assert!(!page.is_protected_view());
    assert_eq!(gutter.width(), width, "the undo did not resize the gutter");

    // What a marker column shown and hidden at entry and exit cost: the same
    // gutter made wider, then narrower again.
    let numbers = view.shows_line_numbers();
    let (resize_own, resize_worst) = longest_dispatch_us(
        || view.set_show_line_marks(!view.shows_line_marks()),
        Duration::from_millis(1_500),
    );
    let resized = gutter.width();
    view.set_show_line_marks(!view.shows_line_marks());
    crate::common::flush_after_delay(Duration::from_millis(1_500));
    eprintln!(
        "gutter-width-evidence entry_dispatch_us={entry_own} entry_worst_us={entry_worst} \
         undo_dispatch_us={undo_own} undo_worst_us={undo_worst} \
         resize_dispatch_us={resize_own} resize_worst_us={resize_worst} \
         width={width} resized_width={resized} line_numbers={numbers}"
    );
}

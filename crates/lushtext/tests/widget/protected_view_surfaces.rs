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
use lushtext_core::model::document_scale::L_PROTECT_EDIT;
use lushtext_core::model::protected_fold::N_INSPECT;
use lushtext_core::ui::editor_page::{
    EditorLoadState, InspectorRequest, LushtextEditorPage, WholeLineProvider,
};
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
    assert_eq!(provider.line_bytes(), giant.len());
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

#[test]
fn test_save_as_of_an_inexact_decode_is_refused_with_a_notice() {
    ensure_gtk_init();
    // A protected Shift_JIS document holding 0xED40, which decodes without
    // errors but does not encode back to the same bytes.
    let mut bytes = b"head\n".to_vec();
    bytes.extend(std::iter::repeat_n(b'a', n(L_PROTECT_EDIT) + 1));
    bytes.extend_from_slice(&[0xED, 0x40, b'\n']);
    let dir = tempfile::tempdir().expect("save-as tempdir");
    let path = dir.path().join("inexact.txt");
    fixture::write_bytes(&path, &bytes);
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

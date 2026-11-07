// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for the virtualized, budget-bounded notes browser
//! (`bound-notes-browser-by-budget`): every admitted row listed and reachable,
//! realized rows bounded, the selection kept across queries, the sliced
//! open-editor capture, and "Showing X of Y" with one announcement.

use crate::common::{
    NotesBrowserList, activate_action, ensure_gtk_init, find_descendant, find_notes_browser_list,
    fixture, flush_after_delay, flush_events, labels_in, mapped_list_rows, present_window,
    timing_verdicts_enforced, wait_until, worst_dispatch_until,
};
use gio::prelude::{ActionGroupExt, ListModelExt};
use glib::prelude::{Cast, ToVariant};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwApplicationWindowExt, AdwDialogExt};
use lushtext_core::model::bookmark::BookmarkRecord;
use lushtext_core::model::note::RichNoteBody;
use lushtext_core::model::responsiveness_budget::{B_INTERACT_US, B_SETTLE_US};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::{
    bookmark_service, document_note_service, folder_note_service, json_store, workspace_manager,
};
use lushtext_core::ui::editor_page::LushtextEditorPage;
use lushtext_core::ui::window::{
    LushtextWindow, NotesBrowserRuntimeSnapshot, set_notes_browser_budgets_for_test,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Restores the browser's production text budget when a test ends.
struct TextBudgetReset;

impl Drop for TextBudgetReset {
    fn drop(&mut self) {
        set_notes_browser_budgets_for_test(0, 0);
    }
}

/// One workspace folder, returned with the tempdir that owns it.
fn seed_workspace() -> (tempfile::TempDir, PathBuf) {
    ensure_gtk_init();
    let folders = tempfile::tempdir().expect("workspace tempdir");
    let folder = folders.path().join("w");
    fixture::create_dir_all(&folder);
    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::All,
        workspaces: vec![WorkspaceConfig::with_one_folder(
            WorkspaceId::new("ws"),
            "w",
            folder.clone(),
        )],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspace");
    (folders, folder)
}

/// `files` closed files with `per_file` unlabelled bookmarks each.
fn seed_closed_bookmarks(folder: &Path, files: usize, per_file: usize) {
    for file in 0..files {
        let path = folder.join(format!("c{file:03}.rs"));
        fixture::write_text(&path, "x\n");
        let bookmarks = (0..per_file)
            .map(|line| BookmarkRecord::new(u32::try_from(line).expect("line"), None))
            .collect::<Vec<_>>();
        bookmark_service::save_for_path(&json_store::data_dir(), &path, &bookmarks)
            .expect("save bookmarks");
    }
}

/// An open editor holding `count` bookmarks, labelled with `label_bytes`
/// bytes each when non-zero.
fn open_editor_with_bookmarks(
    window: &LushtextWindow,
    folder: &Path,
    name: &str,
    count: usize,
    label_bytes: usize,
) -> PathBuf {
    let path = folder.join(name);
    let content = (0..count)
        .map(|line| format!("l{line}\n"))
        .collect::<String>();
    fixture::write_text(&path, &content);
    let bookmarks = (0..count)
        .map(|line| {
            BookmarkRecord::new(
                u32::try_from(line).expect("line"),
                (label_bytes > 0).then(|| format!("{line:08}{}", "x".repeat(label_bytes))),
            )
        })
        .collect::<Vec<_>>();
    bookmark_service::save_for_path(&json_store::data_dir(), &path, &bookmarks)
        .expect("save editor bookmarks");
    window.open_document(&path);
    wait_until(Duration::from_secs(60), || {
        window.notes_evidence().active_document_bookmark_count == count
    });
    path
}

fn new_window() -> LushtextWindow {
    let window = crate::common::test_window();
    present_window(&window);
    wait_until(Duration::from_secs(10), || {
        !window.imp().sidebar.all_workspace_folder_paths().is_empty()
    });
    window
}

fn browser(window: &LushtextWindow) -> NotesBrowserRuntimeSnapshot {
    window.notes_evidence().browser.expect("notes browser open")
}

fn dialog(window: &LushtextWindow) -> libadwaita::Dialog {
    window
        .visible_dialog()
        .and_then(|dialog| dialog.downcast::<libadwaita::Dialog>().ok())
        .expect("notes browser dialog")
}

fn notes_list(window: &LushtextWindow) -> NotesBrowserList {
    find_notes_browser_list(&dialog(window).child().expect("dialog child")).expect("notes list")
}

/// Open Browse Notes and wait until its first query has published `rows`.
fn open_browser_and_wait(window: &LushtextWindow, rows: u32) {
    activate_action(window, "show-notes");
    wait_until(Duration::from_secs(120), || {
        window.notes_evidence().browser.is_some_and(|b| {
            b.source_ready && b.query.active == 0 && b.query.pending == 0 && b.listed_rows == rows
        })
    });
}

/// Labels of the list rows GTK draws inside the scroller's viewport (A10).
fn drawn_row_titles(list: &NotesBrowserList) -> Vec<String> {
    let scroller = list
        .list_view
        .parent()
        .and_then(|parent| parent.downcast::<gtk4::ScrolledWindow>().ok())
        .expect("the list's own scroller");
    let viewport = f64::from(scroller.height());
    mapped_list_rows(&list.list_view)
        .filter(|row| row.css_name() == "row")
        .filter(|row| {
            row.compute_bounds(&scroller).is_some_and(|bounds| {
                f64::from(bounds.y() + bounds.height()) > 0.0 && f64::from(bounds.y()) < viewport
            })
        })
        .filter_map(|row| labels_in(&row).into_iter().next())
        .collect()
}

/// D1/D2: 20,000 closed-file bookmarks plus 20,000 open-editor bookmarks are
/// all listed, the last is reached and drawn, nothing says "refine", and the
/// realized rows stay within GTK's cap (A1).
#[test]
fn test_every_note_and_bookmark_is_listed_and_the_last_is_drawn() {
    let (_folders, folder) = seed_workspace();
    seed_closed_bookmarks(&folder, 20, 1_000);
    let window = new_window();
    open_editor_with_bookmarks(&window, &folder, "zz-open.rs", 20_000, 0);
    open_browser_and_wait(&window, 40_000);

    let snapshot = browser(&window);
    assert_eq!(snapshot.source_rows, 40_000);
    assert_eq!(snapshot.rows_found, 40_000);
    assert!(!snapshot.limited);
    let list = notes_list(&window);
    assert_eq!(list.items().n_items(), 40_000);
    assert!(
        !labels_in(&dialog(&window).child().expect("child"))
            .iter()
            .any(|label| label.contains("Refine") || label.contains("Showing")),
        "no refine or limit notice"
    );

    // The last row: reach it the way End does, by scrolling to it with focus.
    list.list_view.scroll_to(
        39_999,
        gtk4::ListScrollFlags::FOCUS | gtk4::ListScrollFlags::SELECT,
        None,
    );
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(list.selected(), Some(39_999));
    let selected = browser(&window).selected_source_index.expect("selected");
    assert!(selected < 40_000);
    let drawn = drawn_row_titles(&list);
    assert!(!drawn.is_empty(), "rows are drawn at the end of the list");
    let realized = browser(&window).realized_row_widgets;
    assert!(
        realized <= 256,
        "realized row widgets stay within GTK's cap, got {realized}"
    );
    dialog(&window).close();
    flush_events();
}

/// D2: realized row widgets at 131,072 rows stay within the A1 cap, and the
/// byte budgets, not a count, bound the source.
#[test]
fn test_the_list_stays_virtualized_at_131072_rows() {
    let (_folders, folder) = seed_workspace();
    seed_closed_bookmarks(&folder, 128, 1_024);
    let window = new_window();
    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(300), || {
        window.notes_evidence().browser.is_some_and(|b| {
            b.source_ready && b.query.active == 0 && b.query.pending == 0 && b.listed_rows > 0
        })
    });
    flush_after_delay(Duration::from_millis(300));
    let snapshot = browser(&window);
    assert_eq!(u64::from(snapshot.listed_rows), snapshot.source_rows as u64);
    // The retained-graph budget, not a count, bounds the source: every row is
    // found, the ones that fit are listed, and the cut is stated.
    assert!(
        snapshot.source_rows >= 64 * 1024,
        "the byte budgets retain most of the source, got {}",
        snapshot.source_rows
    );
    assert_eq!(snapshot.rows_found, 128 * 1024);
    assert_eq!(snapshot.limited, snapshot.source_rows < snapshot.rows_found);
    assert!(
        snapshot.realized_row_widgets <= 256,
        "realized rows {} within GTK's cap",
        snapshot.realized_row_widgets
    );
    dialog(&window).close();
    flush_events();
}

/// D6/task 8.2: the browser's dispatches at the 16,384-row reference case,
/// against twice their measured worst on the optimized widget build
/// (`bound-notes-browser-by-budget` "Implementation decisions"; the BL task
/// 8.2 pattern). The query publish and the keystroke fit their budgets; the
/// first frame, the relayout frame after a query that changes the shown rows,
/// and the close are recorded toolkit-floor exceptions (about 205 realized
/// rows laid out, A1, A90), bounded here so they cannot grow back with rows.
#[test]
fn test_browser_dispatches_stay_within_their_measured_bounds() {
    const MS: u128 = 1_000;
    let (_folders, folder) = seed_workspace();
    seed_closed_bookmarks(&folder, 16, 1_024);
    let window = new_window();
    flush_after_delay(Duration::from_millis(300));

    let started = Instant::now();
    gio::prelude::ActionGroupExt::activate_action(&window, "show-notes", None);
    let open_action = started.elapsed();
    let (held, until_rows) = worst_dispatch_until(Duration::from_secs(120), || {
        window.notes_evidence().browser.is_some_and(|b| {
            b.source_ready
                && b.query.active == 0
                && b.query.pending == 0
                && b.listed_rows == 16 * 1024
        })
    });
    assert!(held, "the browser lists every row");
    let (_, first_frames) = worst_dispatch_until(Duration::from_millis(700), || false);

    let child = dialog(&window).child().expect("child");
    let entry = find_descendant(&child, glib::object::ObjectExt::is::<gtk4::SearchEntry>)
        .and_then(|widget| widget.downcast::<gtk4::SearchEntry>().ok())
        .expect("search entry");
    let queries = browser(&window).query.started;
    let typed = Instant::now();
    entry.set_text("c000.rs");
    let keystroke = typed.elapsed();
    let (held, query_publish) = worst_dispatch_until(Duration::from_secs(60), || {
        window.notes_evidence().browser.is_some_and(|b| {
            b.query.started > queries && b.query.active == 0 && b.listed_rows == 1_024
        })
    });
    assert!(held, "the query publishes");
    let (_, query_frames) = worst_dispatch_until(Duration::from_millis(500), || false);

    dialog(&window).close();
    let (_, close) = worst_dispatch_until(Duration::from_millis(1_500), || false);
    println!(
        "list-budget-evidence {{\"case\":\"notes16384\",\"open_action_us\":{},\"until_rows_us\":{},\"first_frames_us\":{},\"keystroke_us\":{},\"query_publish_us\":{},\"query_frames_us\":{},\"close_us\":{}}}",
        open_action.as_micros(),
        until_rows.as_micros(),
        first_frames.as_micros(),
        keystroke.as_micros(),
        query_publish.as_micros(),
        query_frames.as_micros(),
        close.as_micros()
    );
    if timing_verdicts_enforced() {
        // Measured worst (optimized, interleaved, load < 2, 2026-11-07):
        // open 11.7, until rows 19.9, first frames 64.9, keystroke 0.4,
        // query publish 2.8, query frames 22.9, close 34.4 ms.
        assert!(
            open_action.as_micros() < u128::from(B_SETTLE_US),
            "{open_action:?}"
        );
        assert!(until_rows.as_micros() < 40 * MS, "{until_rows:?}");
        assert!(first_frames.as_micros() < 130 * MS, "{first_frames:?}");
        assert!(
            keystroke.as_micros() < u128::from(B_INTERACT_US),
            "{keystroke:?}"
        );
        assert!(query_publish.as_micros() < 6 * MS, "{query_publish:?}");
        assert!(query_frames.as_micros() < 46 * MS, "{query_frames:?}");
        assert!(close.as_micros() < 70 * MS, "{close:?}");
    }
}

/// D2: a query that still lists the selected row keeps it selected and does
/// not reload its preview.
#[test]
fn test_the_selection_survives_a_query_without_reloading_the_preview() {
    let (_folders, folder) = seed_workspace();
    seed_closed_bookmarks(&folder, 3, 4);
    let window = new_window();
    open_browser_and_wait(&window, 12);
    let list = notes_list(&window);
    list.set_selected(5);
    wait_until(Duration::from_secs(10), || {
        browser(&window).preview.active == 0 && browser(&window).preview.started > 0
    });
    let selected = browser(&window).selected_source_index.expect("selected");
    let previews_started = browser(&window).preview.started;
    // Narrow to the selected row's own file, which still lists it.
    let subtitle = list
        .item(5)
        .and_then(|item| item.subtitle())
        .expect("subtitle");
    let file = ["c000", "c001", "c002"]
        .into_iter()
        .find(|name| subtitle.contains(name))
        .expect("the row names its file");
    let started = browser(&window).query.started;
    ActionGroupExt::activate_action(&window, "set-notes-browser-query", Some(&file.to_variant()));
    wait_until(Duration::from_secs(10), || {
        let b = browser(&window);
        b.query.started > started && b.query.active == 0 && b.listed_rows == 4
    });
    let after = browser(&window);
    assert_eq!(after.selected_source_index, Some(selected));
    assert_eq!(
        after.preview.started, previews_started,
        "the kept selection's preview is not loaded again"
    );
    dialog(&window).close();
    flush_events();
}

/// D1: a section header is not a row, so the flat index of
/// `win.select-notes-browser-row` and keyboard moves across a section
/// boundary land on items; Enter previews without opening.
#[test]
fn test_rows_cross_section_boundaries_without_headers() {
    let (_folders, folder) = seed_workspace();
    seed_closed_bookmarks(&folder, 1, 2);
    folder_note_service::save_for_folder(
        &json_store::data_dir(),
        &folder,
        &RichNoteBody::new("folder body"),
    )
    .expect("save folder note");
    let doc = folder.join("doc.md");
    fixture::write_text(&doc, "doc\n");
    document_note_service::save_for_path(&json_store::data_dir(), &doc, &RichNoteBody::new("doc"))
        .expect("save document note");
    let window = new_window();
    open_browser_and_wait(&window, 4);
    let list = notes_list(&window);
    assert_eq!(
        list.section_titles(),
        ["Bookmarks", "Folder Notes", "Document Notes"]
    );

    // Flat automation index 2 is the folder note: headers are not counted.
    ActionGroupExt::activate_action(
        &window,
        "select-notes-browser-row",
        Some(&2u32.to_variant()),
    );
    flush_events();
    assert_eq!(list.selected(), Some(2));
    let child = dialog(&window).child().expect("child");
    wait_until(Duration::from_secs(5), || {
        labels_in(&child)
            .iter()
            .any(|label| label.starts_with("Folder Note"))
    });

    // Headers take no keyboard focus (A94).
    let mut child_widget = list.list_view.first_child();
    while let Some(widget) = child_widget {
        if widget.css_name() == "header" {
            assert!(
                !widget.is_focusable() && !widget.can_focus()
                    || find_descendant(&widget, WidgetExt::is_focusable).is_none(),
                "a section header is not a focus stop"
            );
        }
        child_widget = widget.next_sibling();
    }

    // Enter on the last bookmark previews it and opens nothing.
    list.activate(1);
    flush_events();
    assert_eq!(list.selected(), Some(1));
    assert!(window.visible_dialog().is_some(), "Enter does not open");
    dialog(&window).close();
    flush_events();
}

/// D5: a text-budget cut shows "Showing X of Y", lists the rows after the
/// refused one, and announces once across later queries.
#[test]
fn test_a_text_cut_says_showing_x_of_y_and_announces_once() {
    let _reset = TextBudgetReset;
    let (_folders, folder) = seed_workspace();
    let path = folder.join("big.rs");
    fixture::write_text(&path, "a\nb\n");
    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &[
            BookmarkRecord::new(0, Some("a".repeat(4_096))),
            BookmarkRecord::new(1, Some("b".repeat(4_096))),
        ],
    )
    .expect("save bookmarks");
    folder_note_service::save_for_folder(
        &json_store::data_dir(),
        &folder,
        &RichNoteBody::new("folder"),
    )
    .expect("save folder note");
    set_notes_browser_budgets_for_test(6_000, 0);
    let window = new_window();
    open_browser_and_wait(&window, 2);

    let snapshot = browser(&window);
    assert_eq!(snapshot.source_rows, 2, "one bookmark and the folder note");
    assert_eq!(snapshot.rows_found, 3);
    assert!(snapshot.limited);
    assert_eq!(snapshot.limit_announcements, 1);
    let child = dialog(&window).child().expect("child");
    assert!(
        labels_in(&child).iter().any(|label| label
            == "Showing 2 of 3 notes and bookmarks. The rest exceed the memory budget."),
        "the limit label states X of Y"
    );
    assert_eq!(
        notes_list(&window).section_titles(),
        ["Bookmarks", "Folder Notes"]
    );
    // The limit label is a status whose accessible name is its visible text.
    let limit_label = find_descendant(&child, |widget| {
        widget
            .downcast_ref::<gtk4::Label>()
            .is_some_and(|label| label.text().starts_with("Showing "))
    })
    .expect("limit label");
    lushtext_core::ui::accessibility::test_audit::AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&limit_label);
    // The automation snapshot projects the same facts (D9).
    let app = window
        .application()
        .and_then(|app| {
            app.downcast::<lushtext_core::app::LushtextApplication>()
                .ok()
        })
        .expect("lushtext application");
    let notes = lushtext_core::ui::automation::app_snapshot(&app)
        .window
        .expect("window snapshot")
        .notes;
    assert!(notes.browser_open);
    assert_eq!(notes.browser_row_count, 2);
    assert_eq!(notes.browser_rows_found, 3);
    assert!(notes.browser_limited);

    for query in ["folder", "a", ""] {
        let started = browser(&window).query.started;
        ActionGroupExt::activate_action(
            &window,
            "set-notes-browser-query",
            Some(&query.to_variant()),
        );
        wait_until(Duration::from_secs(10), || {
            let b = browser(&window);
            b.query.started > started && b.query.active == 0 && b.query.pending == 0
        });
    }
    assert_eq!(browser(&window).limit_announcements, 1, "announced once");
    dialog(&window).close();
    flush_events();
}

/// D4: 20,000 open-editor bookmarks are captured across several turns, each
/// within B_SETTLE, with no count.
#[test]
fn test_a_20000_bookmark_editor_is_captured_in_bounded_turns() {
    let _reset = TextBudgetReset;
    let (_folders, folder) = seed_workspace();
    let window = new_window();
    open_editor_with_bookmarks(&window, &folder, "open.rs", 20_000, 0);
    // A turn ends at the first unit started at or after the slice; on a fast
    // host 20,000 bookmarks fit one slice, so the test caps units per turn to
    // see the turns, and the clock-driven bound is checked by the timing below.
    set_notes_browser_budgets_for_test(0, 8);
    open_browser_and_wait(&window, 20_000);
    let snapshot = browser(&window);
    assert!(
        snapshot.snapshot.turns > 1,
        "the capture took {} turns",
        snapshot.snapshot.turns
    );
    assert!(!snapshot.limited);
    println!(
        "list-budget-evidence {{\"case\":\"capture20000\",\"turns\":{},\"worst_turn_us\":{}}}",
        snapshot.snapshot.turns, snapshot.snapshot.worst_turn_us
    );
    if timing_verdicts_enforced() {
        assert!(snapshot.snapshot.worst_turn_us < B_SETTLE_US);
    }
    dialog(&window).close();
    flush_events();
}

/// D4: a bookmark added between capture slices restarts that editor, and the
/// listed set is consistent with the final bookmarks.
#[test]
fn test_a_bookmark_change_mid_capture_restarts_the_editor() {
    let _reset = TextBudgetReset;
    // Several units per turn at most, so the capture is still running when
    // the test acts between turns, however fast the host is.
    set_notes_browser_budgets_for_test(0, 8);
    let (_folders, folder) = seed_workspace();
    let window = new_window();
    open_editor_with_bookmarks(&window, &folder, "open.rs", 20_000, 0);
    let editor = window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
        .expect("active editor");
    gio::prelude::ActionGroupExt::activate_action(&window, "show-notes", None);
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline
        && !window
            .notes_evidence()
            .browser
            .is_some_and(|b| b.snapshot.turns >= 1 && b.snapshot_capturing)
    {
        context.iteration(false);
    }
    assert!(
        browser(&window).snapshot_capturing,
        "caught the capture mid-way"
    );
    let mut bookmarks = editor.bookmark_records();
    bookmarks.push(BookmarkRecord::new(20_000, Some("added".to_string())));
    editor.load_bookmarks(&bookmarks);
    wait_until(Duration::from_secs(60), || {
        window.notes_evidence().browser.is_some_and(|b| {
            b.source_ready && b.query.active == 0 && b.query.pending == 0 && b.listed_rows > 0
        })
    });
    let snapshot = browser(&window);
    assert!(snapshot.snapshot.restarts >= 1, "the restart is counted");
    assert_eq!(snapshot.listed_rows, 20_001, "the capture is consistent");
    dialog(&window).close();
    flush_events();
}

/// D4/D5: a snapshot byte cut counts the rows it did not copy into Y.
#[test]
fn test_a_snapshot_byte_cut_counts_the_rows_it_did_not_copy() {
    let (_folders, folder) = seed_workspace();
    let window = new_window();
    // 3,000 bookmarks with 2 KiB labels exceed the 4 MiB capture budget.
    open_editor_with_bookmarks(&window, &folder, "labelled.rs", 3_000, 2_048);
    activate_action(&window, "show-notes");
    wait_until(Duration::from_secs(60), || {
        window.notes_evidence().browser.is_some_and(|b| {
            b.source_ready && b.query.active == 0 && b.query.pending == 0 && b.listed_rows > 0
        })
    });
    let snapshot = browser(&window);
    assert!(snapshot.limited);
    assert!(snapshot.source_rows < 3_000);
    assert_eq!(snapshot.rows_found, 3_000);
    assert_eq!(
        snapshot.snapshot.rows_counted_not_retained,
        u64::try_from(3_000 - snapshot.source_rows).expect("small")
    );
    dialog(&window).close();
    flush_events();
}

/// D4: closing the dialog mid-capture removes the turn source; nothing
/// publishes afterwards.
#[test]
fn test_closing_mid_capture_removes_the_turn_source() {
    let _reset = TextBudgetReset;
    // Several units per turn at most, so the capture is still running when
    // the test acts between turns, however fast the host is.
    set_notes_browser_budgets_for_test(0, 8);
    let (_folders, folder) = seed_workspace();
    let window = new_window();
    open_editor_with_bookmarks(&window, &folder, "open.rs", 20_000, 0);
    gio::prelude::ActionGroupExt::activate_action(&window, "show-notes", None);
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline
        && !window
            .notes_evidence()
            .browser
            .is_some_and(|b| b.snapshot.turns >= 1 && b.snapshot_capturing)
    {
        context.iteration(false);
    }
    assert!(browser(&window).snapshot_capturing);
    dialog(&window).close();
    flush_after_delay(Duration::from_millis(500));
    assert!(window.notes_evidence().browser.is_none());
}

/// D2: republishing over realized, selected rows, as queries do, keeps the
/// model consistent with each emission. A model already holding the new rows
/// while its removal was emitted crashed GTK's list item manager
/// (`gtk_rb_tree_node_get_previous` from `gtk_list_item_manager_ensure_items`).
#[test]
fn test_repeated_queries_over_realized_rows_republish_safely() {
    let (_folders, folder) = seed_workspace();
    seed_closed_bookmarks(&folder, 2, 512);
    let window = new_window();
    open_browser_and_wait(&window, 1_024);
    let list = notes_list(&window);
    list.list_view
        .scroll_to(600, gtk4::ListScrollFlags::SELECT, None);
    flush_after_delay(Duration::from_millis(200));
    for (query, rows) in [
        ("c000", 512),
        ("", 1_024),
        ("c001", 512),
        ("zzz-none", 0),
        ("", 1_024),
    ] {
        let started = browser(&window).query.started;
        ActionGroupExt::activate_action(
            &window,
            "set-notes-browser-query",
            Some(&query.to_variant()),
        );
        wait_until(Duration::from_secs(10), || {
            let b = browser(&window);
            b.query.started > started && b.query.active == 0 && b.listed_rows == rows
        });
        flush_after_delay(Duration::from_millis(100));
    }
    assert_eq!(list.items().n_items(), 1_024);
    dialog(&window).close();
    flush_events();
}

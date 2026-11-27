// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the workspace search panel and its components.

use crate::common::{
    accessible_hidden, activate_action, ensure_gtk_init, fixture, flush_after_delay, flush_events,
    isolated_data_dir, present_window, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use lushtext_model::workspace::content_search::{
    BoundedDiagnosticSample, ContentSearchOptions, ReplaceResult, Replacement, SavedSearch,
    SearchEvent, SearchHistoryEntry, SearchMatch, SearchMatchId, SearchQuerySpec,
};
use lushtext_services::coordination::notifications::{
    NotificationBus, NotificationOwner, NotificationPayload, NotificationSeverity,
    NotificationSurface, StatusMessage,
};
use lushtext_services::search::content_search::{ReplaceUndoBackup, generate_replacement_preview};
use lushtext_services::{persistence::json_store, search::search_backup};
use lushtext_ui::ui::accessibility::{self, test_audit::AccessibleAudit};
use lushtext_ui::ui::editor_page::LushtextEditorPage;
use lushtext_ui::ui::plain_disposal::hold_disposal_capacity_for_test;
use lushtext_ui::ui::search_panel::item::SearchResultItem;
use lushtext_ui::ui::search_panel::{
    LushtextSearchPanel, SearchFileGroup, SearchMatchLocation, SearchPanelTestPolicy,
    SearchProgressUpdate, apply_search_result_row_accessibility_for_test,
};
use lushtext_ui::ui::status_bar::LushtextStatusBar;
use lushtext_ui::ui::window::LushtextWindow;
use std::assert_matches;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn make_history_entry(
    query: &str,
    case_sensitive: bool,
    regex: bool,
    whole_word: bool,
    gitignore: bool,
    glob: Option<&str>,
) -> SearchHistoryEntry {
    SearchHistoryEntry::from_spec(SearchQuerySpec::new(
        query.to_string(),
        ContentSearchOptions::new(
            lushtext_model::workspace::content_search::ContentSearchFlags {
                case_sensitive,
                regex,
                whole_word,
                gitignore,
            },
            glob.map(ToString::to_string),
        ),
    ))
}

fn make_saved_search(name: &str, query: &str) -> SavedSearch {
    SavedSearch::from_spec(
        name.to_string(),
        SearchQuerySpec::new(query.to_string(), ContentSearchOptions::default()),
    )
}

fn search_spec(query: &str) -> SearchQuerySpec {
    SearchQuerySpec::new(query.to_string(), ContentSearchOptions::default())
}

/// A one-file streamed undo journal, written to the isolated data directory
/// as a Replace All run of this launch writes it, and its descriptors.
fn sample_replace_backup(path: &str) -> ReplaceUndoBackup {
    search_backup::fixture::write_stream_journal(
        &json_store::data_dir(),
        &[(
            std::path::Path::new(path),
            b"original content",
            b"replaced content",
        )],
    )
    .expect("write sample undo journal")
}

/// The descriptors of [`sample_replace_backup`], without writing a journal:
/// a run's result before that run committed it.
fn sample_replace_descriptors(path: &str) -> ReplaceUndoBackup {
    search_backup::fixture::stream_descriptors(
        &json_store::data_dir(),
        &[(
            std::path::Path::new(path),
            b"original content",
            b"replaced content",
        )],
    )
}

/// Wait until the panel's undo-journal disk work has drained.
///
/// "Has the journal's disk save or delete landed" is a question about the
/// workflow, and the workflow's evidence surface answers it directly. Polling
/// the journal directory instead answers a weaker question — it can observe a
/// partially written directory, and it cannot distinguish "not written yet" from
/// "written differently". Tests wait here and then assert the on-disk bytes,
/// which keeps the disk assertion an assertion instead of a timeout.
fn wait_for_journal_disk_idle(panel: &LushtextSearchPanel) {
    wait_until(Duration::from_secs(10), || {
        panel.evidence().journal_disk_jobs_in_flight == 0
    });
}

fn panel_with_one_search_match() -> LushtextSearchPanel {
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("hello");
    flush_after_delay(Duration::from_millis(200));
    let _ = panel.imp().runtime.search_debounce.invalidate();
    let search_match = SearchMatch::new(
        std::path::PathBuf::from("/test.rs"),
        1,
        "let hello = 1;",
        4..9,
    );
    let file_item = SearchResultItem::new_file("/test.rs", "test.rs", 1);
    let match_item = SearchResultItem::new_match(
        "/test.rs",
        1,
        "let hello = 1;",
        4,
        9,
        SearchMatchId::from_index(0),
    );
    let child_store = gtk4::gio::ListStore::new::<SearchResultItem>();
    child_store.append(&match_item);
    panel.imp().runtime.root_store.borrow().append(&file_item);
    panel.imp().runtime.file_groups.borrow_mut().insert(
        std::path::PathBuf::from("/test.rs").into(),
        SearchFileGroup::new(file_item, child_store),
    );
    panel
        .imp()
        .runtime
        .accepted_matches
        .replace(Some(Arc::new(vec![search_match])));
    panel.imp().runtime.total_matches.set(1);
    panel
}

fn populate_large_result_generation(panel: &LushtextSearchPanel, path: &str, match_count: usize) {
    let path = PathBuf::from(path);
    let display_path = path.display().to_string();
    let header = SearchResultItem::new_file(
        &display_path,
        &display_path,
        u32::try_from(match_count).unwrap_or(u32::MAX),
    );
    let child_store = gtk4::gio::ListStore::new::<SearchResultItem>();
    let mut accepted = Vec::with_capacity(match_count);
    for index in 0..match_count {
        let match_id = SearchMatchId::from_index(index);
        child_store.append(&SearchResultItem::new_match(
            &display_path,
            u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX),
            "needle",
            0,
            6,
            match_id,
        ));
        accepted.push(
            SearchMatch::new(
                path.clone(),
                u64::try_from(index.saturating_add(1)).unwrap_or(u64::MAX),
                "needle",
                0..6,
            )
            .with_id(match_id),
        );
    }
    panel.imp().runtime.root_store.borrow().append(&header);
    panel
        .imp()
        .runtime
        .file_groups
        .borrow_mut()
        .insert(path.into(), SearchFileGroup::new(header, child_store));
    panel
        .imp()
        .runtime
        .accepted_matches
        .replace(Some(Arc::new(accepted)));
    panel
        .imp()
        .navigation
        .match_rows
        .borrow_mut()
        .resize(match_count, None);
    panel
        .imp()
        .runtime
        .total_matches
        .set(u32::try_from(match_count).unwrap_or(u32::MAX));
    panel.imp().runtime.total_files.set(1);
}

#[derive(Clone, Copy, Debug)]
enum RetirementFixtureCategory {
    RootRows,
    FileGroups,
    ChildRows,
    CachedMatchRows,
    CachedFileRows,
    StreamedMatches,
    AcceptedMatches,
    Positions,
}

fn retirement_fixture_match(path: &str, index: usize) -> SearchMatch {
    SearchMatch::new(
        PathBuf::from(path),
        u64::try_from(index.saturating_add(1)).unwrap_or(u64::MAX),
        "needle",
        0..6,
    )
}

fn retirement_fixture_tree_rows(count: usize) -> Vec<gtk4::TreeListRow> {
    let store = gtk4::gio::ListStore::new::<SearchResultItem>();
    for index in 0..count {
        let path = format!("/cached-row-{index}.txt");
        store.append(&SearchResultItem::new_file(&path, &path, 0));
    }
    let model = gtk4::TreeListModel::new(store, false, false, |_| None::<gtk4::gio::ListModel>);
    (0..count)
        .map(|index| {
            model
                .item(u32::try_from(index).unwrap_or(u32::MAX))
                .and_downcast::<gtk4::TreeListRow>()
                .expect("fixture tree row")
        })
        .collect()
}

fn populate_retirement_category(
    panel: &LushtextSearchPanel,
    category: RetirementFixtureCategory,
    count: usize,
) {
    match category {
        RetirementFixtureCategory::RootRows => {
            let store = panel.imp().runtime.root_store.borrow();
            for index in 0..count {
                let path = format!("/retired-root-{index}.txt");
                store.append(&SearchResultItem::new_file(&path, &path, 0));
            }
        }
        RetirementFixtureCategory::FileGroups => {
            let mut groups = panel.imp().runtime.file_groups.borrow_mut();
            for index in 0..count {
                let path = PathBuf::from(format!("/retired-group-{index}.txt"));
                let display = path.display().to_string();
                let header = SearchResultItem::new_file(&display, &display, 0);
                groups.insert(
                    path.into(),
                    SearchFileGroup::new(header, gtk4::gio::ListStore::new::<SearchResultItem>()),
                );
            }
        }
        RetirementFixtureCategory::ChildRows => {
            let path = PathBuf::from("/retired-child-group.txt");
            let header = SearchResultItem::new_file("/retired-child-group.txt", "child", 0);
            let children = gtk4::gio::ListStore::new::<SearchResultItem>();
            for index in 0..count {
                children.append(&SearchResultItem::new_match(
                    "/retired-child-group.txt",
                    u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX),
                    "needle",
                    0,
                    6,
                    SearchMatchId::from_index(index),
                ));
            }
            panel
                .imp()
                .runtime
                .file_groups
                .borrow_mut()
                .insert(path.into(), SearchFileGroup::new(header, children));
        }
        RetirementFixtureCategory::CachedMatchRows => panel
            .imp()
            .navigation
            .match_rows
            .borrow_mut()
            .extend(retirement_fixture_tree_rows(count).into_iter().map(Some)),
        RetirementFixtureCategory::CachedFileRows => {
            let mut file_rows = panel.imp().navigation.file_rows.borrow_mut();
            for (index, row) in retirement_fixture_tree_rows(count).into_iter().enumerate() {
                file_rows.insert(
                    PathBuf::from(format!("/cached-file-{index}.txt")).into(),
                    row,
                );
            }
        }
        RetirementFixtureCategory::StreamedMatches => {
            panel.imp().runtime.search_matches.borrow_mut().extend(
                (0..count).map(|index| retirement_fixture_match("/retired-streamed.txt", index)),
            );
        }
        RetirementFixtureCategory::AcceptedMatches => {
            let matches = (0..count)
                .map(|index| retirement_fixture_match("/retired-accepted.txt", index))
                .collect();
            panel
                .imp()
                .runtime
                .accepted_matches
                .replace(Some(Arc::new(matches)));
        }
        RetirementFixtureCategory::Positions => panel
            .imp()
            .navigation
            .match_positions
            .borrow_mut()
            .extend((0..count).map(|index| {
                SearchMatchLocation::new(
                    PathBuf::from("/retired-position.txt"),
                    u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX),
                )
            })),
    }
}

fn released_for_category(
    released: lushtext_ui::ui::search_panel::SearchRetirementOwnership,
    category: RetirementFixtureCategory,
) -> usize {
    match category {
        RetirementFixtureCategory::RootRows => released.root_rows,
        RetirementFixtureCategory::FileGroups => released.file_groups,
        RetirementFixtureCategory::ChildRows => released.child_rows,
        RetirementFixtureCategory::CachedMatchRows => released.cached_match_rows,
        RetirementFixtureCategory::CachedFileRows => released.cached_file_rows,
        RetirementFixtureCategory::StreamedMatches => released.streamed_matches,
        RetirementFixtureCategory::AcceptedMatches => released.accepted_matches,
        RetirementFixtureCategory::Positions => released.positions,
    }
}

struct SearchPanelDelayReset;

impl Drop for SearchPanelDelayReset {
    fn drop(&mut self) {
        SearchPanelTestPolicy::reset();
    }
}

/// Create a window attached to a test application (not registered with D-Bus).
fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

// ---------------------------------------------------------------------------
// SearchResultItem GObject adapter
// ---------------------------------------------------------------------------

#[test]
fn test_search_result_item_new_file() {
    ensure_gtk_init();
    let item = SearchResultItem::new_file("/home/user/project/src/main.rs", "src/main.rs", 5);
    assert!(item.is_file_item());
    assert!(!item.is_match_item());
    assert_eq!(item.file_path(), "/home/user/project/src/main.rs");
    assert_eq!(item.display_path(), "src/main.rs");
    assert_eq!(item.match_count(), 5);
    assert_eq!(item.line_number(), 0);
    assert!(item.line_content().is_empty());
}

#[test]
fn test_search_result_item_new_match() {
    ensure_gtk_init();
    let item = SearchResultItem::new_match(
        "/home/user/project/src/main.rs",
        42,
        "fn main() {",
        0,
        2,
        SearchMatchId::from_index(0),
    );
    assert!(!item.is_file_item());
    assert!(item.is_match_item());
    assert_eq!(item.file_path(), "/home/user/project/src/main.rs");
    assert_eq!(item.line_number(), 42);
    assert_eq!(item.line_content(), "fn main() {");
    assert_eq!(item.match_count(), 0);
    assert!(item.display_path().is_empty());
    assert_eq!(item.match_start(), 0);
    assert_eq!(item.match_end(), 2);
    assert_eq!(item.match_id(), SearchMatchId::from_index(0));
}

#[test]
fn test_search_result_item_match_count_is_mutable() {
    ensure_gtk_init();
    let item = SearchResultItem::new_file("/path/to/file.rs", "file.rs", 0);
    assert_eq!(item.match_count(), 0);
    item.set_match_count(10);
    assert_eq!(item.match_count(), 10);
    item.set_match_count(25);
    assert_eq!(item.match_count(), 25);
}

// ---------------------------------------------------------------------------
// LushtextSearchPanel widget
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_construction() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Verify template children are accessible.
    assert!(panel.imp().search_entry.text().is_empty());
    assert!(panel.imp().count_label.text().is_empty());
    assert!(!panel.imp().error_label.is_visible());
}

#[test]
fn test_search_panel_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let imp = panel.imp();

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::SearchBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::HasPopup,
        ])
        .assert_on(&*imp.search_entry);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::List)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .states(&[gtk4::AccessibleState::Hidden])
        .assert_on(&*imp.results_list);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*imp.count_label);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Alert)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*imp.error_label);

    for toggle in [
        &*imp.case_toggle,
        &*imp.regex_toggle,
        &*imp.word_toggle,
        &*imp.more_toggle,
        &*imp.gitignore_toggle,
    ] {
        AccessibleAudit::new()
            .role(gtk4::AccessibleRole::ToggleButton)
            .properties(&[gtk4::AccessibleProperty::Label])
            .states(&[gtk4::AccessibleState::Pressed])
            .assert_on(toggle);
    }

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TextBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*imp.glob_entry);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TextBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*imp.replace_entry);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::ValueText,
        ])
        .states(&[gtk4::AccessibleState::Disabled])
        .assert_on(&*imp.replace_all_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[gtk4::AccessibleProperty::Label])
        .states(&[
            gtk4::AccessibleState::Disabled,
            gtk4::AccessibleState::Hidden,
        ])
        .assert_on(&*imp.undo_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&*imp.save_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::KeyShortcuts,
        ])
        .assert_on(&*imp.close_button);
}

#[test]
fn test_search_panel_accessibility_tracks_busy_error_and_result_visibility() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let imp = panel.imp();

    imp.runtime.searching.set(true);
    panel.refresh_accessibility_state_for_test();
    assert!(gtk4::test_accessible_has_state(
        &*imp.search_entry,
        gtk4::AccessibleState::Busy
    ));
    assert!(gtk4::test_accessible_has_state(
        &*imp.results_list,
        gtk4::AccessibleState::Busy
    ));

    imp.runtime.searching.set(false);
    imp.error_label.set_visible(true);
    panel.refresh_accessibility_state_for_test();
    assert!(gtk4::test_accessible_has_state(
        &*imp.search_entry,
        gtk4::AccessibleState::Invalid
    ));

    imp.error_label.set_visible(false);
    imp.results_body_revealer.set_reveal_child(true);
    panel.refresh_accessibility_state_for_test();
    assert!(!accessible_hidden(&*imp.results_list));
}

#[test]
fn test_search_panel_accessibility_tracks_replace_preview_and_undo_state() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Let the startup journal load finish before the run's journal exists.
    wait_for_journal_disk_idle(&panel);
    let imp = panel.imp();

    imp.preview.preview_pending.set(true);
    imp.replace_all_button.set_label("Preparing Preview…");
    imp.replace_all_button.set_sensitive(false);
    panel.refresh_accessibility_state_for_test();
    AccessibleAudit::new()
        .states(&[gtk4::AccessibleState::Busy, gtk4::AccessibleState::Disabled])
        .properties(&[gtk4::AccessibleProperty::ValueText])
        .assert_on(&*imp.replace_all_button);

    imp.preview.preview_pending.set(false);
    panel.set_persisted_undo_backup(sample_replace_backup("/tmp/undo.txt"));
    imp.undo_button.set_visible(true);
    panel.refresh_accessibility_state_for_test();
    assert!(!gtk4::test_accessible_has_state(
        &*imp.undo_button,
        gtk4::AccessibleState::Disabled
    ));
    assert!(!accessible_hidden(&*imp.undo_button));
}

#[test]
fn test_search_result_row_accessibility_metadata_is_bounded_and_clearable() {
    ensure_gtk_init();
    let row = gtk4::TreeExpander::new();
    let file_item = SearchResultItem::new_file("/project/src/lib.rs", "src/lib.rs", 3);

    apply_search_result_row_accessibility_for_test(&row, &file_item, Some(true));
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .states(&[gtk4::AccessibleState::Expanded])
        .assert_on(&row);

    accessibility::clear_row_accessibility(&row);
    accessibility::set_expanded(&row, None);
    assert!(!gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Label
    ));

    let long_line = "needle ".repeat(80);
    let match_item = SearchResultItem::new_match(
        "/project/src/lib.rs",
        42,
        &long_line,
        0,
        6,
        SearchMatchId::from_index(0),
    );
    apply_search_result_row_accessibility_for_test(&row, &match_item, None);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&row);
}

#[test]
fn test_search_panel_set_query() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("hello world");
    assert_eq!(panel.query(), "hello world");
}

#[test]
fn test_search_panel_set_workspace_folders() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let workspace_folders = vec![
        std::path::PathBuf::from("/project/folder1"),
        std::path::PathBuf::from("/project/folder2"),
    ];
    panel.set_workspace_folders(workspace_folders.clone());
    assert_eq!(
        panel.imp().runtime.workspace_folders.borrow().as_ref(),
        workspace_folders.as_slice()
    );
}

#[test]
fn test_start_search_uses_passed_query_spec_instead_of_live_widget_state() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("notes.txt"), "needle here\n");

    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.set_query("absent");
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        !panel.imp().runtime.searching.get()
    });

    assert_eq!(panel.query(), "absent");
    assert_eq!(panel.imp().runtime.total_matches.get(), 1);
}

#[test]
fn test_rapid_workspace_queries_keep_one_worker_and_publish_latest_generation() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    SearchPanelTestPolicy::current()
        .with_search_worker_delay(Duration::from_millis(300))
        .install();
    let dir = tempfile::tempdir().expect("single-flight search tempdir");
    fixture::write_text(
        &dir.path().join("matches.txt"),
        "first query\nsecond query\nlatest query\n",
    );
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);

    panel.set_query("first");
    let _ = panel.imp().runtime.search_debounce.invalidate();
    panel.start_search(&search_spec("first"));
    panel.set_query("second");
    let _ = panel.imp().runtime.search_debounce.invalidate();
    panel.start_search(&search_spec("second"));
    panel.set_query("latest");
    let _ = panel.imp().runtime.search_debounce.invalidate();
    panel.start_search(&search_spec("latest"));

    let evidence = panel.evidence();
    assert_eq!(
        (
            evidence.active_worker_groups,
            evidence.active_worker_groups_high_water,
            evidence.pending_search_requests
        ),
        (1, 1, 1)
    );
    wait_until(Duration::from_secs(10), || {
        !panel.is_searching() && panel.total_matches() == 1
    });

    let evidence = panel.evidence();
    assert_eq!(
        (
            evidence.active_worker_groups,
            evidence.active_worker_groups_high_water,
            evidence.pending_search_requests
        ),
        (0, 1, 0)
    );
    let accepted = panel
        .imp()
        .runtime
        .accepted_matches
        .borrow()
        .clone()
        .expect("latest accepted snapshot");
    assert_eq!(accepted.len(), 1);
    assert!(accepted[0].line_content.contains("latest query"));
}

#[test]
fn test_search_panel_connect_close_requested() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    panel.connect_close_requested(move || {
        called.set(true);
    });
    assert!(
        panel
            .imp()
            .callbacks
            .close_requested_callback
            .borrow()
            .is_some()
    );
}

#[test]
fn test_search_panel_connect_open_file() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    panel.connect_open_file(move |_path, _line| {
        called.set(true);
    });
    assert!(panel.imp().callbacks.open_file_callback.borrow().is_some());
}

#[test]
fn test_search_panel_clear_results_resets_state() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Simulate some state.
    panel.imp().runtime.total_matches.set(42);
    panel.imp().runtime.total_files.set(5);
    panel.imp().runtime.result_capped.set(true);

    // Add a file group to root_store.
    let file_item = SearchResultItem::new_file("/test.rs", "test.rs", 3);
    panel.imp().runtime.root_store.borrow().append(&file_item);

    // Trigger clear via start_search with empty query.
    panel.start_search(&search_spec(""));

    assert_eq!(panel.imp().runtime.total_matches.get(), 0);
    assert_eq!(panel.imp().runtime.total_files.get(), 0);
    assert!(!panel.imp().runtime.result_capped.get());
    assert_eq!(panel.imp().runtime.root_store.borrow().n_items(), 0);
    assert!(panel.imp().runtime.file_groups.borrow().is_empty());
}

#[test]
fn test_search_retirement_categories_release_actual_ownership_over_bounded_turns() {
    ensure_gtk_init();
    const ITEM_COUNT: usize = 300;
    let categories = [
        RetirementFixtureCategory::RootRows,
        RetirementFixtureCategory::FileGroups,
        RetirementFixtureCategory::ChildRows,
        RetirementFixtureCategory::CachedMatchRows,
        RetirementFixtureCategory::CachedFileRows,
        RetirementFixtureCategory::StreamedMatches,
        RetirementFixtureCategory::AcceptedMatches,
        RetirementFixtureCategory::Positions,
    ];

    for category in categories {
        let panel = glib::Object::builder::<LushtextSearchPanel>().build();
        populate_retirement_category(&panel, category, ITEM_COUNT);
        panel.close();
        wait_until(Duration::from_secs(10), || {
            panel.evidence().retirement_backlog == 0
        });

        let observations = panel.evidence().retirement_observations;
        assert!(
            observations.len() > 1,
            "{category:?} must span multiple bounded turns"
        );
        for observation in &observations {
            let actual_release = observation.released.total();
            assert_eq!(observation.charged, actual_release, "{category:?}");
            assert!((1..=256).contains(&actual_release), "{category:?}");
            assert!(observation.before.total() > observation.after.total());
            assert_eq!(observation.pending, !observation.terminal_drain);
        }
        let category_release = observations
            .iter()
            .map(|observation| released_for_category(observation.released, category))
            .sum::<usize>();
        assert_eq!(category_release, ITEM_COUNT, "{category:?}");
        let terminal = observations
            .last()
            .expect("terminal retirement observation");
        assert!(terminal.terminal_drain, "{category:?}");
        assert!(!terminal.pending, "{category:?}");
        assert_eq!(terminal.after.total(), 0, "{category:?}");
    }
}

#[test]
fn test_large_result_retirement_is_sliced_and_cannot_clear_new_generation() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("needle");
    let _ = panel.imp().runtime.search_debounce.invalidate();
    populate_large_result_generation(&panel, "/large-results.txt", 10_000);
    SearchPanelTestPolicy::current()
        .with_replace_preview_delay(Duration::from_millis(300))
        .install();
    panel.enter_preview_mode("replacement");
    assert!(panel.imp().preview.preview_pending.get());

    panel.close();
    assert!(!panel.imp().preview.preview_pending.get());
    assert!(panel.imp().runtime.accepted_matches.borrow().is_none());

    populate_large_result_generation(&panel, "/second-generation.txt", 10_000);
    panel.start_search(&search_spec(""));
    let live_header = SearchResultItem::new_file("/live.txt", "live.txt", 1);
    panel.imp().runtime.root_store.borrow().append(&live_header);

    assert!(panel.is_searching());
    wait_until(Duration::from_secs(10), || {
        !panel.is_searching() && !panel.imp().preview.preview_worker_running.get()
    });

    let evidence = panel.evidence();
    let retirement_high_water = evidence.retirement_rows_per_slice_high_water;
    assert!(retirement_high_water <= 256);
    let observations = evidence.retirement_observations;
    assert!(observations.len() > 1);
    assert!(observations.iter().all(|observation| {
        let actual_release = observation.released.total();
        observation.charged == actual_release && (1..=256).contains(&actual_release)
    }));
    assert!(
        observations
            .last()
            .is_some_and(|observation| observation.terminal_drain && !observation.pending)
    );
    let evidence = panel.evidence();
    let generation_high_water = evidence.retirement_generations_high_water;
    let generation_limit = evidence.retirement_backlog_limit;
    assert!(generation_high_water <= generation_limit);
    assert!(!evidence.deferred_search_pending);
    eprintln!(
        "search-live-retirement-evidence results=10000 rows_per_slice_high_water={retirement_high_water} generation_high_water={generation_high_water} generation_limit={generation_limit}"
    );
    assert_eq!(panel.imp().runtime.root_store.borrow().n_items(), 1);
    let current = panel
        .imp()
        .runtime
        .root_store
        .borrow()
        .item(0)
        .and_downcast::<SearchResultItem>()
        .expect("new generation header remains mounted");
    assert_eq!(current.file_path(), "/live.txt");
}

#[test]
fn test_retirement_backpressure_retains_only_latest_query_and_bounds_generations() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("retirement backpressure tempdir");
    fixture::write_text(&dir.path().join("latest.txt"), "latest query\n");
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);

    populate_large_result_generation(&panel, "/retired-one.txt", 10_000);
    panel.start_search(&search_spec(""));
    populate_large_result_generation(&panel, "/retired-two.txt", 10_000);
    panel.start_search(&search_spec(""));
    populate_large_result_generation(&panel, "/visible-until-resume.txt", 10_000);

    panel.start_search(&search_spec("older deferred"));
    panel.start_search(&search_spec("latest"));
    let evidence = panel.evidence();
    assert_eq!(evidence.retirement_backlog, 2);
    assert_eq!(evidence.retirement_backlog_limit, 3);
    assert!(evidence.deferred_search_pending);

    wait_until(Duration::from_secs(10), || {
        !panel.is_searching() && panel.total_matches() == 1
    });
    let evidence = panel.evidence();
    assert!(evidence.retirement_generations_high_water <= evidence.retirement_backlog_limit);
    assert!(!evidence.deferred_search_pending);
    let accepted = panel
        .imp()
        .runtime
        .accepted_matches
        .borrow()
        .clone()
        .expect("latest deferred search accepted");
    assert_eq!(accepted.len(), 1);
    assert!(accepted[0].line_content.contains("latest query"));
}

// ---------------------------------------------------------------------------
// Results height clamping
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_clamp_results_height_sets_max_content_height() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    assert_eq!(panel.imp().results_scroll.max_content_height(), -1);
    assert_eq!(panel.imp().results_scroll.height_request(), -1);

    panel.clamp_results_height(300);
    assert_eq!(panel.imp().results_scroll.min_content_height(), 300);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 300);
    assert_eq!(panel.imp().results_scroll.height_request(), 300);

    // Clamp to a smaller value.
    panel.clamp_results_height(200);
    assert_eq!(panel.imp().results_scroll.min_content_height(), 200);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 200);
    assert_eq!(panel.imp().results_scroll.height_request(), 200);
}

#[test]
fn test_search_panel_clamp_results_height_can_yield_to_short_window_budget() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // Optional results chrome must be able to shrink below its comfortable
    // height so the normal status bar keeps its allocation in tiny windows.
    panel.clamp_results_height(50);
    assert_eq!(panel.imp().results_scroll.min_content_height(), 50);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 50);
    assert_eq!(panel.imp().results_scroll.height_request(), 50);

    panel.clamp_results_height(0);
    assert_eq!(panel.imp().results_scroll.min_content_height(), 0);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 0);
    assert_eq!(panel.imp().results_scroll.height_request(), 0);

    panel.clamp_results_height(-10);
    assert_eq!(panel.imp().results_scroll.min_content_height(), 0);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 0);
    assert_eq!(panel.imp().results_scroll.height_request(), 0);
}

#[test]
fn test_search_panel_clamp_results_height_guard_skips_redundant_set() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    // First call sets the value.
    panel.clamp_results_height(300);
    assert_eq!(panel.imp().results_scroll.min_content_height(), 300);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 300);

    // Second call with the same value is a no-op (guard check).
    // We verify by confirming the value is still correct — the guard prevents
    // unnecessary set_max_content_height calls that would trigger re-layout.
    panel.clamp_results_height(300);
    assert_eq!(panel.imp().results_scroll.max_content_height(), 300);
    assert_eq!(panel.imp().results_scroll.height_request(), 300);
}

// ---------------------------------------------------------------------------
// Window integration: toggle action + revealer
// ---------------------------------------------------------------------------

#[test]
fn test_toggle_search_panel_action_exists_and_enabled() {
    ensure_gtk_init();
    let window = test_window();
    let action = window.lookup_action("toggle-search-panel");
    assert!(action.is_some(), "toggle-search-panel action must exist");
    assert!(
        action.expect("expected operation to succeed").is_enabled(),
        "action must be enabled"
    );
}

#[test]
fn test_toggle_search_panel_revealer_shows_on_activate() {
    ensure_gtk_init();
    let window = test_window();

    // Initially hidden (GSettings default is false).
    let revealer = &window.imp().search_panel_revealer;
    assert!(!revealer.reveals_child());

    // Activate the toggle action — should reveal the panel.
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-search-panel", None);
    flush_events();
    assert!(revealer.reveals_child());
}

#[test]
fn test_toggle_search_panel_close_hides_revealer() {
    ensure_gtk_init();
    let window = test_window();

    // Open the panel.
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-search-panel", None);
    flush_events();
    assert!(window.imp().search_panel_revealer.reveals_child());

    // Close the panel.
    window.close_search_panel();
    flush_events();
    assert!(!window.imp().search_panel_revealer.reveals_child());
}

// ---------------------------------------------------------------------------
// Match range on SearchResultItem
// ---------------------------------------------------------------------------

#[test]
fn test_search_result_item_match_range_stored_and_returned() {
    ensure_gtk_init();
    let item = SearchResultItem::new_match(
        "/path/test.rs",
        10,
        "let x = 42;",
        4,
        5,
        SearchMatchId::from_index(0),
    );
    assert_eq!(item.match_start(), 4);
    assert_eq!(item.match_end(), 5);
}

#[test]
fn test_search_result_item_match_range_defaults_to_zero() {
    ensure_gtk_init();
    let item = SearchResultItem::new_file("/path/test.rs", "test.rs", 3);
    assert_eq!(item.match_start(), 0);
    assert_eq!(item.match_end(), 0);
}

// ---------------------------------------------------------------------------
// Toggle buttons on search panel
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_has_toggle_template_children() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let imp = panel.imp();
    assert!(!imp.case_toggle.is_active());
    assert!(!imp.regex_toggle.is_active());
    assert!(!imp.word_toggle.is_active());
    assert!(imp.more_toggle.is_sensitive());
}

#[test]
fn test_search_panel_toggle_initial_state_all_off() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(!panel.imp().case_toggle.is_active());
    assert!(!panel.imp().regex_toggle.is_active());
    assert!(!panel.imp().word_toggle.is_active());
}

#[test]
fn test_search_panel_gsettings_keys_exist_with_defaults() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(!panel.imp().case_toggle.is_active());
    assert!(!panel.imp().regex_toggle.is_active());
    assert!(!panel.imp().word_toggle.is_active());
}

// ---------------------------------------------------------------------------
// Options panel, gitignore toggle, glob filter
// ---------------------------------------------------------------------------

#[test]
fn test_more_toggle_is_sensitive() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(panel.imp().more_toggle.is_sensitive());
}

#[test]
fn test_options_revealer_exists_and_starts_hidden() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Options revealer starts with reveal_child=false (GSettings default).
    assert!(!panel.imp().options_revealer.reveals_child());
}

#[test]
fn test_gitignore_toggle_exists_and_starts_active() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Gitignore toggle defaults to active=true (GSettings default for search-gitignore).
    assert!(panel.imp().gitignore_toggle.is_active());
}

#[test]
fn test_glob_entry_exists_and_starts_empty() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(panel.imp().glob_entry.text().is_empty());
}

#[test]
fn test_gsettings_search_panel_options_expanded_default() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(!panel.imp().more_toggle.is_active());
    assert!(!panel.imp().options_revealer.reveals_child());
}

#[test]
fn test_gsettings_search_gitignore_default() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // GSettings default for search-gitignore is true.
    assert!(panel.imp().gitignore_toggle.is_active());
}

#[test]
fn test_clear_results_removes_warning_class() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Simulate warning state from truncation.
    panel.imp().count_label.add_css_class("warning");
    assert!(panel.imp().count_label.has_css_class("warning"));

    panel.start_search(&search_spec(""));
    assert!(!panel.imp().count_label.has_css_class("warning"));
}

// ---------------------------------------------------------------------------
// SearchEvent::Progress variant
// ---------------------------------------------------------------------------

#[test]
fn test_search_event_progress_variant() {
    ensure_gtk_init();
    // Progress variant can be constructed and pattern-matched.
    let event = SearchEvent::Progress(42);
    assert_matches!(event, SearchEvent::Progress(42));
}

// ---------------------------------------------------------------------------
// Match navigation state
// ---------------------------------------------------------------------------

#[test]
fn test_navigate_next_match_empty_is_noop() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.navigate_next_match();
    assert!(panel.imp().navigation.current_match_index.get().is_none());
}

#[test]
fn test_navigate_prev_match_empty_is_noop() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.navigate_prev_match();
    assert!(panel.imp().navigation.current_match_index.get().is_none());
}

#[test]
fn test_has_results_false_on_fresh_panel() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(!panel.has_results());
}

#[test]
fn test_has_results_true_after_matches() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Simulate matches arriving (set internal state directly).
    panel.imp().runtime.total_matches.set(5);
    assert!(panel.has_results());
}

#[test]
fn test_current_match_index_resets_on_clear() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Simulate some navigation state.
    panel.imp().navigation.current_match_index.set(Some(3));
    panel
        .imp()
        .navigation
        .match_positions
        .borrow_mut()
        .push(SearchMatchLocation::new(
            std::path::PathBuf::from("/test.rs"),
            10,
        ));

    // Clear via empty search.
    panel.start_search(&search_spec(""));

    assert!(panel.imp().navigation.current_match_index.get().is_none());
    assert!(panel.imp().navigation.match_positions.borrow().is_empty());
}

// ---------------------------------------------------------------------------
// F4/Shift+F4 actions and shortcuts
// ---------------------------------------------------------------------------

#[test]
fn test_f4_shortcut_bound_search_next_match() {
    ensure_gtk_init();
    let window = test_window();
    let action = window.lookup_action("search-next-match");
    assert!(action.is_some(), "search-next-match action must exist");
}

#[test]
fn test_shift_f4_shortcut_bound_search_prev_match() {
    ensure_gtk_init();
    let window = test_window();
    let action = window.lookup_action("search-prev-match");
    assert!(action.is_some(), "search-prev-match action must exist");
}

#[test]
fn test_search_navigation_actions_start_disabled() {
    ensure_gtk_init();
    let window = test_window();
    let next = window
        .lookup_action("search-next-match")
        .expect("expected operation to succeed");
    assert!(
        !next.is_enabled(),
        "search-next-match should start disabled"
    );

    let prev = window
        .lookup_action("search-prev-match")
        .expect("expected operation to succeed");
    assert!(
        !prev.is_enabled(),
        "search-prev-match should start disabled"
    );
}

// ---------------------------------------------------------------------------
// Status bar notification rendering
// ---------------------------------------------------------------------------

#[test]
fn test_status_bar_starts_empty() {
    ensure_gtk_init();
    let bar = glib::Object::builder::<LushtextStatusBar>().build();
    assert!(bar.imp().message_label.text().is_empty());
}

#[test]
fn test_render_progress_message_sets_label() {
    ensure_gtk_init();
    let bar = glib::Object::builder::<LushtextStatusBar>().build();
    bar.render_message(Some(&StatusMessage {
        text: "Searching 100 / 500 files\u{2026}".to_string(),
        severity: NotificationSeverity::Info,
    }));
    assert_eq!(
        bar.imp().message_label.text().as_str(),
        "Searching 100 / 500 files\u{2026}"
    );
}

#[test]
fn test_notification_bus_prefers_transient_over_progress() {
    ensure_gtk_init();
    let bus = NotificationBus::default();
    bus.publish(
        NotificationOwner::Search,
        NotificationSurface::StatusBar,
        NotificationPayload::Progress(StatusMessage {
            text: "Searching...".to_string(),
            severity: NotificationSeverity::Info,
        }),
    );
    bus.publish(
        NotificationOwner::Window,
        NotificationSurface::StatusBar,
        NotificationPayload::Transient(StatusMessage {
            text: "Error!".to_string(),
            severity: NotificationSeverity::Error,
        }),
    );

    let view = bus.status_bar_view().expect("status bar view exists");
    assert_eq!(view.text, "Error!");
    assert_eq!(view.severity, NotificationSeverity::Error);
}

#[test]
fn test_connect_navigate_callback_stored() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.connect_navigate_to_match(|_path, _line| {});
    assert!(panel.imp().callbacks.navigate_callback.borrow().is_some());
}

#[test]
fn test_connect_search_progress_callback_stored() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.connect_search_progress(|update| {
        assert_matches!(
            update,
            SearchProgressUpdate::Started
                | SearchProgressUpdate::Progress { .. }
                | SearchProgressUpdate::Cancelled { .. }
                | SearchProgressUpdate::Done { .. }
        );
    });
    assert!(panel.imp().callbacks.progress_callback.borrow().is_some());
}

// ---------------------------------------------------------------------------
// Replace UI widgets exist
// ---------------------------------------------------------------------------

#[test]
fn test_replace_entry_exists_on_panel() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // replace_entry is accessible as a template child.
    assert!(panel.imp().replace_entry.text().is_empty());
}

#[test]
fn test_replace_all_button_starts_insensitive() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // replace_all_button starts with sensitive=false (no text, no results).
    assert!(
        !panel.imp().replace_all_button.is_sensitive(),
        "replace_all_button should start insensitive"
    );
}

#[test]
fn test_undo_button_starts_hidden() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(
        !panel.imp().undo_button.property::<bool>("visible"),
        "undo_button should start hidden"
    );
}

#[test]
fn test_enter_preview_mode_sets_flag() {
    ensure_gtk_init();
    let panel = panel_with_one_search_match();
    assert!(!panel.is_preview_mode());

    panel.enter_preview_mode("goodbye");
    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert!(panel.is_preview_mode());
}

#[test]
fn test_enter_preview_mode_uses_cached_search_matches_without_gtk_rows() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("hello");
    flush_after_delay(Duration::from_millis(200));
    let _ = panel.imp().runtime.search_debounce.invalidate();
    panel.imp().runtime.total_matches.set(1);
    panel
        .imp()
        .runtime
        .accepted_matches
        .replace(Some(Arc::new(vec![SearchMatch::new(
            std::path::PathBuf::from("/test.rs"),
            1,
            "let hello = 1;",
            4..9,
        )])));

    panel.enter_preview_mode("goodbye");

    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert_eq!(panel.replace_preview_count(), 1);
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Replace 1 checked")
    );
}

#[test]
fn test_exit_preview_mode_clears_state() {
    ensure_gtk_init();
    let panel = panel_with_one_search_match();

    panel.enter_preview_mode("goodbye");
    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert!(panel.is_preview_mode());

    panel.exit_preview_mode();
    assert!(!panel.is_preview_mode());
    assert!(panel.imp().preview.preview_outcome.borrow().is_none());
    assert!(panel.imp().preview.checked_match_ids.borrow().is_empty());
    wait_until(Duration::from_secs(10), || !panel.replace_preview_pending());
    assert_eq!(panel.evidence().preview_retirement_pending, 0);
}

#[test]
fn test_enter_preview_mode_shows_pending_until_worker_finishes() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = panel_with_one_search_match();

    SearchPanelTestPolicy::current()
        .with_replace_preview_delay(Duration::from_millis(250))
        .install();
    panel.enter_preview_mode("goodbye");

    assert!(panel.imp().preview.preview_pending.get());
    assert!(!panel.is_preview_mode());
    assert!(!panel.imp().replace_all_button.is_sensitive());
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Preparing Preview…")
    );

    wait_until(Duration::from_secs(10), || {
        panel.is_preview_mode() && !panel.imp().preview.preview_pending.get()
    });
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Replace 1 checked")
    );
}

#[test]
fn test_stale_replace_preview_result_is_rejected_after_replacement_change() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = panel_with_one_search_match();

    SearchPanelTestPolicy::current()
        .with_replace_preview_delay(Duration::from_millis(250))
        .install();
    panel.enter_preview_mode("goodbye");
    assert!(panel.imp().preview.preview_pending.get());

    panel.imp().replace_entry.set_text("newer replacement");
    flush_events();

    assert!(!panel.imp().preview.preview_pending.get());
    assert!(!panel.is_preview_mode());
    assert!(panel.imp().preview.preview_outcome.borrow().is_none());
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Replace All")
    );

    std::thread::sleep(Duration::from_millis(350));
    flush_events();

    assert!(!panel.is_preview_mode());
    assert!(panel.imp().preview.preview_outcome.borrow().is_none());
}

#[test]
fn test_query_edit_invalidates_preview_before_debounce() {
    ensure_gtk_init();
    let panel = panel_with_one_search_match();
    panel.enter_preview_mode("goodbye");
    wait_until(Duration::from_secs(10), || {
        !panel.imp().preview.preview_worker_running.get()
    });
    assert!(panel.is_preview_mode(), "initial preview was not accepted");

    panel.imp().search_entry.set_text("new query");
    flush_after_delay(Duration::from_millis(200));
    assert!(!panel.is_preview_mode());
    assert!(panel.imp().preview.preview_outcome.borrow().is_none());
}

#[test]
fn test_glob_edit_invalidates_preview_before_debounce() {
    ensure_gtk_init();
    let panel = panel_with_one_search_match();
    panel.enter_preview_mode("goodbye");
    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    panel.imp().glob_entry.set_text("*.rs");
    flush_events();
    assert!(!panel.is_preview_mode());
    assert!(panel.imp().preview.preview_outcome.borrow().is_none());
}

#[test]
fn test_superseding_preview_requests_coalesce_to_latest_result() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = panel_with_one_search_match();
    SearchPanelTestPolicy::current()
        .with_replace_preview_delay(Duration::from_millis(250))
        .install();

    panel.enter_preview_mode("first");
    panel.enter_preview_mode("second");
    panel.enter_preview_mode("latest");

    wait_until(Duration::from_secs(10), || {
        !panel.imp().preview.preview_worker_running.get()
    });
    assert!(
        panel.is_preview_mode(),
        "latest request was not accepted: generation={} pending={} outcome={}",
        panel.imp().preview.preview_generation.get(),
        panel.imp().preview.preview_pending.get(),
        panel.imp().preview.preview_outcome.borrow().is_some()
    );
    let outcome = panel.imp().preview.preview_outcome.borrow();
    let replacement = outcome
        .as_ref()
        .and_then(|outcome| outcome.replacements.first())
        .expect("latest preview row");
    assert_eq!(replacement.replacement.as_ref(), "latest");
    assert!(!panel.imp().preview.preview_worker_running.get());
    let evidence = panel.evidence();
    assert!(evidence.preview_retirement_jobs >= 1);
    assert_eq!(evidence.preview_retirement_pending, 0);
    assert!(!evidence.replace_preview_pending);
}

#[test]
fn test_new_search_cancels_active_preview_before_latest_request_runs() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = panel_with_one_search_match();
    SearchPanelTestPolicy::current()
        .with_replace_preview_delay(Duration::from_millis(250))
        .install();

    panel.enter_preview_mode("stale");
    panel.start_search(&search_spec(""));
    panel
        .imp()
        .runtime
        .accepted_matches
        .replace(Some(Arc::new(vec![
            SearchMatch::new(
                std::path::PathBuf::from("/test.rs"),
                1,
                "let hello = 1;",
                4..9,
            )
            .with_id(SearchMatchId::from_index(0)),
        ])));
    panel.imp().runtime.total_matches.set(1);
    panel.enter_preview_mode("latest");

    wait_until(Duration::from_secs(10), || {
        !panel.imp().preview.preview_worker_running.get()
    });
    assert!(panel.is_preview_mode(), "latest request was not accepted");
    let outcome = panel.imp().preview.preview_outcome.borrow();
    assert_eq!(
        outcome
            .as_ref()
            .and_then(|outcome| outcome.replacements.first())
            .expect("latest preview")
            .replacement
            .as_ref(),
        "latest"
    );
}

#[test]
fn test_bounded_preview_reports_omitted_and_confirms_only_generated_checked_rows() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("hello");
    flush_after_delay(Duration::from_millis(200));
    let _ = panel.imp().runtime.search_debounce.invalidate();
    let matches = vec![
        SearchMatch::new(
            std::path::PathBuf::from("/project/awkward/界 one.rs"),
            1,
            "hello one",
            0..5,
        )
        .with_id(SearchMatchId::from_index(0)),
        SearchMatch::new(
            std::path::PathBuf::from("/project/awkward/界 two.rs"),
            2,
            "hello two",
            0..5,
        )
        .with_id(SearchMatchId::from_index(1)),
        SearchMatch::new(
            std::path::PathBuf::from("/project/awkward/界 three.rs"),
            3,
            "hello three",
            0..5,
        )
        .with_id(SearchMatchId::from_index(2)),
    ];
    panel
        .imp()
        .runtime
        .accepted_matches
        .replace(Some(Arc::new(matches)));
    panel.imp().runtime.total_matches.set(3);
    panel.imp().runtime.total_files.set(3);
    SearchPanelTestPolicy::current()
        .with_replace_preview_budget(2, u64::MAX)
        .install();

    let confirmed = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Replacement>::new()));
    let captured = confirmed.clone();
    panel.connect_replace_all(move |replacements| {
        captured.replace(replacements);
    });
    panel.enter_preview_mode("goodbye");

    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert_eq!(panel.replace_preview_count(), 2);
    assert_eq!(panel.checked_replacement_count(), 2);
    assert_eq!(panel.omitted_replacement_count(), 1);
    assert_eq!(panel.skipped_replacement_count(), 0);
    assert_eq!(
        panel.imp().count_label.text(),
        "2 previewed, 2 checked, 1 omitted, 0 truncated, 0 stale ranges"
    );
    assert_eq!(
        panel.imp().results_scroll.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );

    panel
        .imp()
        .preview
        .checked_match_ids
        .borrow_mut()
        .remove(&SearchMatchId::from_index(1));
    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(10), || {
        !panel.imp().preview.preview_pending.get()
            && !panel.imp().preview.preview_worker_running.get()
            && confirmed.borrow().len() == 1
    });
    let confirmed = confirmed.borrow();
    assert_eq!(confirmed.len(), 1);
    assert_eq!(confirmed[0].match_id, SearchMatchId::from_index(0));
    assert_eq!(panel.evidence().preview_selection_jobs, 1);
}

#[test]
fn test_invalid_preview_rows_report_private_free_reason_counts_and_never_confirm() {
    ensure_gtk_init();
    const SOURCE_SENTINEL: &str = "PRIVATE-SOURCE-WIDGET-55c7";
    const REPLACEMENT_SENTINEL: &str = "PRIVATE-REPLACEMENT-WIDGET-a821";
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("([a-z]+)");
    panel.imp().regex_toggle.set_active(true);
    flush_after_delay(Duration::from_millis(200));
    let _ = panel.imp().runtime.search_debounce.invalidate();
    panel
        .imp()
        .runtime
        .accepted_matches
        .replace(Some(Arc::new(vec![
            SearchMatch::new(PathBuf::from("/project/valid.rs"), 1, "abc", 0..3)
                .with_id(SearchMatchId::from_index(0)),
            SearchMatch::new(
                PathBuf::from("/project/stale.rs"),
                2,
                &format!("123 {SOURCE_SENTINEL}"),
                0..3,
            )
            .with_id(SearchMatchId::from_index(1)),
        ])));
    panel.imp().runtime.total_matches.set(2);
    panel.imp().runtime.total_files.set(2);

    let confirmed = Rc::new(RefCell::new(Vec::<Replacement>::new()));
    let captured = Rc::clone(&confirmed);
    panel.connect_replace_all(move |replacements| {
        captured.replace(replacements);
    });
    panel.enter_preview_mode(REPLACEMENT_SENTINEL);

    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert_eq!(panel.replace_preview_count(), 1);
    assert_eq!(panel.checked_replacement_count(), 1);
    assert_eq!(panel.skipped_replacement_count(), 1);
    let summary = panel.imp().count_label.text();
    assert_eq!(
        summary,
        "1 previewed, 1 checked, 0 omitted, 0 truncated, 1 stale ranges"
    );
    assert!(!summary.contains(SOURCE_SENTINEL));
    assert!(!summary.contains(REPLACEMENT_SENTINEL));

    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(10), || {
        !panel.imp().preview.preview_pending.get()
            && !panel.imp().preview.preview_worker_running.get()
            && confirmed.borrow().len() == 1
    });
    let confirmed = confirmed.borrow();
    assert_eq!(confirmed.len(), 1);
    assert_eq!(confirmed[0].match_id, SearchMatchId::from_index(0));
}

#[test]
fn test_all_unchecked_preview_rows_retire_without_apply() {
    ensure_gtk_init();
    let panel = panel_with_one_search_match();
    let confirmed = Rc::new(RefCell::new(Vec::<Replacement>::new()));
    let captured = Rc::clone(&confirmed);
    panel.connect_replace_all(move |replacements| {
        captured.replace(replacements);
    });
    panel.enter_preview_mode("goodbye");
    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    panel.imp().preview.checked_match_ids.borrow_mut().clear();

    panel.activate_confirm_replacements();

    wait_until(Duration::from_secs(10), || {
        !panel.imp().preview.preview_pending.get()
            && !panel.imp().preview.preview_worker_running.get()
    });
    assert!(confirmed.borrow().is_empty());
    let evidence = panel.evidence();
    assert_eq!(evidence.preview_selection_jobs, 1);
    assert_eq!(evidence.preview_retirement_jobs, 0);
    assert!(!evidence.preview_worker_running);
    assert_eq!(evidence.queued_preview_requests, 0);
}

#[test]
fn test_confirmation_selection_rejects_stale_generation_off_main() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = panel_with_one_search_match();
    let confirmed = Rc::new(RefCell::new(Vec::<Replacement>::new()));
    let captured = Rc::clone(&confirmed);
    panel.connect_replace_all(move |replacements| {
        captured.replace(replacements);
    });
    panel.set_persisted_undo_backup(sample_replace_backup("/prior-undo.rs"));
    panel.show_undo_button();
    let undo_calls = Rc::new(RefCell::new(0u32));
    let captured_undo_calls = Rc::clone(&undo_calls);
    panel.connect_undo_all(move |_| {
        *captured_undo_calls.borrow_mut() += 1;
    });
    panel.enter_preview_mode("goodbye");
    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    SearchPanelTestPolicy::current()
        .with_preview_selection_delay(Duration::from_millis(250))
        .install();

    panel.activate_confirm_replacements();
    assert!(panel.imp().preview.preview_pending.get());
    assert!(!panel.imp().undo_button.is_sensitive());
    panel.activate_undo_replacements();
    assert_eq!(*undo_calls.borrow(), 0);
    assert!(panel.has_undo_backup());
    panel.imp().search_entry.set_text("newer query");
    flush_events();

    wait_until(Duration::from_secs(10), || {
        !panel.imp().preview.preview_worker_running.get()
    });
    assert!(confirmed.borrow().is_empty());
    assert!(!panel.is_preview_mode());
    assert!(!panel.imp().preview.preview_pending.get());
    let evidence = panel.evidence();
    assert_eq!(evidence.preview_selection_jobs, 1);
    assert!(evidence.preview_retirement_jobs >= 1);
    assert!(!evidence.preview_worker_running);
    assert_eq!(evidence.queued_preview_requests, 0);
    assert!(panel.imp().undo_button.is_sensitive());
    panel.activate_undo_replacements();
    assert_eq!(*undo_calls.borrow(), 1);
}

#[test]
fn test_entering_new_preview_retires_visible_outcome_before_latest_generation() {
    ensure_gtk_init();
    let panel = panel_with_one_search_match();
    panel.enter_preview_mode("first");
    wait_until(Duration::from_secs(10), || panel.is_preview_mode());

    panel.enter_preview_mode("latest");

    wait_until(Duration::from_secs(10), || {
        panel.is_preview_mode() && !panel.imp().preview.preview_worker_running.get()
    });
    let outcome = panel.imp().preview.preview_outcome.borrow();
    assert_eq!(
        outcome
            .as_ref()
            .and_then(|outcome| outcome.replacements.first())
            .expect("latest preview row")
            .replacement
            .as_ref(),
        "latest"
    );
    assert!(panel.evidence().preview_retirement_jobs >= 1);
}

#[test]
fn test_no_eligible_preview_has_explicit_feedback_and_disabled_confirmation() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("needle");
    flush_after_delay(Duration::from_millis(200));
    let _ = panel.imp().runtime.search_debounce.invalidate();
    let line = format!("{}needle{}", "a".repeat(5000), "b".repeat(5000));
    let search_match = SearchMatch::new(
        std::path::PathBuf::from("/project/深い/very-long-name.rs"),
        1,
        &line,
        5000..5006,
    )
    .with_id(SearchMatchId::from_index(0));
    panel
        .imp()
        .runtime
        .accepted_matches
        .replace(Some(Arc::new(vec![search_match])));
    panel.imp().runtime.total_matches.set(1);
    panel.imp().runtime.total_files.set(1);

    panel.enter_preview_mode("");

    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert_eq!(panel.replace_preview_count(), 0);
    assert_eq!(panel.omitted_replacement_count(), 0);
    assert_eq!(panel.skipped_replacement_count(), 1);
    assert_eq!(
        panel.imp().count_label.text(),
        "No eligible replacements; 0 omitted, 1 truncated, 0 stale ranges"
    );
    assert!(!panel.imp().replace_all_button.is_sensitive());
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Replace 0 checked")
    );

    panel.clamp_results_height(0);
    assert_eq!(panel.imp().results_scroll.height_request(), 0);
    assert!(panel.imp().footer_box.property::<bool>("visible"));
    assert!(panel.imp().replace_all_button.property::<bool>("visible"));
}

#[test]
fn test_byte_limited_preview_reports_omitted_when_no_row_fits() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = panel_with_one_search_match();
    SearchPanelTestPolicy::current()
        .with_replace_preview_budget(u64::MAX, 1)
        .install();

    panel.enter_preview_mode(&"replacement".repeat(1024));

    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert_eq!(panel.replace_preview_count(), 0);
    assert_eq!(panel.checked_replacement_count(), 0);
    assert_eq!(panel.omitted_replacement_count(), 1);
    assert_eq!(panel.skipped_replacement_count(), 0);
    assert_eq!(
        panel.imp().count_label.text(),
        "No eligible replacements; 1 omitted, 0 truncated, 0 stale ranges"
    );
    assert!(!panel.imp().replace_all_button.is_sensitive());
}

#[test]
fn test_large_and_empty_replacement_previews_remain_accessible() {
    ensure_gtk_init();
    let panel = panel_with_one_search_match();
    let large_replacement = "界".repeat(2048);

    panel.enter_preview_mode(&large_replacement);
    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert_eq!(panel.replace_preview_count(), 1);
    assert!(panel.imp().replace_all_button.is_sensitive());
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*panel.imp().replace_all_button);

    panel.exit_preview_mode();
    panel.enter_preview_mode("");
    wait_until(Duration::from_secs(10), || panel.is_preview_mode());
    assert_eq!(panel.replace_preview_count(), 1);
    let sharing_evidence = panel.evidence();
    assert_eq!(sharing_evidence.shared_snapshot_handoffs, 2);
    assert_eq!(sharing_evidence.whole_result_clones, 0);
    assert_eq!(
        panel
            .imp()
            .preview
            .preview_outcome
            .borrow()
            .as_ref()
            .expect("preview")
            .replacements[0]
            .replaced_line,
        "let  = 1;"
    );
}

#[test]
fn test_clear_results_preserves_undo_backup() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Let the startup journal load finish before the run's journal exists.
    wait_for_journal_disk_idle(&panel);
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);

    // Simulate an undo backup.
    let backup = sample_replace_backup("/test.rs");
    panel.set_undo_backup(backup.clone());
    panel.show_undo_button();
    assert!(panel.imp().preview.undo_backup.borrow().is_some());
    wait_for_journal_disk_idle(&panel);
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    // Starting a new search should not discard the rollback path for a
    // previous Replace All. Undo remains available until it is used or a new
    // Replace All replaces the backup.
    panel.start_search(&search_spec(""));
    assert!(panel.imp().preview.undo_backup.borrow().is_some());
    assert!(
        panel.imp().undo_button.property::<bool>("visible"),
        "undo_button should stay visible after a new search starts"
    );
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    let _ = search_backup::delete(&data_dir);
}

#[test]
fn test_search_panel_restores_active_persisted_undo_backup_on_construction() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);

    let backup = search_backup::fixture::write_stream_journal(
        &data_dir,
        &[(
            std::path::Path::new("/persisted.rs"),
            b"persisted content",
            b"persisted replacement",
        )],
    )
    .expect("expected operation to succeed");

    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    wait_until(Duration::from_secs(2), || {
        panel.has_undo_backup() && panel.imp().undo_button.property::<bool>("visible")
    });
    assert!(panel.evidence().undo_backup_matches(&backup));
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    let _ = search_backup::delete(&data_dir);
}

#[test]
fn test_clearing_persisted_undo_cancels_capacity_retry_before_it_can_reload() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);
    // The persisted journal the panel finds at construction.
    sample_replace_backup("/persisted-capacity.rs");
    let capacity_hold = hold_disposal_capacity_for_test();

    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    wait_until(Duration::from_secs(2), || {
        panel.evidence().undo_capacity_retry_pending
    });
    assert!(!panel.has_undo_backup());

    panel.clear_undo_backup_for_test();
    assert!(!panel.evidence().undo_capacity_retry_pending);
    drop(capacity_hold);
    flush_after_delay(Duration::from_millis(350));

    assert!(!panel.has_undo_backup());
    assert!(!panel.evidence().undo_capacity_retry_pending);
    wait_for_journal_disk_idle(&panel);
    assert!(
        search_backup::load(&data_dir)
            .expect("load cleared persisted undo")
            .is_empty()
    );
}

#[test]
fn test_persisted_undo_load_resumes_after_disposal_capacity_clears() {
    ensure_gtk_init();
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);
    let backup = sample_replace_backup("/persisted-capacity-resume.rs");
    let capacity_hold = hold_disposal_capacity_for_test();

    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    wait_until(Duration::from_secs(2), || {
        panel.evidence().undo_capacity_retry_pending
    });
    assert!(!panel.has_undo_backup());

    drop(capacity_hold);
    wait_until(Duration::from_secs(5), || {
        panel.has_undo_backup() && panel.evidence().undo_backup_matches(&backup)
    });
    assert!(!panel.evidence().undo_capacity_retry_pending);

    let _ = search_backup::delete(&data_dir);
}

#[test]
fn test_search_panel_close_preserves_durable_undo_backup() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Let the startup journal load finish before the run's journal exists.
    wait_for_journal_disk_idle(&panel);
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);

    let backup = sample_replace_backup("/persisted-close.rs");
    panel.set_undo_backup(backup.clone());
    panel.show_undo_button();
    wait_for_journal_disk_idle(&panel);
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    panel.close();

    assert!(panel.evidence().undo_backup_matches(&backup));
    assert!(panel.imp().undo_button.property::<bool>("visible"));
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    let _ = search_backup::delete(&data_dir);
}

#[test]
fn test_search_panel_close_cancels_active_search() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let cancel = Arc::new(AtomicBool::new(false));
    panel
        .imp()
        .runtime
        .cancel_token
        .replace(Some(cancel.clone()));
    panel.imp().runtime.searching.set(true);

    panel.close();

    assert!(cancel.load(Ordering::Relaxed));
    assert!(panel.imp().runtime.cancel_token.borrow().is_none());
    assert!(!panel.imp().runtime.searching.get());
}

#[test]
fn test_set_undo_backup_updates_ui_before_delayed_disk_save() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // The panel's startup journal load runs on the disk lane; let it finish
    // before the run's journal exists, as a run always starts after it.
    wait_for_journal_disk_idle(&panel);
    let backup = sample_replace_backup("/delayed-save.rs");

    SearchPanelTestPolicy::current()
        .with_undo_backup_disk_delay(Duration::from_millis(250))
        .install();
    panel.set_undo_backup(backup.clone());
    panel.show_undo_button();

    assert!(panel.imp().preview.undo_backup.borrow().is_some());
    assert!(panel.imp().undo_button.property::<bool>("visible"));
    assert_eq!(
        panel.evidence().journal_disk_jobs_in_flight,
        1,
        "disk persistence should still be sleeping while UI undo state is visible",
    );
    wait_for_journal_disk_idle(&panel);
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    let _ = search_backup::delete(&data_dir);
}

#[test]
fn test_clear_undo_backup_updates_ui_before_delayed_disk_delete() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // The panel's startup journal load runs on the disk lane; let it finish
    // before the run's journal exists, as a run always starts after it.
    wait_for_journal_disk_idle(&panel);
    let backup = sample_replace_backup("/delayed-delete.rs");

    panel.set_undo_backup(backup.clone());
    panel.show_undo_button();
    wait_for_journal_disk_idle(&panel);
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup
    );

    SearchPanelTestPolicy::current()
        .with_undo_backup_disk_delay(Duration::from_millis(250))
        .install();
    panel.clear_undo_backup_for_test();

    assert!(panel.imp().preview.undo_backup.borrow().is_none());
    assert!(!panel.imp().undo_button.property::<bool>("visible"));
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        backup,
        "disk backup should still exist while delayed delete is sleeping",
    );
    wait_for_journal_disk_idle(&panel);
    assert!(
        search_backup::load(&data_dir)
            .expect("expected operation to succeed")
            .is_empty()
    );
}

#[test]
fn test_clear_after_delayed_undo_backup_save_keeps_disk_empty() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Let the startup journal load finish before the run's journal exists.
    wait_for_journal_disk_idle(&panel);
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);
    let backup = sample_replace_backup("/clear-after-save.rs");

    SearchPanelTestPolicy::current()
        .with_undo_backup_disk_delay(Duration::from_millis(250))
        .install();
    panel.set_undo_backup(backup);
    panel.clear_undo_backup_for_test();

    assert!(panel.imp().preview.undo_backup.borrow().is_none());
    wait_for_journal_disk_idle(&panel);
    assert!(
        search_backup::load(&data_dir)
            .expect("expected operation to succeed")
            .is_empty()
    );
}

#[test]
fn test_save_after_delayed_undo_backup_clear_keeps_newer_disk_backup() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Let the startup journal load finish before the run's journal exists.
    wait_for_journal_disk_idle(&panel);
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);
    let old_backup = sample_replace_backup("/old-backup.rs");
    let new_backup = sample_replace_descriptors("/new-backup.rs");

    panel.set_undo_backup(old_backup.clone());
    wait_for_journal_disk_idle(&panel);
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        old_backup
    );

    SearchPanelTestPolicy::current()
        .with_undo_backup_disk_delay(Duration::from_millis(250))
        .install();
    panel.clear_undo_backup_for_test();
    // A newer Replace All commits its own journal before the panel shows it.
    sample_replace_backup("/new-backup.rs");
    panel.set_undo_backup(new_backup.clone());

    wait_for_journal_disk_idle(&panel);
    assert_eq!(
        search_backup::load(&data_dir).expect("expected operation to succeed"),
        new_backup
    );
}

#[test]
fn test_reserved_replace_generation_blocks_stale_delete_after_service_commit() {
    ensure_gtk_init();
    let _reset = SearchPanelDelayReset;
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Let the startup journal load finish before the run's journal exists.
    wait_for_journal_disk_idle(&panel);
    let data_dir = json_store::data_dir();
    let _ = search_backup::delete(&data_dir);
    let old_backup = sample_replace_backup("/old-committed.rs");
    let new_backup = sample_replace_descriptors("/new-committed.rs");

    panel.set_undo_backup(old_backup.clone());
    wait_for_journal_disk_idle(&panel);
    assert_eq!(
        search_backup::load(&data_dir).expect("load old journal"),
        old_backup
    );

    SearchPanelTestPolicy::current()
        .with_undo_backup_disk_delay(Duration::from_millis(250))
        .install();
    panel.clear_undo_backup_for_test();
    let reserved = panel.reserve_undo_backup_generation_for_test();
    sample_replace_backup("/new-committed.rs");
    assert!(panel.set_persisted_undo_backup_for_generation_for_test(new_backup.clone(), reserved));

    flush_after_delay(Duration::from_millis(400));
    assert_eq!(
        search_backup::load(&data_dir).expect("load current journal"),
        new_backup
    );
    assert!(panel.evidence().undo_backup_matches(&new_backup));
}

#[test]
fn test_replace_transaction_blocks_second_apply_and_prior_undo_activation() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Let the startup journal load finish before the run's journal exists.
    wait_for_journal_disk_idle(&panel);
    let backup = sample_replace_backup("/prior-undo.rs");
    panel.set_undo_backup(backup);
    panel.show_undo_button();
    let undo_calls = Rc::new(RefCell::new(0u32));
    let captured_calls = Rc::clone(&undo_calls);
    panel.connect_undo_all(move |_| {
        *captured_calls.borrow_mut() += 1;
    });

    assert!(panel.begin_replace_transaction_for_test().is_some());
    assert!(panel.replace_preview_pending());
    assert!(!panel.imp().undo_button.is_sensitive());
    assert!(panel.begin_replace_transaction_for_test().is_none());
    panel.activate_undo_replacements();
    assert_eq!(*undo_calls.borrow(), 0);
    assert!(panel.has_undo_backup());

    panel.finish_replace_transaction_for_test();
    assert!(!panel.replace_preview_pending());
    assert!(panel.imp().undo_button.is_sensitive());
    panel.activate_undo_replacements();
    assert_eq!(*undo_calls.borrow(), 1);
}

// ---------------------------------------------------------------------------
// Model types
// ---------------------------------------------------------------------------

#[test]
fn test_replacement_and_replace_result_construction() {
    ensure_gtk_init();
    let r = Replacement {
        match_id: SearchMatchId::from_index(0),
        path: std::path::PathBuf::from("/test.rs"),
        line_number: 1,
        original_line: "let hello = 1;".into(),
        replaced_line: "let goodbye = 1;".to_string(),
        replacement: "goodbye".into(),
        match_range: 4..9,
    };
    assert_eq!(r.path.display().to_string(), "/test.rs");
    assert_eq!(r.line_number, 1);
    assert_eq!(r.original_line.as_ref(), "let hello = 1;");
    assert_eq!(r.replaced_line, "let goodbye = 1;");

    let result = ReplaceResult {
        replaced_count: 5,
        files_affected: 2,
        skipped_count: 1,
        error_count: 0,
        skipped_sample: {
            let mut sample = BoundedDiagnosticSample::default();
            sample.record_path(std::path::Path::new("/skip.rs"));
            sample
        },
        error_sample: BoundedDiagnosticSample::default(),
        affected_open_paths: Vec::new(),
    };
    assert_eq!(result.replaced_count, 5);
    assert_eq!(result.files_affected, 2);
    assert_eq!(result.skipped_count, 1);
}

#[test]
fn test_generate_replacement_preview_literal() {
    ensure_gtk_init();
    let matches = vec![SearchMatch::new(
        std::path::PathBuf::from("/test.rs"),
        1,
        "let hello = 1;",
        4..9,
    )];

    let options = ContentSearchOptions::default();
    let result = generate_replacement_preview(&matches, "hello", "goodbye", &options);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].original_line.as_ref(), "let hello = 1;");
    assert_eq!(result[0].replaced_line, "let goodbye = 1;");
    assert_eq!(result[0].replacement.as_ref(), "goodbye");
}

#[test]
fn test_generate_replacement_preview_regex_backreference() {
    ensure_gtk_init();
    let matches = vec![SearchMatch::new(
        std::path::PathBuf::from("/test.rs"),
        1,
        "fn hello_world() {}",
        3..14,
    )];

    let options = ContentSearchOptions {
        regex: true,
        ..Default::default()
    };
    // Regex: capture word, replace with prefix.
    let result = generate_replacement_preview(&matches, r"(\w+)_(\w+)", "new_${1}_${2}", &options);

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].replaced_line, "fn new_hello_world() {}",
        "backreference should expand correctly"
    );
}

#[test]
fn test_generate_replacement_preview_large_match_set() {
    ensure_gtk_init();
    let matches = (0..5_000)
        .map(|index| {
            SearchMatch::new(
                std::path::PathBuf::from("/test.rs"),
                index + 1,
                &format!("let hello_{index} = hello;"),
                4..9,
            )
        })
        .collect::<Vec<_>>();
    let options = ContentSearchOptions::default();

    let result = generate_replacement_preview(&matches, "hello", "goodbye", &options);

    assert_eq!(result.len(), matches.len());
    assert_eq!(result[0].replaced_line, "let goodbye_0 = hello;");
    assert_eq!(result[4_999].replaced_line, "let goodbye_4999 = hello;");
}

#[test]
fn test_connect_replace_all_callback_stored() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.connect_replace_all(|_replacements| {});
    assert!(panel.imp().callbacks.replace_callback.borrow().is_some());
}

#[test]
fn test_connect_undo_all_callback_stored() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.connect_undo_all(|_backup| {});
    assert!(panel.imp().callbacks.undo_callback.borrow().is_some());
}

#[test]
fn test_search_navigation_actions_enabled_lifecycle() {
    ensure_gtk_init();
    let window = test_window();

    // 1. Start: disabled (no tabs, no panel, no results).
    let next = window
        .lookup_action("search-next-match")
        .expect("expected operation to succeed");
    assert!(!next.is_enabled(), "should start disabled");

    // 2. Open a tab — still disabled (panel not visible, no results).
    window.new_tab();
    flush_events();
    assert!(!next.is_enabled(), "disabled: panel not visible yet");

    // 3. Show panel — still disabled (no results).
    gtk4::prelude::ActionGroupExt::activate_action(&window, "toggle-search-panel", None);
    flush_events();
    assert!(
        window.imp().search_panel_revealer.reveals_child(),
        "panel should be visible"
    );
    assert!(!next.is_enabled(), "disabled: no results yet");

    // 4. Simulate results arriving (set internal state directly).
    window.imp().search_panel.imp().runtime.total_matches.set(5);
    window.update_search_navigation_actions();
    assert!(next.is_enabled(), "enabled: tabs + panel visible + results");

    // 5. Close panel — disabled again.
    window.close_search_panel();
    flush_events();
    assert!(!next.is_enabled(), "disabled: panel closed");
}

// ---------------------------------------------------------------------------
// Search History
// ---------------------------------------------------------------------------

#[test]
fn test_history_popover_and_list_exist_on_fresh_panel() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let imp = panel.imp();
    assert!(!imp.history_popover.is_visible());
    assert_eq!(
        imp.history_list.selection_mode(),
        gtk4::SelectionMode::Single
    );
}

#[test]
fn test_set_search_history_stores_entries() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    let entries = vec![
        make_history_entry("hello", false, false, false, true, None),
        make_history_entry("world", true, true, false, false, Some("*.rs")),
    ];
    panel.set_search_history(entries);
    let retrieved = panel.search_history();
    assert_eq!(retrieved.len(), 2);
    assert_eq!(retrieved[0].spec.query, "hello");
    assert_eq!(retrieved[1].spec.query, "world");
}

#[test]
fn test_restore_from_history_sets_search_entry_text() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let entry = make_history_entry("restored query", false, false, false, true, None);
    panel.restore_from_history(&entry);
    assert_eq!(panel.query(), "restored query");
}

#[test]
fn test_restore_from_history_sets_toggle_states() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let entry = make_history_entry("test", true, true, true, false, Some("*.toml"));
    panel.restore_from_history(&entry);

    assert!(panel.imp().case_toggle.is_active());
    assert!(panel.imp().regex_toggle.is_active());
    assert!(panel.imp().word_toggle.is_active());
    assert!(!panel.imp().gitignore_toggle.is_active());
}

#[test]
fn test_restore_from_history_sets_glob_entry() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let entry = make_history_entry("test", false, false, false, true, Some("*.rs"));
    panel.restore_from_history(&entry);
    assert_eq!(panel.imp().glob_entry.text().as_str(), "*.rs");
}

#[test]
fn test_restore_from_history_with_glob_none_clears_glob_entry() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.imp().glob_entry.set_text("*.md");
    assert_eq!(panel.imp().glob_entry.text().as_str(), "*.md");

    let entry = make_history_entry("test", false, false, false, true, None);
    panel.restore_from_history(&entry);
    assert!(panel.imp().glob_entry.text().is_empty());
}

#[test]
fn test_search_history_entry_serialization_roundtrip() {
    // Pure data test — no GTK needed, but ensure_gtk_init doesn't hurt.
    ensure_gtk_init();
    let entry = make_history_entry("test query", true, false, true, false, Some("*.rs"));
    let json = serde_json::to_string(&entry).expect("expected operation to succeed");
    let deserialized: lushtext_model::workspace::content_search::SearchHistoryEntry =
        serde_json::from_str(&json).expect("expected operation to succeed");
    assert_eq!(entry, deserialized);
}

// ---------------------------------------------------------------------------
// Saved Searches & Panel State Persistence
// ---------------------------------------------------------------------------

#[test]
fn test_save_button_exists_and_starts_invisible() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;
    let save_btn = &panel.imp().save_button;
    assert!(!save_btn.property::<bool>("visible"));
}

#[test]
fn test_set_and_get_saved_searches() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;

    let entries = vec![
        make_saved_search("My Search", "fn main"),
        make_saved_search("TODOs", "TODO"),
    ];
    panel.set_saved_searches(entries);

    let retrieved = panel.saved_searches();
    assert_eq!(retrieved.len(), 2);
    assert_eq!(retrieved[0].name, "My Search");
    assert_eq!(retrieved[1].name, "TODOs");
}

#[test]
fn test_restore_from_saved_search_sets_query() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;

    let entry = SavedSearch::from_spec(
        "Test",
        SearchQuerySpec::new(
            "fn main",
            ContentSearchOptions::new(
                lushtext_model::workspace::content_search::ContentSearchFlags {
                    case_sensitive: true,
                    regex: true,
                    whole_word: false,
                    gitignore: false,
                },
                Some("*.rs".to_string()),
            ),
        ),
    );
    panel.restore_from_saved_search(&entry);

    assert_eq!(panel.imp().search_entry.text(), "fn main");
    assert!(panel.imp().case_toggle.is_active());
    assert!(panel.imp().regex_toggle.is_active());
    assert!(!panel.imp().word_toggle.is_active());
    assert!(!panel.imp().gitignore_toggle.is_active());
    assert_eq!(panel.imp().glob_entry.text(), "*.rs");
}

#[test]
fn test_restore_from_saved_search_clears_glob_when_none() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;

    // First set a glob.
    panel.imp().glob_entry.set_text("*.toml");

    let entry = make_saved_search("test", "test");
    panel.restore_from_saved_search(&entry);
    assert!(panel.imp().glob_entry.text().is_empty());
}

#[test]
fn test_remove_saved_search_updates_state() {
    ensure_gtk_init();
    let window = test_window();
    let panel = &window.imp().search_panel;

    let entries = vec![
        make_saved_search("First", "fn main"),
        make_saved_search("Second", "TODO"),
        make_saved_search("Third", "FIXME"),
    ];
    panel.set_saved_searches(entries);
    assert_eq!(panel.saved_searches().len(), 3);

    // remove_saved_search is private, so we test via the service directly
    // and verify the panel state after set_saved_searches.
    let mut current = panel.saved_searches();
    lushtext_services::search::saved_searches::remove(&mut current, 1);
    panel.set_saved_searches(current);

    let after = panel.saved_searches();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].name, "First");
    assert_eq!(after[1].name, "Third");
}

#[test]
fn test_saved_search_serialization_roundtrip() {
    ensure_gtk_init();
    let entry = SavedSearch::from_spec(
        "My Search",
        SearchQuerySpec::new(
            "fn main",
            ContentSearchOptions::new(
                lushtext_model::workspace::content_search::ContentSearchFlags {
                    case_sensitive: true,
                    regex: false,
                    whole_word: true,
                    gitignore: false,
                },
                Some("*.rs".to_string()),
            ),
        ),
    );
    let json = serde_json::to_string(&entry).expect("expected operation to succeed");
    let deserialized: SavedSearch =
        serde_json::from_str(&json).expect("expected operation to succeed");
    assert_eq!(entry, deserialized);
}

// ---------------------------------------------------------------------------
// UI Polish: visual distinction, spacing, revealer transition
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_has_search_panel_css_class() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(panel.has_css_class("search-panel"));
}

#[test]
fn test_search_panel_revealer_uses_slide_up_transition() {
    ensure_gtk_init();
    let window = test_window();
    let revealer = &window.imp().search_panel_revealer;
    assert_eq!(
        revealer.transition_type(),
        gtk4::RevealerTransitionType::SlideUp,
    );
}

#[test]
fn test_search_panel_count_label_uses_default_body_text() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    assert!(!panel.imp().count_label.has_css_class("caption"));
    assert!(!panel.imp().count_label.has_css_class("heading"));
}

#[test]
fn test_search_panel_results_revealers_start_hidden_and_match_panel_animation() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let imp = panel.imp();

    assert!(!imp.results_feedback_revealer.reveals_child());
    assert!(!imp.results_body_revealer.reveals_child());
    assert_eq!(
        imp.results_feedback_revealer.transition_type(),
        gtk4::RevealerTransitionType::SlideUp,
    );
    assert_eq!(imp.results_feedback_revealer.transition_duration(), 250);
    assert_eq!(
        imp.results_body_revealer.transition_type(),
        gtk4::RevealerTransitionType::SlideUp,
    );
    assert_eq!(imp.results_body_revealer.transition_duration(), 250);
}

#[test]
fn test_search_panel_no_results_keeps_results_body_hidden() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("notes.txt"), "completely unrelated text");

    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        !panel.imp().runtime.searching.get()
    });

    let imp = panel.imp();
    assert!(imp.results_feedback_revealer.reveals_child());
    assert!(!imp.results_body_revealer.reveals_child());
    assert_eq!(imp.runtime.total_matches.get(), 0);
    assert_eq!(imp.count_label.text().as_str(), "No results found");
}

#[test]
fn test_search_panel_first_result_reveals_fixed_max_height_results_body() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(
        &dir.path().join("notes.txt"),
        "needle one\nneedle two\nneedle three\n",
    );

    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        panel.imp().runtime.total_matches.get() > 0
    });

    let imp = panel.imp();
    assert!(imp.results_feedback_revealer.reveals_child());
    assert!(imp.results_body_revealer.reveals_child());
    assert!(imp.runtime.total_matches.get() >= 1);
    assert_eq!(imp.results_scroll.max_content_height(), 240);
    assert_eq!(imp.results_scroll.height_request(), 240);
}

#[test]
fn test_search_panel_clearing_query_hides_results_revealers_after_results() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("notes.txt"), "needle once\n");

    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        panel.imp().runtime.total_matches.get() > 0
    });

    panel.start_search(&search_spec(""));
    flush_events();

    let imp = panel.imp();
    assert!(!imp.results_feedback_revealer.reveals_child());
    assert!(!imp.results_body_revealer.reveals_child());
    assert_eq!(imp.runtime.total_matches.get(), 0);
    assert!(imp.count_label.text().is_empty());
}

#[test]
fn test_search_panel_followup_search_keeps_results_body_open_until_new_outcome() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("notes.txt"), "needle once\n");

    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(2), || {
        panel.imp().runtime.total_matches.get() > 0
    });
    assert!(panel.imp().results_body_revealer.reveals_child());

    panel.start_search(&search_spec("absent"));

    let imp = panel.imp();
    assert!(
        imp.results_feedback_revealer.reveals_child(),
        "follow-up search should preserve the expanded feedback area",
    );
    assert!(
        imp.results_body_revealer.reveals_child(),
        "follow-up search should keep the results body open until the new search resolves",
    );

    wait_until(Duration::from_secs(2), || {
        !panel.imp().runtime.searching.get()
    });

    assert!(imp.results_feedback_revealer.reveals_child());
    assert!(!imp.results_body_revealer.reveals_child());
    assert_eq!(imp.count_label.text().as_str(), "No results found");
}

/// Collect every descendant `GtkLabel` under one widget subtree.
///
/// Realized `GtkListView` rows are built by the search panel's list factory, so
/// the row labels only exist after the compositor has realized the list. The
/// walk is the only way to reach them from a test.
fn collect_row_labels(widget: &gtk4::Widget, out: &mut Vec<gtk4::Label>) {
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(label) = current.downcast_ref::<gtk4::Label>() {
            out.push(label.clone());
        }
        collect_row_labels(&current, out);
        child = current.next_sibling();
    }
}

#[test]
fn test_search_panel_single_result_reports_one_result_in_one_file() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("notes.txt"), "only one needle here\n");

    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(10), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == 1
    });

    let imp = panel.imp();
    assert_eq!(imp.runtime.total_matches.get(), 1);
    assert_eq!(imp.runtime.total_files.get(), 1);
    assert_eq!(imp.count_label.text().as_str(), "1 results in 1 files");
    assert!(!imp.count_label.has_css_class("warning"));
    assert!(imp.results_body_revealer.reveals_child());

    let evidence = panel.evidence();
    assert_eq!(evidence.match_count, 1);
    assert_eq!(evidence.file_count, 1);
    assert!(!evidence.result_capped);
}

#[test]
fn test_search_panel_many_results_group_by_file_and_scroll_within_clamp() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    // Twelve files with two matches each: enough rows to require scrolling in a
    // 240px results body, and enough files to prove grouped counting.
    for index in 0..12 {
        fixture::write_text(
            &dir.path().join(format!("file_{index}.txt")),
            "needle first\nunrelated middle line\nneedle second\n",
        );
    }

    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_files.get() == 12
    });

    let imp = panel.imp();
    assert_eq!(imp.runtime.total_matches.get(), 24);
    assert_eq!(imp.runtime.total_files.get(), 12);
    assert_eq!(imp.count_label.text().as_str(), "24 results in 12 files");
    assert!(!imp.count_label.has_css_class("warning"));
    assert!(imp.results_body_revealer.reveals_child());
    // One grouped root row per file, each owning its own child store.
    assert_eq!(imp.runtime.root_store.borrow().n_items(), 12);
    assert_eq!(imp.runtime.file_groups.borrow().len(), 12);
    // Dense results scroll the item region only; the clamp is unchanged and the
    // panel never offers horizontal scrolling.
    assert_eq!(imp.results_scroll.max_content_height(), 240);
    assert_eq!(
        imp.results_scroll.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );
    // Header/footer chrome stays present rather than being pushed out by rows.
    assert!(imp.search_entry.property::<bool>("visible"));
    assert!(imp.close_button.property::<bool>("visible"));

    let evidence = panel.evidence();
    assert_eq!(evidence.match_count, 24);
    assert_eq!(evidence.file_count, 12);
    assert!(!evidence.result_capped);
}

#[test]
fn test_search_panel_narrow_window_ellipsizes_rows_without_horizontal_scrolling() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let deep = dir
        .path()
        .join("a-very-long-workspace-subdirectory-name")
        .join("another-deeply-nested-directory-level");
    fixture::create_dir_all(&deep);
    fixture::write_text(
        &deep.join("an-extremely-long-file-name-that-cannot-fit-in-a-narrow-panel.txt"),
        "needle followed by a very long line of content that cannot possibly fit inside a narrow search results row without being shortened somehow\n",
    );

    let window = gtk4::Window::builder()
        .default_width(360)
        .default_height(640)
        .build();
    window.set_child(Some(&panel));
    present_window(&window);

    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(15), || {
        panel.imp().runtime.total_matches.get() > 0
    });
    flush_after_delay(Duration::from_millis(120));

    let imp = panel.imp();
    // The narrow window must not be widened by long paths or long match lines.
    assert!(
        window.width() <= 400,
        "long paths must not force the window wider; width was {}",
        window.width(),
    );
    assert_eq!(
        imp.results_scroll.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );
    let hadjustment = imp.results_scroll.hadjustment();
    assert!(
        hadjustment.upper() <= hadjustment.page_size() + 1.0,
        "narrow results must not overflow horizontally: upper {} page {}",
        hadjustment.upper(),
        hadjustment.page_size(),
    );
    // Query and dismissal controls stay allocated and reachable at this width.
    assert!(imp.search_entry.width() > 0);
    assert!(imp.close_button.width() > 0);

    // The realized rows must shorten their own text rather than expand the row.
    let list: gtk4::Widget = imp.results_list.get().upcast();
    let mut labels = Vec::new();
    wait_until(Duration::from_secs(10), || {
        labels.clear();
        collect_row_labels(&list, &mut labels);
        labels
            .iter()
            .any(|label| label.ellipsize() != gtk4::pango::EllipsizeMode::None)
    });
    assert!(
        !labels.is_empty(),
        "expected realized result rows in a presented narrow window",
    );
    let modes: Vec<gtk4::pango::EllipsizeMode> =
        labels.iter().map(gtk4::Label::ellipsize).collect();
    assert!(
        modes.contains(&gtk4::pango::EllipsizeMode::Middle),
        "the file label must ellipsize in the middle so both ends stay readable; modes {modes:?}",
    );
    assert!(
        modes.contains(&gtk4::pango::EllipsizeMode::End),
        "the match-line label must ellipsize at the end; modes {modes:?}",
    );
    for label in &labels {
        assert!(
            label.width() <= window.width(),
            "no row label may be wider than the narrow window",
        );
    }
    assert!(crate::common::assert_monospace_widgets_registered("search results") >= 2);

    panel.close();
    flush_events();
    window.set_child(None::<&gtk4::Widget>);
}

#[test]
fn test_replace_all_then_undo_restores_original_file_bytes() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    let original_first = "alpha needle beta\nplain line\n";
    let original_second = "needle only\n";
    fixture::write_text(&first, original_first);
    fixture::write_text(&second, original_second);

    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));

    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == 2
    });
    assert_eq!(panel.imp().runtime.total_matches.get(), 2);

    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 2 && !evidence.replace_preview_pending
    });
    let previewed = panel.evidence();
    assert_eq!(previewed.replace_preview_count, 2);
    assert_eq!(
        previewed.checked_replacement_count, 2,
        "previewed rows start checked so the confirm step applies exactly them",
    );

    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        fixture::read_text(&first).contains("thread")
            && fixture::read_text(&second).contains("thread")
    });
    fixture::assert_text(&first, "alpha thread beta\nplain line\n");
    fixture::assert_text(&second, "thread only\n");

    wait_until(Duration::from_secs(20), || {
        panel.evidence().has_undo_backup && panel.imp().undo_button.property::<bool>("visible")
    });
    assert!(
        panel.evidence().has_undo_backup,
        "a successful Replace All must publish a journal-backed undo affordance",
    );

    panel.activate_undo_replacements();
    wait_until(Duration::from_secs(20), || {
        fixture::read_text(&first) == original_first
            && fixture::read_text(&second) == original_second
    });
    fixture::assert_text(&first, original_first);
    fixture::assert_text(&second, original_second);

    // A fully consumed journal is cleared, so a second Undo has nothing to hand
    // back and must be refused rather than writing the replacement bytes again.
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        !evidence.has_undo_backup && !evidence.replace_transaction_pending
    });
    assert!(!panel.imp().undo_button.property::<bool>("visible"));
    panel.activate_undo_replacements();
    flush_after_delay(Duration::from_millis(250));
    fixture::assert_text(&first, original_first);
    fixture::assert_text(&second, original_second);
    assert!(
        !panel.evidence().replace_transaction_pending,
        "a refused second undo must not leave the apply transaction claimed",
    );
}

/// Directory the Replace All undo journal is written into.
///
/// Named here rather than imported because the constant is private to the
/// service; the on-disk layout is what these tests are asserting about.
const JOURNAL_DIR: &str = "replace-backup-journal";

/// Per-file journal payload names present on disk, sorted.
///
/// Goes through the `services::filesystem::fixture` boundary rather than raw
/// standard-library calls, per `.agents/rules/rust.md`. Returns empty when the
/// journal directory is absent,
/// which is the state a fully cleaned-up journal leaves behind.
fn journal_payload_files(data_dir: &std::path::Path) -> Vec<String> {
    let journal_dir = data_dir.join(JOURNAL_DIR);
    if !fixture::exists(&journal_dir) {
        return Vec::new();
    }
    let mut names = fixture::entry_names(&journal_dir)
        .into_iter()
        .filter(|name| {
            // The service writes these names itself, so an exact extension match
            // is the contract here, not a user-supplied filename.
            std::path::Path::new(name)
                .extension()
                .is_some_and(|extension| extension == "json")
                && name != "manifest.json"
                && name != "cleanup-in-progress.json"
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn journal_entry_file_for(path: &std::path::Path) -> String {
    format!(
        "{}.json",
        lushtext_model::persistence::sidecar_identity::stable_path_hash(path)
    )
}

/// Run one workspace search plus Replace All over a prepared fixture folder.
///
/// Returns the panel so the caller can keep asserting on it.
fn replace_all_over(
    window: &LushtextWindow,
    dir: &std::path::Path,
    expected_matches: u32,
    replacement: &str,
) -> lushtext_ui::ui::search_panel::LushtextSearchPanel {
    replace_all_over_with(
        window,
        dir,
        ReplaceAllFixture {
            search_matches: expected_matches,
            preview_rows: expected_matches,
            budget: Duration::from_secs(20),
        },
        replacement,
        || {},
    )
}

/// The counts one fixture's search and preview reach, and how long its run
/// may take.
#[derive(Clone, Copy)]
struct ReplaceAllFixture {
    search_matches: u32,
    preview_rows: u32,
    budget: Duration,
}

/// [`replace_all_over`] with `before_confirm` run between the preview and the
/// confirmation: a fixture the search and preview must see as valid UTF-8
/// becomes invalid only there, because the content search reads UTF-8 lines
/// and reports no match in a file that is not.
fn replace_all_over_with(
    window: &LushtextWindow,
    dir: &std::path::Path,
    plan: ReplaceAllFixture,
    replacement: &str,
    before_confirm: impl FnOnce(),
) -> lushtext_ui::ui::search_panel::LushtextSearchPanel {
    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(plan.budget, || {
        !panel.imp().runtime.searching.get()
            && panel.imp().runtime.total_matches.get() == plan.search_matches
    });

    panel.enter_preview_mode(replacement);
    wait_until(plan.budget, || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == plan.preview_rows && !evidence.replace_preview_pending
    });
    before_confirm();
    panel.activate_confirm_replacements();
    wait_until(plan.budget, || {
        panel.evidence().last_apply_counts.is_some() && !panel.evidence().replace_preview_pending
    });
    panel
}

#[test]
fn test_partial_undo_shrinks_the_journal_and_keeps_the_skipped_entry_on_disk() {
    // The sole production caller of `search_backup::shrink_journal_to`: a partial
    // undo whose skipped file still holds Replace All output. That file's durable
    // rollback copy must survive the shrink, and the journal must stay active, or
    // the user loses the only way back to their original content.
    ensure_gtk_init();
    let data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let restored = dir.path().join("restored.txt");
    let skipped = dir.path().join("skipped.txt");
    let original_restored = "alpha needle beta\n";
    fixture::write_text(&restored, original_restored);
    fixture::write_text(&skipped, "needle only\n");

    let panel = replace_all_over(&window, dir.path(), 2, "thread");
    wait_until(Duration::from_secs(20), || {
        panel.evidence().undo_backup_entry_count == 2
    });
    assert_eq!(
        journal_payload_files(&data_dir).len(),
        2,
        "a two-file Replace All journals both files",
    );

    // Change one target after the replacement, so undo must refuse to restore it
    // and keep it in the remaining journal instead.
    fixture::write_text(&skipped, "the user edited this after Replace All\n");

    panel.activate_undo_replacements();
    wait_until(Duration::from_secs(20), || {
        fixture::read_text(&restored) == original_restored
            && panel.evidence().undo_backup_entry_count == 1
    });
    wait_for_journal_disk_idle(&panel);

    fixture::assert_text(&restored, original_restored);
    fixture::assert_text(&skipped, "the user edited this after Replace All\n");

    // The journal must still be *active* on disk, holding exactly the skipped
    // file. `load` returns an empty backup for an inactive journal, so a
    // non-empty result is itself the activation assertion.
    let on_disk = search_backup::load(&data_dir).expect("load journal after partial undo");
    assert_eq!(
        on_disk.len(),
        1,
        "the shrunken journal stays active with only the unrestored file",
    );
    let (journalled_path, _) = on_disk
        .iter()
        .next()
        .expect("the shrunken journal has one entry");
    assert_eq!(
        journalled_path.file_name(),
        Some(std::ffi::OsStr::new("skipped.txt")),
    );

    let payloads = journal_payload_files(&data_dir);
    assert_eq!(
        payloads,
        vec![journal_entry_file_for(journalled_path)],
        "exactly the skipped file's entry survives; the restored file's is gone",
    );
    assert!(
        !payloads.contains(&journal_entry_file_for(&restored)),
        "the restored file's journal entry must not linger",
    );
    drop(data);
}

#[test]
fn test_replace_all_with_no_matches_writes_nothing_and_journals_nothing() {
    ensure_gtk_init();
    let data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let only = dir.path().join("only.txt");
    let original = "nothing to see here\n";
    fixture::write_text(&only, original);

    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get()
    });
    assert_eq!(panel.evidence().match_count, 0);

    panel.enter_preview_mode("thread");
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(400));

    fixture::assert_text(&only, original);
    assert!(
        !panel.evidence().has_undo_backup,
        "nothing was written, so no undo affordance may appear",
    );
    assert!(
        search_backup::load(&data_dir)
            .expect("load journal after a no-match Replace All")
            .is_empty(),
        "a Replace All that wrote nothing must journal nothing",
    );
    assert!(journal_payload_files(&data_dir).is_empty());
    drop(data);
}

#[test]
fn test_replace_all_one_match_one_file_writes_and_journals_exactly_that_file() {
    ensure_gtk_init();
    let data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let target = dir.path().join("target.txt");
    let bystander = dir.path().join("bystander.txt");
    let original_bystander = "unrelated content\n";
    fixture::write_text(&target, "alpha needle beta\n");
    fixture::write_text(&bystander, original_bystander);

    let panel = replace_all_over(&window, dir.path(), 1, "thread");

    fixture::assert_text(&target, "alpha thread beta\n");
    fixture::assert_text(&bystander, original_bystander);
    let counts = panel
        .evidence()
        .last_apply_counts
        .expect("apply counts recorded");
    assert_eq!(counts.replaced, 1);
    assert_eq!(counts.errors, 0);

    wait_until(Duration::from_secs(20), || {
        panel.evidence().undo_backup_entry_count == 1
    });
    wait_for_journal_disk_idle(&panel);
    let on_disk = search_backup::load(&data_dir).expect("load one-file journal");
    assert_eq!(on_disk.len(), 1);
    assert_eq!(
        on_disk
            .keys()
            .next()
            .expect("one journalled file")
            .file_name(),
        Some(std::ffi::OsStr::new("target.txt")),
    );
    assert_eq!(journal_payload_files(&data_dir).len(), 1);
    drop(data);
}

#[test]
fn test_replace_all_many_matches_many_files_writes_and_journals_every_file() {
    ensure_gtk_init();
    let data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    const FILES: u32 = 5;
    const MATCHES_PER_FILE: u32 = 2;
    for index in 0..FILES {
        fixture::write_text(
            &dir.path().join(format!("file_{index}.txt")),
            "needle one\nplain\nneedle two\n",
        );
    }

    let panel = replace_all_over(&window, dir.path(), FILES * MATCHES_PER_FILE, "thread");

    for index in 0..FILES {
        fixture::assert_text(
            &dir.path().join(format!("file_{index}.txt")),
            "thread one\nplain\nthread two\n",
        );
    }
    let counts = panel
        .evidence()
        .last_apply_counts
        .expect("apply counts recorded");
    assert_eq!(counts.replaced, FILES * MATCHES_PER_FILE);
    assert_eq!(counts.errors, 0);

    wait_until(Duration::from_secs(30), || {
        panel.evidence().undo_backup_entry_count == FILES as usize
    });
    wait_for_journal_disk_idle(&panel);
    let on_disk = search_backup::load(&data_dir).expect("load many-file journal");
    assert_eq!(
        on_disk.len(),
        FILES as usize,
        "every written file must have a durable rollback entry",
    );
    assert_eq!(journal_payload_files(&data_dir).len(), FILES as usize);
    drop(data);
}

#[test]
fn test_replace_all_applies_only_checked_rows_and_records_apply_counts() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    let original_first = "alpha needle beta\n";
    let original_second = "needle only\n";
    fixture::write_text(&first, original_first);
    fixture::write_text(&second, original_second);

    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == 2
    });

    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 2 && !evidence.replace_preview_pending
    });

    // Uncheck the row belonging to the second file, so only the first is written.
    let unchecked = panel
        .imp()
        .preview
        .preview_outcome
        .borrow()
        .as_ref()
        .expect("preview outcome")
        .replacements
        .iter()
        .find(|replacement| replacement.path == second)
        .expect("a preview row for the second file")
        .match_id;
    panel
        .imp()
        .preview
        .checked_match_ids
        .borrow_mut()
        .remove(&unchecked);
    assert_eq!(panel.evidence().checked_replacement_count, 1);

    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        fixture::read_text(&first).contains("thread")
    });
    fixture::assert_text(&first, "alpha thread beta\n");
    fixture::assert_text(&second, original_second);

    wait_until(Duration::from_secs(20), || {
        panel.evidence().last_apply_counts.is_some()
    });
    let evidence = panel.evidence();
    let counts = evidence.last_apply_counts.expect("apply counts recorded");
    assert_eq!(counts.replaced, 1);
    assert_eq!(counts.errors, 0);
    assert_eq!(
        evidence.undo_backup_entry_count, 1,
        "only the written file may enter the undo journal",
    );
    assert!(
        evidence.undo_backup_retained_bytes > 0,
        "an installed journal retains disposal weight",
    );
}

#[test]
fn test_evidence_reports_transaction_state_separately_from_preview_pending() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();

    let idle = panel.evidence();
    assert!(!idle.replace_transaction_pending);
    assert!(!idle.replace_preview_pending);
    assert_eq!(idle.replace_transaction_generation, None);

    let reserved = panel
        .begin_replace_transaction_for_test()
        .expect("an idle panel grants the transaction");
    let claimed = panel.evidence();
    assert!(
        claimed.replace_transaction_pending,
        "the apply transaction must be observable on its own field",
    );
    assert_eq!(claimed.replace_transaction_generation, Some(reserved));
    assert_eq!(
        claimed.undo_backup_generation, reserved,
        "claiming the transaction reserves the journal generation it reports",
    );
    assert!(
        claimed.replace_preview_pending,
        "the folded field still reports the transaction, which is why the \
         separate field is needed to tell the two apart",
    );

    assert!(
        panel.begin_replace_transaction_for_test().is_none(),
        "the transaction is single-owner",
    );

    panel.finish_replace_transaction_for_test();
    let released = panel.evidence();
    assert!(!released.replace_transaction_pending);
    assert_eq!(released.replace_transaction_generation, None);
    assert!(!released.replace_preview_pending);
}

#[test]
fn test_evidence_reads_stay_side_effect_free_across_journal_mutation() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    // Let the startup journal load finish before the run's journal exists.
    wait_for_journal_disk_idle(&panel);
    let backup = sample_replace_backup("/evidence-reentrancy.rs");

    // Read at every point the workflow mutates journal state, including
    // immediately after operations that take `borrow_mut()` on the queued
    // preview request and the checked-identity set. A live path holding one of
    // those borrows across an evidence read would panic here.
    let before = panel.evidence();
    panel.set_query("needle");
    let _ = panel.evidence();
    panel.enter_preview_mode("thread");
    let _ = panel.evidence();
    panel.exit_preview_mode();
    let _ = panel.evidence();
    panel.set_undo_backup(backup);
    let installed = panel.evidence();
    assert!(installed.has_undo_backup);
    assert_eq!(installed.undo_backup_entry_count, 1);
    let _ = panel.evidence();
    panel.clear_undo_backup_for_test();
    let cleared = panel.evidence();
    assert!(!cleared.has_undo_backup);
    assert_eq!(cleared.undo_backup_entry_count, 0);
    assert_eq!(cleared.undo_backup_retained_bytes, 0);

    // Repeated reads of the same state must be identical: reading evidence must
    // not advance a generation, drain a queue, or charge a job.
    let first_read = panel.evidence();
    let second_read = panel.evidence();
    assert_eq!(
        first_read.undo_backup_generation,
        second_read.undo_backup_generation
    );
    assert_eq!(
        first_read.preview_generation,
        second_read.preview_generation
    );
    assert_eq!(first_read.journal_disk_jobs, second_read.journal_disk_jobs);
    assert_eq!(
        first_read.journal_disk_jobs_in_flight,
        second_read.journal_disk_jobs_in_flight
    );
    assert!(
        second_read.undo_backup_generation > before.undo_backup_generation,
        "the journal generation advanced from real workflow steps, not from reads",
    );

    wait_for_journal_disk_idle(&panel);
    assert!(
        panel.evidence().journal_disk_jobs >= 2,
        "one install and one clear each dispatch a journal disk job",
    );

    // The parked Replace All run (`replace-all-confirm-disposal-lane` task
    // 2.9a): read around each operation that takes a mutable borrow of
    // `parked_replace_run` (defer, resume, cancel), and check two reads of
    // the same state agree.
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let assert_reads_agree = |panel: &LushtextSearchPanel| {
        let first = panel.evidence();
        let second = panel.evidence();
        assert_eq!(
            first.replace_admission_deferred,
            second.replace_admission_deferred
        );
        assert_eq!(first.replace_in_flight, second.replace_in_flight);
        assert_eq!(
            first.replace_transaction_pending,
            second.replace_transaction_pending
        );
        assert_eq!(first.undo_backup_generation, second.undo_backup_generation);
        assert_eq!(first.preview_generation, second.preview_generation);
        assert_eq!(
            first.preview_retirement_jobs,
            second.preview_retirement_jobs
        );
        first
    };
    // Defer, then resume.
    let (window_panel, files) = preview_named_files(&window, dir.path(), "resumed");
    wait_for_journal_disk_idle(&window_panel);
    let fill = fill_lane_after_settling(&window);
    window_panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(10), || {
        window_panel.evidence().replace_admission_deferred
    });
    assert!(assert_reads_agree(&window_panel).replace_admission_deferred);
    drop(fill);
    wait_until(Duration::from_secs(20), || {
        !window_panel.evidence().replace_admission_deferred
    });
    let resumed = assert_reads_agree(&window_panel);
    assert!(!resumed.replace_admission_deferred);
    wait_until(Duration::from_secs(20), || {
        all_read(&files, "thread\n") && !window_panel.evidence().replace_transaction_pending
    });
    wait_for_journal_disk_idle(&window_panel);
    // Defer, then cancel.
    let (_, files) = preview_named_files(&window, dir.path(), "cancelled");
    let fill = fill_lane_after_settling(&window);
    window_panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(10), || {
        window_panel.evidence().replace_admission_deferred
    });
    assert!(assert_reads_agree(&window_panel).replace_admission_deferred);
    window_panel.imp().replace_all_button.get().emit_clicked();
    let cancelled = assert_reads_agree(&window_panel);
    assert!(!cancelled.replace_admission_deferred && !cancelled.replace_in_flight);
    drop(fill);
    assert_texts(&files, "needle\n");
}

/// Write a workspace of 18,000 matches (30 files of 600 lines).
fn write_over_cap_fixture(root: &std::path::Path) {
    for index in 0..30 {
        fixture::write_text(
            &root.join(format!("big_{index}.txt")),
            &"needle\n".repeat(600),
        );
    }
}

/// A search budget small enough for the 18,000-match fixture to cross it:
/// about 2,000 matches of the fixture's 6-byte lines.
const TEST_SEARCH_BUDGET: u64 = 4 * 1024 * 1024;

/// Run `test` with the search budget lowered to `bytes`, restoring production.
fn with_search_budget(bytes: u64, test: impl FnOnce()) {
    SearchPanelTestPolicy::default()
        .with_search_retained_bytes(bytes)
        .install();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
    SearchPanelTestPolicy::reset();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn test_search_panel_capped_results_warn_and_keep_every_delivered_match() {
    ensure_gtk_init();
    with_search_budget(TEST_SEARCH_BUDGET, || {
        let panel = glib::Object::builder::<LushtextSearchPanel>().build();
        let dir = tempfile::tempdir().expect("expected operation to succeed");
        write_over_cap_fixture(dir.path());

        panel.clamp_results_height(240);
        panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
        panel.start_search(&search_spec("needle"));

        let mut found_seen = Vec::new();
        wait_until(Duration::from_secs(60), || {
            let evidence = panel.evidence();
            if found_seen.last() != Some(&evidence.matches_found) {
                found_seen.push(evidence.matches_found);
            }
            !evidence.searching && evidence.result_capped
        });

        let imp = panel.imp();
        assert!(
            imp.runtime.result_capped.get(),
            "reaching the retained-byte budget must set the limited state, not stop silently",
        );
        let total = imp.runtime.total_matches.get();
        let files = imp.runtime.total_files.get();
        let evidence = panel.evidence();
        assert_eq!(
            evidence.matches_found, 18_000,
            "the walk counted every match"
        );
        assert!(
            total < 18_000 && total > 0,
            "the budget cut the search: {total}"
        );
        assert_eq!(
            imp.count_label.text().as_str(),
            format!(
                "Showing {} matches in {files} files; {} more not shown \u{2014} refine the search to see them",
                group_thousands(total as usize),
                group_thousands(18_000 - total as usize)
            ),
        );
        assert!(
            imp.count_label.has_css_class("warning"),
            "a limited result set must be styled as a warning, not as a plain count",
        );
        assert!(imp.results_body_revealer.reveals_child());
        // The retained matches are a gapless prefix: ids 0..total in order.
        let accepted = imp.runtime.accepted_matches.borrow().clone();
        let accepted = accepted.expect("a limited search is a completed search");
        assert_eq!(accepted.len(), total as usize);
        assert!(
            accepted
                .iter()
                .enumerate()
                .all(|(index, search_match)| search_match.id.index() == index)
        );
        assert_eq!(
            imp.navigation.match_positions.borrow().len(),
            total as usize
        );
        assert!(imp.save_button.property::<bool>("visible"));
        assert!(
            found_seen.windows(2).all(|pair| pair[0] <= pair[1]),
            "the found count only grows: {found_seen:?}"
        );
        assert!(evidence.result_capped);
        assert_eq!(evidence.match_count, total);
        assert_eq!(evidence.retained_matches, total);
        assert!(evidence.file_count > 0);
    });
}

/// `count` with comma thousands separators, as the panel prints counts.
fn group_thousands(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// No count cap (`bound-lists-by-budget-not-count`): 40,000 ordinary matches
/// in 2,000 files are all shown and navigable, with no limited state, and
/// Replace Preview lists its own 16,384-row budget and states the rest.
#[test]
fn test_search_past_mains_count_cap_shows_every_match() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(700)
        .child(&panel)
        .build();
    present_window(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    for index in 0..2_000 {
        fixture::write_text(
            &dir.path().join(format!("file_{index:04}.txt")),
            &"let needle = value;\n".repeat(20),
        );
    }
    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(120), || !panel.evidence().searching);

    let evidence = panel.evidence();
    assert_eq!(evidence.match_count, 40_000, "every match is shown");
    assert_eq!(evidence.file_count, 2_000);
    assert_eq!(evidence.matches_found, 40_000);
    assert!(!evidence.result_capped, "no limited state below the budget");
    assert_eq!(evidence.navigation_match_count, 40_000);
    assert_eq!(
        panel.imp().count_label.text().as_str(),
        "40000 results in 2000 files"
    );
    // F4 wraps through the whole list: Shift+F4 from the start lands on the
    // last match, which is the 40,000th.
    panel.navigate_prev_match();
    assert_eq!(
        panel.imp().navigation.current_match_index.get(),
        Some(39_999)
    );

    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(120), || {
        panel.is_preview_mode() && !panel.imp().preview.preview_pending.get()
    });
    let evidence = panel.evidence();
    assert_eq!(evidence.replace_preview_count, 16 * 1024);
    assert_eq!(evidence.omitted_replacement_count, 23_616);
    let summary = panel.imp().count_label.text();
    assert!(
        summary.contains("23616 omitted") && !summary.contains("search limit"),
        "the preview states its own omitted count and no search-limit clause: {summary}"
    );
    drop(window);
}

/// `main`'s worst case under the 16,384-match cap (every match a 4,096-byte
/// line in its own file under a path near 4,096 bytes) is retained in full.
#[test]
fn test_search_keeps_mains_worst_case_in_full() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    // Nest 250-byte directory names until the file paths approach PATH_MAX.
    let mut deep = dir.path().to_path_buf();
    while deep.as_os_str().len() < 3_700 {
        deep = deep.join("d".repeat(250));
    }
    fixture::create_dir_all(&deep);
    let line = format!("needle{}\n", "x".repeat(4_090));
    for index in 0..16 * 1024 {
        fixture::write_text(&deep.join(format!("f{index:05}.txt")), &line);
    }
    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(240), || !panel.evidence().searching);
    let evidence = panel.evidence();
    assert_eq!(
        evidence.match_count,
        16 * 1024,
        "main's worst case is kept in full"
    );
    assert!(!evidence.result_capped);
}

/// A turn's matches reach the model as one splice per touched file plus at
/// most one root splice, and the reveal turn takes at most one small batch.
#[test]
fn test_search_turn_applies_matches_as_one_splice_per_file() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    for index in 0..4 {
        fixture::write_text(
            &dir.path().join(format!("dense_{index}.txt")),
            &"needle\n".repeat(2_000),
        );
    }
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    let mut largest_batch = (0, 0, 0);
    wait_until(Duration::from_secs(60), || {
        let evidence = panel.evidence();
        if evidence.last_match_batch.0 > largest_batch.0 {
            largest_batch = evidence.last_match_batch;
        }
        !evidence.searching
    });
    let evidence = panel.evidence();
    assert_eq!(evidence.match_count, 8_000);
    let (matches, child_splices, root_splices) = largest_batch;
    assert!(
        matches > 4,
        "a turn applied several matches: {largest_batch:?}"
    );
    assert!(
        (1..=4).contains(&child_splices),
        "one splice per touched file, not per match: {largest_batch:?}"
    );
    assert!(root_splices <= 1);
    assert!(
        evidence.reveal_turn_events >= 1 && evidence.reveal_turn_events <= 32,
        "the reveal turn carries one small batch: {}",
        evidence.reveal_turn_events
    );
}

/// Cancelling while the worker counts past the budget stops the counting:
/// no later count reaches the panel.
#[test]
fn test_cancelling_a_limited_search_stops_its_count() {
    ensure_gtk_init();
    with_search_budget(64 * 1024, || {
        let panel = glib::Object::builder::<LushtextSearchPanel>().build();
        let dir = tempfile::tempdir().expect("expected operation to succeed");
        for index in 0..400 {
            fixture::write_text(
                &dir.path().join(format!("file_{index:03}.txt")),
                &"needle\n".repeat(500),
            );
        }
        panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
        panel.start_search(&search_spec("needle"));
        wait_until(Duration::from_secs(60), || panel.evidence().result_capped);
        panel.start_search(&search_spec(""));
        wait_until(Duration::from_secs(30), || !panel.evidence().searching);
        let after_cancel = panel.evidence();
        flush_after_delay(Duration::from_millis(600));
        let later = panel.evidence();
        assert_eq!(later.matches_found, after_cancel.matches_found);
        assert_eq!(
            later.match_count, 0,
            "the cancelled flight's results are gone"
        );
        assert!(!later.result_capped);
    });
}

/// Replace Preview after a limited search lists only retained matches, states
/// the excluded count, and replaces only checked rows; after an unlimited
/// search it carries no clause (checked by the 40,000-match test above).
#[test]
fn test_preview_after_a_limited_search_states_the_search_limit() {
    ensure_gtk_init();
    with_search_budget(TEST_SEARCH_BUDGET, || {
        let panel = glib::Object::builder::<LushtextSearchPanel>().build();
        let dir = tempfile::tempdir().expect("expected operation to succeed");
        write_over_cap_fixture(dir.path());
        panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
        panel.start_search(&search_spec("needle"));
        wait_until(Duration::from_secs(60), || {
            let evidence = panel.evidence();
            !evidence.searching && evidence.result_capped
        });
        let retained = panel.evidence().match_count as usize;
        panel.enter_preview_mode("thread");
        wait_until(Duration::from_secs(60), || {
            panel.is_preview_mode() && !panel.imp().preview.preview_pending.get()
        });
        let evidence = panel.evidence();
        assert_eq!(evidence.replace_preview_count as usize, retained);
        let summary = panel.imp().count_label.text();
        assert!(
            summary.ends_with(&format!(
                "; {} matches past the search limit are not included",
                group_thousands(18_000 - retained)
            )),
            "{summary}"
        );
    });
}

#[test]
fn test_search_panel_superseding_a_capped_search_discards_it_immediately() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    write_over_cap_fixture(dir.path());
    fixture::write_text(&dir.path().join("distinct.txt"), "one solitary marker\n");

    panel.clamp_results_height(240);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.set_query("needle");
    let _ = panel.imp().runtime.search_debounce.invalidate();
    panel.start_search(&search_spec("needle"));

    // Supersede while the capped search is still streaming into the panel.
    wait_until(Duration::from_secs(30), || {
        let imp = panel.imp();
        imp.runtime.searching.get() && imp.runtime.total_matches.get() > 250
    });
    assert!(
        panel.imp().runtime.searching.get(),
        "the fixture must still be streaming when the newer query arrives",
    );
    // The retained latest request is only admitted while it still matches the
    // visible query, so the entry must move with the superseding spec.
    panel.set_query("marker");
    let _ = panel.imp().runtime.search_debounce.invalidate();
    panel.start_search(&search_spec("marker"));

    wait_until(Duration::from_secs(60), || {
        let imp = panel.imp();
        !imp.runtime.searching.get() && imp.runtime.total_matches.get() == 1
    });

    let imp = panel.imp();
    // A budget cut stops nothing; a user supersede still discards
    // everything the old flight had buffered.
    assert_eq!(imp.runtime.total_matches.get(), 1);
    assert_eq!(imp.runtime.total_files.get(), 1);
    assert_eq!(imp.count_label.text().as_str(), "1 results in 1 files");
    assert!(!imp.runtime.result_capped.get());
    assert!(!imp.count_label.has_css_class("warning"));
    assert!(!panel.evidence().result_capped);
}

#[test]
fn test_evidence_reads_survive_widget_disposal() {
    // A disposed widget is a legitimate observation point: a teardown test asks
    // what the workflow recorded after the panel goes away. GTK4 clears template
    // children in `dispose()` before Rust's `Drop`, so every evidence field
    // derived from one must answer honestly instead of panicking.
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    panel.set_query("needle");
    panel.imp().regex_toggle.set_active(true);

    let before = panel.evidence();
    assert_eq!(before.query, "needle");
    assert!(before.regex_enabled);

    // SAFETY: this standalone test panel is disposed exactly once, and the
    // assertions afterwards only read the evidence surface.
    unsafe { panel.run_dispose() };

    let after = panel.evidence();
    assert_eq!(after.query, "");
    assert!(!after.case_sensitive);
    assert!(!after.regex_enabled);
    assert!(!after.whole_word_enabled);
    assert!(!after.gitignore_enabled);
    assert_eq!(after.glob_filter, None);
    assert_eq!(after.replace_query, "");

    // Repeated reads after disposal stay identical and still do not panic.
    let second = panel.evidence();
    assert_eq!(after.query, second.query);
    assert_eq!(after.glob_filter, second.glob_filter);
}

/// `power-of-two-thresholds` task 5.4: one receive turn dispatches at most
/// `MAX_SEARCH_EVENTS_PER_TICK` (256) `Match` events, and one retirement turn
/// releases at most `SEARCH_RETIREMENT_ROWS_PER_SLICE` (256) references. The
/// evidence times both full turns; the measurement reads the printed line.
#[test]
fn test_search_turns_dispatch_and_retire_at_most_256() {
    ensure_gtk_init();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("many.txt"), &"needle\n".repeat(3_000));
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(60), || {
        !panel.imp().runtime.searching.get()
    });

    let turns = panel.evidence().event_turn_observations;
    let largest = turns.iter().map(|&(events, _)| events).max().unwrap_or(0);
    assert_eq!(
        largest, 256,
        "a full receive turn dispatches 256 events: {turns:?}"
    );
    let full_turns: Vec<i64> = turns
        .iter()
        .filter(|&&(events, _)| events == 256)
        .map(|&(_, us)| us)
        .collect();

    populate_large_result_generation(&panel, "/retired-generation.txt", 3_000);
    panel.start_search(&search_spec(""));
    wait_until(Duration::from_secs(10), || {
        let evidence = panel.evidence();
        !panel.is_searching() && !evidence.retirement_pending
    });
    let evidence = panel.evidence();
    assert_eq!(evidence.retirement_rows_per_slice_high_water, 256);
    let full_slices: Vec<i64> = evidence
        .retirement_observations
        .iter()
        .filter(|observation| observation.charged == 256)
        .map(|observation| observation.duration_us)
        .collect();
    assert!(!full_slices.is_empty());
    eprintln!(
        "search-turn-timing-evidence full_event_turns={} event_turn_max_us={} event_turn_median_us={} full_retirement_slices={} retirement_slice_max_us={} retirement_slice_median_us={}",
        full_turns.len(),
        full_turns.iter().max().copied().unwrap_or(0),
        median(&full_turns),
        full_slices.len(),
        full_slices.iter().max().copied().unwrap_or(0),
        median(&full_slices),
    );
}

fn median(values: &[i64]) -> i64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied().unwrap_or(0)
}

/// `bound-lists-by-budget-not-count` tasks 8.1–8.2: the search past the
/// former cap (30 files of 600 matches, the PT M2/M3 fixture), its Replace
/// Preview entry, and its retirement, presented: every dispatch fits
/// `B_SETTLE_US` on the optimized widget build (printed on every build).
#[test]
fn test_search_and_preview_dispatches_fit_the_settle_budget() {
    use crate::common::{timing_verdicts_enforced, worst_dispatch_until};
    use lushtext_model::budgets::responsiveness_budget::B_SETTLE_US;
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    write_over_cap_fixture(dir.path());
    for rep in 0..3 {
        let panel = glib::Object::builder::<LushtextSearchPanel>().build();
        let window = gtk4::Window::builder()
            .default_width(1000)
            .default_height(800)
            .child(&panel)
            .build();
        present_window(&window);
        panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
        panel.start_search(&search_spec("needle"));
        let (held, search_worst) =
            worst_dispatch_until(Duration::from_secs(120), || !panel.evidence().searching);
        assert!(held, "the search settles");
        let evidence = panel.evidence();
        assert_eq!(evidence.match_count, 18_000);
        panel.enter_preview_mode("thread");
        let (held, preview_worst) = worst_dispatch_until(Duration::from_secs(120), || {
            panel.is_preview_mode() && !panel.imp().preview.preview_pending.get()
        });
        assert!(held, "the preview settles");
        panel.start_search(&search_spec(""));
        let (held, retire_worst) = worst_dispatch_until(Duration::from_secs(120), || {
            !panel.is_searching() && !panel.evidence().retirement_pending
        });
        assert!(held, "the retirement settles");
        let worst_turn = evidence
            .event_turn_observations
            .iter()
            .map(|(_, us)| *us)
            .max()
            .unwrap_or(0);
        println!(
            "list-budget-evidence {{\"m\":\"search\",\"rep\":{rep},\"matches\":{},\"reveal_turn_events\":{},\"worst_turn_us\":{worst_turn},\"search_worst_us\":{},\"preview_worst_us\":{},\"retire_worst_us\":{}}}",
            evidence.match_count,
            evidence.reveal_turn_events,
            search_worst.as_micros(),
            preview_worst.as_micros(),
            retire_worst.as_micros(),
        );
        if timing_verdicts_enforced() {
            for (phase, worst) in [
                ("search", search_worst),
                ("preview", preview_worst),
                ("retirement", retire_worst),
            ] {
                assert!(
                    worst.as_micros() <= u128::from(B_SETTLE_US.as_us()),
                    "{phase} dispatch {} us exceeds B_SETTLE_US",
                    worst.as_micros()
                );
            }
        }
        drop(window);
    }
}

/// `bound-lists-by-budget-not-count` tasks 2.4 and 8.3: what 16,384 retained
/// matches (120-byte lines, 20 per file) really cost, presented, against what
/// the budget charges them: resident high-water growth at most 5/4 of the
/// charge. Prints the per-match growth the fixed charge is calibrated from.
#[test]
fn test_search_retention_stays_within_its_charge() {
    use crate::common::{proc_status_kib, reset_vm_hwm};
    use lushtext_ui::ui::search_panel::policy::SEARCH_MATCH_FIXED_CHARGE;
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let line = format!("needle {}\n", "x".repeat(112));
    let mut path_bytes = 0u64;
    for index in 0..820 {
        let path = dir.path().join(format!("file_{index:04}.txt"));
        path_bytes += path.as_os_str().len() as u64;
        fixture::write_text(&path, &line.repeat(20));
    }
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let window = gtk4::Window::builder()
        .default_width(1000)
        .default_height(800)
        .child(&panel)
        .build();
    present_window(&window);
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    flush_after_delay(Duration::from_millis(200));
    reset_vm_hwm();
    let before_kib = proc_status_kib("VmRSS:");
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(120), || !panel.evidence().searching);
    flush_after_delay(Duration::from_millis(300));
    let growth = proc_status_kib("VmHWM:").saturating_sub(before_kib) * 1024;
    let matches = u64::from(panel.evidence().match_count);
    assert_eq!(matches, 16_400);
    let charged = matches * (119 + SEARCH_MATCH_FIXED_CHARGE) + 2 * path_bytes;
    println!(
        "list-budget-evidence {{\"m\":\"search-memory\",\"matches\":{matches},\"growth_bytes\":{growth},\"charged_bytes\":{charged},\"growth_per_match\":{}}}",
        growth / matches
    );
    assert!(
        growth * 4 <= charged * 5,
        "16,400 matches grew the resident set by {growth} bytes, over 5/4 of the {charged} bytes charged"
    );
    drop(window);
}

// ---------------------------------------------------------------------------
// stream-replace-all: skip report, Skipped Files dialog and cancellation
// ---------------------------------------------------------------------------

/// The count of `reason` in the panel's last skip report.
fn skipped_for(
    panel: &LushtextSearchPanel,
    reason: lushtext_model::workspace::content_search::ReplaceSkipReason,
) -> usize {
    panel.evidence().replace_skip_counts[reason.index()]
}

/// The list view of the presented Skipped Files dialog.
fn skipped_files_list(window: &LushtextWindow) -> gtk4::ListView {
    let dialog = window
        .visible_dialog()
        .expect("the Skipped Files dialog is presented");
    crate::common::find_descendant(
        dialog.upcast_ref(),
        glib::prelude::ObjectExt::is::<gtk4::ListView>,
    )
    .and_downcast::<gtk4::ListView>()
    .expect("the Skipped Files dialog lists its files")
}

/// stream-replace-all 5.5: a run that skips a file ends with "· N skipped",
/// offers Show Skipped Files, and the dialog opens the file from its row.
/// Keyboard path: the button takes focus, the dialog's list takes it when it
/// opens, a row activation opens the file, and closing returns focus.
#[test]
fn test_replace_all_with_a_skipped_file_offers_show_skipped_files() {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    // The panel is shown, as it is when a user runs Replace All, so its
    // controls can take focus.
    gtk4::gio::prelude::ActionGroupExt::activate_action(
        &window,
        "set-search-panel-visible",
        Some(&true.to_variant()),
    );
    wait_until(Duration::from_secs(5), || {
        window.imp().search_panel.get().is_visible()
    });

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let good = dir.path().join("a-good.txt");
    let latin1 = dir.path().join("b-latin1.txt");
    fixture::write_text(&good, "needle\n");
    fixture::write_text(&latin1, "needle\nplain\n");

    let panel = replace_all_over_with(
        &window,
        dir.path(),
        ReplaceAllFixture {
            search_matches: 2,
            preview_rows: 2,
            budget: Duration::from_secs(20),
        },
        "thread",
        || fixture::write_bytes(&latin1, b"needle\ncaf\xe9\n"),
    );
    wait_until(Duration::from_secs(10), || {
        panel.evidence().skipped_files_offered
    });

    fixture::assert_text(&good, "thread\n");
    assert_eq!(fixture::read_bytes(&latin1), b"needle\ncaf\xe9\n");
    let evidence = panel.evidence();
    assert!(!evidence.replace_in_flight);
    assert_eq!(skipped_for(&panel, ReplaceSkipReason::NotUtf8), 1);
    let button = panel.imp().skipped_files_button.get();
    assert!(button.property::<bool>("visible"));
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Replace All")
    );

    button.grab_focus();
    flush_events();
    button.emit_clicked();
    flush_events();
    let list = skipped_files_list(&window);
    wait_until(Duration::from_secs(5), || {
        gtk4::prelude::RootExt::focus(&window).is_some_and(|focus| {
            focus.as_ptr() == list.upcast_ref::<gtk4::Widget>().as_ptr() || focus.is_ancestor(&list)
        })
    });
    let requests = panel
        .evidence()
        .skipped_files_item_requests
        .expect("the dialog's model is alive while it is shown");
    assert!(requests >= 1, "the realized row was built");

    let dialog = window.visible_dialog().expect("dialog");
    dialog.close();
    flush_after_delay(Duration::from_millis(100));
    assert!(window.visible_dialog().is_none());
    assert!(
        gtk4::prelude::RootExt::focus(&window)
            .is_some_and(|focus| focus.as_ptr() == button.upcast_ref::<gtk4::Widget>().as_ptr()),
        "closing the dialog returns focus to Show Skipped Files"
    );

    // Opening it again and activating the row opens the skipped file.
    activate_action(&window, "show-search-panel-skipped-files");
    let list = skipped_files_list(&window);
    list.emit_by_name::<()>("activate", &[&0u32]);
    wait_until(Duration::from_secs(10), || {
        window.visible_dialog().is_none()
            && window
                .imp()
                .tab_view
                .selected_page()
                .and_then(|page| {
                    page.child()
                        .downcast::<lushtext_ui::ui::editor_page::LushtextEditorPage>()
                        .ok()
                })
                .and_then(|editor| editor.file_path())
                .is_some_and(|path| path == latin1)
    });
}

/// stream-replace-all 5.5: the dialog's model is lazy. At 16,384 skipped
/// files it builds only the rows GTK realizes.
#[test]
fn test_skipped_files_dialog_builds_only_realized_rows() {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    const FILES: usize = 16 * 1024;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let files: Vec<_> = (0..FILES)
        .map(|index| dir.path().join(format!("f{index:05}.txt")))
        .collect();
    for file in &files {
        fixture::write_text(file, "needle\nplain\n");
    }
    let panel = replace_all_over_with(
        &window,
        dir.path(),
        ReplaceAllFixture {
            search_matches: u32::try_from(FILES).expect("FILES fits u32"),
            preview_rows: u32::try_from(FILES).expect("FILES fits u32"),
            budget: Duration::from_secs(180),
        },
        "thread",
        || {
            for file in &files {
                fixture::write_bytes(file, b"needle\ncaf\xe9\n");
            }
        },
    );
    wait_until(Duration::from_secs(60), || {
        panel.evidence().skipped_files_offered
    });
    assert_eq!(skipped_for(&panel, ReplaceSkipReason::NotUtf8), FILES);

    // The opening dispatch builds only the staged first rows: handed the
    // whole model at once, the list realized GTK's whole band of about 205
    // rows inside that dispatch (A1), 16.5–20.2 ms in the lane (task 9.1).
    activate_action(&window, "show-search-panel-skipped-files");
    let opening = panel.evidence();
    let opening_requests = opening
        .skipped_files_item_requests
        .expect("the dialog's model is alive while it is shown");
    assert!(
        opening_requests <= u64::from(lushtext_model::budgets::list_budget::LIST_FIRST_ROWS),
        "the opening dispatch builds at most the staged first rows; it built {opening_requests}",
    );
    assert!(
        opening
            .skipped_files_listed
            .is_some_and(|listed| listed <= lushtext_model::budgets::list_budget::LIST_FIRST_ROWS),
        "the dialog first lists only the staged rows: {:?}",
        opening.skipped_files_listed
    );
    // Once shown, the list's frame clock runs the growth.
    wait_until(Duration::from_secs(5), || {
        skipped_files_list(&window).is_realized()
    });
    // The growth lists every row within a few frames.
    wait_until(Duration::from_secs(10), || {
        panel.evidence().skipped_files_listed == Some(u32::try_from(FILES).expect("FILES fits u32"))
    });
    let requests = panel
        .evidence()
        .skipped_files_item_requests
        .expect("the dialog's model is alive while it is shown");
    assert!(requests >= 1, "the visible rows were built");
    assert!(
        requests < 1024,
        "a lazy model builds the realized band, not {FILES} rows; it built {requests}",
    );
    // A key moves the selection over the whole list: End lands on the last
    // file, which the growth listed (or a key finishes first).
    let list = skipped_files_list(&window);
    assert_eq!(
        list.model().map(|model| model.n_items()),
        Some(u32::try_from(FILES).expect("FILES fits u32")),
        "every skipped file is listed once the growth ends"
    );
    window.visible_dialog().expect("dialog").close();
    flush_events();
}

/// Skipped files the staged-growth tests list: past the staged first rows,
/// so the dialog opens with a growth left to run.
const STAGED_GROWTH_FILES: usize = 256;

/// A window whose last Replace All skipped [`STAGED_GROWTH_FILES`] files
/// (not UTF-8), ready to open Show Skipped Files.
fn window_with_staged_skipped_files() -> (
    LushtextWindow,
    lushtext_ui::ui::search_panel::LushtextSearchPanel,
    tempfile::TempDir,
) {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let files: Vec<_> = (0..STAGED_GROWTH_FILES)
        .map(|index| dir.path().join(format!("f{index:04}.txt")))
        .collect();
    for file in &files {
        fixture::write_text(file, "needle\nplain\n");
    }
    let count = u32::try_from(STAGED_GROWTH_FILES).expect("the fixture count fits u32");
    let panel = replace_all_over_with(
        &window,
        dir.path(),
        ReplaceAllFixture {
            search_matches: count,
            preview_rows: count,
            budget: Duration::from_secs(60),
        },
        "thread",
        || {
            for file in &files {
                fixture::write_bytes(file, b"needle\ncaf\xe9\n");
            }
        },
    );
    wait_until(Duration::from_secs(30), || {
        panel.evidence().skipped_files_offered
    });
    assert_eq!(
        skipped_for(&panel, ReplaceSkipReason::NotUtf8),
        STAGED_GROWTH_FILES
    );
    (window, panel, dir)
}

/// The Skipped Files list's own controller of `T` (capture phase), which the
/// dialog adds beside GTK's internal ones.
fn skipped_files_capture_controller<T: IsA<gtk4::EventController> + IsA<glib::Object>>(
    widget: &gtk4::Widget,
) -> T {
    let controllers = widget.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| controllers.item(index).and_downcast::<T>())
        .find(|controller| controller.propagation_phase() == gtk4::PropagationPhase::Capture)
        .expect("the Skipped Files dialog's capture-phase controller")
}

/// stream-replace-all task 9.1 (design C3, widget-wiring.md "Testing"): a key
/// pressed while the Skipped Files list still lists only its staged first
/// rows finishes the growth in that dispatch, so keyboard navigation lands
/// where it would on the whole list.
#[test]
fn test_skipped_files_key_finishes_the_staged_growth() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (window, panel, _dir) = window_with_staged_skipped_files();
    let full = u32::try_from(STAGED_GROWTH_FILES).expect("the fixture count fits u32");

    activate_action(&window, "show-search-panel-skipped-files");
    // Same turn as the opening: neither the frame clock nor the 512 ms delay
    // has run, so only the key can list the rest.
    let list = skipped_files_list(&window);
    assert_eq!(
        panel.evidence().skipped_files_listed,
        Some(lushtext_model::budgets::list_budget::LIST_FIRST_ROWS),
        "the dialog opens with only the staged first rows listed"
    );
    let keys: gtk4::EventControllerKey = skipped_files_capture_controller(list.upcast_ref());
    let args: [&dyn ToValue; 3] = [
        &gtk4::gdk::Key::End,
        &0u32,
        &gtk4::gdk::ModifierType::empty(),
    ];
    let stopped: bool = glib::object::ObjectExt::emit_by_name(&keys, "key-pressed", &args);
    assert!(
        !stopped,
        "the key goes on to the list once the growth is done"
    );
    assert_eq!(
        panel.evidence().skipped_files_listed,
        Some(full),
        "a key finishes the growth before the list handles it"
    );
    assert_eq!(list.model().map(|model| model.n_items()), Some(full));
    window.visible_dialog().expect("dialog").close();
    flush_events();
}

/// stream-replace-all task 9.1 (design C3): the growth waits out the dialog's
/// open animation (512 ms), but a wheel or touchpad scroll starts it at once.
#[test]
fn test_skipped_files_scroll_starts_the_staged_growth() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (window, panel, _dir) = window_with_staged_skipped_files();
    let first = lushtext_model::budgets::list_budget::LIST_FIRST_ROWS;
    // The delay the dialog waits before growing on its own
    // (`SKIPPED_FILES_GROWTH_DELAY`).
    let delay = Duration::from_millis(512);

    let opened = std::time::Instant::now();
    activate_action(&window, "show-search-panel-skipped-files");
    let list = skipped_files_list(&window);
    let scroller = list
        .parent()
        .and_downcast::<gtk4::ScrolledWindow>()
        .expect("the Skipped Files list sits in its scroller");
    let scroll: gtk4::EventControllerScroll =
        skipped_files_capture_controller(scroller.upcast_ref());
    let _: bool = glib::object::ObjectExt::emit_by_name(&scroll, "scroll", &[&0.0f64, &1.0f64]);
    assert_eq!(
        panel.evidence().skipped_files_listed,
        Some(first),
        "a scroll starts the growth; the list's frame clock runs its steps"
    );
    wait_until(Duration::from_secs(5), || {
        panel
            .evidence()
            .skipped_files_listed
            .is_some_and(|listed| listed > first)
    });
    let grown_after = opened.elapsed();
    println!(
        "skipped-files-scroll-growth {{\"first_step_after_ms\":{}}}",
        grown_after.as_millis()
    );
    assert!(
        grown_after < delay,
        "the scroll started the growth before the {delay:?} delay ran out; the first step came after {grown_after:?}"
    );
    window.visible_dialog().expect("dialog").close();
    flush_events();
}

/// stream-replace-all 5.5a: when every file is open with unsaved changes,
/// nothing runs, and the report lists each file as `UnsavedChanges`.
#[test]
fn test_replace_all_with_every_file_unsaved_reports_each_file() {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file = dir.path().join("open.txt");
    fixture::write_text(&file, "needle\n");
    window.open_document(&file);
    wait_until(Duration::from_secs(10), || {
        let editor = crate::common::active_editor(&window);
        editor.load_state() == lushtext_ui::ui::editor_page::EditorLoadState::Loaded
            && editor.accepts_user_edits()
    });
    let editor = crate::common::active_editor(&window);
    let buffer = editor.buffer();
    buffer.insert(&mut buffer.end_iter(), "edit\n");
    wait_until(Duration::from_secs(5), || editor.is_modified());

    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    search_through_fields(&panel, "needle", false, 1);
    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 1 && !evidence.replace_preview_pending
    });
    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        panel.evidence().skipped_files_offered
    });

    fixture::assert_text(&file, "needle\n");
    assert!(editor.is_modified(), "the unsaved edit is kept");
    let evidence = panel.evidence();
    assert_eq!(skipped_for(&panel, ReplaceSkipReason::UnsavedChanges), 1);
    assert!(!evidence.has_undo_backup);
    assert!(!evidence.replace_in_flight);
    let report = panel.last_skip_report().expect("the run's skip report");
    assert_eq!(report.files().len(), 1);
    assert_eq!(report.files()[0].path, file);
    assert_eq!(report.files()[0].reason, ReplaceSkipReason::UnsavedChanges);

    // 5.4: Open and Replace in Editor selects the unsaved tab and prefills
    // the bar from the run's request, though the panel's fields were edited
    // since, and the buffer is left as it was.
    let other = dir.path().join("other.txt");
    fixture::write_text(&other, "plain\n");
    window.open_document(&other);
    wait_until(Duration::from_secs(10), || {
        crate::common::active_editor(&window).file_path().as_deref() == Some(other.as_path())
    });
    panel.set_query("edited");
    panel.imp().replace_entry.set_text("edited too");
    activate_action(&window, "show-search-panel-skipped-files");
    skipped_files_list(&window).emit_by_name::<()>("activate", &[&0u32]);
    wait_until(Duration::from_secs(10), || {
        let selected = crate::common::active_editor(&window);
        selected.file_path().as_deref() == Some(file.as_path())
            && selected.search_bar().is_replace_revealed()
    });
    let bar = editor.search_bar();
    assert_eq!(bar.search_entry().text().as_str(), "needle");
    assert_eq!(bar.replace_entry().text().as_str(), "thread");
    assert_eq!(crate::common::editor_text(&editor), "needle\nedit\n");
    assert!(editor.is_modified());
    fixture::assert_text(&file, "needle\n");
}

/// stream-replace-all 5.5b: a file with a match on a short line and one on a
/// 10 KiB line is replaced where the preview could show it, and is reported
/// as partly replaced with the long line left.
#[test]
fn test_replace_all_reports_a_partly_replaced_file() {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file = dir.path().join("mixed.txt");
    let long_line = format!("{}needle\n", "x".repeat(10 * 1024));
    fixture::write_text(&file, &format!("needle short\n{long_line}"));

    // The 10 KiB line is past the preview's truncation limit, so the
    // preview shows one row for the file's two matches.
    let panel = replace_all_over_with(
        &window,
        dir.path(),
        ReplaceAllFixture {
            search_matches: 2,
            preview_rows: 1,
            budget: Duration::from_secs(20),
        },
        "thread",
        || {},
    );
    wait_until(Duration::from_secs(10), || {
        panel.evidence().skipped_files_offered
    });

    fixture::assert_text(&file, &format!("thread short\n{long_line}"));
    let evidence = panel.evidence();
    // A partly replaced file is replaced, so no reason counts it as skipped;
    // its row names the reason its long line was left.
    assert_eq!(skipped_for(&panel, ReplaceSkipReason::LineTooLong), 0);
    assert_eq!(evidence.replace_skip_partly_replaced, 1);
    let report = panel.last_skip_report().expect("the run's skip report");
    let row = report
        .files()
        .first()
        .expect("the partly replaced file is listed");
    assert_eq!(row.reason, ReplaceSkipReason::LineTooLong);
    assert_eq!(row.matches_left, Some(1));
    assert_eq!(row.first_line, Some(2));
}

/// stream-replace-all 6.2: Cancel during the third of five files restores the
/// first two, never writes the third, and leaves no journal and no Undo.
#[test]
fn test_cancel_replace_all_mid_run_restores_every_file() {
    cancel_mid_run_restores_every_file(|window, _| {
        activate_action(window, "cancel-search-panel-replacements");
    });
}

/// stream-replace-all 6.2: the same, through the button: Replace All becomes
/// Cancel Replace All, and clicking it cancels the run.
#[test]
fn test_clicking_cancel_replace_all_mid_run_restores_every_file() {
    cancel_mid_run_restores_every_file(|_, panel| {
        let button = panel.imp().replace_all_button.get();
        assert!(button.is_sensitive());
        // The panel is not realized here, so `gtk_widget_activate` would not
        // play the press; emit the `clicked` a press ends with.
        button.emit_clicked();
        assert_eq!(button.label().as_deref(), Some("Cancelling…"));
    });
}

/// Run five files, hold the third, cancel through `cancel`, and assert that
/// every file is original, the journal is empty and the button is restored.
fn cancel_mid_run_restores_every_file(cancel: impl FnOnce(&LushtextWindow, &LushtextSearchPanel)) {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let files: Vec<_> = (0..5)
        .map(|index| {
            let path = dir.path().join(format!("file_{index}.txt"));
            fixture::write_text(&path, "needle\n");
            path
        })
        .collect();
    // Hold the worker after the third file's undo body, so Cancel lands before
    // its pass B starts. The walk orders files by path.
    lushtext_services::search::content_search::register_after_pass_a_hook_for_test(
        files[2].clone(),
        |_| std::thread::sleep(Duration::from_millis(1_500)),
    );

    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == 5
    });
    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 5 && !evidence.replace_preview_pending
    });
    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        panel
            .evidence()
            .replace_progress
            .is_some_and(|progress| progress.files_done == 2)
    });
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Cancel Replace All")
    );
    assert!(panel.imp().replace_all_button.is_sensitive());
    cancel(&window, &panel);
    assert!(panel.evidence().replace_cancel_requested);

    wait_until(Duration::from_secs(30), || {
        let evidence = panel.evidence();
        !evidence.replace_in_flight && !evidence.replace_transaction_pending
    });
    for path in &files {
        fixture::assert_text(path, "needle\n");
    }
    assert!(!panel.evidence().has_undo_backup);
    wait_for_journal_disk_idle(&panel);
    assert!(
        search_backup::load(&data_dir)
            .expect("load journal")
            .is_empty(),
        "a fully rolled-back run leaves no active journal",
    );
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Replace All")
    );
}

/// Start a three-file Replace All whose worker holds after the second file's
/// undo body, and return once the run is in flight and admitted: a run that
/// waits for disposal capacity is in flight too, but it has not yet retired
/// any window's Undo or started its worker.
fn replace_all_held_at_second_file(
    window: &LushtextWindow,
    dir: &std::path::Path,
) -> (LushtextSearchPanel, Vec<PathBuf>) {
    replace_all_held_at_second_file_with(
        window,
        dir,
        LushtextSearchPanel::activate_confirm_replacements,
    )
}

/// [`replace_all_held_at_second_file`], confirming through `confirm`.
fn replace_all_held_at_second_file_with(
    window: &LushtextWindow,
    dir: &std::path::Path,
    confirm: impl FnOnce(&LushtextSearchPanel),
) -> (LushtextSearchPanel, Vec<PathBuf>) {
    let files: Vec<_> = (0..3)
        .map(|index| {
            let path = dir.join(format!("file_{index}.txt"));
            fixture::write_text(&path, "needle\n");
            path
        })
        .collect();
    lushtext_services::search::content_search::register_after_pass_a_hook_for_test(
        files[1].clone(),
        |_| std::thread::sleep(Duration::from_millis(1_500)),
    );
    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == 3
    });
    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 3 && !evidence.replace_preview_pending
    });
    confirm(&panel);
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        evidence.replace_in_flight && !evidence.replace_admission_deferred
    });
    (panel, files)
}

/// Run one palette file-index build in every window of `window`'s
/// application and wait until each has finished, so no build holds the
/// ordinary disposal lane when a test then fills it.
///
/// A Confirm no longer needs this: a refused undo reservation waits for the
/// lane (`replace-all-confirm-disposal-lane`). A fill does: it reserves only
/// what is free when it is taken, and a build reserves
/// `MAX_FILE_INDEX_RETAINED_BYTES` (64 MiB) of the 128 MiB lane until its
/// worker shrinks it, crediting the replaced weight, so a fill taken during a
/// build leaves room once the build shrinks and the waiting run would be
/// admitted under the fill. Each window's startup build fires 300 ms after
/// its scope refresh. Asking for a build supersedes a debounce still pending,
/// which no evidence exposes, and the wait covers the build that request
/// starts, so no startup build can begin after it.
fn settle_file_index_builds(window: &LushtextWindow) {
    let windows: Vec<LushtextWindow> = window
        .application()
        .expect("the test window's application")
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<LushtextWindow>().ok())
        .collect();
    let started: Vec<usize> = windows
        .iter()
        .map(|window| window.imp().file_index_builds.borrow().snapshot().started)
        .collect();
    for window in &windows {
        window.rebuild_file_index();
    }
    wait_until(Duration::from_secs(30), || {
        windows.iter().zip(&started).all(|(window, started)| {
            let builds = window.imp().file_index_builds.borrow();
            builds.snapshot().started > *started
                && !builds.has_work()
                && window.imp().file_index_admission.borrow().is_none()
        })
    });
}

/// Close the window during the run and return the dialog it asks with.
fn close_during_run(window: &LushtextWindow) -> libadwaita::AlertDialog {
    window.close();
    still_running_dialog(window)
}

/// Wait for "Replace All Is Still Running" and return it.
fn still_running_dialog(window: &LushtextWindow) -> libadwaita::AlertDialog {
    wait_until(Duration::from_secs(5), || {
        window
            .visible_dialog()
            .and_then(|dialog| dialog.downcast::<libadwaita::AlertDialog>().ok())
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading == "Replace All Is Still Running")
    });
    window
        .visible_dialog()
        .and_downcast::<libadwaita::AlertDialog>()
        .expect("the close-during-run dialog")
}

/// stream-replace-all 6.3: Keep Running closes nothing and the run completes.
#[test]
fn test_close_during_replace_all_keep_running_closes_nothing() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (panel, files) = replace_all_held_at_second_file(&window, dir.path());

    let dialog = close_during_run(&window);
    dialog.emit_by_name::<()>("response", &[&"keep-running"]);
    dialog.force_close();
    wait_until(Duration::from_secs(30), || {
        !panel.evidence().replace_in_flight && !panel.evidence().replace_transaction_pending
    });
    flush_after_delay(Duration::from_millis(100));
    assert!(window.is_visible(), "Keep Running leaves the window open");
    for path in &files {
        fixture::assert_text(path, "thread\n");
    }
    assert!(panel.evidence().has_undo_backup);
}

/// stream-replace-all 6.3: Close When Finished completes the run, keeps its
/// journal active, and then closes.
#[test]
fn test_close_during_replace_all_close_when_finished_keeps_the_journal() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (_panel, files) = replace_all_held_at_second_file(&window, dir.path());

    let dialog = close_during_run(&window);
    dialog.emit_by_name::<()>("response", &[&"close-when-finished"]);
    dialog.force_close();
    wait_until(Duration::from_secs(30), || !window.is_visible());
    for path in &files {
        fixture::assert_text(path, "thread\n");
    }
    assert_eq!(
        search_backup::load(&data_dir).expect("load journal").len(),
        3,
        "the finished run's journal stays active for Undo after the close",
    );
}

/// stream-replace-all 6.3: Stop and Close rolls back, then closes.
#[test]
fn test_close_during_replace_all_stop_and_close_restores_every_file() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (_panel, files) = replace_all_held_at_second_file(&window, dir.path());

    let dialog = close_during_run(&window);
    dialog.emit_by_name::<()>("response", &[&"stop-and-close"]);
    dialog.force_close();
    wait_until(Duration::from_secs(30), || !window.is_visible());
    for path in &files {
        fixture::assert_text(path, "needle\n");
    }
    assert!(
        search_backup::load(&data_dir)
            .expect("load journal")
            .is_empty(),
        "a rolled-back run leaves no active journal",
    );
}

/// stream-replace-all 6.4: the undo owner on GTK holds descriptors, never file
/// bytes. After a Replace All over four 1 MiB files, and after loading a
/// legacy journal whose originals are 1 MiB each, the installed journal's
/// disposal weight is paths and digests: a few hundred bytes per file.
#[test]
fn test_undo_owner_on_gtk_holds_descriptors_not_file_bytes() {
    const FILE_BYTES: usize = 1024 * 1024;
    const FILES: usize = 4;
    /// Far above a descriptor (a path, an entry file name, digests and node
    /// overhead), far below one file's bytes.
    const DESCRIPTOR_CEILING: u64 = 4 * 1024;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let filler = "x".repeat(FILE_BYTES);
    for index in 0..FILES {
        fixture::write_text(
            &dir.path().join(format!("f{index}.txt")),
            &format!("needle\n{filler}\n"),
        );
    }
    let panel = replace_all_over(
        &window,
        dir.path(),
        u32::try_from(FILES).expect("FILES fits u32"),
        "thread",
    );
    wait_until(Duration::from_secs(10), || panel.has_undo_backup());
    let evidence = panel.evidence();
    assert_eq!(evidence.undo_backup_entry_count, FILES);
    assert!(
        evidence.undo_backup_retained_bytes <= DESCRIPTOR_CEILING * FILES as u64,
        "a streamed journal's GTK owner retains {} bytes for {FILES} files of {FILE_BYTES} bytes",
        evidence.undo_backup_retained_bytes,
    );
    drop(window);

    // A legacy journal keeps its originals inline in its entry files; the
    // loader leaves them on disk.
    let data_dir = json_store::data_dir();
    let original = format!("needle\n{filler}\n");
    let replaced = format!("thread\n{filler}\n");
    let paths: Vec<_> = (0..FILES)
        .map(|index| std::path::PathBuf::from(format!("/legacy-{index}.txt")))
        .collect();
    let files: Vec<_> = paths
        .iter()
        .map(|path| (path.as_path(), original.as_bytes(), replaced.as_bytes()))
        .collect();
    search_backup::fixture::write_legacy_journal(&data_dir, &files, false)
        .expect("write legacy journal");
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    wait_until(Duration::from_secs(10), || panel.has_undo_backup());
    let evidence = panel.evidence();
    assert_eq!(evidence.undo_backup_entry_count, FILES);
    assert!(
        evidence.undo_backup_retained_bytes <= DESCRIPTOR_CEILING * FILES as u64,
        "a legacy journal's GTK owner retains {} bytes for {FILES} files of {FILE_BYTES} bytes",
        evidence.undo_backup_retained_bytes,
    );
    let _ = search_backup::delete(&data_dir);
}

/// A panel restored visible from GSettings takes its automation actions with
/// it: `set-search-panel-query` through `undo-search-panel-replacements`
/// require a visible panel and must be enabled when it is (found by the
/// stream-replace-all crash smoke, whose relaunch's Undo did nothing).
#[test]
fn test_a_panel_restored_visible_enables_its_actions() {
    use lushtext_ui::config::keys;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let settings = lushtext_ui::ui::settings_writes::shared();
    settings
        .set_boolean(keys::SEARCH_PANEL_VISIBLE, true)
        .expect("set search-panel-visible");
    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(5), || {
        window.imp().search_panel_revealer.reveals_child()
    });
    let enabled = |name: &str| gtk4::gio::prelude::ActionGroupExt::is_action_enabled(&window, name);
    for name in [
        "set-search-panel-query",
        "set-search-panel-replace-query",
        "preview-search-panel-replacements",
        "confirm-search-panel-replacements",
        "undo-search-panel-replacements",
    ] {
        assert!(
            enabled(name),
            "{name} is disabled on a restored visible panel"
        );
    }
    settings
        .set_boolean(keys::SEARCH_PANEL_VISIBLE, false)
        .expect("reset search-panel-visible");
    window.close();
}

// ---------------------------------------------------------------------------
// stream-replace-all: Open and Replace in Editor (5.4), one journal per
// application (4.9) and quitting during a run (6.3)
// ---------------------------------------------------------------------------

/// The window's status-bar text.
fn status_text(window: &LushtextWindow) -> String {
    window
        .imp()
        .status_bar
        .imp()
        .message_label
        .label()
        .to_string()
}

/// Search through the panel's own fields, as a user types the query, so the
/// preview and its Replace All carry the query a Skipped Files row prefills.
fn search_through_fields(panel: &LushtextSearchPanel, query: &str, regex: bool, matches: u32) {
    panel.imp().regex_toggle.set_active(regex);
    panel.set_query(query);
    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == matches
    });
}

/// Search, preview and confirm through the panel's fields, running
/// `before_confirm` between the preview and the confirmation, and wait for
/// the skip report.
fn replace_all_through_fields(
    window: &LushtextWindow,
    dir: &std::path::Path,
    query: &str,
    regex: bool,
    replacement: &str,
    before_confirm: impl FnOnce(),
) -> LushtextSearchPanel {
    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.to_path_buf()]);
    search_through_fields(&panel, query, regex, 1);
    panel.enter_preview_mode(replacement);
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 1 && !evidence.replace_preview_pending
    });
    before_confirm();
    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        panel.evidence().skipped_files_offered && !panel.evidence().replace_in_flight
    });
    panel
}

/// Run `win.replace-in-editor` for `path`.
fn replace_in_editor(window: &LushtextWindow, path: &std::path::Path) {
    gtk4::gio::prelude::ActionGroupExt::activate_action(
        window,
        "replace-in-editor",
        Some(&path.to_str().expect("UTF-8 fixture path").to_variant()),
    );
    flush_events();
}

/// The selected tab's editor once it shows `path` loaded with its replace
/// bar open.
fn wait_for_prefilled_editor(
    window: &LushtextWindow,
    path: &std::path::Path,
    replace_row: bool,
) -> lushtext_ui::ui::editor_page::LushtextEditorPage {
    wait_until(Duration::from_secs(15), || {
        let Some(editor) = window
            .imp()
            .tab_view
            .selected_page()
            .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
        else {
            return false;
        };
        editor.file_path().as_deref() == Some(path)
            && editor.load_state() == lushtext_ui::ui::editor_page::EditorLoadState::Loaded
            && editor.is_search_visible()
            && editor.search_bar().is_replace_revealed() == replace_row
            && !editor.search_bar().search_entry().text().is_empty()
    });
    crate::common::active_editor(window)
}

/// Open `path` and make it unsaved, so a Replace All skips it.
fn open_unsaved(window: &LushtextWindow, path: &std::path::Path) -> LushtextEditorPage {
    let editor = open_loaded(window, path);
    edit_unsaved(&editor);
    editor
}

/// stream-replace-all 5.4: a closed file opens at its first skipped line,
/// with the bar prefilled, and nothing is replaced in it.
#[test]
fn test_replace_in_editor_opens_a_closed_file_at_its_first_skipped_line() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file = dir.path().join("mixed.txt");
    let long_line = format!("{}needle\n", "x".repeat(10 * 1024));
    fixture::write_text(&file, &format!("needle short\n{long_line}"));
    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    search_through_fields(&panel, "needle", false, 2);
    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 1 && !evidence.replace_preview_pending
    });
    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        panel.evidence().skipped_files_offered && !panel.evidence().replace_in_flight
    });
    let replaced = format!("thread short\n{long_line}");
    fixture::assert_text(&file, &replaced);
    assert_eq!(window.imp().tab_view.n_pages(), 0, "the file is closed");

    replace_in_editor(&window, &file);
    let editor = wait_for_prefilled_editor(&window, &file, true);
    let bar = editor.search_bar();
    assert_eq!(bar.search_entry().text().as_str(), "needle");
    assert_eq!(bar.replace_entry().text().as_str(), "thread");
    let buffer = editor.buffer();
    let caret = buffer.iter_at_mark(&buffer.get_insert());
    assert_eq!(caret.line(), 1, "the caret is on the first skipped line");
    assert_eq!(crate::common::editor_text(&editor), replaced);
    assert!(!editor.is_modified(), "prefilling replaces nothing");
}

/// stream-replace-all 5.4: in regex mode the run's template is translated
/// for GtkSourceView, `${1}0` staying group 1 then `0`, and applying it in
/// the bar gives what Workspace Replace All would have written (A127).
#[test]
fn test_replace_in_editor_translates_a_regex_replacement() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file = dir.path().join("pair.txt");
    fixture::write_text(&file, "foo-bar\n");
    let editor = open_unsaved(&window, &file);
    let _panel =
        replace_all_through_fields(&window, dir.path(), r"(\w+)-(\w+)", true, "$2-${1}0", || {});

    replace_in_editor(&window, &file);
    let shown = wait_for_prefilled_editor(&window, &file, true);
    assert_eq!(shown, editor);
    let bar = editor.search_bar();
    assert_eq!(bar.search_entry().text().as_str(), r"(\w+)-(\w+)");
    assert_eq!(bar.replace_entry().text().as_str(), r"\g<2>-\g<1>0");
    let settings = bar
        .search_context()
        .expect("the bar's search session")
        .settings();
    assert!(sourceview5::prelude::SearchSettingsExt::is_regex_enabled(
        &settings
    ));
    assert_eq!(crate::common::editor_text(&editor), "foo-bar\nedit\n");

    bar.replace_all();
    flush_events();
    assert_eq!(crate::common::editor_text(&editor), "bar-foo0\nedit\n");
    fixture::assert_text(&file, "foo-bar\n");
}

/// stream-replace-all 5.4: a template that names a group the pattern lacks
/// is not carried over; the field stays empty and says why.
#[test]
fn test_replace_in_editor_leaves_a_missing_group_replacement_empty() {
    use lushtext_ui::ui::search_bar::policy::MISSING_GROUP_PLACEHOLDER;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file = dir.path().join("pair.txt");
    fixture::write_text(&file, "foo-bar\n");
    let editor = open_unsaved(&window, &file);
    let _panel = replace_all_through_fields(&window, dir.path(), r"(\w+)-(\w+)", true, "$3", || {});

    replace_in_editor(&window, &file);
    wait_for_prefilled_editor(&window, &file, true);
    let bar = editor.search_bar();
    assert_eq!(bar.search_entry().text().as_str(), r"(\w+)-(\w+)");
    assert!(bar.replace_entry().text().is_empty());
    assert_eq!(
        bar.replace_entry().placeholder_text().as_deref(),
        Some(MISSING_GROUP_PLACEHOLDER)
    );
}

/// stream-replace-all 5.4: a file too large to open has no row action, and
/// the action refuses it and any path the last run did not skip.
#[test]
fn test_replace_in_editor_has_no_action_for_a_file_too_large_to_open() {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let big = dir.path().join("big.txt");
    fixture::write_text(&big, "needle\n");
    let panel = replace_all_through_fields(&window, dir.path(), "needle", false, "thread", || {
        fixture::create_sparse_file(
            &big,
            lushtext_model::document::document_scale::REFUSE_TO_OPEN + 1,
        );
    });
    assert_eq!(
        skipped_for(&panel, ReplaceSkipReason::LargerThanOpenTier),
        1
    );

    activate_action(&window, "show-search-panel-skipped-files");
    skipped_files_list(&window).emit_by_name::<()>("activate", &[&0u32]);
    flush_after_delay(Duration::from_millis(200));
    assert!(window.visible_dialog().is_some(), "the row has no action");
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    window.visible_dialog().expect("dialog").close();
    flush_events();

    replace_in_editor(&window, &big);
    flush_after_delay(Duration::from_millis(200));
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert_eq!(
        status_text(&window),
        "This skipped file cannot be opened in the editor"
    );
    replace_in_editor(&window, &dir.path().join("never-skipped.txt"));
    assert_eq!(
        status_text(&window),
        "Replace in Editor needs a file the last Replace All skipped"
    );
    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

/// stream-replace-all 5.4: a file that opens in Protected View keeps its
/// replace row hidden and shows the Protected View notice; find still gets
/// the query.
#[test]
fn test_replace_in_editor_on_a_protected_document_shows_the_notice() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file = dir.path().join("giant.txt");
    let giant = format!("needle{}\n", "x".repeat(60_000));
    fixture::write_text(&file, &format!("needle short\n{giant}"));
    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    search_through_fields(&panel, "needle", false, 2);
    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 1 && !evidence.replace_preview_pending
    });
    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        panel.evidence().skipped_files_offered && !panel.evidence().replace_in_flight
    });

    replace_in_editor(&window, &file);
    let editor = wait_for_prefilled_editor(&window, &file, false);
    assert!(editor.is_protected_view());
    assert_eq!(editor.search_bar().search_entry().text().as_str(), "needle");
    assert_eq!(
        status_text(&window),
        "Replace is not available in this view-only document with very long lines."
    );
}

/// Another window of `window`'s application, once its startup has settled.
fn second_window(window: &LushtextWindow) -> LushtextWindow {
    let app = window
        .application()
        .and_downcast::<libadwaita::Application>()
        .expect("the test window's application");
    let second = LushtextWindow::new(&app);
    present_window(&second);
    crate::common::wait_for_startup_data_flow(&second);
    second
}

/// The refusal a second window's apply or Undo gets during a run.
const OTHER_WINDOW_BUSY: &str =
    "Another window is running Replace All or its Undo; try again when it finishes";

/// stream-replace-all 4.9: a run in window B retires window A's Undo, and A
/// says so once.
#[test]
fn test_a_run_in_another_window_retires_this_windows_undo() {
    const RETIRED: &str = "Undo for the earlier Replace All is no longer available";
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window_a = test_window();
    present_window(&window_a);
    crate::common::wait_for_startup_data_flow(&window_a);
    let window_b = second_window(&window_a);

    let dir_a = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir_a.path().join("a.txt"), "needle\n");
    let panel_a = replace_all_over(&window_a, dir_a.path(), 1, "thread");
    wait_until(Duration::from_secs(20), || {
        panel_a.evidence().has_undo_backup
    });

    let dir_b = tempfile::tempdir().expect("expected operation to succeed");
    fixture::write_text(&dir_b.path().join("b.txt"), "needle\n");
    let panel_b = replace_all_over(&window_b, dir_b.path(), 1, "thread");
    wait_until(Duration::from_secs(20), || {
        panel_b.evidence().has_undo_backup
    });

    assert!(!panel_a.evidence().has_undo_backup);
    assert!(!panel_a.imp().undo_button.property::<bool>("visible"));
    assert_eq!(status_text(&window_a), RETIRED);
    assert_ne!(status_text(&window_b), RETIRED);
}

/// stream-replace-all 4.9: an apply in window A while window B's run is in
/// flight is refused, touches no file, and keeps A's preview.
#[test]
fn test_an_apply_while_another_window_runs_is_refused() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window_a = test_window();
    present_window(&window_a);
    crate::common::wait_for_startup_data_flow(&window_a);
    let window_b = second_window(&window_a);

    let dir_a = tempfile::tempdir().expect("expected operation to succeed");
    let file_a = dir_a.path().join("a.txt");
    fixture::write_text(&file_a, "needle\n");
    let panel_a = window_a.imp().search_panel.get();
    panel_a.set_workspace_folders(vec![dir_a.path().to_path_buf()]);
    panel_a.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel_a.imp().runtime.searching.get() && panel_a.imp().runtime.total_matches.get() == 1
    });
    panel_a.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel_a.evidence();
        evidence.replace_preview_count == 1 && !evidence.replace_preview_pending
    });

    let dir_b = tempfile::tempdir().expect("expected operation to succeed");
    let (panel_b, files_b) = replace_all_held_at_second_file(&window_b, dir_b.path());
    panel_a.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(300));
    let evidence = panel_a.evidence();
    assert!(!evidence.replace_transaction_pending && !evidence.replace_in_flight);
    assert_eq!(status_text(&window_a), OTHER_WINDOW_BUSY);
    assert!(
        panel_a.imp().preview.preview_mode.get(),
        "A can retry its preview"
    );
    fixture::assert_text(&file_a, "needle\n");

    wait_until(Duration::from_secs(30), || {
        !panel_b.evidence().replace_in_flight && !panel_b.evidence().replace_transaction_pending
    });
    for path in &files_b {
        fixture::assert_text(path, "thread\n");
    }
    fixture::assert_text(&file_a, "needle\n");
}

/// stream-replace-all 4.9: an Undo in window A while window B's run is in
/// flight is refused. B's run already retired A's Undo, so the test puts
/// the retired journal back to reach the application-wide gate itself.
#[test]
fn test_an_undo_while_another_window_runs_is_refused() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window_a = test_window();
    present_window(&window_a);
    crate::common::wait_for_startup_data_flow(&window_a);
    let window_b = second_window(&window_a);

    let dir_a = tempfile::tempdir().expect("expected operation to succeed");
    let file_a = dir_a.path().join("a.txt");
    fixture::write_text(&file_a, "needle\n");
    let panel_a = replace_all_over(&window_a, dir_a.path(), 1, "thread");
    wait_until(Duration::from_secs(20), || {
        panel_a.evidence().has_undo_backup
    });
    let journal = panel_a
        .imp()
        .preview
        .undo_backup
        .borrow()
        .clone()
        .expect("A's undo journal");

    let dir_b = tempfile::tempdir().expect("expected operation to succeed");
    let (panel_b, files_b) = replace_all_held_at_second_file(&window_b, dir_b.path());
    assert!(
        !panel_a.evidence().has_undo_backup,
        "B's run retired A's Undo"
    );
    panel_a.imp().preview.undo_backup.replace(Some(journal));
    panel_a.activate_undo_replacements();
    flush_after_delay(Duration::from_millis(300));
    assert!(!panel_a.evidence().replace_transaction_pending);
    assert_eq!(status_text(&window_a), OTHER_WINDOW_BUSY);
    fixture::assert_text(&file_a, "thread\n");

    wait_until(Duration::from_secs(30), || {
        !panel_b.evidence().replace_in_flight && !panel_b.evidence().replace_transaction_pending
    });
    for path in &files_b {
        fixture::assert_text(path, "thread\n");
    }
    fixture::assert_text(&file_a, "thread\n");
}

/// stream-replace-all 6.3: quitting the application during a run asks the
/// same question; Stop and Close rolls back and then closes.
#[test]
fn test_quitting_during_replace_all_asks_and_stop_and_close_restores_every_file() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (_panel, files) = replace_all_held_at_second_file(&window, dir.path());

    let app = window.application().expect("the test window's application");
    gtk4::gio::prelude::ActionGroupExt::activate_action(&app, "quit", None);
    let dialog = still_running_dialog(&window);
    dialog.emit_by_name::<()>("response", &[&"stop-and-close"]);
    dialog.force_close();
    wait_until(Duration::from_secs(30), || !window.is_visible());
    for path in &files {
        fixture::assert_text(path, "needle\n");
    }
    assert!(
        search_backup::load(&data_dir)
            .expect("load journal")
            .is_empty(),
        "a rolled-back run leaves no active journal",
    );
}

// ---------------------------------------------------------------------------
// replace-all-confirm-disposal-lane: a confirmed Replace All waits for the
// disposal capacity its undo reservation needs (design D8)
// ---------------------------------------------------------------------------

/// Write three one-match files under `dir`, search and preview them in
/// `window`'s panel, and return the panel and the files.
fn preview_three_files(
    window: &LushtextWindow,
    dir: &std::path::Path,
) -> (LushtextSearchPanel, Vec<PathBuf>) {
    preview_named_files(window, dir, "wait")
}

/// [`preview_three_files`] with files named `<prefix>_<n>.txt`, so a second
/// preview in the same folder finds only its own files once the first run
/// replaced theirs.
fn preview_named_files(
    window: &LushtextWindow,
    dir: &std::path::Path,
    prefix: &str,
) -> (LushtextSearchPanel, Vec<PathBuf>) {
    let files: Vec<_> = (0..3)
        .map(|index| {
            let path = dir.join(format!("{prefix}_{index}.txt"));
            fixture::write_text(&path, "needle\n");
            path
        })
        .collect();
    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == 3
    });
    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 3 && !evidence.replace_preview_pending
    });
    (panel, files)
}

/// Fill the ordinary disposal lane once no palette file-index build holds
/// it (design D8, "Filling the lane"): the fill is taken in the same turn the
/// settle wait returns, so no build can start between the two.
fn fill_lane_after_settling(
    window: &LushtextWindow,
) -> lushtext_ui::ui::plain_disposal::DisposalCapacityHold {
    settle_file_index_builds(window);
    lushtext_ui::ui::plain_disposal::fill_disposal_capacity_for_test()
}

fn assert_texts(files: &[PathBuf], expected: &str) {
    for path in files {
        fixture::assert_text(path, expected);
    }
}

fn all_read(files: &[PathBuf], expected: &str) -> bool {
    files
        .iter()
        .all(|path| fixture::read_text(path) == expected)
}

/// The status line while a confirmed run waits (design D6).
const REPLACE_WAITING: &str = "Replace All will start when memory frees up";
/// The Cancel Replace All button's description and tooltip while it waits.
const REPLACE_WAITING_DESCRIPTION: &str = "Waiting for memory before Replace All starts";
/// The status after Cancel ends a wait.
const REPLACE_WAIT_CANCELLED: &str = "Replace All cancelled; no file was changed";

/// Assert the state of a confirmed run waiting for capacity (design D8,
/// T1 step 3): deferred, in flight, holding the transaction, and offering
/// Cancel Replace All with the waiting description.
fn assert_waiting(panel: &LushtextSearchPanel) {
    let evidence = panel.evidence();
    assert!(
        evidence.replace_admission_deferred,
        "the confirmed run waits for capacity"
    );
    assert!(evidence.replace_in_flight, "a waiting run is in flight");
    assert!(
        evidence.replace_transaction_pending,
        "a waiting run holds the transaction"
    );
    let button = panel.imp().replace_all_button.get();
    assert_eq!(button.label().as_deref(), Some("Cancel Replace All"));
    assert!(button.is_sensitive());
    assert_eq!(
        button.tooltip_text().as_deref(),
        Some(REPLACE_WAITING_DESCRIPTION)
    );
    assert!(crate::common::accessible_description_is(
        &button,
        REPLACE_WAITING_DESCRIPTION
    ));
}

/// Open `path` in `window` and wait until its tab accepts edits.
fn open_loaded(window: &LushtextWindow, path: &std::path::Path) -> LushtextEditorPage {
    window.open_document(path);
    wait_until(Duration::from_secs(10), || {
        let editor = crate::common::active_editor(window);
        editor.file_path().as_deref() == Some(path)
            && editor.load_state() == lushtext_ui::ui::editor_page::EditorLoadState::Loaded
            && editor.accepts_user_edits()
    });
    crate::common::active_editor(window)
}

/// Type into `editor` without saving.
fn edit_unsaved(editor: &LushtextEditorPage) {
    let buffer = editor.buffer();
    buffer.insert(&mut buffer.end_iter(), "edit\n");
    wait_until(Duration::from_secs(5), || editor.is_modified());
}

/// T1: a Confirm refused for disposal capacity waits, writes nothing while
/// the lane is full, and replaces every file once it frees.
#[test]
fn test_confirm_under_a_full_lane_waits_and_then_replaces() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (panel, files) = preview_three_files(&window, dir.path());
    wait_for_journal_disk_idle(&panel);

    let fill = fill_lane_after_settling(&window);
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    assert_texts(&files, "needle\n");

    assert_waiting(&panel);
    assert!(
        window
            .imp()
            .status_bar
            .imp()
            .status_announcement_throttler
            .has_recent_announcement_for_test(
                accessibility::AnnouncementLane::StatusUpdate,
                "workflow:replace-all-waiting",
            ),
        "entering the wait is announced",
    );
    let app = window
        .application()
        .and_downcast::<lushtext_ui::app::LushtextApplication>()
        .expect("the test window's application");
    let search_ready = glib::MainContext::default().block_on(
        lushtext_ui::ui::automation::wait_for_ready_for_test(
            app,
            lushtext_model::automation::AutomationReadinessPredicate::SearchComplete,
            1,
        ),
    );
    assert!(
        !search_ready.ok,
        "search-complete stays pending while the run waits"
    );
    // A one-off status message expires after 10 s; the waiting text is kept
    // up for the whole wait.
    flush_after_delay(Duration::from_secs(11));
    assert_eq!(status_text(&window), REPLACE_WAITING);
    assert_waiting(&panel);
    assert_texts(&files, "needle\n");

    drop(fill);
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        all_read(&files, "thread\n")
            && evidence.last_apply_counts.is_some()
            && evidence.has_undo_backup
            && !evidence.replace_in_flight
            && !evidence.replace_transaction_pending
    });
    assert_texts(&files, "thread\n");
    assert!(panel.imp().undo_button.property::<bool>("visible"));
    assert!(!panel.evidence().replace_admission_deferred);
    assert_ne!(status_text(&window), REPLACE_WAITING);
    assert_eq!(panel.imp().replace_all_button.tooltip_text(), None);
}

/// T2: while window B's confirmed run waits for capacity, window A's apply
/// and A's Undo are refused as during a run; once B is admitted and done,
/// A's kept preview confirms.
#[test]
fn test_a_waiting_run_refuses_another_windows_apply_and_undo() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window_a = test_window();
    present_window(&window_a);
    crate::common::wait_for_startup_data_flow(&window_a);
    let window_b = second_window(&window_a);

    // A: an earlier Replace All, so A offers Undo, then a second preview.
    let dir_a = tempfile::tempdir().expect("expected operation to succeed");
    let first_a = dir_a.path().join("a.txt");
    fixture::write_text(&first_a, "needle\n");
    let panel_a = replace_all_over(&window_a, dir_a.path(), 1, "thread");
    wait_until(Duration::from_secs(20), || {
        let evidence = panel_a.evidence();
        evidence.has_undo_backup && !evidence.replace_transaction_pending
    });
    wait_for_journal_disk_idle(&panel_a);
    let second_a = dir_a.path().join("a2.txt");
    fixture::write_text(&second_a, "needle\n");
    panel_a.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel_a.imp().runtime.searching.get() && panel_a.imp().runtime.total_matches.get() == 1
    });
    panel_a.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel_a.evidence();
        evidence.replace_preview_count == 1 && !evidence.replace_preview_pending
    });

    // B previews three files, then confirms under a full lane.
    let dir_b = tempfile::tempdir().expect("expected operation to succeed");
    let (panel_b, files_b) = preview_three_files(&window_b, dir_b.path());
    wait_for_journal_disk_idle(&panel_b);
    let fill = fill_lane_after_settling(&window_a);
    panel_b.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    assert_waiting(&panel_b);

    panel_a.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(status_text(&window_a), OTHER_WINDOW_BUSY);
    assert!(
        panel_a.imp().preview.preview_mode.get(),
        "A keeps its preview for a retry"
    );
    assert!(!panel_a.evidence().replace_transaction_pending);
    fixture::assert_text(&second_a, "needle\n");

    panel_a.activate_undo_replacements();
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(status_text(&window_a), OTHER_WINDOW_BUSY);
    assert!(
        panel_a.evidence().has_undo_backup,
        "A's Undo stays installed"
    );
    fixture::assert_text(&first_a, "thread\n");
    assert_texts(&files_b, "needle\n");

    drop(fill);
    wait_until(Duration::from_secs(20), || {
        let evidence = panel_b.evidence();
        all_read(&files_b, "thread\n")
            && !evidence.replace_in_flight
            && !evidence.replace_transaction_pending
    });
    assert!(
        !panel_a.evidence().has_undo_backup,
        "B's admitted run retired A's Undo"
    );
    panel_a.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        fixture::read_text(&second_a) == "thread\n"
            && !panel_a.evidence().replace_transaction_pending
    });
}

/// T6: the original two-window regression, with a palette file-index build
/// provably holding its reservation when B confirms (design D8, T6).
#[test]
fn test_an_apply_while_another_window_runs_is_refused_across_an_index_build() {
    use lushtext_ui::ui::command_palette::CommandPaletteTestPolicy;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    struct PolicyReset;
    impl Drop for PolicyReset {
        fn drop(&mut self) {
            CommandPaletteTestPolicy::reset();
        }
    }
    let _reset = PolicyReset;
    CommandPaletteTestPolicy::current()
        .with_file_index_build_delay(Duration::from_secs(3))
        .install();
    let window_a = test_window();
    present_window(&window_a);
    crate::common::wait_for_startup_data_flow(&window_a);
    let window_b = second_window(&window_a);

    let dir_a = tempfile::tempdir().expect("expected operation to succeed");
    let file_a = dir_a.path().join("a.txt");
    fixture::write_text(&file_a, "needle\n");
    let panel_a = window_a.imp().search_panel.get();
    panel_a.set_workspace_folders(vec![dir_a.path().to_path_buf()]);
    panel_a.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel_a.imp().runtime.searching.get() && panel_a.imp().runtime.total_matches.get() == 1
    });
    panel_a.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel_a.evidence();
        evidence.replace_preview_count == 1 && !evidence.replace_preview_pending
    });

    let dir_b = tempfile::tempdir().expect("expected operation to succeed");
    let (panel_b, files_b) =
        replace_all_held_at_second_file_with(&window_b, dir_b.path(), |panel| {
            let started = window_b.imp().file_index_builds.borrow().snapshot().started;
            window_b.rebuild_file_index();
            wait_until(Duration::from_secs(20), || {
                window_b.imp().file_index_builds.borrow().snapshot().started > started
            });
            panel.activate_confirm_replacements();
        });
    panel_a.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(300));
    let evidence = panel_a.evidence();
    assert!(!evidence.replace_transaction_pending && !evidence.replace_in_flight);
    assert_eq!(status_text(&window_a), OTHER_WINDOW_BUSY);
    assert!(
        panel_a.imp().preview.preview_mode.get(),
        "A can retry its preview"
    );
    fixture::assert_text(&file_a, "needle\n");

    wait_until(Duration::from_secs(30), || {
        !panel_b.evidence().replace_in_flight && !panel_b.evidence().replace_transaction_pending
    });
    assert_texts(&files_b, "thread\n");
    fixture::assert_text(&file_a, "needle\n");
}

/// T3: Cancel while the run waits ends it at once with nothing written, keeps
/// the earlier run's Undo and Show Skipped Files report, and Undo is not
/// offered during the wait.
#[test]
fn test_cancel_while_waiting_keeps_the_earlier_undo_and_skip_report() {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let earlier: Vec<_> = (0..2)
        .map(|index| {
            let path = dir.path().join(format!("earlier_{index}.txt"));
            fixture::write_text(&path, "needle\n");
            path
        })
        .collect();
    let unsaved = dir.path().join("unsaved.txt");
    fixture::write_text(&unsaved, "needle\n");
    let editor = open_unsaved(&window, &unsaved);

    // The earlier run: two files replaced, the unsaved one skipped.
    let panel = replace_all_over(&window, dir.path(), 3, "thread");
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        evidence.has_undo_backup && evidence.skipped_files_offered && !evidence.replace_in_flight
    });
    wait_for_journal_disk_idle(&panel);
    assert_texts(&earlier, "thread\n");

    // The later run, previewed before the fill, waits.
    let later = dir.path().join("later.txt");
    fixture::write_text(&later, "needle\n");
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == 2
    });
    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 2 && !evidence.replace_preview_pending
    });
    let fill = fill_lane_after_settling(&window);
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    assert_waiting(&panel);
    assert!(
        !panel.imp().undo_button.is_sensitive(),
        "Undo is not offered while the run waits"
    );
    panel.activate_undo_replacements();
    flush_events();
    assert_texts(&earlier, "thread\n");

    panel.imp().replace_all_button.get().emit_clicked();
    let cancelled = panel.evidence();
    assert!(!cancelled.replace_admission_deferred);
    assert!(!cancelled.replace_in_flight);
    assert!(!cancelled.replace_transaction_pending);
    assert_eq!(status_text(&window), REPLACE_WAIT_CANCELLED);
    assert_eq!(
        panel.imp().replace_all_button.label().as_deref(),
        Some("Replace All")
    );
    assert_eq!(panel.imp().replace_all_button.tooltip_text(), None);
    fixture::assert_text(&later, "needle\n");
    assert!(
        cancelled.skipped_files_offered,
        "the earlier skip report stays"
    );
    assert_eq!(skipped_for(&panel, ReplaceSkipReason::UnsavedChanges), 1);
    let report = panel.last_skip_report().expect("the earlier skip report");
    assert_eq!(report.files()[0].path, unsaved);
    assert!(
        cancelled.has_undo_backup,
        "the earlier Undo stays installed"
    );
    assert!(panel.imp().undo_button.is_sensitive());
    assert!(panel.imp().undo_button.property::<bool>("visible"));

    drop(fill);
    flush_after_delay(Duration::from_millis(300));
    fixture::assert_text(&later, "needle\n");
    panel.activate_undo_replacements();
    wait_until(Duration::from_secs(20), || {
        all_read(&earlier, "needle\n") && !panel.evidence().replace_transaction_pending
    });
    assert!(editor.is_modified(), "the unsaved tab keeps its edit");
}

/// T4: hiding the panel (Escape) while the run waits keeps the run, which
/// replaces the files once capacity frees.
#[test]
fn test_escape_while_waiting_still_replaces() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    window.toggle_search_panel();
    let (panel, files) = preview_three_files(&window, dir.path());
    wait_for_journal_disk_idle(&panel);
    let fill = fill_lane_after_settling(&window);
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    assert_waiting(&panel);

    // A preview asked for during the run says why nothing happens.
    panel.activate_replace_preview();
    flush_events();
    assert_eq!(
        status_text(&window),
        "Replace All is running; preview again when it finishes"
    );
    assert_waiting(&panel);

    window.close_search_panel();
    flush_after_delay(Duration::from_millis(300));
    assert!(!window.imp().search_panel_revealer.reveals_child());
    assert_waiting(&panel);
    assert_texts(&files, "needle\n");

    drop(fill);
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        all_read(&files, "thread\n") && !evidence.replace_in_flight && evidence.has_undo_backup
    });
}

/// T4b: a workspace search during the wait shows its own state, the waiting
/// text comes back within a keepalive period of the search ending, and the
/// run still starts once capacity frees.
#[test]
fn test_a_new_search_while_waiting_hands_the_status_line_back() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    struct PolicyReset;
    impl Drop for PolicyReset {
        fn drop(&mut self) {
            SearchPanelTestPolicy::reset();
        }
    }
    let _reset = PolicyReset;
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    window.toggle_search_panel();
    let (panel, files) = preview_three_files(&window, dir.path());
    wait_for_journal_disk_idle(&panel);
    let fill = fill_lane_after_settling(&window);
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(1_500));
    assert_waiting(&panel);
    assert_eq!(status_text(&window), REPLACE_WAITING);

    SearchPanelTestPolicy::current()
        .with_search_worker_delay(Duration::from_millis(2_500))
        .install();
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(5), || {
        panel.imp().runtime.searching.get()
    });
    let mut saw_waiting_during_search = false;
    while panel.imp().runtime.searching.get() {
        saw_waiting_during_search |= status_text(&window) == REPLACE_WAITING;
        flush_after_delay(Duration::from_millis(50));
    }
    assert!(
        !saw_waiting_during_search,
        "the search's own state shows while it runs"
    );
    wait_until(Duration::from_secs(2), || {
        status_text(&window) == REPLACE_WAITING
    });
    assert_waiting(&panel);
    assert_texts(&files, "needle\n");

    drop(fill);
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        all_read(&files, "thread\n") && !evidence.replace_in_flight && evidence.has_undo_backup
    });
}

/// T5: Stop and Close while the run waits closes the window with no file
/// written, even once capacity frees.
#[test]
fn test_stop_and_close_while_waiting_writes_nothing() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (panel, files) = preview_three_files(&window, dir.path());
    wait_for_journal_disk_idle(&panel);
    let fill = fill_lane_after_settling(&window);
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    assert_waiting(&panel);

    let dialog = close_during_run(&window);
    assert_eq!(
        dialog.body().as_str(),
        "Replace All is waiting for memory and has not changed any file yet.",
        "the question says the waiting run has written nothing"
    );
    dialog.emit_by_name::<()>("response", &[&"stop-and-close"]);
    dialog.force_close();
    // Stop ends the wait at once: no worker existed, so there is nothing to
    // roll back, and the close goes on into the ordinary close path.
    let stopped = panel.evidence();
    assert!(!stopped.replace_admission_deferred);
    assert!(!stopped.replace_in_flight);
    assert!(!stopped.replace_transaction_pending);
    assert_texts(&files, "needle\n");
    // The close's own persistence needs the lane too, so the window goes once
    // the other owner releases it; nothing is written meanwhile.
    drop(fill);
    wait_until(Duration::from_secs(30), || !window.is_visible());
    flush_after_delay(Duration::from_millis(500));
    assert_texts(&files, "needle\n");
}

/// T7: a file open before the fill and edited during the wait is skipped as
/// open with unsaved changes when the run starts; the others are replaced.
#[test]
fn test_a_file_edited_while_waiting_is_skipped_as_unsaved() {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (panel, files) = preview_three_files(&window, dir.path());
    let editor = open_loaded(&window, &files[0]);
    wait_for_journal_disk_idle(&panel);
    let fill = fill_lane_after_settling(&window);
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    assert_waiting(&panel);
    edit_unsaved(&editor);

    drop(fill);
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        evidence.last_apply_counts.is_some()
            && !evidence.replace_in_flight
            && !evidence.replace_transaction_pending
    });
    fixture::assert_text(&files[0], "needle\n");
    assert_texts(&files[1..], "thread\n");
    assert_eq!(skipped_for(&panel, ReplaceSkipReason::UnsavedChanges), 1);
    let report = panel.last_skip_report().expect("the run's skip report");
    assert_eq!(report.files()[0].path, files[0]);
    assert!(editor.is_modified(), "the tab keeps its edit");
}

/// T8: every file edited during the wait means nothing is written, and a
/// Close When Finished chosen during the wait re-enters the close path,
/// which asks Save Changes? for the edited tabs.
#[test]
fn test_every_file_edited_while_waiting_writes_nothing_and_close_resumes() {
    use lushtext_model::workspace::content_search::ReplaceSkipReason;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (panel, files) = preview_three_files(&window, dir.path());
    let editors: Vec<_> = files
        .iter()
        .map(|path| open_loaded(&window, path))
        .collect();
    wait_for_journal_disk_idle(&panel);
    let fill = fill_lane_after_settling(&window);
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    assert_waiting(&panel);
    for editor in &editors {
        edit_unsaved(editor);
    }
    let dialog = close_during_run(&window);
    dialog.emit_by_name::<()>("response", &[&"close-when-finished"]);
    dialog.force_close();
    flush_after_delay(Duration::from_millis(300));
    assert!(window.is_visible(), "the window waits for the run");

    drop(fill);
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        evidence.skipped_files_offered
            && !evidence.replace_in_flight
            && !evidence.replace_transaction_pending
    });
    assert_texts(&files, "needle\n");
    assert_eq!(skipped_for(&panel, ReplaceSkipReason::UnsavedChanges), 3);
    assert!(!panel.evidence().has_undo_backup);
    wait_until(Duration::from_secs(10), || {
        window
            .visible_dialog()
            .and_then(|dialog| dialog.downcast::<libadwaita::AlertDialog>().ok())
            .and_then(|dialog| dialog.heading())
            .is_some_and(|heading| heading == "Save Changes?")
    });
    assert!(window.is_visible());
    if let Some(dialog) = window.visible_dialog() {
        dialog.force_close();
    }
    flush_events();
}

/// T9: a capacity change that still leaves the lane short keeps the wait,
/// with its selection and transaction held.
#[test]
fn test_a_capacity_change_that_is_still_short_keeps_the_wait() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (panel, files) = preview_three_files(&window, dir.path());
    wait_for_journal_disk_idle(&panel);
    let fill = fill_lane_after_settling(&window);
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    assert_waiting(&panel);

    // One turn: the epoch moves, the lane stays full.
    drop(fill);
    let second_fill = lushtext_ui::ui::plain_disposal::fill_disposal_capacity_for_test();
    flush_after_delay(Duration::from_millis(500));
    assert_waiting(&panel);
    assert_texts(&files, "needle\n");

    drop(second_fill);
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        all_read(&files, "thread\n") && !evidence.replace_in_flight && evidence.has_undo_backup
    });
}

/// T10: destroying the window of a waiting run writes none of its files
/// once capacity frees, and frees the transaction for another window.
#[test]
fn test_destroying_a_waiting_window_writes_nothing_and_frees_the_transaction() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window_a = test_window();
    present_window(&window_a);
    crate::common::wait_for_startup_data_flow(&window_a);
    let window_b = second_window(&window_a);

    let dir_a = tempfile::tempdir().expect("expected operation to succeed");
    let file_a = dir_a.path().join("a.txt");
    fixture::write_text(&file_a, "needle\n");
    let panel_a = window_a.imp().search_panel.get();
    panel_a.set_workspace_folders(vec![dir_a.path().to_path_buf()]);
    panel_a.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel_a.imp().runtime.searching.get() && panel_a.imp().runtime.total_matches.get() == 1
    });
    panel_a.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel_a.evidence();
        evidence.replace_preview_count == 1 && !evidence.replace_preview_pending
    });
    let dir_b = tempfile::tempdir().expect("expected operation to succeed");
    let (panel_b, files_b) = preview_three_files(&window_b, dir_b.path());
    wait_for_journal_disk_idle(&panel_b);

    let fill = fill_lane_after_settling(&window_a);
    panel_b.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    let deferred = panel_b.evidence();
    assert!(deferred.replace_admission_deferred);
    assert!(deferred.replace_transaction_pending);

    let panel_b_weak = panel_b.downgrade();
    window_b.destroy();
    drop(panel_b);
    drop(window_b);
    wait_until(Duration::from_secs(20), || panel_b_weak.upgrade().is_none());

    drop(fill);
    panel_a.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        fixture::read_text(&file_a) == "thread\n" && !panel_a.evidence().replace_transaction_pending
    });
    flush_after_delay(Duration::from_millis(300));
    assert_texts(&files_b, "needle\n");
}

/// T10b: a disposed panel that something still holds releases its claimed
/// transaction, so another panel can claim it.
#[test]
fn test_dispose_releases_a_claimed_transaction_while_the_panel_lives() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    let other = glib::Object::builder::<LushtextSearchPanel>().build();
    wait_for_journal_disk_idle(&panel);
    wait_for_journal_disk_idle(&other);
    panel
        .begin_replace_transaction_for_test()
        .expect("an idle panel grants the transaction");
    assert!(
        other.begin_replace_transaction_for_test().is_none(),
        "the transaction is one per application"
    );

    // SAFETY: this standalone, never-parented test panel is disposed exactly
    // once, and afterwards only its evidence is read.
    unsafe { panel.run_dispose() };
    assert!(!panel.evidence().replace_transaction_pending);
    assert!(
        other.begin_replace_transaction_for_test().is_some(),
        "dispose released the disposed panel's transaction"
    );
    other.finish_replace_transaction_for_test();
}

// ---------------------------------------------------------------------------
// replace-all-confirm-disposal-lane, apply review: the wait never waits on
// what only its own admission frees, and no journal load races it
// ---------------------------------------------------------------------------

/// Descriptors of one file whose retained weight is about `weight` bytes:
/// the target path keeps that capacity, as a journal of many files would.
fn heavy_undo_descriptors(path: &str, weight: usize) -> ReplaceUndoBackup {
    sample_replace_descriptors(path)
        .into_iter()
        .map(|(target, entry)| {
            let mut heavy = PathBuf::with_capacity(weight);
            heavy.push(target);
            (heavy, entry)
        })
        .collect()
}

/// Apply review (code-review 1): a Replace All's undo reservation credits
/// every window's installed Undo journal, because its admission retires all
/// of them in the same turn. Two other windows' journals that leave no room
/// for the reservation beside them must not make the run wait for capacity
/// only that admission would free.
#[test]
fn test_a_run_credits_every_windows_undo_journal_it_retires() {
    const HEAVY_JOURNAL_BYTES: usize = 48 << 20;
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (panel, files) = preview_three_files(&window, dir.path());
    wait_for_journal_disk_idle(&panel);
    settle_file_index_builds(&window);
    let others: Vec<LushtextSearchPanel> = (0..2)
        .map(|index| {
            let other = glib::Object::builder::<LushtextSearchPanel>().build();
            wait_for_journal_disk_idle(&other);
            other.set_persisted_undo_backup(heavy_undo_descriptors(
                &format!("/heavy-{index}.txt"),
                HEAVY_JOURNAL_BYTES,
            ));
            assert!(other.has_undo_backup());
            other
        })
        .collect();

    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        all_read(&files, "thread\n")
            && evidence.has_undo_backup
            && !evidence.replace_in_flight
            && !evidence.replace_transaction_pending
    });
    for other in &others {
        assert!(
            !other.has_undo_backup(),
            "the admitted run retired every other window's Undo"
        );
    }
}

/// Apply review (code-review 2): the startup journal load stands down while
/// this panel's transaction is claimed, so it cannot advance the journal
/// generation a confirmed run reserved; it resumes when the transaction ends.
#[test]
fn test_persisted_undo_load_waits_for_a_claimed_transaction() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let data_dir = json_store::data_dir();
    let backup = sample_replace_backup("/persisted-claimed.rs");
    let capacity_hold = hold_disposal_capacity_for_test();
    let panel = glib::Object::builder::<LushtextSearchPanel>().build();
    wait_until(Duration::from_secs(2), || {
        panel.evidence().undo_capacity_retry_pending
    });
    let claimed = panel
        .begin_replace_transaction_for_test()
        .expect("an idle panel grants the transaction");

    drop(capacity_hold);
    flush_after_delay(Duration::from_millis(350));
    wait_for_journal_disk_idle(&panel);
    assert!(
        !panel.has_undo_backup(),
        "no journal is installed under a claimed transaction"
    );
    assert_eq!(
        panel.evidence().undo_backup_generation,
        claimed,
        "the claimed generation stays current"
    );
    assert!(
        panel.evidence().undo_capacity_retry_pending,
        "the load waits for the transaction to end"
    );

    panel.finish_replace_transaction_for_test();
    wait_until(Duration::from_secs(5), || {
        panel.has_undo_backup() && panel.evidence().undo_backup_matches(&backup)
    });
    assert!(!panel.evidence().undo_capacity_retry_pending);
    let _ = search_backup::delete(&data_dir);
}

/// Apply review (data-safety RS-3): a run's admission retires another
/// window's startup journal load that is still in flight, so that window
/// never offers the superseded journal as Undo.
#[test]
fn test_a_run_retires_another_windows_journal_load_in_flight() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    struct PolicyReset;
    impl Drop for PolicyReset {
        fn drop(&mut self) {
            SearchPanelTestPolicy::reset();
        }
    }
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let (panel, files) = preview_three_files(&window, dir.path());
    wait_for_journal_disk_idle(&panel);
    settle_file_index_builds(&window);

    // An earlier run's active journal, which a window opened now recovers;
    // its load holds what it read for two seconds before its completion.
    sample_replace_backup("/earlier-run.txt");
    let _reset = PolicyReset;
    SearchPanelTestPolicy::default()
        .with_undo_backup_disk_delay(Duration::from_secs(2))
        .install();
    let other = glib::Object::builder::<LushtextSearchPanel>().build();
    assert_eq!(other.evidence().journal_disk_jobs_in_flight, 1);

    panel.activate_confirm_replacements();
    wait_until(Duration::from_secs(20), || {
        let evidence = panel.evidence();
        all_read(&files, "thread\n")
            && evidence.has_undo_backup
            && !evidence.replace_in_flight
            && !evidence.replace_transaction_pending
    });
    wait_for_journal_disk_idle(&other);
    flush_events();
    assert!(
        !other.has_undo_backup(),
        "the journal the run superseded is not offered as Undo"
    );
}

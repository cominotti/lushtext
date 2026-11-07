// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for the command palette's "More" rows and category view
//! (`drill-down-palette-categories`, tasks 3.1–3.8): no visible loss, ranking
//! identity, leaving one level per Escape, freshness, no item walk,
//! accessibility, the uncounted note source, and More-row activation.

use crate::common::{
    ensure_gtk_init, fixture, flush_events, isolated_data_dir, present_window, wait_until,
};
use glib::prelude::ToValue;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::bookmark::BookmarkRecord;
use lushtext_core::model::palette::{
    IndexedFile, PaletteCategory, PaletteFileEntry, PaletteFileIdentity, PaletteNoteCategory,
    PaletteNoteEntry, PaletteNoteTarget, SearchMode,
};
use lushtext_core::model::responsiveness_budget::B_SETTLE_US;
use lushtext_core::services::palette::{FileIndex, MAX_INDEXED_FILES};
use lushtext_core::ui::command_palette::item::PaletteItem;
use lushtext_core::ui::command_palette::{CommandPaletteTestPolicy, LushtextCommandPalette};
use lushtext_core::ui::editor_page::LushtextEditorPage;
use lushtext_core::ui::window::LushtextWindow;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// GTK's realized-row cap for one list view (ledger A1: about 205).
const REALIZED_ROWS: u64 = 205;

/// Restores the palette's production posture on scope exit.
struct PolicyReset;

impl Drop for PolicyReset {
    fn drop(&mut self) {
        CommandPaletteTestPolicy::reset();
        lushtext_core::ui::window::set_notes_browser_budgets_for_test(0, 0);
    }
}

fn synthetic_index(count: usize) -> FileIndex {
    let root = Arc::new(PathBuf::from("/synthetic/category"));
    FileIndex::from(
        (0..count)
            .map(|index| {
                let path = root.join(format!("d{}/f-{index:06}.rs", index % 13));
                IndexedFile::new(
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                    Arc::clone(&root),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn open_tabs_for(index: &FileIndex, count: usize) -> Vec<PaletteFileEntry> {
    index.files()[..count]
        .iter()
        .map(|file| {
            PaletteFileEntry::new(
                file.name.clone(),
                file.relative_display(),
                file.path.clone(),
                file.identity.clone(),
            )
        })
        .collect()
}

fn bookmark_notes(count: usize) -> Vec<PaletteNoteEntry> {
    (0..count)
        .map(|index| PaletteNoteEntry {
            category: PaletteNoteCategory::Bookmarks,
            title: format!("f mark {index:04}"),
            subtitle: "notes.rs".to_string(),
            detail: None,
            note_text: None,
            target: PaletteNoteTarget::Bookmark {
                path: PathBuf::from("/synthetic/notes.rs"),
                line: u32::try_from(index).unwrap_or(u32::MAX),
                workspace_folders: Vec::new(),
            },
        })
        .collect()
}

fn emit_key(widget: &impl IsA<gtk4::Widget>, key: gtk4::gdk::Key, signal: &str) -> bool {
    let controllers = widget.as_ref().observe_controllers();
    for index in 0..controllers.n_items() {
        if let Some(controller) = controllers
            .item(index)
            .and_then(|object| object.downcast::<gtk4::EventControllerKey>().ok())
        {
            let args: [&dyn ToValue; 3] = [&key, &0u32, &gtk4::gdk::ModifierType::empty()];
            if signal == "key-released" {
                glib::object::ObjectExt::emit_by_name::<()>(&controller, signal, &args);
                return false;
            }
            return glib::object::ObjectExt::emit_by_name(&controller, signal, &args);
        }
    }
    panic!("widget had no EventControllerKey");
}

fn press(palette: &LushtextCommandPalette, key: gtk4::gdk::Key) -> bool {
    emit_key(&*palette.imp().search_entry, key, "key-pressed")
}

fn release(palette: &LushtextCommandPalette, key: gtk4::gdk::Key) {
    let _ = emit_key(&*palette.imp().search_entry, key, "key-released");
}

fn more_hidden(palette: &LushtextCommandPalette, category: PaletteCategory) -> Option<usize> {
    palette
        .evidence()
        .more_rows
        .into_iter()
        .find(|(candidate, _)| *candidate == category)
        .map(|(_, hidden)| hidden)
}

fn more_position(palette: &LushtextCommandPalette, category: PaletteCategory) -> u32 {
    let store = palette.imp().results_store.clone();
    (0..store.n_items())
        .find(|position| {
            store
                .item(*position)
                .and_downcast::<PaletteItem>()
                .is_some_and(|item| item.is_more() && item.category() == Some(category))
        })
        .expect("More row present")
}

fn select_grouped(palette: &LushtextCommandPalette, position: u32) {
    palette
        .imp()
        .results_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .expect("grouped selection")
        .set_selected(position);
}

fn select_category_row(palette: &LushtextCommandPalette, position: u32) {
    palette
        .imp()
        .category_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .expect("category selection")
        .set_selected(position);
}

/// Select `category`'s More row and press Enter in the query entry.
fn enter_by_more_row(palette: &LushtextCommandPalette, category: PaletteCategory) {
    wait_until(Duration::from_secs(20), || {
        more_hidden(palette, category).is_some()
    });
    select_grouped(palette, more_position(palette, category));
    palette.imp().search_entry.emit_activate();
    wait_until(Duration::from_secs(20), || {
        let evidence = palette.evidence();
        evidence.category == Some(category) && !evidence.searching && evidence.category_rows > 0
    });
}

/// Rows of one grouped category, as display names, in order.
fn grouped_rows(palette: &LushtextCommandPalette, category: PaletteCategory) -> Vec<String> {
    let store = palette.imp().results_store.clone();
    (0..store.n_items())
        .filter_map(|position| store.item(position).and_downcast::<PaletteItem>())
        .filter(|item| item.is_result() && item.category() == Some(category))
        .map(|item| format!("{}|{}", item.display_name(), item.subtitle()))
        .collect()
}

fn category_rows(palette: &LushtextCommandPalette, count: u32) -> Vec<String> {
    let model = palette.imp().category_model.clone();
    (0..count.min(model.len()))
        .filter_map(|position| model.item(position).and_downcast::<PaletteItem>())
        .map(|item| format!("{}|{}", item.display_name(), item.subtitle()))
        .collect()
}

fn quiet(palette: &LushtextCommandPalette) {
    wait_until(Duration::from_secs(20), || !palette.evidence().searching);
    flush_events();
}

#[test]
fn test_palette_category_no_visible_loss_at_the_index_ceiling() {
    ensure_gtk_init();
    let index = synthetic_index(MAX_INDEXED_FILES);
    let last_path = index.files()[MAX_INDEXED_FILES - 1].path.clone();
    for open in [0usize, 32] {
        let palette = LushtextCommandPalette::new();
        let opened = Rc::new(RefCell::new(None));
        let opened_clone = Rc::clone(&opened);
        palette.connect_item_activated(move |item| {
            opened_clone.replace(item.file_path());
        });
        palette.set_file_index(synthetic_index(MAX_INDEXED_FILES));
        palette.set_open_tabs(open_tabs_for(&index, open));
        palette.open();
        wait_until(Duration::from_secs(20), || {
            more_hidden(&palette, PaletteCategory::WorkspaceFiles).is_some()
        });
        assert_eq!(
            more_hidden(&palette, PaletteCategory::WorkspaceFiles),
            Some(MAX_INDEXED_FILES - open - 64),
            "the empty query states the exact hidden count with {open} open tabs"
        );

        enter_by_more_row(&palette, PaletteCategory::WorkspaceFiles);
        let len = palette.evidence().category_rows;
        assert_eq!(len as usize, MAX_INDEXED_FILES - open);
        assert!(!palette.evidence().category_limited);

        // Page Down reaches the last file and Enter opens it.
        select_category_row(&palette, len - 5);
        assert!(press(&palette, gtk4::gdk::Key::Page_Down));
        assert_eq!(palette.evidence().selected_position, Some(len - 1));
        palette.imp().search_entry.emit_activate();
        assert_eq!(opened.borrow().as_ref(), Some(&last_path));

        // A pointer double-click on a far row opens it.
        opened.replace(None);
        palette
            .imp()
            .category_view
            .emit_by_name::<()>("activate", &[&(len - 2)]);
        assert_eq!(
            opened.borrow().as_ref(),
            Some(&index.files()[MAX_INDEXED_FILES - 2].path)
        );
        palette.close();
    }
}

#[test]
fn test_palette_category_view_begins_with_the_grouped_rows() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let index = synthetic_index(4_000);
    palette.set_open_tabs(open_tabs_for(&index, 80));
    palette.set_file_index(index);
    palette.set_note_entries(bookmark_notes(300));
    palette.open();
    palette.set_query("f");
    quiet(&palette);
    for category in [
        PaletteCategory::OpenTabs,
        PaletteCategory::WorkspaceFiles,
        PaletteCategory::Note(PaletteNoteCategory::Bookmarks),
    ] {
        let grouped = grouped_rows(&palette, category);
        assert_eq!(grouped.len(), 64, "{category:?} is cut in All mode");
        enter_by_more_row(&palette, category);
        assert_eq!(
            category_rows(&palette, 64),
            grouped,
            "{category:?}: the category view begins with the grouped rows"
        );
        assert!(palette.leave_category());
        quiet(&palette);
    }
}

fn window_with_commands_cut(per_source: usize) -> LushtextWindow {
    CommandPaletteTestPolicy::current()
        .with_results_per_source(per_source)
        .install();
    let window = crate::common::test_window();
    window.new_tab();
    present_window(&window);
    window
}

fn activate_action(window: &LushtextWindow, name: &str) {
    ActionGroupExt::activate_action(window, name, None);
    flush_events();
}

fn activate_string_action(window: &LushtextWindow, name: &str, value: &str) {
    ActionGroupExt::activate_action(window, name, Some(&value.to_variant()));
    flush_events();
}

fn open_commands_category(window: &LushtextWindow) -> LushtextCommandPalette {
    activate_action(window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.set_search_mode(SearchMode::Commands);
    enter_by_more_row(&palette, PaletteCategory::Commands);
    palette
}

fn active_editor(window: &LushtextWindow) -> LushtextEditorPage {
    window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
        .expect("active editor")
}

#[test]
fn test_palette_category_one_escape_leaves_one_level() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let window = window_with_commands_cut(4);
    activate_action(&window, "toggle-search-panel");
    wait_until(Duration::from_secs(5), || {
        window.imp().search_panel_revealer.reveals_child()
    });
    let palette = open_commands_category(&window);

    // Escape in the entry: `stop-search` leaves, and the window's own
    // controller then sees the same key press and must do nothing more.
    palette.imp().search_entry.emit_stop_search();
    assert!(window.handle_transient_escape_for_test());
    flush_events();
    assert_eq!(palette.category(), None);
    assert!(
        window.imp().palette_revealer.reveals_child(),
        "the first Escape leaves the category and keeps the palette open"
    );
    assert!(window.imp().search_panel_revealer.reveals_child());
    quiet(&palette);
    assert_eq!(
        palette.evidence().selected_more,
        Some(PaletteCategory::Commands),
        "the More row is selected again"
    );

    // A second Escape closes the palette, and only the palette.
    palette.imp().search_entry.emit_stop_search();
    assert!(window.handle_transient_escape_for_test());
    flush_events();
    assert!(!window.imp().palette_revealer.reveals_child());
    assert!(window.imp().search_panel_revealer.reveals_child());
}

#[test]
fn test_palette_category_escape_from_the_editor_and_the_list_leaves() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let window = window_with_commands_cut(4);
    let palette = open_commands_category(&window);
    palette.set_query("");
    quiet(&palette);

    // Focus in the editor: the window's ladder leaves the category.
    active_editor(&window).source_view().grab_focus();
    assert!(window.handle_transient_escape_for_test());
    flush_events();
    assert_eq!(palette.category(), None);
    assert!(window.imp().palette_revealer.reveals_child());

    // Focus in the category list: the same rung.
    assert!(palette.show_category(PaletteCategory::Commands));
    quiet(&palette);
    palette.imp().category_view.grab_focus();
    assert!(window.handle_transient_escape_for_test());
    flush_events();
    assert_eq!(palette.category(), None);
    assert!(window.imp().palette_revealer.reveals_child());
    assert!(window.handle_transient_escape_for_test());
    assert!(!window.imp().palette_revealer.reveals_child());
}

#[test]
fn test_palette_category_backspace_back_button_and_tab_leave_with_the_query_kept() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let window = window_with_commands_cut(4);
    let palette = open_commands_category(&window);

    // Backspace on the query that was already empty.
    assert!(press(&palette, gtk4::gdk::Key::BackSpace));
    assert_eq!(palette.category(), None);
    quiet(&palette);
    assert_eq!(
        palette.evidence().selected_more,
        Some(PaletteCategory::Commands)
    );

    // The back button.
    assert!(palette.show_category(PaletteCategory::Commands));
    quiet(&palette);
    palette.imp().category_back_button.emit_clicked();
    assert_eq!(palette.category(), None);

    // Tab changes the mode and leaves, keeping the query.
    palette.set_query("o");
    quiet(&palette);
    assert!(palette.show_category(PaletteCategory::Commands));
    quiet(&palette);
    assert!(press(&palette, gtk4::gdk::Key::Tab));
    assert_eq!(palette.category(), None);
    assert_eq!(palette.mode(), SearchMode::All);
    assert_eq!(palette.query(), "o");
}

#[test]
fn test_palette_category_holding_backspace_to_clear_the_query_stays() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let window = window_with_commands_cut(1);
    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.set_search_mode(SearchMode::Commands);
    palette.set_query("rs");
    quiet(&palette);
    enter_by_more_row(&palette, PaletteCategory::Commands);

    // Auto-repeat: presses without a release while the query empties.
    assert!(!press(&palette, gtk4::gdk::Key::BackSpace));
    palette.imp().search_entry.set_text("r");
    assert!(!press(&palette, gtk4::gdk::Key::BackSpace));
    palette.imp().search_entry.set_text("");
    assert!(
        !press(&palette, gtk4::gdk::Key::BackSpace),
        "a repeat on the now-empty query does not leave"
    );
    assert!(!press(&palette, gtk4::gdk::Key::BackSpace));
    assert_eq!(palette.category(), Some(PaletteCategory::Commands));

    // Release, then press: leaves.
    release(&palette, gtk4::gdk::Key::BackSpace);
    assert!(press(&palette, gtk4::gdk::Key::BackSpace));
    assert_eq!(palette.category(), None);
}

#[test]
fn test_palette_category_completions_publish_only_into_their_view() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let palette = LushtextCommandPalette::new();
    palette.set_file_index(synthetic_index(500));
    palette.open();
    quiet(&palette);

    // A category completion arriving after leaving publishes nothing.
    CommandPaletteTestPolicy::current()
        .with_search_worker_delay(Duration::from_millis(300))
        .install();
    assert!(palette.show_category(PaletteCategory::WorkspaceFiles));
    assert!(palette.leave_category());
    quiet(&palette);
    assert_eq!(palette.evidence().category_rows, 0);
    assert!(!palette.evidence().category_holds_sources);
    assert!(palette.evidence().more_rows.len() == 1);

    // A grouped completion arriving after entering publishes nothing.
    let grouped_before = palette.imp().results_store.n_items();
    palette.set_query("f-00001");
    assert!(palette.show_category(PaletteCategory::WorkspaceFiles));
    quiet(&palette);
    assert_eq!(palette.imp().results_store.n_items(), grouped_before);
    let evidence = palette.evidence();
    assert!(evidence.category_rows > 0);
    assert_eq!(evidence.category_rows as usize, evidence.category_total);
    CommandPaletteTestPolicy::reset();

    // An index replacement republishes the shown category.
    palette.set_query("");
    wait_until(Duration::from_secs(10), || {
        palette.evidence().category_total == 500 && !palette.evidence().searching
    });
    palette.set_file_index(synthetic_index(900));
    wait_until(Duration::from_secs(10), || {
        palette.evidence().category_total == 900 && !palette.evidence().searching
    });
    assert_eq!(palette.category(), Some(PaletteCategory::WorkspaceFiles));
}

#[test]
fn test_palette_category_view_creates_rows_only_around_the_realized_ones() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let window = gtk4::Window::new();
    window.set_default_size(640, 480);
    window.set_child(Some(&palette));
    present_window(&window);
    palette.set_file_index(synthetic_index(MAX_INDEXED_FILES));
    palette.open();
    enter_by_more_row(&palette, PaletteCategory::WorkspaceFiles);
    palette.set_query("f");
    quiet(&palette);
    for _ in 0..3 {
        assert!(press(&palette, gtk4::gdk::Key::Page_Down));
        assert!(press(&palette, gtk4::gdk::Key::Down));
    }
    palette.refresh_accessibility_state_for_test();
    flush_events();
    let evidence = palette.evidence();
    assert_eq!(evidence.category_rows as usize, MAX_INDEXED_FILES);
    assert!(
        evidence.category_item_requests <= 4 * REALIZED_ROWS,
        "{} row objects for {} rows",
        evidence.category_item_requests,
        evidence.category_rows
    );
    // Leaving releases the sources the projection held, from a low-priority
    // idle after the leave's own dispatch.
    assert!(palette.leave_category());
    wait_until(Duration::from_secs(5), || {
        !palette.evidence().category_holds_sources
    });
    palette.close();
    window.close();
}

#[test]
fn test_palette_category_accessibility_names_and_announcements() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let palette = LushtextCommandPalette::new();
    palette.set_file_index(synthetic_index(100));
    palette.open();
    wait_until(Duration::from_secs(10), || {
        more_hidden(&palette, PaletteCategory::WorkspaceFiles) == Some(36)
    });
    let more = palette
        .imp()
        .results_store
        .item(more_position(&palette, PaletteCategory::WorkspaceFiles))
        .and_downcast::<PaletteItem>()
        .expect("More row");
    assert_eq!(more.display_name(), "36 more…");
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    lushtext_core::ui::command_palette::apply_palette_row_accessibility_for_test(
        &row, &more, true, 66, 66,
    );
    lushtext_core::ui::accessibility::test_audit::AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&row);

    CommandPaletteTestPolicy::current()
        .with_category_row_limit(10)
        .install();
    enter_by_more_row(&palette, PaletteCategory::WorkspaceFiles);
    for query in ["f", "f-", "f-0"] {
        palette.set_query(query);
        quiet(&palette);
    }
    let evidence = palette.evidence();
    assert_eq!(
        evidence.category_entry_announcements, 1,
        "one entry announcement"
    );
    assert!(evidence.category_limited);
    assert_eq!(
        evidence.category_notice.as_deref(),
        Some("Showing 10 of 100 results")
    );
    assert_eq!(
        evidence.category_notice_announcements, 1,
        "the unchanged cut is announced once"
    );
    lushtext_core::ui::accessibility::test_audit::AccessibleAudit::new()
        .role(gtk4::AccessibleRole::List)
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&*palette.imp().category_view);
    lushtext_core::ui::accessibility::test_audit::AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&*palette.imp().category_back_button);
}

#[test]
fn test_palette_more_row_activation_stays_in_the_palette() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let window = window_with_commands_cut(4);
    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    // The window's activate callback opens or runs the row and then closes
    // the palette, so a palette still revealed after the activation proves
    // the More row never reached it.
    palette.set_search_mode(SearchMode::Commands);
    enter_by_more_row(&palette, PaletteCategory::Commands);
    assert!(
        window.imp().palette_revealer.reveals_child(),
        "activating a More row keeps the palette open"
    );

    // Click-away from the category view closes the palette once.
    let bounds = palette
        .compute_bounds(&window)
        .expect("palette bounds in window");
    let x = f64::from(bounds.x() + bounds.width() + 4.0).min(f64::from(window.width() - 2));
    let y = f64::from(window.height() - 4);
    assert!(window.handle_command_palette_pointer_press_for_test(x, y));
    flush_events();
    assert!(!window.imp().palette_revealer.reveals_child());
    assert_eq!(palette.category(), None);
    activate_action(&window, "toggle-command-palette");
    assert_eq!(palette.category(), None, "reopening shows the grouped view");
}

#[test]
fn test_palette_category_action_opens_refuses_and_leaves() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let window = window_with_commands_cut(4);
    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.set_search_mode(SearchMode::Commands);
    quiet(&palette);
    activate_string_action(&window, "set-command-palette-category", "nonexistent");
    assert_eq!(palette.category(), None);
    activate_string_action(&window, "set-command-palette-category", "workspace-files");
    assert_eq!(palette.category(), None, "not a group of Commands mode");
    activate_string_action(&window, "set-command-palette-category", "commands");
    quiet(&palette);
    let evidence = palette.evidence();
    assert_eq!(evidence.category, Some(PaletteCategory::Commands));
    assert_eq!(
        evidence.category_rows as usize, evidence.category_total,
        "every command listed"
    );
    activate_string_action(&window, "set-command-palette-category", "");
    assert_eq!(palette.category(), None);
    let evidence = palette.evidence();
    assert_eq!((evidence.category_rows, evidence.category_total), (0, 0));
}

#[test]
fn test_palette_note_capture_is_sliced_and_uncounted() {
    ensure_gtk_init();
    let _reset = PolicyReset;
    let _data_dir = isolated_data_dir();
    let files = tempfile::tempdir().expect("palette capture tempdir");
    let path = files.path().join("marks.rs");
    fixture::write_text(&path, &"line\n".repeat(20_001));
    let window = crate::common::test_window();
    present_window(&window);
    window.open_document(&path);
    wait_until(Duration::from_secs(10), || {
        active_editor(&window).file_path().as_deref() == Some(path.as_path())
            && active_editor(&window).load_state()
                == lushtext_core::ui::editor_page::EditorLoadState::Loaded
    });
    let bookmarks: Vec<_> = (0..20_000)
        .map(|line| BookmarkRecord::new(line, None))
        .collect();
    active_editor(&window).load_bookmarks(&bookmarks);
    // Four slices a turn: the capture of 20,000 bookmarks takes many turns.
    lushtext_core::ui::window::set_notes_browser_budgets_for_test(0, 4);

    activate_action(&window, "toggle-command-palette");
    let palette = window.imp().command_palette.clone();
    palette.set_search_mode(SearchMode::Notes);
    wait_until(Duration::from_secs(30), || {
        let more: usize = palette
            .evidence()
            .more_rows
            .iter()
            .map(|(_, hidden)| *hidden)
            .sum();
        more + 64 >= 20_000
    });
    let capture = window.command_palette_note_capture_evidence();
    assert!(capture.turns > 1, "the capture ran in turns: {capture:?}");
    assert_eq!(capture.rows_counted_not_retained, 0, "4 MiB holds them all");
    assert!(
        capture.worst_turn_us <= 2 * B_SETTLE_US,
        "worst capture turn {} µs",
        capture.worst_turn_us
    );
}

#[test]
fn test_palette_category_bar_and_list_fit_a_narrow_overlay() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let window = gtk4::Window::new();
    window.set_default_size(360, 520);
    window.set_child(Some(&palette));
    present_window(&window);
    palette.set_workspace_group_label(
        "A Workspace With A Very Long Name That Would Not Fit A Narrow Palette",
    );
    palette.set_file_index(synthetic_index(400));
    palette.open();
    enter_by_more_row(&palette, PaletteCategory::WorkspaceFiles);
    crate::common::flush_after_delay(Duration::from_millis(200));
    let width = palette.width();
    let imp = palette.imp();
    for (name, widget) in [
        (
            "category bar",
            imp.category_bar.clone().upcast::<gtk4::Widget>(),
        ),
        ("category scroller", imp.category_scroll.clone().upcast()),
        ("back button", imp.category_back_button.clone().upcast()),
        ("title", imp.category_title_label.clone().upcast()),
    ] {
        let bounds = widget
            .compute_bounds(&palette)
            .unwrap_or_else(|| panic!("{name} has bounds"));
        assert!(
            bounds.x() >= 0.0 && f64::from(bounds.x() + bounds.width()) <= f64::from(width) + 0.5,
            "{name} fits the palette's {width} px: {bounds:?}"
        );
        assert!(
            widget.width() > 0 && widget.height() > 0,
            "{name} is allocated"
        );
    }
    assert!(
        imp.category_scroll.height() <= 400,
        "max-content-height bounds the category list"
    );
    assert!(
        !imp.results_scroll.is_visible(),
        "the grouped list is hidden"
    );
    window.close();
}

/// The worst main-loop dispatch while `window` of wall time passes.
fn worst_dispatch_for(window: Duration) -> Duration {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + window;
    let mut worst = Duration::ZERO;
    while std::time::Instant::now() < deadline {
        let start = std::time::Instant::now();
        if context.iteration(false) {
            worst = worst.max(start.elapsed());
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    worst
}

/// Recorded toolkit-floor exception X1 (`drill-down-palette-categories`,
/// design "Implementation decisions"): the frame after the first category
/// publish realizes the about 205 category rows (A1, A90) and measured
/// 13.2–16.6 ms at 512 to 131,072 rows, flat in rows. Bounded at twice its
/// measured worst; every input dispatch (Enter on the More row, Escape) stays
/// within `B_INTERACT_US`, and the leave's deferred clear within
/// `B_SETTLE_US`. Enforced on the optimized widget build only.
#[test]
fn test_palette_category_dispatches_stay_within_their_budgets() {
    use lushtext_core::model::responsiveness_budget::B_INTERACT_US;
    const X1_BOUND: Duration = Duration::from_micros(2 * 16_600);
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let window = gtk4::Window::new();
    window.set_default_size(900, 700);
    window.set_child(Some(&palette));
    present_window(&window);
    palette.set_file_index(synthetic_index(MAX_INDEXED_FILES));
    palette.open();
    palette.set_search_mode(SearchMode::Files);
    wait_until(Duration::from_secs(30), || {
        more_hidden(&palette, PaletteCategory::WorkspaceFiles).is_some()
    });
    select_grouped(
        &palette,
        more_position(&palette, PaletteCategory::WorkspaceFiles),
    );
    let start = std::time::Instant::now();
    palette.imp().search_entry.emit_activate();
    let enter_input = start.elapsed();
    let (held, publish) = crate::common::worst_dispatch_until(Duration::from_secs(30), || {
        let evidence = palette.evidence();
        evidence.category.is_some() && !evidence.searching && evidence.category_rows > 0
    });
    assert!(held, "the category view published");
    let frame = worst_dispatch_for(Duration::from_millis(200));
    let start = std::time::Instant::now();
    palette.imp().search_entry.emit_stop_search();
    let escape_input = start.elapsed();
    let leave = worst_dispatch_for(Duration::from_millis(300));
    println!(
        "palette-category-budgets enter_input_us={} publish_us={} frame_us={} escape_input_us={} leave_worst_us={}",
        enter_input.as_micros(),
        publish.as_micros(),
        frame.as_micros(),
        escape_input.as_micros(),
        leave.as_micros()
    );
    assert!(!palette.evidence().category_holds_sources);
    if crate::common::timing_verdicts_enforced() {
        let interact = Duration::from_micros(B_INTERACT_US);
        let settle = Duration::from_micros(B_SETTLE_US);
        assert!(
            enter_input <= interact,
            "Enter on a More row: {enter_input:?}"
        );
        assert!(escape_input <= interact, "Escape: {escape_input:?}");
        assert!(publish <= settle, "the first category publish: {publish:?}");
        assert!(frame <= X1_BOUND, "X1, the frame after it: {frame:?}");
        assert!(
            leave <= settle,
            "the leave and its deferred clear: {leave:?}"
        );
    }
    palette.close();
    window.close();
}

/// BN's crash regression shape on the category model: repeated publishes
/// that shrink and grow the list while GTK holds realized rows, each emission
/// matching the length the model reports, and the retargeted rows rebinding
/// in place (no warning, the rows read the new ranking).
#[test]
fn test_palette_category_republishes_over_realized_rows() {
    ensure_gtk_init();
    let palette = LushtextCommandPalette::new();
    let window = gtk4::Window::new();
    window.set_default_size(640, 480);
    window.set_child(Some(&palette));
    present_window(&window);
    palette.set_file_index(synthetic_index(2_000));
    palette.open();
    enter_by_more_row(&palette, PaletteCategory::WorkspaceFiles);
    for query in [
        "f-0001", "f-00", "f-000019", "", "f-001", "f-0001", "zzzz", "f",
    ] {
        palette.set_query(query);
        quiet(&palette);
        crate::common::flush_after_delay(Duration::from_millis(60));
        let evidence = palette.evidence();
        assert_eq!(evidence.category, Some(PaletteCategory::WorkspaceFiles));
        assert_eq!(
            evidence.category_rows as usize, evidence.category_total,
            "{query:?}"
        );
        if evidence.category_rows > 0 {
            let first = palette
                .imp()
                .category_model
                .item(0)
                .and_downcast::<PaletteItem>()
                .expect("row 0");
            assert!(first.is_file(), "{query:?}");
        }
    }
    palette.close();
    window.close();
}

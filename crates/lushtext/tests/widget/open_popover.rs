// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget coverage for the GNOME-style Open popover.

use crate::common::{ensure_gtk_init, fixture, flush_events, present_window, test_window, wait_until};
use glib::object::ObjectExt;
use glib::prelude::Cast;
use glib::prelude::ToValue;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::recent_document::{RecentDocumentEntry, RecentDocumentRow};
use lushtext_core::ui::open_popover::LushtextOpenPopover;
use lushtext_core::ui::window::LushtextWindow;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

fn recent_entry(path: impl Into<PathBuf>, secs: u64) -> RecentDocumentEntry {
    RecentDocumentEntry::new(path.into(), None, secs)
}

fn row(path: impl Into<PathBuf>, secs: u64) -> RecentDocumentRow {
    RecentDocumentRow::from_entry(&recent_entry(path, secs), secs)
}

fn open_recent_action(window: &LushtextWindow) {
    wait_until(Duration::from_secs(2), || {
        window.imp().open_menu_button.is_mapped()
    });
    gtk4::prelude::ActionGroupExt::activate_action(window, "open-recent", None);
    flush_events();
}

fn open_popover_open(window: &LushtextWindow) -> bool {
    window.imp().open_menu_button.is_active() || window.imp().open_popover.is_visible()
}

fn focus_is_inside_open_search(window: &LushtextWindow) -> bool {
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return false;
    };
    let search = window.imp().open_popover.search_entry_for_test();
    let search = search.upcast_ref::<gtk4::Widget>();
    focus.as_ptr() == search.as_ptr() || focus.is_ancestor(search)
}

fn focus_is_inside_widget(window: &LushtextWindow, target: &impl IsA<gtk4::Widget>) -> bool {
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return false;
    };
    let target = target.as_ref();
    focus.as_ptr() == target.as_ptr() || focus.is_ancestor(target)
}

fn shortcut_bound(window: &LushtextWindow, action_name: &str, trigger_string: &str) -> bool {
    let expected_trigger = gtk4::ShortcutTrigger::parse_string(trigger_string)
        .unwrap_or_else(|| panic!("shortcut trigger '{trigger_string}' should parse"));
    let expected_trigger = expected_trigger.to_str();
    let controllers = window.observe_controllers();
    let shortcut_controller = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|object| object.downcast::<gtk4::ShortcutController>().ok())
        .find(|controller| controller.scope() == gtk4::ShortcutScope::Managed)
        .expect("window should install a managed shortcut controller");

    (0..shortcut_controller.n_items())
        .filter_map(|index| shortcut_controller.item(index))
        .filter_map(|object| object.downcast::<gtk4::Shortcut>().ok())
        .any(|shortcut| {
            let action_matches = shortcut
                .action()
                .and_then(|action| action.downcast::<gtk4::NamedAction>().ok())
                .is_some_and(|action| action.action_name().as_str() == action_name);
            let trigger_matches = shortcut
                .trigger()
                .is_some_and(|trigger| trigger.to_str() == expected_trigger);
            action_matches && trigger_matches
        })
}

fn assert_header_open_precedes_new_tab(window: &LushtextWindow, context: &str) {
    let header_bar = window.imp().header_bar.upcast_ref::<gtk4::Widget>();
    let open_button = window.imp().open_menu_button.upcast_ref::<gtk4::Widget>();
    let new_button = window.imp().new_tab_button.upcast_ref::<gtk4::Widget>();

    wait_until(Duration::from_secs(2), || {
        open_button.compute_bounds(header_bar).is_some()
            && new_button.compute_bounds(header_bar).is_some()
    });

    let open_bounds = open_button
        .compute_bounds(header_bar)
        .expect("Open button should have header-relative bounds");
    let new_bounds = new_button
        .compute_bounds(header_bar)
        .expect("New file button should have header-relative bounds");
    let open_right = open_bounds.x() + open_bounds.width();
    let new_right = new_bounds.x() + new_bounds.width();
    let header_width = header_bar.width() as f32;

    assert!(
        open_bounds.width() > 0.0
            && open_bounds.height() > 0.0
            && new_bounds.width() > 0.0
            && new_bounds.height() > 0.0,
        "{context}: Open and New controls should both have positive allocations, open={open_bounds:?}, new={new_bounds:?}",
    );
    assert!(
        open_bounds.x() >= 0.0 && new_right <= header_width,
        "{context}: header start controls should stay inside the header bar, header_width={header_width}, open={open_bounds:?}, new={new_bounds:?}",
    );
    assert!(
        open_right <= new_bounds.x(),
        "{context}: Open should render before New File without overlap, open={open_bounds:?}, new={new_bounds:?}",
    );
}

fn active_file_path(window: &LushtextWindow) -> Option<PathBuf> {
    window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| page.child().downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>().ok())
        .and_then(|editor| editor.file_path())
}

fn descendants(root: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Widget> {
    let mut widgets = Vec::new();
    let root = root.as_ref();
    let mut child = root.first_child();
    while let Some(widget) = child {
        widgets.push(widget.clone());
        widgets.extend(descendants(&widget));
        child = widget.next_sibling();
    }
    widgets
}

fn emit_key_until_handled(
    widget: &impl IsA<gtk4::Widget>,
    key: gtk4::gdk::Key,
) -> glib::Propagation {
    let controllers = widget.as_ref().observe_controllers();
    let mut saw_key_controller = false;
    for index in 0..controllers.n_items() {
        let Some(controller) = controllers
            .item(index)
            .and_then(|object| object.downcast::<gtk4::EventControllerKey>().ok())
        else {
            continue;
        };
        saw_key_controller = true;
        let args: [&dyn ToValue; 3] = [&key, &0u32, &gtk4::gdk::ModifierType::empty()];
        let stopped: bool =
            glib::object::ObjectExt::emit_by_name(&controller, "key-pressed", &args);
        if stopped {
            return glib::Propagation::Stop;
        }
    }
    if saw_key_controller {
        glib::Propagation::Proceed
    } else {
        panic!("widget had no EventControllerKey");
    }
}

#[test]
fn test_open_header_precedes_new_tab_in_wide_and_compact_presentations() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1280, 720);
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().open_button_stack.visible_child_name().as_deref() == Some("wide")
    });
    assert_header_open_precedes_new_tab(&window, "wide Open label presentation");

    window.imp().open_button_stack.set_visible_child_name("narrow");
    flush_events();
    assert_eq!(
        window.imp().open_button_stack.visible_child_name().as_deref(),
        Some("narrow")
    );
    assert_header_open_precedes_new_tab(&window, "compact folder-icon Open presentation");
}

#[test]
fn test_open_popover_empty_state_keeps_search_and_chooser_reachable() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();

    popover.set_recent_rows(Vec::new());
    popover.prepare_to_show();

    assert_eq!(popover.visible_row_count_for_test(), 0);
    assert!(!popover.list_visible_for_test());
    assert_eq!(
        popover.search_entry_for_test().accessible_role(),
        gtk4::AccessibleRole::SearchBox
    );
    assert_eq!(
        popover.chooser_button_for_test().accessible_role(),
        gtk4::AccessibleRole::Button
    );
}

#[test]
fn test_open_popover_one_representative_row_uses_file_title() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();

    popover.set_recent_rows(vec![row("/tmp/project/src/main.rs", 10)]);

    assert_eq!(popover.visible_titles_for_test(), vec!["main.rs"]);
    assert!(popover.list_visible_for_test());
}

#[test]
fn test_open_popover_search_filters_and_reports_no_matches_without_fake_rows() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    popover.set_recent_rows(vec![
        row("/tmp/project/src/main.rs", 30),
        row("/tmp/project/README.md", 20),
        row("/tmp/project/notes.txt", 10),
    ]);

    popover.set_search_text_for_test("read");
    assert_eq!(popover.visible_titles_for_test(), vec!["README.md"]);

    popover.set_search_text_for_test("does-not-exist");
    assert_eq!(popover.visible_row_count_for_test(), 0);
    assert!(!popover.list_visible_for_test());
}

#[test]
fn test_open_popover_ten_row_viewport_contract_and_eleventh_stays_model_backed() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    let rows = (0..11)
        .map(|idx| row(format!("/tmp/recent/file-{idx}.txt"), 100 - idx))
        .collect();

    popover.set_recent_rows(rows);

    assert_eq!(popover.visible_row_count_for_test(), 11);
    assert_eq!(popover.list_max_content_height_for_test(), 10 * 54);
}

#[test]
fn test_open_popover_search_finds_rows_beyond_initial_ten() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    let mut rows: Vec<_> = (0..12)
        .map(|idx| row(format!("/tmp/recent/file-{idx}.txt"), 100 - idx))
        .collect();
    rows.push(row("/tmp/recent/deep-target.md", 1));
    popover.set_recent_rows(rows);

    popover.set_search_text_for_test("target");

    assert_eq!(popover.visible_titles_for_test(), vec!["deep-target.md"]);
}

#[test]
fn test_open_popover_prepare_clears_stale_search_and_resets_list_scroll() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    let rows = (0..15)
        .map(|idx| row(format!("/tmp/recent/file-{idx}.txt"), 100 - idx))
        .collect();
    popover.set_recent_rows(rows);

    popover.set_search_text_for_test("file-13");
    popover.set_list_scroll_value_for_test(120.0);
    assert_eq!(popover.visible_titles_for_test(), vec!["file-13.txt"]);
    assert!(popover.list_scroll_value_for_test() > 0.0);

    popover.prepare_to_show();

    assert_eq!(popover.search_entry_for_test().text().as_str(), "");
    assert_eq!(popover.list_scroll_value_for_test(), 0.0);
    assert_eq!(popover.visible_titles_for_test()[0], "file-0.txt");
}

#[test]
fn test_open_popover_enter_activates_first_filtered_match() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    let first = PathBuf::from("/tmp/recent/alpha.txt");
    let target = PathBuf::from("/tmp/recent/target.txt");
    popover.set_recent_rows(vec![row(first, 20), row(target.as_path(), 10)]);
    let activated = Rc::new(RefCell::new(None));
    let activated_for_cb = Rc::clone(&activated);
    popover.connect_recent_activated(move |path| {
        activated_for_cb.replace(Some(path));
    });

    popover.set_search_text_for_test("target");
    popover
        .search_entry_for_test()
        .emit_by_name::<()>("activate", &[]);
    flush_events();

    assert_eq!(activated.borrow().as_ref(), Some(&target));
    assert_eq!(popover.search_entry_for_test().text().as_str(), "");
}

#[test]
fn test_open_recent_action_opens_popover_and_focuses_search() {
    ensure_gtk_init();
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry("/tmp/example.txt", 10)]);
    present_window(&window);

    open_recent_action(&window);

    wait_until(Duration::from_secs(2), || {
        open_popover_open(&window) && focus_is_inside_open_search(&window)
    });
}

#[test]
fn test_open_popover_down_and_up_move_focus_between_search_and_first_row() {
    ensure_gtk_init();
    let window = test_window();
    window.set_recent_documents_for_test(vec![
        recent_entry("/tmp/first.txt", 20),
        recent_entry("/tmp/second.txt", 10),
    ]);
    present_window(&window);
    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || focus_is_inside_open_search(&window));

    assert_eq!(
        emit_key_until_handled(
            &window.imp().open_popover.search_entry_for_test(),
            gtk4::gdk::Key::Down
        ),
        glib::Propagation::Stop
    );
    flush_events();
    wait_until(Duration::from_secs(2), || {
        focus_is_inside_widget(&window, &window.imp().open_popover.list_view_for_test())
    });

    assert_eq!(
        emit_key_until_handled(
            &window.imp().open_popover.list_view_for_test(),
            gtk4::gdk::Key::Up
        ),
        glib::Propagation::Stop
    );
    flush_events();
    wait_until(Duration::from_secs(2), || focus_is_inside_open_search(&window));
}

#[test]
fn test_open_popover_escape_dismisses_without_document_context() {
    ensure_gtk_init();
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry("/tmp/example.txt", 10)]);
    present_window(&window);
    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || open_popover_open(&window));

    window
        .imp()
        .open_popover
        .search_entry_for_test()
        .emit_by_name::<()>("stop-search", &[]);
    flush_events();

    wait_until(Duration::from_secs(2), || !open_popover_open(&window));
    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_open_popover_activation_uses_normal_document_workflow_and_closes_once() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("recent activation tempdir");
    let path = dir.path().join("chosen.txt");
    fixture::write_text(&path, "opened from recent\n");
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry(path.clone(), 10)]);
    present_window(&window);
    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || open_popover_open(&window));

    window
        .imp()
        .open_popover
        .list_view_for_test()
        .emit_by_name::<()>("activate", &[&0u32]);
    flush_events();

    wait_until(Duration::from_secs(2), || active_file_path(&window) == Some(path.clone()));
    assert!(!open_popover_open(&window));
}

#[test]
fn test_open_popover_hides_already_open_documents_and_reveals_after_close() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("recent exclusion tempdir");
    let open_path = dir.path().join("open.txt");
    let closed_path = dir.path().join("closed.txt");
    fixture::write_text(&open_path, "open\n");
    fixture::write_text(&closed_path, "closed\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&open_path);
    wait_until(Duration::from_secs(2), || active_file_path(&window) == Some(open_path.clone()));

    window.set_recent_documents_for_test(vec![
        recent_entry(open_path, 20),
        recent_entry(closed_path, 10),
    ]);
    assert_eq!(window.imp().open_popover.visible_titles_for_test(), vec!["closed.txt"]);

    gtk4::prelude::ActionGroupExt::activate_action(&window, "close-tab", None);
    flush_events();
    wait_until(Duration::from_secs(2), || window.imp().tab_view.n_pages() == 0);

    assert_eq!(
        window.imp().open_popover.visible_titles_for_test(),
        vec!["open.txt", "closed.txt"]
    );
}

#[test]
fn test_open_popover_row_remove_keeps_popover_open_and_does_not_open_document() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("recent remove tempdir");
    let path = dir.path().join("remove-me.txt");
    fixture::write_text(&path, "remove\n");
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry(path, 10)]);
    present_window(&window);
    open_recent_action(&window);
    let popover = window.imp().open_popover.clone();
    wait_until(Duration::from_secs(2), || {
        descendants(&popover)
            .into_iter()
            .any(|widget| widget.downcast_ref::<gtk4::Button>().is_some_and(|button| {
                button.tooltip_text().as_deref() == Some("Remove")
            }))
    });

    let remove = descendants(&popover)
        .into_iter()
        .find_map(|widget| {
            widget.downcast::<gtk4::Button>().ok().filter(|button| {
                button.tooltip_text().as_deref() == Some("Remove")
            })
        })
        .expect("remove button");
    remove.emit_clicked();
    flush_events();

    assert!(open_popover_open(&window));
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert!(window.recent_documents_for_test().is_empty());
}

#[test]
fn test_open_popover_file_chooser_button_invokes_callback_without_recent_activation() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    let called = Rc::new(Cell::new(false));
    let called_for_cb = Rc::clone(&called);
    popover.connect_open_file_requested(move || called_for_cb.set(true));

    popover.chooser_button_for_test().emit_clicked();

    assert!(called.get());
    assert_eq!(popover.visible_row_count_for_test(), 0);
}

#[test]
fn test_open_and_new_shortcuts_keep_ctrl_o_ctrl_k_and_ctrl_n_bindings() {
    ensure_gtk_init();
    let window = test_window();

    assert!(window.lookup_action("new-tab").is_some());
    assert!(window.lookup_action("open-file").is_some());
    assert!(window.lookup_action("open-recent").is_some());
    assert!(shortcut_bound(&window, "win.new-tab", "<Control>n"));
    assert!(shortcut_bound(&window, "win.open-file", "<Control>o"));
    assert!(shortcut_bound(&window, "win.open-recent", "<Control>k"));

    let shortcuts_ui = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/ui/shortcuts.ui"
    ));
    assert!(shortcuts_ui.contains("Open File"));
    assert!(shortcuts_ui.contains("&lt;Control&gt;o"));
    assert!(shortcuts_ui.contains("Open Recent Documents"));
    assert!(shortcuts_ui.contains("&lt;Control&gt;k"));
}

#[test]
fn test_open_header_menu_button_and_popover_controls_have_accessible_roles() {
    ensure_gtk_init();
    let window = test_window();
    let popover = &window.imp().open_popover;

    assert_eq!(
        window.imp().new_tab_button.accessible_role(),
        gtk4::AccessibleRole::Button
    );
    assert_eq!(
        window.imp().open_menu_button.accessible_role(),
        gtk4::AccessibleRole::Button
    );
    assert_eq!(
        popover.search_entry_for_test().accessible_role(),
        gtk4::AccessibleRole::SearchBox
    );
    assert_eq!(
        popover.chooser_button_for_test().accessible_role(),
        gtk4::AccessibleRole::Button
    );
    assert_eq!(
        popover.list_view_for_test().accessible_role(),
        gtk4::AccessibleRole::List
    );
    assert_eq!(
        window.imp().new_tab_button.action_name().as_deref(),
        Some("win.new-tab")
    );
    assert_eq!(
        window.imp().new_tab_button.tooltip_text().as_deref(),
        Some("New File")
    );
    assert_eq!(
        window.imp().open_menu_button.tooltip_text().as_deref(),
        Some("Recently Used Documents")
    );
}

#[test]
fn test_open_popover_awkward_labels_do_not_require_horizontal_scroll() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    popover.set_recent_rows(vec![row(
        "/tmp/a very long folder name/with spaces/symbols []() and mixed width/this-is-a-ridiculously-long-file-name-that-must-ellipsize.rs",
        10,
    )]);

    assert_eq!(popover.visible_row_count_for_test(), 1);
    assert!(popover.list_visible_for_test());
    assert_eq!(
        popover.list_hscrollbar_policy_for_test(),
        gtk4::PolicyType::Never
    );
    assert!(!popover.list_propagates_natural_width_for_test());
}

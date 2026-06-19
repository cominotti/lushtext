// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget coverage for the GNOME-style Open popover.

use crate::common::{
    ensure_gtk_init, fixture, flush_events, fs_metadata, fs_mutate, isolated_data_dir,
    present_window, test_window, wait_until,
};
use glib::object::ObjectExt;
use glib::prelude::Cast;
use glib::prelude::ToValue;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::recent_document::{RecentDocumentEntry, RecentDocumentRow};
use lushtext_core::services::recent_documents;
use lushtext_core::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_core::ui::open_popover::LushtextOpenPopover;
use lushtext_core::ui::window::LushtextWindow;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

fn recent_entry(path: impl Into<PathBuf>, secs: u64) -> RecentDocumentEntry {
    RecentDocumentEntry::new(path.into(), None, secs)
}

fn recent_entry_with_canonical(
    path: impl Into<PathBuf>,
    canonical: impl Into<PathBuf>,
    secs: u64,
) -> RecentDocumentEntry {
    RecentDocumentEntry::new(path.into(), Some(canonical.into()), secs)
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

fn open_recent_button(window: &LushtextWindow) {
    wait_until(Duration::from_secs(2), || {
        window.imp().open_menu_button.is_mapped()
    });
    window.imp().open_menu_button.popup();
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
        // GTK exposes observed controllers as generic Objects; tests downcast
        // them before checking concrete controller properties.
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
    active_editor(window).and_then(|editor| editor.file_path())
}

fn active_editor(
    window: &LushtextWindow,
) -> Option<lushtext_core::ui::editor_page::LushtextEditorPage> {
    window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| {
            page.child()
                .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
                .ok()
        })
}

fn active_editor_has_focus(
    window: &LushtextWindow,
    editor: &lushtext_core::ui::editor_page::LushtextEditorPage,
) -> bool {
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return false;
    };
    focus.as_ptr() == editor.source_view().upcast_ref::<gtk4::Widget>().as_ptr()
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

fn find_remove_button(root: &impl IsA<gtk4::Widget>) -> gtk4::Button {
    remove_buttons(root)
        .into_iter()
        .next()
        .expect("remove button")
}

/// Find row remove buttons by their action tooltip so tests do not depend on row indices.
fn remove_buttons(root: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Button> {
    descendants(root)
        .into_iter()
        .filter_map(|widget| {
            widget
                .downcast::<gtk4::Button>()
                .ok()
                .filter(|button| button.tooltip_text().as_deref() == Some("Remove"))
        })
        .collect()
}

/// Collect tooltip properties directly instead of showing transient tooltip popups.
///
/// The root is included because production may put a path tooltip on the row
/// grid itself, not just on child widgets.
fn tooltip_texts(root: &impl IsA<gtk4::Widget>) -> Vec<String> {
    let mut tooltips = root
        .as_ref()
        .tooltip_text()
        .map(|text| vec![text.to_string()])
        .unwrap_or_default();
    tooltips.extend(
        descendants(root)
            .into_iter()
            .filter_map(|widget| widget.tooltip_text().map(|text| text.to_string())),
    );
    tooltips
}

fn has_tooltip(root: &impl IsA<gtk4::Widget>, expected: &str) -> bool {
    tooltip_texts(root)
        .iter()
        .any(|tooltip| tooltip == expected)
}

fn path_tooltip(path: &Path) -> String {
    path.display().to_string()
}

/// Assert that a realized row exposes the full activation path as tooltip text.
fn assert_has_path_tooltip(root: &impl IsA<gtk4::Widget>, path: &Path) {
    let expected = path_tooltip(path);
    let tooltips = tooltip_texts(root);
    assert!(
        tooltips.iter().any(|tooltip| tooltip == &expected),
        "Open popover should expose full path tooltip {expected:?}; saw {tooltips:?}"
    );
}

/// Find the realized recent-row grid for a path tooltip.
fn path_tooltip_grid(root: &impl IsA<gtk4::Widget>, path: &Path) -> gtk4::Grid {
    let expected = path_tooltip(path);
    descendants(root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Grid>().ok())
        .find(|grid| grid.tooltip_text().as_deref() == Some(expected.as_str()))
        .expect("recent row grid with path tooltip")
}

/// Assert that recycled rows no longer expose a previous row's path tooltip.
fn assert_lacks_path_tooltip(root: &impl IsA<gtk4::Widget>, path: &Path) {
    let unexpected = path_tooltip(path);
    let tooltips = tooltip_texts(root);
    assert!(
        !tooltips.iter().any(|tooltip| tooltip == &unexpected),
        "Open popover should not expose stale path tooltip {unexpected:?}; saw {tooltips:?}"
    );
}

/// Assert source-parity CSS declarations without relying on GTK style resolution.
fn assert_css_rule_contains(selector: &str, declarations: &[&str]) {
    let css = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/style/style.css"
    ));
    let selector_start = css
        .find(selector)
        .unwrap_or_else(|| panic!("Open popover CSS should contain selector {selector}"));
    let rule_start = css[selector_start..].find('{').map_or_else(
        || panic!("Open popover CSS selector {selector} should open a rule"),
        |offset| selector_start + offset + 1,
    );
    let rule_end = css[rule_start..].find('}').map_or_else(
        || panic!("Open popover CSS selector {selector} should close a rule"),
        |offset| rule_start + offset,
    );
    let rule = &css[rule_start..rule_end];

    for declaration in declarations {
        assert!(
            rule.contains(declaration),
            "Open popover CSS selector {selector} should contain {declaration}"
        );
    }
}

/// Emit `key-pressed` on installed key controllers without synthesizing a full display event.
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
        window
            .imp()
            .open_button_stack
            .visible_child_name()
            .as_deref()
            == Some("wide")
    });
    assert_header_open_precedes_new_tab(&window, "wide Open label presentation");

    window
        .imp()
        .open_button_stack
        .set_visible_child_name("narrow");
    flush_events();
    assert_eq!(
        window
            .imp()
            .open_button_stack
            .visible_child_name()
            .as_deref(),
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
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::SearchBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&popover.search_entry_for_test());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&popover.chooser_button_for_test());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::List)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .states(&[gtk4::AccessibleState::Hidden])
        .assert_on(&popover.list_view_for_test());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&popover.empty_state_for_test());
}

#[test]
fn test_open_popover_one_representative_row_uses_file_title() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();

    popover.set_recent_rows(vec![row("/tmp/project/src/main.rs", 10)]);

    assert_eq!(popover.visible_titles_for_test(), vec!["main.rs"]);
    assert!(popover.list_visible_for_test());
    assert!(!gtk4::test_accessible_has_state(
        &popover.list_view_for_test(),
        gtk4::AccessibleState::Hidden
    ));
    assert!(gtk4::test_accessible_has_state(
        &popover.empty_state_for_test(),
        gtk4::AccessibleState::Hidden
    ));
}

#[test]
fn test_open_popover_recent_row_tooltip_shows_full_activation_path() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("representative tooltip tempdir");
    let path = dir.path().join("main.rs");
    fixture::write_text(&path, "fn main() {}\n");
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry(path.clone(), 10)]);
    present_window(&window);

    open_recent_action(&window);

    let popover = window.imp().open_popover.clone();
    let expected = path_tooltip(&path);
    wait_until(Duration::from_secs(2), || has_tooltip(&popover, &expected));
    assert_has_path_tooltip(&popover, &path);
    let row_grid = path_tooltip_grid(&popover, &path);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .relations(&[
            gtk4::AccessibleRelation::PosInSet,
            gtk4::AccessibleRelation::SetSize,
        ])
        .assert_on(&row_grid);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&find_remove_button(&popover));
    assert!(popover.list_visible_for_test());
}

#[test]
fn test_open_popover_deep_awkward_path_tooltip_stays_complete() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("awkward tooltip tempdir");
    let deep_dir = dir
        .path()
        .join("folder with spaces")
        .join("symbols []() and plus + equals =")
        .join("another very deep folder name");
    fs_mutate::create_dir_all(&deep_dir).expect("create awkward tooltip directories");
    let path = deep_dir.join("this-is-a-ridiculously-long-file-name-that-must-ellipsize.rs");
    fixture::write_text(&path, "fn awkward() {}\n");
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry(path.clone(), 10)]);
    present_window(&window);

    open_recent_action(&window);

    let popover = window.imp().open_popover.clone();
    let expected = path_tooltip(&path);
    wait_until(Duration::from_secs(2), || has_tooltip(&popover, &expected));
    assert_has_path_tooltip(&popover, &path);
    assert_eq!(
        popover.list_hscrollbar_policy_for_test(),
        gtk4::PolicyType::Never
    );
    assert!(!popover.list_propagates_natural_width_for_test());
}

#[test]
fn test_open_popover_recent_list_uses_no_selection_and_single_click_activation() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();

    popover.set_recent_rows(vec![row("/tmp/project/src/main.rs", 10)]);

    assert!(popover.recent_list_uses_no_selection_for_test());
    assert!(popover.list_view_for_test().is_single_click_activate());
}

#[test]
fn test_open_popover_recent_row_layout_matches_gnome_text_editor_source() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    let layout = popover.row_layout_snapshot_for_test();

    assert_eq!(layout.grid_margin_top, 3);
    assert_eq!(layout.grid_margin_bottom, 3);
    assert_eq!(layout.grid_margin_start, 0);
    assert_eq!(layout.grid_margin_end, 6);
    assert_eq!(layout.grid_row_spacing, 3);
    assert_eq!(layout.grid_column_spacing, 6);
    assert_eq!(layout.grid_height_request, -1);
    assert!(layout.marker_hhomogeneous);
    assert!(layout.marker_vhomogeneous);
    assert_eq!(
        (layout.marker_layout.column, layout.marker_layout.row),
        (0, 0)
    );
    assert_eq!(
        layout.title_overflow,
        gtk4::InscriptionOverflow::EllipsizeMiddle
    );
    assert_eq!(layout.title_xalign, 0.0);
    assert!(layout.title_hexpand);
    assert_eq!(
        (
            layout.title_layout.column,
            layout.title_layout.row,
            layout.title_layout.column_span,
            layout.title_layout.row_span,
        ),
        (1, 0, 2, 1)
    );
    assert_eq!(
        layout.subtitle_overflow,
        gtk4::InscriptionOverflow::EllipsizeEnd
    );
    assert_eq!(layout.subtitle_min_chars, 25);
    assert_eq!(layout.subtitle_nat_chars, 25);
    assert_eq!(layout.subtitle_min_lines, 1);
    assert_eq!(layout.subtitle_nat_lines, 1);
    assert!(layout.subtitle_has_caption);
    assert!(layout.subtitle_has_dim_label);
    assert_eq!(
        (
            layout.subtitle_layout.column,
            layout.subtitle_layout.row,
            layout.subtitle_layout.column_span,
            layout.subtitle_layout.row_span,
        ),
        (1, 1, 1, 1)
    );
    assert!(!layout.age_visible);
    assert_eq!(layout.age_halign, gtk4::Align::End);
    assert_eq!(layout.age_valign, gtk4::Align::Center);
    assert!(layout.age_has_caption);
    assert!(layout.age_has_dim_label);
    assert_eq!(
        (
            layout.age_layout.column,
            layout.age_layout.row,
            layout.age_layout.column_span,
            layout.age_layout.row_span,
        ),
        (2, 1, 1, 1)
    );
    assert_eq!(
        layout.remove_icon_name.as_deref(),
        Some("window-close-symbolic")
    );
    assert_eq!(layout.remove_tooltip.as_deref(), Some("Remove"));
    assert_eq!(layout.remove_halign, gtk4::Align::End);
    assert_eq!(layout.remove_valign, gtk4::Align::Center);
    assert!(layout.remove_has_flat);
    assert!(layout.remove_has_circular);
    assert_eq!(
        (
            layout.remove_layout.column,
            layout.remove_layout.row,
            layout.remove_layout.column_span,
            layout.remove_layout.row_span,
        ),
        (3, 0, 1, 2)
    );
}

#[test]
fn test_open_popover_css_row_constants_match_gnome_text_editor_source() {
    assert_css_rule_contains(".open-popover contents", &["padding: 0;"]);
    assert_css_rule_contains(
        ".open-popover listview",
        &[
            "margin-bottom: 3px;",
            "background: none;",
            "color: inherit;",
        ],
    );
    assert_css_rule_contains(
        ".open-popover listview row",
        &["border-radius: 6px;", "margin: 3px 6px;"],
    );
    assert_css_rule_contains(
        ".open-popover listview row:first-child",
        &["margin-top: 6px;"],
    );
    assert_css_rule_contains(
        ".open-popover listview row button",
        &[
            "padding: 3px;",
            "margin: 0;",
            "min-height: 24px;",
            "min-width: 24px;",
        ],
    );
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
    assert_eq!(popover.empty_title_for_test(), "No Matching Documents");
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&popover.empty_state_for_test());
    assert!(gtk4::test_accessible_has_state(
        &popover.list_view_for_test(),
        gtk4::AccessibleState::Hidden
    ));
    assert!(gtk4::test_accessible_has_state(
        &popover.recent_scroller_for_test(),
        gtk4::AccessibleState::Hidden
    ));
}

#[test]
fn test_open_popover_gnome_scroll_cap_and_extra_rows_stay_model_backed() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    let rows = (0..11)
        .map(|idx| row(format!("/tmp/recent/file-{idx}.txt"), 100 - idx))
        .collect();

    popover.set_recent_rows(rows);

    assert_eq!(popover.visible_row_count_for_test(), 11);
    assert_eq!(popover.list_min_content_width_for_test(), 250);
    assert_eq!(popover.list_max_content_width_for_test(), 250);
    assert_eq!(popover.list_max_content_height_for_test(), 600);
    assert!(popover.list_propagates_natural_height_for_test());
    assert_eq!(
        popover.list_hscrollbar_policy_for_test(),
        gtk4::PolicyType::Never
    );
}

#[test]
fn test_open_popover_search_finds_rows_beyond_visible_scroll_region() {
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
fn test_open_popover_filter_rebinds_path_tooltips_without_stale_leaks() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("filtered tooltip tempdir");
    let first = dir.path().join("alpha.txt");
    let target = dir.path().join("target.txt");
    fixture::write_text(&first, "alpha\n");
    fixture::write_text(&target, "target\n");
    let window = test_window();
    window.set_recent_documents_for_test(vec![
        recent_entry(first.clone(), 20),
        recent_entry(target.clone(), 10),
    ]);
    present_window(&window);

    open_recent_action(&window);

    let popover = window.imp().open_popover.clone();
    let first_tooltip = path_tooltip(&first);
    wait_until(Duration::from_secs(2), || {
        has_tooltip(&popover, &first_tooltip)
    });
    assert_has_path_tooltip(&popover, &first);

    popover.set_search_text_for_test("target");
    flush_events();

    let target_tooltip = path_tooltip(&target);
    wait_until(Duration::from_secs(2), || {
        popover.visible_titles_for_test() == vec!["target.txt"]
            && has_tooltip(&popover, &target_tooltip)
            && !has_tooltip(&popover, &first_tooltip)
    });
    assert_has_path_tooltip(&popover, &target);
    assert_lacks_path_tooltip(&popover, &first);
}

#[test]
fn test_open_popover_no_match_state_clears_recent_path_tooltips() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("no match tooltip tempdir");
    let path = dir.path().join("visible-before-filter.txt");
    fixture::write_text(&path, "visible before filter\n");
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry(path.clone(), 10)]);
    present_window(&window);

    open_recent_action(&window);

    let popover = window.imp().open_popover.clone();
    let expected = path_tooltip(&path);
    wait_until(Duration::from_secs(2), || has_tooltip(&popover, &expected));

    popover.set_search_text_for_test("does-not-exist");
    flush_events();

    wait_until(Duration::from_secs(2), || {
        popover.visible_row_count_for_test() == 0 && !popover.list_visible_for_test()
    });
    assert_eq!(popover.empty_title_for_test(), "No Matching Documents");
    assert_lacks_path_tooltip(&popover, &path);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::SearchBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&popover.search_entry_for_test());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&popover.chooser_button_for_test());
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
fn test_open_popover_filtered_list_activation_uses_visible_position() {
    ensure_gtk_init();
    let popover = LushtextOpenPopover::new();
    let hidden = PathBuf::from("/tmp/recent/alpha.txt");
    let target = PathBuf::from("/tmp/recent/target.txt");
    popover.set_recent_rows(vec![row(hidden, 20), row(target.as_path(), 10)]);
    let activated = Rc::new(RefCell::new(None));
    let activated_for_cb = Rc::clone(&activated);
    popover.connect_recent_activated(move |path| {
        activated_for_cb.replace(Some(path));
    });

    popover.set_search_text_for_test("target");
    popover
        .list_view_for_test()
        .emit_by_name::<()>("activate", &[&0u32]);
    flush_events();

    assert_eq!(activated.borrow().as_ref(), Some(&target));
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
    wait_until(Duration::from_secs(2), || {
        focus_is_inside_open_search(&window)
    });

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
        window.imp().open_popover.keyboard_row_position_for_test(),
        Some(0)
    );

    assert_eq!(
        emit_key_until_handled(
            &window.imp().open_popover.list_view_for_test(),
            gtk4::gdk::Key::Down
        ),
        glib::Propagation::Stop
    );
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.keyboard_row_position_for_test() == Some(1)
    });

    assert_eq!(
        emit_key_until_handled(
            &window.imp().open_popover.list_view_for_test(),
            gtk4::gdk::Key::Up
        ),
        glib::Propagation::Stop
    );
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.keyboard_row_position_for_test() == Some(0)
            && focus_is_inside_widget(&window, &window.imp().open_popover.list_view_for_test())
    });

    assert_eq!(
        emit_key_until_handled(
            &window.imp().open_popover.list_view_for_test(),
            gtk4::gdk::Key::Up
        ),
        glib::Propagation::Stop
    );
    flush_events();
    wait_until(Duration::from_secs(2), || {
        focus_is_inside_open_search(&window)
    });
}

#[test]
fn test_open_popover_gtk_row_focus_syncs_no_selection_keynav_position() {
    ensure_gtk_init();
    let window = test_window();
    window.set_recent_documents_for_test(vec![
        recent_entry("/tmp/first.txt", 30),
        recent_entry("/tmp/second.txt", 20),
        recent_entry("/tmp/third.txt", 10),
    ]);
    present_window(&window);
    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || {
        focus_is_inside_open_search(&window)
    });

    let popover = window.imp().open_popover.clone();
    wait_until(Duration::from_secs(2), || {
        remove_buttons(&popover).len() >= 3
    });
    let remove = remove_buttons(&popover)
        .get(1)
        .cloned()
        .expect("second remove button");
    // Focus a row child to prove no-selection keynav still follows the row
    // position when the ListView itself is not focused.
    remove.grab_focus();
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.keyboard_row_position_for_test() == Some(1)
    });

    let row_grid = remove
        .parent()
        .and_downcast::<gtk4::Grid>()
        .expect("remove button should be attached to the recent-row grid");
    assert_eq!(
        emit_key_until_handled(&row_grid, gtk4::gdk::Key::Up),
        glib::Propagation::Stop
    );
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.keyboard_row_position_for_test() == Some(0)
            && focus_is_inside_widget(&window, &window.imp().open_popover.list_view_for_test())
    });
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
fn test_open_popover_escape_dismisses_and_restores_editor_focus() {
    ensure_gtk_init();
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry("/tmp/example.txt", 10)]);
    window.new_tab();
    present_window(&window);
    let editor = active_editor(&window).expect("new tab should be active");
    editor.source_view().grab_focus();
    flush_events();
    wait_until(Duration::from_secs(2), || {
        active_editor_has_focus(&window, &editor)
    });

    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || {
        open_popover_open(&window) && focus_is_inside_open_search(&window)
    });

    window
        .imp()
        .open_popover
        .search_entry_for_test()
        .emit_by_name::<()>("stop-search", &[]);
    flush_events();

    wait_until(Duration::from_secs(2), || !open_popover_open(&window));
    wait_until(Duration::from_secs(2), || {
        active_editor_has_focus(&window, &editor)
    });
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

    wait_until(Duration::from_secs(2), || {
        active_file_path(&window) == Some(path.clone())
    });
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
    wait_until(Duration::from_secs(2), || {
        active_file_path(&window) == Some(open_path.clone())
    });

    window.set_recent_documents_for_test(vec![
        recent_entry(open_path, 20),
        recent_entry(closed_path, 10),
    ]);
    assert_eq!(
        window.imp().open_popover.visible_titles_for_test(),
        vec!["closed.txt"]
    );

    gtk4::prelude::ActionGroupExt::activate_action(&window, "close-tab", None);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 0
    });

    assert_eq!(
        window.imp().open_popover.visible_titles_for_test(),
        vec!["open.txt", "closed.txt"]
    );
}

#[test]
fn test_open_popover_startup_loaded_recents_visible_with_no_tabs() {
    ensure_gtk_init();
    let data_dir = isolated_data_dir();
    let dir = tempfile::tempdir().expect("startup recent tempdir");
    let path = dir.path().join("disk-recent.txt");
    fixture::write_text(&path, "loaded from recent persistence\n");
    let canonical = fs_metadata::canonical_path(&path).expect("canonical recent path");
    recent_documents::save(
        data_dir.path(),
        &[recent_entry_with_canonical(path, canonical, 30)],
    )
    .expect("seed recent persistence");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(5), || {
        window.recent_documents_for_test().len() == 1
    });

    assert_eq!(window.imp().tab_view.n_pages(), 0);
    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test() == vec!["disk-recent.txt"]
    });
    assert!(window.imp().open_popover.list_visible_for_test());
}

#[test]
fn test_open_popover_header_button_rebuilds_startup_loaded_recents() {
    ensure_gtk_init();
    let data_dir = isolated_data_dir();
    let dir = tempfile::tempdir().expect("startup recent button tempdir");
    let path = dir.path().join("disk-button-recent.txt");
    fixture::write_text(&path, "loaded for the visible header button\n");
    let canonical = fs_metadata::canonical_path(&path).expect("canonical recent path");
    recent_documents::save(
        data_dir.path(),
        &[recent_entry_with_canonical(path, canonical, 30)],
    )
    .expect("seed recent persistence");

    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(5), || {
        window.recent_documents_for_test().len() == 1
    });

    assert_eq!(window.imp().tab_view.n_pages(), 0);
    open_recent_button(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test() == vec!["disk-button-recent.txt"]
    });
    assert!(window.imp().open_popover.list_visible_for_test());
}

#[test]
fn test_open_popover_same_session_file_chooser_close_reveals_recent() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("same-session recent tempdir");
    let path = dir.path().join("chooser-opened.txt");
    fixture::write_text(&path, "opened through chooser seam\n");
    let window = test_window();
    present_window(&window);

    window.select_open_file_for_test(&path);
    wait_until(Duration::from_secs(3), || {
        active_file_path(&window) == Some(path.clone())
            && window
                .recent_documents_for_test()
                .iter()
                .any(|entry| entry.matches_path(&path))
    });

    gtk4::prelude::ActionGroupExt::activate_action(&window, "close-tab", None);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 0
    });

    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test() == vec!["chooser-opened.txt"]
    });
}

#[test]
fn test_open_popover_header_button_rebuilds_same_session_closed_recent() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("same-session recent button tempdir");
    let path = dir.path().join("chooser-button-opened.txt");
    fixture::write_text(
        &path,
        "opened through chooser and shown from the header button\n",
    );
    let window = test_window();
    present_window(&window);

    window.select_open_file_for_test(&path);
    wait_until(Duration::from_secs(3), || {
        active_file_path(&window) == Some(path.clone())
            && window
                .recent_documents_for_test()
                .iter()
                .any(|entry| entry.matches_path(&path))
    });

    gtk4::prelude::ActionGroupExt::activate_action(&window, "close-tab", None);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 0
    });

    open_recent_button(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test() == vec!["chooser-button-opened.txt"]
    });
    assert!(window.imp().open_popover.list_visible_for_test());
}

#[test]
fn test_open_popover_ignores_stale_display_and_canonical_open_identities() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("stale identity recent tempdir");
    let path = dir.path().join("stale-open-cache.txt");
    fixture::write_text(&path, "closed tab should be visible\n");
    let canonical = fs_metadata::canonical_path(&path).expect("canonical recent path");
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry_with_canonical(
        path.clone(),
        canonical.clone(),
        20,
    )]);
    window.imp().open_paths.borrow_mut().extend([
        path,
        canonical,
        dir.path().join("unrelated-stale-key.txt"),
    ]);
    present_window(&window);

    open_recent_action(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test() == vec!["stale-open-cache.txt"]
    });
    assert_eq!(window.imp().tab_view.n_pages(), 0);
}

#[test]
fn test_open_popover_updates_when_open_tab_closes_while_popover_visible() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("close while popover visible tempdir");
    let open_path = dir.path().join("open-while-visible.txt");
    let closed_path = dir.path().join("already-closed.txt");
    fixture::write_text(&open_path, "open\n");
    fixture::write_text(&closed_path, "closed\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&open_path);
    wait_until(Duration::from_secs(3), || {
        active_file_path(&window) == Some(open_path.clone())
    });
    window.set_recent_documents_for_test(vec![
        recent_entry(open_path, 20),
        recent_entry(closed_path, 10),
    ]);

    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test() == vec!["already-closed.txt"]
    });
    gtk4::prelude::ActionGroupExt::activate_action(&window, "close-tab", None);
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 0
    });

    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test()
            == vec!["open-while-visible.txt", "already-closed.txt"]
    });
}

#[test]
fn test_open_popover_bulk_path_close_reconciles_after_batch() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("bulk close recent tempdir");
    let first_path = dir.path().join("first-open.txt");
    let second_path = dir.path().join("second-open.txt");
    fixture::write_text(&first_path, "first\n");
    fixture::write_text(&second_path, "second\n");
    let window = test_window();
    present_window(&window);
    window.open_document(&first_path);
    wait_until(Duration::from_secs(3), || {
        active_file_path(&window) == Some(first_path.clone())
    });
    window.open_document(&second_path);
    wait_until(Duration::from_secs(3), || {
        window.imp().tab_view.n_pages() == 2
            && active_file_path(&window) == Some(second_path.clone())
    });
    window.set_recent_documents_for_test(vec![
        recent_entry(first_path, 30),
        recent_entry(second_path, 20),
    ]);

    open_recent_action(&window);
    wait_until(Duration::from_secs(2), || {
        window
            .imp()
            .open_popover
            .visible_titles_for_test()
            .is_empty()
    });

    window.close_tab_for_path(dir.path());
    flush_events();
    wait_until(Duration::from_secs(2), || {
        window.imp().tab_view.n_pages() == 0
    });
    wait_until(Duration::from_secs(2), || {
        window.imp().open_popover.visible_titles_for_test()
            == vec!["first-open.txt", "second-open.txt"]
    });

    assert_eq!(window.imp().tab_projection_refresh_defer_depth.get(), 0);
    assert!(window.imp().open_paths.borrow().is_empty());
}

#[test]
fn test_open_popover_row_remove_keeps_popover_open_and_does_not_open_document() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("recent remove tempdir");
    let path = dir.path().join("remove-me.txt");
    fixture::write_text(&path, "remove\n");
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry(path.clone(), 10)]);
    present_window(&window);
    open_recent_action(&window);
    let popover = window.imp().open_popover.clone();
    wait_until(Duration::from_secs(2), || {
        descendants(&popover).into_iter().any(|widget| {
            widget
                .downcast_ref::<gtk4::Button>()
                .is_some_and(|button| button.tooltip_text().as_deref() == Some("Remove"))
        })
    });

    let remove = find_remove_button(&popover);
    assert_eq!(remove.accessible_role(), gtk4::AccessibleRole::Button);
    let document_tooltip = path_tooltip(&path);
    assert_eq!(remove.tooltip_text().as_deref(), Some("Remove"));
    assert_ne!(
        remove.tooltip_text().as_deref(),
        Some(document_tooltip.as_str())
    );
    assert_has_path_tooltip(&popover, &path);
    let row = path_tooltip_grid(&popover, &path);
    assert!(gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Label
    ));
    assert!(gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Description
    ));
    remove.emit_clicked();
    flush_events();

    assert!(open_popover_open(&window));
    assert_eq!(window.imp().tab_view.n_pages(), 0);
    assert!(window.recent_documents_for_test().is_empty());
}

#[test]
fn test_open_popover_repeated_remove_reaches_empty_state_without_closing() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("repeat recent remove tempdir");
    let first = dir.path().join("remove-first.txt");
    let second = dir.path().join("remove-second.txt");
    fixture::write_text(&first, "first\n");
    fixture::write_text(&second, "second\n");
    let window = test_window();
    window.set_recent_documents_for_test(vec![recent_entry(first, 20), recent_entry(second, 10)]);
    present_window(&window);
    open_recent_action(&window);
    let popover = window.imp().open_popover.clone();
    wait_until(Duration::from_secs(2), || {
        popover.visible_row_count_for_test() == 2
    });

    find_remove_button(&popover).emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(2), || {
        popover.visible_row_count_for_test() == 1
    });

    find_remove_button(&popover).emit_clicked();
    flush_events();
    wait_until(Duration::from_secs(2), || {
        popover.visible_row_count_for_test() == 0
    });

    assert!(open_popover_open(&window));
    assert!(!popover.list_visible_for_test());
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

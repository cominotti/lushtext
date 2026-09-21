// SPDX-License-Identifier: GPL-3.0-or-later

//! Rendered-row coverage for the workspace tree.
//!
//! `GtkListView` realizes at most `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200, plus two
//! extra) row widgets for one visible range. Before the slice bin, each
//! section handed its list view the whole tree as viewport, so directories
//! above ~200 entries rendered blank space after row ~205 even though every
//! entry was in the tree model. These tests therefore assert **rendered**
//! rows, never model rows, against the real window and sidebar so the outer
//! scroller is part of the picture.

use crate::common::{
    ensure_gtk_init, find_descendant, fixture, flush_after_delay, flush_events, present_window,
    realized_list_rows, test_window, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::workspace::{WorkspaceConfig, WorkspaceId, WorkspacesFile};
use lushtext_core::services::{json_store, workspace_manager};
use lushtext_core::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_core::ui::sidebar::LushtextSidebar;
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::sidebar::workspace_section::LushtextWorkspaceSection;
use lushtext_core::ui::window::LushtextWindow;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// GTK's `GTK_LIST_VIEW_MAX_LIST_ITEMS`; rendered rows must stay below it.
const GTK_LIST_VIEW_REALIZED_CAP: usize = 200;

/// One workspace window whose first section is mapped and expanded.
struct LargeTree {
    _dir: tempfile::TempDir,
    root: PathBuf,
    window: LushtextWindow,
    sidebar: LushtextSidebar,
    section: LushtextWorkspaceSection,
}

fn file_name(index: usize) -> String {
    format!("row-{index:05}.txt")
}

/// Create `dir` and fill it with `count` one-byte files named by `file_name`.
fn seed_files(dir: &Path, count: usize) {
    fixture::create_dir_all(dir);
    for index in 0..count {
        fixture::write_text(&dir.join(file_name(index)), "x");
    }
}

/// Seed one workspace folder holding `top.txt` plus the given nested
/// directories, each populated with `count` files.
fn seed_workspace(dirs: &[(&str, usize)]) -> (tempfile::TempDir, PathBuf) {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("large tree tempdir");
    let root = dir.path().join("root");
    fixture::create_dir_all(&root);
    fixture::write_text(&root.join("top.txt"), "");
    for (relative, count) in dirs {
        seed_files(&root.join(relative), *count);
    }
    (dir, root)
}

fn save_workspaces(configs: Vec<WorkspaceConfig>) {
    let mut workspaces = WorkspacesFile::default();
    workspaces.workspaces.extend(configs);
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces.json");
}

fn present_sidebar_window(height: i32) -> (LushtextWindow, LushtextSidebar) {
    let window = test_window();
    window.set_default_size(1200, height);
    let sidebar = window.imp().sidebar.clone();
    present_window(&window);
    sidebar.load_workspaces();
    (window, sidebar)
}

/// The sidebar rebuilds its sections after `load_workspaces()` returns, so a
/// handle taken at the first `sections` entry can point at a detached widget.
/// Let the rebuild land, then wait for the mapped instances.
fn mapped_sections(sidebar: &LushtextSidebar, count: usize) -> Vec<LushtextWorkspaceSection> {
    flush_after_delay(Duration::from_secs(1));
    wait_until(Duration::from_secs(10), || {
        let sections = sidebar.imp().sections.borrow();
        sections.len() == count && sections.iter().all(WidgetExt::is_mapped)
    });
    sidebar.imp().sections.borrow().clone()
}

fn large_tree(dirs: &[(&str, usize)], height: i32) -> LargeTree {
    let (dir, root) = seed_workspace(dirs);
    save_workspaces(vec![WorkspaceConfig::with_one_folder(
        WorkspaceId::new("virtualized"),
        "virtualized",
        root.clone(),
    )]);
    let (window, sidebar) = present_sidebar_window(height);
    let section = mapped_sections(&sidebar, 1).remove(0);
    section.expand_folders();
    LargeTree {
        _dir: dir,
        root,
        window,
        sidebar,
        section,
    }
}

fn tree_index_for_path(section: &LushtextWorkspaceSection, target: &Path) -> Option<u32> {
    let tree_model = section.imp().tree_model.borrow().as_ref()?.clone();
    (0..tree_model.n_items()).find(|index| {
        tree_model
            .item(*index)
            .and_downcast::<gtk4::TreeListRow>()
            .and_then(|row| row.item().and_downcast::<FileTreeItem>())
            .and_then(|item| item.path())
            .as_deref()
            == Some(target)
    })
}

fn row_for_path(section: &LushtextWorkspaceSection, target: &Path) -> Option<gtk4::TreeListRow> {
    let index = tree_index_for_path(section, target)?;
    let tree_model = section.imp().tree_model.borrow().as_ref()?.clone();
    tree_model.item(index).and_downcast::<gtk4::TreeListRow>()
}

fn selection(section: &LushtextWorkspaceSection) -> gtk4::SingleSelection {
    section
        .imp()
        .file_tree_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .expect("file tree should use a SingleSelection")
}

fn wait_for_refresh_idle(section: &LushtextWorkspaceSection) {
    wait_until(Duration::from_secs(30), || {
        !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
    });
}

/// Expand every ancestor between the workspace folder and `target`, then
/// `target` itself, waiting for readiness after each step.
fn expand_path(section: &LushtextWorkspaceSection, root: &Path, target: &Path) {
    let relative = target.strip_prefix(root).expect("target inside root");
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        wait_until(Duration::from_secs(10), || {
            row_for_path(section, &current).is_some()
        });
        row_for_path(section, &current)
            .expect("directory row")
            .set_expanded(true);
        wait_for_refresh_idle(section);
    }
    flush_after_delay(Duration::from_millis(300));
    wait_for_refresh_idle(section);
}

/// Select `target` and scroll it into view with focus, as the sidebar does
/// for its own selection restores.
fn select_and_scroll_to(section: &LushtextWorkspaceSection, target: &Path) -> u32 {
    let index = tree_index_for_path(section, target).expect("row index for path");
    selection(section).set_selected(index);
    section
        .imp()
        .file_tree_view
        .scroll_to(index, gtk4::ListScrollFlags::FOCUS, None);
    flush_after_delay(Duration::from_millis(300));
    index
}

fn row_label(widget: &gtk4::Widget) -> Option<gtk4::Label> {
    find_descendant(widget, glib::object::ObjectExt::is::<gtk4::Label>)
        .and_downcast::<gtk4::Label>()
}

/// Realized row widgets of the section's list view, in child order, with their labels.
fn rendered_rows(section: &LushtextWorkspaceSection) -> Vec<(gtk4::Widget, String)> {
    realized_list_rows(&section.imp().file_tree_view)
        .into_iter()
        .map(|widget| {
            let label = row_label(&widget).map(|label| label.text().to_string());
            (widget, label.unwrap_or_default())
        })
        .collect()
}

fn rendered_labels(section: &LushtextWorkspaceSection) -> BTreeSet<String> {
    rendered_rows(section)
        .into_iter()
        .map(|(_, label)| label)
        .collect()
}

fn rendered_row_widget(section: &LushtextWorkspaceSection, label: &str) -> Option<gtk4::Widget> {
    rendered_rows(section)
        .into_iter()
        .find_map(|(widget, text)| (text == label).then_some(widget))
}

/// The row-content widget that carries the accessible label; row metadata
/// lives on the row content, not the list item wrapper.
fn labelled_descendant(root: &gtk4::Widget) -> gtk4::Widget {
    find_descendant(root, |candidate| {
        gtk4::test_accessible_has_property(candidate, gtk4::AccessibleProperty::Label)
    })
    .expect("no accessible label in the row subtree")
}

fn outer_adjustment(sidebar: &LushtextSidebar) -> gtk4::Adjustment {
    sidebar.imp().outer_scrolled_window.vadjustment()
}

fn scroll_outer_to(sidebar: &LushtextSidebar, value: f64) {
    outer_adjustment(sidebar).set_value(value);
    flush_after_delay(Duration::from_millis(250));
}

fn scroll_outer_to_bottom(sidebar: &LushtextSidebar) {
    let adjustment = outer_adjustment(sidebar);
    scroll_outer_to(sidebar, adjustment.upper() - adjustment.page_size());
}

/// Bottom edge of `widget` in the outer scroller's coordinates, if allocated.
fn bottom_in_outer(sidebar: &LushtextSidebar, widget: &gtk4::Widget) -> Option<f64> {
    widget
        .compute_bounds(&*sidebar.imp().outer_scrolled_window)
        .map(|bounds| f64::from(bounds.y() + bounds.height()))
}

/// True when `widget` lies inside the outer scroller's visible area.
fn inside_outer_viewport(sidebar: &LushtextSidebar, widget: &gtk4::Widget) -> bool {
    let outer = &*sidebar.imp().outer_scrolled_window;
    widget.compute_bounds(outer).is_some_and(|bounds| {
        f64::from(bounds.y()) >= -1.0
            && f64::from(bounds.y() + bounds.height()) <= f64::from(outer.height()) + 1.0
    })
}

fn assert_last_row_rendered(
    section: &LushtextWorkspaceSection,
    sidebar: &LushtextSidebar,
    count: usize,
) {
    scroll_outer_to_bottom(sidebar);
    let last = file_name(count - 1);
    let labels = rendered_labels(section);
    assert!(
        labels.contains(&last),
        "last row {last} must be rendered after scrolling to the bottom; rendered {} rows, last rendered {:?}",
        labels.len(),
        labels.iter().next_back()
    );
}

// --- Single directory sizes ---

#[test]
fn test_large_directory_renders_last_row_after_scroll() {
    let tree = large_tree(&[("nested", 300)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    assert_last_row_rendered(&tree.section, &tree.sidebar, 300);
    // No band of the section's allocated height is left without a row: the
    // rendered rows tile the outer viewport from top to bottom. A virtualized
    // list can only tile at row boundaries, so the last row may stop up to one
    // row short of the fold — but it must not be drawn *past* it. Rows below
    // the fold mean the list was handed a taller band than the viewport shows,
    // which is how a revealed row ends up behind the workspace header.
    let bottoms: Vec<f64> = rendered_rows(&tree.section)
        .iter()
        .filter_map(|(widget, _)| bottom_in_outer(&tree.sidebar, widget))
        .collect();
    let max_bottom = bottoms.iter().copied().fold(0.0, f64::max);
    let row_height = rendered_rows(&tree.section)
        .iter()
        .filter_map(|(widget, _)| widget.compute_bounds(&*tree.sidebar.imp().outer_scrolled_window))
        .map(|bounds| f64::from(bounds.height()))
        .fold(0.0, f64::max)
        .max(1.0);
    let viewport_height = f64::from(tree.sidebar.imp().outer_scrolled_window.height());
    assert!(
        max_bottom >= viewport_height - row_height,
        "rendered rows must reach the bottom of the viewport within one row \
         (max bottom {max_bottom}, row height {row_height}, viewport {viewport_height})"
    );
    assert!(
        max_bottom <= viewport_height + 1.0,
        "rendered rows must not be drawn below the fold \
         (max bottom {max_bottom}, viewport {viewport_height})"
    );
    drop(tree.window);
}

#[test]
fn test_boundary_row_counts_around_the_gtk_cap_render_last_row() {
    for count in [199usize, 200, 201, 202, 205] {
        let tree = large_tree(&[("nested", count)], 800);
        expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
        assert_last_row_rendered(&tree.section, &tree.sidebar, count);
        tree.window.close();
        flush_events();
    }
}

#[test]
fn test_nested_depth_two_large_directory_is_fully_reachable() {
    let tree = large_tree(&[("_mid/artifacts", 400)], 800);
    let target = tree.root.join("_mid").join("artifacts");
    expand_path(&tree.section, &tree.root, &target);
    let expected: BTreeSet<String> = (0..400).map(file_name).collect();
    let adjustment = outer_adjustment(&tree.sidebar);
    let mut seen = BTreeSet::new();
    let mut value = 0.0;
    while value <= adjustment.upper() - adjustment.page_size() {
        scroll_outer_to(&tree.sidebar, value);
        seen.extend(rendered_labels(&tree.section));
        value += adjustment.page_size() * 0.5;
    }
    scroll_outer_to_bottom(&tree.sidebar);
    seen.extend(rendered_labels(&tree.section));
    let missing: Vec<_> = expected.difference(&seen).collect();
    assert!(
        missing.is_empty(),
        "rows never rendered while scrolling: {missing:?}"
    );
    drop(tree.window);
}

#[test]
fn test_thousand_rows_keep_realized_widgets_bounded_and_range_exact() {
    let tree = large_tree(&[("nested", 1_000)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    let outer = &*tree.sidebar.imp().outer_scrolled_window;
    let initial_rows = rendered_rows(&tree.section);
    let row_height = initial_rows
        .first()
        .map(|(widget, _)| widget.height())
        .expect("a rendered row");
    let viewport_rows = usize::try_from(outer.height() / row_height.max(1)).unwrap_or(0) + 1;
    for value in [0.0, 0.5, 1.0] {
        let adjustment = outer_adjustment(&tree.sidebar);
        scroll_outer_to(
            &tree.sidebar,
            (adjustment.upper() - adjustment.page_size()) * value,
        );
        let rendered = rendered_rows(&tree.section).len();
        // GTK keeps extra realized items around the visible range (roughly one
        // to two more pages); the contract is that the count follows the viewport.
        assert!(
            rendered <= 3 * viewport_rows + 4,
            "rendered {rendered} rows for a viewport of {viewport_rows} rows"
        );
        assert!(rendered < GTK_LIST_VIEW_REALIZED_CAP);
    }
    assert!(rendered_labels(&tree.section).contains(&file_name(999)));
    // The outer range is exact: the slice bin advertises every model row's
    // height even though the list view itself is allocated only the slice.
    // Rows are laid out at a fixed pitch (row height plus list spacing); the
    // list view's own padding is the only other allowed remainder.
    let model_rows = tree
        .section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .map_or(0, gtk4::prelude::ListModelExt::n_items);
    let slice_height = tree.section.imp().file_tree_slice.height();
    let list = &*tree.section.imp().file_tree_view;
    let row_top = |widget: &gtk4::Widget| {
        widget
            .compute_bounds(list)
            .map_or(0.0, |bounds| f64::from(bounds.y()))
    };
    let pitch = row_top(&initial_rows[1].0) - row_top(&initial_rows[0].0);
    assert!(
        pitch >= f64::from(row_height),
        "pitch {pitch} vs row height {row_height}"
    );
    let rows_height = pitch * f64::from(model_rows);
    let remainder = f64::from(slice_height) - rows_height;
    assert!(
        (-pitch..=24.0).contains(&remainder),
        "bin height {slice_height} vs rows {rows_height} (pitch {pitch})"
    );
    assert!(tree.section.imp().file_tree_view.height() < slice_height);
    assert!(outer_adjustment(&tree.sidebar).upper() >= f64::from(slice_height));
    drop(tree.window);
}

#[test]
fn test_realized_rows_follow_the_scroll_position() {
    let tree = large_tree(&[("nested", 1_000)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    let top = rendered_labels(&tree.section);
    let adjustment = outer_adjustment(&tree.sidebar);
    scroll_outer_to(
        &tree.sidebar,
        (adjustment.upper() - adjustment.page_size()) / 2.0,
    );
    let middle = rendered_labels(&tree.section);
    assert!(top.contains(&file_name(0)));
    assert!(
        !middle.contains(&file_name(0)),
        "row 0 must be released once far outside the viewport"
    );
    assert!(
        middle
            .iter()
            .any(|label| label.starts_with("row-004") || label.starts_with("row-005"))
    );
    drop(tree.window);
}

// --- Multiple sections and the workspace filter ---

fn two_workspaces(count: usize) -> (tempfile::TempDir, PathBuf, PathBuf) {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("two workspaces tempdir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    seed_files(&first.join("nested"), count);
    seed_files(&second.join("nested"), count);
    save_workspaces(vec![
        WorkspaceConfig::with_one_folder(WorkspaceId::new("first"), "first", first.clone()),
        WorkspaceConfig::with_one_folder(WorkspaceId::new("second"), "second", second.clone()),
    ]);
    (dir, first, second)
}

#[test]
fn test_two_sections_each_above_the_cap_render_completely() {
    let (_dir, first, second) = two_workspaces(250);
    let (window, sidebar) = present_sidebar_window(800);
    let sections = mapped_sections(&sidebar, 2);
    for (section, root) in sections.iter().zip([&first, &second]) {
        section.expand_folders();
        expand_path(section, root, &root.join("nested"));
    }
    scroll_outer_to_bottom(&sidebar);
    assert!(rendered_labels(&sections[1]).contains(&file_name(249)));
    // The first section's last row sits above the second section: scroll it in.
    let first_last_index =
        tree_index_for_path(&sections[0], &first.join("nested").join(file_name(249)))
            .expect("first last row index");
    sections[0]
        .imp()
        .file_tree_view
        .scroll_to(first_last_index, gtk4::ListScrollFlags::NONE, None);
    flush_after_delay(Duration::from_millis(300));
    assert!(rendered_labels(&sections[0]).contains(&file_name(249)));
    assert!(
        sidebar.imp().workspace_filter_dropdown.is_mapped(),
        "the fixed workspace-scope row stays visible while sections scroll"
    );
    drop(window);
}

#[test]
fn test_workspace_filter_hides_a_large_section_and_restores_it() {
    let (_dir, first, _second) = two_workspaces(300);
    let (window, sidebar) = present_sidebar_window(800);
    let sections = mapped_sections(&sidebar, 2);
    sections[0].expand_folders();
    expand_path(&sections[0], &first, &first.join("nested"));
    assert_last_row_rendered(&sections[0], &sidebar, 300);

    // Narrow the scope to the second workspace.
    sidebar.imp().workspace_filter_dropdown.set_selected(2);
    flush_after_delay(Duration::from_millis(400));
    assert!(
        !sections[0].is_visible(),
        "the filtered-out section is hidden"
    );
    assert_eq!(sections[0].height(), 0);
    assert!(
        rendered_rows(&sections[0]).is_empty() || !sections[0].imp().file_tree_view.is_mapped()
    );

    // `All workspaces` brings it back complete.
    sidebar.imp().workspace_filter_dropdown.set_selected(0);
    flush_after_delay(Duration::from_millis(400));
    assert!(sections[0].is_mapped());
    assert_last_row_rendered(&sections[0], &sidebar, 300);
    drop(window);
}

// --- Navigation ---

#[test]
fn test_focus_traversal_to_the_last_row_keeps_it_in_the_outer_viewport() {
    let tree = large_tree(&[("nested", 300)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    let first = tree_index_for_path(&tree.section, &tree.root.join("nested").join(file_name(0)))
        .expect("first row index");
    let last = tree_index_for_path(
        &tree.section,
        &tree.root.join("nested").join(file_name(299)),
    )
    .expect("last row index");
    let list = tree.section.imp().file_tree_view.clone();
    // Down-arrow in GtkListView calls the same scroll-to-item primitive with
    // FOCUS | SELECT for the next position; drive it directly, checking every
    // tenth row and the final one.
    for index in (first..=last).step_by(10).chain(std::iter::once(last)) {
        list.scroll_to(
            index,
            gtk4::ListScrollFlags::FOCUS | gtk4::ListScrollFlags::SELECT,
            None,
        );
        flush_after_delay(Duration::from_millis(200));
        let expected = file_name((index - first) as usize);
        let widget = rendered_row_widget(&tree.section, &expected)
            .unwrap_or_else(|| panic!("row {expected} must be rendered when focused"));
        assert!(
            inside_outer_viewport(&tree.sidebar, &widget),
            "focused row {expected} must lie inside the outer viewport"
        );
    }
    drop(tree.window);
}

#[test]
fn test_programmatic_select_and_scroll_reaches_a_row_beyond_the_cap() {
    let tree = large_tree(&[("nested", 300)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    let index = select_and_scroll_to(
        &tree.section,
        &tree.root.join("nested").join(file_name(279)),
    );
    let widget = rendered_row_widget(&tree.section, &file_name(279)).expect("row 280 rendered");
    assert!(inside_outer_viewport(&tree.sidebar, &widget));
    assert_eq!(selection(&tree.section).selected(), index);
    drop(tree.window);
}

#[test]
fn test_pending_selection_survives_a_batched_refresh_while_scrolled_to_the_end() {
    let tree = large_tree(&[("nested", 300)], 800);
    let nested = tree.root.join("nested");
    expand_path(&tree.section, &tree.root, &nested);
    tree.section.stop_workspace_watch_for_test();
    let selected = nested.join(file_name(289));
    let index = tree_index_for_path(&tree.section, &selected).expect("row 290");
    selection(&tree.section).set_selected(index);
    scroll_outer_to_bottom(&tree.sidebar);

    for i in 100..220 {
        fixture::remove_file(&nested.join(file_name(i)));
        fixture::write_text(&nested.join(format!("mid-{i:05}.txt")), "");
    }
    tree.section.imp().refresh_button.emit_clicked();
    wait_until(Duration::from_secs(30), || {
        !tree
            .section
            .workspace_section_evidence()
            .refresh_blocks_readiness
            && row_for_path(&tree.section, &nested.join("mid-00219.txt")).is_some()
    });
    flush_after_delay(Duration::from_millis(300));

    let still_selected = selection(&tree.section)
        .selected_item()
        .and_downcast::<gtk4::TreeListRow>()
        .and_then(|row| row.item().and_downcast::<FileTreeItem>())
        .and_then(|item| item.path());
    assert_eq!(still_selected.as_deref(), Some(selected.as_path()));
    let adjustment = outer_adjustment(&tree.sidebar);
    assert!(
        adjustment.value() <= adjustment.upper() - adjustment.page_size() + 0.5,
        "outer value must be clamped after the refresh"
    );
    scroll_outer_to_bottom(&tree.sidebar);
    assert!(rendered_labels(&tree.section).contains(&file_name(299)));
    drop(tree.window);
}

#[test]
fn test_file_peek_anchors_to_a_late_rendered_row() {
    let tree = large_tree(&[("nested", 300)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    select_and_scroll_to(
        &tree.section,
        &tree.root.join("nested").join(file_name(269)),
    );
    // Peek is a keyboard flow (`Space` on the focused list), so the list owns
    // focus before the toggle, exactly as a user would have it.
    tree.section.imp().file_tree_view.grab_focus();
    flush_after_delay(Duration::from_millis(100));
    let sidebar_width_before = tree.sidebar.width();
    assert!(tree.section.toggle_peek_for_selection());
    wait_until(Duration::from_secs(5), || tree.section.peek_visible());
    assert!(rendered_row_widget(&tree.section, &file_name(269)).is_some());
    assert_eq!(
        tree.sidebar.width(),
        sidebar_width_before,
        "peek must not resize the split layout"
    );
    let _ = tree.section.toggle_peek_for_selection();
    flush_after_delay(Duration::from_millis(200));
    assert!(!tree.section.peek_visible());
    drop(tree.window);
}

#[test]
fn test_focus_folder_on_a_large_directory_keeps_the_last_entry_reachable() {
    let tree = large_tree(&[("nested", 300)], 800);
    let nested = tree.root.join("nested");
    expand_path(&tree.section, &tree.root, &nested);
    tree.section.focus_folder(&nested);
    wait_until(Duration::from_secs(10), || {
        !tree.section.imp().drilldown_stack.borrow().is_empty()
            && !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
            && row_for_path(&tree.section, &nested.join(file_name(299))).is_some()
    });
    flush_after_delay(Duration::from_millis(500));
    assert!(tree.section.imp().drilldown_header_box.is_visible());
    scroll_outer_to_bottom(&tree.sidebar);
    assert!(rendered_labels(&tree.section).contains(&file_name(299)));
    drop(tree.window);
}

// --- Geometry under change ---

#[test]
fn test_short_and_tall_windows_both_render_the_last_row() {
    for height in [800, 500] {
        let tree = large_tree(&[("nested", 300)], height);
        expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
        assert_last_row_rendered(&tree.section, &tree.sidebar, 300);
        let adjustment = outer_adjustment(&tree.sidebar);
        assert!(adjustment.value() <= adjustment.upper() - adjustment.page_size() + 0.5);
        tree.window.close();
        flush_events();
    }
}

#[test]
fn test_collapsing_a_large_directory_removes_rows_and_blank_space() {
    let tree = large_tree(&[("nested", 300)], 800);
    let nested = tree.root.join("nested");
    expand_path(&tree.section, &tree.root, &nested);
    let expanded_height = tree.section.imp().file_tree_slice.height();
    row_for_path(&tree.section, &nested)
        .expect("nested row")
        .set_expanded(false);
    flush_after_delay(Duration::from_millis(400));
    let collapsed_height = tree.section.imp().file_tree_slice.height();
    let rows = rendered_rows(&tree.section);
    assert!(collapsed_height < expanded_height / 10);
    // Root, top.txt, nested: three rows whose heights tile the bin, with the
    // list view's own top/bottom padding as the only allowed remainder.
    assert_eq!(rows.len(), 3);
    let tiled: i32 = rows.iter().map(|(widget, _)| widget.height()).sum();
    assert!(
        (0..=24).contains(&(collapsed_height - tiled)),
        "bin height {collapsed_height} vs rows {tiled}"
    );
    let adjustment = outer_adjustment(&tree.sidebar);
    assert!(
        adjustment.upper() <= adjustment.page_size() + 1.0,
        "no scrollable blank space remains"
    );
    drop(tree.window);
}

#[test]
fn test_continuous_outer_scrolling_stays_stable() {
    let tree = large_tree(&[("nested", 1_000)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    let adjustment = outer_adjustment(&tree.sidebar);
    let range = adjustment.upper() - adjustment.page_size();
    let height_before = tree.section.imp().file_tree_slice.height();
    for step in 0..=120 {
        let value = range * (f64::from(step % 61) / 60.0);
        adjustment.set_value(value);
        flush_after_delay(Duration::from_millis(16));
    }
    // GTK warnings during the drag fail the harness; here we prove geometry stability.
    assert_eq!(tree.section.imp().file_tree_slice.height(), height_before);
    flush_after_delay(Duration::from_millis(300));
    // GTK may keep its full anchor window (the cap plus its extra items)
    // realized after a fast drag; it must not grow past that.
    assert!(rendered_rows(&tree.section).len() <= GTK_LIST_VIEW_REALIZED_CAP + 8);
    scroll_outer_to_bottom(&tree.sidebar);
    assert!(rendered_labels(&tree.section).contains(&file_name(999)));
    drop(tree.window);
}

#[test]
fn test_truncation_placeholder_is_reachable_at_the_bottom() {
    let tree = large_tree(&[("nested", 10_050)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    scroll_outer_to_bottom(&tree.sidebar);
    let labels = rendered_labels(&tree.section);
    assert!(
        labels.iter().any(|label| label.contains("showing first")),
        "the truncation placeholder must be rendered at the bottom; got {:?}",
        labels.iter().next_back()
    );
    drop(tree.window);
}

#[test]
fn test_wide_names_in_a_large_directory_stay_clipped_and_reachable() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("wide names tempdir");
    let root = dir.path().join("root");
    let nested = root.join("nested");
    fixture::create_dir_all(&nested);
    for index in 0..300 {
        let name = format!("{index:05}-{}.txt", "wide-name-".repeat(19));
        fixture::write_text(&nested.join(name), "x");
    }
    save_workspaces(vec![WorkspaceConfig::with_one_folder(
        WorkspaceId::new("wide"),
        "wide",
        root.clone(),
    )]);
    let (window, sidebar) = present_sidebar_window(800);
    let section = mapped_sections(&sidebar, 1).remove(0);
    section.expand_folders();
    expand_path(&section, &root, &nested);
    scroll_outer_to_bottom(&sidebar);
    let labels = rendered_labels(&section);
    assert!(labels.iter().any(|label| label.starts_with("00299-")));
    assert_eq!(
        sidebar.imp().outer_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );
    assert!(section.imp().file_tree_view.width() <= sidebar.width());
    let (widget, _) = rendered_rows(&section).pop().expect("a rendered row");
    let label = row_label(&widget).expect("row label");
    assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::End);
    drop(window);
}

#[test]
fn test_late_rows_expose_the_same_accessibility_metadata_as_early_rows() {
    let tree = large_tree(&[("nested", 300)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    let busy_before = tree
        .section
        .workspace_section_evidence()
        .refresh_blocks_readiness;
    let early = labelled_descendant(
        &rendered_row_widget(&tree.section, &file_name(0)).expect("early row rendered"),
    );
    AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&early);
    let early_role = early.accessible_role();

    let index = tree_index_for_path(
        &tree.section,
        &tree.root.join("nested").join(file_name(259)),
    )
    .expect("row 260");
    tree.section
        .imp()
        .file_tree_view
        .scroll_to(index, gtk4::ListScrollFlags::NONE, None);
    flush_after_delay(Duration::from_millis(300));
    let late = labelled_descendant(
        &rendered_row_widget(&tree.section, &file_name(259)).expect("row 260 rendered"),
    );
    AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&late);
    assert_eq!(late.accessible_role(), early_role);
    assert_eq!(
        tree.section
            .workspace_section_evidence()
            .refresh_blocks_readiness,
        busy_before,
        "scrolling must not change readiness"
    );
    drop(tree.window);
}

// --- The workspace header belongs to the user, not to the slice bin ----------
//
// Each section stacks a separator and its header box above the slice bin, all
// inside the one sidebar scroller. A bin that scrolls the outer window while
// merely resting therefore scrolls its own header out of view and pins the
// tree's first row to the top, and the user cannot scroll back: the next
// allocation undoes it. v0.7.0 shipped exactly that. These checks assert the
// sidebar's side of `gtk_lush_adoption`'s widget-level contract, over the
// content-height changes a real sidebar goes through.
//
// Every resting assertion first proves the sidebar *can* scroll. Without that
// guard these checks pass against the defect itself, because a workspace whose
// tree fits the viewport has no scroll range to be stolen — which is how four
// of them were written the first time.

/// Files seeded directly in a workspace folder, enough that the tree overflows
/// any test viewport without a nested expansion step.
const TALL_WORKSPACE_ROWS: usize = 400;

/// One workspace whose folder directly holds `TALL_WORKSPACE_ROWS` files.
fn tall_workspace(height: i32) -> LargeTree {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tall workspace tempdir");
    let root = dir.path().join("root");
    seed_files(&root, TALL_WORKSPACE_ROWS);
    save_workspaces(vec![WorkspaceConfig::with_one_folder(
        WorkspaceId::new("tall"),
        "tall",
        root.clone(),
    )]);
    let (window, sidebar) = present_sidebar_window(height);
    let section = mapped_sections(&sidebar, 1).remove(0);
    section.expand_folders();
    wait_for_refresh_idle(&section);
    flush_after_delay(Duration::from_millis(400));
    LargeTree {
        _dir: dir,
        root,
        window,
        sidebar,
        section,
    }
}

/// Two workspaces whose folders each directly hold `count` files.
fn two_tall_workspaces(
    count: usize,
    height: i32,
) -> (
    tempfile::TempDir,
    LushtextWindow,
    LushtextSidebar,
    Vec<LushtextWorkspaceSection>,
) {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("two tall workspaces tempdir");
    let mut configs = Vec::new();
    for name in ["left", "right"] {
        let root = dir.path().join(name);
        seed_files(&root, count);
        configs.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new(name),
            name,
            root,
        ));
    }
    save_workspaces(configs);
    let (window, sidebar) = present_sidebar_window(height);
    let sections = mapped_sections(&sidebar, 2);
    for section in &sections {
        section.expand_folders();
        wait_for_refresh_idle(section);
    }
    flush_after_delay(Duration::from_millis(600));
    (dir, window, sidebar, sections)
}

/// Poll the sidebar's scroll position so a value that happens to look right at
/// one instant cannot pass for a settled one.
fn settled_outer_values(sidebar: &LushtextSidebar, samples: usize) -> Vec<f64> {
    (0..samples)
        .map(|_| {
            flush_after_delay(Duration::from_millis(120));
            outer_adjustment(sidebar).value()
        })
        .collect()
}

/// Scroll positions within this many logical pixels count as the same place.
const SCROLL_TOLERANCE: f64 = 1.0;

fn header_of(section: &LushtextWorkspaceSection) -> gtk4::Widget {
    section.imp().header_box.clone().upcast()
}

#[track_caller]
fn assert_sidebar_can_scroll(sidebar: &LushtextSidebar, when: &str) {
    let adjustment = outer_adjustment(sidebar);
    assert!(
        adjustment.upper() > adjustment.page_size() + 1.0,
        "this check is vacuous unless the sidebar has scroll range {when}:          content is {} tall in a {} viewport",
        adjustment.upper(),
        adjustment.page_size(),
    );
}

#[track_caller]
fn assert_header_visible(
    sidebar: &LushtextSidebar,
    section: &LushtextWorkspaceSection,
    when: &str,
) {
    let header = header_of(section);
    let bounds = header
        .compute_bounds(&*sidebar.imp().outer_scrolled_window)
        .expect("the workspace header must be allocated");
    assert!(
        inside_outer_viewport(sidebar, &header),
        "the workspace header must stay visible {when}: it sits at y={} height={} in a {}px          viewport, with the sidebar scrolled to {}",
        bounds.y(),
        bounds.height(),
        sidebar.imp().outer_scrolled_window.height(),
        outer_adjustment(sidebar).value(),
    );
}

#[track_caller]
fn assert_sidebar_rests_at_top(sidebar: &LushtextSidebar, when: &str) {
    assert_sidebar_can_scroll(sidebar, when);
    let values = settled_outer_values(sidebar, 12);
    assert!(
        values.iter().all(|value| *value < SCROLL_TOLERANCE),
        "the sidebar must stay where the user left it {when}; scroll positions were {values:?}"
    );
}

#[test]
fn test_workspace_header_is_visible_when_the_sidebar_is_at_rest() {
    let tree = tall_workspace(800);
    assert_sidebar_rests_at_top(&tree.sidebar, "after the workspace loads");
    assert_header_visible(&tree.sidebar, &tree.section, "after the workspace loads");
    drop(tree.window);
}

#[test]
fn test_scrolling_the_sidebar_back_to_the_top_reveals_the_workspace_header() {
    let tree = tall_workspace(800);
    scroll_outer_to_bottom(&tree.sidebar);
    assert!(
        outer_adjustment(&tree.sidebar).value() > 0.0,
        "the sidebar must scroll away from the top before the return trip is meaningful"
    );

    scroll_outer_to(&tree.sidebar, 0.0);
    assert_sidebar_rests_at_top(&tree.sidebar, "after scrolling back to the top");
    assert_header_visible(
        &tree.sidebar,
        &tree.section,
        "after scrolling back to the top",
    );
    drop(tree.window);
}

#[test]
fn test_workspace_header_survives_expanding_a_large_directory() {
    let tree = large_tree(&[("nested", 600)], 800);
    // Expanding multiplies the content height, which re-slices every frame.
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    scroll_outer_to(&tree.sidebar, 0.0);
    assert_sidebar_rests_at_top(&tree.sidebar, "after expanding a large directory");
    assert_header_visible(
        &tree.sidebar,
        &tree.section,
        "after expanding a large directory",
    );
    drop(tree.window);
}

#[test]
fn test_workspace_header_survives_a_manual_refresh() {
    let tree = tall_workspace(800);
    tree.section.imp().refresh_button.emit_clicked();
    wait_for_refresh_idle(&tree.section);
    assert_sidebar_rests_at_top(&tree.sidebar, "after a manual refresh");
    assert_header_visible(&tree.sidebar, &tree.section, "after a manual refresh");
    drop(tree.window);
}

#[test]
fn test_workspace_header_survives_collapsing_and_expanding_the_section() {
    let tree = tall_workspace(800);
    tree.section.set_section_body_collapsed(true);
    flush_after_delay(Duration::from_millis(400));
    // Collapsed, the tree leaves the scroller entirely; only the header's own
    // visibility is meaningful here, so this half makes no resting claim.
    assert_header_visible(
        &tree.sidebar,
        &tree.section,
        "while the section body is collapsed",
    );

    tree.section.set_section_body_collapsed(false);
    wait_for_refresh_idle(&tree.section);
    scroll_outer_to(&tree.sidebar, 0.0);
    assert_sidebar_rests_at_top(&tree.sidebar, "after re-expanding the section body");
    assert_header_visible(
        &tree.sidebar,
        &tree.section,
        "after re-expanding the section body",
    );
    drop(tree.window);
}

#[test]
fn test_workspace_header_survives_toggling_hidden_files() {
    // v0.7.0's other sidebar feature changes the tree's height from a settings
    // key, through the automatic refresh path rather than a user scroll.
    let tree = tall_workspace(800);
    let settings = gtk4::gio::Settings::new(lushtext_core::config::APP_ID);
    settings.reset(lushtext_core::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
    for show_hidden in [true, false] {
        settings
            .set_boolean(
                lushtext_core::config::keys::WORKSPACE_SHOW_HIDDEN_FILES,
                show_hidden,
            )
            .expect("set show-hidden key");
        wait_for_refresh_idle(&tree.section);
        scroll_outer_to(&tree.sidebar, 0.0);
        assert_sidebar_rests_at_top(&tree.sidebar, "after toggling hidden files");
        assert_header_visible(&tree.sidebar, &tree.section, "after toggling hidden files");
    }
    settings.reset(lushtext_core::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
    flush_events();
    drop(tree.window);
}

#[test]
fn test_workspace_header_is_visible_in_a_short_sidebar() {
    // A viewport barely taller than the chrome above the tree is the position
    // where a forced scroll hides the header most completely.
    let tree = tall_workspace(320);
    assert_sidebar_rests_at_top(&tree.sidebar, "in a short window");
    assert_header_visible(&tree.sidebar, &tree.section, "in a short window");
    drop(tree.window);
}

#[test]
fn test_two_workspace_sections_reach_both_ends_without_oscillating() {
    let (_dir, window, sidebar, sections) = two_tall_workspaces(300, 800);
    assert_sidebar_can_scroll(&sidebar, "with two workspace sections");

    let adjustment = outer_adjustment(&sidebar);
    let bottom = adjustment.upper() - adjustment.page_size();
    scroll_outer_to_bottom(&sidebar);
    let at_bottom = settled_outer_values(&sidebar, 15);
    assert!(
        at_bottom
            .iter()
            .all(|value| (value - bottom).abs() < SCROLL_TOLERANCE),
        "two workspace sections must not pull the sidebar back and forth at the bottom; \
         wanted {bottom}, saw {at_bottom:?}"
    );

    scroll_outer_to(&sidebar, 0.0);
    assert_sidebar_rests_at_top(&sidebar, "with two workspace sections");
    assert_header_visible(&sidebar, &sections[0], "with two workspace sections");
    drop(window);
}

/// Scroll the outer adjustment the way a wheel does: many small deltas, each
/// given a chance to allocate, instead of one jump to a target value.
///
/// This distinction is the whole point of the tests below. `scroll_outer_to`
/// proves a *settled end state*: set the value once, wait for everything to
/// quiesce, assert. A wheel produces a sequence of deltas interleaved with
/// allocation passes, and `GtkListView` revises its total-height estimate as
/// rows realize, so `upper` can move underneath a trip that is already in
/// progress. A single `set_value` can never reproduce that interleaving.
fn wheel_scroll_by(sidebar: &LushtextSidebar, delta: f64, steps: usize) {
    let adjustment = outer_adjustment(sidebar);
    for _ in 0..steps {
        let target = (adjustment.value() + delta)
            .clamp(0.0, (adjustment.upper() - adjustment.page_size()).max(0.0));
        adjustment.set_value(target);
        // One short settle per delta: enough for an allocation pass to run,
        // far too short to let a revised height estimate quiesce.
        flush_after_delay(Duration::from_millis(16));
    }
}

#[test]
fn test_wheel_scrolling_back_to_the_top_reveals_the_workspace_header() {
    let tree = tall_workspace(800);
    let adjustment = outer_adjustment(&tree.sidebar);
    let span = (adjustment.upper() - adjustment.page_size()).max(0.0);
    assert!(
        span > 0.0,
        "the fixture must be scrollable for the round trip to mean anything"
    );

    // Down in wheel-sized increments, which is what realizes new rows and lets
    // the list revise the content height the bin is slicing against.
    wheel_scroll_by(&tree.sidebar, span / 12.0, 14);
    assert!(
        outer_adjustment(&tree.sidebar).value() > 0.0,
        "the sidebar must leave the top before the return trip is meaningful"
    );

    // Back up the same way. The user's gesture, not a jump.
    wheel_scroll_by(&tree.sidebar, -span / 12.0, 14);

    assert_sidebar_rests_at_top(&tree.sidebar, "after wheel-scrolling back to the top");
    assert_header_visible(
        &tree.sidebar,
        &tree.section,
        "after wheel-scrolling back to the top",
    );
    drop(tree.window);
}

#[test]
fn test_repeated_wheel_round_trips_keep_the_workspace_header_reachable() {
    // The reported symptom was intermittent: the first return trip worked and
    // later ones mostly did not. A single round trip can therefore pass while
    // the defect is present, so this repeats the gesture.
    let tree = tall_workspace(800);
    let adjustment = outer_adjustment(&tree.sidebar);
    let span = (adjustment.upper() - adjustment.page_size()).max(0.0);
    assert!(span > 0.0, "the fixture must be scrollable");

    for trip in 1..=4 {
        wheel_scroll_by(&tree.sidebar, span / 10.0, 12);
        wheel_scroll_by(&tree.sidebar, -span / 10.0, 12);
        assert_sidebar_rests_at_top(&tree.sidebar, &format!("after wheel round trip {trip}"));
        assert_header_visible(
            &tree.sidebar,
            &tree.section,
            &format!("after wheel round trip {trip}"),
        );
    }
    drop(tree.window);
}

// ---------------------------------------------------------------------------
// Rendered-row stability: where a row is drawn, not what the adjustment says.
//
// Every earlier check in this file reads adjustment values, or accepts a row
// anywhere inside the viewport with a pixel of slack. None pins a row's exact
// bounds across an event, which is how a few-pixel jitter on every click
// passed the whole suite. These sample a named row's placement relative to its
// workspace header across forced layout passes and require it not to move.
// ---------------------------------------------------------------------------

/// Top and height of a row relative to its section header.
type RowPlacement = (f32, f32);

/// Run a layout pass now instead of waiting for the compositor to deliver a
/// frame: `flush_after_delay` alone only pumps the main loop.
fn force_section_layout(section: &LushtextWorkspaceSection) {
    section.imp().file_tree_slice.queue_allocate();
    flush_events();
    flush_after_delay(Duration::from_millis(16));
}

/// Where the row labelled `label` is drawn, resolved by label each time
/// because the list recycles row widgets.
fn row_placement(section: &LushtextWorkspaceSection, label: &str) -> Option<RowPlacement> {
    rendered_row_widget(section, label)
        .filter(WidgetExt::is_mapped)?
        .compute_bounds(&header_of(section))
        .map(|bounds| (bounds.y(), bounds.height()))
}

fn sampled_row_placements(
    section: &LushtextWorkspaceSection,
    label: &str,
    samples: usize,
) -> Vec<RowPlacement> {
    (0..samples)
        .map(|_| {
            force_section_layout(section);
            row_placement(section, label)
                .unwrap_or_else(|| panic!("row {label} must stay rendered while sampled"))
        })
        .collect()
}

/// Labels of the section's rows currently drawn wholly inside the viewport.
fn fully_visible_labels(
    sidebar: &LushtextSidebar,
    section: &LushtextWorkspaceSection,
) -> Vec<String> {
    rendered_rows(section)
        .into_iter()
        // `GtkListView` keeps rows around the selection and focus realized
        // but child-invisible; they have bounds and are not drawn.
        .filter(|(widget, _)| widget.is_mapped())
        .filter(|(widget, _)| inside_outer_viewport(sidebar, widget))
        .map(|(_, label)| label)
        .filter(|label| !label.is_empty())
        .collect()
}

/// Select, then focus, several already-visible rows and require the first
/// visible row to be drawn at exactly the same place throughout, with the
/// outer scroller unmoved.
#[track_caller]
fn assert_rows_still_across_selection(
    sidebar: &LushtextSidebar,
    section: &LushtextWorkspaceSection,
    root: &Path,
    when: &str,
) {
    let outer = outer_adjustment(sidebar);
    let resting = outer.value();
    let visible = fully_visible_labels(sidebar, section);
    assert!(
        visible.len() >= 5,
        "this check needs at least five fully visible rows {when}; saw {}",
        visible.len()
    );
    let anchor = visible[0].clone();
    let baseline = sampled_row_placements(section, &anchor, 3);
    assert!(
        baseline.iter().all(|placement| *placement == baseline[0]),
        "the anchor row must rest before selection starts {when}; saw {baseline:?}"
    );
    let expected = baseline[0];

    for target in &visible[1..visible.len().min(8)] {
        let index = tree_index_for_path(section, &root.join(target))
            .unwrap_or_else(|| panic!("tree index for {target}"));
        selection(section).set_selected(index);
        let selected = sampled_row_placements(section, &anchor, 3);
        if let Some(row) = rendered_row_widget(section, target) {
            row.grab_focus();
        }
        let focused = sampled_row_placements(section, &anchor, 3);
        assert!(
            (outer.value() - resting).abs() < 0.5,
            "selecting an already visible row must not move the sidebar {when}: {target} moved \
             it from {resting} to {}",
            outer.value()
        );
        for (how, samples) in [("selecting", &selected), ("focusing", &focused)] {
            assert!(
                samples.iter().all(|placement| *placement == expected),
                "{how} {target} must not move the rendered rows {when}: {anchor} rested at \
                 (top, height) = {expected:?} relative to the header and was drawn at {samples:?}"
            );
        }
    }
}

/// After an event that may change the model or content height, the rows must
/// come to rest: once refresh is idle, repeated forced layouts draw the first
/// rendered row in one place.
#[track_caller]
fn assert_rows_settle(section: &LushtextWorkspaceSection, when: &str) {
    wait_for_refresh_idle(section);
    flush_after_delay(Duration::from_millis(200));
    let anchor = rendered_rows(section)
        .into_iter()
        .map(|(_, label)| label)
        .find(|label| !label.is_empty())
        .expect("a rendered row to anchor on");
    let samples = sampled_row_placements(section, &anchor, 6);
    assert!(
        samples.iter().all(|placement| *placement == samples[0]),
        "rows must settle {when}: {anchor} was drawn at {samples:?} across forced layouts"
    );
}

#[test]
fn test_selecting_rows_at_the_top_keeps_rendered_rows_still() {
    let tree = tall_workspace(800);
    scroll_outer_to(&tree.sidebar, 0.0);
    assert_rows_still_across_selection(&tree.sidebar, &tree.section, &tree.root, "at the top");
    drop(tree.window);
}

#[test]
fn test_selecting_rows_mid_content_keeps_rendered_rows_still() {
    let tree = tall_workspace(800);
    let adjustment = outer_adjustment(&tree.sidebar);
    scroll_outer_to(
        &tree.sidebar,
        (adjustment.upper() - adjustment.page_size()) / 2.0,
    );
    flush_after_delay(Duration::from_millis(300));
    assert!(adjustment.value() > 1.0, "the fixture must have scrolled");
    assert_rows_still_across_selection(&tree.sidebar, &tree.section, &tree.root, "mid-content");
    drop(tree.window);
}

#[test]
fn test_selecting_rows_in_a_short_sidebar_keeps_rendered_rows_still() {
    let tree = tall_workspace(420);
    scroll_outer_to(&tree.sidebar, 0.0);
    assert_rows_still_across_selection(
        &tree.sidebar,
        &tree.section,
        &tree.root,
        "in a short window",
    );
    drop(tree.window);
}

#[test]
fn test_selecting_rows_with_two_sections_keeps_rendered_rows_still() {
    let (dir, window, sidebar, sections) = two_tall_workspaces(300, 800);
    scroll_outer_to(&sidebar, 0.0);
    assert_rows_still_across_selection(
        &sidebar,
        &sections[0],
        &dir.path().join("left"),
        "with two workspace sections",
    );
    drop(window);
}

#[test]
fn test_rendered_rows_stay_still_across_a_manual_refresh() {
    // Refresh over an identical tree changes neither model nor height, so the
    // rows may not move across it either.
    let tree = tall_workspace(800);
    scroll_outer_to(&tree.sidebar, 0.0);
    let anchor = fully_visible_labels(&tree.sidebar, &tree.section)
        .into_iter()
        .next()
        .expect("a visible row");
    let before = sampled_row_placements(&tree.section, &anchor, 3);
    tree.section.imp().refresh_button.emit_clicked();
    wait_for_refresh_idle(&tree.section);
    let after = sampled_row_placements(&tree.section, &anchor, 3);
    assert!(
        before
            .iter()
            .chain(&after)
            .all(|placement| *placement == before[0]),
        "a refresh over an identical tree must not move the rows: {anchor} was at {before:?} \
         and after the refresh at {after:?}"
    );
    drop(tree.window);
}

#[test]
fn test_rendered_rows_settle_after_model_changes() {
    // These events change the model or the content height, so no comparison
    // across the event is made; the rows must come to rest afterwards.
    let tree = tall_workspace(800);
    scroll_outer_to(&tree.sidebar, 0.0);

    tree.section.set_section_body_collapsed(true);
    flush_after_delay(Duration::from_millis(300));
    tree.section.set_section_body_collapsed(false);
    assert_rows_settle(
        &tree.section,
        "after collapsing and re-expanding the section body",
    );

    let settings = gtk4::gio::Settings::new(lushtext_core::config::APP_ID);
    settings.reset(lushtext_core::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
    for show_hidden in [true, false] {
        settings
            .set_boolean(
                lushtext_core::config::keys::WORKSPACE_SHOW_HIDDEN_FILES,
                show_hidden,
            )
            .expect("set show-hidden key");
        assert_rows_settle(&tree.section, "after toggling hidden files");
    }
    settings.reset(lushtext_core::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
    flush_events();
    drop(tree.window);
}

#[test]
fn test_a_row_clipped_at_the_slice_edge_still_pulls_the_sidebar_by_its_overflow() {
    // Positive control: a genuine few-pixel request must still be honoured,
    // and honoured by about its overflow rather than by the header height.
    let tree = tall_workspace(800);
    // Nudge so no row boundary coincides with the viewport bottom.
    scroll_outer_to(&tree.sidebar, 7.0);
    let outer = outer_adjustment(&tree.sidebar);
    let resting = outer.value();
    let viewport_bottom = f64::from(tree.sidebar.imp().outer_scrolled_window.height());
    let (label, overflow) = rendered_rows(&tree.section)
        .into_iter()
        .find_map(|(widget, label)| {
            let bottom = bottom_in_outer(&tree.sidebar, &widget)?;
            let top = widget
                .compute_bounds(&*tree.sidebar.imp().outer_scrolled_window)
                .map(|bounds| f64::from(bounds.y()))?;
            let overflow = bottom - viewport_bottom;
            (top < viewport_bottom && overflow > 0.5 && !label.is_empty())
                .then_some((label, overflow))
        })
        .expect("a row straddling the viewport bottom");
    let index = tree_index_for_path(&tree.section, &tree.root.join(&label)).expect("row index");
    tree.section
        .imp()
        .file_tree_view
        .scroll_to(index, gtk4::ListScrollFlags::FOCUS, None);
    wait_until(Duration::from_secs(5), || {
        (outer.value() - resting).abs() > 0.5
    });
    flush_after_delay(Duration::from_millis(400));
    let moved = outer.value() - resting;
    assert!(
        (moved - overflow).abs() <= 2.0,
        "revealing {label}, clipped by {overflow:.1}px, must scroll the sidebar by about that \
         much; it moved {moved:.1}px"
    );
    drop(tree.window);
}

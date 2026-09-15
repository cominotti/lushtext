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

/// Seed one workspace root holding `top.txt` plus the given nested
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

/// Expand every ancestor between the workspace root and `target`, then
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
    // rendered rows tile the outer viewport from top to bottom.
    let max_bottom = rendered_rows(&tree.section)
        .iter()
        .filter_map(|(widget, _)| bottom_in_outer(&tree.sidebar, widget))
        .fold(0.0, f64::max);
    let viewport_height = f64::from(tree.sidebar.imp().outer_scrolled_window.height());
    assert!(
        max_bottom >= viewport_height - 1.0,
        "rendered rows must reach the bottom of the viewport (max bottom {max_bottom}, viewport {viewport_height})"
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

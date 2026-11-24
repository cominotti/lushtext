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
    RowPlacement, RowStillnessProbe, activate_reveal_workspace_path, assert_reveal_then_rest,
    assert_rows_still_across_selection, ensure_gtk_init, find_descendant, first_label, fixture,
    flush_after_delay, flush_events, force_layout, mapped_list_rows, mapped_row_with_label,
    observed_between_dispatches, pin_scroller_height, placement_relative_to, present_window,
    realized_list_rows, row_straddling_bottom, sample_placements, selected_tree_path, test_window,
    wait_until, wait_until_or_false,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceFolder, WorkspaceId, WorkspacesFile,
};
use lushtext_core::services::{json_store, workspace_manager};
use lushtext_core::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_core::ui::sidebar::LushtextSidebar;
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::sidebar::policy::WorkspaceRevealStatus;
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
    let tree = present_single_folder(dir, root, "virtualized", height);
    tree.section.expand_folders();
    tree
}

/// Save one workspace `id` holding the seeded folder `root` (kept alive by
/// `dir`), present a window of `height`, and return its one mapped section,
/// still collapsed.
fn present_single_folder(
    dir: tempfile::TempDir,
    root: PathBuf,
    id: &str,
    height: i32,
) -> LargeTree {
    save_workspaces(vec![WorkspaceConfig::with_one_folder(
        WorkspaceId::new(id),
        id,
        root.clone(),
    )]);
    let (window, sidebar) = present_sidebar_window(height);
    let section = mapped_sections(&sidebar, 1).remove(0);
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

/// Labels of the section's rows GTK maps **and** draws inside the outer
/// viewport, at least in part. A mapped row is not necessarily on screen
/// (ledger A10): `GtkListView` maps rows it allocates outside its own band,
/// and a slice bin keeps its band allocated after the outer scroller has moved
/// past it, so "the last row is rendered" must be checked against the
/// viewport, not against the realized or mapped set.
fn drawn_labels(sidebar: &LushtextSidebar, section: &LushtextWorkspaceSection) -> BTreeSet<String> {
    let outer = &*sidebar.imp().outer_scrolled_window;
    let viewport = f64::from(outer.height());
    mapped_list_rows(&section.imp().file_tree_view)
        .filter(|row| {
            row.compute_bounds(outer).is_some_and(|bounds| {
                f64::from(bounds.y() + bounds.height()) > 0.0 && f64::from(bounds.y()) < viewport
            })
        })
        .filter_map(|row| row_label(&row).map(|label| label.text().to_string()))
        .collect()
}

fn assert_last_row_rendered(
    section: &LushtextWorkspaceSection,
    sidebar: &LushtextSidebar,
    count: usize,
) {
    scroll_outer_to_bottom(sidebar);
    let last = file_name(count - 1);
    let labels = drawn_labels(sidebar, section);
    assert!(
        labels.contains(&last),
        "last row {last} must be drawn in the viewport after scrolling to the bottom; drawn {} rows, last drawn {:?}",
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
    assert!(drawn_labels(&tree.sidebar, &tree.section).contains(&file_name(999)));
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
    assert!(drawn_labels(&sidebar, &sections[1]).contains(&file_name(249)));
    // The first section sits above the viewport: ask for a row near its end
    // and it must be drawn, not merely realized. (The row covering the
    // one-pixel band an off-screen bin keeps at its edge, here the last one,
    // is the recorded edge-row residual: GTK treats a row that covers its
    // whole view as already visible. See the programme record, phase 3.)
    let near_last_index =
        tree_index_for_path(&sections[0], &first.join("nested").join(file_name(248)))
            .expect("first section's second-to-last row index");
    sections[0]
        .imp()
        .file_tree_view
        .scroll_to(near_last_index, gtk4::ListScrollFlags::NONE, None);
    let drawn_both = || {
        let drawn = drawn_labels(&sidebar, &sections[0]);
        drawn.contains(&file_name(248)) && drawn.contains(&file_name(249))
    };
    wait_until_or_false(Duration::from_secs(5), drawn_both);
    let drawn = drawn_labels(&sidebar, &sections[0]);
    assert!(
        drawn.contains(&file_name(248)) && drawn.contains(&file_name(249)),
        "a row requested in a section above the viewport must be scrolled on screen; drawn {drawn:?}"
    );
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

    let still_selected = selected_tree_path(&tree.section);
    assert_eq!(still_selected.as_deref(), Some(selected.as_path()));
    let adjustment = outer_adjustment(&tree.sidebar);
    assert!(
        adjustment.value() <= adjustment.upper() - adjustment.page_size() + 0.5,
        "outer value must be clamped after the refresh"
    );
    scroll_outer_to_bottom(&tree.sidebar);
    assert!(drawn_labels(&tree.sidebar, &tree.section).contains(&file_name(299)));
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
    assert!(drawn_labels(&tree.sidebar, &tree.section).contains(&file_name(299)));
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
    assert!(drawn_labels(&tree.sidebar, &tree.section).contains(&file_name(999)));
    drop(tree.window);
}

/// Run `test` with the sidebar's directory budget lowered to `bytes`.
fn with_directory_budget(bytes: u64, test: impl FnOnce()) {
    lushtext_core::ui::sidebar::set_sidebar_directory_retained_bytes_for_test(bytes);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
    lushtext_core::ui::sidebar::set_sidebar_directory_retained_bytes_for_test(0);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// The listing evidence of `directory`, from the section's plain mirrors.
fn listing(
    section: &LushtextWorkspaceSection,
    directory: &Path,
) -> lushtext_core::ui::sidebar::evidence::DirectoryListingEvidence {
    section
        .workspace_section_evidence()
        .directory_listings
        .into_iter()
        .find(|listing| listing.path == directory)
        .expect("the directory is materialized")
}

/// A directory past the per-directory budget ends with the budget row,
/// rendered at the bottom, stating exactly X of Y.
#[test]
fn test_truncation_placeholder_is_reachable_at_the_bottom() {
    with_directory_budget(256 * 1024, || {
        let tree = large_tree(&[("nested", 2_000)], 800);
        let nested = tree.root.join("nested");
        expand_path(&tree.section, &tree.root, &nested);
        let evidence = listing(&tree.section, &nested);
        assert!(evidence.directory_limited);
        assert_eq!(evidence.directory_total_entries, 2_000);
        let shown = evidence.directory_retained_entries;
        assert!(
            shown > 0 && shown < 2_000,
            "the budget cut the listing: {shown}"
        );
        assert_eq!(evidence.loading_tail_rows, 0);
        scroll_outer_to_bottom(&tree.sidebar);
        let expected = format!("Showing {} of 2,000 entries", group_thousands(shown));
        let labels = drawn_labels(&tree.sidebar, &tree.section);
        assert!(
            labels.contains(&expected),
            "the budget row must be drawn at the bottom; got {:?}",
            labels.iter().next_back()
        );
        // X entries above it, in sort order.
        assert!(labels.contains(&file_name(shown - 1)));
        drop(tree.window);
    });
}

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

/// No count cap (`bound-lists-by-budget-not-count`): 16,385 entries are all
/// listed, with no placeholder row, and the last one is drawn.
#[test]
fn test_directory_past_mains_cap_lists_every_entry() {
    let tree = large_tree(&[("nested", 16 * 1024 + 1)], 800);
    let nested = tree.root.join("nested");
    expand_path(&tree.section, &tree.root, &nested);
    let evidence = listing(&tree.section, &nested);
    assert_eq!(evidence.directory_retained_entries, 16 * 1024 + 1);
    assert_eq!(evidence.directory_total_entries, 16 * 1024 + 1);
    assert!(!evidence.directory_limited);
    assert_eq!(
        evidence.loading_tail_rows, 0,
        "no loading row after the terminal"
    );
    assert_last_row_rendered(&tree.section, &tree.sidebar, 16 * 1024 + 1);
    assert!(
        !rendered_labels(&tree.section)
            .iter()
            .any(|label| label.contains("showing first") || label.starts_with("Showing ")),
        "no placeholder row ends a directory within its budget"
    );
    let section_evidence = tree.section.workspace_section_evidence();
    assert_eq!(section_evidence.cache_full_rebuilds, 0);
    drop(tree.window);
}

/// 131,072 entries are listed and reachable: the loading row ends the
/// directory while batches remain and readiness stays pending, the first rows
/// are interactive before the last batch, the last row is drawn after a
/// focus scroll (what keyboard traversal does) and after a path reveal, and
/// realized rows stay bounded (A1).
#[test]
fn test_directory_of_131072_entries_is_listed_and_reachable() {
    let tree = large_tree(&[("nested", 128 * 1024)], 800);
    let nested = tree.root.join("nested");
    wait_until(Duration::from_secs(10), || {
        row_for_path(&tree.section, &nested).is_some()
    });
    row_for_path(&tree.section, &nested)
        .expect("directory row")
        .set_expanded(true);
    let mut saw_loading = false;
    let mut first_rows_while_loading = false;
    wait_until(Duration::from_secs(240), || {
        let evidence = tree.section.workspace_section_evidence();
        let loading = evidence
            .directory_listings
            .iter()
            .any(|listing| listing.path == nested && listing.loading_tail_rows == 1);
        if loading {
            saw_loading = true;
            assert!(
                evidence.refresh_blocks_readiness,
                "readiness stays pending while the directory loads"
            );
            if rendered_labels(&tree.section).contains(&file_name(0)) {
                first_rows_while_loading = true;
            }
        }
        evidence.directory_listings.iter().any(|listing| {
            listing.path == nested
                && listing.directory_retained_entries == 128 * 1024
                && listing.loading_tail_rows == 0
        }) && !evidence.refresh_blocks_readiness
    });
    assert!(
        saw_loading,
        "a large directory shows its loading row while it loads"
    );
    assert!(
        first_rows_while_loading,
        "the first screen of rows is interactive before the last batch"
    );
    let evidence = listing(&tree.section, &nested);
    assert_eq!(evidence.directory_total_entries, 128 * 1024);
    assert!(!evidence.directory_limited);
    assert_eq!(
        tree.section
            .workspace_section_evidence()
            .cache_full_rebuilds,
        0
    );

    let last = nested.join(file_name(128 * 1024 - 1));
    select_and_scroll_to(&tree.section, &last);
    flush_after_delay(Duration::from_millis(300));
    assert!(
        drawn_labels(&tree.sidebar, &tree.section).contains(&file_name(128 * 1024 - 1)),
        "the last of 131,072 rows is drawn after a focus scroll"
    );
    assert!(rendered_rows(&tree.section).len() <= GTK_LIST_VIEW_REALIZED_CAP + 8);

    let middle = nested.join(file_name(64 * 1024));
    assert_eq!(
        reveal_and_settle(&tree.window, &tree.sidebar, &middle),
        WorkspaceRevealStatus::Revealed
    );
    assert!(drawn_labels(&tree.sidebar, &tree.section).contains(&file_name(64 * 1024)));
    drop(tree.window);
}

/// Collapsing, and refreshing, a directory that is still loading leaves no
/// stale batch, cache entry or loading row, and the next load completes.
#[test]
fn test_superseding_a_loading_directory_leaves_no_stale_rows() {
    let tree = large_tree(&[("nested", 20_000)], 800);
    let nested = tree.root.join("nested");
    tree.section
        .set_reconciliation_batch_delay_for_test(Duration::from_millis(30));
    wait_until(Duration::from_secs(10), || {
        row_for_path(&tree.section, &nested).is_some()
    });
    let row = row_for_path(&tree.section, &nested).expect("directory row");
    row.set_expanded(true);
    wait_until(Duration::from_secs(30), || {
        tree.section
            .workspace_section_evidence()
            .directory_listings
            .iter()
            .any(|listing| listing.path == nested && listing.loading_tail_rows == 1)
    });
    // Collapse mid-load: once the section settles, no loading row is left
    // behind by the collapsed directory's load.
    row.set_expanded(false);
    wait_for_refresh_idle(&tree.section);
    flush_after_delay(Duration::from_millis(200));
    assert!(
        !tree
            .section
            .workspace_section_evidence()
            .directory_listings
            .iter()
            .any(|listing| listing.loading_tail_rows > 0),
        "no loading row survives a collapse"
    );
    // Expand again, and refresh while it loads.
    row_for_path(&tree.section, &nested)
        .expect("directory row")
        .set_expanded(true);
    wait_until(Duration::from_secs(30), || {
        tree.section
            .workspace_section_evidence()
            .directory_listings
            .iter()
            .any(|listing| listing.path == nested && listing.loading_tail_rows == 1)
    });
    tree.section.imp().refresh_button.emit_clicked();
    tree.section
        .set_reconciliation_batch_delay_for_test(Duration::ZERO);
    wait_for_refresh_idle(&tree.section);
    flush_after_delay(Duration::from_millis(300));
    wait_for_refresh_idle(&tree.section);
    let evidence = listing(&tree.section, &nested);
    assert_eq!(evidence.directory_retained_entries, 20_000);
    assert_eq!(evidence.loading_tail_rows, 0);
    let section = tree.section.workspace_section_evidence();
    assert_eq!(
        section.cache_full_rebuilds, 0,
        "no batch left the caches stale"
    );
    assert_last_row_rendered(&tree.section, &tree.sidebar, 20_000);
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
    let tree = root_with_files("tall", TALL_WORKSPACE_ROWS, height);
    tree.section.expand_folders();
    wait_for_refresh_idle(&tree.section);
    flush_after_delay(Duration::from_millis(400));
    tree
}

/// One workspace `id` whose folder directly holds `count` files, presented
/// collapsed.
fn root_with_files(id: &str, count: usize, height: i32) -> LargeTree {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("workspace tempdir");
    let root = dir.path().join("root");
    seed_files(&root, count);
    present_single_folder(dir, root, id, height)
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
    let settings = lushtext_core::ui::settings_writes::shared();
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
// passed the whole suite. The probes live in `common`; this suite supplies the
// sidebar's way of resolving a label and the workspace header as reference.
// ---------------------------------------------------------------------------

fn file_tree_view(section: &LushtextWorkspaceSection) -> gtk4::ListView {
    section.imp().file_tree_view.clone()
}

fn force_section_layout(section: &LushtextWorkspaceSection) {
    force_layout([section.imp().file_tree_slice.upcast_ref::<gtk4::Widget>()]);
}

/// Where the row labelled `label` is drawn relative to its workspace header.
fn row_placement(section: &LushtextWorkspaceSection, label: &str) -> Option<RowPlacement> {
    let row = mapped_row_with_label(&file_tree_view(section), label)?;
    placement_relative_to(&row, &header_of(section))
}

fn sampled_row_placements(
    section: &LushtextWorkspaceSection,
    label: &str,
    samples: usize,
) -> Vec<RowPlacement> {
    sample_placements(
        samples,
        || force_section_layout(section),
        || row_placement(section, label),
    )
}

/// Labels of the section's rows currently drawn wholly inside the viewport.
fn fully_visible_labels(
    sidebar: &LushtextSidebar,
    section: &LushtextWorkspaceSection,
) -> Vec<String> {
    mapped_list_rows(&file_tree_view(section))
        .filter(|row| inside_outer_viewport(sidebar, row))
        .filter_map(|row| first_label(&row).map(|label| label.text().to_string()))
        .collect()
}

#[track_caller]
fn assert_sidebar_rows_still_across_selection(
    sidebar: &LushtextSidebar,
    section: &LushtextWorkspaceSection,
    root: &Path,
    when: &str,
) {
    let outer = outer_adjustment(sidebar);
    let visible_labels = || fully_visible_labels(sidebar, section);
    let select = |label: &str| {
        let index = tree_index_for_path(section, &root.join(label))
            .unwrap_or_else(|| panic!("tree index for {label}"));
        selection(section).set_selected(index);
    };
    let focus = |label: &str| {
        if let Some(row) = mapped_row_with_label(&file_tree_view(section), label) {
            row.grab_focus();
        }
    };
    let sample = |label: &str, samples: usize| sampled_row_placements(section, label, samples);
    assert_rows_still_across_selection(
        &RowStillnessProbe {
            outer: &outer,
            visible_labels: &visible_labels,
            select: &select,
            focus: &focus,
            sample: &sample,
        },
        when,
    );
}

/// After an event that may change the model or content height, the rows must
/// come to rest: once refresh is idle, repeated forced layouts draw the first
/// rendered row in one place.
#[track_caller]
fn assert_rows_settle(section: &LushtextWorkspaceSection, when: &str) {
    wait_for_refresh_idle(section);
    flush_after_delay(Duration::from_millis(200));
    let anchor = mapped_list_rows(&file_tree_view(section))
        .find_map(|row| first_label(&row).map(|label| label.text().to_string()))
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
    assert_sidebar_rows_still_across_selection(
        &tree.sidebar,
        &tree.section,
        &tree.root,
        "at the top",
    );
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
    assert_sidebar_rows_still_across_selection(
        &tree.sidebar,
        &tree.section,
        &tree.root,
        "mid-content",
    );
    drop(tree.window);
}

#[test]
fn test_selecting_rows_in_a_short_sidebar_keeps_rendered_rows_still() {
    let tree = tall_workspace(420);
    scroll_outer_to(&tree.sidebar, 0.0);
    assert_sidebar_rows_still_across_selection(
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
    assert_sidebar_rows_still_across_selection(
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

    let settings = lushtext_core::ui::settings_writes::shared();
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
fn test_a_row_clipped_at_the_slice_edge_is_still_revealed_and_the_sidebar_then_rests() {
    // Positive control for the stillness checks; the travel is the list's
    // decision (see `gtk_lush_widgets::outer_scroll_request`).
    let tree = tall_workspace(800);
    // Nudge so no row boundary coincides with the viewport bottom.
    scroll_outer_to(&tree.sidebar, 7.0);
    let outer = outer_adjustment(&tree.sidebar);
    let resting = outer.value();
    let list = file_tree_view(&tree.section);
    let (row, overflow) = row_straddling_bottom(&list, &*tree.sidebar.imp().outer_scrolled_window)
        .expect("a row straddling the viewport bottom");
    let label = first_label(&row)
        .expect("a labelled row")
        .text()
        .to_string();
    let index = tree_index_for_path(&tree.section, &tree.root.join(&label)).expect("row index");
    list.scroll_to(index, gtk4::ListScrollFlags::FOCUS, None);
    wait_until(Duration::from_secs(5), || {
        (outer.value() - resting).abs() > 0.5
    });
    assert_reveal_then_rest(
        resting,
        overflow,
        &settled_outer_values(&tree.sidebar, 6),
        &label,
    );
    let row = mapped_row_with_label(&list, &label).expect("the revealed row stays rendered");
    assert!(
        inside_outer_viewport(&tree.sidebar, &row),
        "{label} must be fully inside the viewport after the request"
    );
    drop(tree.window);
}

#[test]
fn test_a_sidebar_height_change_after_scrolling_keeps_the_scroll_position() {
    // A window resize changes the sidebar's viewport height. The file tree
    // re-derives its value from its scroll anchor against the new page (ledger
    // A7); forwarded as a request, that scrolled the sidebar 9 px here (4664 px
    // in the GTK Lush adoption lab on maximize). GTK4 cannot shrink a
    // presented window, so the outer scroller's own height is pinned instead.
    let tree = large_tree(&[("nested", 1_000)], 800);
    expand_path(&tree.section, &tree.root, &tree.root.join("nested"));
    let adjustment = outer_adjustment(&tree.sidebar);
    let outer = &*tree.sidebar.imp().outer_scrolled_window;
    for (scroll, height) in [(9_000.0, 400), (20_000.0, 560)] {
        scroll_outer_to(&tree.sidebar, scroll);
        flush_after_delay(Duration::from_millis(400));
        let before = adjustment.value();
        let top = fully_visible_labels(&tree.sidebar, &tree.section)
            .first()
            .cloned();
        pin_scroller_height(outer, height);
        flush_after_delay(Duration::from_millis(600));
        assert!(
            (adjustment.value() - before).abs() < 1.0,
            "a sidebar height change must not scroll it: {before} -> {} (viewport {height} px)",
            adjustment.value()
        );
        assert_eq!(
            fully_visible_labels(&tree.sidebar, &tree.section)
                .first()
                .cloned(),
            top,
            "the row at the top of the sidebar must stay there"
        );
    }
    drop(tree.window);
}

/// Maximizing and unmaximizing the window while the sidebar is scrolled deep
/// into a large tree keeps every sidebar widget allocated when it is drawn.
///
/// Unmaximizing changes the outer viewport's height, so the slice bin re-slices
/// from an idle a frame later, and the window's breakpoint transitions land in
/// that same frame. The re-slice reached the bin through
/// `gtk_widget_ensure_allocate`, because its section and `sections_box` kept
/// their sizes; the new band moved the list's anchor, the list rebound rows,
/// and their resize left those two ancestors (and, after the bin's post-
/// allocation re-announcement, the list itself) marked for allocation until
/// the frame clock's next layout pass. `AdwBreakpointBin` snapshots its child
/// inside its own allocation when its breakpoint changes, which found
/// `sections_box` without a current allocation: a `Gtk-WARNING` the widget
/// runner fails on.
#[test]
fn test_maximize_round_trip_while_scrolled_keeps_the_sidebar_allocated() {
    let tree = large_tree(&[("nested", 1_000)], 800);
    let target = tree.root.join("nested");
    expand_path(&tree.section, &tree.root, &target);
    for value in [9_000.0, 20_000.0] {
        scroll_outer_to(&tree.sidebar, value);
        flush_after_delay(Duration::from_millis(500));
        tree.window.maximize();
        flush_after_delay(Duration::from_millis(1_500));
        tree.window.unmaximize();
        flush_after_delay(Duration::from_millis(1_500));
        let sections_box = tree.sidebar.imp().sections_box.get();
        assert!(
            sections_box.width() > 0 && sections_box.height() > 0,
            "the sections box must hold a real allocation after the round trip"
        );
        assert!(
            !rendered_labels(&tree.section).is_empty(),
            "the scrolled tree must still render rows after the round trip"
        );
    }
    drop(tree.window);
}

// ---------------------------------------------------------------------------
// Workspace path reveal (`win.reveal-workspace-path`).
//
// The reveal must reach its row through the same bounded paths a user takes:
// ancestor expansion through admitted scans, selection, and the list's own
// `scroll_to` forwarded by the slice bin. So every case asserts **rendered**
// rows — selected, mapped, drawn wholly inside the outer viewport, contiguous
// with their neighbours, and still across forced layouts — never model rows.
// ---------------------------------------------------------------------------

/// Activate a reveal and wait for it to reach its terminal status.
#[track_caller]
fn reveal_and_settle(
    window: &LushtextWindow,
    sidebar: &LushtextSidebar,
    target: &Path,
) -> WorkspaceRevealStatus {
    let before = sidebar.workspace_tree_evidence().reveal_generation;
    activate_reveal_workspace_path(window, target);
    let mut status = WorkspaceRevealStatus::None;
    let settled = wait_until_or_false(Duration::from_secs(30), || {
        let evidence = sidebar.workspace_tree_evidence();
        status = evidence.reveal_status;
        evidence.reveal_generation > before && status != WorkspaceRevealStatus::Pending
    });
    assert!(
        settled,
        "the reveal of {} never reached a terminal status; last status {status:?}",
        target.display()
    );
    // Let the forwarded outer scroll and the re-slice it causes land before
    // any rendered-row assertion.
    flush_after_delay(Duration::from_millis(200));
    status
}

/// A workspace folder seeded with `dirs`, loaded collapsed so the reveal has
/// to expand its ancestors itself.
fn collapsed_tree(dirs: &[(&str, usize)], height: i32) -> LargeTree {
    let (dir, root) = seed_workspace(dirs);
    let tree = present_single_folder(dir, root, "reveal", height);
    wait_for_refresh_idle(&tree.section);
    tree
}

/// The rendered-bounds contract after a reveal: the row is selected, mapped,
/// wholly inside the outer viewport, drawn contiguously with its on-screen
/// neighbours, and still across six forced layouts without the bin having to
/// correct the list.
#[track_caller]
fn assert_revealed_row_rendered(
    sidebar: &LushtextSidebar,
    section: &LushtextWorkspaceSection,
    target: &Path,
) {
    let label = target
        .file_name()
        .expect("target has a file name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        selected_tree_path(section).as_deref(),
        Some(target),
        "the revealed row must be the selected one"
    );
    let list = file_tree_view(section);
    let row = mapped_row_with_label(&list, &label)
        .unwrap_or_else(|| panic!("the revealed row {label} must be mapped"));
    assert!(
        inside_outer_viewport(sidebar, &row),
        "the revealed row {label} must be drawn wholly inside the sidebar viewport"
    );
    let slice = &section.imp().file_tree_slice;
    assert!(
        !slice.outer_request_pending(),
        "a revealed row's bin must have no forwarded request left"
    );

    // Neighbours on screen tile at one constant pitch: a row's border box
    // excludes its CSS margin, so consecutive boxes are separated by the same
    // small gap. A row drawn even one pixel off makes two gaps differ.
    let outer = &*sidebar.imp().outer_scrolled_window;
    let viewport = outer.height() as f32;
    let mut drawn: Vec<(f32, f32)> = mapped_list_rows(&list)
        .filter_map(|candidate| candidate.compute_bounds(outer))
        .filter(|bounds| bounds.y() + bounds.height() > 0.0 && bounds.y() < viewport)
        .map(|bounds| (bounds.y(), bounds.height()))
        .collect();
    drawn.sort_by(|a, b| a.0.total_cmp(&b.0));
    let target_top = row.compute_bounds(outer).expect("revealed row bounds").y();
    let position = drawn
        .iter()
        .position(|(top, _)| *top == target_top)
        .expect("the revealed row among the drawn rows");
    let first = position.saturating_sub(4);
    let last = (position + 4).min(drawn.len() - 1);
    let gaps: Vec<f32> = drawn[first..=last]
        .windows(2)
        .map(|pair| pair[1].0 - (pair[0].0 + pair[0].1))
        .collect();
    assert!(
        gaps.iter().all(|gap| *gap >= 0.0 && *gap == gaps[0]),
        "rows around the revealed row must tile at one constant pitch; gaps were {gaps:?} for {:?}",
        &drawn[first..=last]
    );

    let corrections = slice.correction_count();
    let samples = sampled_row_placements(section, &label, 6);
    assert!(
        samples.iter().all(|placement| *placement == samples[0]),
        "the revealed row {label} must stay still across forced layouts: {samples:?}"
    );
    assert_eq!(
        slice.correction_count(),
        corrections,
        "the bin must not correct the list after a reveal settles"
    );
}

/// One collapsed workspace folder directly holding `count` files.
fn collapsed_root_with_files(count: usize, height: i32) -> LargeTree {
    let tree = root_with_files("reveal", count, height);
    wait_for_refresh_idle(&tree.section);
    tree
}

#[test]
fn test_reveal_reaches_the_last_of_four_hundred_files() {
    // `beyond-cap`: past GTK's realized-row cap in a collapsed folder.
    let tree = collapsed_root_with_files(400, 800);
    let target = tree.root.join(file_name(399));

    let status = reveal_and_settle(&tree.window, &tree.sidebar, &target);

    assert_eq!(status, WorkspaceRevealStatus::Revealed);
    assert_revealed_row_rendered(&tree.sidebar, &tree.section, &target);
    assert_eq!(tree.sidebar.workspace_tree_evidence().reveal_depth, Some(1));
    assert!(
        realized_list_rows(&tree.section.imp().file_tree_view).len() < GTK_LIST_VIEW_REALIZED_CAP,
        "realized rows stay bounded by the viewport"
    );
    drop(tree.window);
}

#[test]
fn test_reveal_expands_two_collapsed_ancestors_to_reach_entry_250() {
    // `nested-beyond-cap`: two collapsed directories, then entry 250 of 300.
    let tree = collapsed_tree(&[("outer/inner", 300), ("sibling", 3)], 800);
    let target = tree.root.join("outer/inner").join(file_name(249));

    let status = reveal_and_settle(&tree.window, &tree.sidebar, &target);

    assert_eq!(status, WorkspaceRevealStatus::Revealed);
    assert_revealed_row_rendered(&tree.sidebar, &tree.section, &target);
    assert_eq!(tree.sidebar.workspace_tree_evidence().reveal_depth, Some(3));
    let expanded = tree.section.workspace_section_evidence().expanded_paths;
    assert_eq!(
        expanded,
        [
            tree.root.clone(),
            tree.root.join("outer"),
            tree.root.join("outer/inner")
        ]
        .into_iter()
        .collect(),
        "only the ancestor chain is expanded"
    );
    drop(tree.window);
}

#[test]
fn test_reveal_reaches_a_row_in_the_second_of_two_tall_workspaces() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("two tall workspaces tempdir");
    let mut configs = Vec::new();
    for name in ["left", "right"] {
        let root = dir.path().join(name);
        seed_files(&root, 300);
        configs.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new(name),
            name,
            root,
        ));
    }
    save_workspaces(configs);
    let (window, sidebar) = present_sidebar_window(800);
    let sections = mapped_sections(&sidebar, 2);
    // The first section is expanded so the chrome above the second bin
    // includes a whole tall section.
    sections[0].expand_folders();
    wait_for_refresh_idle(&sections[0]);
    wait_for_refresh_idle(&sections[1]);
    let target = dir.path().join("right").join(file_name(260));

    let status = reveal_and_settle(&window, &sidebar, &target);

    assert_eq!(status, WorkspaceRevealStatus::Revealed);
    assert_revealed_row_rendered(&sidebar, &sections[1], &target);
    drop(window);
}

#[test]
fn test_reveal_of_a_visible_row_moves_neither_the_sidebar_nor_the_header() {
    // `already-visible`: the resting-bin claim.
    let tree = tall_workspace(800);
    scroll_outer_to(&tree.sidebar, 0.0);
    assert_sidebar_can_scroll(&tree.sidebar, "before revealing a visible row");
    let label = fully_visible_labels(&tree.sidebar, &tree.section)
        .into_iter()
        .nth(4)
        .expect("a fifth fully visible row");
    let outer = outer_adjustment(&tree.sidebar);
    let outer_before = outer.value();
    let header = header_of(&tree.section);
    let header_before = header
        .compute_bounds(&*tree.sidebar.imp().outer_scrolled_window)
        .expect("header bounds");
    let target = tree.root.join(&label);

    let status = reveal_and_settle(&tree.window, &tree.sidebar, &target);

    assert_eq!(status, WorkspaceRevealStatus::Revealed);
    assert_revealed_row_rendered(&tree.sidebar, &tree.section, &target);
    assert!(
        settled_outer_values(&tree.sidebar, 6)
            .iter()
            .all(|value| *value == outer_before),
        "revealing a visible row must not scroll the sidebar from {outer_before}"
    );
    let header_after = header
        .compute_bounds(&*tree.sidebar.imp().outer_scrolled_window)
        .expect("header bounds");
    assert_eq!(
        (header_after.y(), header_after.height()),
        (header_before.y(), header_before.height()),
        "the workspace header must be drawn where it was"
    );
    drop(tree.window);
}

#[test]
fn test_reveal_of_a_visible_row_in_a_short_sidebar_does_not_scroll() {
    // The at-rest scenario's short-wide geometry: a sidebar viewport of about
    // 178 px, where the header and a few rows fill it. A setup reveal first
    // (as the scenario does), then a reveal of the folder row, which lies
    // wholly inside the bin's band, margins included.
    let tree = collapsed_root_with_files(400, 800);
    pin_scroller_height(&tree.sidebar.imp().outer_scrolled_window, 178);
    flush_after_delay(Duration::from_millis(400));
    let setup = tree.root.join(file_name(0));
    assert_eq!(
        reveal_and_settle(&tree.window, &tree.sidebar, &setup),
        WorkspaceRevealStatus::Revealed
    );
    assert_sidebar_can_scroll(&tree.sidebar, "before revealing a visible row");
    let outer_before = outer_adjustment(&tree.sidebar).value();
    let header = header_of(&tree.section);
    let header_before = header
        .compute_bounds(&*tree.sidebar.imp().outer_scrolled_window)
        .expect("header bounds");

    let status = reveal_and_settle(&tree.window, &tree.sidebar, &tree.root);

    assert_eq!(status, WorkspaceRevealStatus::Revealed);
    assert_eq!(tree.sidebar.workspace_tree_evidence().reveal_depth, Some(0));
    assert!(
        settled_outer_values(&tree.sidebar, 6)
            .iter()
            .all(|value| *value == outer_before),
        "revealing a visible row must not scroll a short sidebar from {outer_before}"
    );
    let header_after = header
        .compute_bounds(&*tree.sidebar.imp().outer_scrolled_window)
        .expect("header bounds");
    assert_eq!(header_after.y(), header_before.y(), "the header stays put");
    drop(tree.window);
}

/// Pins the documented price of the slice bin's forwarding rule
/// (`gtk_lush_widgets::outer_scroll_request`): a row at the band's bottom edge
/// whose CSS margin GTK counts as clipped asks for one pixel, and honouring it
/// scrolls the chrome above the bin away, so a reveal of such a row is a
/// scrolling reveal, never an at-rest one. If this ever stops scrolling, the
/// forwarding rule changed and the at-rest fixture choice should be revisited.
#[test]
fn test_reveal_of_a_row_clipped_by_its_margin_scrolls_the_chrome_away() {
    let tree = collapsed_root_with_files(400, 800);
    pin_scroller_height(&tree.sidebar.imp().outer_scrolled_window, 178);
    flush_after_delay(Duration::from_millis(400));
    assert_eq!(
        reveal_and_settle(&tree.window, &tree.sidebar, &tree.root.join(file_name(0))),
        WorkspaceRevealStatus::Revealed
    );
    let outer_before = outer_adjustment(&tree.sidebar).value();
    let target = tree.root.join(file_name(1));

    let status = reveal_and_settle(&tree.window, &tree.sidebar, &target);

    assert_eq!(status, WorkspaceRevealStatus::Revealed);
    assert!(
        outer_adjustment(&tree.sidebar).value() > outer_before,
        "the margin-clipped row's one-pixel request scrolls the outer"
    );
    assert_revealed_row_rendered(&tree.sidebar, &tree.section, &target);
    drop(tree.window);
}

/// Everything a failed reveal must leave exactly as it found it.
#[derive(Debug, PartialEq)]
struct SidebarRestState {
    selected: Vec<u32>,
    outer: f64,
    scope: lushtext_core::model::workspace::WorkspaceScope,
    show_hidden: bool,
    expanded: Vec<std::collections::HashSet<PathBuf>>,
}

fn rest_state(sidebar: &LushtextSidebar) -> SidebarRestState {
    let sections = sidebar.imp().sections.borrow().clone();
    let settings = lushtext_core::ui::settings_writes::shared();
    SidebarRestState {
        selected: sections
            .iter()
            .map(|section| selection(section).selected())
            .collect(),
        outer: outer_adjustment(sidebar).value(),
        scope: sidebar.current_scope(),
        show_hidden: settings.boolean(lushtext_core::config::keys::WORKSPACE_SHOW_HIDDEN_FILES),
        expanded: sections
            .iter()
            .map(|section| section.workspace_section_evidence().expanded_paths)
            .collect(),
    }
}

#[track_caller]
fn assert_reveal_fails_without_side_effects(
    window: &LushtextWindow,
    sidebar: &LushtextSidebar,
    target: &Path,
    expected: WorkspaceRevealStatus,
) {
    let before = rest_state(sidebar);
    let scans_before = sidebar
        .workspace_tree_evidence()
        .process_scan_task_high_water;
    let status = reveal_and_settle(window, sidebar, target);
    assert_eq!(status, expected, "reveal of {}", target.display());
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(
        rest_state(sidebar),
        before,
        "a failed reveal must leave selection, scroll, scope, visibility, and expansion unchanged"
    );
    if matches!(
        expected,
        WorkspaceRevealStatus::NotInWorkspace
            | WorkspaceRevealStatus::OutsideScope
            | WorkspaceRevealStatus::HiddenByVisibility
    ) {
        assert_eq!(
            sidebar
                .workspace_tree_evidence()
                .process_scan_task_high_water,
            scans_before,
            "a reveal refused before expansion starts no scan"
        );
    }
}

#[test]
fn test_reveal_outside_every_workspace_fails_as_not_in_workspace() {
    let tree = collapsed_tree(&[("nested", 3)], 800);
    let elsewhere = tempfile::tempdir().expect("outside tempdir");
    fixture::write_text(&elsewhere.path().join("loose.txt"), "x");
    assert_reveal_fails_without_side_effects(
        &tree.window,
        &tree.sidebar,
        &elsewhere.path().join("loose.txt"),
        WorkspaceRevealStatus::NotInWorkspace,
    );
    drop(tree.window);
}

#[test]
fn test_reveal_in_a_workspace_hidden_by_scope_fails_as_outside_scope() {
    let (_dir, _first, second) = two_workspaces(5);
    let (window, sidebar) = present_sidebar_window(800);
    let sections = mapped_sections(&sidebar, 2);
    // Narrow the scope to the first workspace.
    sidebar.imp().workspace_filter_dropdown.set_selected(1);
    flush_after_delay(Duration::from_millis(400));
    for section in &sections {
        wait_for_refresh_idle(section);
    }
    assert_reveal_fails_without_side_effects(
        &window,
        &sidebar,
        &second.join("nested").join(file_name(2)),
        WorkspaceRevealStatus::OutsideScope,
    );
    sidebar.imp().workspace_filter_dropdown.set_selected(0);
    flush_after_delay(Duration::from_millis(400));
    drop(window);
}

#[test]
fn test_reveal_of_a_missing_middle_component_fails_as_not_found() {
    let tree = collapsed_tree(&[("present", 3)], 800);
    assert_reveal_fails_without_side_effects(
        &tree.window,
        &tree.sidebar,
        &tree.root.join("present/missing/file.txt"),
        WorkspaceRevealStatus::NotFound,
    );
    drop(tree.window);
}

#[test]
fn test_reveal_through_a_hidden_component_fails_without_changing_visibility() {
    let tree = collapsed_tree(&[(".secret", 3)], 800);
    let settings = lushtext_core::ui::settings_writes::shared();
    settings.reset(lushtext_core::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
    flush_events();
    assert_reveal_fails_without_side_effects(
        &tree.window,
        &tree.sidebar,
        &tree.root.join(".secret").join(file_name(1)),
        WorkspaceRevealStatus::HiddenByVisibility,
    );
    drop(tree.window);
}

#[test]
fn test_reveal_past_the_directory_entry_cap_fails_as_beyond_entry_cap() {
    // A directory has no entry-count cap (`bound-lists-by-budget-not-count`);
    // its retained-byte budget, lowered here, is what can cut it, and the
    // target sorts after every kept row.
    with_directory_budget(256 * 1024, || {
        let tree = collapsed_tree(&[("huge", 2_000)], 800);
        assert_reveal_fails_without_side_effects(
            &tree.window,
            &tree.sidebar,
            &tree.root.join("huge").join(file_name(1_990)),
            WorkspaceRevealStatus::BeyondEntryCap,
        );
        drop(tree.window);
    });
}

#[test]
fn test_a_newer_reveal_supersedes_one_awaiting_a_scan() {
    let tree = collapsed_tree(&[("slow", 20), ("fast", 20)], 800);
    tree.section
        .set_child_scan_delay_for_test(Duration::from_millis(800));
    let first = tree.root.join("slow").join(file_name(15));
    let second = tree.root.join("top.txt");
    let superseded_before = tree
        .sidebar
        .workspace_tree_evidence()
        .reveal_superseded_count;

    activate_reveal_workspace_path(&tree.window, &first);
    flush_after_delay(Duration::from_millis(50));
    let status = reveal_and_settle(&tree.window, &tree.sidebar, &second);

    assert_eq!(status, WorkspaceRevealStatus::Revealed);
    assert_eq!(
        tree.sidebar
            .workspace_tree_evidence()
            .reveal_superseded_count,
        superseded_before + 1,
        "the first reveal must end superseded"
    );
    // Outlive the first reveal's delayed scan: its result must select nothing.
    flush_after_delay(Duration::from_millis(1_200));
    wait_for_refresh_idle(&tree.section);
    assert_eq!(
        selected_tree_path(&tree.section).as_deref(),
        Some(second.as_path())
    );
    assert_eq!(
        tree.sidebar.workspace_tree_evidence().reveal_status,
        WorkspaceRevealStatus::Revealed
    );
    tree.section.set_child_scan_delay_for_test(Duration::ZERO);
    drop(tree.window);
}

#[test]
fn test_disposing_the_window_mid_reveal_clears_the_blocker() {
    let tree = collapsed_tree(&[("slow", 20)], 800);
    tree.section
        .set_child_scan_delay_for_test(Duration::from_millis(800));
    let target = tree.root.join("slow").join(file_name(10));
    activate_reveal_workspace_path(&tree.window, &target);
    flush_after_delay(Duration::from_millis(50));
    assert!(
        tree.sidebar.workspace_path_reveal_pending(),
        "the reveal must still be waiting on its scan"
    );
    let generation = tree.sidebar.workspace_tree_evidence().reveal_generation;

    // Destroy the window and drop every handle but the sidebar's, so the window
    // is really disposed and tears its widget tree down around the reveal.
    let LargeTree {
        _dir,
        window,
        sidebar,
        section,
        ..
    } = tree;
    drop(section);
    window.destroy();
    drop(window);
    flush_after_delay(Duration::from_millis(100));

    assert!(
        !sidebar.workspace_path_reveal_pending(),
        "disposal must end the reveal and clear its blocker"
    );
    assert_eq!(
        sidebar.workspace_tree_evidence().reveal_status,
        WorkspaceRevealStatus::Superseded
    );
    flush_after_delay(Duration::from_millis(1_200));
    let evidence = sidebar.workspace_tree_evidence();
    assert_eq!(
        evidence.reveal_generation, generation,
        "no continuation ran"
    );
    assert_eq!(evidence.reveal_status, WorkspaceRevealStatus::Superseded);
    drop(_dir);
}

#[test]
fn test_sidebar_slice_bin_reports_a_forwarded_request_until_its_idle_applies_it() {
    let tree = tall_workspace(800);
    scroll_outer_to(&tree.sidebar, 0.0);
    let slice = &tree.section.imp().file_tree_slice;
    for _ in 0..6 {
        force_section_layout(&tree.section);
        assert!(
            !slice.outer_request_pending(),
            "a resting sidebar bin must report no pending request"
        );
    }
    let outer = outer_adjustment(&tree.sidebar);
    let resting = outer.value();
    let index = tree_index_for_path(&tree.section, &tree.root.join(file_name(399)))
        .expect("last row index");

    file_tree_view(&tree.section).scroll_to(index, gtk4::ListScrollFlags::FOCUS, None);
    let mut outer_when_pending = None;
    let seen = observed_between_dispatches(Duration::from_secs(5), || {
        let pending = slice.outer_request_pending();
        if pending {
            outer_when_pending = Some(outer.value());
        }
        pending
    });
    assert!(
        seen,
        "the sidebar bin must report its forwarded request as pending"
    );
    assert_eq!(outer_when_pending, Some(resting));

    wait_until(Duration::from_secs(5), || {
        !slice.outer_request_pending() && outer.value() > resting
    });
    flush_after_delay(Duration::from_millis(300));
    assert!(!slice.outer_request_pending());
    drop(tree.window);
}

#[test]
fn test_reveal_geometry_snapshot_is_bounded_and_unlabelled() {
    let tree = collapsed_root_with_files(400, 800);
    let app = tree
        .window
        .application()
        .and_downcast::<lushtext_core::app::LushtextApplication>()
        .expect("test window belongs to a LushtextApplication");
    let surface = |snapshot: &lushtext_core::model::automation::AutomationSnapshot, name: &str| {
        snapshot
            .window
            .as_ref()
            .expect("window snapshot")
            .visual_geometry
            .surfaces
            .iter()
            .find(|surface| surface.name == name)
            .cloned()
    };

    // Before any reveal the named surfaces are present, absent, and explained.
    let before = lushtext_core::ui::automation::app_snapshot(&app);
    for name in [
        "workspace-sidebar-viewport",
        "workspace-revealed-row",
        "workspace-revealed-section-header",
    ] {
        let absent = surface(&before, name).unwrap_or_else(|| panic!("{name} reported"));
        assert!(!absent.visible);
        assert_eq!(absent.absence_reason.as_deref(), Some("no-reveal"));
    }
    assert_eq!(
        before
            .window
            .as_ref()
            .expect("window")
            .workspace
            .reveal_status,
        "none"
    );

    let target = tree.root.join(file_name(399));
    assert_eq!(
        reveal_and_settle(&tree.window, &tree.sidebar, &target),
        WorkspaceRevealStatus::Revealed
    );
    let after = lushtext_core::ui::automation::app_snapshot(&app);
    let window_snapshot = after.window.as_ref().expect("window snapshot");
    assert_eq!(window_snapshot.workspace.reveal_status, "revealed");
    assert_eq!(window_snapshot.workspace.reveal_depth, Some(1));
    let viewport = surface(&after, "workspace-sidebar-viewport")
        .and_then(|surface| surface.rect)
        .expect("viewport rect");
    let row = surface(&after, "workspace-revealed-row")
        .and_then(|surface| surface.rect)
        .expect("revealed row rect");
    assert!(surface(&after, "workspace-revealed-section-header").is_some_and(|s| s.visible));
    assert!(
        row.y >= viewport.y && row.y + row.height <= viewport.y + viewport.height,
        "the revealed row {row:?} lies inside the sidebar viewport {viewport:?}"
    );
    let neighbours = window_snapshot
        .visual_geometry
        .surfaces
        .iter()
        .filter(|surface| surface.name.starts_with("workspace-revealed-neighbour-"))
        .count();
    assert!(
        (2..=8).contains(&neighbours),
        "between two and eight neighbour rows are reported, not {neighbours}"
    );
    let anchor = window_snapshot
        .visual_geometry
        .pixel_anchors
        .iter()
        .find(|anchor| anchor.name == "workspace-revealed-row-ink")
        .expect("selected-row pixel anchor");
    assert_eq!(anchor.rect, Some(row));
    assert!(
        window_snapshot
            .visual_geometry
            .scroll_anchors
            .iter()
            .any(|anchor| anchor.name == "workspace-sidebar-viewport"
                && anchor.y_value_milli.is_some()),
        "the outer sidebar scroller is reported as a scroll anchor"
    );
    let json = serde_json::to_string(&after).expect("snapshot serializes");
    assert!(
        !json.contains("row-00"),
        "no file name may cross the automation privacy boundary"
    );
    drop(tree.window);
}

/// `bound-lists-by-budget-not-count` tasks 4.1 and 8.1–8.2: expanding
/// directories of 10,000, 16,385 and 131,072 entries, presented: every
/// dispatch from the expansion to the directory's terminal fits `B_SETTLE_US`
/// on the optimized widget build (printed on every build), and the terminal
/// does not grow with the directory: at 10,000 and 131,072 entries it differs
/// by less than one 120 Hz reference frame (8.3 ms) on every build.
#[test]
fn test_directory_expansion_dispatches_fit_the_settle_budget() {
    use crate::common::{timing_verdicts_enforced, worst_dispatch_until};
    use lushtext_core::model::responsiveness_budget::B_SETTLE_US;
    let mut terminals = Vec::new();
    for count in [10_000usize, 16 * 1024 + 1, 128 * 1024] {
        let tree = large_tree(&[("nested", count)], 800);
        let nested = tree.root.join("nested");
        wait_until(Duration::from_secs(10), || {
            row_for_path(&tree.section, &nested).is_some()
        });
        wait_for_refresh_idle(&tree.section);
        row_for_path(&tree.section, &nested)
            .expect("directory row")
            .set_expanded(true);
        let (held, worst) = worst_dispatch_until(Duration::from_secs(240), || {
            listing(&tree.section, &nested).directory_retained_entries == count
                && !tree
                    .section
                    .workspace_section_evidence()
                    .refresh_blocks_readiness
        });
        assert!(held, "the expansion of {count} entries settles");
        let evidence = tree.section.workspace_section_evidence();
        terminals.push(evidence.reconcile_last_terminal_us);
        println!(
            "list-budget-evidence {{\"m\":\"sidebar\",\"entries\":{count},\"worst_dispatch_us\":{},\"terminal_us\":{},\"batches_per_turn\":{},\"realized_rows\":{},\"owner_resets\":{}}}",
            worst.as_micros(),
            evidence.reconcile_last_terminal_us,
            evidence.reconcile_max_batches_per_turn,
            rendered_rows(&tree.section).len(),
            evidence.cache_owner_resets,
        );
        if timing_verdicts_enforced() {
            assert!(
                worst.as_micros() <= u128::from(B_SETTLE_US),
                "a dispatch of the {count}-entry expansion took {} us, over B_SETTLE_US",
                worst.as_micros()
            );
        }
        // Collapse and expand again: the new store takes the directory's
        // path caches over from the old one, forgetting its rows in slices.
        // The collapse is GTK's synchronous `set_expanded(false)`, called
        // directly: the route the residual collapses (a refresh splice, an
        // AT-SPI `DoAction`) still take, printed, not enforced. LushText's
        // own routes pace the collapse, and
        // `test_directory_collapse_and_small_expansion_dispatches_fit_the_budgets`
        // measures their dispatches (`slice-large-directory-collapse`, which
        // re-attributed BL's toolkit-floor exception).
        let collapsing = row_for_path(&tree.section, &nested).expect("directory row");
        let started = std::time::Instant::now();
        collapsing.set_expanded(false);
        println!(
            "list-budget-evidence {{\"m\":\"sidebar-collapse\",\"entries\":{count},\"collapse_call_us\":{}}}",
            started.elapsed().as_micros()
        );
        wait_for_refresh_idle(&tree.section);
        row_for_path(&tree.section, &nested)
            .expect("directory row")
            .set_expanded(true);
        let (held, again) = worst_dispatch_until(Duration::from_secs(240), || {
            listing(&tree.section, &nested).directory_retained_entries == count
                && !tree
                    .section
                    .workspace_section_evidence()
                    .refresh_blocks_readiness
        });
        assert!(held, "the re-expansion of {count} entries settles");
        assert_eq!(
            tree.section
                .workspace_section_evidence()
                .cache_full_rebuilds,
            0
        );
        println!(
            "list-budget-evidence {{\"m\":\"sidebar-reexpand\",\"entries\":{count},\"worst_dispatch_us\":{}}}",
            again.as_micros()
        );
        // Printed, not enforced: GTK's synchronous collapse of the old
        // listing (19 ms at 16,000 rows, before this measurement starts) can
        // leave work that lands in the re-expansion's first dispatches (one
        // 21 ms outlier in 6 runs at 16,385); recorded in the design as an
        // open follow-up.
        drop(tree.window);
    }
    assert!(
        terminals[2].abs_diff(terminals[0]) < 8_333,
        "terminal durations {terminals:?} µs grow with the directory"
    );
}

/// `bound-lists-by-budget-not-count` tasks 2.4 and 8.3: what 32,768 expanded
/// rows really cost, presented, against what the directory budget charges
/// them: resident high-water growth at most 5/4 of the charge. Prints the
/// per-row growth.
///
/// Measured only once the resident set is quiescent (`restore-main-ci` D4).
/// The window is presented and the folder scanned just before, and their
/// asynchronous tail kept allocating for about a second after the refresh
/// went idle; with the former 300 ms settle part of it landed in the measured
/// window, more of it the slower the machine. That made the growth bimodal on
/// CI, 2,362-2,375 or 2,534-2,537 B/row against a 2,430 B/row bound at 16,384
/// rows, where the workstation measured 1,970-1,985. In a fixture with two
/// more large sibling directories the overlap reproduced locally: 2,328-2,561
/// B/row after 300 ms, 1,322-1,329 after a 5 s settle. Fitted on the
/// workstation in separate processes without the wait, the growth is 1,644
/// B/row plus about 5.3 MB, against a charge of 1,960 B/row. The test also
/// measures 32,768 rows rather than 16,384, so any constant left weighs half
/// as much per row, while a per-row undercharge still fails. With both, 20
/// workstation runs measured 1,369-1,552 B/row; the expansion can reuse
/// memory the setup tail freed, so the figure checks that the charge is not
/// exceeded; it does not calibrate the charge.
#[test]
fn test_directory_retention_stays_within_its_charge() {
    use crate::common::{proc_status_kib, reset_vm_hwm};
    use lushtext_core::ui::sidebar::policy::{
        SIDEBAR_PATH_GRAPH_OVERHEAD, SIDEBAR_ROW_FIXED_CHARGE, SIDEBAR_ROW_PATH_COPIES,
    };
    const ROWS: usize = 32 * 1024;
    let tree = large_tree(&[("nested", ROWS)], 800);
    let nested = tree.root.join("nested");
    wait_until(Duration::from_secs(10), || {
        row_for_path(&tree.section, &nested).is_some()
    });
    wait_for_refresh_idle(&tree.section);
    crate::common::wait_for_resident_quiescence(Duration::from_secs(30));
    reset_vm_hwm();
    let before_kib = proc_status_kib("VmRSS:");
    row_for_path(&tree.section, &nested)
        .expect("directory row")
        .set_expanded(true);
    wait_until(Duration::from_secs(120), || {
        listing(&tree.section, &nested).directory_retained_entries == ROWS
    });
    wait_for_refresh_idle(&tree.section);
    flush_after_delay(Duration::from_millis(300));
    let growth = proc_status_kib("VmHWM:").saturating_sub(before_kib) * 1024;
    let path_len = nested.join(file_name(0)).as_os_str().len() as u64;
    let charged = ROWS as u64
        * ((path_len + SIDEBAR_PATH_GRAPH_OVERHEAD) * SIDEBAR_ROW_PATH_COPIES
            + SIDEBAR_ROW_FIXED_CHARGE);
    println!(
        "list-budget-evidence {{\"m\":\"sidebar-memory\",\"rows\":{ROWS},\"path_len\":{path_len},\"growth_bytes\":{growth},\"charged_bytes\":{charged},\"growth_per_row\":{}}}",
        growth / ROWS as u64
    );
    assert!(
        growth * 4 <= charged * 5,
        "{ROWS} rows grew the resident set by {growth} bytes, over 5/4 of the {charged} bytes charged"
    );
    drop(tree.window);
}

/// Expand `directory` and wait until its listing holds `count` entries.
fn expand_and_load(tree: &LargeTree, directory: &Path, count: usize) {
    wait_until(Duration::from_secs(10), || {
        row_for_path(&tree.section, directory).is_some()
    });
    wait_for_refresh_idle(&tree.section);
    row_for_path(&tree.section, directory)
        .expect("directory row")
        .set_expanded(true);
    wait_until(Duration::from_secs(120), || {
        listing(&tree.section, directory).directory_retained_entries == count
            && !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
}

/// A weak reference to the child store the expanded `directory` row shows.
fn shown_store(tree: &LargeTree, directory: &Path) -> glib::WeakRef<gtk4::gio::ListStore> {
    row_for_path(&tree.section, directory)
        .expect("directory row")
        .children()
        .and_downcast::<gtk4::gio::ListStore>()
        .expect("an expanded directory shows a child store")
        .downgrade()
}

/// Stage 4 review of `bound-lists-by-budget-not-count` (ledger A91): GTK's
/// collapse of a directory row also disposed the directory's child store
/// inside `set_expanded(false)`, finalizing every listed item in that one
/// dispatch (a quarter of a 131,072-entry collapse). The section holds the
/// store an expanded directory shows, so the collapse only unlinks it, and
/// retires the listing in time-sliced turns afterwards, releasing it.
///
/// It calls `TreeListRow::set_expanded(false)` directly, so it tests GTK's
/// direct collapse, which the residual routes (a refresh splice, an AT-SPI
/// `DoAction`) still take; LushText's own routes pace the collapse
/// (`slice-large-directory-collapse`).
#[test]
fn test_collapsing_a_large_directory_retires_its_listing_after_the_collapse() {
    let count = 16 * 1024 + 1;
    let tree = large_tree(&[("nested", count)], 800);
    let nested = tree.root.join("nested");
    expand_and_load(&tree, &nested, count);
    let store = shown_store(&tree, &nested);
    row_for_path(&tree.section, &nested)
        .expect("directory row")
        .set_expanded(false);
    assert!(
        store
            .upgrade()
            .is_some_and(|store| store.n_items() as usize > count / 2),
        "the collapse itself must not free the directory's listing"
    );
    wait_until(Duration::from_secs(30), || store.upgrade().is_none());
    assert!(
        store.upgrade().is_none(),
        "the retired listing is released once emptied"
    );
    drop(tree.window);
}

/// Collapsing an ancestor unlinks its expanded descendants' listings too:
/// the section retires and releases them, so holding a shown store never
/// keeps a collapsed subtree alive.
///
/// It calls `TreeListRow::set_expanded(false)` directly, so it tests GTK's
/// direct collapse, which the residual routes (a refresh splice, an AT-SPI
/// `DoAction`) still take; LushText's own routes pace the collapse
/// (`slice-large-directory-collapse`).
#[test]
fn test_collapsing_an_ancestor_releases_its_descendants_listings() {
    let tree = large_tree(&[("outer/inner", 300)], 800);
    let outer = tree.root.join("outer");
    let inner = outer.join("inner");
    expand_and_load(&tree, &outer, 1);
    expand_and_load(&tree, &inner, 300);
    let outer_store = shown_store(&tree, &outer);
    let inner_store = shown_store(&tree, &inner);
    row_for_path(&tree.section, &outer)
        .expect("outer row")
        .set_expanded(false);
    wait_until(Duration::from_secs(30), || {
        outer_store.upgrade().is_none() && inner_store.upgrade().is_none()
    });
    assert!(
        outer_store.upgrade().is_none(),
        "the collapsed listing is released"
    );
    assert!(
        inner_store.upgrade().is_none(),
        "the descendant's listing is released with it"
    );
    drop(tree.window);
}

/// A listing someone else still holds when its row collapses is only let
/// go, never emptied: retirement drains a store only the section holds.
///
/// It calls `TreeListRow::set_expanded(false)` directly, so it tests GTK's
/// direct collapse, which the residual routes (a refresh splice, an AT-SPI
/// `DoAction`) still take; LushText's own routes pace the collapse
/// (`slice-large-directory-collapse`).
#[test]
fn test_a_collapsed_listing_held_elsewhere_is_released_not_emptied() {
    let tree = large_tree(&[("nested", 300)], 800);
    let nested = tree.root.join("nested");
    expand_and_load(&tree, &nested, 300);
    let held = shown_store(&tree, &nested)
        .upgrade()
        .expect("the shown store is alive");
    row_for_path(&tree.section, &nested)
        .expect("directory row")
        .set_expanded(false);
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(held.n_items(), 300, "a shared listing keeps every item");
    drop(tree.window);
}

// ---------------------------------------------------------------------------
// The owned, sliced directory collapse (`slice-large-directory-collapse`).
// ---------------------------------------------------------------------------

/// The most row widgets one collapse step may make GTK create (design D3).
const COLLAPSE_UNIT: usize =
    lushtext_core::ui::sidebar::policy::SIDEBAR_REALIZED_ROWS_PER_FRAME as usize;

/// CSS class `collapse_execution` puts on a collapsing row's expander.
const COLLAPSING_CLASS: &str = "lushtext-collapsing";

/// Activate the row at `path` as Enter or a double click does.
fn activate_tree_row(section: &LushtextWorkspaceSection, path: &Path) {
    let position = tree_index_for_path(section, path).expect("the row is listed");
    section
        .imp()
        .file_tree_view
        .emit_by_name::<()>("activate", &[&position]);
}

/// The realized expander showing `path`'s row, if any.
fn expander_for(section: &LushtextWorkspaceSection, path: &Path) -> Option<gtk4::TreeExpander> {
    let row = row_for_path(section, path)?;
    let mut child = section.imp().file_tree_view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        let expander = find_descendant(&widget, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
            .and_downcast::<gtk4::TreeExpander>();
        if let Some(expander) = expander
            && expander.list_row().as_ref() == Some(&row)
        {
            return Some(expander);
        }
    }
    None
}

/// Whether `path`'s row is listed, GTK-collapsed, and the section settled.
fn collapse_settled(section: &LushtextWorkspaceSection, path: &Path) -> bool {
    row_for_path(section, path).is_some_and(|row| !row.is_expanded())
        && !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
}

/// Each top-level row's path and GTK expanded state, and the list's length.
fn top_level_rows(section: &LushtextWorkspaceSection) -> (u32, Vec<(Option<PathBuf>, bool)>) {
    let Some(model) = section.imp().tree_model.borrow().as_ref().cloned() else {
        return (0, Vec::new());
    };
    let rows = (0..model.n_items())
        .filter_map(|index| model.item(index).and_downcast::<gtk4::TreeListRow>())
        .filter(|row| row.depth() == 0)
        .map(|row| {
            (
                row.item()
                    .and_downcast::<FileTreeItem>()
                    .and_then(|item| item.path()),
                row.is_expanded(),
            )
        })
        .collect();
    (model.n_items(), rows)
}

/// The collapse evidence, for a failure message.
fn collapse_state(section: &LushtextWorkspaceSection) -> String {
    let evidence = section.workspace_section_evidence();
    let roots = top_level_rows(section);
    format!(
        "roots={roots:?} collapses={:?} last_end={:?} steps={} direct={} starved={} blocks_readiness={} pressure={:?}",
        evidence.collapses,
        evidence.last_collapse_end,
        evidence.collapse_steps_total,
        evidence.collapse_direct_total,
        evidence.collapse_starved_steps_total,
        evidence.refresh_blocks_readiness,
        evidence.scan_pressure,
    )
}

/// The tree row each realized row content widget shows, keyed by the
/// widget's address.
type BoundRows = std::collections::HashMap<usize, usize>;

/// Record which tree row every realized row content widget (the overlay the
/// factory's `setup` creates) shows now; return how many widgets are new or
/// show another row than at the last call (`shown`), and how many were
/// destroyed (`destroyed`). GTK 4.24 may reuse a removed
/// row's list item for an item entering the band, so a replaced row costs a
/// rebind as well as, or instead of, a new widget (A118); both count.
fn mark_new_row_widgets(section: &LushtextWorkspaceSection, seen: &mut BoundRows) -> RowChange {
    use glib::translate::ToGlibPtr;
    let mut changed = 0;
    let mut now = BoundRows::new();
    let mut child = section.imp().file_tree_view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        let Some(content) = widget.first_child() else {
            continue;
        };
        let row = find_descendant(&content, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
            .and_downcast::<gtk4::TreeExpander>()
            .and_then(|expander| expander.list_row());
        let key: *mut glib::gobject_ffi::GObject =
            content.upcast_ref::<glib::Object>().to_glib_none().0;
        let value = row.as_ref().map_or(0, |row| {
            let pointer: *mut glib::gobject_ffi::GObject =
                row.upcast_ref::<glib::Object>().to_glib_none().0;
            pointer as usize
        });
        let key = key as usize;
        if seen.get(&key) != Some(&value) {
            changed += 1;
        }
        now.insert(key, value);
    }
    let destroyed = seen.keys().filter(|key| !now.contains_key(key)).count();
    *seen = now;
    RowChange {
        shown: changed,
        destroyed,
    }
}

/// Row widgets one dispatch changed: shown anew or for another row, and
/// destroyed.
#[derive(Clone, Copy, Debug, Default)]
struct RowChange {
    shown: usize,
    destroyed: usize,
}

impl RowChange {
    /// The first dispatch over a collapse step's bounds (design D2, D18):
    /// at most one unit of row widgets shown, and at most the destroy-only
    /// unit destroyed (a removal that brings no row in may take more).
    fn over_unit(changes: &[Self]) -> Option<Self> {
        changes
            .iter()
            .copied()
            .find(|change| change.shown > COLLAPSE_UNIT || change.destroyed > COLLAPSE_DESTROY_ONLY)
    }
}

/// The most row widgets one destroy-only collapse step may destroy (design
/// D18).
const COLLAPSE_DESTROY_ONLY: usize =
    lushtext_core::ui::sidebar::policy::COLLAPSE_DESTROY_ONLY_ROWS as usize;

/// Run `start` (the input dispatch), then the main loop one dispatch at a
/// time until `done` holds (or 120 s pass), returning the row widgets GTK
/// showed anew or destroyed in the input dispatch and in each later one.
fn row_widgets_changed_per_dispatch(
    section: &LushtextWorkspaceSection,
    start: impl FnOnce(),
    mut done: impl FnMut() -> bool,
) -> Vec<RowChange> {
    let context = glib::MainContext::default();
    let started = std::time::Instant::now();
    let mut created = Vec::new();
    let mut seen = BoundRows::new();
    mark_new_row_widgets(section, &mut seen);
    start();
    created.push(mark_new_row_widgets(section, &mut seen));
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "the collapse settles within two minutes: {}",
            collapse_state(section)
        );
        if context.iteration(false) {
            created.push(mark_new_row_widgets(section, &mut seen));
        } else {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
    created
}

/// A presented tree whose `nested/` (with `count` entries, `siblings` files
/// after it) is expanded and fully listed.
/// The most files after `nested/` for which [`expanded_nested`]'s collapse
/// is still paced: design R13's C3 sends a collapse that brings more than one
/// unit (`SIDEBAR_REALIZED_ROWS_PER_FRAME`) of rows into the band from after
/// its subtree through `main`'s direct collapse ("C3 as built"), and
/// `seed_workspace`'s `top.txt` is one of the rows after it. The paced
/// mechanism's tests with rows after the subtree use this many.
const PACED_SIBLINGS: usize =
    lushtext_core::ui::sidebar::policy::SIDEBAR_REALIZED_ROWS_PER_FRAME as usize - 1;

fn expanded_nested(count: usize, siblings: usize) -> (LargeTree, PathBuf) {
    let (dir, root) = seed_workspace(&[("nested", count)]);
    for index in 0..siblings {
        fixture::write_text(&root.join(format!("zz-{index:04}.txt")), "x");
    }
    let tree = present_single_folder(dir, root, "virtualized", 800);
    tree.section.expand_folders();
    let nested = tree.root.join("nested");
    expand_and_load(&tree, &nested, count);
    flush_after_delay(Duration::from_millis(300));
    (tree, nested)
}

/// Design R13's C3 ("C3 as built"): a collapse that brings more than one
/// unit of rows into the band from after its subtree keeps `main`'s direct
/// collapse, at every size, and one with at most a unit after it stays
/// paced. Fails with the paced collapse for every class: the 400-row
/// collapses stay GTK-expanded after the input and run steps.
#[test]
fn test_a_collapse_with_rows_after_it_takes_the_direct_collapse() {
    for (count, siblings, direct) in [
        (64usize, 400usize, true),
        (4096, 400, true),
        (4096, PACED_SIBLINGS + 1, true),
        (4096, PACED_SIBLINGS, false),
        (4096, 0, false),
    ] {
        let (tree, nested) = expanded_nested(count, siblings);
        let before = tree.section.workspace_section_evidence();
        activate_row(&tree.section, &nested);
        let after_input = tree.section.workspace_section_evidence();
        let row = row_for_path(&tree.section, &nested).expect("the directory row");
        let label = format!("{siblings}:{count}");
        if direct {
            assert!(
                !row.is_expanded(),
                "{label}: collapsed in the input dispatch"
            );
            assert!(
                after_input.collapses.is_empty(),
                "{label}: no paced collapse"
            );
            assert_eq!(
                after_input.collapse_last_input_rows_after_direct, 1,
                "{label}"
            );
            assert_eq!(
                after_input.collapse_rows_after_direct_total,
                before.collapse_rows_after_direct_total + 1,
                "{label}"
            );
            assert_eq!(
                after_input.collapse_direct_total,
                before.collapse_direct_total + 1,
                "{label}"
            );
        } else {
            assert!(row.is_expanded(), "{label}: still GTK-expanded, collapsing");
            assert_eq!(
                after_input.collapses.len(),
                1,
                "{label}: one paced collapse"
            );
            assert_eq!(
                after_input.collapse_last_input_rows_after_direct, 0,
                "{label}"
            );
        }
        wait_until(Duration::from_secs(60), || {
            collapsed_and_settled(&tree.section, &nested)
        });
        let settled = tree.section.workspace_section_evidence();
        if direct {
            assert_eq!(
                settled.collapse_steps_total, before.collapse_steps_total,
                "{label}"
            );
        } else {
            assert!(
                settled.collapse_steps_total > before.collapse_steps_total,
                "{label}"
            );
        }
        drop(tree.window);
    }
}

/// Dispatch one main-loop source at a time until `done` holds, for a test
/// that must stop a collapse between two of its steps: `wait_until` drains
/// every ready source, and the collapse's step sources stay ready until it
/// ends, so one `wait_until` can run a whole collapse past the state the test
/// waits for.
fn pump_dispatches_until(timeout: Duration, mut done: impl FnMut() -> bool) {
    let context = glib::MainContext::default();
    let started = std::time::Instant::now();
    while !done() {
        assert!(
            started.elapsed() < timeout,
            "condition was not met within {timeout:?}"
        );
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
}

/// Set the collapse pacing for one test, and restore production pacing even
/// when an assertion fails.
struct Pacing;

impl Pacing {
    fn set(pacing: lushtext_core::ui::sidebar::CollapsePacingForTest) -> Self {
        lushtext_core::ui::sidebar::set_sidebar_collapse_pacing_for_test(pacing);
        Self
    }
}

impl Drop for Pacing {
    fn drop(&mut self) {
        lushtext_core::ui::sidebar::set_sidebar_collapse_pacing_for_test(
            lushtext_core::ui::sidebar::CollapsePacingForTest::FrameClock,
        );
    }
}

use lushtext_core::ui::sidebar::CollapsePacingForTest as CollapsePacing;

/// Design D6: the input dispatch shows the collapse (arrow class, the row's
/// and the expander's accessible state) and removes no row; the class goes
/// once the directory is collapsed. Fails on `main`: no class exists.
#[test]
fn test_collapse_shows_in_the_first_frame() {
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let store = shown_store(&tree, &nested);
    let expander = expander_for(&tree.section, &nested).expect("the row is realized");
    assert!(
        crate::common::accessible_expanded_is(&expander, true),
        "the expander reads expanded before the input (the check can fail)"
    );
    activate_tree_row(&tree.section, &nested);
    let row = row_for_path(&tree.section, &nested).expect("directory row");
    assert!(
        row.is_expanded(),
        "GTK still lists the children after the input"
    );
    assert_eq!(
        store.upgrade().expect("the store").n_items(),
        4096,
        "the input dispatch removes no row"
    );
    let expander = expander_for(&tree.section, &nested).expect("the row is realized");
    assert!(expander.has_css_class(COLLAPSING_CLASS));
    assert!(crate::common::accessible_expanded_is(&expander, false));
    let overlay = expander.parent().expect("the row overlay");
    assert!(crate::common::accessible_expanded_is(&overlay, false));
    // Design D21 (a guard, revised for D19, D25 and T1; task 10.9): the first
    // frame after the input changes at most one unit of row widgets, and the
    // step source the input armed runs before that frame (ledger A125), so
    // its `before-paint` already sees fewer rows than the input left; the
    // second frame's sees fewer still.
    let clock = tree
        .section
        .imp()
        .file_tree_view
        .frame_clock()
        .expect("a mapped list has a frame clock");
    let frames = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(u32, RowChange)>::new()));
    let seen = std::rc::Rc::new(std::cell::RefCell::new(BoundRows::new()));
    mark_new_row_widgets(&tree.section, &mut seen.borrow_mut());
    let before_tick_rows = std::rc::Rc::new(std::cell::RefCell::new(Vec::<u32>::new()));
    let before_handler = {
        let before_tick_rows = std::rc::Rc::clone(&before_tick_rows);
        let store = store.clone();
        clock.connect_before_paint(move |_| {
            let mut rows = before_tick_rows.borrow_mut();
            if rows.len() < 2 {
                rows.push(store.upgrade().map_or(0, |store| store.n_items()));
            }
        })
    };
    let handler = {
        let frames = std::rc::Rc::clone(&frames);
        let seen = std::rc::Rc::clone(&seen);
        let section = tree.section.clone();
        clock.connect_after_paint(move |_| {
            if frames.borrow().len() < 2 {
                let rows = store.upgrade().map_or(0, |store| store.n_items());
                let change = mark_new_row_widgets(&section, &mut seen.borrow_mut());
                frames.borrow_mut().push((rows, change));
            }
        })
    };
    wait_until(Duration::from_secs(10), || {
        frames.borrow().len() == 2 && before_tick_rows.borrow().len() == 2
    });
    clock.disconnect(handler);
    clock.disconnect(before_handler);
    let frames = frames.borrow().clone();
    let before_tick_rows = before_tick_rows.borrow().clone();
    assert!(
        before_tick_rows[0] < 4096,
        "the step source ran before the first frame: {before_tick_rows:?}"
    );
    assert!(
        before_tick_rows[1] < before_tick_rows[0],
        "and again before the second: {before_tick_rows:?}"
    );
    assert!(
        RowChange::over_unit(&[frames[0].1]).is_none(),
        "the first frame changed {:?}",
        frames[0].1
    );
    // Design R13's rung T1: the first frame lays out the intent alone, so
    // no bound row has left (no row widget shown or destroyed) by its
    // `after-paint`; D23's chunk removed only unbound rows.
    assert!(
        frames[0].1.shown == 0 && frames[0].1.destroyed == 0,
        "the first frame changed no row widget: {:?}",
        frames[0].1
    );
    assert!(
        frames[1].0 < 4096,
        "rows left by the second frame: {frames:?}"
    );
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    let expander = expander_for(&tree.section, &nested).expect("the row is realized");
    assert!(!expander.has_css_class(COLLAPSING_CLASS));
    drop(tree.window);
}

/// One frame's view of a collapse: the phase (`before-paint` or
/// `after-paint`), the frame counter, and the collapsing subtree's listed
/// rows then (0 once GTK has collapsed the row or its store is gone).
type FramePhaseRows = (char, i64, u32);

/// Record, for every frame until `nested` is collapsed, the subtree's rows at
/// `before-paint` and at `after-paint` (design D25).
fn collapse_rows_per_frame_phase(tree: &LargeTree, nested: &Path) -> Vec<FramePhaseRows> {
    let store = shown_store(tree, nested);
    let clock = tree
        .section
        .imp()
        .file_tree_view
        .frame_clock()
        .expect("a mapped list has a frame clock");
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::<FramePhaseRows>::new()));
    let row = row_for_path(&tree.section, nested).expect("directory row");
    let rows = move || {
        if row.is_expanded() {
            store.upgrade().map_or(0, |store| store.n_items())
        } else {
            0
        }
    };
    let before = {
        let events = std::rc::Rc::clone(&events);
        let rows = rows.clone();
        clock.connect_before_paint(move |clock| {
            events
                .borrow_mut()
                .push(('B', clock.frame_counter(), rows()));
        })
    };
    let after = {
        let events = std::rc::Rc::clone(&events);
        clock.connect_after_paint(move |clock| {
            events
                .borrow_mut()
                .push(('A', clock.frame_counter(), rows()));
        })
    };
    activate_tree_row(&tree.section, nested);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, nested)
    });
    clock.disconnect(before);
    clock.disconnect(after);
    events.borrow().clone()
}

/// Design D25 (task 11.3): no collapse step runs inside the frame clock's
/// dispatch: between a frame's `before-paint` and its `after-paint` the
/// collapsing subtree loses no row, while rows do leave over the collapse.
/// Fails before D25: the tick removed each bound unit in the frame's update
/// phase, between the two.
/// Rung T1 (design R13, task 10.7's reading): no bound unit runs before the
/// first frame after the input. Fails before T1: the first bound unit ran in
/// the step source the input armed.
#[test]
fn test_bound_units_run_outside_the_frame() {
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let events = collapse_rows_per_frame_phase(&tree, &nested);
    let inside: Vec<_> = events
        .windows(2)
        .filter(|pair| pair[0].0 == 'B' && pair[1].0 == 'A' && pair[0].1 == pair[1].1)
        .filter(|pair| pair[1].2 < pair[0].2)
        .map(|pair| (pair[0], pair[1]))
        .collect();
    assert!(
        inside.is_empty(),
        "rows left inside a frame dispatch: {inside:?} (all: {events:?})"
    );
    assert!(
        events.first().is_some_and(|first| first.2 < 4096),
        "rows left before the first frame: {events:?}"
    );
    // The step-source dispatches, each tagged with the counter of the frame
    // painted before it (ledger A125), so the ones before the first frame
    // carry a counter below that frame's.
    use lushtext_core::ui::sidebar::workspace_section::StepDispatchKind as Kind;
    let dispatches = tree
        .section
        .workspace_section_evidence()
        .collapse_step_dispatches;
    let first_frame = events.first().map_or(i64::MAX, |first| first.1);
    let before_first: Vec<Kind> = dispatches
        .iter()
        .filter(|dispatch| dispatch.frame_counter < first_frame)
        .map(|dispatch| dispatch.kind)
        .filter(|kind| *kind != Kind::Between && *kind != Kind::Idle)
        .collect();
    // Design R13's rung T1: D23's chunk runs before the first frame, and the
    // first bound unit only after it, so that frame lays out the intent alone.
    assert!(
        before_first.first() == Some(&Kind::Chunk) && !before_first.contains(&Kind::Bound),
        "D23's chunk before the first frame, and no bound unit until it has painted: \
         {before_first:?} (all: {dispatches:?})"
    );
    assert!(
        dispatches
            .iter()
            .any(|dispatch| dispatch.kind == Kind::Bound && dispatch.frame_counter >= first_frame),
        "the bound units run from the first frame's `after-paint` on: {dispatches:?}"
    );
    // At most one unit of bound rows per frame interval, or one destroy-only
    // unit in an interval whose removals created no row widget (design D18:
    // once the 32 rows after the subtree are all in the band, nothing is
    // listed after it).
    let unit = lushtext_core::ui::sidebar::policy::SIDEBAR_REALIZED_ROWS_PER_FRAME;
    let destroy_only = lushtext_core::ui::sidebar::policy::COLLAPSE_DESTROY_ONLY_ROWS;
    let mut per_interval = std::collections::BTreeMap::<i64, (u32, u32, u32)>::new();
    for dispatch in &dispatches {
        let entry = per_interval.entry(dispatch.frame_counter).or_default();
        match dispatch.kind {
            Kind::Bound | Kind::Descendant | Kind::Terminal => {
                entry.0 += dispatch.rows;
                entry.2 += dispatch.rows_created;
            }
            Kind::Idle => entry.1 += 1,
            Kind::Chunk | Kind::Between => {}
        }
    }
    let over: Vec<_> = per_interval
        .iter()
        .filter(|(_, (rows, _, created))| *rows > if *created == 0 { destroy_only } else { unit })
        .collect();
    assert!(
        over.is_empty(),
        "an interval removed more than {unit} bound rows: {over:?}"
    );
    // No busy loop between frames: an interval runs at most two dispatches
    // that take no step (the between-frames idle and the step source must
    // not re-arm each other while the interval is spent).
    let idling: Vec<_> = per_interval
        .iter()
        .filter(|(_, (_, idle, _))| *idle > 2)
        .collect();
    assert!(
        idling.is_empty(),
        "intervals with more than two idle step dispatches: {idling:?}"
    );
    drop(tree.window);
}

/// Design D23 (task 10.9's guard, the large and held cases): at 0:131,072
/// the first step-source dispatch after the input removes D23's one unbound
/// chunk, before the first frame, and the between-frames idle removes the
/// rest of the unbound rows; under `CollapsePacingForTest::Hold` nothing is
/// removed at all.
#[test]
fn test_a_large_collapse_takes_one_chunk_then_the_between_frames_idle() {
    use lushtext_core::ui::sidebar::workspace_section::StepDispatchKind as Kind;
    let count = 1 << 17;
    let (tree, nested) = expanded_nested(count, 0);
    let events = collapse_rows_per_frame_phase(&tree, &nested);
    let first_frame = events.first().map_or(i64::MAX, |first| first.1);
    let dispatches = tree
        .section
        .workspace_section_evidence()
        .collapse_step_dispatches;
    let before_first: Vec<_> = dispatches
        .iter()
        .filter(|dispatch| dispatch.frame_counter < first_frame)
        .filter(|dispatch| dispatch.kind != Kind::Idle)
        .collect();
    assert!(
        before_first
            .first()
            .is_some_and(|first| first.kind == Kind::Chunk),
        "the first step-source dispatch takes D23's chunk: {before_first:?}"
    );
    assert_eq!(
        before_first
            .iter()
            .filter(|dispatch| dispatch.kind == Kind::Chunk)
            .count(),
        1,
        "one chunk from the step source before the first frame: {before_first:?}"
    );
    assert!(
        dispatches
            .iter()
            .any(|dispatch| dispatch.kind == Kind::Between),
        "the between-frames idle removes the rest: {dispatches:?}"
    );
    assert!(
        tree.section
            .workspace_section_evidence()
            .collapse_between_steps_total
            > 0
    );
    // Held: the input dispatch and everything after it remove nothing.
    expand_and_load(&tree, &nested, count);
    flush_after_delay(Duration::from_millis(300));
    let pacing = Pacing::set(CollapsePacing::Hold);
    let store = row_for_path(&tree.section, &nested)
        .and_then(|row| row.children())
        .and_downcast::<gtk4::gio::ListStore>()
        .expect("the directory's store");
    activate_tree_row(&tree.section, &nested);
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(
        store.n_items(),
        u32::try_from(count).unwrap_or(u32::MAX),
        "a held collapse removes no row: {}",
        collapse_state(&tree.section)
    );
    drop(pacing);
    wait_until(Duration::from_secs(120), || {
        collapse_settled(&tree.section, &nested)
    });
    drop(tree.window);
}

/// Design D25 (task 11.3): each frame's `after-paint` arms the step source,
/// which runs before the next frame (ledger A125): while the subtree still
/// lists rows at a frame's `after-paint`, the next frame's `before-paint`
/// sees fewer. Fails before D25: the unit left in the next frame's update
/// phase, after its `before-paint`.
#[test]
fn test_the_step_source_runs_before_the_next_frame() {
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let steps = tree
        .section
        .workspace_section_evidence()
        .collapse_steps_total;
    let events = collapse_rows_per_frame_phase(&tree, &nested);
    assert!(
        tree.section
            .workspace_section_evidence()
            .collapse_steps_total
            > steps,
        "the collapse was paced"
    );
    let stalled: Vec<_> = events
        .windows(2)
        .filter(|pair| pair[0].0 == 'A' && pair[1].0 == 'B' && pair[0].2 > 0)
        .filter(|pair| pair[1].2 >= pair[0].2)
        .map(|pair| (pair[0], pair[1]))
        .collect();
    assert!(
        stalled.is_empty(),
        "a frame interval with rows pending removed none before the next frame: {stalled:?} \
         (all: {events:?})"
    );
    // With the frame clock suppressed the fallback still completes it.
    let _pacing = Pacing::set(CollapsePacing::NoFrameClock);
    expand_and_load(&tree, &nested, 4096);
    activate_tree_row(&tree.section, &nested);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    drop(tree.window);
}

/// Design D25 (task 11.3): the section's `after-paint` handler lives only
/// while a collapse is pending: connected during it, disconnected once it
/// ends, and no frame runs it after.
#[test]
fn test_the_step_source_leaves_no_frame_handler() {
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    assert!(
        !tree
            .section
            .workspace_section_evidence()
            .collapse_after_paint_connected,
        "no handler before any collapse"
    );
    activate_tree_row(&tree.section, &nested);
    assert!(
        tree.section
            .workspace_section_evidence()
            .collapse_after_paint_connected,
        "the input connects the handler on the mapped list"
    );
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    let evidence = tree.section.workspace_section_evidence();
    assert!(
        !evidence.collapse_after_paint_connected,
        "the end of the collapse disconnects it: {}",
        collapse_state(&tree.section)
    );
    assert!(evidence.collapse_after_paint_runs > 0, "frames ran it");
    let runs = evidence.collapse_after_paint_runs;
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(
        tree.section
            .workspace_section_evidence()
            .collapse_after_paint_runs,
        runs,
        "no frame runs it after the collapse"
    );
    // Cancelled by a re-expansion: the handler goes with the collapse.
    expand_and_load(&tree, &nested, 4096);
    flush_after_delay(Duration::from_millis(200));
    activate_tree_row(&tree.section, &nested);
    assert!(
        tree.section
            .workspace_section_evidence()
            .collapse_after_paint_connected,
        "the second collapse connects it again"
    );
    // A second activation, before any step ran, expands the row again.
    activate_tree_row(&tree.section, &nested);
    assert_eq!(
        tree.section.workspace_section_evidence().last_collapse_end,
        Some(lushtext_core::ui::sidebar::workspace_section::CollapseEnd::ReExpanded),
        "the second activation re-expanded the row"
    );
    assert!(
        !tree
            .section
            .workspace_section_evidence()
            .collapse_after_paint_connected,
        "the re-expansion that ends the collapse disconnects it: {}",
        collapse_state(&tree.section)
    );
    wait_for_refresh_idle(&tree.section);
    // Across an unmap and map mid-collapse: off while unmapped, back on map,
    // and the collapse still completes from frames.
    expand_and_load(&tree, &nested, 4096);
    flush_after_delay(Duration::from_millis(200));
    activate_tree_row(&tree.section, &nested);
    tree.section.set_visible(false);
    assert!(
        !tree
            .section
            .workspace_section_evidence()
            .collapse_after_paint_connected,
        "unmapping the list disconnects it"
    );
    tree.section.set_visible(true);
    let pending = !tree
        .section
        .workspace_section_evidence()
        .collapses
        .is_empty();
    assert_eq!(
        tree.section
            .workspace_section_evidence()
            .collapse_after_paint_connected,
        pending,
        "mapping it again reconnects it while the collapse is pending"
    );
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    assert!(
        !tree
            .section
            .workspace_section_evidence()
            .collapse_after_paint_connected
    );
    drop(tree.window);
}

/// Design D10 (task 6.1): a directory load brings at most one unit of rows
/// into the realized band per dispatch, the first batch being that unit, and
/// the first frame after it shows the rows at the top of the listing. With 0
/// and 400 rows after a 256-entry directory. Fails with BL's batching alone:
/// after the first small batch the rest of the first 256 rows go in one turn
/// and the frame after it creates about 200 row widgets.
#[test]
fn test_expanding_a_directory_brings_at_most_one_unit_into_the_band_per_frame() {
    for siblings in [0usize, 400] {
        let tree = tree_with_siblings(256, siblings);
        let nested = tree.root.join("nested");
        wait_until(Duration::from_secs(10), || {
            row_for_path(&tree.section, &nested).is_some()
        });
        wait_for_refresh_idle(&tree.section);
        flush_after_delay(Duration::from_millis(300));
        let clock = tree
            .section
            .imp()
            .file_tree_view
            .frame_clock()
            .expect("a mapped list has a frame clock");
        // At each painted frame: the rows the directory's store lists, and
        // which of its entries the list draws.
        let frames = std::rc::Rc::new(std::cell::RefCell::new(
            Vec::<(u32, BTreeSet<String>)>::new(),
        ));
        let handler = {
            let frames = std::rc::Rc::clone(&frames);
            let section = tree.section.clone();
            let nested = nested.clone();
            clock.connect_after_paint(move |_| {
                let listed = row_for_path(&section, &nested)
                    .and_then(|row| row.children())
                    .map_or(0, |store| store.n_items());
                frames.borrow_mut().push((listed, drawn_entries(&section)));
            })
        };
        let changed = row_widgets_changed_per_dispatch(
            &tree.section,
            || activate_tree_row(&tree.section, &nested),
            || loaded_and_settled(&tree.section, &nested, 256),
        );
        clock.disconnect(handler);
        let over = changed
            .iter()
            .copied()
            .find(|change| change.shown > COLLAPSE_UNIT);
        assert!(
            over.is_none(),
            "{siblings} rows after: one dispatch showed {over:?} row widgets, over the unit of {COLLAPSE_UNIT}: {changed:?}"
        );
        flush_after_delay(Duration::from_millis(100));
        let settled = drawn_entries(&tree.section);
        assert!(!settled.is_empty(), "the loaded entries are drawn");
        let frames = frames.borrow().clone();
        let first = frames
            .iter()
            .position(|(listed, _)| *listed > 0)
            .expect("a frame painted the loaded rows");
        // The first frame that paints listed rows draws every entry of the
        // first unit that the loaded tree draws: the first batch covers the
        // viewport at this window's height (design D10). (The loaded tree may
        // also hold a row GTK keeps realized outside the viewport.)
        let first_unit: BTreeSet<String> = (0..COLLAPSE_UNIT).map(file_name).collect();
        let expected: BTreeSet<String> = settled.intersection(&first_unit).cloned().collect();
        assert!(
            !expected.is_empty() && frames[first].1.is_superset(&expected),
            "{siblings} rows after: the frame after the first batch drew {:?}, the loaded tree {settled:?}",
            frames[first].1
        );
        drop(tree.window);
    }
}

/// The directory entries (`file_name` labels) the list draws.
fn drawn_entries(section: &LushtextWorkspaceSection) -> BTreeSet<String> {
    rendered_labels(section)
        .into_iter()
        .filter(|label| label.starts_with("row-"))
        .collect()
}

/// Design D19 (task 10.3): a 131,072-entry collapse removes its unbound rows
/// between frames, from the idle below the frame clock, not one chunk per
/// tick: every unbound chunk is a between-frames (or, on a starved clock,
/// fallback) step, and the frame ticks take only the bound units. Fails
/// with unbound chunks paced by the ticks (no between-frames step).
#[test]
fn test_unbound_rows_leave_between_frames() {
    use lushtext_core::ui::sidebar::policy::COLLAPSE_UNBOUND_CHUNK_ROWS;
    let count = 128 * 1024;
    let (tree, nested) = expanded_nested(count, 0);
    let before = tree.section.workspace_section_evidence();
    activate_tree_row(&tree.section, &nested);
    wait_until(Duration::from_secs(120), || {
        collapse_settled(&tree.section, &nested)
    });
    let after = tree.section.workspace_section_evidence();
    let between = after.collapse_between_steps_total - before.collapse_between_steps_total;
    let starved = after.collapse_starved_steps_total - before.collapse_starved_steps_total;
    // The band holds at most a few hundred rows (A1), so all but one chunk's
    // worth of the listing is unbound.
    let unbound_chunks =
        u64::try_from(count).unwrap_or(u64::MAX) / u64::from(COLLAPSE_UNBOUND_CHUNK_ROWS) - 1;
    assert!(between > 0, "no unbound chunk ran between frames");
    assert!(
        between + starved >= unbound_chunks,
        "{between} between-frames and {starved} fallback steps for {unbound_chunks} unbound chunks"
    );
    assert_eq!(
        after.last_collapse_end,
        Some(lushtext_core::ui::sidebar::workspace_section::CollapseEnd::Completed)
    );
    drop(tree.window);
}

/// Rung T1's hold on unbound turns (design R13, "Attributions of the T1
/// round"): no between-frames turn runs before the first frame after the
/// input, so a turn in flight cannot delay that frame past its frame
/// callback. D23's one chunk still runs in the step source before it, and
/// the first bound unit still runs in the first interval after that frame
/// (the hold costs the unbound rows an interval, not the bound ones). Fails
/// when the between-frames idle starts at the input: the T1 round's
/// 0:131,072 collapses ran three or four turns before their first frame.
/// 16,385 entries is the T1 round's case with one chunk left after D23's.
#[test]
fn test_no_unbound_turn_runs_before_the_first_frame_after_the_input() {
    use lushtext_core::ui::sidebar::workspace_section::StepDispatchKind as Kind;
    let count = 16 * 1024 + 1;
    let (tree, nested) = expanded_nested(count, 0);
    let clock = tree
        .section
        .imp()
        .file_tree_view
        .frame_clock()
        .expect("a mapped list has a frame clock");
    let input_frame = clock.frame_counter();
    activate_tree_row(&tree.section, &nested);
    let paints = tree
        .section
        .workspace_section_evidence()
        .collapse_after_paint_runs;
    assert!(
        !tree
            .section
            .workspace_section_evidence()
            .collapse_between_armed,
        "the input armed the between-frames idle before its first frame"
    );
    // One dispatch at a time up to the first `after-paint`: no turn runs.
    pump_dispatches_until(Duration::from_secs(30), || {
        let evidence = tree.section.workspace_section_evidence();
        assert!(
            !evidence
                .collapse_step_dispatches
                .iter()
                .any(|dispatch| dispatch.kind == Kind::Between),
            "a between-frames turn ran before the first after-paint"
        );
        evidence.collapse_after_paint_runs > paints
    });
    let painted = clock.frame_counter();
    wait_until(Duration::from_secs(120), || {
        collapse_settled(&tree.section, &nested)
    });
    let evidence = tree.section.workspace_section_evidence();
    let first_bound = evidence
        .collapse_step_dispatches
        .iter()
        .find(|dispatch| dispatch.kind == Kind::Bound)
        .map(|dispatch| dispatch.frame_counter);
    assert_eq!(
        first_bound,
        Some(painted),
        "the first bound unit runs in the first interval after the frame that showed the intent"
    );
    let turns: Vec<i64> = evidence
        .collapse_step_dispatches
        .iter()
        .filter(|dispatch| dispatch.kind == Kind::Between)
        .map(|dispatch| dispatch.frame_counter)
        .collect();
    assert!(!turns.is_empty(), "the collapse ran between-frames turns");
    assert!(
        turns.iter().all(|frame| *frame > input_frame),
        "a between-frames turn ran before the first frame after the input (frame {input_frame}): {turns:?}"
    );
    assert_eq!(
        evidence.last_collapse_end,
        Some(lushtext_core::ui::sidebar::workspace_section::CollapseEnd::Completed)
    );
    drop(tree.window);
}

/// Design D2/D4: with 32 rows after a 256-entry directory (`PACED_SIBLINGS`
/// and `top.txt`, the most C3 leaves paced), no dispatch of the collapse
/// makes GTK create more than one unit of row widgets or destroy more than
/// a destroy-only unit. Fails with `main`'s direct collapse, which destroys
/// most of the band's row widgets in one dispatch (checked by routing every
/// collapse direct, 2026-11-24).
#[test]
fn test_collapsing_a_directory_with_rows_after_it_bounds_each_step() {
    let (tree, nested) = expanded_nested(256, PACED_SIBLINGS);
    let created = row_widgets_changed_per_dispatch(
        &tree.section,
        || activate_tree_row(&tree.section, &nested),
        || collapse_settled(&tree.section, &nested),
    );
    let over = RowChange::over_unit(&created);
    assert!(
        over.is_none(),
        "one dispatch changed {over:?} row widgets, over the unit of {COLLAPSE_UNIT}: {created:?}"
    );
    let evidence = tree.section.workspace_section_evidence();
    assert!(evidence.collapse_steps_total > 1, "the collapse was sliced");
    assert_eq!(
        evidence.last_collapse_end,
        Some(lushtext_core::ui::sidebar::workspace_section::CollapseEnd::Completed)
    );
    drop(tree.window);
}

/// Design D7: the folder header collapses two expanded 4,096-entry roots;
/// every dispatch replaces at most one unit across both, and both complete.
/// Fails on `main`: one direct call per root.
#[test]
fn test_collapsing_several_directories_takes_one_unit_per_frame() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("two roots tempdir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    seed_files(&first, 4096);
    seed_files(&second, 4096);
    save_workspaces(vec![WorkspaceConfig::with_folders(
        WorkspaceId::new("two-roots"),
        "two-roots",
        vec![
            WorkspaceFolder::new(first.clone()),
            WorkspaceFolder::new(second.clone()),
        ],
    )]);
    let (window, sidebar) = present_sidebar_window(800);
    let section = mapped_sections(&sidebar, 1).remove(0);
    section.expand_folders();
    wait_until(Duration::from_secs(60), || {
        [&first, &second].iter().all(|root| {
            section
                .workspace_section_evidence()
                .directory_listings
                .iter()
                .any(|listing| listing.path == **root && listing.directory_retained_entries == 4096)
        }) && !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
    });
    flush_after_delay(Duration::from_millis(300));
    let shown_paths = std::rc::Rc::new(std::cell::RefCell::new(Vec::<PathBuf>::new()));
    let created = row_widgets_changed_per_dispatch(
        &section,
        || section.collapse_folders(),
        || {
            shown_paths.borrow_mut().extend(
                realized_expanders(&section)
                    .iter()
                    .filter_map(expander_path)
                    .filter(|path| path.starts_with(&second) && *path != second),
            );
            collapse_settled(&section, &first) && collapse_settled(&section, &second)
        },
    );
    let over = RowChange::over_unit(&created);
    assert!(
        over.is_none(),
        "one dispatch changed {over:?} row widgets across both roots: {created:?}"
    );
    // Design R5, D7: the second root's rows leave unbound before the first
    // root's units bring rows into the band, so none of them is ever shown.
    let shown = shown_paths.borrow();
    assert!(
        shown.is_empty(),
        "{} rows of second/ were shown during the collapse, first {:?}",
        shown.len(),
        shown.first()
    );
    drop(window);
    drop(sidebar);
}

/// Design D4: with the frame clock kept from pacing the collapse, the
/// starvation fallback completes it and the evidence counts its steps; once
/// ticks resume, no dispatch lays out two units.
#[test]
fn test_a_starved_frame_clock_still_completes_the_collapse() {
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let pacing = Pacing::set(CollapsePacing::NoFrameClock);
    activate_tree_row(&tree.section, &nested);
    wait_until(Duration::from_secs(30), || {
        tree.section
            .workspace_section_evidence()
            .collapse_starved_steps_total
            > 0
    });
    let created = row_widgets_changed_per_dispatch(
        &tree.section,
        || drop(pacing),
        || collapse_settled(&tree.section, &nested),
    );
    let over = RowChange::over_unit(&created);
    assert!(
        over.is_none(),
        "a resumed tick changed {over:?}: {created:?}"
    );
    drop(tree.window);
}

/// Design D6: a collapsing row scrolled out of the band and back keeps the
/// overlay's and the expander's accessible `expanded` false and the class.
#[test]
fn test_a_rebound_collapsing_row_stays_collapsed_for_accessibility() {
    let (tree, nested) = expanded_nested(128 * 1024, 0);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    let section = tree.section.clone();
    let index = tree_index_for_path(&section, &nested).expect("row index");
    section
        .imp()
        .file_tree_view
        .scroll_to(index + 4000, gtk4::ListScrollFlags::NONE, None);
    flush_after_delay(Duration::from_millis(150));
    section
        .imp()
        .file_tree_view
        .scroll_to(index, gtk4::ListScrollFlags::NONE, None);
    flush_after_delay(Duration::from_millis(150));
    assert!(
        !section.workspace_section_evidence().collapses.is_empty(),
        "the collapse is still running when the row comes back"
    );
    let expander = expander_for(&section, &nested).expect("the row is realized again");
    assert!(expander.has_css_class(COLLAPSING_CLASS));
    assert!(crate::common::accessible_expanded_is(&expander, false));
    let overlay = expander.parent().expect("the row overlay");
    assert!(crate::common::accessible_expanded_is(&overlay, false));
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&section, &nested)
    });
    drop(tree.window);
}

/// Design D8: the directory row on screen does not move at any step, the
/// slice bin makes no correction, and no forwarded request is left pending.
#[test]
fn test_collapse_keeps_the_directory_row_still() {
    let (tree, nested) = expanded_nested(256, PACED_SIBLINGS);
    let bin = tree.section.imp().file_tree_slice.clone();
    let corrections = bin.correction_count();
    let before = row_placement(&tree.section, "nested").expect("the row is drawn");
    let steps = tree
        .section
        .workspace_section_evidence()
        .collapse_steps_total;
    activate_tree_row(&tree.section, &nested);
    assert!(
        row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded()),
        "the collapse is paced, not direct"
    );
    let context = glib::MainContext::default();
    let started = std::time::Instant::now();
    while !collapse_settled(&tree.section, &nested) {
        assert!(started.elapsed() < Duration::from_secs(60));
        if context.iteration(false) {
            if let Some(now) = row_placement(&tree.section, "nested") {
                assert_eq!(now, before, "the directory row moved during the collapse");
            }
        } else {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(row_placement(&tree.section, "nested"), Some(before));
    assert!(
        tree.section
            .workspace_section_evidence()
            .collapse_steps_total
            > steps
    );
    assert_eq!(bin.correction_count(), corrections);
    assert!(!bin.outer_request_pending());
    drop(tree.window);
}

/// Design D7: selection and focus inside the subtree move to the directory
/// row, on the sliced and on the direct path. Fails on `main`, where the
/// selection lands on another row.
#[test]
fn test_collapse_moves_selection_and_focus_to_the_directory() {
    for count in [4096usize, 8] {
        let (tree, nested) = expanded_nested(count, PACED_SIBLINGS);
        let child = nested.join(file_name(3));
        select_and_scroll_to(&tree.section, &child);
        let directory = tree_index_for_path(&tree.section, &nested).expect("row index");
        let child_index = tree_index_for_path(&tree.section, &child).expect("row index");
        tree.section.imp().file_tree_view.grab_focus();
        tree.section.imp().file_tree_view.scroll_to(
            child_index,
            gtk4::ListScrollFlags::FOCUS | gtk4::ListScrollFlags::SELECT,
            None,
        );
        wait_until(Duration::from_secs(5), || {
            focused_row_position(&tree.section) == Some(child_index)
        });
        activate_tree_row(&tree.section, &nested);
        assert_eq!(
            selection(&tree.section).selected(),
            directory,
            "{count} entries: the selection moves to the directory row at once"
        );
        // The list's focus row follows (finding 9).
        wait_until(Duration::from_secs(5), || {
            focused_row_position(&tree.section) == Some(directory)
        });
        wait_until(Duration::from_secs(60), || {
            collapse_settled(&tree.section, &nested)
        });
        assert_eq!(selection(&tree.section).selected(), directory);
        assert_eq!(focused_row_position(&tree.section), Some(directory));
        drop(tree.window);
    }
}

/// The flattened position of the list's focus row (the focused list item
/// widget's row), if the list holds the focus.
fn focused_row_position(section: &LushtextWorkspaceSection) -> Option<u32> {
    section
        .imp()
        .file_tree_view
        .focus_child()
        .and_then(|item| find_descendant(&item, glib::object::ObjectExt::is::<gtk4::TreeExpander>))
        .and_downcast::<gtk4::TreeExpander>()
        .and_then(|expander| expander.list_row())
        .map(|row| row.position())
        .filter(|position| *position != gtk4::INVALID_LIST_POSITION)
}

/// Design D7: expanding a collapsing directory again, by activation and by a
/// reveal, ends the collapse and lists every entry, with no full recache.
/// Passes on `main` (no collapse is ever pending there) and must stay green.
#[test]
fn test_expanding_again_mid_collapse_restores_every_entry() {
    use lushtext_core::ui::sidebar::workspace_section::CollapseEnd;
    let count = 4096;
    let (tree, nested) = expanded_nested(count, 0);
    let rebuilds = tree
        .section
        .workspace_section_evidence()
        .cache_full_rebuilds;
    for route in ["activation", "reveal"] {
        let pacing = Pacing::set(CollapsePacing::NoFrameClock);
        activate_tree_row(&tree.section, &nested);
        pump_dispatches_until(Duration::from_secs(30), || {
            tree.section
                .workspace_section_evidence()
                .collapses
                .first()
                .is_some_and(|collapse| collapse.steps > 0)
        });
        // No main-loop turn since the predicate held: the collapse is pinned
        // after its first steps, with rows already drained.
        lushtext_core::ui::sidebar::set_sidebar_collapse_pacing_for_test(CollapsePacing::Hold);
        assert!(
            listing(&tree.section, &nested).directory_retained_entries < count
                || !tree
                    .section
                    .workspace_section_evidence()
                    .collapses
                    .is_empty(),
            "{route}: the collapse is pending"
        );
        if route == "activation" {
            activate_tree_row(&tree.section, &nested);
        } else {
            activate_reveal_workspace_path(&tree.window, &nested.join(file_name(10)));
        }
        drop(pacing);
        wait_until(Duration::from_secs(60), || {
            row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded())
                && listing(&tree.section, &nested).directory_retained_entries == count
                && !tree
                    .section
                    .workspace_section_evidence()
                    .refresh_blocks_readiness
        });
        let evidence = tree.section.workspace_section_evidence();
        assert_eq!(
            evidence.last_collapse_end,
            Some(CollapseEnd::ReExpanded),
            "{route}"
        );
        assert!(evidence.collapses.is_empty(), "{route}");
        assert_eq!(
            evidence.cache_full_rebuilds, rebuilds,
            "{route}: no full recache"
        );
        let expander = expander_for(&tree.section, &nested).expect("realized");
        assert!(!expander.has_css_class(COLLAPSING_CLASS), "{route}");
        assert!(
            crate::common::accessible_expanded_is(&expander, true),
            "{route}"
        );
    }
    drop(tree.window);
}

/// Design D4: a collapse completes when the sidebar is hidden mid-collapse,
/// and when the section body is collapsed, and readiness then clears.
#[test]
fn test_a_collapse_nobody_draws_completes() {
    for hide in ["sidebar", "section-body"] {
        let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
        let pacing = Pacing::set(CollapsePacing::Hold);
        activate_tree_row(&tree.section, &nested);
        if hide == "sidebar" {
            gio::prelude::ActionGroupExt::activate_action(&tree.window, "toggle-sidebar", None);
        } else {
            tree.section.imp().collapse_button.emit_clicked();
        }
        wait_until(Duration::from_secs(10), || {
            !tree.section.imp().file_tree_view.is_mapped()
        });
        assert!(
            !tree
                .section
                .workspace_section_evidence()
                .collapses
                .is_empty(),
            "{hide}: the collapse is still pending when the list is hidden"
        );
        drop(pacing);
        wait_until(Duration::from_secs(60), || {
            row_for_path(&tree.section, &nested).is_some_and(|row| !row.is_expanded())
                && !tree
                    .section
                    .workspace_section_evidence()
                    .refresh_blocks_readiness
        });
        assert!(
            tree.section
                .workspace_section_evidence()
                .collapses
                .is_empty(),
            "{hide}"
        );
        if hide == "sidebar" {
            // The sidebar's visibility is a persisted preference: show it
            // again so the next window maps its sections.
            gio::prelude::ActionGroupExt::activate_action(&tree.window, "toggle-sidebar", None);
            flush_after_delay(Duration::from_millis(300));
        }
        drop(tree.window);
    }
}

/// Design D4: collapsing `outer/` drains and collapses the expanded
/// `inner/` (16,385 entries) before any of `outer/`'s own rows go.
#[test]
fn test_ancestor_collapse_drains_the_descendant_first() {
    let (dir, root) = seed_workspace(&[("outer/inner", 16 * 1024 + 1)]);
    for index in 0..100 {
        fixture::write_text(&root.join("outer").join(format!("o-{index:03}.txt")), "x");
    }
    let tree = present_single_folder(dir, root, "virtualized", 800);
    tree.section.expand_folders();
    let outer = tree.root.join("outer");
    let inner = outer.join("inner");
    expand_and_load(&tree, &outer, 101);
    expand_and_load(&tree, &inner, 16 * 1024 + 1);
    flush_after_delay(Duration::from_millis(300));
    let outer_store = shown_store(&tree, &outer);
    let inner_row = row_for_path(&tree.section, &inner).expect("inner row");
    activate_tree_row(&tree.section, &outer);
    let context = glib::MainContext::default();
    let started = std::time::Instant::now();
    while !collapse_settled(&tree.section, &outer) {
        assert!(started.elapsed() < Duration::from_secs(60));
        if context.iteration(false)
            && let Some(store) = outer_store.upgrade()
            && store.n_items() < 101
        {
            assert!(
                !inner_row.is_expanded(),
                "outer/ lost rows while inner/ was still expanded"
            );
        }
    }
    drop(tree.window);
}

/// Design D7: a collapse during a load stops the load (no batch is applied
/// afterwards), and re-expansion loads afresh. 32,768 entries keep the load
/// running for many turns (asserted when the collapse starts) while keeping
/// the shard's memory: at 131,072 entries a re-expansion right after the
/// stopped load peaked at 2.4 GiB in a debug build, past the shard's 2 GiB
/// scope.
#[test]
fn test_collapse_stops_loads_and_refreshes_under_it() {
    let count = 32 * 1024;
    let tree = large_tree(&[("nested", count)], 800);
    let nested = tree.root.join("nested");
    wait_until(Duration::from_secs(10), || {
        row_for_path(&tree.section, &nested).is_some()
    });
    wait_for_refresh_idle(&tree.section);
    activate_tree_row(&tree.section, &nested);
    // One dispatch at a time, so the collapse starts at the load's first
    // batch: `wait_until` drains every ready source, the whole load with it.
    let context = glib::MainContext::default();
    let started = std::time::Instant::now();
    // The shown store, not the evidence's listings: GTK's `is_expandable()`
    // probe gives the path a second store (A22), whose listing can be whole.
    let shown_rows = || {
        row_for_path(&tree.section, &nested)
            .and_then(|row| row.children())
            .map_or(0, |children| children.n_items())
    };
    while shown_rows() == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the load lists a first batch"
        );
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let store = shown_store(&tree, &nested);
    let loaded = shown_rows();
    assert!(
        usize::try_from(loaded).is_ok_and(|loaded| loaded < count),
        "the collapse must start while the load runs ({loaded} of {count} rows shown)"
    );
    activate_tree_row(&tree.section, &nested);
    let mut last = store.upgrade().map_or(0, |store| store.n_items());
    let started = std::time::Instant::now();
    while !collapse_settled(&tree.section, &nested) {
        assert!(started.elapsed() < Duration::from_secs(60));
        if context.iteration(false)
            && let Some(store) = store.upgrade()
        {
            assert!(
                store.n_items() <= last,
                "a load batch was applied to a collapsing store ({last} -> {}): {}",
                store.n_items(),
                collapse_state(&tree.section)
            );
            last = store.n_items();
        }
    }
    // The finished collapse retired the drained listing: no listing of the
    // path keeps a loading row once nothing loads (the step once held the
    // store while GTK collapsed the row, so its retirement was skipped and
    // the drained mirror outlived it).
    assert!(
        tree.section
            .workspace_section_evidence()
            .directory_listings
            .iter()
            .all(|listing| listing.path != nested || listing.loading_tail_rows == 0),
        "the collapsed listing is retired: {:?}",
        tree.section.workspace_section_evidence().directory_listings
    );
    activate_tree_row(&tree.section, &nested);
    let reloaded = crate::common::wait_until_or_false(Duration::from_secs(60), || {
        usize::try_from(shown_rows()).is_ok_and(|shown| shown == count)
            && !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    assert!(
        reloaded,
        "re-expansion reloads every entry: expanded={:?} shown={} listings={:?} {}",
        row_for_path(&tree.section, &nested).map(|row| row.is_expanded()),
        shown_rows(),
        tree.section
            .workspace_section_evidence()
            .directory_listings
            .iter()
            .filter(|listing| listing.path == nested)
            .collect::<Vec<_>>(),
        collapse_state(&tree.section)
    );
    drop(tree.window);
}

/// Design D5: activation, the folder header's collapse and toggle, and a
/// failed reveal all collapse through the entry point, counted by the
/// evidence. (The arrow is covered by
/// `test_the_arrow_click_survives_an_icon_recreation`, and the expander's
/// keys, which reach it once Tab focuses it (A119), by
/// `test_expander_keys_take_the_collapse_entry_point`.)
#[test]
fn test_collapse_routes_reach_the_entry_point() {
    let (tree, nested) = expanded_nested(8, 0);
    let counted = |section: &LushtextWorkspaceSection| {
        let evidence = section.workspace_section_evidence();
        evidence.collapse_direct_total + evidence.collapse_steps_total
    };
    let before = counted(&tree.section);
    activate_tree_row(&tree.section, &nested);
    wait_until(Duration::from_secs(10), || {
        collapse_settled(&tree.section, &nested)
    });
    assert!(counted(&tree.section) > before, "activation");
    let before = counted(&tree.section);
    tree.section.collapse_folders();
    wait_until(Duration::from_secs(10), || {
        collapse_settled(&tree.section, &tree.root)
    });
    assert!(counted(&tree.section) > before, "header collapse");
    tree.section.toggle_folders();
    wait_until(Duration::from_secs(10), || {
        row_for_path(&tree.section, &tree.root).is_some_and(|row| row.is_expanded())
    });
    wait_for_refresh_idle(&tree.section);
    let before = counted(&tree.section);
    tree.section.toggle_folders();
    wait_until(Duration::from_secs(10), || {
        collapse_settled(&tree.section, &tree.root)
    });
    assert!(counted(&tree.section) > before, "header toggle");
    tree.section.expand_folders();
    wait_for_refresh_idle(&tree.section);
    let before = counted(&tree.section);
    let status = reveal_and_settle(&tree.window, &tree.sidebar, &nested.join("missing.txt"));
    assert_eq!(status, WorkspaceRevealStatus::NotFound);
    wait_for_refresh_idle(&tree.section);
    assert!(counted(&tree.section) > before, "reveal failure");
    drop(tree.window);
}

/// Design D5's key route (ledger A119): a click leaves the focus on the row
/// widget and Tab moves it onto the expander, from which its class collapse
/// keys reach LushText's entry point (sliced, origin `Keys`); an expand key on
/// the collapsing row ends the collapse, and an expand key on a collapsed row
/// is GTK's own.
#[test]
fn test_expander_keys_take_the_collapse_entry_point() {
    use lushtext_core::ui::sidebar::workspace_section::{CollapseEnd, CollapseOrigin};
    const MINUS: u32 = 0x2d;
    const PLUS: u32 = 0x2b;
    const CONTROL_L: u32 = 0xffe3;
    const SPACE: u32 = 0x20;
    const TAB: u32 = 0xff09;
    let (tree, nested) = expanded_nested(4096, 0);
    let pacing = Pacing::set(CollapsePacing::Hold);
    let expander = expander_for(&tree.section, &nested).expect("the row is realized");
    let label = expander.child().expect("the row content");
    crate::common::activate_with_real_press(&tree.window, &label);
    let focus_is_the_row = || {
        gtk4::prelude::GtkWindowExt::focus(&tree.window)
            .and_downcast::<gtk4::TreeExpander>()
            .is_some_and(|focused| focused.list_row() == row_for_path(&tree.section, &nested))
    };
    let row_widget = expander
        .parent()
        .and_then(|overlay| overlay.parent())
        .expect("the row widget");
    // A click leaves the focus on the row widget, where no expander key
    // reaches (A119).
    wait_until(Duration::from_secs(5), || {
        gtk4::prelude::GtkWindowExt::focus(&tree.window).as_ref() == Some(&row_widget)
    });
    crate::common::type_with_real_keyboard(&[&[TAB]]);
    wait_until(Duration::from_secs(5), focus_is_the_row);
    crate::common::type_with_real_keyboard(&[&[MINUS]]);
    wait_until(Duration::from_secs(5), || {
        tree.section.workspace_section_evidence().collapses.len() == 1
    });
    let evidence = tree.section.workspace_section_evidence();
    assert_eq!(
        evidence.collapses.len(),
        1,
        "{}",
        collapse_state(&tree.section)
    );
    assert_eq!(evidence.collapses[0].origin, CollapseOrigin::Keys);
    assert!(
        row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded()),
        "the collapse is paced, not GTK's direct one"
    );
    crate::common::type_with_real_keyboard(&[&[PLUS]]);
    wait_until(Duration::from_secs(5), || {
        tree.section
            .workspace_section_evidence()
            .collapses
            .is_empty()
    });
    let evidence = tree.section.workspace_section_evidence();
    assert!(
        evidence.collapses.is_empty(),
        "{}",
        collapse_state(&tree.section)
    );
    assert_eq!(evidence.last_collapse_end, Some(CollapseEnd::ReExpanded));
    wait_for_refresh_idle(&tree.section);
    crate::common::type_with_real_keyboard(&[&[CONTROL_L, SPACE]]);
    wait_until(Duration::from_secs(5), || {
        tree.section.workspace_section_evidence().collapses.len() == 1
    });
    let evidence = tree.section.workspace_section_evidence();
    assert_eq!(
        evidence.collapses.len(),
        1,
        "Ctrl+space collapses through the entry point"
    );
    assert_eq!(evidence.collapses[0].origin, CollapseOrigin::Keys);
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    crate::common::type_with_real_keyboard(&[&[PLUS]]);
    wait_until(Duration::from_secs(10), || {
        row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded())
    });
    drop(tree.window);
}

/// Design D7/D11: readiness is blocked while a collapse runs, the evidence
/// reports it, and reading the evidence mid-collapse changes nothing.
#[test]
fn test_collapse_readiness_and_evidence() {
    use lushtext_core::ui::sidebar::workspace_section::CollapseOrigin;
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    let first = tree.section.workspace_section_evidence();
    assert!(first.refresh_blocks_readiness);
    assert_eq!(first.collapses.len(), 1);
    assert_eq!(first.collapses[0].path, nested);
    assert_eq!(first.collapses[0].origin, CollapseOrigin::Activation);
    let second = tree.section.workspace_section_evidence();
    assert_eq!(first.collapses, second.collapses, "reading is inert");
    assert_eq!(first.collapse_steps_total, second.collapse_steps_total);
    assert_eq!(
        first.expansion_capture_scans,
        second.expansion_capture_scans
    );
    assert!(
        tree.sidebar
            .workspace_tree_evidence()
            .refresh_blocks_readiness
    );
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    assert!(
        tree.section
            .workspace_section_evidence()
            .collapses
            .is_empty()
    );
    drop(tree.window);
}

/// Design D7: removing the collapsing directory (deleted on disk, refreshed)
/// ends its collapse as removed and clears readiness.
#[test]
fn test_removing_a_collapsing_directory_cancels_the_collapse() {
    use lushtext_core::ui::sidebar::workspace_section::CollapseEnd;
    // Removed on disk while steps run under frame pacing: its parent's
    // targeted refresh splices its row out, which the next step's facts see
    // (finding 9). The parent is not a workspace folder, whose refresh
    // reloads the whole model (the second case).
    {
        let count = 128 * 1024;
        let (dir, root) = seed_workspace(&[("parent/nested", count)]);
        let tree = present_single_folder(dir, root, "virtualized", 800);
        tree.section.expand_folders();
        let parent = tree.root.join("parent");
        let nested = parent.join("nested");
        expand_and_load(&tree, &parent, 1);
        expand_and_load(&tree, &nested, count);
        flush_after_delay(Duration::from_millis(300));
        activate_tree_row(&tree.section, &nested);
        pump_dispatches_until(Duration::from_secs(30), || {
            tree.section
                .workspace_section_evidence()
                .collapse_steps_total
                > 0
        });
        assert!(
            !tree
                .section
                .workspace_section_evidence()
                .collapses
                .is_empty(),
            "the collapse still runs after its first step"
        );
        // Pause between two steps, so the removal lands mid-collapse.
        let pacing = Pacing::set(CollapsePacing::Hold);
        fixture::remove_dir_all(&nested);
        tree.section.queue_auto_refresh_for_test(vec![parent]);
        tree.section.apply_queued_refresh_for_test();
        wait_until(Duration::from_secs(60), || {
            row_for_path(&tree.section, &nested).is_none()
        });
        // The splice ended nothing by itself: the collapse is still queued,
        // so its end below comes from the next step's facts, which no longer
        // find the row (`step_facts`), not from a reset.
        assert_eq!(
            tree.section.workspace_section_evidence().collapses.len(),
            1,
            "{}",
            collapse_state(&tree.section)
        );
        drop(pacing);
        wait_until(Duration::from_secs(60), || {
            !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
        });
        let evidence = tree.section.workspace_section_evidence();
        assert!(evidence.collapses.is_empty());
        assert_eq!(
            evidence.last_collapse_end,
            Some(CollapseEnd::Removed),
            "{}",
            collapse_state(&tree.section)
        );
        drop(tree.window);
    }
    // The whole-model reload while the collapse is held.
    let (tree, nested) = expanded_nested(4096, 0);
    let _pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    fixture::remove_dir_all(&nested);
    tree.section
        .queue_auto_refresh_for_test(vec![tree.root.clone()]);
    tree.section.apply_queued_refresh_for_test();
    wait_until(Duration::from_secs(60), || {
        row_for_path(&tree.section, &nested).is_none()
            && !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    let evidence = tree.section.workspace_section_evidence();
    assert!(evidence.collapses.is_empty());
    assert_eq!(evidence.last_collapse_end, Some(CollapseEnd::Removed));
    drop(tree.window);
}

/// The expander's current disclosure icon (CSS name `expander`).
fn arrow_icon(expander: &gtk4::TreeExpander) -> Option<gtk4::Widget> {
    let mut child = expander.first_child();
    while let Some(widget) = child {
        if widget.css_name() == "expander" {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

/// Design D5 (ledger A119): a real click on the arrow reaches the collapse
/// entry point and GTK's own toggle does nothing, also after GTK re-created
/// the icon; a click on a collapsed arrow expands and selects the row.
#[test]
fn test_the_arrow_click_survives_an_icon_recreation() {
    use crate::common::activate_with_real_press;
    let (tree, nested) = expanded_nested(256, 0);
    let expander = expander_for(&tree.section, &nested).expect("realized");
    let icon = arrow_icon(&expander).expect("an expandable row has an arrow");
    let row = row_for_path(&tree.section, &nested).expect("row");
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_with_real_press(&tree.window, &icon);
    wait_until(Duration::from_secs(5), || {
        expander.has_css_class(COLLAPSING_CLASS)
    });
    assert!(
        row.is_expanded() && expander.has_css_class(COLLAPSING_CLASS),
        "the click started the owned collapse and GTK's toggle did not run"
    );
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    // GTK re-creates the icon on a rebind.
    let expander = expander_for(&tree.section, &nested).expect("realized");
    let row = row_for_path(&tree.section, &nested).expect("row");
    expander.set_list_row(None::<&gtk4::TreeListRow>);
    expander.set_list_row(Some(&row));
    flush_after_delay(Duration::from_millis(100));
    let icon = arrow_icon(&expander).expect("the arrow again");
    // Select another row first, so a press that moved the selection to the
    // directory row would show.
    selection(&tree.section).set_selected(0);
    flush_events();
    let selected = selection(&tree.section).selected();
    activate_with_real_press(&tree.window, &icon);
    wait_until(Duration::from_secs(30), || {
        row.is_expanded() && listing(&tree.section, &nested).directory_retained_entries == 256
    });
    // Let any late consequence of the press land before reading the
    // selection (task 9.8): the expansion above is what the click caused.
    wait_for_refresh_idle(&tree.section);
    flush_after_delay(Duration::from_millis(100));
    // GTK's own icon gesture claims the release too, so an arrow click
    // leaves the selection where it was (A119 measures both alike).
    assert_eq!(
        selection(&tree.section).selected(),
        selected,
        "the arrow click leaves the selection as GTK's toggle does"
    );
    drop(tree.window);
}

/// Design D8: the folder header collapsing a root while the sidebar is
/// scrolled into a 4,096-entry directory ends at the outer value and row
/// placement GTK's direct collapse gives a twin tree.
#[test]
fn test_collapse_from_inside_the_children_matches_a_direct_collapse() {
    let mut results = Vec::new();
    for owned in [true, false] {
        let (tree, nested) = expanded_nested(4096, 0);
        scroll_outer_to(&tree.sidebar, 40_000.0);
        flush_after_delay(Duration::from_millis(300));
        if owned {
            tree.section.collapse_folders();
        } else {
            row_for_path(&tree.section, &tree.root)
                .expect("folder row")
                .set_expanded(false);
        }
        wait_until(Duration::from_secs(60), || {
            collapse_settled(&tree.section, &tree.root)
        });
        flush_after_delay(Duration::from_millis(500));
        results.push((
            outer_adjustment(&tree.sidebar).value(),
            row_placement(&tree.section, "root"),
        ));
        let _ = nested;
        drop(tree.window);
    }
    assert_eq!(results[0], results[1], "owned vs direct collapse end state");
}

/// Design D5: the arrow gesture keeps GTK's behaviour for what it does not
/// own. A click on a collapsed directory's arrow expands and lists it and
/// leaves the selection as GTK's own toggle does; a double click on a file row still activates the file (the expander's
/// own gesture no longer runs for file rows on GTK 4.24.1, ledger A119); and
/// a click on the arrow activates nothing.
#[test]
fn test_expand_routes_keep_gtk_behaviour() {
    use crate::common::press_with_real_pointer;
    use std::cell::RefCell;
    use std::rc::Rc;
    let (dir, root) = seed_workspace(&[("nested", 64)]);
    fixture::write_text(&root.join("zz-file.txt"), "x");
    let tree = present_single_folder(dir, root, "virtualized", 800);
    tree.section.expand_folders();
    let nested = tree.root.join("nested");
    let file = tree.root.join("zz-file.txt");
    wait_until(Duration::from_secs(10), || {
        row_for_path(&tree.section, &file).is_some()
    });
    wait_for_refresh_idle(&tree.section);
    let activated: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let recorder = Rc::clone(&activated);
    tree.section
        .connect_file_activated(move |path| recorder.borrow_mut().push(path.to_path_buf()));
    let expander = expander_for(&tree.section, &nested).expect("realized");
    let icon = arrow_icon(&expander).expect("a directory row has an arrow");
    selection(&tree.section).set_selected(0);
    flush_events();
    let selected = selection(&tree.section).selected();
    press_with_real_pointer(&tree.window, &icon, 1);
    wait_until(Duration::from_secs(30), || {
        row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded())
            && listing(&tree.section, &nested).directory_retained_entries == 64
    });
    assert_eq!(
        selection(&tree.section).selected(),
        selected,
        "the arrow click leaves the selection as GTK's toggle does (A119)"
    );
    flush_after_delay(Duration::from_millis(500));
    assert!(
        activated.borrow().is_empty(),
        "an arrow click activates nothing"
    );
    select_and_scroll_to(&tree.section, &file);
    flush_after_delay(Duration::from_millis(300));
    let file_expander = expander_for(&tree.section, &file).expect("the file row is realized");
    press_with_real_pointer(&tree.window, &file_expander, 2);
    wait_until(Duration::from_secs(10), || !activated.borrow().is_empty());
    assert_eq!(activated.borrow().as_slice(), std::slice::from_ref(&file));
    drop(tree.window);
}

/// Every dispatch the main loop ran while a collapse or an expansion
/// settled, for `test_directory_collapse_and_small_expansion_dispatches_fit_the_budgets`.
#[derive(Debug, Default)]
struct DispatchTimeline {
    /// Dispatches run, in order.
    dispatches: Vec<TimedDispatch>,
    /// Microseconds from the timeline's start until `done` first held.
    done_us: Option<u64>,
    /// Microseconds from the timeline's start to the end of the first frame
    /// dispatch that ran once the row was GTK-collapsed: the collapse's
    /// completion (design R3; the input is added by the caller).
    shown_us: Option<u64>,
    /// `glib::monotonic_time()` (CLOCK_MONOTONIC, µs) at the timeline's start,
    /// so `perf record -k CLOCK_MONOTONIC` samples can be cut to a case's
    /// dispatches.
    mono_start_us: i64,
    /// Scan terminals published from the start until the completing frame,
    /// counted once there rather than attributed per dispatch.
    terminals_before_shown: Option<u64>,
    /// Microseconds from the timeline's start to the end of the first frame
    /// dispatch from which every later frame's picture, until the timeline
    /// ended, equals the last one (task 10.16): when the user last saw the
    /// rows move.
    picture_final_us: Option<u64>,
}

/// The rows the viewport draws after a frame (task 10.16): each realized
/// row widget of the list with opacity above 0 (D22 hides by opacity) that
/// intersects the sidebar's viewport, as its path and rendered top. It
/// reads the list view's realized children only, never the evidence, so its
/// cost is bounded by the realized band, not the listing.
fn drawn_rows(list: &gtk4::ListView) -> Vec<(Option<PathBuf>, i32)> {
    let Some(viewport) = list.ancestor(gtk4::ScrolledWindow::static_type()) else {
        return Vec::new();
    };
    let height = viewport.height();
    let mut rows = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.opacity() <= 0.0 || !widget.is_mapped() {
            continue;
        }
        let Some(bounds) = widget.compute_bounds(&viewport) else {
            continue;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a widget coordinate in the viewport, whole pixels"
        )]
        let (top, bottom) = (
            bounds.y().round() as i32,
            (bounds.y() + bounds.height()).round() as i32,
        );
        if bottom <= 0 || top >= height {
            continue;
        }
        let path = find_descendant(&widget, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
            .and_downcast::<gtk4::TreeExpander>()
            .and_then(|expander| expander.item())
            .and_downcast::<FileTreeItem>()
            .and_then(|item| item.path());
        rows.push((path, top));
    }
    rows.sort_by_key(|(_, top)| *top);
    rows
}

/// One main-loop dispatch of a [`DispatchTimeline`].
#[derive(Debug, Clone, Copy)]
struct TimedDispatch {
    /// Microseconds from the timeline's start.
    start_us: u64,
    /// The dispatch's own time.
    took_us: u64,
    /// A frame's layout phase, `None` for a dispatch that drew no frame.
    layout_us: Option<u64>,
    /// A frame's paint phase, `None` for a dispatch that drew no frame.
    paint_us: Option<u64>,
    /// Whether the section published a scan terminal in it (design D12's
    /// probe publication), and if so whether its store was the one the
    /// directory's row shows (`Some(true)`) or one no row shows, such as the
    /// `is_expandable()` probe's (`Some(false)`, task 1.4).
    published_terminal: Option<bool>,
}

impl DispatchTimeline {
    fn worst(&self, frames: bool) -> u64 {
        self.dispatches
            .iter()
            .filter(|dispatch| dispatch.layout_us.is_some() == frames)
            .map(|dispatch| dispatch.took_us)
            .max()
            .unwrap_or(0)
    }

    fn worst_layout(&self) -> u64 {
        self.dispatches
            .iter()
            .filter_map(|dispatch| dispatch.layout_us)
            .max()
            .unwrap_or(0)
    }

    fn worst_paint(&self) -> u64 {
        self.dispatches
            .iter()
            .filter_map(|dispatch| dispatch.paint_us)
            .max()
            .unwrap_or(0)
    }

    /// Dispatches over `limit` µs, as `[start, took, frame, terminal]`, the
    /// terminal `null`, `"shown"` or `"unshown"`.
    fn over(&self, limit: u64) -> String {
        let over: Vec<String> = self
            .dispatches
            .iter()
            .filter(|dispatch| dispatch.took_us > limit)
            .map(|dispatch| {
                format!(
                    "[{},{},{},{}]",
                    dispatch.start_us,
                    dispatch.took_us,
                    dispatch.layout_us.is_some(),
                    match dispatch.published_terminal {
                        None => "null",
                        Some(true) => "\"shown\"",
                        Some(false) => "\"unshown\"",
                    }
                )
            })
            .collect();
        format!("[{}]", over.join(","))
    }
}

/// Run the main loop one dispatch at a time until `done` holds and then for
/// `after` more, recording every dispatch, its frame phases, and whether the
/// section published a scan terminal in it.
fn dispatch_timeline(
    section: &LushtextWorkspaceSection,
    budget: Duration,
    after: Duration,
    done: impl FnMut() -> bool,
) -> DispatchTimeline {
    dispatch_timeline_with(section, budget, after, done, None::<fn() -> bool>)
}

/// [`dispatch_timeline`] that also records when the first frame dispatch
/// after which `shown_probe` holds ended (design R3's completion).
///
/// Until then it reads only what `shown_probe` reads (the directory row's
/// expansion), never the evidence surface, whose read after every dispatch
/// was about 5.7 % of a 131,072-row collapse's window (design R9, task
/// 10.1): scan terminals are attributed per dispatch from the completing
/// frame on, and the ones published before it are counted once
/// (`terminals_before_shown`). The `done` predicate the callers pass reads
/// the row first and the evidence only once the row is collapsed.
fn dispatch_timeline_shown(
    section: &LushtextWorkspaceSection,
    budget: Duration,
    after: Duration,
    done: impl FnMut() -> bool,
    shown_probe: impl FnMut() -> bool,
) -> DispatchTimeline {
    dispatch_timeline_with(section, budget, after, done, Some(shown_probe))
}

fn dispatch_timeline_with(
    section: &LushtextWorkspaceSection,
    budget: Duration,
    after: Duration,
    mut done: impl FnMut() -> bool,
    mut shown_probe: Option<impl FnMut() -> bool>,
) -> DispatchTimeline {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Instant;
    let clock = section
        .imp()
        .file_tree_view
        .frame_clock()
        .expect("a realized list has a frame clock");
    let layout_end: Rc<Cell<Option<Instant>>> = Rc::default();
    let paint_end: Rc<Cell<Option<Instant>>> = Rc::default();
    let layout_handler = clock.connect_layout({
        let layout_end = Rc::clone(&layout_end);
        move |_| layout_end.set(Some(Instant::now()))
    });
    let paint_handler = clock.connect_paint({
        let paint_end = Rc::clone(&paint_end);
        move |_| paint_end.set(Some(Instant::now()))
    });
    let context = glib::MainContext::default();
    let mono_start_us = glib::monotonic_time();
    let started = Instant::now();
    let micros = |duration: Duration| u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
    let mut timeline = DispatchTimeline {
        mono_start_us,
        ..DispatchTimeline::default()
    };
    let mut done_at: Option<Instant> = None;
    let terminals_at_start = section
        .workspace_section_evidence()
        .scan_pressure
        .terminal_publications;
    let list = section.imp().file_tree_view.clone();
    let mut last_picture: Option<Vec<(Option<PathBuf>, i32)>> = None;
    while started.elapsed() < budget {
        let track_terminals = shown_probe.is_none() || timeline.shown_us.is_some();
        if done_at.is_none() && done() {
            done_at = Some(Instant::now());
            timeline.done_us = Some(micros(started.elapsed()));
        }
        if done_at.is_some_and(|at| at.elapsed() >= after) {
            break;
        }
        let terminals = track_terminals.then(|| {
            section
                .workspace_section_evidence()
                .scan_pressure
                .terminal_publications
        });
        layout_end.set(None);
        paint_end.set(None);
        let start = Instant::now();
        if context.iteration(false) {
            let took = start.elapsed();
            let (layout, paint) = match (layout_end.get(), paint_end.get()) {
                (Some(layout), Some(paint)) => (
                    Some(micros(layout.saturating_duration_since(start))),
                    Some(micros(paint.saturating_duration_since(layout))),
                ),
                (None, Some(paint)) => (
                    Some(0),
                    Some(micros(paint.saturating_duration_since(start))),
                ),
                (Some(layout), None) => (
                    Some(micros(layout.saturating_duration_since(start))),
                    Some(0),
                ),
                (None, None) => (None, None),
            };
            let advanced = terminals.and_then(|terminals| {
                let evidence = section.workspace_section_evidence();
                (evidence.scan_pressure.terminal_publications > terminals).then(|| {
                    evidence
                        .last_scan_terminal
                        .as_ref()
                        .is_some_and(|(_, shown)| *shown)
                })
            });
            // The frame that drew the row collapsed: a frame dispatch after
            // which the row is GTK-collapsed (the final step runs in the
            // frame's update phase, a direct collapse before the frame).
            if timeline.shown_us.is_none()
                && layout.is_some()
                && shown_probe.as_mut().is_some_and(|probe| probe())
            {
                timeline.shown_us = Some(micros(started.elapsed()));
                timeline.terminals_before_shown = Some(
                    section
                        .workspace_section_evidence()
                        .scan_pressure
                        .terminal_publications
                        .saturating_sub(terminals_at_start),
                );
            }
            if layout.is_some() {
                let picture = drawn_rows(&list);
                if last_picture.as_ref() != Some(&picture) {
                    timeline.picture_final_us = Some(micros(started.elapsed()));
                    last_picture = Some(picture);
                }
            }
            timeline.dispatches.push(TimedDispatch {
                start_us: micros(start.saturating_duration_since(started)),
                took_us: micros(took),
                layout_us: layout,
                paint_us: paint,
                published_terminal: advanced,
            });
        } else {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
    clock.disconnect(layout_handler);
    clock.disconnect(paint_handler);
    timeline
}

/// Activate the row at `path` the way Enter or a double click does (the
/// list's `activate`), returning the call's time in µs.
fn activate_row(section: &LushtextWorkspaceSection, path: &Path) -> u64 {
    let position = tree_index_for_path(section, path).expect("the row is listed");
    let started = std::time::Instant::now();
    section
        .imp()
        .file_tree_view
        .emit_by_name::<()>("activate", &[&position]);
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Whether `path`'s row is listed and collapsed and the section is settled.
fn collapsed_and_settled(section: &LushtextWorkspaceSection, path: &Path) -> bool {
    row_for_path(section, path).is_some_and(|row| !row.is_expanded())
        && !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
}

/// Whether `path` lists `count` entries and the section is settled.
fn loaded_and_settled(section: &LushtextWorkspaceSection, path: &Path, count: usize) -> bool {
    section
        .workspace_section_evidence()
        .directory_listings
        .iter()
        .any(|listing| listing.path == path && listing.directory_retained_entries == count)
        && !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
}

// ---------------------------------------------------------------------------
// The design revision's findings (groups 9 and 10 of the change's tasks).
// ---------------------------------------------------------------------------

/// Open the file-row context menu on `path`'s row from the keyboard, so the
/// section's actions target that row and its realized expander.
fn open_row_context_menu(section: &LushtextWorkspaceSection, path: &Path) {
    let index = tree_index_for_path(section, path).expect("the row is listed");
    section
        .imp()
        .file_tree_view
        .scroll_to(index, gtk4::ListScrollFlags::NONE, None);
    wait_until(Duration::from_secs(5), || {
        expander_for(section, path).is_some_and(|expander| expander.height() > 0)
    });
    selection(section).set_selected(index);
    section.imp().file_tree_view.grab_focus();
    flush_events();
    let controllers = section.imp().file_tree_view.observe_controllers();
    let mut opened = false;
    for position in 0..controllers.n_items() {
        if let Some(controller) = controllers
            .item(position)
            .and_downcast::<gtk4::EventControllerKey>()
        {
            let args: [&dyn ToValue; 3] = [
                &gtk4::gdk::Key::Menu,
                &0u32,
                &gtk4::gdk::ModifierType::empty(),
            ];
            if glib::object::ObjectExt::emit_by_name::<bool>(&controller, "key-pressed", &args) {
                opened = true;
                break;
            }
        }
    }
    assert!(opened, "the Menu key opens the row's context menu");
    wait_until(Duration::from_secs(5), || {
        section
            .imp()
            .context_menu
            .borrow()
            .as_ref()
            .is_some_and(gtk4::prelude::WidgetExt::is_visible)
    });
}

/// The inline rename entry in `path`'s realized row, if any.
fn rename_entry_for(section: &LushtextWorkspaceSection, path: &Path) -> Option<gtk4::Entry> {
    let expander = expander_for(section, path)?;
    find_descendant(
        expander.upcast_ref(),
        glib::object::ObjectExt::is::<gtk4::Entry>,
    )
    .and_downcast::<gtk4::Entry>()
}

/// Every realized row expander of the section.
fn realized_expanders(section: &LushtextWorkspaceSection) -> Vec<gtk4::TreeExpander> {
    let mut expanders = Vec::new();
    let mut child = section.imp().file_tree_view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(expander) =
            find_descendant(&widget, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
                .and_downcast::<gtk4::TreeExpander>()
        {
            expanders.push(expander);
        }
    }
    expanders
}

/// The path a realized expander shows.
fn expander_path(expander: &gtk4::TreeExpander) -> Option<PathBuf> {
    expander
        .list_row()
        .and_then(|row| row.item())
        .and_downcast::<FileTreeItem>()
        .and_then(|item| item.path())
}

/// Design R6.1 (finding 1): a key typed into an inline rename entry on an
/// expanded directory reaches the entry, never the collapse key route. Fails
/// before the fix: `-` collapsed the directory and `+` expanded it.
#[test]
fn test_typing_into_inline_rename_keeps_every_character() {
    const MINUS: u32 = 0x2d;
    const PLUS: u32 = 0x2b;
    const ASTERISK: u32 = 0x2a;
    const SLASH: u32 = 0x2f;
    const KP_SUBTRACT: u32 = 0xffad;
    const SHIFT_L: u32 = 0xffe1;
    const LEFT: u32 = 0xff51;
    const ESCAPE: u32 = 0xff1b;
    let (tree, nested) = expanded_nested(64, 0);
    let entry = start_inline_rename(&tree.section, &nested);
    entry.set_text("");
    flush_events();
    let before = tree.section.workspace_section_evidence();
    crate::common::type_with_real_keyboard(&[
        &[MINUS],
        &[PLUS],
        &[ASTERISK],
        &[SLASH],
        &[KP_SUBTRACT],
        &[SHIFT_L, LEFT],
    ]);
    wait_until(Duration::from_secs(10), || entry.text() == "-+*/-");
    let (start, end) = entry.selection_bounds().expect("Shift+Left selected");
    assert_eq!((start, end), (4, 5), "Shift+Left reached the entry");
    let after = tree.section.workspace_section_evidence();
    assert!(
        row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded()),
        "the directory stays expanded"
    );
    assert!(
        after.collapses.is_empty(),
        "{}",
        collapse_state(&tree.section)
    );
    assert_eq!(after.collapse_direct_total, before.collapse_direct_total);
    assert_eq!(after.collapse_steps_total, before.collapse_steps_total);
    crate::common::type_with_real_keyboard(&[&[ESCAPE]]);
    wait_until(Duration::from_secs(5), || {
        rename_entry_for(&tree.section, &nested).is_none()
    });
    assert!(
        fixture::exists(&nested),
        "the cancelled rename kept the name"
    );
    drop(tree.window);
}

/// Start inline rename on `path`'s row and wait until its entry holds the
/// focus. Returns the entry.
fn start_inline_rename(section: &LushtextWorkspaceSection, path: &Path) -> gtk4::Entry {
    open_row_context_menu(section, path);
    section
        .activate_action("section.rename", None)
        .expect("the rename action");
    wait_until(Duration::from_secs(5), || {
        rename_entry_for(section, path).is_some_and(|entry| {
            section
                .root()
                .and_downcast::<gtk4::Window>()
                .and_then(|window| gtk4::prelude::GtkWindowExt::focus(&window))
                .is_some_and(|focus| focus.is_ancestor(&entry) || focus == entry)
        })
    });
    let entry = rename_entry_for(section, path).expect("the rename entry");
    // A presented test window is not active, and an inactive window's focus
    // moves deliver no crossing the rename's focus controller acts on; on a
    // desktop the window is active. Make it active with a real press inside
    // the entry, which keeps the focus there.
    let window = section
        .root()
        .and_downcast::<gtk4::Window>()
        .expect("the section's window");
    if !window.is_active() {
        crate::common::activate_with_real_press(&window, &entry);
    }
    wait_until(Duration::from_secs(5), || {
        gtk4::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| focus.is_ancestor(&entry) || focus == entry)
    });
    entry
}

/// Task 9.12: moving the focus out of an inline-rename entry ends the rename
/// without a toolkit CRITICAL. The entry's focus-leave handler used to remove
/// the entry from its row inside GTK's focus crossing, so the crossing's walk
/// from the old focus to the common ancestor (`gtkwindow.c`,
/// `synthesize_focus_change_events`: `for (w = old_focus; w != ancestor;
/// w = gtk_widget_get_parent (w))`) never met the ancestor and spun on
/// `gtk_widget_get_parent: assertion 'GTK_IS_WIDGET (widget)' failed`
/// forever. Three cases: the list's own `scroll_to(FOCUS)` onto another row
/// and onto the renamed row (the common ancestor is then the new focus
/// itself), and a button outside the list.
#[test]
fn test_moving_the_focus_out_of_inline_rename_ends_it_cleanly() {
    let (tree, nested) = expanded_nested(64, 0);
    let capture = crate::common::LogCapture::start();
    for case in ["another row", "the renamed row", "a header button"] {
        let entry = start_inline_rename(&tree.section, &nested);
        let list = &tree.section.imp().file_tree_view;
        match case {
            "another row" => {
                let root = tree_index_for_path(&tree.section, &tree.root).expect("the root row");
                list.scroll_to(root, gtk4::ListScrollFlags::FOCUS, None);
            }
            "the renamed row" => {
                let row = tree_index_for_path(&tree.section, &nested).expect("the renamed row");
                list.scroll_to(row, gtk4::ListScrollFlags::FOCUS, None);
            }
            _ => {
                assert!(tree.section.imp().refresh_button.grab_focus());
            }
        }
        wait_until(Duration::from_secs(5), || entry.parent().is_none());
        flush_events();
        assert!(
            rename_entry_for(&tree.section, &nested).is_none(),
            "{case}: the rename ended"
        );
        assert!(fixture::exists(&nested), "{case}: the name was kept");
    }
    let criticals: Vec<_> = capture
        .entries()
        .into_iter()
        .filter(|entry| entry.critical)
        .collect();
    assert!(criticals.is_empty(), "{criticals:?}");
    drop(tree.window);
}

/// Task 9.12: a background refresh of the directory being renamed restores
/// the selection without taking the focus from the inline-rename entry, so
/// the rename survives it (on `main` the restore's `scroll_to(FOCUS)` moved
/// the focus to the row, which ended the rename inside the focus crossing:
/// the flood of the test above).
#[test]
fn test_a_background_refresh_keeps_the_inline_rename() {
    let (tree, nested) = expanded_nested(64, 0);
    let capture = crate::common::LogCapture::start();
    let entry = start_inline_rename(&tree.section, &nested);
    let selected_before = selection(&tree.section).selected();
    let scans_before = tree
        .section
        .workspace_section_evidence()
        .scan_pressure
        .terminal_publications;
    tree.section.queue_auto_refresh_for_test(vec![nested]);
    tree.section.apply_queued_refresh_for_test();
    wait_until(Duration::from_secs(10), || {
        let evidence = tree.section.workspace_section_evidence();
        evidence.scan_pressure.terminal_publications > scans_before
            && !evidence.refresh_blocks_readiness
    });
    // The restore runs from a 1 ms timeout after the scan terminal.
    flush_after_delay(Duration::from_millis(50));
    assert!(entry.parent().is_some(), "the rename entry stays");
    assert!(
        gtk4::prelude::GtkWindowExt::focus(&tree.window)
            .is_some_and(|focus| focus.is_ancestor(&entry) || focus == entry),
        "the rename entry keeps the focus"
    );
    assert_eq!(selection(&tree.section).selected(), selected_before);
    let criticals: Vec<_> = capture
        .entries()
        .into_iter()
        .filter(|entry| entry.critical)
        .collect();
    assert!(criticals.is_empty(), "{criticals:?}");
    drop(tree.window);
}

/// A background refresh that finds the selection where it left it leaves the
/// keyboard focus alone: a row the user focused off the selection (Ctrl+arrow
/// moves the focus without selecting) keeps it. The restore's
/// `scroll_to(FOCUS)` moved the focus back to the selected row on every
/// refresh, which `test_replayed_navigation_matches_gtk` met as a flake when
/// the attention refresh of a window re-activation ran while it typed
/// (design R13, "The focus a background refresh took").
#[test]
fn test_a_background_refresh_keeps_the_focus_off_the_selection() {
    let (tree, nested) = expanded_nested(64, 0);
    tree.section.stop_workspace_watch_for_test();
    selection(&tree.section).set_selected(0);
    let focused = nested.join(file_name(2));
    let focused_index = tree_index_for_path(&tree.section, &focused).expect("row index");
    let row = rendered_row_widget(&tree.section, &file_name(2)).expect("a realized row");
    assert!(row.grab_focus(), "the row takes the focus");
    flush_after_delay(Duration::from_millis(50));
    assert_eq!(
        window_focused_row(&tree.window, &tree.section),
        Some(focused_index),
        "the focus is on the row, off the selection"
    );
    let scans_before = tree
        .section
        .workspace_section_evidence()
        .scan_pressure
        .terminal_publications;
    tree.section.queue_auto_refresh_for_test(vec![nested]);
    tree.section.apply_queued_refresh_for_test();
    wait_until(Duration::from_secs(10), || {
        let evidence = tree.section.workspace_section_evidence();
        evidence.scan_pressure.terminal_publications > scans_before
            && !evidence.refresh_blocks_readiness
    });
    // The restore runs from a 1 ms timeout after the scan terminal.
    flush_after_delay(Duration::from_millis(50));
    assert_eq!(
        selection(&tree.section).selected(),
        0,
        "the selection stays"
    );
    assert_eq!(
        window_focused_row(&tree.window, &tree.section),
        Some(focused_index),
        "the refresh left the focus on the row the user focused"
    );
    drop(tree.window);
}

/// Design R6.2 (finding 2): a mapped list whose frame clock delivers no tick
/// still completes a sliced collapse through the starvation fallback, which
/// counts its steps; once ticks resume, no dispatch changes more than one
/// unit of row widgets (the tick after a starved unit takes none).
#[test]
fn test_a_mapped_list_without_frames_still_completes_the_collapse() {
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let pacing = Pacing::set(CollapsePacing::MappedNoTicks);
    activate_tree_row(&tree.section, &nested);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    let evidence = tree.section.workspace_section_evidence();
    assert!(
        evidence.collapse_starved_steps_total > 0,
        "{}",
        collapse_state(&tree.section)
    );
    assert!(!evidence.refresh_blocks_readiness);
    // Again, releasing the pacing mid-collapse.
    activate_tree_row(&tree.section, &nested);
    wait_until(Duration::from_secs(30), || {
        listing(&tree.section, &nested).directory_retained_entries == 4096
            && !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    let starved = tree
        .section
        .workspace_section_evidence()
        .collapse_starved_steps_total;
    activate_tree_row(&tree.section, &nested);
    wait_until(Duration::from_secs(30), || {
        tree.section
            .workspace_section_evidence()
            .collapse_starved_steps_total
            > starved
    });
    let created = row_widgets_changed_per_dispatch(
        &tree.section,
        || drop(pacing),
        || collapse_settled(&tree.section, &nested),
    );
    let over = RowChange::over_unit(&created);
    assert!(
        over.is_none(),
        "a resumed tick changed {over:?}: {created:?}"
    );
    drop(tree.window);
}

/// Design R6.3 (finding 3): a window closed mid-collapse leaves no token
/// that holds back the next window's collapse; its first frames take units.
/// The pure helper's unit test (`the_frame_unit_token_frees_one_unit_per_frame`)
/// is the failing-first half; this guards the weak keying.
#[test]
fn test_a_closed_window_leaves_the_next_window_its_units() {
    let (first, nested) = expanded_nested(4096, PACED_SIBLINGS);
    activate_tree_row(&first.section, &nested);
    wait_until(Duration::from_secs(30), || {
        first
            .section
            .workspace_section_evidence()
            .collapse_steps_total
            > 4
    });
    first.window.close();
    drop(first);
    flush_after_delay(Duration::from_millis(200));
    let (second, nested) = expanded_nested(256, PACED_SIBLINGS);
    let steps = second
        .section
        .workspace_section_evidence()
        .collapse_steps_total;
    activate_tree_row(&second.section, &nested);
    // About eight units at one per frame: well inside two seconds unless a
    // stale token holds them back.
    wait_until(Duration::from_secs(2), || {
        collapse_settled(&second.section, &nested)
    });
    assert!(
        second
            .section
            .workspace_section_evidence()
            .collapse_steps_total
            > steps
    );
    drop(second.window);
}

/// `outer/` with `inner/` (`inner` entries) and `files` files, both expanded
/// and listed.
fn outer_with_inner(inner: usize, files: usize) -> (LargeTree, PathBuf, PathBuf) {
    let (dir, root) = seed_workspace(&[("outer/inner", inner)]);
    for index in 0..files {
        fixture::write_text(&root.join("outer").join(format!("o-{index:03}.txt")), "x");
    }
    let tree = present_single_folder(dir, root, "virtualized", 800);
    tree.section.expand_folders();
    let outer = tree.root.join("outer");
    let inner_path = outer.join("inner");
    expand_and_load(&tree, &outer, files + 1);
    expand_and_load(&tree, &inner_path, inner);
    flush_after_delay(Duration::from_millis(300));
    (tree, outer, inner_path)
}

/// Design R6.4 (finding 4): `inner/` collapsing, taken over by `outer/`'s
/// collapse, and `outer/` expanded again: `inner/` ends collapsed, its
/// expander loses the class and reads collapsed, and no realized row left
/// the collapsing set with the class.
#[test]
fn test_a_taken_over_directory_ends_collapsed_after_its_ancestor_reexpands() {
    use lushtext_core::ui::sidebar::workspace_section::CollapseEnd;
    // `outer/`'s files follow `inner/`'s subtree, so with more than
    // `PACED_SIBLINGS` of them C3 would collapse `inner/` directly and
    // nothing would be taken over.
    let (tree, outer, inner) = outer_with_inner(16 * 1024 + 1, PACED_SIBLINGS);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &inner);
    activate_tree_row(&tree.section, &outer);
    assert_eq!(
        tree.section.workspace_section_evidence().last_collapse_end,
        Some(CollapseEnd::TakenOver)
    );
    activate_tree_row(&tree.section, &outer);
    drop(pacing);
    wait_until(Duration::from_secs(120), || {
        let evidence = tree.section.workspace_section_evidence();
        evidence.collapses.is_empty()
            && !evidence.refresh_blocks_readiness
            && row_for_path(&tree.section, &inner).is_some_and(|row| !row.is_expanded())
    });
    assert!(
        row_for_path(&tree.section, &outer).is_some_and(|row| row.is_expanded()),
        "{}",
        collapse_state(&tree.section)
    );
    let evidence = tree.section.workspace_section_evidence();
    assert!(!evidence.expanded_paths.contains(&inner));
    let expander = expander_for(&tree.section, &inner).expect("inner/ is realized");
    assert!(!expander.has_css_class(COLLAPSING_CLASS));
    assert!(crate::common::accessible_expanded_is(&expander, false));
    for expander in realized_expanders(&tree.section) {
        assert!(
            !expander.has_css_class(COLLAPSING_CLASS),
            "{:?} kept the class after every collapse ended",
            expander_path(&expander)
        );
    }
    drop(tree.window);
}

/// Design R6.5 (finding 5): an expanded descendant in the band of a
/// collapsing directory reads collapsed from the input dispatch, and its
/// arrow, Enter and the expand key do nothing and leave the ancestor's
/// collapse running.
#[test]
fn test_rows_inside_a_collapsing_subtree_show_collapsed_and_ignore_disclosure() {
    use crate::common::activate_with_real_press;
    const PLUS: u32 = 0x2b;
    let (tree, outer, inner) = outer_with_inner(64, 100);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &outer);
    let expander = expander_for(&tree.section, &inner).expect("inner/ is realized");
    assert!(expander.has_css_class(COLLAPSING_CLASS));
    assert!(crate::common::accessible_expanded_is(&expander, false));
    let overlay = expander.parent().expect("the row overlay");
    assert!(crate::common::accessible_expanded_is(&overlay, false));
    let only_outer_collapses = |section: &LushtextWorkspaceSection| {
        let evidence = section.workspace_section_evidence();
        evidence.collapses.len() == 1 && evidence.collapses[0].path == outer
    };
    let icon = arrow_icon(&expander).expect("a directory row has an arrow");
    activate_with_real_press(&tree.window, &icon);
    flush_after_delay(Duration::from_millis(100));
    assert!(
        only_outer_collapses(&tree.section),
        "{}",
        collapse_state(&tree.section)
    );
    activate_tree_row(&tree.section, &inner);
    flush_events();
    assert!(
        only_outer_collapses(&tree.section),
        "{}",
        collapse_state(&tree.section)
    );
    expander.grab_focus();
    flush_events();
    crate::common::type_with_real_keyboard(&[&[PLUS]]);
    flush_after_delay(Duration::from_millis(100));
    assert!(
        only_outer_collapses(&tree.section),
        "{}",
        collapse_state(&tree.section)
    );
    let expander = expander_for(&tree.section, &inner).expect("inner/ is realized");
    assert!(expander.has_css_class(COLLAPSING_CLASS));
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &outer)
    });
    drop(tree.window);
}

/// Design R6.6 (finding 6): New File from a file row inside a collapsing
/// directory expands the directory through the expand entry point, lists
/// the new item in the refilled listing and starts its inline rename; a
/// cancel leaves no temporary file.
#[test]
fn test_new_file_inside_a_collapsing_subtree_expands_its_target_first() {
    use lushtext_core::ui::sidebar::workspace_section::CollapseEnd;
    // Above `DIRECT_COLLAPSE_ROWS`, so the collapse is sliced, and within the
    // realized band, so the appended item is bound and starts its rename.
    let (tree, nested) = expanded_nested(40, 0);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    let child = nested.join(file_name(3));
    tree.section
        .set_context_target_for_test(&child, false, None);
    tree.section
        .activate_action("section.new-file", None)
        .expect("the new-file action");
    let created = nested.join("New File");
    wait_until(Duration::from_secs(30), || {
        rename_entry_for(&tree.section, &created).is_some()
    });
    let evidence = tree.section.workspace_section_evidence();
    assert!(
        evidence.collapses.is_empty(),
        "{}",
        collapse_state(&tree.section)
    );
    assert_eq!(evidence.last_collapse_end, Some(CollapseEnd::ReExpanded));
    assert!(row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded()));
    assert!(tree.section.imp().is_new_item.get());
    let entry = rename_entry_for(&tree.section, &created).expect("the entry");
    // Committing the unchanged name is the production cancel of a new item.
    entry.emit_by_name::<()>("activate", &[]);
    wait_until(Duration::from_secs(10), || {
        !fixture::exists(&created) && !tree.section.imp().is_new_item.get()
    });
    assert_eq!(fixture::entry_names(&nested).len(), 40);
    drop(pacing);
    drop(tree.window);
}

/// The section's state a collapse leaves, relative to `root`: the listed
/// rows, the expanded paths and the watch targets.
fn collapsed_state(section: &LushtextWorkspaceSection, root: &Path) -> String {
    let evidence = section.workspace_section_evidence();
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
    };
    let mut expanded: Vec<PathBuf> = evidence
        .expanded_paths
        .iter()
        .map(|path| relative(path))
        .collect();
    expanded.sort();
    let mut watched: Vec<String> = evidence
        .watch_targets
        .iter()
        .map(|target| format!("{target:?}").replace(&root.display().to_string(), "<root>"))
        .collect();
    watched.sort();
    let (listed, roots) = top_level_rows(section);
    format!(
        "listed={listed} roots={} expanded={expanded:?} watched={watched:?}",
        roots.len()
    )
}

/// Design R6.7 (finding 7): GTK's own expansion (`listitem.expand`) of a
/// collapsed descendant inside a collapsing subtree is undone, and the
/// collapse ends where a direct collapse from the same state does.
#[test]
fn test_gtk_expanding_inside_a_collapsing_subtree_is_undone() {
    let mut states = Vec::new();
    for gtk_expands in [true, false] {
        let (tree, outer, inner) = outer_with_inner(64, 100);
        // `inner/` starts collapsed, as GTK's expansion needs.
        activate_tree_row(&tree.section, &inner);
        wait_until(Duration::from_secs(30), || {
            collapse_settled(&tree.section, &inner)
        });
        let pacing = Pacing::set(CollapsePacing::Hold);
        activate_tree_row(&tree.section, &outer);
        if gtk_expands {
            let expander = expander_for(&tree.section, &inner).expect("inner/ is realized");
            expander
                .activate_action("listitem.expand", None)
                .expect("GTK's expand action");
            flush_after_delay(Duration::from_millis(200));
            // Undone while the ancestor's collapse is still held.
            assert!(
                row_for_path(&tree.section, &inner).is_some_and(|row| !row.is_expanded()),
                "{}",
                collapse_state(&tree.section)
            );
            assert!(
                !tree
                    .section
                    .workspace_section_evidence()
                    .expanded_paths
                    .contains(&inner)
            );
        }
        drop(pacing);
        wait_until(Duration::from_secs(60), || {
            collapse_settled(&tree.section, &outer)
        });
        flush_after_delay(Duration::from_millis(500));
        states.push(collapsed_state(&tree.section, &tree.root));
        drop(tree.window);
    }
    assert_eq!(states[0], states[1]);
}

/// Design R6.10 (finding 10): with an expanded directory of the collapsing
/// folder shown twice by overlapping workspace folders, a collapse whose
/// subtree cannot be located takes GTK's direct path and leaves the selection
/// on a file outside it.
#[test]
fn test_collapse_with_an_unlocated_subtree_leaves_the_selection() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("overlapping folders tempdir");
    let root = dir.path().join("root");
    let shared = root.join("shared");
    seed_files(&shared.join("inner"), 8);
    for index in 0..64 {
        fixture::write_text(&shared.join(file_name(index)), "x");
    }
    save_workspaces(vec![WorkspaceConfig::with_folders(
        WorkspaceId::new("overlap"),
        "overlap",
        vec![
            WorkspaceFolder::new(root.clone()),
            WorkspaceFolder::new(shared.clone()),
        ],
    )]);
    let (window, sidebar) = present_sidebar_window(800);
    let section = mapped_sections(&sidebar, 1).remove(0);
    section.expand_folders();
    let row_at_depth = |path: &Path, depth: u32| {
        let model = section.imp().tree_model.borrow().as_ref().cloned()?;
        (0..model.n_items())
            .filter_map(|index| model.item(index).and_downcast::<gtk4::TreeListRow>())
            .find(|row| {
                row.depth() == depth
                    && row
                        .item()
                        .and_downcast::<FileTreeItem>()
                        .and_then(|item| item.path())
                        .as_deref()
                        == Some(path)
            })
    };
    wait_until(Duration::from_secs(30), || {
        row_at_depth(&shared, 1).is_some()
    });
    wait_for_refresh_idle(&section);
    row_at_depth(&shared, 1)
        .expect("shared/ under root/")
        .set_expanded(true);
    let inner = shared.join("inner");
    wait_until(Duration::from_secs(30), || {
        row_at_depth(&inner, 2).is_some()
    });
    row_at_depth(&inner, 2)
        .expect("inner/ under root/shared/")
        .set_expanded(true);
    let file = shared.join(file_name(10));
    wait_until(Duration::from_secs(30), || {
        row_at_depth(&inner.join(file_name(0)), 3).is_some()
            && row_at_depth(&file, 1).is_some()
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    let selected = row_at_depth(&file, 1).expect("the file under the second folder");
    selection(&section).set_selected(selected.position());
    flush_events();
    // `shared/` is listed twice (under `root/` and as the second folder), so
    // it has no unique location, and `root/`'s subtree, which holds it
    // expanded, cannot be located.
    let directory = row_at_depth(&root, 0).expect("the root/ folder row");
    let before = section.workspace_section_evidence();
    section
        .imp()
        .file_tree_view
        .emit_by_name::<()>("activate", &[&directory.position()]);
    // The subtree could not be located: GTK's direct collapse, not a paced one.
    let after = section.workspace_section_evidence();
    assert_eq!(
        after.collapse_direct_total,
        before.collapse_direct_total + 1,
        "{}",
        collapse_state(&section)
    );
    assert_eq!(after.collapse_steps_total, before.collapse_steps_total);
    assert!(after.collapses.is_empty());
    wait_until(Duration::from_secs(30), || {
        !directory.is_expanded()
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    let now = selection(&section)
        .selected_item()
        .and_downcast::<gtk4::TreeListRow>()
        .expect("a selected row");
    assert_eq!(now.depth(), 1);
    assert_eq!(
        now.item()
            .and_downcast::<FileTreeItem>()
            .and_then(|item| item.path()),
        Some(file),
        "the selection stays on the file outside the subtree"
    );
    drop(window);
    drop(sidebar);
}

/// Design R4, D20: the folder header collapses every expanded root, also one
/// whose row no widget showed (a row object nobody hooked, ledger A114), and
/// a later refresh restores none; a directory under a root collapsed
/// through such a row (a failed reveal's own collapse) leaves the expanded
/// paths and the watch targets as a collapse through its bound row does.
/// Fails on `main` and on the branch's direct path: the far root's collapse
/// notified nobody, and the deferred restore expanded it again.
#[test]
fn test_the_folder_header_collapses_every_root() {
    use lushtext_core::services::workspace_watch::WorkspaceWatchTarget;
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("two roots tempdir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    seed_files(&first, 4096);
    seed_files(&second.join("small"), 8);
    seed_files(&second, 16);
    save_workspaces(vec![WorkspaceConfig::with_folders(
        WorkspaceId::new("two-roots"),
        "two-roots",
        vec![
            WorkspaceFolder::new(first.clone()),
            WorkspaceFolder::new(second.clone()),
        ],
    )]);
    let (window, sidebar) = present_sidebar_window(800);
    let section = mapped_sections(&sidebar, 1).remove(0);
    section.expand_folders();
    wait_until(Duration::from_secs(60), || {
        listing(&section, &first).directory_retained_entries == 4096
            && row_for_path(&section, &second).is_some_and(|row| row.is_expanded())
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    scroll_outer_to(&sidebar, 0.0);
    assert!(
        expander_for(&section, &second).is_none(),
        "the second root's row lies outside the band"
    );
    section.collapse_folders();
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&section, &first) && collapse_settled(&section, &second)
    });
    section.queue_auto_refresh_for_test(vec![first.clone(), second.clone()]);
    section.apply_queued_refresh_for_test();
    wait_for_refresh_idle(&section);
    flush_after_delay(Duration::from_millis(500));
    let evidence = section.workspace_section_evidence();
    for root in [&first, &second] {
        assert!(
            row_for_path(&section, root).is_some_and(|row| !row.is_expanded()),
            "{} stays collapsed: {}",
            root.display(),
            collapse_state(&section)
        );
        assert!(!evidence.expanded_paths.contains(root));
    }
    // A directory under a root, expanded and collapsed through row objects no
    // widget shows: a reveal of a missing path below it expands it, fails and
    // collapses it again.
    section.expand_folders();
    wait_until(Duration::from_secs(60), || {
        listing(&section, &first).directory_retained_entries == 4096
            && row_for_path(&section, &second).is_some_and(|row| row.is_expanded())
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    scroll_outer_to(&sidebar, 0.0);
    let small = second.join("small");
    assert!(
        expander_for(&section, &small).is_none(),
        "small/ lies outside the band"
    );
    let direct_before = section.workspace_section_evidence().collapse_direct_total;
    activate_reveal_workspace_path(&window, &small.join("missing").join("file.txt"));
    wait_until(Duration::from_secs(30), || {
        !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
            && !sidebar.workspace_tree_evidence().reveal_pending
    });
    flush_after_delay(Duration::from_millis(500));
    let evidence = section.workspace_section_evidence();
    assert!(
        evidence.collapse_direct_total > direct_before,
        "the reveal expanded small/ and collapsed it again: {}",
        collapse_state(&section)
    );
    assert!(
        row_for_path(&section, &small).is_some_and(|row| !row.is_expanded()),
        "{}",
        collapse_state(&section)
    );
    assert!(!evidence.expanded_paths.contains(&small));
    assert!(
        !evidence
            .watch_targets
            .contains(&WorkspaceWatchTarget::directory(small)),
        "{:?}",
        evidence.watch_targets
    );
    drop(window);
    drop(sidebar);
}

/// Design R13's rung S2 (task 11.4): one input collapses small directories
/// directly only up to one step of rows, `COLLAPSE_STEP_ROWS`, and paces the
/// rest. Four roots of eight entries each: the folder header collapses the
/// first in the input dispatch and the other three in later steps, so the
/// input never replaces more row widgets than one step. Fails while the
/// input's budget is the frame interval's (all four collapse in the input).
#[test]
fn test_the_folder_header_collapses_directly_at_most_one_step_of_rows() {
    use lushtext_core::ui::sidebar::policy::COLLAPSE_STEP_ROWS;
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("four roots tempdir");
    let roots: Vec<PathBuf> = (0..4)
        .map(|index| dir.path().join(format!("root-{index}")))
        .collect();
    for root in &roots {
        seed_files(root, COLLAPSE_STEP_ROWS as usize);
    }
    save_workspaces(vec![WorkspaceConfig::with_folders(
        WorkspaceId::new("four-roots"),
        "four-roots",
        roots.iter().cloned().map(WorkspaceFolder::new).collect(),
    )]);
    let (window, sidebar) = present_sidebar_window(800);
    let section = mapped_sections(&sidebar, 1).remove(0);
    section.expand_folders();
    wait_until(Duration::from_secs(60), || {
        roots.iter().all(|root| {
            listing(&section, root).directory_retained_entries == COLLAPSE_STEP_ROWS as usize
                && row_for_path(&section, root).is_some_and(|row| row.is_expanded())
        }) && !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
    });
    section.collapse_folders();
    let collapsed_in_input = roots
        .iter()
        .filter(|root| row_for_path(&section, root).is_some_and(|row| !row.is_expanded()))
        .count();
    assert_eq!(
        collapsed_in_input,
        1,
        "the input collapsed {collapsed_in_input} roots of {COLLAPSE_STEP_ROWS} rows directly: {}",
        collapse_state(&section)
    );
    wait_until(Duration::from_secs(60), || {
        roots.iter().all(|root| collapse_settled(&section, root))
    });
    for root in &roots {
        assert!(
            row_for_path(&section, root).is_some_and(|row| !row.is_expanded()),
            "{} ends collapsed: {}",
            root.display(),
            collapse_state(&section)
        );
    }
    drop(window);
    drop(sidebar);
}

/// Design D18 and R13's C3 ("C3 as built"): with the view at the tail of the
/// collapsing directory, the view, clamped at its end, refills the band from
/// above. With 400 rows before it that refill is more than a unit, and the
/// collapse takes `main`'s direct collapse. With few rows before it (at most
/// a unit with the directory rows) the collapse stays paced and no dispatch
/// changes more than a unit of row widgets. Fails with the paced collapse for
/// both (the 400-row case stays GTK-expanded after the input), and with D18's
/// first rule alone for the paced case (the larger destroy-only unit brought
/// its rows in from above).
#[test]
fn test_a_collapse_at_the_tail_refills_no_more_than_a_unit_from_above() {
    for (before_rows, direct) in [(400usize, true), (28, false)] {
        let (dir, root) = seed_workspace(&[("a-before", before_rows), ("nested", 4096)]);
        let tree = present_single_folder(dir, root, "virtualized", 800);
        tree.section.expand_folders();
        let before = tree.root.join("a-before");
        let nested = tree.root.join("nested");
        expand_and_load(&tree, &before, before_rows);
        expand_and_load(&tree, &nested, 4096);
        flush_after_delay(Duration::from_millis(300));
        scroll_outer_to_bottom(&tree.sidebar);
        flush_after_delay(Duration::from_millis(300));
        let routed = tree
            .section
            .workspace_section_evidence()
            .collapse_rows_after_direct_total;
        let created = row_widgets_changed_per_dispatch(
            &tree.section,
            || activate_tree_row(&tree.section, &nested),
            || collapse_settled(&tree.section, &nested),
        );
        let evidence = tree.section.workspace_section_evidence();
        if direct {
            assert_eq!(
                evidence.collapse_rows_after_direct_total,
                routed + 1,
                "{before_rows} rows before: the tail collapse took the direct route"
            );
        } else {
            assert_eq!(evidence.collapse_rows_after_direct_total, routed);
            let over = RowChange::over_unit(&created);
            assert!(
                over.is_none(),
                "{before_rows} rows before: one dispatch changed {over:?} row widgets: {created:?}"
            );
        }
        drop(tree.window);
    }
}

/// `main`'s worst completion of a collapse case, in µs, from the stage 4
/// campaign (design R3's table, "Where `main` stands"; R13's selection-bar
/// table), or `None` for a case R3 sets no completion bar for.
const fn main_worst_completion_us(case: &str, siblings: usize, entries: usize) -> Option<u64> {
    const JUST_OVER: usize = 16 * 1024 + 1;
    const LARGE: usize = 128 * 1024;
    match (case.as_bytes(), siblings, entries) {
        (b"collapse", 0, LARGE) => Some(30_000),
        (b"collapse", 0, 64 | 256 | 4096 | JUST_OVER) => Some(23_000),
        (b"collapse", 400, 64) => Some(37_000),
        (b"collapse", 400, 256) => Some(121_000),
        (b"collapse", 400, 4096) => Some(104_000),
        (b"collapse", 400, JUST_OVER) => Some(106_000),
        (b"collapse", 400, LARGE) => Some(124_000),
        (b"header-two-roots", 0, 4096) => Some(115_000),
        _ => None,
    }
}

/// Design R3's completion bar for a case, with R9's allowance:
/// `max(main's worst + 33.3 ms, 100 ms) + allowance`, in µs (R13's table:
/// 137.3 ms at 400:4,096, 157.3 ms at 0:131,072, 214.6 ms at 400:131,072).
fn completion_bar_us(
    case: &str,
    siblings: usize,
    entries: usize,
    allowance_us: usize,
) -> Option<u64> {
    const TWO_FRAMES_US: u64 = 33_300;
    const INSTANT_US: u64 = 100_000;
    let main = main_worst_completion_us(case, siblings, entries)?;
    let allowance = u64::try_from(allowance_us).unwrap_or(u64::MAX);
    Some(
        (main + TWO_FRAMES_US)
            .max(INSTANT_US)
            .saturating_add(allowance),
    )
}

/// One case's verdict against the bars task 7.1 enforces, every one as
/// written, with no selection margin: design D2's pass bar (the input and
/// every collapse step within `B_INTERACT_US`, every dispatch within
/// `B_SETTLE_US`, except the named residual of D12, the probe scan's
/// publication into a store no row shows, which is listed and not judged),
/// rung T1's `INPUT_TO_FRAME_US` for the collapse routes, and, for the cases R3 sets one, R3's
/// completion bar with R9's allowance for both completion and the
/// picture-final figure (task 10.16 decided that no case meets the
/// first-frame bar's condition). The settled time is printed beside them,
/// unjudged. Residual routes (D2) are printed and not judged. Returns the
/// misses, one line each; D2's recorded toolkit-floor exceptions, if a
/// final campaign records one, are classified by the reading of its round,
/// not excepted here in advance.
///
/// A case C3 sent through `main`'s direct collapse (`direct_route`, design
/// R13, "C3 as built") keeps `main`'s frames by decision: its input, step,
/// dispatch and first-frame figures are printed and not judged here (Part B
/// compares them with `main`'s side), and its completion is judged at R3's
/// bar like every other case.
#[expect(
    clippy::too_many_arguments,
    reason = "a verdict-test helper over one case's printed figures"
)]
fn judge_case(
    case: &str,
    siblings: usize,
    entries: usize,
    allowance_us: usize,
    input_us: u64,
    step_us_max: u64,
    input_to_frame_us: Option<u64>,
    direct_route: bool,
    timeline: &DispatchTimeline,
) -> Vec<String> {
    use lushtext_core::model::responsiveness_budget::{
        B_INTERACT_US, B_SETTLE_US, input_to_frame_within,
    };
    if case.starts_with("residual") {
        return Vec::new();
    }
    let label = format!("{case} {siblings}:{entries}");
    let mut misses = Vec::new();
    let paced = !direct_route;
    if paced && input_us > B_INTERACT_US {
        misses.push(format!("{label}: input {input_us} us over B_INTERACT_US"));
    }
    if paced && step_us_max > B_INTERACT_US {
        misses.push(format!(
            "{label}: a step took {step_us_max} us, over B_INTERACT_US"
        ));
    }
    let over: Vec<(u64, u64)> = timeline
        .dispatches
        .iter()
        .filter(|dispatch| {
            dispatch.took_us > B_SETTLE_US && dispatch.published_terminal != Some(false)
        })
        .map(|dispatch| (dispatch.start_us, dispatch.took_us))
        .collect();
    if paced && !over.is_empty() {
        misses.push(format!(
            "{label}: dispatches (start, took) over B_SETTLE_US: {over:?}"
        ));
    }
    // Rung T1's bar is the collapse's first frame (design D6): the collapse
    // routes only; an expansion's first frame is judged by D2's dispatch bar.
    let collapse_route = case == "collapse" || case == "header-two-roots";
    if paced && collapse_route {
        match input_to_frame_us {
            Some(us) if !input_to_frame_within(us) => {
                misses.push(format!(
                    "{label}: input to frame {us} us, over INPUT_TO_FRAME_US"
                ));
            }
            None => misses.push(format!("{label}: no frame after the input")),
            Some(_) => {}
        }
    }
    let bar = completion_bar_us(case, siblings, entries, allowance_us);
    let completion = timeline.shown_us.map(|us| input_us + us);
    let picture_final = timeline.picture_final_us.map(|us| input_us + us);
    let residual: Vec<(u64, u64)> = timeline
        .dispatches
        .iter()
        .filter(|dispatch| {
            dispatch.took_us > B_SETTLE_US && dispatch.published_terminal == Some(false)
        })
        .map(|dispatch| (dispatch.start_us, dispatch.took_us))
        .collect();
    if let Some(bar) = bar {
        match completion {
            Some(us) if us > bar => {
                misses.push(format!(
                    "{label}: completion {us} us over its bar of {bar} us"
                ));
            }
            None => misses.push(format!("{label}: no completing frame")),
            Some(_) => {}
        }
        match picture_final {
            Some(us) if us > bar => {
                misses.push(format!(
                    "{label}: picture final at {us} us, over its bar of {bar} us"
                ));
            }
            None => misses.push(format!("{label}: no picture-final frame")),
            Some(_) => {}
        }
    }
    let null = |value: Option<u64>| value.map_or_else(|| "null".to_owned(), |us| us.to_string());
    println!(
        "collapse-judged {{\"case\":\"{case}\",\"siblings\":{siblings},\"entries\":{entries},\"completion_us\":{},\"completion_bar_us\":{},\"picture_final_us\":{},\"settled_us\":{},\"d12_residual\":{residual:?},\"c3_direct\":{direct_route},\"misses\":{}}}",
        null(completion),
        null(bar),
        null(picture_final),
        null(timeline.done_us.map(|us| input_us + us)),
        misses.len(),
    );
    misses
}

/// One case line of the collapse and expansion verdict, and its misses
/// ([`judge_case`]).
#[must_use]
fn print_case(
    section: &LushtextWorkspaceSection,
    case: &str,
    siblings: usize,
    entries: usize,
    input_us: u64,
    timeline: &DispatchTimeline,
) -> Vec<String> {
    let evidence = section.workspace_section_evidence();
    let step_us_max = evidence.collapse_step_us_max;
    // Design R13's C3 ("C3 as built"): a collapse case whose input took
    // `main`'s direct collapse.
    let direct_route = (case == "collapse" || case == "header-two-roots")
        && evidence.collapse_last_input_rows_after_direct > 0;
    let route = if direct_route { "direct" } else { "paced" };
    use lushtext_core::model::responsiveness_budget::{B_INTERACT_US, B_SETTLE_US};
    // R of design R9: the collapsing subtree's flattened rows at the input,
    // summed over the subtrees one input collapses (R10); every case's
    // directories hold `entries` rows and nothing expanded under them.
    let subtree_rows = if case.starts_with("header") {
        entries * 2
    } else {
        entries
    };
    // R9's allowance, `max(0, R - 16,384) x 0.5 us`, in whole microseconds.
    let allowance_us = subtree_rows.saturating_sub(16 * 1024) / 2;
    let input_over_interact = input_us > B_INTERACT_US;
    // Task 7.1 (rung T1's figure): the input plus the timeline up to the end
    // of its first frame dispatch, against `INPUT_TO_FRAME_US`.
    let input_to_frame_us = timeline
        .dispatches
        .iter()
        .find(|dispatch| dispatch.layout_us.is_some())
        .map(|frame| input_us + frame.start_us + frame.took_us);
    let input_to_frame_over = input_to_frame_us
        .is_some_and(|us| !lushtext_core::model::responsiveness_budget::input_to_frame_within(us));
    println!(
        "collapse-verdict {{\"case\":\"{case}\",\"siblings\":{siblings},\"entries\":{entries},\"subtree_rows\":{subtree_rows},\"r9_allowance_us\":{allowance_us},\"input_us\":{input_us},\"input_over_interact\":{input_over_interact},\"worst_frame_us\":{},\"worst_frame_layout_us\":{},\"worst_frame_paint_us\":{},\"worst_other_us\":{},\"done_us\":{},\"shown_us\":{},\"picture_final_us\":{},\"mono_start_us\":{},\"terminals_before_shown\":{},\"step_us_max\":{step_us_max},\"dispatches\":{},\"over_interact\":{},\"over_settle\":{},\"input_to_frame_us\":{},\"input_to_frame_over\":{input_to_frame_over},\"route\":\"{route}\"}}",
        timeline.worst(true),
        timeline.worst_layout(),
        timeline.worst_paint(),
        timeline.worst(false),
        timeline
            .done_us
            .map_or_else(|| "null".to_owned(), |us| us.to_string()),
        timeline
            .shown_us
            .map_or_else(|| "null".to_owned(), |us| us.to_string()),
        timeline
            .picture_final_us
            .map_or_else(|| "null".to_owned(), |us| us.to_string()),
        timeline.mono_start_us,
        timeline
            .terminals_before_shown
            .map_or_else(|| "null".to_owned(), |count| count.to_string()),
        timeline.dispatches.len(),
        timeline.over(B_INTERACT_US),
        timeline.over(B_SETTLE_US),
        input_to_frame_us.map_or_else(|| "null".to_owned(), |us| us.to_string()),
    );
    print_step_dispatches(section, case, siblings, entries, timeline.mono_start_us);
    judge_case(
        case,
        siblings,
        entries,
        allowance_us,
        input_us,
        step_us_max,
        input_to_frame_us,
        direct_route,
        timeline,
    )
}

/// The step dispatches of the case just printed (design D25, R14): their
/// longest, how many there were, and each one over 4 ms with its kind, rows,
/// row widgets created and offset from the input, one `collapse-steps` line.
fn print_step_dispatches(
    section: &LushtextWorkspaceSection,
    case: &str,
    siblings: usize,
    entries: usize,
    mono_start_us: i64,
) {
    const SHOWN_OVER_US: u64 = 4_000;
    let evidence = section.workspace_section_evidence();
    let over: Vec<String> = evidence
        .collapse_step_dispatches
        .iter()
        .filter(|dispatch| dispatch.took_us > SHOWN_OVER_US)
        .map(|dispatch| {
            format!(
                "{{\"kind\":\"{:?}\",\"rows\":{},\"created\":{},\"took_us\":{},\"offset_us\":{},\"frame\":{}}}",
                dispatch.kind,
                dispatch.rows,
                dispatch.rows_created,
                dispatch.took_us,
                dispatch.start_mono_us.saturating_sub(mono_start_us),
                dispatch.frame_counter,
            )
        })
        .collect();
    println!(
        "collapse-steps {{\"case\":\"{case}\",\"siblings\":{siblings},\"entries\":{entries},\"step_dispatch_us_max\":{},\"step_dispatches\":{},\"after_paint_runs\":{},\"over_4ms\":[{}]}}",
        evidence.collapse_step_dispatch_us_max,
        evidence.collapse_step_dispatches.len(),
        evidence.collapse_after_paint_runs,
        over.join(","),
    );
}

/// The cases a run measures: `COLLAPSE_VERDICT_CASES` (comma-separated case
/// names), or every case.
fn verdict_case_enabled(case: &str) -> bool {
    std::env::var("COLLAPSE_VERDICT_CASES").map_or(true, |cases| {
        cases.split(',').any(|enabled| enabled == case)
    })
}

/// A presented single-folder tree holding `nested/` with `count` entries and
/// `siblings` files after it, its folder expanded.
fn tree_with_siblings(count: usize, siblings: usize) -> LargeTree {
    let (dir, root) = seed_workspace(&[("nested", count)]);
    for index in 0..siblings {
        fixture::write_text(&root.join(format!("zz-{index:04}.txt")), "x");
    }
    let tree = present_single_folder(dir, root, "virtualized", 800);
    tree.section.expand_folders();
    tree
}

/// `slice-large-directory-collapse` task 1.1 (design D2): collapsing a
/// sidebar directory by a LushText-owned route (row activation, the folder
/// header) and expanding a small one, presented: the input dispatch, every
/// dispatch until collapsed plus 700 ms with frames split into layout and
/// paint, the time until collapsed, and each dispatch's scan terminal
/// publication. Printed on every build; task 7.1 enforces the pass bar on the
/// optimized build. A debug build, which judges nothing, walks every case at
/// small sizes only, so the shard keeps its time and memory.
#[test]
fn test_directory_collapse_and_small_expansion_dispatches_fit_the_budgets() {
    use crate::common::timing_verdicts_enforced;
    const AFTER: Duration = Duration::from_millis(700);
    const BUDGET: Duration = Duration::from_secs(240);
    let full = timing_verdicts_enforced();
    let mut misses: Vec<String> = Vec::new();
    let collapse_sizes: &[usize] = if full {
        &[64, 256, 4096, 16 * 1024 + 1, 128 * 1024]
    } else {
        &[64, 256]
    };
    let expansion_sizes: &[usize] = if full { &[64, 256, 1024] } else { &[64] };
    let large = if full { 128 * 1024 } else { 4096 };
    let reps: usize = std::env::var("COLLAPSE_VERDICT_REPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let sizes_filter: Option<Vec<usize>> =
        std::env::var("COLLAPSE_VERDICT_SIZES").ok().map(|sizes| {
            sizes
                .split(',')
                .filter_map(|size| size.parse().ok())
                .collect()
        });
    let size_enabled = |size: usize| {
        sizes_filter
            .as_ref()
            .is_none_or(|sizes| sizes.contains(&size))
    };
    let siblings_filter: Option<Vec<usize>> =
        std::env::var("COLLAPSE_VERDICT_SIBLINGS")
            .ok()
            .map(|sizes| {
                sizes
                    .split(',')
                    .filter_map(|size| size.parse().ok())
                    .collect()
            });
    for siblings in [0usize, 400] {
        if siblings_filter
            .as_ref()
            .is_some_and(|wanted| !wanted.contains(&siblings))
        {
            continue;
        }
        // Collapse by row activation.
        if verdict_case_enabled("collapse") {
            for &count in collapse_sizes {
                if !size_enabled(count) {
                    continue;
                }
                let tree = tree_with_siblings(count, siblings);
                let nested = tree.root.join("nested");
                for _ in 0..reps {
                    expand_and_load(&tree, &nested, count);
                    flush_after_delay(Duration::from_millis(300));
                    let input_us = activate_row(&tree.section, &nested);
                    let timeline = dispatch_timeline_shown(
                        &tree.section,
                        BUDGET,
                        AFTER,
                        || collapsed_and_settled(&tree.section, &nested),
                        || {
                            row_for_path(&tree.section, &nested)
                                .is_some_and(|row| !row.is_expanded())
                        },
                    );
                    misses.extend(print_case(
                        &tree.section,
                        "collapse",
                        siblings,
                        count,
                        input_us,
                        &timeline,
                    ));
                    flush_after_delay(Duration::from_millis(300));
                }
                drop(tree.window);
            }
        }
        // Expansion, cold and right after a collapse.
        if verdict_case_enabled("expand") {
            for &count in expansion_sizes {
                if !size_enabled(count) {
                    continue;
                }
                for _ in 0..reps {
                    let tree = tree_with_siblings(count, siblings);
                    let nested = tree.root.join("nested");
                    wait_until(Duration::from_secs(10), || {
                        row_for_path(&tree.section, &nested).is_some()
                    });
                    wait_for_refresh_idle(&tree.section);
                    flush_after_delay(Duration::from_millis(300));
                    let input_us = activate_row(&tree.section, &nested);
                    let timeline = dispatch_timeline(&tree.section, BUDGET, AFTER, || {
                        loaded_and_settled(&tree.section, &nested, count)
                    });
                    misses.extend(print_case(
                        &tree.section,
                        "expand-cold",
                        siblings,
                        count,
                        input_us,
                        &timeline,
                    ));
                    activate_row(&tree.section, &nested);
                    wait_until(Duration::from_secs(30), || {
                        collapsed_and_settled(&tree.section, &nested)
                    });
                    let input_us = activate_row(&tree.section, &nested);
                    let timeline = dispatch_timeline(&tree.section, BUDGET, AFTER, || {
                        loaded_and_settled(&tree.section, &nested, count)
                    });
                    misses.extend(print_case(
                        &tree.section,
                        "expand-after-collapse",
                        siblings,
                        count,
                        input_us,
                        &timeline,
                    ));
                    drop(tree.window);
                }
            }
        }
    }
    // The folder header collapsing two expanded 4,096-entry roots at once.
    if verdict_case_enabled("header") {
        for _ in 0..reps {
            ensure_gtk_init();
            let dir = tempfile::tempdir().expect("two roots tempdir");
            let first = dir.path().join("first");
            let second = dir.path().join("second");
            seed_files(&first, 4096);
            seed_files(&second, 4096);
            save_workspaces(vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("two-roots"),
                "two-roots",
                vec![
                    WorkspaceFolder::new(first.clone()),
                    WorkspaceFolder::new(second.clone()),
                ],
            )]);
            let (window, sidebar) = present_sidebar_window(800);
            let section = mapped_sections(&sidebar, 1).remove(0);
            section.expand_folders();
            wait_until(Duration::from_secs(60), || {
                loaded_and_settled(&section, &first, 4096)
                    && loaded_and_settled(&section, &second, 4096)
            });
            flush_after_delay(Duration::from_millis(300));
            let started = std::time::Instant::now();
            section.collapse_folders();
            let input_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
            let timeline = dispatch_timeline_shown(
                &section,
                BUDGET,
                AFTER,
                || {
                    collapsed_and_settled(&section, &first)
                        && collapsed_and_settled(&section, &second)
                },
                || {
                    row_for_path(&section, &first).is_some_and(|row| !row.is_expanded())
                        && row_for_path(&section, &second).is_some_and(|row| !row.is_expanded())
                },
            );
            misses.extend(print_case(
                &section,
                "header-two-roots",
                0,
                4096,
                input_us,
                &timeline,
            ));
            drop(window);
        }
    }
    // Expanding again in the middle of a 131,072-entry collapse.
    if verdict_case_enabled("reexpand") {
        for _ in 0..reps {
            let count = large;
            let tree = tree_with_siblings(count, 0);
            let nested = tree.root.join("nested");
            expand_and_load(&tree, &nested, count);
            flush_after_delay(Duration::from_millis(300));
            activate_row(&tree.section, &nested);
            let _ = dispatch_timeline(
                &tree.section,
                Duration::from_millis(30),
                Duration::ZERO,
                || false,
            );
            let input_us = activate_row(&tree.section, &nested);
            let timeline = dispatch_timeline(&tree.section, BUDGET, AFTER, || {
                loaded_and_settled(&tree.section, &nested, count)
                    && row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded())
            });
            misses.extend(print_case(
                &tree.section,
                "reexpand-mid-collapse",
                0,
                count,
                input_us,
                &timeline,
            ));
            drop(tree.window);
        }
    }
    // Residual routes (design D2), printed only.
    if verdict_case_enabled("residual") {
        // An AT-SPI DoAction on the expander's `listitem.collapse`.
        let tree = tree_with_siblings(256, 400);
        let nested = tree.root.join("nested");
        expand_and_load(&tree, &nested, 256);
        flush_after_delay(Duration::from_millis(300));
        let expander = rendered_row_widget(&tree.section, "nested")
            .and_then(|row| {
                find_descendant(&row, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
            })
            .and_downcast::<gtk4::TreeExpander>()
            .expect("the nested row's expander");
        let started = std::time::Instant::now();
        let _ = gtk4::prelude::WidgetExt::activate_action(&expander, "listitem.collapse", None);
        let input_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let timeline = dispatch_timeline(&tree.section, BUDGET, AFTER, || {
            collapsed_and_settled(&tree.section, &nested)
        });
        misses.extend(print_case(
            &tree.section,
            "residual-expander-action",
            400,
            256,
            input_us,
            &timeline,
        ));
        // A collapse while the sidebar is hidden.
        expand_and_load(&tree, &nested, 256);
        flush_after_delay(Duration::from_millis(300));
        gio::prelude::ActionGroupExt::activate_action(&tree.window, "toggle-sidebar", None);
        wait_until(Duration::from_secs(5), || !tree.section.is_mapped());
        flush_after_delay(Duration::from_millis(300));
        let input_us = activate_row(&tree.section, &nested);
        let timeline = dispatch_timeline(&tree.section, BUDGET, AFTER, || {
            collapsed_and_settled(&tree.section, &nested)
        });
        misses.extend(print_case(
            &tree.section,
            "residual-hidden-sidebar",
            400,
            256,
            input_us,
            &timeline,
        ));
        // The sidebar's visibility is persisted: show it again for the next
        // window.
        gio::prelude::ActionGroupExt::activate_action(&tree.window, "toggle-sidebar", None);
        flush_after_delay(Duration::from_millis(300));
        drop(tree.window);
        // A refresh splice removing an expanded directory, and a section
        // rebuild with one expanded.
        for count in [256, large] {
            let tree = tree_with_siblings(count, 0);
            let nested = tree.root.join("nested");
            expand_and_load(&tree, &nested, count);
            flush_after_delay(Duration::from_millis(300));
            fixture::remove_dir_all(&nested);
            let started = std::time::Instant::now();
            tree.section
                .queue_auto_refresh_for_test(vec![tree.root.clone()]);
            tree.section.apply_queued_refresh_for_test();
            let input_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
            let timeline = dispatch_timeline(&tree.section, BUDGET, AFTER, || {
                row_for_path(&tree.section, &nested).is_none()
                    && !tree
                        .section
                        .workspace_section_evidence()
                        .refresh_blocks_readiness
            });
            misses.extend(print_case(
                &tree.section,
                "residual-refresh-splice",
                0,
                count,
                input_us,
                &timeline,
            ));
            drop(tree.window);
        }
        let tree = tree_with_siblings(256, 0);
        let nested = tree.root.join("nested");
        expand_and_load(&tree, &nested, 256);
        flush_after_delay(Duration::from_millis(300));
        let started = std::time::Instant::now();
        tree.sidebar.load_workspaces();
        let input_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let timeline = dispatch_timeline(&tree.section, BUDGET, AFTER, || {
            let sections = tree.sidebar.imp().sections.borrow();
            sections.len() == 1
                && sections[0] != tree.section
                && sections[0].is_mapped()
                && !sections[0]
                    .workspace_section_evidence()
                    .refresh_blocks_readiness
        });
        misses.extend(print_case(
            &tree.section,
            "residual-section-rebuild",
            0,
            256,
            input_us,
            &timeline,
        ));
        drop(tree.window);
    }
    println!("collapse-verdict-misses {}", misses.len());
    for miss in &misses {
        println!("collapse-verdict-miss {miss}");
    }
    if full {
        assert!(
            misses.is_empty(),
            "the collapse verdict missed {} bars (task 7.1): {misses:#?}",
            misses.len()
        );
    }
}

/// Widgets in a realized row's subtree, the row's overlay included.
fn row_subtree_widgets(root: &gtk4::Widget) -> usize {
    let mut count = 1;
    let mut child = root.first_child();
    while let Some(widget) = child {
        count += row_subtree_widgets(&widget);
        child = widget.next_sibling();
    }
    count
}

/// Whether any widget in the row's subtree is a `GtkButton`.
fn row_has_button(root: &gtk4::Widget) -> bool {
    find_descendant(root, glib::object::ObjectExt::is::<gtk4::Button>).is_some()
}

/// The most widgets a realized file row carries once its decorations are
/// created on demand (design D17): the overlay, the expander and its indent
/// and disclosure icons, the content box, the open indicator, the icon and the
/// label. `main`'s rows carry 16 at depth one
/// (`.claude/handoffs/slice-large-directory-collapse/rev5/failing-first-10-4.log`).
const LIGHT_FILE_ROW_WIDGETS: usize = 9;

/// Design D17 (task 10.4): a file row carries only what every row shows; the
/// drag handle, the Focus Folder button and the three drop surfaces exist only
/// on rows that use them. Fails on `main`: every row is created with all of
/// them (16 widgets per depth-one file row).
#[test]
fn test_file_rows_carry_only_the_widgets_every_row_shows() {
    let (tree, nested) = expanded_nested(64, 0);
    let mut file_rows = 0;
    let mut most_widgets = 0;
    let mut nested_row = None;
    for widget in realized_list_rows(&tree.section.imp().file_tree_view) {
        let Some(overlay) = widget.first_child().and_downcast::<gtk4::Overlay>() else {
            continue;
        };
        let Some(row) = overlay
            .child()
            .and_downcast::<gtk4::TreeExpander>()
            .and_then(|expander| expander.list_row())
        else {
            continue;
        };
        let Some(item) = row.item().and_downcast::<FileTreeItem>() else {
            continue;
        };
        if item.path().as_deref() == Some(nested.as_path()) {
            nested_row = Some(overlay.clone());
        }
        if item.is_dir() || item.is_placeholder() {
            continue;
        }
        file_rows += 1;
        let overlay = overlay.upcast::<gtk4::Widget>();
        let widgets = row_subtree_widgets(&overlay);
        most_widgets = most_widgets.max(widgets);
        assert!(
            widgets <= LIGHT_FILE_ROW_WIDGETS,
            "file row {} carries {widgets} widgets, over {LIGHT_FILE_ROW_WIDGETS}",
            item.name()
        );
        assert!(
            !row_has_button(&overlay),
            "file row {} carries a button",
            item.name()
        );
    }
    assert!(file_rows > 0, "no realized file row");
    println!("{file_rows} realized file rows, at most {most_widgets} widgets each");
    // A non-root directory row still gets its Focus Folder button on demand.
    let nested_row = nested_row.expect("the nested directory row is realized");
    assert!(
        row_has_button(nested_row.upcast_ref()),
        "the non-root directory row lost its Focus Folder button"
    );
    drop(tree.window);
}

/// Design D17 (task 10.4): rows are created without the reorder shield, so a
/// reorder drag gives one to every realized row when it starts, file rows
/// included, before GTK can deliver drag hover to an expander. A guard test
/// (it passes on `main`, whose rows are all created with a shield). Prints the
/// drag start's GTK time: the shields are created in that dispatch, for the
/// rows the slice bin's band realizes (about the viewport's rows, not A1's
/// 200), so it is small (not a verdict; design implementation decisions).
#[test]
fn test_a_reorder_drag_gives_every_realized_row_a_shield() {
    let (tree, _nested) = expanded_nested(1024, 0);
    let shielded = |section: &LushtextWorkspaceSection| {
        realized_list_rows(&section.imp().file_tree_view)
            .into_iter()
            .filter(|widget| {
                find_descendant(widget, |candidate| {
                    candidate.has_css_class("workspace-folder-dnd-shield") && candidate.can_target()
                })
                .is_some()
            })
            .count()
    };
    let realized = realized_list_rows(&tree.section.imp().file_tree_view).len();
    assert!(
        realized > 1,
        "the viewport's rows are realized ({realized} rows)"
    );
    assert_eq!(
        shielded(&tree.section),
        0,
        "no shield is targetable outside a drag"
    );
    let folder_id = lushtext_core::model::workspace::WorkspaceFolderId::new("drag-start");
    let started = std::time::Instant::now();
    let during = tree
        .section
        .with_active_workspace_folder_reorder_drag_for_test(&folder_id, || {
            let elapsed = started.elapsed();
            (elapsed, shielded(&tree.section))
        });
    println!(
        "reorder drag start: {} us to shield {realized} realized rows",
        during.0.as_micros()
    );
    assert_eq!(during.1, realized, "every realized row owns drag hover");
    assert_eq!(
        shielded(&tree.section),
        0,
        "the shields are inert after the drag"
    );
    drop(tree.window);
}

/// Each realized row widget (the list's child, the row overlay's parent),
/// with whether its tree row lies strictly inside `directory`'s subtree by
/// tree ancestry, and its row's flattened position.
fn realized_row_widgets(
    section: &LushtextWorkspaceSection,
    directory: &Path,
) -> Vec<(gtk4::Widget, bool, u32)> {
    let mut rows = Vec::new();
    let mut child = section.imp().file_tree_view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        let Some(row) = widget
            .first_child()
            .and_downcast::<gtk4::Overlay>()
            .and_then(|overlay| overlay.child().and_downcast::<gtk4::TreeExpander>())
            .and_then(|expander| expander.list_row())
        else {
            continue;
        };
        let mut inside = false;
        let mut ancestor = row.parent();
        while let Some(candidate) = ancestor {
            if candidate
                .item()
                .and_downcast::<FileTreeItem>()
                .and_then(|item| item.path())
                .as_deref()
                == Some(directory)
            {
                inside = true;
                break;
            }
            ancestor = candidate.parent();
        }
        rows.push((widget, inside, row.position()));
    }
    rows
}

/// Whether a row widget is hidden the way D22 hides it.
fn row_widget_hidden(widget: &gtk4::Widget) -> bool {
    widget.opacity() == 0.0 && !widget.can_target()
}

/// Whether a row widget carries none of D22's hiding.
fn row_widget_shown(widget: &gtk4::Widget) -> bool {
    widget.opacity() == 1.0 && widget.can_target()
}

/// Design D22 (task 10.13): after the input dispatch every realized row of
/// the collapsing subtree is hidden on its row widget (opacity 0, no pointer
/// target, accessible `hidden`), the directory row and every row outside the
/// subtree are unchanged, and the list realized, created and placed nothing
/// for it (ledger A123). Fails without D22: the subtree's rows still draw.
#[test]
fn test_collapse_hides_the_subtree_rows_in_the_first_frame() {
    for siblings in [0, PACED_SIBLINGS] {
        let (tree, nested) = expanded_nested(4096, siblings);
        let before = realized_row_widgets(&tree.section, &nested);
        let setups = tree.section.workspace_section_evidence().row_setups;
        let first = before.iter().map(|(_, _, position)| *position).min();
        let pacing = Pacing::set(CollapsePacing::Hold);
        activate_tree_row(&tree.section, &nested);
        let after = realized_row_widgets(&tree.section, &nested);
        assert_eq!(
            after.len(),
            before.len(),
            "{siblings}: the list's children are those it had"
        );
        assert_eq!(
            tree.section.workspace_section_evidence().row_setups,
            setups,
            "{siblings}: no row widget was created"
        );
        assert_eq!(
            after.iter().map(|(_, _, position)| *position).min(),
            first,
            "{siblings}: the band did not move"
        );
        let inside: Vec<_> = after.iter().filter(|(_, inside, _)| *inside).collect();
        assert!(inside.len() > 100, "{siblings}: the band holds the subtree");
        for (widget, inside, position) in &after {
            if *inside {
                assert!(
                    row_widget_hidden(widget) && crate::common::accessible_hidden(widget),
                    "{siblings}: subtree row {position} is hidden"
                );
            } else {
                assert!(
                    row_widget_shown(widget),
                    "{siblings}: row {position} outside the subtree is unchanged"
                );
            }
        }
        assert_eq!(
            usize::try_from(
                tree.section
                    .workspace_section_evidence()
                    .collapse_hidden_rows
            )
            .unwrap_or(usize::MAX),
            inside.len()
        );
        drop(pacing);
        wait_until(Duration::from_secs(60), || {
            collapse_settled(&tree.section, &nested)
        });
        drop(tree.window);
    }
}

/// Design D22: scrolling during a held collapse rebinds rows into the
/// subtree, which are hidden, and rows out of it, which are not; rows the
/// scroll maps read `hidden` once the dispatch that mapped them has returned
/// (A123: GTK's map writes it false).
#[test]
fn test_a_rebound_row_in_a_collapsing_subtree_stays_hidden() {
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    let index = tree_index_for_path(&tree.section, &nested).expect("row index");
    let list = tree.section.imp().file_tree_view.clone();
    for (target, label) in [
        (index + 2000, "inside the subtree"),
        (index + 4096 + 20, "across the subtree's end"),
        (index + 100, "back inside"),
    ] {
        list.scroll_to(target, gtk4::ListScrollFlags::NONE, None);
        flush_after_delay(Duration::from_millis(200));
        let rows = realized_row_widgets(&tree.section, &nested);
        assert!(
            rows.iter().any(|(_, inside, _)| *inside),
            "{label}: subtree rows are realized"
        );
        for (widget, inside, position) in &rows {
            if *inside {
                assert!(row_widget_hidden(widget), "{label}: row {position} hidden");
                if widget.is_mapped() {
                    assert!(
                        crate::common::accessible_hidden(widget),
                        "{label}: mapped row {position} reads hidden"
                    );
                }
            } else {
                assert!(row_widget_shown(widget), "{label}: row {position} shown");
            }
        }
    }
    assert!(
        !tree
            .section
            .workspace_section_evidence()
            .collapses
            .is_empty(),
        "the collapse was held throughout"
    );
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    drop(tree.window);
}

/// Design D22: every end of a collapse (completion, re-expansion, removal,
/// a model reset) leaves no realized row hidden, and after a re-expansion
/// the children are shown again.
#[test]
fn test_every_collapse_end_shows_the_rows_again() {
    let assert_nothing_hidden = |section: &LushtextWorkspaceSection, end: &str| {
        let mut child = section.imp().file_tree_view.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            assert!(row_widget_shown(&widget), "{end}: a row stays hidden");
        }
        assert_eq!(
            section.workspace_section_evidence().collapse_hidden_rows,
            0,
            "{end}"
        );
    };
    // Completion.
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    activate_tree_row(&tree.section, &nested);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    assert_nothing_hidden(&tree.section, "completion");
    // Re-expansion.
    expand_and_load(&tree, &nested, 4096);
    flush_after_delay(Duration::from_millis(300));
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    assert!(
        tree.section
            .workspace_section_evidence()
            .collapse_hidden_rows
            > 0
    );
    activate_tree_row(&tree.section, &nested);
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded())
            && !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    assert_nothing_hidden(&tree.section, "re-expansion");
    assert!(
        realized_row_widgets(&tree.section, &nested)
            .iter()
            .any(|(_, inside, _)| *inside),
        "re-expansion: the children are listed and drawn"
    );
    // A model reset while the collapse is held.
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    assert!(
        tree.section
            .workspace_section_evidence()
            .collapse_hidden_rows
            > 0
    );
    tree.section
        .queue_auto_refresh_for_test(vec![tree.root.clone()]);
    tree.section.apply_queued_refresh_for_test();
    wait_until(Duration::from_secs(60), || {
        tree.section
            .workspace_section_evidence()
            .collapses
            .is_empty()
            && !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    drop(pacing);
    assert_nothing_hidden(&tree.section, "model reset");
    drop(tree.window);
    // Removal of the collapsing directory.
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    fixture::remove_dir_all(&nested);
    tree.section
        .queue_auto_refresh_for_test(vec![tree.root.clone()]);
    tree.section.apply_queued_refresh_for_test();
    wait_until(Duration::from_secs(60), || {
        row_for_path(&tree.section, &nested).is_none()
            && !tree
                .section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    drop(pacing);
    assert_nothing_hidden(&tree.section, "removal");
    drop(tree.window);
}

/// Design D22 (ledger A123): a row the list creates during a collapse (a
/// bound step's removal brings new row widgets into the band, A118) is not in
/// the list at its bind, and is hidden once inserted, by the end of the step
/// that created it: it draws nothing, takes no pointer and reads `hidden`.
/// Hiding the sidebar and showing it again mid-collapse maps the rows again,
/// and every mapped hidden row reads `hidden` again (GTK's map writes it
/// false). No hiding pass runs inside a frame clock dispatch.
#[test]
fn test_a_created_row_in_a_collapsing_subtree_reports_hidden() {
    let (tree, nested) = expanded_nested(4096, PACED_SIBLINGS);
    let syncs_in_frames = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let clock = tree
        .section
        .imp()
        .file_tree_view
        .frame_clock()
        .expect("a mapped list has a frame clock");
    let before_syncs = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let handlers = {
        let section = tree.section.clone();
        let syncs_at_before = std::rc::Rc::clone(&before_syncs);
        let before = clock.connect_before_paint(move |_| {
            syncs_at_before.set(section.workspace_section_evidence().collapse_hidden_syncs);
        });
        let section = tree.section.clone();
        let before_syncs = std::rc::Rc::clone(&before_syncs);
        let syncs_in_frames = std::rc::Rc::clone(&syncs_in_frames);
        let after = clock.connect_after_paint(move |_| {
            let now = section.workspace_section_evidence().collapse_hidden_syncs;
            syncs_in_frames.set(syncs_in_frames.get() + now.saturating_sub(before_syncs.get()));
        });
        [before, after]
    };
    let setups = tree.section.workspace_section_evidence().row_setups;
    activate_tree_row(&tree.section, &nested);
    pump_dispatches_until(Duration::from_secs(30), || {
        tree.section.workspace_section_evidence().row_setups > setups
    });
    // No main-loop turn since a step created rows: hold the collapse there.
    let pacing = Pacing::set(CollapsePacing::Hold);
    assert!(
        !tree
            .section
            .workspace_section_evidence()
            .collapses
            .is_empty(),
        "the collapse is pending after the step that created rows"
    );
    let check = |when: &str| {
        let rows = realized_row_widgets(&tree.section, &nested);
        assert!(rows.iter().any(|(_, inside, _)| *inside), "{when}");
        for (widget, inside, position) in rows {
            if inside {
                assert!(row_widget_hidden(&widget), "{when}: row {position} hidden");
                if widget.is_mapped() {
                    assert!(
                        crate::common::accessible_hidden(&widget),
                        "{when}: mapped row {position} reads hidden"
                    );
                }
            } else {
                assert!(row_widget_shown(&widget), "{when}: row {position} shown");
            }
        }
    };
    check("after the step that created rows");
    flush_after_delay(Duration::from_millis(200));
    check("frames later");
    tree.sidebar.set_visible(false);
    flush_after_delay(Duration::from_millis(200));
    tree.sidebar.set_visible(true);
    flush_after_delay(Duration::from_millis(400));
    check("after the sidebar was hidden and shown");
    for handler in handlers {
        clock.disconnect(handler);
    }
    assert_eq!(
        syncs_in_frames.get(),
        0,
        "no hiding pass ran between a frame's before-paint and after-paint"
    );
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    drop(tree.window);
}

/// Design D22 (ledger A123): real pointer input over the hidden rows of a
/// held collapse (a hover, a press, a double press) selects, activates,
/// expands and prelights nothing: the rows take no pointer, and the press
/// lands on the list.
#[test]
fn test_a_click_on_hidden_rows_changes_nothing() {
    use crate::common::press_with_real_pointer;
    use std::cell::RefCell;
    use std::rc::Rc;
    let (tree, nested) = expanded_nested(4096, 0);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    // Connected after the activation: a second `activate` handler would
    // toggle the directory back.
    let activated: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let recorder = Rc::clone(&activated);
    tree.section
        .connect_file_activated(move |path| recorder.borrow_mut().push(path.to_path_buf()));
    let index = tree_index_for_path(&tree.section, &nested).expect("row index");
    let target = realized_row_widgets(&tree.section, &nested)
        .into_iter()
        .find(|(_, inside, position)| *inside && *position == index + 3)
        .map(|(widget, _, _)| widget)
        .expect("a hidden subtree row near the directory");
    assert!(
        row_widget_hidden(&target),
        "opacity {} can-target {} {}",
        target.opacity(),
        target.can_target(),
        collapse_state(&tree.section)
    );
    let selected = selection(&tree.section).selected();
    let rows = tree
        .section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .map(gtk4::TreeListModel::n_items);
    for presses in [1, 2] {
        press_with_real_pointer(&tree.window, &target, presses);
        flush_after_delay(Duration::from_millis(500));
        assert_eq!(
            selection(&tree.section).selected(),
            selected,
            "{presses}: the selection stays on the directory row"
        );
        assert!(
            activated.borrow().is_empty(),
            "{presses}: nothing is activated"
        );
        assert_eq!(
            tree.section
                .imp()
                .tree_model
                .borrow()
                .as_ref()
                .map(gtk4::TreeListModel::n_items),
            rows,
            "{presses}: nothing expands or collapses"
        );
        assert!(
            !target.state_flags().contains(gtk4::StateFlags::PRELIGHT),
            "{presses}: the hidden row under the pointer is not prelit"
        );
        assert!(row_widget_hidden(&target), "{presses}: it stays hidden");
    }
    assert_eq!(
        tree.section.workspace_section_evidence().collapses.len(),
        1,
        "the collapse was held throughout"
    );
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        collapse_settled(&tree.section, &nested)
    });
    drop(tree.window);
}

/// Design D22, R6.5 (task 10.13): with two overlapping folders (`root/` and
/// `root/shared/`), a sliced collapse of `shared/` under `root/` decides its
/// subtree by tree ancestry: the second folder's occurrence of `shared/`,
/// its root row included, is not hidden, carries no collapsing class, reads
/// expanded, and keeps loading the listing it started in the same dispatch.
/// Fails with path membership: the path prefix marked the second folder
/// collapsing and `stop_subtree_work` cancelled its load.
#[test]
fn test_overlapping_folders_hide_only_the_collapsing_occurrence() {
    ensure_gtk_init();
    let count = 4096;
    let dir = tempfile::tempdir().expect("overlapping folders tempdir");
    let root = dir.path().join("root");
    let shared = root.join("shared");
    seed_files(&shared, count);
    save_workspaces(vec![WorkspaceConfig::with_folders(
        WorkspaceId::new("overlap"),
        "overlap",
        vec![
            WorkspaceFolder::new(root.clone()),
            WorkspaceFolder::new(shared.clone()),
        ],
    )]);
    let (window, sidebar) = present_sidebar_window(800);
    let section = mapped_sections(&sidebar, 1).remove(0);
    let row_at_depth = |path: &Path, depth: u32| {
        let model = section.imp().tree_model.borrow().as_ref().cloned()?;
        (0..model.n_items())
            .filter_map(|index| model.item(index).and_downcast::<gtk4::TreeListRow>())
            .find(|row| {
                row.depth() == depth
                    && row
                        .item()
                        .and_downcast::<FileTreeItem>()
                        .and_then(|item| item.path())
                        .as_deref()
                        == Some(path)
            })
    };
    let children = |row: &gtk4::TreeListRow| {
        row.children()
            .and_downcast::<gtk4::gio::ListStore>()
            .map_or(0, |store| store.n_items())
    };
    // Only `root/` is expanded, then `shared/` under it, fully loaded.
    wait_until(Duration::from_secs(30), || {
        row_at_depth(&root, 0).is_some() && row_at_depth(&shared, 0).is_some()
    });
    wait_for_refresh_idle(&section);
    row_at_depth(&root, 0).expect("root/").set_expanded(true);
    wait_until(Duration::from_secs(30), || {
        row_at_depth(&shared, 1).is_some()
    });
    wait_for_refresh_idle(&section);
    let inner = row_at_depth(&shared, 1).expect("shared/ under root/");
    inner.set_expanded(true);
    wait_until(Duration::from_secs(60), || {
        children(&inner) == u32::try_from(count).unwrap_or(u32::MAX)
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    flush_after_delay(Duration::from_millis(300));
    let pacing = Pacing::set(CollapsePacing::Hold);
    // The second folder starts loading `shared/`, and in the same dispatch
    // the occurrence under `root/` collapses.
    let outer = row_at_depth(&shared, 0).expect("the shared/ folder row");
    outer.set_expanded(true);
    section
        .imp()
        .file_tree_view
        .emit_by_name::<()>("activate", &[&inner.position()]);
    assert_eq!(
        section.workspace_section_evidence().collapses.len(),
        1,
        "a sliced collapse: {}",
        collapse_state(&section)
    );
    wait_until(Duration::from_secs(60), || {
        children(&outer) == u32::try_from(count).unwrap_or(u32::MAX)
    });
    // Bring the second folder's rows into the band.
    let target = outer.position() + 100;
    section
        .imp()
        .file_tree_view
        .scroll_to(target, gtk4::ListScrollFlags::NONE, None);
    flush_after_delay(Duration::from_millis(600));
    let mut seen_outer = 0;
    let mut positions = Vec::new();
    let mut child = section.imp().file_tree_view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        let Some(expander) = widget
            .first_child()
            .and_downcast::<gtk4::Overlay>()
            .and_then(|overlay| overlay.child().and_downcast::<gtk4::TreeExpander>())
        else {
            continue;
        };
        let Some(row) = expander.list_row() else {
            continue;
        };
        positions.push(row.position());
        let in_outer = row == outer || row.parent().as_ref() == Some(&outer);
        if !in_outer {
            continue;
        }
        seen_outer += 1;
        assert!(
            row_widget_shown(&widget) && !crate::common::accessible_hidden(&widget),
            "row {} of the second folder is not hidden",
            row.position()
        );
        if row == outer {
            assert!(!expander.has_css_class(COLLAPSING_CLASS));
            assert!(crate::common::accessible_expanded_is(&expander, true));
        }
    }
    assert!(
        seen_outer > 100,
        "the second folder's rows are realized: target {target}, band {:?}..{:?}",
        positions.iter().min(),
        positions.iter().max()
    );
    assert!(
        !section.workspace_section_evidence().collapses.is_empty(),
        "the collapse was held throughout"
    );
    drop(pacing);
    wait_until(Duration::from_secs(60), || {
        !inner.is_expanded()
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    assert!(outer.is_expanded(), "the second folder stays expanded");
    assert_eq!(children(&outer), u32::try_from(count).unwrap_or(u32::MAX));
    drop(window);
    drop(sidebar);
}

// ---------------------------------------------------------------------------
// Design D22, task 10.14: navigation keys during a collapse.
// ---------------------------------------------------------------------------

/// X keysyms the held-key tests type.
mod keysym {
    pub const UP: u32 = 0xff52;
    pub const DOWN: u32 = 0xff54;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const HOME: u32 = 0xff50;
    pub const END: u32 = 0xff57;
    pub const RETURN: u32 = 0xff0d;
    pub const SPACE: u32 = 0x20;
    pub const ESCAPE: u32 = 0xff1b;
    pub const TAB: u32 = 0xff09;
    pub const SHIFT_L: u32 = 0xffe1;
    pub const CONTROL_L: u32 = 0xffe3;
}

/// Frames painted while a collapse was pending with the list's focus on a
/// hidden row, counted at each frame's `before-paint` (design D22: no
/// painted frame shows the focus on a hidden row).
struct HiddenFocusWatch {
    clock: gtk4::gdk::FrameClock,
    handler: Option<glib::SignalHandlerId>,
    frames: std::rc::Rc<std::cell::Cell<u32>>,
}

impl HiddenFocusWatch {
    fn start(window: &LushtextWindow, section: &LushtextWorkspaceSection) -> Self {
        let clock = section
            .imp()
            .file_tree_view
            .frame_clock()
            .expect("the list has a frame clock");
        let frames = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = std::rc::Rc::clone(&frames);
        let window = window.downgrade();
        let list = section
            .imp()
            .file_tree_view
            .upcast_ref::<gtk4::Widget>()
            .downgrade();
        let handler = clock.connect_before_paint(move |_| {
            let (Some(window), Some(list)) = (window.upgrade(), list.upgrade()) else {
                return;
            };
            let mut widget = gtk4::prelude::GtkWindowExt::focus(&window);
            while let Some(current) = widget {
                let parent = current.parent();
                if parent.as_ref() == Some(&list) {
                    if row_widget_hidden(&current) {
                        count.set(count.get() + 1);
                    }
                    return;
                }
                widget = parent;
            }
        });
        Self {
            clock,
            handler: Some(handler),
            frames,
        }
    }

    fn frames(&self) -> u32 {
        self.frames.get()
    }
}

impl Drop for HiddenFocusWatch {
    fn drop(&mut self) {
        if let Some(handler) = self.handler.take() {
            self.clock.disconnect(handler);
        }
    }
}

/// The position of the row holding the window's focus, if the focus is on a
/// row of `section`'s list. Read from the window, not the list's
/// `focus-child`: GTK clears the focus-child chain while the window is
/// inactive, and the headless compositor deactivates it between real-input
/// sessions, while the window's focus widget, which keys go to, stays.
fn window_focused_row(window: &LushtextWindow, section: &LushtextWorkspaceSection) -> Option<u32> {
    let list = section
        .imp()
        .file_tree_view
        .upcast_ref::<gtk4::Widget>()
        .clone();
    let mut widget = gtk4::prelude::GtkWindowExt::focus(window);
    while let Some(current) = widget {
        let parent = current.parent();
        if parent.as_ref() == Some(&list) {
            return find_descendant(&current, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
                .and_downcast::<gtk4::TreeExpander>()
                .and_then(|expander| expander.list_row())
                .map(|row| row.position())
                .filter(|position| *position != gtk4::INVALID_LIST_POSITION);
        }
        widget = parent;
    }
    None
}

/// Wait up to `budget` for `done`, failing with `what`, the collapse state
/// and where the focus is.
fn wait_or_report(tree: &LargeTree, budget: Duration, what: &str, done: impl FnMut() -> bool) {
    assert!(
        wait_until_or_false(budget, done),
        "not met: {what}: {}; {}",
        collapse_state(&tree.section),
        focus_report(tree)
    );
}

/// Where the window's focus is, for a failure message.
fn focus_report(tree: &LargeTree) -> String {
    let focus = gtk4::prelude::GtkWindowExt::focus(&tree.window);
    let mut chain = Vec::new();
    let mut widget = focus;
    while let Some(current) = widget {
        chain.push(current.type_().name().to_owned());
        if chain.len() > 6 {
            break;
        }
        widget = current.parent();
    }
    let list = &tree.section.imp().file_tree_view;
    let evidence = tree.section.workspace_section_evidence();
    format!(
        "held keys waiting {} held {} replayed {} dropped {}; window active {} focus chain \
         {chain:?}; list focus_child {:?}, list has_focus {}, focused row {:?}",
        evidence.collapse_held_keys,
        evidence.collapse_keys_held_total,
        evidence.collapse_keys_replayed_total,
        evidence.collapse_keys_dropped_total,
        tree.window.is_active(),
        list.focus_child()
            .map(|child| child.type_().name().to_owned()),
        list.has_focus(),
        window_focused_row(&tree.window, &tree.section)
    )
}

/// Make the window active with a real press on `path`'s row, bringing it
/// into view first, which focuses and selects it, so real key input reaches
/// the list. (A programmatic focus move is not enough: the headless
/// compositor deactivates the window between real-input sessions, and
/// `scroll_to`'s focus flag then leaves the focus where it was.) The press
/// activates the window, which gives the focus back to its focus row, and
/// GTK scrolls the list to that row before the press lands: keep the focus
/// row near `path` before the list grows ([`expand_with_focus_on`]).
fn press_row_for_real(tree: &LargeTree, path: &Path) {
    // Past GTK's 400 ms double-click time: a press soon after another on the
    // same row would activate it.
    flush_after_delay(Duration::from_millis(500));
    if !tree.window.is_active()
        && let Some(focused) = window_focused_row(&tree.window, &tree.section)
    {
        // Activating the window gives the focus back to its focus row, and
        // GTK then scrolls the list to that row: a press aimed elsewhere
        // would land on whatever the scroll brings under it. Activate the
        // window with a press on the focus row itself, shown first, so the
        // activation scrolls nothing.
        press_shown_row(tree, focused);
        flush_after_delay(Duration::from_millis(500));
    }
    let index = tree_index_for_path(&tree.section, path).expect("row index");
    press_shown_row(tree, index);
    wait_or_report(
        tree,
        Duration::from_secs(5),
        "the pressed row has the focus",
        || window_focused_row(&tree.window, &tree.section) == Some(index),
    );
    flush_after_delay(Duration::from_millis(100));
}

/// Scroll row `index` into view and press it with real input.
fn press_shown_row(tree: &LargeTree, index: u32) {
    let row = || {
        tree.section
            .imp()
            .tree_model
            .borrow()
            .as_ref()
            .and_then(|model| model.item(index))
            .and_downcast::<gtk4::TreeListRow>()
    };
    let expander = || {
        let row = row()?;
        let mut child = tree.section.imp().file_tree_view.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Some(expander) =
                find_descendant(&widget, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
                    .and_downcast::<gtk4::TreeExpander>()
                && expander.list_row().as_ref() == Some(&row)
            {
                return Some(expander);
            }
        }
        None
    };
    tree.section
        .imp()
        .file_tree_view
        .scroll_to(index, gtk4::ListScrollFlags::NONE, None);
    wait_or_report(tree, Duration::from_secs(10), "the row is shown", || {
        expander().is_some_and(|expander| {
            expander.is_mapped() && inside_outer_viewport(&tree.sidebar, expander.upcast_ref())
        }) && !tree.section.imp().file_tree_slice.outer_request_pending()
    });
    flush_after_delay(Duration::from_millis(200));
    let label = expander()
        .and_then(|expander| expander.child())
        .expect("the row content");
    crate::common::activate_with_real_press(&tree.window, &label);
}

/// Expand `directory` again with the focus on its row first, so the focus
/// row stays near the top of the list ([`press_row_for_real`]).
fn expand_with_focus_on(tree: &LargeTree, directory: &Path, count: usize) {
    press_row_for_real(tree, directory);
    expand_and_load(tree, directory, count);
}

/// Put the focus on `path`'s row without selecting it: a real press on the
/// root row, then Ctrl+Down, which moves GTK's focus and not its selection.
/// `path` must be the root's first child.
fn focus_row_unselected(tree: &LargeTree, path: &Path) {
    use keysym::{CONTROL_L, DOWN};
    press_row_for_real(tree, &tree.root);
    crate::common::type_with_real_keyboard(&[&[CONTROL_L, DOWN]]);
    let index = tree_index_for_path(&tree.section, path).expect("row index");
    wait_or_report(
        tree,
        Duration::from_secs(5),
        "Ctrl+Down moved the focus",
        || window_focused_row(&tree.window, &tree.section) == Some(index),
    );
    flush_after_delay(Duration::from_millis(100));
}

/// Start a held sliced collapse of `directory`, type `chords` while it is
/// held, and check that the keys waited and the focus did not move; then let
/// the collapse run and wait until it has ended and every held key has been
/// put back and delivered.
fn type_during_a_held_collapse(
    tree: &LargeTree,
    directory: &Path,
    chords: &[&[u32]],
    held: u32,
    watch: &HiddenFocusWatch,
) {
    held_collapse_then(tree, directory, chords, held, watch, || {});
}

/// [`type_during_a_held_collapse`], running `while_held` once the keys are
/// held and before the collapse resumes.
fn held_collapse_then(
    tree: &LargeTree,
    directory: &Path,
    chords: &[&[u32]],
    held: u32,
    watch: &HiddenFocusWatch,
    while_held: impl FnOnce(),
) {
    let focused = window_focused_row(&tree.window, &tree.section);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, directory);
    assert_eq!(
        tree.section.workspace_section_evidence().collapses.len(),
        1,
        "the collapse is sliced and held: {}",
        collapse_state(&tree.section)
    );
    for chord in chords {
        crate::common::type_with_real_keyboard(&[chord]);
    }
    wait_or_report(tree, Duration::from_secs(5), "the keys are held", || {
        tree.section.workspace_section_evidence().collapse_held_keys == held
    });
    flush_after_delay(Duration::from_millis(100));
    let evidence = tree.section.workspace_section_evidence();
    assert_eq!(
        evidence.collapse_held_keys,
        held,
        "the keys wait for the collapse: {}",
        collapse_state(&tree.section)
    );
    assert_eq!(
        window_focused_row(&tree.window, &tree.section),
        focused,
        "the focus has not moved while the collapse runs: {}",
        focus_report(tree)
    );
    assert_eq!(
        watch.frames(),
        0,
        "no frame painted the focus on a hidden row"
    );
    while_held();
    drop(pacing);
    wait_or_report(
        tree,
        Duration::from_secs(60),
        "the collapse ends and the keys are delivered",
        || {
            collapse_settled(&tree.section, directory)
                && tree.section.workspace_section_evidence().collapse_held_keys == 0
        },
    );
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(
        tree.section.workspace_section_evidence().collapse_held_keys,
        0,
        "every held key was delivered: {}",
        collapse_state(&tree.section)
    );
}

/// Design D22 (task 10.14): in a 400:4,096 sliced collapse, Down pressed
/// right after the input waits for the collapse, no painted frame shows the
/// focus on a hidden row, and once it ends the focus is on the row after the
/// directory, where `main` leaves it; Down, Down and End land where they
/// would on `main`. Fails without the hold: Down moves the focus onto the
/// first hidden child.
#[test]
fn test_navigation_keys_wait_for_the_collapse() {
    use keysym::{DOWN, END};
    let count = 4096;
    let (tree, nested) = expanded_nested(count, PACED_SIBLINGS);
    press_row_for_real(&tree, &nested);
    let directory = tree_index_for_path(&tree.section, &nested).expect("row index");
    let watch = HiddenFocusWatch::start(&tree.window, &tree.section);
    type_during_a_held_collapse(&tree, &nested, &[&[DOWN]], 1, &watch);
    assert_eq!(
        watch.frames(),
        0,
        "no frame painted the focus on a hidden row"
    );
    wait_or_report(&tree, Duration::from_secs(5), "wait 5", || {
        window_focused_row(&tree.window, &tree.section) == Some(directory + 1)
    });
    assert_eq!(
        window_focused_row(&tree.window, &tree.section),
        Some(directory + 1)
    );
    assert_eq!(selection(&tree.section).selected(), directory + 1);
    let evidence = tree.section.workspace_section_evidence();
    assert_eq!(evidence.collapse_keys_replayed_total, 1);
    assert_eq!(evidence.collapse_keys_dropped_total, 0);

    expand_with_focus_on(&tree, &nested, count);
    press_row_for_real(&tree, &nested);
    type_during_a_held_collapse(&tree, &nested, &[&[DOWN], &[DOWN], &[END]], 3, &watch);
    assert_eq!(
        watch.frames(),
        0,
        "no frame painted the focus on a hidden row"
    );
    let last = tree
        .section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .map(gtk4::TreeListModel::n_items)
        .expect("the model")
        - 1;
    wait_or_report(&tree, Duration::from_secs(5), "wait 6", || {
        window_focused_row(&tree.window, &tree.section) == Some(last)
    });
    assert_eq!(window_focused_row(&tree.window, &tree.section), Some(last));
    assert_eq!(selection(&tree.section).selected(), last);
    drop(watch);
    drop(tree.window);
}

/// Design D22 (task 10.14, the hold wider than the directory row): Up from
/// the row after a collapsing subtree would land on its last hidden row, so
/// it waits too, and lands on the directory row once the collapse ends.
/// Fails without the hold.
#[test]
fn test_up_from_the_row_after_the_subtree_waits_for_the_collapse() {
    use keysym::UP;
    let count = 256;
    let (tree, nested) = expanded_nested(count, PACED_SIBLINGS);
    press_row_for_real(&tree, &nested);
    let after = tree.root.join("top.txt");
    press_row_for_real(&tree, &after);
    let directory = tree_index_for_path(&tree.section, &nested).expect("row index");
    let watch = HiddenFocusWatch::start(&tree.window, &tree.section);
    type_during_a_held_collapse(&tree, &nested, &[&[UP]], 1, &watch);
    assert_eq!(
        watch.frames(),
        0,
        "no frame painted the focus on a hidden row"
    );
    wait_or_report(&tree, Duration::from_secs(5), "wait 7", || {
        window_focused_row(&tree.window, &tree.section) == Some(directory)
    });
    assert_eq!(
        window_focused_row(&tree.window, &tree.section),
        Some(directory)
    );
    drop(watch);
    drop(tree.window);
}

/// Design D22, R12 (task 10.14): a key whose effect depends on the focused
/// row waits behind a held key and acts on the row the held key reaches:
/// Down then Space reaches the list's peek route on the row after the
/// directory; Down then Return
/// activates that row and does not expand the directory again. Escape and
/// Tab are not held: they act at once, as on any frame, and the held Down
/// then acts from where Tab left the focus.
#[test]
fn test_keys_after_a_held_key_wait_behind_it() {
    use keysym::{DOWN, ESCAPE, RETURN, SPACE, TAB};
    use std::cell::RefCell;
    use std::rc::Rc;
    let count = 256;
    let (tree, nested) = expanded_nested(count, PACED_SIBLINGS);
    let directory = tree_index_for_path(&tree.section, &nested).expect("row index");
    press_row_for_real(&tree, &nested);
    let watch = HiddenFocusWatch::start(&tree.window, &tree.section);

    // Down, then Space. The peek popover is not let open: mapping it on the
    // presented window stops the headless compositor (mutter 51 segfaults
    // in `meta_wayland_xdg_popup_apply_state`). A capture controller beside
    // the key route, before the list's peek controller, records where the
    // Space put back lands and stops it there; the peek route itself is
    // covered by the peek tests.
    let space_rows: Rc<RefCell<Vec<Option<u32>>>> = Rc::default();
    let space_probe = gtk4::EventControllerKey::new();
    space_probe.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let rows = Rc::clone(&space_rows);
        let window = tree.window.downgrade();
        let section = tree.section.downgrade();
        space_probe.connect_key_pressed(move |_, keyval, _, _| {
            let (Some(window), Some(section)) = (window.upgrade(), section.upgrade()) else {
                return glib::Propagation::Proceed;
            };
            if keyval != gtk4::gdk::Key::space
                || !section.workspace_section_evidence().collapses.is_empty()
            {
                return glib::Propagation::Proceed;
            }
            rows.borrow_mut()
                .push(window_focused_row(&window, &section));
            glib::Propagation::Stop
        });
    }
    tree.section
        .imp()
        .file_tree_slice
        .add_controller(space_probe.clone());
    held_collapse_then(&tree, &nested, &[&[DOWN], &[SPACE]], 2, &watch, || {
        assert!(space_rows.borrow().is_empty(), "Space waits behind Down");
    });
    tree.section
        .imp()
        .file_tree_slice
        .remove_controller(&space_probe);
    assert_eq!(
        space_rows.borrow().as_slice(),
        &[Some(directory + 1)],
        "Space acts on the row after the directory, which Down reached"
    );
    assert!(!tree.section.peek_visible());

    drop(watch);
    drop(tree.window);

    // Down, then Escape and Tab: neither waits. A fresh window, so no state
    // of the case before carries over.
    let (tree, nested) = expanded_nested(count, PACED_SIBLINGS);
    press_row_for_real(&tree, &nested);
    let watch = HiddenFocusWatch::start(&tree.window, &tree.section);
    let pacing = Pacing::set(CollapsePacing::Hold);
    activate_tree_row(&tree.section, &nested);
    crate::common::type_with_real_keyboard(&[&[DOWN], &[ESCAPE], &[TAB]]);
    flush_after_delay(Duration::from_millis(300));
    assert_eq!(
        tree.section.workspace_section_evidence().collapse_held_keys,
        1,
        "only Down waits: {}",
        focus_report(&tree)
    );
    let on_the_expander = gtk4::prelude::GtkWindowExt::focus(&tree.window)
        .and_downcast::<gtk4::TreeExpander>()
        .is_some_and(|expander| expander.list_row() == row_for_path(&tree.section, &nested));
    assert!(
        on_the_expander,
        "Tab acted at once: the focus is on the directory row's expander: {}",
        focus_report(&tree)
    );
    drop(pacing);
    wait_or_report(&tree, Duration::from_secs(60), "the collapse ends", || {
        collapse_settled(&tree.section, &nested)
            && tree.section.workspace_section_evidence().collapse_held_keys == 0
    });
    wait_or_report(
        &tree,
        Duration::from_secs(5),
        "Down acts after the collapse",
        || window_focused_row(&tree.window, &tree.section) == Some(directory + 1),
    );

    drop(watch);
    drop(tree.window);

    // Down, then Return, in a fresh window (it opens the file in an editor).
    let (tree, nested) = expanded_nested(count, PACED_SIBLINGS);
    let after = tree.root.join("top.txt");
    press_row_for_real(&tree, &nested);
    let watch = HiddenFocusWatch::start(&tree.window, &tree.section);
    let activated: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    // Connected once the collapse is held: a second `activate` handler
    // connected before the activation would toggle the directory back.
    held_collapse_then(&tree, &nested, &[&[DOWN], &[RETURN]], 2, &watch, || {
        let recorder = Rc::clone(&activated);
        tree.section
            .connect_file_activated(move |path| recorder.borrow_mut().push(path.to_path_buf()));
        assert!(activated.borrow().is_empty(), "nothing is activated yet");
    });
    wait_or_report(&tree, Duration::from_secs(5), "Return activates", || {
        !activated.borrow().is_empty()
    });
    assert_eq!(
        activated.borrow().as_slice(),
        std::slice::from_ref(&after),
        "Return activates the row after the directory"
    );
    assert!(
        row_for_path(&tree.section, &nested).is_some_and(|row| !row.is_expanded()),
        "the directory is not expanded again"
    );
    assert_eq!(
        watch.frames(),
        0,
        "no frame painted the focus on a hidden row"
    );
    drop(watch);
    drop(tree.window);
}

/// The focused row's path and the selected row's path.
fn focus_and_selection(
    window: &LushtextWindow,
    section: &LushtextWorkspaceSection,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let path_at = |position: u32| {
        section
            .imp()
            .tree_model
            .borrow()
            .as_ref()
            .and_then(|model| model.item(position))
            .and_downcast::<gtk4::TreeListRow>()
            .and_then(|row| row.item())
            .and_downcast::<FileTreeItem>()
            .and_then(|item| item.path())
    };
    let focused = window_focused_row(window, section).and_then(path_at);
    let selected = selection(section).selected();
    let selected = (selected != gtk4::INVALID_LIST_POSITION)
        .then(|| path_at(selected))
        .flatten();
    (focused, selected)
}

/// A press the window saw: its keyval, the event, the sidebar's scroll
/// position then, and whether that position was settled.
type SeenPress = (u32, gtk4::gdk::Event, f64, bool);

/// Design D22, R12 (task 10.14): each held navigation key, with and
/// without Shift and Ctrl, leaves the focus and the selection where GTK's
/// own binding leaves them for the same key event delivered on the same
/// final tree with no collapse, Page Up and Page Down at the test window's
/// height; and a Down on a focused directory row that is not selected
/// selects it, as GTK's binding does. The reference is the very press the
/// window saw, put back from the same start once the tree is final (ledger
/// A126: a press put back is routed as a fresh one), because a second real
/// press through the headless compositor's input sessions is not delivered
/// reliably. The keypad forms are not typed (the compositor's virtual
/// keymap has none): GTK binds none of them on a list or a window, so they
/// move nothing either way.
#[test]
fn test_replayed_navigation_matches_gtk() {
    use glib::translate::IntoGlib;
    use keysym::{CONTROL_L, DOWN, END, HOME, PAGE_DOWN, PAGE_UP, SHIFT_L, UP};
    use std::cell::RefCell;
    use std::rc::Rc;
    let count = 256;
    let (tree, nested) = expanded_nested(count, PACED_SIBLINGS);
    let watch = HiddenFocusWatch::start(&tree.window, &tree.section);
    // The presses the window sees, before any route takes them.
    // The presses the window sees, before any route takes them, with the
    // sidebar's scroll position at each: the replayed press is seen twice,
    // and its second sighting carries the position it acts from.
    let outer = tree.sidebar.imp().outer_scrolled_window.vadjustment();
    let seen: Rc<RefCell<Vec<SeenPress>>> = Rc::default();
    let recorder = gtk4::EventControllerKey::new();
    recorder.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let seen = Rc::clone(&seen);
        let outer = outer.clone();
        let slice = tree.section.imp().file_tree_slice.clone();
        recorder.connect_key_pressed(move |controller, keyval, _, _| {
            if let Some(event) = controller.current_event() {
                // A settled position: no scroll request the slice bin has
                // not applied yet.
                let settled = !slice.outer_request_pending();
                seen.borrow_mut()
                    .push((keyval.into_glib(), event, outer.value(), settled));
            }
            glib::Propagation::Proceed
        });
    }
    tree.window.add_controller(recorder.clone());
    let mut cases: Vec<(Vec<u32>, bool)> = Vec::new();
    for key in [UP, DOWN, PAGE_UP, PAGE_DOWN, HOME, END] {
        for modifiers in [&[][..], &[SHIFT_L], &[CONTROL_L], &[CONTROL_L, SHIFT_L]] {
            let mut chord = modifiers.to_vec();
            chord.push(key);
            cases.push((chord, true));
        }
    }
    cases.push((vec![DOWN], false));
    cases.push((vec![SHIFT_L, DOWN], false));
    let start = |tree: &LargeTree, selected: bool| {
        if selected {
            press_row_for_real(tree, &nested);
        } else {
            focus_row_unselected(tree, &nested);
        }
    };
    for (index, (chord, selected)) in cases.iter().enumerate() {
        if index > 0 {
            expand_with_focus_on(&tree, &nested, count);
        }
        start(&tree, *selected);
        seen.borrow_mut().clear();
        type_during_a_held_collapse(&tree, &nested, &[chord.as_slice()], 1, &watch);
        let replayed = focus_and_selection(&tree.window, &tree.section);
        let key = *chord.last().expect("a key");
        let (press, scrolled, settled) = seen
            .borrow()
            .iter()
            .rfind(|(keyval, _, _, _)| *keyval == key)
            .map(|(_, event, value, settled)| (event.clone(), *value, *settled))
            .expect("the window saw the press");
        assert!(
            settled,
            "chord {chord:x?}: the replay acted from a settled scroll position"
        );
        // GTK's own binding, on the final tree, for the same press, from the
        // same scroll position (Page Up and Page Down move by the band the
        // slice bin shows, which follows it).
        start(&tree, *selected);
        outer.set_value(scrolled);
        wait_or_report(
            &tree,
            Duration::from_secs(5),
            "the scroll position is restored",
            || {
                (outer.value() - scrolled).abs() < 0.5
                    && !tree.section.imp().file_tree_slice.outer_request_pending()
            },
        );
        flush_after_delay(Duration::from_millis(200));
        #[expect(
            deprecated,
            reason = "ledger A126: gdk_display_put_event, deprecated with no replacement, \
                      delivers the recorded press through GTK's own route"
        )]
        WidgetExt::display(&tree.window).put_event(&press);
        flush_after_delay(Duration::from_millis(300));
        let reference = focus_and_selection(&tree.window, &tree.section);
        assert_eq!(
            replayed,
            reference,
            "chord {chord:x?} (directory selected: {selected}): the replay lands where GTK's \
             binding lands: {}",
            focus_report(&tree)
        );
    }
    assert_eq!(
        watch.frames(),
        0,
        "no frame painted the focus on a hidden row"
    );
    tree.window.remove_controller(&recorder);
    drop(watch);
    drop(tree.window);
}

// ---------------------------------------------------------------------------
// Design D24 (task 10.15): the path cache leaves the collapse's steps.
// ---------------------------------------------------------------------------

/// The sibling paths `directory`'s path cache holds, read from the listing
/// of the store that owns it: GTK's `is_expandable()` probe store lists the
/// same directory and owns no cache (A22).
fn cached_paths(section: &LushtextWorkspaceSection, directory: &Path) -> usize {
    section
        .workspace_section_evidence()
        .directory_listings
        .iter()
        .filter(|listing| listing.path == directory)
        .map(|listing| listing.cached_paths)
        .max()
        .unwrap_or(0)
}

/// Design D24: a sliced 131,072-entry collapse detaches the directory's path
/// cache at its intent, so no collapse step removes a path-cache row, the
/// consistency fallback never runs, and the listing retirement removes the
/// detached entries afterwards. Fails before D24: every chunk and bound step
/// spliced its rows out of the caches (`cache_collapse_rows_removed` counts
/// all 131,072).
#[test]
fn test_a_sliced_collapse_removes_no_path_cache_rows_in_its_steps() {
    let count = 1 << 17;
    let tree = tree_with_siblings(count, 0);
    let nested = tree.root.join("nested");
    expand_and_load(&tree, &nested, count);
    flush_after_delay(Duration::from_millis(300));
    let before = tree.section.workspace_section_evidence();
    assert_eq!(
        cached_paths(&tree.section, &nested),
        count,
        "the loaded directory's cache lists every entry"
    );
    activate_row(&tree.section, &nested);
    let intent = tree.section.workspace_section_evidence();
    assert_eq!(
        intent.collapses.len(),
        1,
        "{}",
        collapse_state(&tree.section)
    );
    assert_eq!(
        cached_paths(&tree.section, &nested),
        0,
        "the cache left at the intent"
    );
    assert_eq!(
        intent.cache_detached_parents,
        before.cache_detached_parents + 1
    );
    wait_until(Duration::from_secs(120), || {
        collapsed_and_settled(&tree.section, &nested)
    });
    let after = tree.section.workspace_section_evidence();
    assert_eq!(
        after.cache_collapse_rows_removed, before.cache_collapse_rows_removed,
        "no step removed a path-cache row"
    );
    assert_eq!(
        after.cache_full_rebuilds, before.cache_full_rebuilds,
        "no step reached the consistency fallback"
    );
    wait_until(Duration::from_secs(60), || {
        tree.section
            .workspace_section_evidence()
            .cache_detached_pending
            == 0
    });
    let retired = tree.section.workspace_section_evidence();
    assert_eq!(
        retired.cache_detached_rows_retired - before.cache_detached_rows_retired,
        u64::try_from(count).unwrap_or(u64::MAX),
        "the retirement removed every detached entry"
    );
    assert_eq!(retired.cache_unique_restores, before.cache_unique_restores);
    drop(tree.window);
}

/// Design D24, R12: a re-expansion before the retirement of the detached
/// cache ends caches the rows the steps left again, in slices (the
/// adoption), before the refill's first batch: the listing ends with every
/// entry cached, no location restored by the duplicate walk, no full
/// rebuild and no owner reset, and the retirement, which ends afterwards,
/// keeps every re-cached entry.
///
/// The re-expansion comes at the first dispatch after the retirement has
/// begun, or 30 ms after the input in a build too slow to begin it by then:
/// a fixed 30 ms wait let an optimized build finish the retirement first, and
/// the precondition then failed (`slice-large-directory-collapse` design R13,
/// "The P2 refill-frame profile read"). A turn retires units of 4,096 rows
/// while `LIST_TURN_SLICE_US` lasts, so the 131,072 detached rows take
/// several turns in any build.
#[test]
fn test_reexpanding_before_the_cache_retirement_ends_recaches_every_entry() {
    let count = 1 << 17;
    let tree = tree_with_siblings(count, 0);
    let nested = tree.root.join("nested");
    expand_and_load(&tree, &nested, count);
    flush_after_delay(Duration::from_millis(300));
    let before = tree.section.workspace_section_evidence();
    activate_row(&tree.section, &nested);
    let _ = dispatch_timeline(
        &tree.section,
        Duration::from_millis(30),
        Duration::ZERO,
        || {
            tree.section
                .workspace_section_evidence()
                .cache_detached_rows_retired
                > before.cache_detached_rows_retired
        },
    );
    let mid = tree.section.workspace_section_evidence();
    assert_eq!(mid.collapses.len(), 1, "{}", collapse_state(&tree.section));
    assert!(
        mid.cache_detached_pending > 0,
        "the detached cache's retirement has not ended"
    );
    activate_row(&tree.section, &nested);
    wait_until(Duration::from_secs(120), || {
        loaded_and_settled(&tree.section, &nested, count)
            && row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded())
    });
    let after = tree.section.workspace_section_evidence();
    assert_eq!(
        cached_paths(&tree.section, &nested),
        count,
        "every entry is cached again"
    );
    assert_eq!(
        after.cache_mirror_recaches, before.cache_mirror_recaches,
        "no directory re-cached whole from its mirror in one call"
    );
    assert_eq!(after.cache_full_rebuilds, before.cache_full_rebuilds);
    assert_eq!(after.cache_unique_restores, before.cache_unique_restores);
    assert_eq!(after.cache_owner_resets, before.cache_owner_resets);
    assert_eq!(after.cache_adoptions_pending, 0);
    assert!(
        after.cache_adopted_rows > before.cache_adopted_rows,
        "the adoption cached the rows the steps left"
    );
    assert_eq!(
        after.cache_collapse_rows_removed,
        before.cache_collapse_rows_removed
    );
    wait_until(Duration::from_secs(60), || {
        tree.section
            .workspace_section_evidence()
            .cache_detached_pending
            == 0
    });
    assert_eq!(
        cached_paths(&tree.section, &nested),
        count,
        "the retirement kept every re-cached entry"
    );
    drop(tree.window);
}

/// Task 1.4 (round P2's attribution, design R13): a re-expansion after the
/// collapse's bound units have removed rows of the band refills the band one
/// unit a frame (design D10), as a load into an empty directory does. The
/// condition of the slow re-expansions P2 profiled (19–34 ms frames of CSS
/// matching and shaping in the list's measure): every unbound row has left,
/// a bound unit has run, and the refill's scan result arrives at a store
/// that still lists the rest of the band. Fails with the refill's
/// reconciliation batches alone: they splice 256 rows a batch into the band,
/// and one dispatch shows well over a unit of row widgets.
#[test]
fn test_reexpanding_after_bound_units_refills_the_band_one_unit_a_frame() {
    use lushtext_core::ui::sidebar::workspace_section::StepDispatchKind as Kind;
    let count = 4096;
    let (tree, nested) = expanded_nested(count, 0);
    activate_row(&tree.section, &nested);
    pump_dispatches_until(Duration::from_secs(60), || {
        let evidence = tree.section.workspace_section_evidence();
        evidence
            .collapse_step_dispatches
            .iter()
            .any(|dispatch| dispatch.kind == Kind::Bound && dispatch.rows > 0)
    });
    let mid = tree.section.workspace_section_evidence();
    assert_eq!(mid.collapses.len(), 1, "{}", collapse_state(&tree.section));
    let changed = row_widgets_changed_per_dispatch(
        &tree.section,
        || activate_tree_row(&tree.section, &nested),
        || {
            loaded_and_settled(&tree.section, &nested, count)
                && row_for_path(&tree.section, &nested).is_some_and(|row| row.is_expanded())
        },
    );
    let over = changed
        .iter()
        .copied()
        .find(|change| change.shown > COLLAPSE_UNIT);
    assert!(
        over.is_none(),
        "one dispatch of the refill showed {over:?} row widgets, over the unit of \
         {COLLAPSE_UNIT}: {changed:?}"
    );
    assert_eq!(
        tree.section.workspace_section_evidence().last_collapse_end,
        Some(lushtext_core::ui::sidebar::workspace_section::CollapseEnd::ReExpanded)
    );
    drop(tree.window);
}

/// Design D24: a subtree another workspace folder lies inside keeps the
/// per-step cache splice instead of detaching, so the folder's path, counted
/// twice, stays exact: the collapse detaches nothing, its steps remove the
/// cache rows, and the second folder's row stays listed and expanded.
#[test]
fn test_a_subtree_holding_another_workspace_folder_is_not_detached() {
    ensure_gtk_init();
    let count = 4096;
    let dir = tempfile::tempdir().expect("overlapping folders tempdir");
    let root = dir.path().join("root");
    let big = root.join("big");
    let shared = big.join("shared");
    seed_files(&big, count);
    seed_files(&shared, 8);
    save_workspaces(vec![WorkspaceConfig::with_folders(
        WorkspaceId::new("overlap-inside"),
        "overlap-inside",
        vec![
            WorkspaceFolder::new(root.clone()),
            WorkspaceFolder::new(shared.clone()),
        ],
    )]);
    let (window, sidebar) = present_sidebar_window(800);
    let section = mapped_sections(&sidebar, 1).remove(0);
    wait_until(Duration::from_secs(30), || {
        row_for_path(&section, &root).is_some()
    });
    wait_for_refresh_idle(&section);
    row_for_path(&section, &root)
        .expect("root/")
        .set_expanded(true);
    wait_until(Duration::from_secs(30), || {
        row_for_path(&section, &big).is_some()
    });
    wait_for_refresh_idle(&section);
    row_for_path(&section, &big)
        .expect("big/")
        .set_expanded(true);
    wait_until(Duration::from_secs(60), || {
        loaded_and_settled(&section, &big, count + 1)
    });
    flush_after_delay(Duration::from_millis(300));
    let before = section.workspace_section_evidence();
    activate_row(&section, &big);
    assert_eq!(
        section.workspace_section_evidence().collapses.len(),
        1,
        "a sliced collapse: {}",
        collapse_state(&section)
    );
    wait_until(Duration::from_secs(60), || {
        collapsed_and_settled(&section, &big)
    });
    let after = section.workspace_section_evidence();
    assert_eq!(
        after.cache_detached_parents, before.cache_detached_parents,
        "nothing detached"
    );
    assert!(
        after.cache_collapse_rows_removed > before.cache_collapse_rows_removed,
        "the steps kept the cache exact"
    );
    assert_eq!(after.cache_full_rebuilds, before.cache_full_rebuilds);
    let model = section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .cloned()
        .expect("model");
    let top_level_shared = (0..model.n_items())
        .filter_map(|index| model.item(index).and_downcast::<gtk4::TreeListRow>())
        .any(|row| {
            row.depth() == 0
                && row
                    .item()
                    .and_downcast::<FileTreeItem>()
                    .and_then(|item| item.path())
                    .as_deref()
                    == Some(shared.as_path())
        });
    assert!(top_level_shared, "the second folder's row stays listed");
    drop(window);
}

/// Design D24: with two overlapping folders (`root/` and `root/shared/`)
/// both showing `shared/` loaded, the collapse of the occurrence under
/// `root/` (whose store owns the directory's path cache, having loaded
/// last) detaches nothing, so the directory keeps the per-step cache splice
/// as before D24, and the other occurrence keeps listing every entry. Fails
/// when only a folder strictly inside the subtree excludes it: the collapse
/// detached the shared directory's cache.
#[test]
fn test_collapsing_one_of_two_listed_occurrences_detaches_nothing() {
    ensure_gtk_init();
    let count = 4096;
    let dir = tempfile::tempdir().expect("overlapping folders tempdir");
    let root = dir.path().join("root");
    let shared = root.join("shared");
    seed_files(&shared, count);
    save_workspaces(vec![WorkspaceConfig::with_folders(
        WorkspaceId::new("overlap-both"),
        "overlap-both",
        // `shared/` first: C3 collapses directly a subtree followed by more
        // than one unit of listed rows, and this test needs the collapse
        // under `root/` sliced, so `root/` is the last folder.
        vec![
            WorkspaceFolder::new(shared.clone()),
            WorkspaceFolder::new(root.clone()),
        ],
    )]);
    let (window, sidebar) = present_sidebar_window(800);
    let section = mapped_sections(&sidebar, 1).remove(0);
    let row_at_depth = |path: &Path, depth: u32| {
        let model = section.imp().tree_model.borrow().as_ref().cloned()?;
        (0..model.n_items())
            .filter_map(|index| model.item(index).and_downcast::<gtk4::TreeListRow>())
            .find(|row| {
                row.depth() == depth
                    && row
                        .item()
                        .and_downcast::<FileTreeItem>()
                        .and_then(|item| item.path())
                        .as_deref()
                        == Some(path)
            })
    };
    let listed = |row: &gtk4::TreeListRow| {
        row.children()
            .and_downcast::<gtk4::gio::ListStore>()
            .map_or(0, |store| store.n_items())
            == u32::try_from(count).unwrap_or(u32::MAX)
    };
    wait_until(Duration::from_secs(30), || {
        row_at_depth(&root, 0).is_some() && row_at_depth(&shared, 0).is_some()
    });
    wait_for_refresh_idle(&section);
    // The top-level `shared/` occurrence loads first, the one under `root/`
    // last, so the latter's store owns the directory's cache.
    let outer = row_at_depth(&shared, 0).expect("the shared/ folder row");
    outer.set_expanded(true);
    wait_until(Duration::from_secs(60), || {
        listed(&outer)
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    row_at_depth(&root, 0).expect("root/").set_expanded(true);
    wait_until(Duration::from_secs(30), || {
        row_at_depth(&shared, 1).is_some()
    });
    wait_for_refresh_idle(&section);
    let inner = row_at_depth(&shared, 1).expect("shared/ under root/");
    inner.set_expanded(true);
    wait_until(Duration::from_secs(60), || {
        listed(&inner)
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    flush_after_delay(Duration::from_millis(300));
    let before = section.workspace_section_evidence();
    // The cache owner (design D24, "As built"): the directory has one, the
    // store that loaded last, the occurrence under `root/`.
    assert_eq!(
        before
            .directory_listings
            .iter()
            .filter(|listing| listing.path == shared && listing.cached_paths == count)
            .count(),
        1,
        "one occurrence owns the shared directory's path cache"
    );
    section
        .imp()
        .file_tree_view
        .emit_by_name::<()>("activate", &[&inner.position()]);
    assert_eq!(
        section.workspace_section_evidence().collapses.len(),
        1,
        "a sliced collapse: {}",
        collapse_state(&section)
    );
    wait_until(Duration::from_secs(60), || {
        !inner.is_expanded()
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    wait_until(Duration::from_secs(60), || {
        section.workspace_section_evidence().cache_detached_pending == 0
    });
    flush_after_delay(Duration::from_millis(300));
    let after = section.workspace_section_evidence();
    assert!(outer.is_expanded() && listed(&outer));
    assert_eq!(
        after.cache_detached_parents, before.cache_detached_parents,
        "nothing detached"
    );
    // Task 10.15's overlap invariant, as built: the directory has one cache
    // owner, the collapsed occurrence's store, whose steps splice its paths
    // out of the cache (nothing was detached), so the surviving occurrence
    // lists every entry with no cache of its own until its next batch
    // re-caches it from its mirror (design R11, D24 "As built"; the same on
    // `main` by construction). Every row operation through it must still
    // land: a delete through the survivor, twice, finds and removes each row
    // through the store walk behind a cache miss.
    let survivor_cached = cached_paths(&section, &shared);
    assert_eq!(
        survivor_cached, 0,
        "the collapsed occurrence owned the cache and its steps spliced it out, so the \
         survivor owns none until its next batch"
    );
    let survivor_store = outer
        .children()
        .and_downcast::<gtk4::gio::ListStore>()
        .expect("the survivor's store");
    for (deleted_so_far, index) in [7, 4000].into_iter().enumerate() {
        let deleted = shared.join(file_name(index));
        fixture::remove_file(&deleted);
        assert!(
            section.remove_from_model(&deleted),
            "a delete through the surviving occurrence finds its row (cached paths \
             before: {survivor_cached})"
        );
        assert_eq!(
            survivor_store.n_items(),
            u32::try_from(count - deleted_so_far - 1).unwrap_or(u32::MAX),
            "the survivor lists one entry less per delete"
        );
        assert!(
            (0..survivor_store.n_items())
                .filter_map(|position| survivor_store.item(position).and_downcast::<FileTreeItem>())
                .all(|item| item.path().as_deref() != Some(deleted.as_path())),
            "the deleted entry's row is gone from the survivor"
        );
    }
    assert!(outer.is_expanded(), "the survivor stays expanded");
    drop(window);
}

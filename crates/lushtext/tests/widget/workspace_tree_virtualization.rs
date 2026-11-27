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
use lushtext_model::workspace::{WorkspaceConfig, WorkspaceFolder, WorkspaceId, WorkspacesFile};
use lushtext_services::{persistence::json_store, workspace::workspace_manager};
use lushtext_ui::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_ui::ui::sidebar::LushtextSidebar;
use lushtext_ui::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_ui::ui::sidebar::policy::WorkspaceRevealStatus;
use lushtext_ui::ui::sidebar::workspace_section::LushtextWorkspaceSection;
use lushtext_ui::ui::window::LushtextWindow;
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
    lushtext_ui::ui::sidebar::set_sidebar_directory_retained_bytes_for_test(bytes);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
    lushtext_ui::ui::sidebar::set_sidebar_directory_retained_bytes_for_test(0);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// The listing evidence of `directory`, if the section has materialized it.
fn maybe_listing(
    section: &LushtextWorkspaceSection,
    directory: &Path,
) -> Option<lushtext_ui::ui::sidebar::evidence::DirectoryListingEvidence> {
    section
        .workspace_section_evidence()
        .directory_listings
        .into_iter()
        .find(|listing| listing.path == directory)
}

/// The listing evidence of `directory`, from the section's plain mirrors.
fn listing(
    section: &LushtextWorkspaceSection,
    directory: &Path,
) -> lushtext_ui::ui::sidebar::evidence::DirectoryListingEvidence {
    maybe_listing(section, directory).expect("the directory is materialized")
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
    let settings = lushtext_ui::ui::settings_writes::shared();
    settings.reset(lushtext_ui::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
    for show_hidden in [true, false] {
        settings
            .set_boolean(
                lushtext_ui::config::keys::WORKSPACE_SHOW_HIDDEN_FILES,
                show_hidden,
            )
            .expect("set show-hidden key");
        wait_for_refresh_idle(&tree.section);
        scroll_outer_to(&tree.sidebar, 0.0);
        assert_sidebar_rests_at_top(&tree.sidebar, "after toggling hidden files");
        assert_header_visible(&tree.sidebar, &tree.section, "after toggling hidden files");
    }
    settings.reset(lushtext_ui::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
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

    let settings = lushtext_ui::ui::settings_writes::shared();
    settings.reset(lushtext_ui::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
    for show_hidden in [true, false] {
        settings
            .set_boolean(
                lushtext_ui::config::keys::WORKSPACE_SHOW_HIDDEN_FILES,
                show_hidden,
            )
            .expect("set show-hidden key");
        assert_rows_settle(&tree.section, "after toggling hidden files");
    }
    settings.reset(lushtext_ui::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
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
    scope: lushtext_model::workspace::WorkspaceScope,
    show_hidden: bool,
    expanded: Vec<std::collections::HashSet<PathBuf>>,
}

fn rest_state(sidebar: &LushtextSidebar) -> SidebarRestState {
    let sections = sidebar.imp().sections.borrow().clone();
    let settings = lushtext_ui::ui::settings_writes::shared();
    SidebarRestState {
        selected: sections
            .iter()
            .map(|section| selection(section).selected())
            .collect(),
        outer: outer_adjustment(sidebar).value(),
        scope: sidebar.current_scope(),
        show_hidden: settings.boolean(lushtext_ui::config::keys::WORKSPACE_SHOW_HIDDEN_FILES),
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
    let settings = lushtext_ui::ui::settings_writes::shared();
    settings.reset(lushtext_ui::config::keys::WORKSPACE_SHOW_HIDDEN_FILES);
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
        .and_downcast::<lushtext_ui::app::LushtextApplication>()
        .expect("test window belongs to a LushtextApplication");
    let surface = |snapshot: &lushtext_model::automation::AutomationSnapshot, name: &str| {
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
    let before = lushtext_ui::ui::automation::app_snapshot(&app);
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
    let after = lushtext_ui::ui::automation::app_snapshot(&app);
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
    use lushtext_model::budgets::responsiveness_budget::B_SETTLE_US;
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
            maybe_listing(&tree.section, &nested)
                .is_some_and(|entry| entry.directory_retained_entries == count)
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
                worst.as_micros() <= u128::from(B_SETTLE_US.as_us()),
                "a dispatch of the {count}-entry expansion took {} us, over B_SETTLE_US",
                worst.as_micros()
            );
        }
        // Collapse and expand again: the new store takes the directory's
        // path caches over from the old one, forgetting its rows in slices.
        // The collapse is GTK's synchronous `set_expanded(false)`, whose cost
        // follows the rows it removes (ledger A91); printed, not enforced
        // (the recorded toolkit-floor exception in the design).
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
            maybe_listing(&tree.section, &nested)
                .is_some_and(|entry| entry.directory_retained_entries == count)
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
    use lushtext_ui::ui::sidebar::policy::{
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
        maybe_listing(&tree.section, &nested)
            .is_some_and(|entry| entry.directory_retained_entries == ROWS)
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
        maybe_listing(&tree.section, directory)
            .is_some_and(|entry| entry.directory_retained_entries == count)
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

/// A presented tree whose `nested/` (with `count` entries, `siblings` files
/// after it) is expanded and fully listed.
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

/// Moving the focus out of an inline-rename entry ends the rename without a
/// toolkit CRITICAL. The entry's focus-leave handler used to remove the entry
/// from its row inside GTK's focus crossing, so the crossing's walk from the
/// old focus to the common ancestor (`gtkwindow.c`,
/// `synthesize_focus_change_events`: `for (w = old_focus; w != ancestor;
/// w = gtk_widget_get_parent (w))`) never met the ancestor and spun on
/// `gtk_widget_get_parent: assertion 'GTK_IS_WIDGET (widget)' failed`
/// forever (originating in `slice-large-directory-collapse` task 9.12). Three
/// cases: the list's own `scroll_to(FOCUS)` onto another row and onto the
/// renamed row (the common ancestor is then the new focus itself), and a
/// button outside the list.
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

/// A background refresh of the directory being renamed restores the selection
/// without taking the focus from the inline-rename entry, so the rename
/// survives it (before `restore_selection`, `scroll_to(FOCUS)` moved the
/// focus to the row, which ended the rename inside the focus crossing;
/// originating in `slice-large-directory-collapse` task 9.12).
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
/// refresh (originating in `slice-large-directory-collapse`, design R13).
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
/// created on demand (D17): the overlay, the expander and its indent
/// and disclosure icons, the content box, the open indicator, the icon and the
/// label (down from 16 at depth one).
const LIGHT_FILE_ROW_WIDGETS: usize = 9;

/// D17: a file row carries only what every row shows; the drag handle, the
/// Focus Folder button and the three drop surfaces exist only on rows that use
/// them.
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

/// D17: rows are created without the reorder shield, so a reorder drag gives
/// one to every realized row when it starts, file rows included, before GTK can
/// deliver drag hover to an expander.
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
    let folder_id = lushtext_model::workspace::WorkspaceFolderId::new("drag-start");
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

/// Design R4, D20: the folder header collapses every expanded root, also one
/// whose row no widget showed (a row object nobody hooked, ledger A114), and
/// a later refresh restores none; a directory under a root collapsed
/// through such a row (a failed reveal's own collapse) leaves the expanded
/// paths and the watch targets as a collapse through its bound row does.
/// Fails before the fix: the far root's collapse notified nobody, and the
/// deferred restore expanded it again.
#[test]
fn test_the_folder_header_collapses_every_root() {
    use lushtext_services::watch::workspace_watch::WorkspaceWatchTarget;
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
        maybe_listing(&section, &first).is_some_and(|l| l.directory_retained_entries == 4096)
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
        row_for_path(&section, &first).is_some_and(|row| !row.is_expanded())
            && row_for_path(&section, &second).is_some_and(|row| !row.is_expanded())
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    let collapsed_evidence = section.workspace_section_evidence();
    for root in [&first, &second] {
        assert!(
            !collapsed_evidence.expanded_paths.contains(root),
            "{} must leave expanded_paths immediately on collapse (got {:?})",
            root.display(),
            collapsed_evidence.expanded_paths
        );
    }
    section.queue_auto_refresh_for_test(vec![first.clone(), second.clone()]);
    section.apply_queued_refresh_for_test();
    wait_for_refresh_idle(&section);
    flush_after_delay(Duration::from_millis(500));
    let evidence = section.workspace_section_evidence();
    for root in [&first, &second] {
        assert!(
            row_for_path(&section, root).is_some_and(|row| !row.is_expanded()),
            "{} stays collapsed (expanded_paths={:?})",
            root.display(),
            evidence.expanded_paths
        );
        assert!(!evidence.expanded_paths.contains(root));
    }
    // A directory under a root, expanded and collapsed through row objects no
    // widget shows: a reveal of an existing file below it records small/ in
    // expanded_paths and watch_targets; then scrolling back to the top and
    // revealing a missing path below a sibling directory (or collapsing) leaves
    // expanded_paths and watch_targets clean.
    section.expand_folders();
    wait_until(Duration::from_secs(60), || {
        maybe_listing(&section, &first).is_some_and(|l| l.directory_retained_entries == 4096)
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
    // First, expanding small/ via a reveal of an offscreen sibling's child must
    // record small/ in expanded_paths and watch_targets even before small/ is scrolled into view;
    // then a failed reveal of small/missing/file.txt expands small/ and rolls it back cleanly.
    activate_reveal_workspace_path(&window, &small.join("row-00000.txt"));
    wait_until(Duration::from_secs(30), || {
        !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
            && !sidebar.workspace_tree_evidence().reveal_pending
    });
    flush_after_delay(Duration::from_millis(500));
    let revealed_evidence = section.workspace_section_evidence();
    assert!(
        revealed_evidence.expanded_paths.contains(&small),
        "successful reveal must record offscreen ancestor {} in expanded_paths ({:?})",
        small.display(),
        revealed_evidence.expanded_paths
    );
    assert!(
        revealed_evidence
            .watch_targets
            .contains(&WorkspaceWatchTarget::directory(small.clone())),
        "successful reveal must watch offscreen ancestor {} ({:?})",
        small.display(),
        revealed_evidence.watch_targets
    );
    // Collapse back and verify a failed reveal of small/missing/file.txt while small/
    // is offscreen expands small/, fails NotFound, and rolls back expanded_paths + watch_targets.
    section.collapse_folders();
    wait_until(Duration::from_secs(60), || {
        row_for_path(&section, &first).is_some_and(|row| !row.is_expanded())
            && row_for_path(&section, &second).is_some_and(|row| !row.is_expanded())
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    section.expand_folders();
    wait_until(Duration::from_secs(60), || {
        maybe_listing(&section, &first).is_some_and(|l| l.directory_retained_entries == 4096)
            && row_for_path(&section, &second).is_some_and(|row| row.is_expanded())
            && !section
                .workspace_section_evidence()
                .refresh_blocks_readiness
    });
    scroll_outer_to(&sidebar, 0.0);
    assert!(
        expander_for(&section, &small).is_none(),
        "small/ lies outside the band"
    );
    activate_reveal_workspace_path(&window, &small.join("missing").join("file.txt"));
    wait_until(Duration::from_secs(30), || {
        !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
            && !sidebar.workspace_tree_evidence().reveal_pending
    });
    flush_after_delay(Duration::from_millis(500));
    let tree_evidence = sidebar.workspace_tree_evidence();
    assert_eq!(
        tree_evidence.reveal_status,
        WorkspaceRevealStatus::NotFound,
        "the reveal expanded small/ and failed on missing/file.txt"
    );
    let evidence = section.workspace_section_evidence();
    assert!(
        row_for_path(&section, &small).is_some_and(|row| !row.is_expanded()),
        "small/ stays collapsed after reveal rollback (expanded_paths={:?})",
        evidence.expanded_paths
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

// ---------------------------------------------------------------------------
// D10 expansion warm-up cap and P2 kept-run refill (`scan_execution.rs`).
// ---------------------------------------------------------------------------

/// Rows per warm-up batch while an empty directory (or kept-run refill) brings
/// row widgets into the realized band (`FIRST_BATCH_ROWS` in `scan_execution.rs`).
const EXPANSION_WARM_UP_UNIT: usize = 32;

/// The tree row each realized row content widget shows, keyed by the widget's
/// address.
type BoundRows = std::collections::HashMap<usize, usize>;

#[derive(Clone, Copy, Debug, Default)]
struct RowChange {
    shown: usize,
    destroyed: usize,
}

/// Record which tree row every realized row content widget (the overlay the
/// factory's `setup` creates) shows now; return how many widgets are new or
/// show another row than at the last call (`shown`), and how many were
/// destroyed (`destroyed`).
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
            "reconciliation did not settle within two minutes"
        );
        if context.iteration(false) {
            created.push(mark_new_row_widgets(section, &mut seen));
        } else {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
    created
}

fn activate_tree_row(section: &LushtextWorkspaceSection, path: &Path) {
    let position = tree_index_for_path(section, path).expect("the row is listed");
    section
        .imp()
        .file_tree_view
        .emit_by_name::<()>("activate", &[&position]);
}

fn loaded_and_settled(section: &LushtextWorkspaceSection, path: &Path, count: usize) -> bool {
    maybe_listing(section, path).is_some_and(|listing| listing.directory_retained_entries == count)
        && !section
            .workspace_section_evidence()
            .refresh_blocks_readiness
}

fn tree_with_siblings(count: usize, siblings: usize) -> LargeTree {
    let (dir, root) = seed_workspace(&[("nested", count)]);
    for index in 0..siblings {
        fixture::write_text(&root.join(format!("zz-{index:04}.txt")), "x");
    }
    let tree = present_single_folder(dir, root, "virtualized", 800);
    tree.section.expand_folders();
    tree
}

fn drawn_entries(section: &LushtextWorkspaceSection) -> BTreeSet<String> {
    rendered_labels(section)
        .into_iter()
        .filter(|label| label.starts_with("row-"))
        .collect()
}

/// D10: expanding an empty directory brings at most `FIRST_BATCH_ROWS` (32)
/// row widgets into the realized band per dispatch/frame during warm-up, and
/// the first frame after the first batch paints the top of the listing.
#[test]
fn test_expanding_a_directory_brings_at_most_one_unit_into_the_band_per_frame() {
    for (count, siblings) in [(256usize, 0usize), (256, 400), (4096, 0)] {
        let tree = tree_with_siblings(count, siblings);
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
        let frames = std::rc::Rc::new(std::cell::RefCell::new(
            Vec::<(u32, BTreeSet<String>)>::new(),
        ));
        let handler = {
            let frames = std::rc::Rc::clone(&frames);
            let section = tree.section.clone();
            let nested = nested.clone();
            clock.connect_paint(move |_| {
                let listed = row_for_path(&section, &nested)
                    .and_then(|row| row.children())
                    .map_or(0, |store| store.n_items());
                frames.borrow_mut().push((listed, drawn_entries(&section)));
            })
        };
        let changed = row_widgets_changed_per_dispatch(
            &tree.section,
            || activate_tree_row(&tree.section, &nested),
            || loaded_and_settled(&tree.section, &nested, count),
        );
        clock.disconnect(handler);
        // Directories above `CHILD_APPEND_BATCH_SIZE` (256) also append one
        // `Loading` placeholder row in the first dispatch (`32 + 1 = 33`).
        let first_limit = EXPANSION_WARM_UP_UNIT + usize::from(count > 256);
        let mut saw_loading_row_dispatch = false;
        let over = changed.iter().copied().find(|change| {
            if change.shown > EXPANSION_WARM_UP_UNIT
                && change.shown <= first_limit
                && !saw_loading_row_dispatch
            {
                saw_loading_row_dispatch = true;
                return false;
            }
            change.shown > EXPANSION_WARM_UP_UNIT
        });
        assert!(
            over.is_none(),
            "{count} entries / {siblings} siblings: one dispatch showed {over:?} row widgets, \
             over the warm-up unit of {EXPANSION_WARM_UP_UNIT}: {changed:?}"
        );
        flush_after_delay(Duration::from_millis(100));
        let settled = drawn_entries(&tree.section);
        assert!(!settled.is_empty(), "the loaded entries are drawn");
        let frames = frames.borrow().clone();
        let first = frames
            .iter()
            .position(|(listed, _)| *listed > 0)
            .expect("a frame painted the loaded rows");
        let first_unit: BTreeSet<String> = (0..EXPANSION_WARM_UP_UNIT).map(file_name).collect();
        let expected: BTreeSet<String> = settled.intersection(&first_unit).cloned().collect();
        assert!(
            !expected.is_empty() && frames[first].1.is_superset(&expected),
            "{count} entries / {siblings} siblings: the frame after the first batch drew {:?}, \
             the loaded tree {settled:?}",
            frames[first].1
        );
        drop(tree.window);
    }
}

/// P2: refilling a materialized directory around a contiguous interior subset
/// (`kept_at`) keeps the existing realized/bound row items and widgets alive
/// without tearing them down, and paces new rows in `<= 32` widget units.
#[test]
fn test_refilling_around_a_kept_subset_preserves_kept_widgets_and_paces_new_rows() {
    use glib::translate::ToGlibPtr;
    let (dir, root) = seed_workspace(&[]);
    let nested = root.join("nested");
    fixture::create_dir_all(&nested);
    for index in 128..192usize {
        fixture::write_text(&nested.join(format!("f-{index:04}.txt")), "x");
    }
    let tree = present_single_folder(dir, root, "virtualized", 800);
    tree.section.expand_folders();
    expand_and_load(&tree, &nested, 64);
    flush_after_delay(Duration::from_millis(300));

    let child_store = row_for_path(&tree.section, &nested)
        .and_then(|row| row.children())
        .and_downcast::<gtk4::gio::ListStore>()
        .expect("nested child ListStore");
    assert_eq!(child_store.n_items(), 64);

    // Record the exact FileTreeItem instances for f-0128..f-0192 and the
    // realized row content widgets currently bound to them.
    let kept_items: Vec<glib::WeakRef<FileTreeItem>> = (0..64)
        .map(|i| {
            child_store
                .item(i)
                .and_downcast::<FileTreeItem>()
                .expect("FileTreeItem")
                .downgrade()
        })
        .collect();
    let mut kept_widgets_before: std::collections::HashMap<
        PathBuf,
        (usize, glib::WeakRef<gtk4::Widget>),
    > = std::collections::HashMap::new();
    let mut child = tree.section.imp().file_tree_view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        let Some(content) = widget.first_child() else {
            continue;
        };
        if let Some(expander) =
            find_descendant(&content, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
                .and_downcast::<gtk4::TreeExpander>()
            && let Some(row) = expander.list_row()
            && let Some(item) = row.item().and_downcast::<FileTreeItem>()
            && let Some(path) = item.path()
            && path.parent() == Some(nested.as_path())
        {
            let ptr: *mut glib::gobject_ffi::GObject =
                content.upcast_ref::<glib::Object>().to_glib_none().0;
            kept_widgets_before.insert(path, (ptr as usize, content.downgrade()));
        }
    }
    assert_eq!(
        kept_widgets_before.len(),
        64,
        "all 64 initial interior rows should be realized and bound"
    );

    // Add entries before (0..128) and after (192..256) on disk so `kept_at == Some(128)`.
    for index in (0..128usize).chain(192..256usize) {
        fixture::write_text(&nested.join(format!("f-{index:04}.txt")), "x");
    }

    let changed = row_widgets_changed_per_dispatch(
        &tree.section,
        || {
            tree.section
                .queue_auto_refresh_for_test(vec![nested.clone()]);
            tree.section.apply_queued_refresh_for_test();
        },
        || loaded_and_settled(&tree.section, &nested, 256),
    );

    let over = changed
        .iter()
        .copied()
        .find(|change| change.shown > EXPANSION_WARM_UP_UNIT);
    assert!(
        over.is_none(),
        "kept-run refill showed {over:?} row widgets in one dispatch, over {EXPANSION_WARM_UP_UNIT}: {changed:?}"
    );
    let total_destroyed: usize = changed.iter().map(|change| change.destroyed).sum();
    // Every kept item must survive intact at positions 128..192 in the child store.
    assert_eq!(child_store.n_items(), 256);
    for (offset, weak) in kept_items.iter().enumerate() {
        let original = weak.upgrade().unwrap_or_else(|| {
            panic!(
                "kept item f-{:04}.txt was dropped instead of kept",
                128 + offset
            )
        });
        let current = child_store
            .item(u32::try_from(128 + offset).expect("u32"))
            .and_downcast::<FileTreeItem>()
            .expect("FileTreeItem");
        assert_eq!(
            original, current,
            "kept item at offset {offset} must be the exact same FileTreeItem instance"
        );
    }
    // Every kept row still bound in the realized band must keep its exact same
    // row content widget (never torn down and re-created).
    let mut kept_still_bound = 0usize;
    let mut child = tree.section.imp().file_tree_view.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        let Some(content) = widget.first_child() else {
            continue;
        };
        if let Some(expander) =
            find_descendant(&content, glib::object::ObjectExt::is::<gtk4::TreeExpander>)
                .and_downcast::<gtk4::TreeExpander>()
            && let Some(row) = expander.list_row()
            && let Some(item) = row.item().and_downcast::<FileTreeItem>()
            && let Some(path) = item.path()
            && let Some((old_ptr, old_weak)) = kept_widgets_before.get(&path)
        {
            kept_still_bound += 1;
            let new_ptr: *mut glib::gobject_ffi::GObject =
                content.upcast_ref::<glib::Object>().to_glib_none().0;
            assert!(
                old_weak.upgrade().is_some() && *old_ptr == new_ptr as usize,
                "kept row {} had its widget replaced (old={old_ptr:#x}, new={:#x}, total_destroyed={total_destroyed})",
                path.display(),
                new_ptr as usize,
            );
        }
    }
    assert!(
        kept_still_bound > 0,
        "at least part of the kept run must remain inside the realized band (total_destroyed={total_destroyed})"
    );
    drop(tree.window);
}

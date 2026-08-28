// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextSidebar multi-workspace orchestrator.

use crate::common::{
    ensure_gtk_init, fixture, flush_after_delay, flush_events, present_window, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::workspace::{WorkspaceConfig, WorkspaceId, WorkspacesFile};
use lushtext_core::services::{json_store, workspace_manager};
use lushtext_core::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_core::ui::sidebar::LushtextSidebar;
use lushtext_core::ui::window::LushtextWindow;
use std::time::Duration;

const WARNING_BAR_ROW_HEIGHT: i32 = 54;

/// Create a window attached to a test application.
fn test_window() -> LushtextWindow {
    crate::common::test_window()
}

// --- Sidebar construction ---

#[test]
fn test_sidebar_new() {
    ensure_gtk_init();
    let _sidebar = LushtextSidebar::new();
}

#[test]
fn test_sidebar_workspace_filter_defaults_to_all_workspaces() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    let model = sidebar
        .imp()
        .workspace_filter_dropdown
        .model()
        .and_downcast::<gtk4::StringList>()
        .expect("workspace filter should use a StringList model");
    assert_eq!(model.n_items(), 1);
    assert_eq!(
        model.string(0).expect("All workspaces option should exist").as_str(),
        "All workspaces"
    );
    assert_eq!(sidebar.imp().workspace_filter_dropdown.selected(), 0);
}

#[test]
fn test_sidebar_starts_with_no_sections() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert!(sidebar.imp().sections.borrow().is_empty());
}

#[test]
fn test_sidebar_new_workspace_button_exists() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    let _button = &sidebar.imp().new_workspace_button;
}

#[test]
fn test_sidebar_selector_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::ComboBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*sidebar.imp().workspace_filter_dropdown);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*sidebar.imp().new_workspace_button);
}

#[test]
fn test_sidebar_selector_row_uses_workspace_tree_left_inset() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(sidebar.imp().new_workspace_box.margin_start(), 6);
}

#[test]
fn test_sidebar_new_workspace_button_carries_vertical_spacing() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().new_workspace_button.icon_name().as_deref(),
        Some("folder-new-symbolic")
    );
    assert_eq!(sidebar.imp().new_workspace_button.valign(), gtk4::Align::Center);
    assert_eq!(sidebar.imp().new_workspace_button.margin_top(), 6);
    assert_eq!(sidebar.imp().new_workspace_button.margin_bottom(), 6);
}

#[test]
fn test_sidebar_workspace_list_revealer_uses_crossfade() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().workspace_list_revealer.transition_type(),
        gtk4::RevealerTransitionType::Crossfade,
    );
    assert_eq!(sidebar.imp().workspace_list_revealer.transition_duration(), 250);
    assert!(sidebar.imp().workspace_list_revealer.reveals_child());
}

#[test]
fn test_workspace_filter_can_show_only_one_workspace() {
    ensure_gtk_init();
    let _folders_dir = seed_restored_workspaces();

    let window = test_window();
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().sections.borrow().len() == 3
    });
    let dropdown = &window.imp().sidebar.imp().workspace_filter_dropdown;
    let model = dropdown
        .model()
        .and_downcast::<gtk4::StringList>()
        .expect("workspace filter should use a StringList model");
    assert_eq!(model.n_items(), 4);
    assert_eq!(
        model.string(0).expect("All workspaces option should exist").as_str(),
        "All workspaces"
    );
    assert_eq!(model.string(1).expect("first workspace option should exist").as_str(), "one");
    assert_eq!(model.string(2).expect("second workspace option should exist").as_str(), "two");
    assert_eq!(model.string(3).expect("third workspace option should exist").as_str(), "three");

    dropdown.set_selected(2);
    flush_after_delay(Duration::from_millis(300));

    wait_until(Duration::from_secs(3), || {
        let sidebar = window.imp().sidebar.imp();
        let revealer = &sidebar.workspace_list_revealer;
        let sections = sidebar.sections.borrow();
        revealer.reveals_child()
            && revealer.is_child_revealed()
            && !sections[0].property::<bool>("visible")
            && sections[1].property::<bool>("visible")
            && !sections[2].property::<bool>("visible")
    });

    dropdown.set_selected(0);
    flush_after_delay(Duration::from_millis(300));
    wait_until(Duration::from_secs(3), || {
        let sidebar = window.imp().sidebar.imp();
        let revealer = &sidebar.workspace_list_revealer;
        let sections = sidebar.sections.borrow();
        revealer.reveals_child()
            && revealer.is_child_revealed()
            && sections.iter().all(|section| section.property::<bool>("visible"))
    });
}

#[test]
fn test_new_workspace_affordance_stays_above_sections_scroll_area() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let first = sidebar
        .first_child()
        .expect("first child is the fixed new-workspace box");
    let separator_after_top = first
        .next_sibling()
        .expect("separator follows the new-workspace box");
    let revealer = separator_after_top
        .next_sibling()
        .and_downcast::<gtk4::Revealer>()
        .expect("workspace list revealer sits below the fixed top row");
    let scroller = revealer
        .child()
        .and_downcast::<gtk4::ScrolledWindow>()
        .expect("workspace scroller should be the revealer child");

    assert!(first.is::<gtk4::Box>());
    assert_eq!(first.as_ptr(), sidebar.imp().new_workspace_button.parent().expect("expected operation to succeed").as_ptr());
    assert!(separator_after_top.is::<gtk4::Separator>());
    assert_eq!(
        revealer.as_ptr(),
        sidebar.imp().workspace_list_revealer.as_ptr()
    );
    assert_eq!(scroller.as_ptr(), sidebar.imp().outer_scrolled_window.as_ptr());
    assert!(revealer.next_sibling().is_none());
}

#[test]
fn test_sidebar_outer_scroller_disables_horizontal_scrollbar() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();
    assert_eq!(
        sidebar.imp().outer_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );
    assert!(!sidebar.imp().outer_scrolled_window.propagates_natural_width());
}

#[test]
fn test_dense_workspace_sections_scroll_below_fixed_selector() {
    ensure_gtk_init();
    const WORKSPACE_COUNT: usize = 18;
    let _folders_dir = seed_dense_workspace_sections(WORKSPACE_COUNT);

    let window = test_window();
    window.set_default_size(360, 320);
    present_window(&window);

    wait_until(Duration::from_secs(3), || {
        let sidebar = window.imp().sidebar.imp();
        let adjustment = sidebar.outer_scrolled_window.vadjustment();
        sidebar.sections.borrow().len() == WORKSPACE_COUNT
            && sidebar.new_workspace_box.height() > 0
            && sidebar.outer_scrolled_window.height() > 0
            && adjustment.upper() > adjustment.page_size() + 1.0
    });

    let sidebar = window.imp().sidebar.upcast_ref::<gtk4::Widget>();
    let sidebar_imp = window.imp().sidebar.imp();
    let selector_bounds = sidebar_imp
        .new_workspace_box
        .compute_bounds(sidebar)
        .expect("fixed selector should have sidebar-relative bounds");
    let scroller_bounds = sidebar_imp
        .outer_scrolled_window
        .compute_bounds(sidebar)
        .expect("workspace list scroller should have sidebar-relative bounds");
    assert!(
        scroller_bounds.y() >= selector_bounds.y() + selector_bounds.height() - 1.0,
        "workspace sections should scroll below the fixed selector (selector y={} h={}, scroller y={})",
        selector_bounds.y(),
        selector_bounds.height(),
        scroller_bounds.y()
    );
    assert_eq!(
        sidebar_imp.outer_scrolled_window.hscrollbar_policy(),
        gtk4::PolicyType::Never
    );

    let adjustment = sidebar_imp.outer_scrolled_window.vadjustment();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    flush_events();

    let selector_after_scroll = sidebar_imp
        .new_workspace_box
        .compute_bounds(sidebar)
        .expect("fixed selector should remain allocated after list scroll");
    assert!(sidebar_imp.new_workspace_box.is_visible());
    assert_eq!(
        selector_bounds.y(),
        selector_after_scroll.y(),
        "scrolling the workspace sections should not move the fixed selector row"
    );
}

#[test]
fn test_sidebar_new_workspace_affordance_matches_document_restored_warning_height() {
    ensure_gtk_init();
    let window = test_window();
    window.set_default_size(1200, 800);
    present_window(&window);

    wait_until(Duration::from_secs(2), || {
        window.imp().sidebar.imp().new_workspace_box.height() > 0
    });

    let sidebar_height = window.imp().sidebar.imp().new_workspace_box.height();
    assert_eq!(
        sidebar_height, WARNING_BAR_ROW_HEIGHT,
        "new workspace affordance height should preserve the warning-bar sizing contract (sidebar={sidebar_height}, expected={WARNING_BAR_ROW_HEIGHT})",
    );
}

#[test]
fn test_sidebar_has_no_persistent_width_footer_controls() {
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let mut children = Vec::new();
    let mut child = sidebar.first_child();
    while let Some(widget) = child {
        children.push(widget.clone());
        child = widget.next_sibling();
    }

    assert_eq!(children.len(), 3);
    assert!(children[0].is::<gtk4::Box>());
    assert!(children[1].is::<gtk4::Separator>());
    assert!(children[2].is::<gtk4::Revealer>());
}

fn seed_restored_workspaces() -> tempfile::TempDir {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("workspace folders tempdir");
    let mut workspaces = WorkspacesFile::default();

    for (idx, name) in ["one", "two", "three"].into_iter().enumerate() {
        let path = folders_dir.path().join(name);
        fixture::create_dir_all(&path);
        workspaces.workspaces.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new(format!("ws-{idx}")),
            name,
            path,
        ));
    }

    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces.json");
    folders_dir
}

fn seed_dense_workspace_sections(count: usize) -> tempfile::TempDir {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("dense workspace folders tempdir");
    let mut workspaces = WorkspacesFile::default();

    for idx in 0..count {
        let name = format!("dense-{idx:02}");
        let path = folders_dir.path().join(&name);
        fixture::create_dir_all(&path);
        workspaces.workspaces.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new(format!("ws-dense-{idx:02}")),
            name,
            path,
        ));
    }

    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces.json");
    folders_dir
}

// --- Window integration: tab path updates ---

#[test]
fn test_update_tab_path_exact_match() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let old_path = dir.path().join("old.rs");
    fixture::write_text(&old_path, "fn main() {}");
    window.open_document(&old_path);

    let new_path = dir.path().join("new.rs");
    window.update_tab_path(&old_path, &new_path);

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "new.rs");
}

#[test]
fn test_update_tab_path_directory_prefix_rewrite() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let old_dir = dir.path().join("old_dir");
    fixture::create_dir(&old_dir);
    let file_path = old_dir.join("file.rs");
    fixture::write_text(&file_path, "content");
    window.open_document(&file_path);

    let new_dir = dir.path().join("new_dir");
    window.update_tab_path(&old_dir, &new_dir);

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "file.rs");

    let editor = page
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .expect("expected operation to succeed");
    assert_eq!(editor.file_path().expect("expected operation to succeed"), new_dir.join("file.rs"));
}

#[test]
fn test_update_tab_path_no_match_is_noop() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file_path = dir.path().join("keep.rs");
    fixture::write_text(&file_path, "content");
    window.open_document(&file_path);

    window.update_tab_path(
        std::path::Path::new("/tmp/other.rs"),
        std::path::Path::new("/tmp/renamed.rs"),
    );

    let page = window.imp().tab_view.nth_page(0);
    assert_eq!(page.title().as_str(), "keep.rs");
}

fn assert_tab_count(window: &LushtextWindow, expected: i32) {
    assert_eq!(
        window.imp().tab_view.n_pages(),
        expected,
        "expected {expected} open tab(s), got {}",
        window.imp().tab_view.n_pages()
    );
}

// --- Window integration: close tabs ---

#[test]
fn test_close_tab_for_path_exact() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let file_path = dir.path().join("doomed.rs");
    fixture::write_text(&file_path, "");
    window.open_document(&file_path);
    assert_tab_count(&window, 1);

    window.close_tab_for_path(&file_path);
    flush_events();
    assert_tab_count(&window, 0);
}

#[test]
fn test_close_tab_for_path_directory_closes_children() {
    ensure_gtk_init();
    let window = test_window();

    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let sub = dir.path().join("sub");
    fixture::create_dir(&sub);
    let f1 = sub.join("a.rs");
    let f2 = sub.join("b.rs");
    let f3 = dir.path().join("outside.rs");
    fixture::write_text(&f1, "");
    fixture::write_text(&f2, "");
    fixture::write_text(&f3, "");

    window.open_document(&f1);
    window.open_document(&f2);
    window.open_document(&f3);
    assert_tab_count(&window, 3);

    window.close_tab_for_path(&sub);
    flush_events();
    assert_tab_count(&window, 1);

    let remaining = window
        .imp()
        .tab_view
        .nth_page(0)
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .expect("expected operation to succeed");
    assert_eq!(remaining.file_path().expect("expected operation to succeed"), f3);
}

#[test]
fn test_close_tab_for_path_nonexistent_is_noop() {
    ensure_gtk_init();
    let window = test_window();
    window.new_tab();
    assert_tab_count(&window, 1);

    window.close_tab_for_path(std::path::Path::new("/does/not/exist"));
    flush_events();
    assert_tab_count(&window, 1);
}

// --- Evidence surface: the three standing proofs plus the two this row exists for ---

/// The tree evidence surface with only the volatile watcher mailbox normalized away.
///
/// Compares **everything else exactly**, which is what the reentrancy proof needs. The
/// claim under test is that *reading* the surface does not mutate — not that an OS
/// filesystem watcher is quiescent between two reads. These tests run against real
/// tempdirs with live watchers, so an inotify notice arriving between two reads would
/// change `watch_mailbox` without any read having caused it; asserting over it would
/// turn the proof into a flake detector for the kernel.
///
/// Every quantity a read could plausibly disturb is still compared, including the
/// expansion-capture counters, the scan-admission counters, the expansion sets, the
/// persistence generations, the watch target generations, and every aggregate.
fn evidence_without_live_mailbox(
    evidence: &lushtext_core::ui::sidebar::evidence::WorkspaceTreeEvidence,
) -> lushtext_core::ui::sidebar::evidence::WorkspaceTreeEvidence {
    let mut normalized = evidence.clone();
    for section in &mut normalized.sections {
        section.watch_mailbox = None;
        section.watch_last_poll_notices = 0;
    }
    normalized
}

/// Seed one workspace whose folder holds a nested directory, so expansion is real.
fn seed_expandable_workspace() -> tempfile::TempDir {
    ensure_gtk_init();
    let folders_dir = tempfile::tempdir().expect("workspace folders tempdir");
    let root = folders_dir.path().join("root");
    fixture::create_dir_all(&root.join("nested").join("deeper"));
    fixture::write_text(&root.join("nested").join("leaf.txt"), "leaf\n");
    fixture::write_text(&root.join("top.txt"), "top\n");

    let mut workspaces = WorkspacesFile::default();
    workspaces.workspaces.push(WorkspaceConfig::with_one_folder(
        WorkspaceId::new("ws-evidence"),
        "evidence",
        root,
    ));
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces.json");
    folders_dir
}

/// Every counter, registry, generation, and metric a surface read must not disturb.
///
/// Captured as one value so a read can be sandwiched between two identical captures.
///
/// **The two registries are captured by key set, not by length**, and that is the whole
/// point of hazards 1 and 6 in `evidence/evidence-surface-materialization.md`:
/// `find_store_for_dir` *inserts* into `dir_stores` while looking a path up, and
/// `find_dir_row` *evicts* from `dir_rows` on a lookup. A count alone can be identical
/// across an insert-plus-evict pair, so counting would let exactly the hazard this proof
/// exists for slip through. An earlier revision of this probe dropped the registries and
/// kept only the counters.
fn inertness_probe(sidebar: &LushtextSidebar) -> InertnessProbe {
    let evidence = sidebar.workspace_tree_evidence();
    let mut dir_store_keys: Vec<std::path::PathBuf> = Vec::new();
    let mut dir_row_keys: Vec<std::path::PathBuf> = Vec::new();
    for section in sidebar.imp().sections.borrow().iter() {
        dir_store_keys.extend(section.imp().dir_stores.borrow().keys().cloned());
        dir_row_keys.extend(section.imp().dir_rows.borrow().keys().cloned());
    }
    dir_store_keys.sort();
    dir_row_keys.sort();
    let section_scan_pressure: usize = sidebar
        .imp()
        .sections
        .borrow()
        .iter()
        .map(|section| {
            let pressure = section.workspace_section_evidence().scan_pressure;
            pressure.active_scans + pressure.pending_scans + pressure.admission_waiting_scans
        })
        .sum();
    let watch_generations: u64 = sidebar
        .imp()
        .sections
        .borrow()
        .iter()
        .map(|section| section.workspace_section_evidence().watch_target_generation)
        .sum();
    InertnessProbe {
        expansion_capture_scans: evidence.expansion_capture_scans,
        expansion_capture_rows: evidence.expansion_capture_rows,
        process_active_scan_tasks: evidence.process_active_scan_tasks,
        process_scan_task_high_water: evidence.process_scan_task_high_water,
        expanded_path_count: evidence.expanded_path_count,
        section_count: evidence.section_count,
        section_scan_pressure,
        watch_generations,
        dir_store_keys,
        dir_row_keys,
    }
}

/// One capture of the state a surface read must leave untouched.
#[derive(Clone, Debug, PartialEq, Eq)]
struct InertnessProbe {
    expansion_capture_scans: u64,
    expansion_capture_rows: u64,
    process_active_scan_tasks: usize,
    process_scan_task_high_water: usize,
    expanded_path_count: usize,
    section_count: usize,
    section_scan_pressure: usize,
    watch_generations: u64,
    /// Hazard 1: `find_store_for_dir` **inserts** while looking up.
    dir_store_keys: Vec<std::path::PathBuf>,
    /// Hazard 6: `find_dir_row` **evicts** on a lookup.
    dir_row_keys: Vec<std::path::PathBuf>,
}

#[test]
fn test_workspace_tree_evidence_is_honest_with_zero_workspaces() {
    // The child-collection rule: an aggregate over a variable-sized set of section
    // widgets must answer honestly when the set is empty, rather than panicking or
    // reporting a neighbour's state.
    ensure_gtk_init();
    let sidebar = LushtextSidebar::new();

    let evidence = sidebar.workspace_tree_evidence();

    assert_eq!(evidence.workspace_count, 0);
    assert_eq!(evidence.folder_count, 0);
    assert_eq!(evidence.scoped_folder_count, 0);
    assert!(evidence.no_workspaces);
    assert_eq!(evidence.section_count, 0);
    assert_eq!(evidence.visible_section_count, 0);
    assert_eq!(evidence.expanded_path_count, 0);
    assert_eq!(evidence.expansion_capture_scans, 0);
    assert_eq!(evidence.expansion_capture_rows, 0);
    assert!(!evidence.refresh_blocks_readiness);
    assert_eq!(evidence.sections_with_watch_worker_inflight, 0);
    assert_eq!(evidence.sections_with_watch_unavailable, 0);
    assert_eq!(evidence.scope_kind, "all");
    assert_eq!(evidence.scope_workspace_id, None);
    assert_eq!(evidence.scope_workspace_name, None);
    // The scan-admission ceiling is process-global and therefore non-zero even with
    // no workspaces. That is exactly why the field is named `process_*`.
    assert!(evidence.process_scan_task_limit > 0);
}

#[test]
fn test_workspace_tree_evidence_reads_are_inert_collapsed_and_expanded() {
    // THE proof this row exists for. Reading the surface must not materialize
    // toolkit state, advance the expansion-capture counters it reports, start a
    // background scan, or queue a watcher restart — with rows collapsed AND with
    // rows expanded, because the hazardous accessors behave differently once a
    // `GtkTreeListModel` has children to hand back.
    let _folders = seed_expandable_workspace();
    let window = test_window();
    let sidebar = window.imp().sidebar.clone();
    present_window(&window);
    sidebar.load_workspaces();
    wait_until(Duration::from_secs(10), || {
        !sidebar.imp().sections.borrow().is_empty()
    });
    flush_after_delay(Duration::from_millis(400));

    // --- collapsed ---
    let before_collapsed = inertness_probe(&sidebar);
    for _ in 0..5 {
        let _ = sidebar.workspace_tree_evidence();
    }
    let after_collapsed = inertness_probe(&sidebar);
    assert_eq!(
        before_collapsed, after_collapsed,
        "reading the surface with rows collapsed must disturb nothing"
    );

    // --- expanded ---
    let section = sidebar.imp().sections.borrow()[0].clone();
    section.expand_folders();
    flush_after_delay(Duration::from_millis(600));

    let before_expanded = inertness_probe(&sidebar);
    // The registry halves of this proof would pass vacuously against two empty maps, so
    // establish that both registries actually hold entries in the expanded case first.
    // Without this, hazards 1 and 6 would be "proved" by there being nothing to insert
    // into or evict from.
    assert!(
        !before_expanded.dir_store_keys.is_empty(),
        "the expanded case must have realized child stores for the no-insert half of \
         hazard 1 to mean anything"
    );
    assert!(
        !before_expanded.dir_row_keys.is_empty(),
        "the expanded case must have cached directory rows for the no-evict half of \
         hazard 6 to mean anything"
    );
    for _ in 0..5 {
        let _ = sidebar.workspace_tree_evidence();
    }
    let after_expanded = inertness_probe(&sidebar);
    assert_eq!(
        before_expanded, after_expanded,
        "reading the surface with rows expanded must disturb nothing — this is the \
         case where row.children() would materialize and a cache lookup would evict"
    );

    // Repeated reads of unchanged state are identical values, not merely
    // non-disturbing: the reentrancy proof's second half.
    let first = sidebar.workspace_tree_evidence();
    let second = sidebar.workspace_tree_evidence();
    assert_eq!(evidence_without_live_mailbox(&first), evidence_without_live_mailbox(&second));
}

#[test]
fn test_workspace_tree_evidence_reads_stay_side_effect_free_across_mutation() {
    // The reentrancy proof: drive the workflow through each operation that takes a
    // mutable borrow of state the accessor reads, and read the surface AFTER each
    // one. Deliberately NOT a read while a borrow is held — that is the panic the
    // constraint prevents, not a demonstration of it.
    let _folders = seed_restored_workspaces();
    let window = test_window();
    let sidebar = window.imp().sidebar.clone();
    present_window(&window);

    // (1) after the load that replaces `workspaces_file` and rebuilds `sections`
    sidebar.load_workspaces();
    wait_until(Duration::from_secs(10), || {
        sidebar.imp().sections.borrow().len() == 3
    });
    let after_load = sidebar.workspace_tree_evidence();
    assert_eq!(after_load.workspace_count, 3);
    assert_eq!(after_load.section_count, 3);
    assert_eq!(
        evidence_without_live_mailbox(&sidebar.workspace_tree_evidence()),
        evidence_without_live_mailbox(&after_load)
    );

    // (2) after a scope change, which borrows `current_scope` mutably
    sidebar.imp().workspace_filter_dropdown.set_selected(1);
    flush_after_delay(Duration::from_millis(300));
    let after_scope = sidebar.workspace_tree_evidence();
    // A present id must come with a present, non-empty name: the exported field is
    // documented as "if any", so `Some("")` beside a non-null id would be a name that
    // is not a name.
    assert!(after_scope.scope_workspace_id.is_some());
    assert!(
        after_scope
            .scope_workspace_name
            .as_deref()
            .is_some_and(|name| !name.is_empty()),
        "a scoped workspace reports its real name, never an empty string"
    );
    assert_eq!(
        evidence_without_live_mailbox(&sidebar.workspace_tree_evidence()),
        evidence_without_live_mailbox(&after_scope)
    );

    // (3) after a persistence request, which borrows `persistence` mutably
    sidebar.rename_workspace_for_test(&WorkspaceId::new("ws-0"), "renamed");
    let after_persist = sidebar.workspace_tree_evidence();
    assert!(after_persist.persistence_pending || after_persist.persistence_inflight);
    assert_eq!(
        evidence_without_live_mailbox(&sidebar.workspace_tree_evidence()),
        evidence_without_live_mailbox(&after_persist)
    );

    // (4) after the write settles
    wait_until(Duration::from_secs(10), || {
        !sidebar.workspace_tree_evidence().persistence_pending
    });
    let settled = sidebar.workspace_tree_evidence();
    assert_eq!(
        evidence_without_live_mailbox(&sidebar.workspace_tree_evidence()),
        evidence_without_live_mailbox(&settled)
    );
    assert!(settled.persistence_durable_generation >= 1);
}

#[test]
fn test_workspace_tree_evidence_answers_honestly_across_a_real_section_teardown() {
    // The disposed-widget rule applied to a SET rather than to one child. The state
    // is produced by a real teardown — unlisting a workspace destroys its section —
    // rather than by fabricating a half-disposed widget.
    //
    // Note deliberately: this surface reads no `TemplateChild`, so there is no
    // panicking accessor to guard. Every per-section field comes from `Cell`/`RefCell`
    // state that outlives GTK's `dispose()`. This test proves the aggregate tracks the
    // live set and keeps answering, which is the observable half of that property.
    let _folders = seed_restored_workspaces();
    let window = test_window();
    let sidebar = window.imp().sidebar.clone();
    present_window(&window);
    sidebar.load_workspaces();
    wait_until(Duration::from_secs(10), || {
        sidebar.imp().sections.borrow().len() == 3
    });

    let live = sidebar.workspace_tree_evidence();
    assert_eq!(live.section_count, 3);
    assert_eq!(live.workspace_count, 3);

    sidebar.remove_workspace_for_test(&WorkspaceId::new("ws-1"));
    wait_until(Duration::from_secs(10), || {
        sidebar.imp().sections.borrow().len() == 2
    });
    flush_after_delay(Duration::from_millis(300));

    let after = sidebar.workspace_tree_evidence();
    assert_eq!(
        after.section_count, 2,
        "the aggregate tracks the live section set across teardown"
    );
    assert_eq!(after.workspace_count, 2);
    assert!(
        after.expanded_path_count <= live.expanded_path_count,
        "a torn-down section cannot contribute expansion state"
    );
    // Still answering, still repeatable, still not a panic.
    assert_eq!(
        evidence_without_live_mailbox(&sidebar.workspace_tree_evidence()),
        evidence_without_live_mailbox(&after)
    );
}

#[test]
fn test_a_workspace_created_during_a_load_is_not_reverted_by_the_load() {
    // M-4, DRIVEN. Slot 5a proved this guard by its shape only and named a driven
    // test as slot 5b's highest-value remaining coverage.
    //
    // The defect: `load_workspaces` dispatches a read of `workspaces.json`, and
    // `build_sections_from_file` unconditionally overwrites `workspaces_file`. If the
    // user creates a workspace while that read is in flight, `persist()` has already
    // scheduled the new workspace for disk — so adopting the loaded file discards it
    // from memory while its write is still pending. That mismatch is what makes it
    // data loss rather than a stale view.
    //
    // The guard captures `requested_generation()` before dispatch and refuses to adopt
    // when a mutation superseded it. Reading a small JSON file finishes far faster than
    // a headless test can drive "New Workspace", so without a delay this race never
    // occurs and the test would pass against the reverted guard too.
    let _folders = seed_restored_workspaces();
    let window = test_window();
    let sidebar = window.imp().sidebar.clone();
    present_window(&window);

    // Let the window's own startup load settle first, so the load this test drives is
    // unambiguously the one it dispatched.
    wait_until(Duration::from_secs(10), || {
        sidebar.workspace_tree_evidence().workspace_count == 3
    });

    // Hold the load worker open long enough to interpose the mutation. 600ms against a
    // 150ms interposition means the mutation lands while the worker is still sleeping.
    lushtext_core::ui::sidebar::set_workspace_load_worker_delay_for_test(600);
    sidebar.load_workspaces();

    // Create a workspace while the load is still in flight. This is the production
    // path: it mutates `workspaces_file` and calls `persist()`.
    flush_after_delay(Duration::from_millis(150));
    sidebar.enter_new_workspace_name_for_test("created-during-load");
    assert_eq!(
        sidebar.workspace_tree_evidence().workspace_count,
        4,
        "the workspace exists in memory before the load completes — which is precisely \
         what an unguarded adoption would then discard"
    );
    assert!(
        sidebar.workspace_tree_evidence().persistence_pending,
        "creating a workspace must request a write, which is what makes adopting the \
         load a loss rather than a stale view"
    );

    // Let the load complete and settle.
    wait_until(Duration::from_secs(10), || {
        !sidebar.workspace_tree_evidence().persistence_pending
    });
    flush_after_delay(Duration::from_millis(400));
    lushtext_core::ui::sidebar::set_workspace_load_worker_delay_for_test(0);

    // The workspace the user created during the load must still exist, in memory and
    // on disk. Without the guard the completed load overwrites it away.
    let settled = sidebar.workspace_tree_evidence();
    assert_eq!(
        settled.workspace_count, 4,
        "the three restored workspaces plus the one created during the load"
    );
    let on_disk = workspace_manager::load_recovering(&json_store::data_dir()).value;
    assert_eq!(
        on_disk.workspaces.len(),
        4,
        "the created workspace must also have reached disk, not just memory"
    );
    assert!(
        on_disk
            .workspaces
            .iter()
            .any(|workspace| workspace.name == "created-during-load"),
        "the workspace created during the load survived by name"
    );
}

#[test]
fn test_a_workspace_created_before_the_first_load_completes_merges_instead_of_overwriting() {
    // The pre-first-load half of the M-4 race, DRIVEN.
    //
    // A previous revision deferred this to pure policy tests, on the reasoning that
    // "the first load has not been adopted yet" cannot be arranged without racing the
    // window's own startup load. It can: a **standalone** sidebar has no startup gate,
    // so the load this test dispatches is unambiguously the first one.
    //
    // Deferring it also hid a real defect. `superseded_load_action` was fed a bit set
    // inside `build_sections_from_file`, which every mutation reaches through
    // `rebuild_sections_from_state` — so creating a workspace during the first load
    // flipped the bit *before* the load completed, the guard chose `KeepMemory`, and
    // the already-scheduled write committed the one new workspace over all three
    // stored ones. `MergeAndPersist` had no reachable production caller at all.
    //
    // This test is the reachability proof: it fails against that code and passes only
    // when the bit means what its parameter name claims.
    let _folders = seed_restored_workspaces();
    let sidebar = LushtextSidebar::new();

    // Hold the load worker open long enough to interpose the mutation. 600ms against a
    // 150ms interposition means the mutation lands while the worker is still sleeping.
    lushtext_core::ui::sidebar::set_workspace_load_worker_delay_for_test(600);
    sidebar.load_workspaces();

    // Nothing has been adopted yet: this is the state the pre-first-load branch exists
    // for, and asserting it here is what makes the interleaving real rather than assumed.
    flush_after_delay(Duration::from_millis(150));
    assert_eq!(
        sidebar.workspace_tree_evidence().workspace_count,
        0,
        "the first load is still in flight, so memory is still the empty initial file"
    );

    // Create a workspace through the production path: it mutates `workspaces_file`,
    // bumps the persistence generation, and schedules the debounced write.
    sidebar.enter_new_workspace_name_for_test("created-before-first-load");
    assert_eq!(
        sidebar.workspace_tree_evidence().workspace_count,
        1,
        "memory now holds one mutation on top of nothing — not the full list"
    );
    assert!(
        sidebar.workspace_tree_evidence().persistence_pending,
        "the pending write is what turns discarding the load into data loss: it would \
         commit this empty-plus-one state over every workspace on disk"
    );

    // Let the load complete and the merged write settle. Polled inline rather than
    // through `wait_until` for one reason: this test's whole value is naming *which*
    // workspaces were lost, and a `wait_until` timeout reports only "condition was not
    // met", so the assertions below have to be the thing that fires. Same budget, same
    // shared drain.
    for _ in 0..100 {
        if sidebar.workspace_tree_evidence().workspace_count == 4 {
            break;
        }
        flush_after_delay(Duration::from_millis(100));
    }
    assert_eq!(
        sidebar.workspace_tree_evidence().workspace_count,
        4,
        "the completed load must merge with the mutation that superseded it; keeping \
         memory here discards every stored workspace and the pending write commits \
         that loss to disk"
    );
    wait_until(Duration::from_secs(10), || {
        !sidebar.workspace_tree_evidence().persistence_pending
    });
    flush_after_delay(Duration::from_millis(400));
    lushtext_core::ui::sidebar::set_workspace_load_worker_delay_for_test(0);

    // All three stored workspaces survive alongside the new one, in memory...
    let settled = sidebar.workspace_tree_evidence();
    assert_eq!(
        settled.workspace_count, 4,
        "the three stored workspaces merged with the one created during the load"
    );
    assert_eq!(settled.section_count, 4);

    // ...and on disk, which is the half the pending write would otherwise have lost.
    let on_disk = workspace_manager::load_recovering(&json_store::data_dir()).value;
    let names = on_disk
        .workspaces
        .iter()
        .map(|workspace| workspace.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        on_disk.workspaces.len(),
        4,
        "the merged set must be persisted, not merely shown: {names:?}"
    );
    for expected in ["one", "two", "three", "created-before-first-load"] {
        assert!(
            names.iter().any(|name| name == expected),
            "`{expected}` must survive the merge: {names:?}"
        );
    }
}

// The remaining pre-first-load reasoning — that merging cannot express a deletion, and
// is therefore gated rather than unconditional — stays covered by pure policy tests in
// `ui/sidebar/policy.rs`: `a_superseded_load_merges_only_before_the_first_adoption`,
// `merging_before_the_first_load_keeps_both_the_stored_list_and_the_new_workspace`, and
// `merging_cannot_express_a_deletion_which_is_why_it_is_gated`. Those are pure
// functions of one bit, so they are deterministic and mutation-covered; the test above
// is what proves the bit itself is derived from a load adoption.


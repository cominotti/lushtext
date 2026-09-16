// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-integration tests for the attention-moment workspace refresh.
//!
//! These cover the defect that motivated the change: the palette's file index
//! had no filesystem input at all, so a file created by `git checkout`, by a
//! build, or by a terminal never became searchable. Each test below fails
//! against the unfixed code — verified by reverting the trigger — because the
//! index it asserts on simply never learns the file exists.
//!
//! Every workspace lives in its own tempdir, which is also what keeps the
//! throttle from leaking between tests: the refresh history is keyed by
//! workspace folder set, so distinct fixtures are distinct keys.

use crate::common::{
    active_editor, ensure_gtk_init, fixture, flush_after_delay, flush_events, isolated_data_dir,
    present_window, test_window, wait_for_palette_index, wait_until, wait_until_or_false,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::attention_refresh::AttentionRefreshAdmission;
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::{json_store, workspace_manager};
use lushtext_core::ui::command_palette::item::PaletteItem;
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::window::{AttentionSurfaces, LushtextWindow, ModalSurfaceGuard};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Generous because the index rebuild is a real traversal on a worker thread.
const INDEX_SETTLE: Duration = Duration::from_secs(10);
/// Shorter: a query runs against an already-built index behind a 150 ms debounce.
const QUERY_SETTLE: Duration = Duration::from_secs(5);

/// Persist a one-folder workspace and return its root.
fn workspace_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let tempdir = tempfile::tempdir().expect("workspace tempdir");
    let folder = tempdir.path().join(name);
    fixture::create_dir_all(&folder);
    let workspaces = WorkspacesFile {
        current_scope: WorkspaceScope::All,
        workspaces: vec![WorkspaceConfig::with_one_folder(
            WorkspaceId::new(name),
            name,
            folder.clone(),
        )],
    };
    workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspace");
    (tempdir, folder)
}

/// Whether the palette can actually find `path`, which is the user-visible
/// contract this change is about.
///
/// Asserted through a real query rather than an index-internals getter: the
/// workflow's evidence surface reports the index *size*, and the question here
/// is whether one specific file is reachable. Searching is both the honest
/// assertion and the one that needs no new test seam.
///
/// The query is cleared first because `set_query` with text identical to the
/// current query does not re-run a search, so a second call for the same file
/// would otherwise read the previous call's rows and report a removed file as
/// still present.
fn palette_finds(window: &LushtextWindow, path: &Path) -> bool {
    let palette = &window.imp().command_palette;
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .expect("fixture file name");
    palette.set_query("");
    flush_after_delay(Duration::from_millis(50));
    palette.set_query(stem);
    wait_until_or_false(QUERY_SETTLE, || {
        let store = palette.imp().results_store.clone();
        (0..store.n_items())
            .filter_map(|index| store.item(index).and_downcast::<PaletteItem>())
            .any(|item| item.file_path().as_deref() == Some(path))
    })
}

#[test]
fn test_external_file_in_unexpanded_directory_appears_after_attention() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("checkout");
    // Deliberately deep and never expanded in the sidebar: this is the case the
    // sidebar's non-recursive, materialized-only watcher cannot observe.
    let nested = folder.join("src").join("deep").join("nested");
    fixture::create_dir_all(&nested);
    fixture::write_text(&folder.join("README.md"), "# readme\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    let arrived = nested.join("arrived_from_terminal.rs");
    fixture::write_text(&arrived, "fn arrived() {}\n");
    flush_events();
    assert!(
        !palette_finds(&window, &arrived),
        "the index must not learn about an external file on its own; \
         if it did, this test would pass against the unfixed code"
    );

    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::Start
    );
    wait_for_palette_index(&window, 2);
    assert!(palette_finds(&window, &arrived));
}

#[test]
fn test_branch_switch_shape_adds_many_files_after_attention() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("branch");
    fixture::write_text(&folder.join("only.rs"), "fn only() {}\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    // A branch switch does not add one file; it rewrites a subtree. This shape
    // is also what pushes an incremental mutation queue past its bound, which
    // is the reason this change refreshes instead of sweeping for changes.
    let feature = folder.join("feature");
    fixture::create_dir_all(&feature);
    for index in 0..64 {
        fixture::write_text(&feature.join(format!("mod_{index}.rs")), "// generated\n");
    }

    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::Start
    );
    wait_for_palette_index(&window, 65);
    assert!(palette_finds(&window, &feature.join("mod_63.rs")));
}

#[test]
fn test_externally_removed_file_stops_appearing_after_attention() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("removal");
    let doomed = folder.join("doomed.rs");
    fixture::write_text(&doomed, "fn doomed() {}\n");
    fixture::write_text(&folder.join("kept.rs"), "fn kept() {}\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 2);
    assert!(palette_finds(&window, &doomed));

    fixture::remove_file(&doomed);
    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::Start
    );
    wait_for_palette_index(&window, 1);
    assert!(!palette_finds(&window, &doomed));
}

#[test]
fn test_excluded_directory_stays_excluded_after_attention() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("excluded");
    fixture::write_text(&folder.join("src.rs"), "fn src() {}\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    // The refresh must inherit the index's coverage rule rather than restate
    // it: a build directory that appears externally stays out.
    let ignored = folder.join("node_modules").join("pkg");
    fixture::create_dir_all(&ignored);
    fixture::write_text(&ignored.join("index.js"), "module.exports = {};\n");

    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::Start
    );
    // The index already held exactly one file *before* the refresh started, so
    // waiting for that count would wait for nothing and let the negative
    // assertion run while the refresh was still in flight. Wait for the refresh
    // itself to stop being in flight instead.
    assert!(
        wait_until_or_false(INDEX_SETTLE, || window
            .refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree)
            != AttentionRefreshAdmission::RefuseInFlight),
        "the refresh never settled"
    );
    assert_eq!(
        window.imp().command_palette.evidence().file_index_len,
        1,
        "the refreshed index admitted something new"
    );
    assert!(
        !palette_finds(&window, &ignored.join("index.js")),
        "excluded directory leaked into the refreshed index"
    );
}

#[test]
fn test_rapid_second_attention_moment_is_refused() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("throttle");
    fixture::write_text(&folder.join("a.rs"), "fn a() {}\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::Start
    );
    // Immediately: the first refresh has not settled, so the second moment is
    // refused as in-flight rather than starting a duplicate traversal.
    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::RefuseInFlight
    );

    // Once it settles, the adaptive interval takes over and still refuses.
    assert!(
        wait_until_or_false(INDEX_SETTLE, || window
            .refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree)
            == AttentionRefreshAdmission::RefuseThrottled),
        "the in-flight refusal never gave way to the throttled one"
    );
}

#[test]
fn test_save_in_flight_refuses_attention_refresh() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("saving");
    let document = folder.join("doc.md");
    fixture::write_text(&document, "original\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    window.open_document(&document);
    flush_events();
    let editor = active_editor(&window);
    editor.buffer().set_text("edited\n");
    editor.save_file_async(|_| {});

    // The save occupies a shared `spawn_blocking_then` slot. A refresh must
    // yield rather than queue in front of the operation that protects the
    // user's bytes.
    if editor.is_saving() {
        assert_eq!(
            window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
            AttentionRefreshAdmission::RefuseUserWorkInFlight
        );
    }
    wait_until(INDEX_SETTLE, || !editor.is_saving());
}

#[test]
fn test_save_as_indexes_the_new_path_without_a_refresh() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("saveas");
    fixture::write_text(&folder.join("existing.md"), "existing\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    window.new_tab();
    flush_events();
    let editor = active_editor(&window);
    editor.buffer().set_text("fresh content\n");

    let destination = folder.join("saved_from_untitled.md");
    window.select_save_as_destination_for_test(&destination);

    // No attention refresh is triggered here: the application wrote this file
    // itself, so the index must learn about it from the save, not from a later
    // traversal that may be minutes away.
    wait_for_palette_index(&window, 2);
    assert!(palette_finds(&window, &destination));
}

#[test]
fn test_save_outside_workspace_scope_is_not_indexed() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (tempdir, _folder) = workspace_fixture("scoped");
    let outside = tempdir.path().join("outside.md");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 0);

    window.new_tab();
    flush_events();
    let editor = active_editor(&window);
    editor.buffer().set_text("outside the workspace\n");
    window.select_save_as_destination_for_test(&outside);

    // Wait on the save's own completion rather than a fixed delay, so any
    // mistaken indexing has actually had its chance before the negative
    // assertion runs.
    assert!(
        wait_until_or_false(INDEX_SETTLE, || !editor.is_saving()
            && editor.file_path().as_deref() == Some(outside.as_path())),
        "the save never completed"
    );
    // Asserted on the index rather than on a palette query: the palette also
    // offers *open tabs* as their own result group, correctly and regardless of
    // workspace membership, so a search would find this file for a reason that
    // has nothing to do with the index. The spec scenario is about admission.
    assert_eq!(
        window.imp().command_palette.evidence().file_index_len,
        0,
        "a path outside every workspace folder must not enter the index"
    );
}

/// The sidebar's half of the defect, and the one place its tree cannot heal
/// itself.
///
/// Expanding a directory re-scans it, so collapsed subtrees are correct the
/// moment a user opens them. What does not heal is the empty-or-not hint: it is
/// computed by the *parent's* bounded lookahead scan and then frozen in the
/// row, while the parent's watch is non-recursive and never fires for an entry
/// created inside the collapsed child. The row keeps claiming to be empty,
/// keeps its expander hidden, and keeps `Focus Folder` disabled.
#[test]
fn test_collapsed_empty_directory_stops_claiming_empty_after_attention() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("emptyhint");
    let collapsed = folder.join("looks_empty");
    fixture::create_dir_all(&collapsed);
    fixture::write_text(&folder.join("sibling.txt"), "sibling\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    // The collapsed child is only in the tree model once its parent is open —
    // which is exactly the real situation: the user sees `looks_empty` because
    // the workspace folder above it is expanded, and `looks_empty` itself is
    // not.
    expand_workspace_folders(&window);
    assert!(
        wait_until_or_false(INDEX_SETTLE, || row_is_empty(&window, &collapsed)
            == Some(true)),
        "fixture precondition: the collapsed directory must start out reported empty"
    );

    fixture::write_text(&collapsed.join("appeared.txt"), "appeared\n");
    flush_events();
    assert_eq!(
        row_is_empty(&window, &collapsed),
        Some(true),
        "the frozen hint must not correct itself; if it did, this test would \
         pass against the unfixed code"
    );

    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::Start
    );
    assert!(
        wait_until_or_false(INDEX_SETTLE, || row_is_empty(&window, &collapsed)
            == Some(false)),
        "the collapsed directory still claims to be empty after the refresh"
    );
}

/// Open every workspace folder row so its children enter the tree model.
fn expand_workspace_folders(window: &LushtextWindow) {
    for section in window.imp().sidebar.imp().sections.borrow().iter() {
        section.expand_folders();
    }
}

/// Read a directory row's frozen empty-or-not hint, across every section.
fn row_is_empty(window: &LushtextWindow, target: &Path) -> Option<bool> {
    for section in window.imp().sidebar.imp().sections.borrow().iter() {
        let Some(tree_model) = section.imp().tree_model.borrow().as_ref().cloned() else {
            continue;
        };
        for index in 0..tree_model.n_items() {
            if let Some(item) = tree_model
                .item(index)
                .and_downcast::<gtk4::TreeListRow>()
                .and_then(|row| row.item())
                .and_downcast::<FileTreeItem>()
                && item.path().as_deref() == Some(target)
            {
                return item.is_empty();
            }
        }
    }
    None
}

/// The ordering hole a save-in-flight check alone does not close.
///
/// A file chooser takes focus away and gives it back, so the window becomes
/// active again *before* the resulting save has been queued. At that instant no
/// editor is saving, so a refresh would be admitted into the shared
/// `spawn_blocking_then` pool immediately ahead of the save it is about to
/// race. The bracket is what makes that window uninteresting — and it must
/// survive a cancelled chooser, or refreshing would stop for the session.
#[test]
fn test_an_open_file_chooser_refuses_attention_refresh_and_releases_it() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let (_tempdir, folder) = workspace_fixture("cancelled");
    fixture::write_text(&folder.join("doc.md"), "content\n");

    let window = test_window();
    present_window(&window);
    wait_for_palette_index(&window, 1);

    // Two nested choosers, both dismissed without a selection: the bracket runs
    // on the cancelled path too, so the counter returns to zero.
    // Two nested surfaces, the inner one dismissed without a selection: the
    // guard releases on drop, so the cancelled path is covered too.
    let outer = ModalSurfaceGuard::acquire();
    {
        let _inner = ModalSurfaceGuard::acquire();
        assert_eq!(
            window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
            AttentionRefreshAdmission::RefuseUserWorkInFlight
        );
    }
    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::RefuseUserWorkInFlight,
        "one surface still open must keep refusing"
    );
    drop(outer);
    assert_eq!(
        window.refresh_workspace_surfaces_on_attention(AttentionSurfaces::IndexAndTree),
        AttentionRefreshAdmission::Start,
        "the refusal must lift once every surface has been dismissed"
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Criterion benchmarks for LushText performance-sensitive code paths.
//!
//! All benchmarked functions are GTK-free — no display server needed.
//! Run with: `cargo bench -p lushtext-core` or `make bench`

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use gtk4::gio;
use gtk4::prelude::ListModelExt;
use notify_debouncer_full::notify::event::{AccessKind, AccessMode, CreateKind};
use notify_debouncer_full::notify::{Event, EventKind};
use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::fmt::Write as _;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tempfile::TempDir;

use lushtext_core::model::bookmark::BookmarkRecord;
use lushtext_core::model::buffer_replacement::{
    BufferReplacementPlan, REPLACEMENT_CLEAR_SLICE_CHARS, REPLACEMENT_INSERT_SLICE_BYTES,
};
use lushtext_core::model::content_search::{
    ContentSearchOptions, MAX_REPLACE_PREVIEW_ROWS, Replacement, SearchEvent, SearchMatch,
    SearchMatchId, generate_replacement_preview,
};
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::editor_memory::{
    EDITOR_MEMORY_UPPER_BUDGET_BYTES, EditorResidency, EditorResidencyLedger,
    estimate_live_editor_bytes, evaluate_editor_memory_budget,
};
use lushtext_core::model::encoding::{DocumentEncoding, LineEnding};
use lushtext_core::model::file_load::{
    FileLoadAdmissionPolicy, FileLoadAdmissionRequest, FileLoadPriority,
    TRANSIENT_LOAD_SHARED_BUDGET_BYTES, next_install_boundary, transient_load_weight,
};
use lushtext_core::model::local_history::LocalHistorySnapshotOrigin;
use lushtext_core::model::local_history::{LocalHistoryDocument, LocalHistorySnapshotMeta};
use lushtext_core::model::migration_ledger::MigrationKind;
use lushtext_core::model::minimap_analysis::{
    MinimapAnalysisAccumulator, MinimapAnalysisPolicy, MinimapAnalysisResult,
};
use lushtext_core::model::palette::{
    IndexedFile, PaletteFileIdentity, PaletteNoteCategory, PaletteNoteEntry, PaletteNoteTarget,
    PaletteOpenEditorNoteSnapshot, PaletteSearchRow, SearchMode, SearchResultItem,
};
use lushtext_core::model::recent_document::{RecentDocumentEntry, RecentDocumentRow};
use lushtext_core::model::save_admission::{
    ExternalTransientPressure, SAVE_PAYLOAD_SHARED_BUDGET_BYTES, SaveAdmissionPolicy,
    SaveAdmissionPriority, SaveAdmissionRequest,
};
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::model::sidecar_identity::{next_record_id, now_epoch_millis, stable_bytes_hash};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceFolder, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::model::workspace_scan::WorkspaceScanFlight;
use lushtext_core::model::workspace_search::{
    WorkspaceSearchFallbackMetrics, WorkspaceSearchTraversalPlan,
};
use lushtext_core::services::content_search;
use lushtext_core::services::editor_io;
use lushtext_core::services::file_limits::FileSizeCheck;
use lushtext_core::services::file_tree::{
    self, DirectoryEntry, DirectoryReconciliationPlan, DirectoryRowState,
};
use lushtext_core::services::filesystem::{
    DirectoryScanPolicy, fixture, metadata as fs_metadata, read as fs_read, tree as fs_tree,
};
use lushtext_core::services::json_format::KIND_LOCAL_HISTORY_INDEX;
use lushtext_core::services::markdown_render::{
    MARKDOWN_EVENTS_PER_PROJECTION_SLICE, plan_markdown,
};
use lushtext_core::services::palette::{self, FileIndex};
use lushtext_core::services::recent_documents;
use lushtext_core::services::recovery_metadata::{
    RecoveryLoadConfig, RecoveryMetadataClass, save_enveloped_json_path,
};
use lushtext_core::services::workspace_manager;
use lushtext_core::services::workspace_watch::{WORKSPACE_WATCH_PATH_CAP, WorkspaceWatchMailbox};
use lushtext_core::services::{
    bookmark_service, draft_service,
    local_history_service::{self, LocalHistoryCapturePolicy},
    migration_ledger, session_service,
};
use lushtext_core::ui::search_panel::policy::{
    SearchRetirementSliceBudget, WorkspaceSearchFlight, WorkspaceSearchRequest,
    WorkspaceSearchSubmission,
};
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::sidebar::workspace_section::child_cache_rebuild_operation_evidence_for_benchmark;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// Bounded channel capacity used by production content search.
///
/// Keeping the benchmark at the same 1024-event capacity exercises the same
/// backpressure contract as the GTK search panel without letting Criterion
/// block forever when a fixture emits more matches than the receiver can hold.
const CONTENT_SEARCH_BENCH_CHANNEL_CAPACITY: usize = 1024;

/// Run streaming content search while draining events concurrently.
///
/// `content_search::search` is a synchronous producer that intentionally sends
/// through a bounded channel. Benchmarks must drain that channel at the same
/// time, matching the UI worker/receiver shape; draining only after `search`
/// returns can deadlock before Criterion finishes warmup.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ContentSearchBenchmarkOutcome {
    events: usize,
    matches: usize,
    incomplete: usize,
    match_identities: Vec<(PathBuf, u64)>,
    fallback_metrics: WorkspaceSearchFallbackMetrics,
}

fn run_content_search_benchmark(
    query: &str,
    workspace_folders: &[&Path],
    options: &ContentSearchOptions,
) -> ContentSearchBenchmarkOutcome {
    run_content_search_benchmark_with_cancel(query, workspace_folders, options, false)
}

fn run_content_search_benchmark_with_cancel(
    query: &str,
    workspace_folders: &[&Path],
    options: &ContentSearchOptions,
    cancelled: bool,
) -> ContentSearchBenchmarkOutcome {
    let (tx, rx) = crossbeam_channel::bounded(CONTENT_SEARCH_BENCH_CHANNEL_CAPACITY);
    let cancel = Arc::new(AtomicBool::new(false));
    cancel.store(cancelled, std::sync::atomic::Ordering::Release);

    // Move the receiver to a short-lived drain thread so the producer keeps the
    // same bounded backpressure contract as production instead of using an
    // unbounded benchmark-only channel.
    let drain = std::thread::spawn(move || {
        rx.iter().fold(
            ContentSearchBenchmarkOutcome::default(),
            |mut outcome, event| {
                outcome.events = outcome.events.saturating_add(1);
                match event {
                    SearchEvent::Match(search_match) => {
                        outcome.matches = outcome.matches.saturating_add(1);
                        outcome
                            .match_identities
                            .push((search_match.path, search_match.line_number));
                    }
                    SearchEvent::Incomplete(_) => {
                        outcome.incomplete = outcome.incomplete.saturating_add(1);
                    }
                    SearchEvent::TraversalMetrics(metrics) => {
                        outcome.fallback_metrics = metrics;
                    }
                    _ => {}
                }
                outcome
            },
        )
    });

    content_search::search(
        black_box(query),
        black_box(workspace_folders),
        black_box(options),
        tx,
        cancel,
        None,
        None,
    );

    drain
        .join()
        .expect("content-search benchmark event drain should not panic")
}

fn workspace_search_plan_retained_bytes(plan: &WorkspaceSearchTraversalPlan) -> u64 {
    plan.display_roots()
        .iter()
        .fold(0u64, |total, root| {
            total
                .saturating_add(
                    u64::try_from(root.configured_path().as_os_str().len()).unwrap_or(u64::MAX),
                )
                .saturating_add(root.canonical_path().map_or(0, |path| {
                    u64::try_from(path.as_os_str().len()).unwrap_or(u64::MAX)
                }))
        })
        .saturating_add(plan.traversal_roots().iter().fold(0u64, |total, root| {
            total
                .saturating_add(
                    u64::try_from(root.scan_path().as_os_str().len()).unwrap_or(u64::MAX),
                )
                .saturating_add(root.canonical_path().map_or(0, |path| {
                    u64::try_from(path.as_os_str().len()).unwrap_or(u64::MAX)
                }))
                .saturating_add(root.excluded_paths().iter().fold(0u64, |bytes, path| {
                    bytes.saturating_add(u64::try_from(path.as_os_str().len()).unwrap_or(u64::MAX))
                }))
        }))
}

/// Build a synthetic in-memory file index with realistic file names.
fn make_synthetic_index(n: usize) -> FileIndex {
    let root = Arc::new(PathBuf::from("/synthetic/project"));
    let dirs = [
        "src",
        "src/model",
        "src/services",
        "src/ui",
        "src/ui/window",
        "tests",
        "docs",
        "benches",
    ];
    let extensions = ["rs", "toml", "md", "json", "txt", "yaml"];

    let files: Vec<IndexedFile> = (0..n)
        .map(|i| {
            let dir = dirs[i % dirs.len()];
            let ext = extensions[i % extensions.len()];
            let name = if i % 1_000 == 0 {
                format!("résumé-shared-{i:06}.md")
            } else if i % 997 == 0 {
                "equal-score-tie.rs".to_string()
            } else {
                format!("file_{i}.{ext}")
            };
            let path = PathBuf::from(format!("/synthetic/project/{dir}/{name}"));
            IndexedFile {
                identity: PaletteFileIdentity::canonical(path.clone()),
                path,
                name,
                workspace_folder: Arc::clone(&root),
            }
        })
        .collect();

    FileIndex::from(files)
}

/// Create a temp directory tree with the given number of files spread across subdirs.
fn make_temp_dir_tree(file_count: usize) -> TempDir {
    let dir = TempDir::new().expect("expected operation to succeed");
    let subdirs = ["src", "src/model", "src/services", "tests", "docs"];
    for subdir in &subdirs {
        fixture::create_dir_all(&dir.path().join(subdir));
    }
    for i in 0..file_count {
        let subdir = subdirs[i % subdirs.len()];
        fixture::write_text(&dir.path().join(format!("{subdir}/file_{i}.rs")), "");
    }
    dir
}

/// Create a flat temp directory with mixed files and subdirs for `scan_directory` benchmarks.
fn make_flat_dir(entry_count: usize) -> TempDir {
    let dir = TempDir::new().expect("expected operation to succeed");
    let n_dirs = entry_count / 2;
    for i in 0..n_dirs {
        fixture::create_dir(&dir.path().join(format!("dir_{i}")));
    }
    for i in 0..(entry_count - n_dirs) {
        fixture::write_text(&dir.path().join(format!("file_{i}.rs")), "");
    }
    dir
}

/// Create a sparse directory forest so index evidence covers directory work
/// independently from admitted file count.
fn make_directory_only_tree(directory_count: usize) -> TempDir {
    let dir = TempDir::new().expect("expected operation to succeed");
    for index in 0..directory_count {
        fixture::create_dir(&dir.path().join(format!("empty_{index:05}")));
    }
    dir
}

/// Create a real tree whose complete raw/canonical path graph approaches the
/// installed and build-byte policies without requiring six-figure file counts.
fn make_near_policy_long_path_tree(file_count: usize) -> TempDir {
    let dir = TempDir::new().expect("near-policy file-index tempdir");
    let mut parent = dir.path().to_path_buf();
    for depth in 0..16 {
        parent = parent.join(format!("segment-{depth:02}-{}", "x".repeat(165)));
        fixture::create_dir(&parent);
    }
    for index in 0..file_count {
        let name = format!("source-{index:05}-{}-界.rs", "n".repeat(96));
        fixture::write_text(&parent.join(name), "");
    }
    dir
}

/// Mirror bounded breadth-first sidebar model population and batching costs.
fn populate_tree_store(entries: Vec<DirectoryEntry>, truncated: bool) -> gio::ListStore {
    // Match the production directory safety cap used by the sidebar fixture.
    const MAX_DIR_ENTRIES: usize = 10_000;
    // Match the production append batch so relayout costs stay representative.
    const CHILD_APPEND_BATCH_SIZE: usize = 256;

    // ListStore is GObject's observable model; GTK list widgets react to its
    // items-changed notifications without rebuilding the model.
    let store = gio::ListStore::new::<FileTreeItem>();
    let mut pending = VecDeque::from(entries);

    while !pending.is_empty() {
        let mut batch = Vec::with_capacity(CHILD_APPEND_BATCH_SIZE);
        for _ in 0..CHILD_APPEND_BATCH_SIZE {
            let Some(entry) = pending.pop_front() else {
                break;
            };
            batch.push(FileTreeItem::new(entry.path, entry.is_dir, entry.is_empty));
        }
        // One splice emits one items-changed signal instead of relayout per row.
        store.splice(store.n_items(), 0, &batch);
    }

    if truncated {
        let placeholder = [FileTreeItem::new_placeholder(format!(
            "{MAX_DIR_ENTRIES}+ items - showing first {MAX_DIR_ENTRIES}"
        ))];
        store.splice(store.n_items(), 0, &placeholder);
    }

    store
}

/// Build a `WorkspacesFile` with the given number of workspaces and entries per workspace.
fn make_workspaces_file(n_workspaces: usize, entries_per: usize) -> WorkspacesFile {
    let workspaces = (0..n_workspaces)
        .map(|w| WorkspaceConfig {
            id: WorkspaceId::new(format!("ws-{w}")),
            name: format!("Workspace {w}"),
            folders: vec![WorkspaceFolder::new(PathBuf::from(format!(
                "/home/user/project_{w}/folder_{entries_per}"
            )))],
        })
        .collect();

    WorkspacesFile {
        current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws-0")),
        workspaces,
    }
}

/// Create a data directory with a manifest and draft files for benchmarking
/// the startup preload pipeline.
fn make_draft_fixtures(
    n_tabs: usize,
    n_drafts: usize,
    draft_size: usize,
) -> (TempDir, SessionData) {
    let dir = TempDir::new().expect("expected operation to succeed");

    // Create real files for session tabs (filter_existing_tabs will stat them).
    let tab_dir = dir.path().join("project");
    fixture::create_dir_all(&tab_dir);
    let mut tabs = Vec::with_capacity(n_tabs);
    for i in 0..n_tabs {
        let file_path = tab_dir.join(format!("file_{i}.rs"));
        fixture::write_text(&file_path, "fn main() {}");
        tabs.push(SessionTab {
            path: Some(file_path),
            draft_id: None,
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
            pinned: false,
        });
    }

    // Create draft files + manifest for the first n_drafts tabs.
    let draft_content = "x".repeat(draft_size);
    let mut manifest = DraftManifest::default();
    for tab in tabs.iter().take(n_drafts) {
        if let Some(ref path) = tab.path {
            let draft_id = draft_service::draft_id_for_path(path);
            draft_service::write_draft(dir.path(), &draft_id, &draft_content)
                .expect("expected operation to succeed");
            manifest.upsert(DraftEntry {
                draft_id,
                original_path: Some(path.clone()),
                original_mtime_secs: editor_io::mtime_secs(path),
                saved_at_secs: 2000,
            });
        }
    }
    draft_service::save_manifest(dir.path(), &manifest).expect("expected operation to succeed");
    session_service::save(
        dir.path(),
        &SessionData {
            tabs: tabs.clone(),
            active_tab_index: Some(0),
        },
    )
    .expect("expected operation to succeed");

    let session = SessionData {
        tabs,
        active_tab_index: Some(0),
    };
    (dir, session)
}

/// Create untitled draft bodies at policy-scale sizes without large `String`s.
fn make_policy_sized_draft_fixtures(sizes: &[u64]) -> TempDir {
    let dir = TempDir::new().expect("expected operation to succeed");
    let drafts_dir = draft_service::drafts_dir(dir.path());
    fixture::create_dir_all(&drafts_dir);
    let mut manifest = DraftManifest::default();
    let mut tabs = Vec::with_capacity(sizes.len());
    for (index, size) in sizes.iter().copied().enumerate() {
        let draft_id = format!("policy-sized-{index}");
        fixture::write_repeated_bytes(&drafts_dir.join(format!("{draft_id}.draft")), b"x", size);
        manifest.upsert(DraftEntry {
            draft_id: draft_id.clone(),
            original_path: None,
            original_mtime_secs: None,
            saved_at_secs: 1,
        });
        tabs.push(SessionTab {
            path: None,
            draft_id: Some(draft_id),
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
            pinned: false,
        });
    }
    draft_service::save_manifest(dir.path(), &manifest).expect("save policy manifest");
    session_service::save(
        dir.path(),
        &SessionData {
            tabs,
            active_tab_index: Some(0),
        },
    )
    .expect("save policy session");
    dir
}

/// Build a `SessionData` with the given number of tabs.
fn make_session_data(n_tabs: usize) -> SessionData {
    SessionData {
        tabs: (0..n_tabs)
            .map(|i| SessionTab {
                path: Some(PathBuf::from(format!("/home/user/project/src/file_{i}.rs"))),
                draft_id: None,
                cursor_line: u32::try_from(i % 500).expect("benchmark fixture tab index fits"),
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            })
            .collect(),
        active_tab_index: Some(0),
    }
}

/// Create disposable files and preview records for Replace All benchmarks.
fn make_replace_all_fixture(
    file_count: usize,
    lines_per_file: usize,
) -> (TempDir, Vec<Replacement>) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let original_line = "prefix needle suffix";
    let replaced_line = "prefix thread suffix";
    let mut replacements = Vec::with_capacity(file_count * lines_per_file);

    for file_index in 0..file_count {
        let path = dir.path().join(format!("replace_{file_index}.txt"));
        let content = std::iter::repeat_n(original_line, lines_per_file)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fixture::write_text(&path, &content);
        for line_index in 0..lines_per_file {
            replacements.push(Replacement {
                match_id: SearchMatchId::from_index(replacements.len()),
                path: path.clone(),
                line_number: u64::try_from(line_index + 1)
                    .expect("benchmark line index fits in u64"),
                original_line: original_line.into(),
                replaced_line: replaced_line.to_string(),
                replacement: "thread".into(),
                match_range: 7..13,
            });
        }
    }

    (dir, replacements)
}

/// Create in-memory search matches for Replace preview generation benchmarks.
fn make_replace_preview_matches(match_count: usize) -> Vec<SearchMatch> {
    (0..match_count)
        .map(|index| {
            SearchMatch::new(
                PathBuf::from(format!("/synthetic/preview/file_{}.rs", index % 250)),
                u64::try_from(index + 1).expect("benchmark match index fits in u64"),
                &format!("let needle_{index} = needle;"),
                4..10,
            )
            .with_id(SearchMatchId::from_index(index))
        })
        .collect()
}

/// Create one file near the Replace All accepted-size cap with a single match.
fn make_replace_all_10mb_fixture() -> (TempDir, Vec<Replacement>) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let path = dir.path().join("accepted-10mb.txt");
    let original_line = "prefix needle suffix";
    let mut content = String::with_capacity(10 * 1024 * 1024);
    content.push_str(original_line);
    content.push('\n');
    content.extend(std::iter::repeat_n('x', (10 * 1024 * 1024) - content.len()));
    fixture::write_text(&path, &content);
    (
        dir,
        vec![Replacement {
            match_id: SearchMatchId::from_index(0),
            path,
            line_number: 1,
            original_line: original_line.into(),
            replaced_line: "prefix thread suffix".to_string(),
            replacement: "thread".into(),
            match_range: 7..13,
        }],
    )
}

/// Create a near-cap file of short lines with the maximum accepted replacements.
fn make_replace_all_dense_10mb_fixture() -> (TempDir, Vec<Replacement>, usize) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let path = dir.path().join("accepted-10mb-dense-lines.txt");
    let byte_limit = usize::try_from(content_search::MAX_REPLACE_FILE_BYTES)
        .expect("Replace All file-byte limit should fit usize");
    let source_lines = byte_limit / 2;
    fixture::write_text(&path, &"x\n".repeat(source_lines));
    let original_line: Arc<str> = Arc::from("x");
    let replacement_text: Arc<str> = Arc::from("y");
    let replacements = (0..MAX_REPLACE_PREVIEW_ROWS)
        .map(|index| {
            let line_index = index.saturating_mul(source_lines.saturating_sub(1))
                / MAX_REPLACE_PREVIEW_ROWS.saturating_sub(1);
            Replacement {
                match_id: SearchMatchId::from_index(index),
                path: path.clone(),
                line_number: u64::try_from(line_index + 1)
                    .expect("dense fixture line number should fit u64"),
                original_line: original_line.clone(),
                replaced_line: "y".to_string(),
                replacement: replacement_text.clone(),
                match_range: 0..1,
            }
        })
        .collect();
    (dir, replacements, source_lines)
}

/// Create one sparse file just over the Replace All cap so the benchmark tracks skip cost.
fn make_replace_all_over_cap_fixture() -> (TempDir, Vec<Replacement>) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let path = dir.path().join("over-cap.txt");
    fixture::create_sparse_file(&path, content_search::MAX_REPLACE_FILE_BYTES + 1);
    (
        dir,
        vec![Replacement {
            match_id: SearchMatchId::from_index(0),
            path,
            line_number: 1,
            original_line: "needle".into(),
            replaced_line: "thread".to_string(),
            replacement: "thread".into(),
            match_range: 0..6,
        }],
    )
}

fn make_malformed_recovery_fixture(corrupt_sidecars: usize) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let workspace = dir.path().join("workspace");
    fixture::create_dir_all(&workspace);
    fixture::write_text(&dir.path().join("session.json"), "{ malformed session");
    fixture::create_dir_all(&draft_service::drafts_dir(dir.path()));
    fixture::write_text(
        &draft_service::drafts_dir(dir.path()).join("manifest.json"),
        "{ malformed manifest",
    );
    fixture::write_text(
        &migration_ledger::ledger_path(dir.path()),
        "{ malformed migration ledger",
    );
    let bookmarks_dir = bookmark_service::bookmarks_dir(dir.path());
    fixture::create_dir_all(&bookmarks_dir);
    for i in 0..corrupt_sidecars {
        fixture::write_text(
            &bookmarks_dir.join(format!("corrupt-{i}.json")),
            "{ malformed bookmark sidecar",
        );
    }
    (dir, workspace)
}

fn make_pending_migration_fixture(entry_count: usize) -> TempDir {
    let dir = TempDir::new().expect("expected operation to succeed");
    let old_root = dir.path().join("old");
    let new_root = dir.path().join("new");
    fixture::create_dir_all(&old_root);
    fixture::create_dir_all(&new_root);
    for i in 0..entry_count {
        migration_ledger::record_pending(
            dir.path(),
            &old_root.join(format!("file-{i}.txt")),
            &new_root.join(format!("file-{i}.txt")),
            &[MigrationKind::Bookmarks, MigrationKind::DocumentNotes],
        )
        .expect("expected operation to succeed");
    }
    dir
}

fn make_duplicate_bookmark_sidecar_fixture() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let workspace = dir.path().join("workspace");
    fixture::create_dir_all(&workspace);
    let old_path = workspace.join("old.txt");
    let new_path = workspace.join("new.txt");
    fixture::write_text(&old_path, "old\n");
    fixture::write_text(&new_path, "new\n");
    bookmark_service::save_for_path(
        dir.path(),
        &old_path,
        &[BookmarkRecord::new(1, Some("old bookmark".to_string()))],
    )
    .expect("expected operation to succeed");
    bookmark_service::save_for_path(
        dir.path(),
        &new_path,
        &[BookmarkRecord::new(2, Some("new bookmark".to_string()))],
    )
    .expect("expected operation to succeed");
    (dir, old_path, new_path)
}

fn make_many_local_history_lineages_fixture(lineage_count: usize) -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let old_root = dir.path().join("old-root");
    let new_root = dir.path().join("new-root");
    fixture::create_dir_all(&old_root);
    fixture::create_dir_all(&new_root);
    for i in 0..lineage_count {
        let old_path = old_root.join(format!("file-{i}.txt"));
        let new_path = new_root.join(format!("file-{i}.txt"));
        fixture::write_text(&old_path, "old\n");
        fixture::write_text(&new_path, "new\n");
        local_history_service::capture_snapshot_for_path(
            dir.path(),
            &old_path,
            &format!("old snapshot {i}\n"),
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::PreserveDuplicate,
        )
        .expect("expected operation to succeed");
        if i % 4 == 0 {
            local_history_service::capture_snapshot_for_path(
                dir.path(),
                &new_path,
                &format!("target snapshot {i}\n"),
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::PreserveDuplicate,
            )
            .expect("expected operation to succeed");
        }
    }
    (dir, old_root, new_root)
}

fn make_mismatched_local_history_lineages_fixture(lineage_count: usize) -> TempDir {
    let dir = TempDir::new().expect("expected operation to succeed");
    let workspace = dir.path().join("workspace");
    fixture::create_dir_all(&workspace);
    for i in 0..lineage_count {
        let path = workspace.join(format!("reconcile-{i}.txt"));
        fixture::write_text(&path, "file\n");
        let identity = local_history_service::resolve_document_identity(&path)
            .expect("expected operation to succeed");
        let lineage_dir =
            local_history_service::local_history_dir(dir.path()).join(format!("stale-{i}"));
        fixture::create_dir_all(&lineage_dir);
        let text = format!("mismatched local-history body {i}\n");
        let meta = LocalHistorySnapshotMeta {
            snapshot_id: next_record_id("history"),
            captured_at_millis: now_epoch_millis(),
            origin: LocalHistorySnapshotOrigin::Save,
            byte_len: text.len() as u64,
            content_hash: stable_bytes_hash(text.as_bytes()),
        };
        fixture::write_text(
            &lineage_dir.join(format!("{}.txt", meta.snapshot_id)),
            &text,
        );
        let index_path = lineage_dir.join("index.json");
        let document = LocalHistoryDocument {
            identity,
            snapshots: vec![meta],
        };
        save_enveloped_json_path(
            &RecoveryLoadConfig::new(
                dir.path(),
                &index_path,
                RecoveryMetadataClass::LocalHistoryIndex,
            ),
            KIND_LOCAL_HISTORY_INDEX,
            &document,
        )
        .expect("expected operation to succeed");
    }
    dir
}

fn make_first_dirty_autosave_fixture(
    draft_count: usize,
    draft_size: usize,
) -> (TempDir, Vec<String>) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let ids = (0..draft_count)
        .map(|i| format!("untitled-{i:016x}"))
        .collect::<Vec<_>>();
    fixture::create_dir_all(&draft_service::drafts_dir(dir.path()));
    fixture::write_text(&dir.path().join("seed.txt"), "seed\n");
    let _content = "x".repeat(draft_size);
    (dir, ids)
}

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

fn bench_fuzzy_score(c: &mut Criterion) {
    let mut group = c.benchmark_group("fuzzy_score");

    let cases: &[(&str, &str, &str)] = &[
        ("exact_short", "main", "main.rs"),
        ("exact_long", "workspace_manager", "workspace_manager.rs"),
        ("prefix", "ma", "main.rs"),
        ("subsequence", "wm", "workspace_manager.rs"),
        ("no_match", "xyz", "main.rs"),
        ("empty_query", "", "workspace_manager.rs"),
        (
            "long_path",
            "ctrl",
            "src/ui/window/actions/keyboard_controller.rs",
        ),
    ];

    for &(id, query, candidate) in cases {
        group.bench_with_input(
            BenchmarkId::new("case", id),
            &(query, candidate),
            |b, &(q, c)| b.iter(|| palette::fuzzy_score(black_box(q), black_box(c))),
        );
    }
    group.finish();
}

fn bench_recent_document_search(c: &mut Criterion) {
    let rows = (0..200)
        .map(|index| {
            let path = PathBuf::from(format!(
                "/workspace/projet-équipe/section-{}/nested/document-{index:03}-résumé.rs",
                index % 20
            ));
            RecentDocumentRow::from_entry(
                &RecentDocumentEntry::new(path, None, 10_000 - index),
                10_000,
            )
        })
        .collect::<Vec<_>>();
    let mut group = c.benchmark_group("recent_document_search_200_rows");
    let cases = [
        ("prefix", "document"),
        ("substring_unicode", "résumé"),
        ("fuzzy", "dcmnt"),
        ("fuzzy_unicode", "rsme"),
        ("deep_path", "équipe/section-19"),
        ("no_match", "zzqvv"),
    ];

    for (id, query) in cases {
        group.bench_with_input(BenchmarkId::new("query", id), query, |b, query| {
            b.iter(|| recent_documents::search_rows(black_box(&rows), black_box(query)));
        });
    }
    group.finish();
}

fn bench_file_index_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_index_search");

    for size in [100, 1_000, 10_000, 50_000, 100_000] {
        let index = make_synthetic_index(size);

        group.bench_with_input(BenchmarkId::new("query_match", size), &index, |b, idx| {
            b.iter(|| idx.search(black_box("file_42"), 50));
        });

        group.bench_with_input(BenchmarkId::new("empty_query", size), &index, |b, idx| {
            b.iter(|| idx.search(black_box(""), 50));
        });

        group.bench_with_input(BenchmarkId::new("no_match", size), &index, |b, idx| {
            b.iter(|| idx.search(black_box("zzzzz"), 50));
        });
    }
    group.finish();
}

fn bench_palette_pipeline_hardening(c: &mut Criterion) {
    let index = make_synthetic_index(100_000);
    let evidence_cancel = palette::PaletteSearchCancellation::default();
    let evidence = index.search_cancellable("file", 50, &evidence_cancel);
    let evidence_metrics = evidence.metrics();
    let coordinator = RefCell::new(palette::PaletteSearchCoordinator::default());
    let active = coordinator
        .borrow_mut()
        .submit("file")
        .expect("first benchmark query starts");
    let replacements_submitted = Cell::new(false);
    let cancelled = index.search_cancellable_with_progress(
        active.request,
        50,
        &active.cancellation,
        &|examined| {
            if examined >= palette::PALETTE_CANCEL_CHECK_INTERVAL
                && !replacements_submitted.replace(true)
            {
                for query in ["f", "fi", "file", "file_9", "file_99999"] {
                    let _ = coordinator.borrow_mut().submit(query);
                }
            }
        },
    );
    let palette::PaletteSearchOutcome::Cancelled {
        metrics: cancelled_metrics,
    } = cancelled
    else {
        panic!("evidence scan must cancel after deterministic progress");
    };
    let ownership = coordinator.borrow().snapshot();
    let latest = coordinator
        .borrow_mut()
        .finish(active.generation)
        .expect("latest benchmark query starts");
    let ownership_after_handoff = coordinator.borrow().snapshot();
    eprintln!(
        "palette-pipeline-evidence corpus=100000 retained_peak={} examined={} matching={} cancelled_examined={} active_high_water={} pending_high_water={} started={} final_query={}",
        evidence_metrics.peak_retained_per_source,
        evidence_metrics.candidates_examined,
        evidence_metrics.matching_candidates,
        cancelled_metrics.candidates_examined,
        ownership.active_high_water,
        ownership.pending_high_water,
        ownership_after_handoff.started,
        latest.request,
    );

    let mut group = c.benchmark_group("palette_pipeline_hardening_100000");
    group.sample_size(20);
    for (case, query) in [
        ("high_hit", "file"),
        ("medium_hit", "file_42"),
        ("unicode", "résumé"),
        ("ties", "equal-score-tie"),
        ("no_hit", "zzqvv"),
    ] {
        for limit in [1usize, 10, 50, 500] {
            group.bench_function(format!("bounded/{case}/limit_{limit}"), |b| {
                b.iter(|| black_box(index.search(black_box(query), black_box(limit))));
            });
            group.bench_function(format!("full_sort_reference/{case}/limit_{limit}"), |b| {
                b.iter(|| {
                    black_box(index.search_full_sort_reference(black_box(query), black_box(limit)))
                });
            });
        }
    }

    group.bench_function("cancelled_before_scan", |b| {
        b.iter(|| {
            let cancellation = palette::PaletteSearchCancellation::default();
            let _ = cancellation.cancel();
            black_box(index.search_cancellable("file", 50, &cancellation))
        });
    });
    group.bench_function("cancelled_during_scan", |b| {
        b.iter(|| {
            let cancellation = palette::PaletteSearchCancellation::default();
            let cancellation_requested = Cell::new(false);
            let outcome =
                index.search_cancellable_with_progress("file", 50, &cancellation, &|examined| {
                    if examined >= palette::PALETTE_CANCEL_CHECK_INTERVAL
                        && !cancellation_requested.replace(true)
                    {
                        let _ = cancellation.cancel();
                    }
                });
            let palette::PaletteSearchOutcome::Cancelled { metrics } = outcome else {
                panic!("checkpoint hook must cancel an active scan");
            };
            assert_eq!(
                metrics.candidates_examined,
                palette::PALETTE_CANCEL_CHECK_INTERVAL
            );
            black_box(metrics)
        });
    });
    group.bench_function("rapid_latest_query_replacement", |b| {
        b.iter(|| {
            let coordinator = RefCell::new(palette::PaletteSearchCoordinator::default());
            let active = coordinator
                .borrow_mut()
                .submit("file")
                .expect("first query starts");
            let replacements_submitted = Cell::new(false);
            let cancelled = index.search_cancellable_with_progress(
                active.request,
                50,
                &active.cancellation,
                &|examined| {
                    if examined >= palette::PALETTE_CANCEL_CHECK_INTERVAL
                        && !replacements_submitted.replace(true)
                    {
                        for query in ["f", "fi", "fil", "file", "file_9", "file_99999"] {
                            let _ = coordinator.borrow_mut().submit(query);
                        }
                    }
                },
            );
            assert!(matches!(
                cancelled,
                palette::PaletteSearchOutcome::Cancelled { .. }
            ));
            let latest = coordinator
                .borrow_mut()
                .finish(active.generation)
                .expect("latest query starts");
            let rows = index.search(latest.request, 50);
            let _ = coordinator.borrow_mut().finish(latest.generation);
            let snapshot = coordinator.borrow().snapshot();
            assert_eq!(snapshot.active_high_water, 1);
            assert_eq!(snapshot.pending_high_water, 1);
            assert_eq!(snapshot.active, 0);
            assert_eq!(snapshot.pending, 0);
            black_box((rows, snapshot))
        });
    });
    group.finish();
}

fn bench_file_index_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_index_rebuild");
    group.sample_size(20);

    let directory_evidence = make_directory_only_tree(1_000);
    let directory_outcome = FileIndex::rebuild_cancellable_with_hint(
        &[directory_evidence.path().to_path_buf()],
        0,
        &palette::PaletteSearchCancellation::default(),
    );
    let palette::FileIndexBuildOutcome::Complete {
        metrics: directory_metrics,
        ..
    } = directory_outcome
    else {
        panic!("fresh directory-only index evidence must complete");
    };
    assert!(directory_metrics.peak_retained_directories <= palette::MAX_INDEXED_DIRECTORIES);
    assert_eq!(directory_metrics.retained_files, 0);
    eprintln!(
        "file-index-directory-bound-evidence fixture_directories=1000 retained_files={} peak_retained_directories={} directory_limit={}",
        directory_metrics.retained_files,
        directory_metrics.peak_retained_directories,
        palette::MAX_INDEXED_DIRECTORIES,
    );

    let common = make_temp_dir_tree(10_000);
    let common_outcome = FileIndex::rebuild_cancellable_with_hint(
        &[common.path().to_path_buf()],
        10_000,
        &palette::PaletteSearchCancellation::default(),
    );
    let palette::FileIndexBuildOutcome::Complete {
        index: common_index,
        metrics: common_metrics,
    } = common_outcome
    else {
        panic!("fresh common file-index evidence must complete");
    };
    assert_eq!(common_index.len(), 10_000);
    assert!(common_metrics.peak_build_bytes <= palette::MAX_FILE_INDEX_BUILD_RETAINED_BYTES);
    assert!(common_metrics.retained_index_bytes <= palette::MAX_FILE_INDEX_RETAINED_BYTES);

    let missing_parent = TempDir::new().expect("file-index missing-root tempdir");
    let missing_roots = (0..1_000)
        .map(|index| missing_parent.path().join(format!("removed-{index:04}")))
        .collect::<Vec<_>>();
    let missing_outcome = FileIndex::rebuild_cancellable_with_hint(
        &missing_roots,
        0,
        &palette::PaletteSearchCancellation::default(),
    );
    let palette::FileIndexBuildOutcome::Complete {
        index: missing_index,
        ..
    } = missing_outcome
    else {
        panic!("fresh missing-root file-index evidence must complete");
    };
    assert!(missing_index.is_empty());

    let near_policy = make_near_policy_long_path_tree(10_000);
    let near_policy_outcome = FileIndex::rebuild_cancellable_with_hint(
        &[near_policy.path().to_path_buf()],
        10_000,
        &palette::PaletteSearchCancellation::default(),
    );
    let palette::FileIndexBuildOutcome::Complete {
        index: near_policy_index,
        metrics: near_policy_metrics,
    } = near_policy_outcome
    else {
        panic!("fresh near-policy file-index evidence must complete");
    };
    assert!(!near_policy_index.is_empty());
    assert!(
        near_policy_metrics.retained_index_bytes
            >= palette::MAX_FILE_INDEX_RETAINED_BYTES.saturating_mul(3) / 4
    );
    assert!(
        near_policy_metrics.peak_build_bytes
            >= palette::MAX_FILE_INDEX_BUILD_RETAINED_BYTES.saturating_mul(3) / 4
    );
    assert!(near_policy_metrics.peak_build_bytes <= palette::MAX_FILE_INDEX_BUILD_RETAINED_BYTES);
    assert!(near_policy_metrics.retained_index_bytes <= palette::MAX_FILE_INDEX_RETAINED_BYTES);
    eprintln!(
        "file-index-near-policy-evidence fixture_files=10000 retained_files={} retained_index_bytes={} installed_limit={} peak_build_bytes={} build_limit={} truncation={:?}",
        near_policy_index.len(),
        near_policy_metrics.retained_index_bytes,
        palette::MAX_FILE_INDEX_RETAINED_BYTES,
        near_policy_metrics.peak_build_bytes,
        palette::MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
        near_policy_metrics.truncation,
    );

    group.bench_function("common_mixed/10000", |b| {
        b.iter(|| {
            FileIndex::rebuild_cancellable_with_hint(
                black_box(&[common.path().to_path_buf()]),
                10_000,
                &palette::PaletteSearchCancellation::default(),
            )
        });
    });
    group.bench_function("missing_workspace_folders/1000", |b| {
        b.iter(|| {
            FileIndex::rebuild_cancellable_with_hint(
                black_box(&missing_roots),
                0,
                &palette::PaletteSearchCancellation::default(),
            )
        });
    });
    group.bench_function("near_policy_long_paths/10000", |b| {
        b.iter(|| {
            FileIndex::rebuild_cancellable_with_hint(
                black_box(&[near_policy.path().to_path_buf()]),
                10_000,
                &palette::PaletteSearchCancellation::default(),
            )
        });
    });

    for file_count in [50, 500, 1_000, 5_000, 10_000, 100_000] {
        group.bench_function(BenchmarkId::from_parameter(file_count), |b| {
            b.iter_batched(
                || make_temp_dir_tree(file_count),
                |dir| {
                    let result = FileIndex::rebuild(black_box(&[dir.path().to_path_buf()]));
                    (result, dir) // keep TempDir alive past timing — drop runs after measurement
                },
                BatchSize::SmallInput,
            );
        });
    }
    for directory_count in [1_000, 10_000] {
        group.bench_function(BenchmarkId::new("directory_only", directory_count), |b| {
            b.iter_batched(
                || make_directory_only_tree(directory_count),
                |dir| {
                    let outcome = FileIndex::rebuild_cancellable_with_hint(
                        black_box(&[dir.path().to_path_buf()]),
                        0,
                        &palette::PaletteSearchCancellation::default(),
                    );
                    let palette::FileIndexBuildOutcome::Complete { metrics, .. } = &outcome else {
                        panic!("fresh file-index evidence must complete");
                    };
                    assert!(metrics.peak_retained_directories <= palette::MAX_INDEXED_DIRECTORIES);
                    assert_eq!(metrics.retained_files, 0);
                    (outcome, dir)
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_end_to_end_boundedness(c: &mut Criterion) {
    let flat = make_flat_dir(10_000);
    let cancellation = palette::PaletteSearchCancellation::default();
    let file_outcome = FileIndex::rebuild_cancellable_with_hint(
        &[flat.path().to_path_buf()],
        10_000,
        &cancellation,
    );
    let palette::FileIndexBuildOutcome::Complete {
        index: file_index,
        metrics: file_metrics,
    } = file_outcome
    else {
        panic!("fresh file-index evidence must complete");
    };
    assert!(file_metrics.retained_files <= palette::MAX_INDEXED_FILES);
    assert!(file_metrics.peak_retained_directories <= palette::MAX_INDEXED_DIRECTORIES);
    assert!(
        file_metrics.peak_retained_directory_entries
            <= palette::MAX_INDEXED_FILES + palette::MAX_INDEXED_DIRECTORIES
    );

    let note_fixture = TempDir::new().expect("real note-source fixture");
    let deep_data_dir = note_fixture
        .path()
        .join("dados-équipe")
        .join("東京-project")
        .join("🙂-notes");
    let note_workspace = note_fixture.path().join("workspace");
    fixture::create_dir_all(&deep_data_dir);
    fixture::create_dir_all(&note_workspace);
    let noted_file = note_workspace.join("main.rs");
    fixture::write_text(&noted_file, "fn main() {}\n");
    lushtext_core::services::bookmark_service::save_for_path(
        &deep_data_dir,
        &noted_file,
        &[BookmarkRecord::new(
            0,
            Some("Unicode production-routed fixture".to_string()),
        )],
    )
    .expect("save benchmark bookmark sidecar");
    let malformed_path = lushtext_core::services::bookmark_service::bookmarks_dir(&deep_data_dir)
        .join("malformed-🙂.json");
    fixture::write_text(&malformed_path, "{ malformed recovery fixture");
    let note_scope = WorkspacesFile {
        current_scope: WorkspaceScope::All,
        workspaces: vec![WorkspaceConfig::with_one_folder(
            WorkspaceId::new("real-note-source"),
            "Real Note Source",
            note_workspace,
        )],
    }
    .current_scope_snapshot();
    let real_note_outcome = palette::load_note_entries_bounded_for_scope(
        &deep_data_dir,
        &note_scope,
        &[],
        false,
        palette::NotesBrowserMode::Bookmarks,
        palette::PALETTE_NOTE_SOURCE_LIMITS,
        &palette::PaletteSearchCancellation::default(),
    )
    .expect("load production-routed note fixture");
    let palette::PaletteNoteSourceOutcome::Complete {
        load: real_note_load,
        metrics: real_note_metrics,
    } = &real_note_outcome
    else {
        panic!("fresh real note source must complete");
    };
    assert_eq!(real_note_load.entries.len(), 1);
    assert!(real_note_metrics.peak_sidecar_path_bytes > 0);
    assert!(real_note_metrics.peak_construction_bytes > 0);
    assert!(!real_note_load.diagnostics.is_empty());

    let entry_bodies = (0..=palette::MAX_PALETTE_NOTE_ENTRIES)
        .map(|index| format!("note-{index:05}"))
        .collect::<Vec<_>>();
    let entry_outcome = palette::admit_synthetic_note_bodies_for_benchmark(&entry_bodies, None);
    let palette::PaletteNoteSourceOutcome::Complete {
        metrics: entry_metrics,
        ..
    } = &entry_outcome
    else {
        panic!("entry-budget evidence must complete");
    };
    assert_eq!(
        entry_metrics.retained_entries,
        palette::MAX_PALETTE_NOTE_ENTRIES
    );

    let byte_bodies = (0..65).map(|_| "n".repeat(1024 * 1024)).collect::<Vec<_>>();
    let byte_outcome = palette::admit_synthetic_note_bodies_for_benchmark(&byte_bodies, None);
    let palette::PaletteNoteSourceOutcome::Complete {
        metrics: byte_metrics,
        ..
    } = &byte_outcome
    else {
        panic!("byte-budget evidence must complete");
    };
    assert!(byte_metrics.retained_searchable_bytes <= palette::MAX_PALETTE_NOTE_TEXT_BYTES);
    let cancelled_outcome =
        palette::admit_synthetic_note_bodies_for_benchmark(&entry_bodies, Some(256));
    let palette::PaletteNoteSourceOutcome::Cancelled {
        metrics: cancelled_note_metrics,
    } = &cancelled_outcome
    else {
        panic!("deterministic note-source cancellation must cancel");
    };
    assert_eq!(cancelled_note_metrics.retained_entries, 256);

    let excluded_path = file_index.files()[0]
        .identity
        .canonical_path()
        .expect("benchmark index identities are canonical")
        .to_path_buf();
    let excluded = HashSet::from([excluded_path.clone()]);
    let exclusion = file_index.search_cancellable_excluding(
        "",
        1,
        &excluded,
        &palette::PaletteSearchCancellation::default(),
    );
    let exclusion_metrics = exclusion.metrics();
    let palette::PaletteSearchOutcome::Complete { value: rows, .. } = exclusion else {
        panic!("fresh canonical exclusion must complete");
    };
    assert_eq!(rows.len(), 1);
    let SearchResultItem::File(fallback) = rows[0].item else {
        panic!("file-index exclusion must return a file row");
    };
    assert_ne!(
        fallback.identity.canonical_path(),
        Some(excluded_path.as_path())
    );

    let request = palette::FileIndexBuildRequest {
        workspace_folders: Arc::from([flat.path().to_path_buf()]),
        capacity_hint: 10_000,
    };
    let mut file_coordinator = palette::FileIndexBuildCoordinator::default();
    let active = file_coordinator
        .submit(request.clone())
        .expect("first build starts");
    for capacity_hint in [1, 10, 100, 1_000, 10_000] {
        let mut latest = request.clone();
        latest.capacity_hint = capacity_hint;
        let _ = file_coordinator.submit(latest);
    }
    let latest = file_coordinator
        .finish(active.generation)
        .expect("latest build starts after active terminal");
    assert_eq!(latest.request.capacity_hint, 10_000);
    let file_ownership = file_coordinator.snapshot();
    assert_eq!((file_ownership.active, file_ownership.pending), (1, 0));

    let scope_snapshot = WorkspacesFile::default().current_scope_snapshot();
    let note_request = palette::NoteSourceRefreshRequest {
        data_dir: flat.path().to_path_buf(),
        scope_snapshot,
        open_editor_snapshots: Arc::from(Vec::<PaletteOpenEditorNoteSnapshot>::new()),
        open_editor_snapshots_truncated: false,
        mode: palette::NotesBrowserMode::AllNotes,
        limits: palette::PALETTE_NOTE_SOURCE_LIMITS,
    };
    let mut note_coordinator = palette::NoteSourceRefreshCoordinator::default();
    let active_note = note_coordinator
        .submit(note_request.clone())
        .expect("first note refresh starts");
    for _ in 0..5 {
        let _ = note_coordinator.submit(note_request.clone());
    }
    let _latest_note = note_coordinator
        .finish(active_note.generation)
        .expect("latest note refresh starts");
    let note_ownership = note_coordinator.snapshot();
    assert_eq!((note_ownership.active, note_ownership.pending), (1, 0));

    let page = fs_tree::scan_directory_page_after(
        flat.path(),
        None,
        DirectoryScanPolicy {
            max_entries: 2_048,
            include_hidden: false,
        },
    )
    .expect("bounded directory page");
    assert_eq!(page.entries.len(), 2_048);
    assert!(page.has_more);

    let current_rows = (0..10_000)
        .map(|index| DirectoryRowState {
            path: Some(PathBuf::from(format!("row-{index:05}"))),
            is_dir: false,
            is_empty: None,
            is_placeholder: false,
        })
        .collect::<Vec<_>>();
    let mut desired_rows = current_rows.clone();
    desired_rows.splice(
        2_500..7_500,
        (0..5_000).map(|index| DirectoryRowState {
            path: Some(PathBuf::from(format!("changed-{index:05}"))),
            is_dir: false,
            is_empty: None,
            is_placeholder: false,
        }),
    );
    let reconciliation = file_tree::plan_directory_reconciliation(&current_rows, &desired_rows);
    let DirectoryReconciliationPlan::Splice {
        removed,
        replacement,
        ..
    } = &reconciliation
    else {
        panic!("broad reconciliation must produce one compact splice");
    };
    assert_eq!((*removed, replacement.len()), (5_000, 5_000));

    let replacement_plan = BufferReplacementPlan::for_sizes(2_000_000, 2_000_000);
    eprintln!(
        "end-to-end-boundedness-evidence flat_entries=10000 retained_files={} file_peak_rows={} note_entries={} note_bytes={} note_retained_bytes={} note_sidecar_path_peak={} note_construction_peak={} note_truncations={} real_note_entries={} real_note_sidecars={} real_note_sidecar_path_peak={} real_note_construction_peak={} real_note_diagnostics={} cancelled_note_entries={} cancelled_note_construction_peak={} canonical_examined={} file_active={} file_pending={} note_active={} note_pending={} cleanup_page={} cleanup_has_more={} reconcile_removed={} reconcile_inserted={} replacement_mode={:?} clear_slice_chars={} insert_slice_bytes={}",
        file_metrics.retained_files,
        file_metrics.peak_retained_directory_entries,
        entry_metrics.retained_entries,
        byte_metrics.retained_searchable_bytes,
        byte_metrics.retained_bytes,
        byte_metrics.peak_sidecar_path_bytes,
        byte_metrics.peak_construction_bytes,
        byte_metrics.truncation_reasons.len(),
        real_note_metrics.retained_entries,
        real_note_metrics.loaded_sidecars,
        real_note_metrics.peak_sidecar_path_bytes,
        real_note_metrics.peak_construction_bytes,
        real_note_load.diagnostics.len(),
        cancelled_note_metrics.retained_entries,
        cancelled_note_metrics.peak_construction_bytes,
        exclusion_metrics.candidates_examined,
        file_ownership.active,
        file_ownership.pending,
        note_ownership.active,
        note_ownership.pending,
        page.entries.len(),
        page.has_more,
        removed,
        replacement.len(),
        replacement_plan.mode,
        REPLACEMENT_CLEAR_SLICE_CHARS,
        REPLACEMENT_INSERT_SLICE_BYTES,
    );

    let mut group = c.benchmark_group("end_to_end_boundedness");
    group.sample_size(10);
    group.bench_function("file_index/flat_10000", |b| {
        b.iter(|| {
            black_box(FileIndex::rebuild_cancellable_with_hint(
                black_box(&[flat.path().to_path_buf()]),
                10_000,
                &palette::PaletteSearchCancellation::default(),
            ))
        });
    });
    group.bench_function("note_source/entry_budget", |b| {
        b.iter(|| {
            black_box(palette::admit_synthetic_note_bodies_for_benchmark(
                black_box(&entry_bodies),
                None,
            ))
        });
    });
    group.bench_function("note_source/byte_budget", |b| {
        b.iter(|| {
            black_box(palette::admit_synthetic_note_bodies_for_benchmark(
                black_box(&byte_bodies),
                None,
            ))
        });
    });
    group.bench_function("note_source/cancel_after_256", |b| {
        b.iter(|| {
            black_box(palette::admit_synthetic_note_bodies_for_benchmark(
                black_box(&entry_bodies),
                Some(256),
            ))
        });
    });
    group.bench_function("note_source/real_sidecars_unicode_paths", |b| {
        b.iter(|| {
            black_box(
                palette::load_note_entries_bounded_for_scope(
                    black_box(&deep_data_dir),
                    black_box(&note_scope),
                    &[],
                    false,
                    palette::NotesBrowserMode::Bookmarks,
                    palette::PALETTE_NOTE_SOURCE_LIMITS,
                    &palette::PaletteSearchCancellation::default(),
                )
                .expect("real note-source benchmark load"),
            )
        });
    });
    group.bench_function("canonical_exclusion/before_top_one", |b| {
        b.iter(|| {
            black_box(file_index.search_cancellable_excluding(
                "",
                1,
                black_box(&excluded),
                &palette::PaletteSearchCancellation::default(),
            ))
        });
    });
    group.bench_function("cleanup_page/flat_10000_cap_2048", |b| {
        b.iter(|| {
            black_box(fs_tree::scan_directory_page_after(
                flat.path(),
                None,
                DirectoryScanPolicy {
                    max_entries: 2_048,
                    include_hidden: false,
                },
            ))
        });
    });
    group.bench_function("tree_reconciliation/middle_10000", |b| {
        b.iter(|| {
            black_box(file_tree::plan_directory_reconciliation(
                black_box(&current_rows),
                black_box(&desired_rows),
            ))
        });
    });
    group.bench_function("buffer_replacement/policy_large_unicode_bytes", |b| {
        b.iter(|| black_box(BufferReplacementPlan::for_sizes(2_000_000, 2_000_000)));
    });
    group.finish();
}

fn bench_search_all(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_all");

    for size in [10_000, 100_000] {
        let index = make_synthetic_index(size);
        for mode in [
            SearchMode::Files,
            SearchMode::Notes,
            SearchMode::Commands,
            SearchMode::All,
        ] {
            let label = match mode {
                SearchMode::Files => "files",
                SearchMode::Notes => "notes",
                SearchMode::Commands => "commands",
                SearchMode::All => "all",
            };
            group.bench_with_input(BenchmarkId::new(label, size), &index, |b, idx| {
                b.iter(|| palette::search_all(black_box(idx), black_box("file_42"), mode, 50));
            });
        }
    }
    group.finish();
}

fn bench_scan_directory(c: &mut Criterion) {
    let mut group = c.benchmark_group("scan_directory");
    group.sample_size(30);

    for entry_count in [10, 100, 1_000, 5_000, 10_000] {
        group.bench_function(BenchmarkId::from_parameter(entry_count), |b| {
            b.iter_batched(
                || make_flat_dir(entry_count),
                |dir| {
                    let result = file_tree::scan_directory(black_box(dir.path()));
                    (result, dir) // keep TempDir alive past timing
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_json_persistence(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_persistence");

    let small = make_workspaces_file(1, 2);
    let large = make_workspaces_file(10, 50);

    // Save benchmarks
    group.bench_function("save/small", |b| {
        b.iter_batched(
            || TempDir::new().expect("expected operation to succeed"),
            |dir| {
                workspace_manager::save(dir.path(), black_box(&small))
                    .expect("expected operation to succeed");
                dir // keep TempDir alive past timing
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("save/large", |b| {
        b.iter_batched(
            || TempDir::new().expect("expected operation to succeed"),
            |dir| {
                workspace_manager::save(dir.path(), black_box(&large))
                    .expect("expected operation to succeed");
                dir
            },
            BatchSize::SmallInput,
        );
    });

    // Load benchmarks — pre-write the file, then benchmark reads
    group.bench_function("load/small", |b| {
        b.iter_batched(
            || {
                let dir = TempDir::new().expect("expected operation to succeed");
                workspace_manager::save(dir.path(), &small).expect("expected operation to succeed");
                dir
            },
            |dir| {
                let _: WorkspacesFile = workspace_manager::load(black_box(dir.path()))
                    .expect("expected operation to succeed");
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("load/large", |b| {
        b.iter_batched(
            || {
                let dir = TempDir::new().expect("expected operation to succeed");
                workspace_manager::save(dir.path(), &large).expect("expected operation to succeed");
                dir
            },
            |dir| {
                let _: WorkspacesFile = workspace_manager::load(black_box(dir.path()))
                    .expect("expected operation to succeed");
            },
            BatchSize::SmallInput,
        );
    });

    // Session save/load
    let session = make_session_data(50);
    group.bench_function("session_save/50_tabs", |b| {
        b.iter_batched(
            || TempDir::new().expect("expected operation to succeed"),
            |dir| {
                session_service::save(dir.path(), black_box(&session))
                    .expect("expected operation to succeed");
                dir // keep TempDir alive past timing
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("session_load/50_tabs", |b| {
        b.iter_batched(
            || {
                let dir = TempDir::new().expect("expected operation to succeed");
                session_service::save(dir.path(), &session).expect("expected operation to succeed");
                dir
            },
            |dir| {
                let _: SessionData = session_service::load(black_box(dir.path()))
                    .expect("expected operation to succeed");
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_utf8_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("utf8_validation");
    group.sample_size(20);

    for size_mb in [1, 5, 10, 50] {
        let size = size_mb * 1_000_000;
        // Generate valid UTF-8 content (repeating ASCII is fast to create)
        let content = "a".repeat(size);

        group.bench_function(
            BenchmarkId::new("boundary_text", format!("{size_mb}MB")),
            |b| {
                b.iter_batched(
                    || {
                        let dir = TempDir::new().expect("expected operation to succeed");
                        let path = dir.path().join("bench.txt");
                        fixture::write_text(&path, &content);
                        (dir, path)
                    },
                    |(dir, path)| {
                        let _s =
                            fs_read::text(black_box(&path)).expect("expected operation to succeed");
                        dir // keep alive
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_function(
            BenchmarkId::new("read_simdutf8", format!("{size_mb}MB")),
            |b| {
                b.iter_batched(
                    || {
                        let dir = TempDir::new().expect("expected operation to succeed");
                        let path = dir.path().join("bench.txt");
                        fixture::write_text(&path, &content);
                        (dir, path)
                    },
                    |(dir, path)| {
                        let bytes = fs_read::bytes(black_box(&path))
                            .expect("expected operation to succeed");
                        simdutf8::basic::from_utf8(&bytes).expect("expected operation to succeed");
                        // SAFETY: `simdutf8` just validated that `bytes` is well-formed UTF-8.
                        let _s = unsafe { String::from_utf8_unchecked(bytes) };
                        dir // keep alive
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_editor_file_io(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor_file_io");
    group.sample_size(20);

    for size_mb in [1, 10, 50] {
        let size = size_mb * 1_000_000;
        let content = "a".repeat(size);

        group.bench_function(
            BenchmarkId::new("load_text_file", format!("{size_mb}MB")),
            |b| {
                b.iter_batched(
                    || {
                        let dir = TempDir::new().expect("expected operation to succeed");
                        let path = dir.path().join("bench.txt");
                        fixture::write_text(&path, &content);
                        (dir, path, AtomicBool::new(false))
                    },
                    |(dir, path, cancel)| {
                        let _loaded = editor_io::load_text_file(
                            black_box(path.as_path()),
                            black_box(&cancel),
                        )
                        .expect("expected operation to succeed");
                        dir
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_function(
            BenchmarkId::new("write_snapshot_to_path", format!("{size_mb}MB")),
            |b| {
                b.iter_batched(
                    || {
                        let dir = TempDir::new().expect("expected operation to succeed");
                        let path = dir.path().join("bench.txt");
                        (dir, path, content.clone())
                    },
                    |(dir, path, text)| {
                        let _written =
                            editor_io::write_snapshot_to_path(black_box(&path), black_box(&text))
                                .expect("expected operation to succeed");
                        dir
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        let windows_1252_text = "café\r\n".repeat(size / 6);
        let lossy_windows_1252_text = "café😀\n".repeat(size / 10);
        let lossy_shift_jis_text = "日本語😀\n".repeat(size / 17);

        group.bench_function(
            BenchmarkId::new("analyze_utf16_lossless", format!("{size_mb}MB")),
            |b| {
                b.iter(|| {
                    black_box(editor_io::analyze_lossy_encoding(
                        black_box(&content),
                        DocumentEncoding::Utf16Le,
                    ))
                });
            },
        );

        group.bench_function(
            BenchmarkId::new("analyze_windows1252_lossless", format!("{size_mb}MB")),
            |b| {
                b.iter(|| {
                    black_box(editor_io::analyze_lossy_encoding(
                        black_box(&windows_1252_text),
                        DocumentEncoding::Windows1252,
                    ))
                });
            },
        );

        group.bench_function(
            BenchmarkId::new("analyze_windows1252_lossy", format!("{size_mb}MB")),
            |b| {
                b.iter(|| {
                    black_box(editor_io::analyze_lossy_encoding(
                        black_box(&lossy_windows_1252_text),
                        DocumentEncoding::Windows1252,
                    ))
                });
            },
        );

        group.bench_function(
            BenchmarkId::new("analyze_shift_jis_lossy", format!("{size_mb}MB")),
            |b| {
                b.iter(|| {
                    black_box(editor_io::analyze_lossy_encoding(
                        black_box(&lossy_shift_jis_text),
                        DocumentEncoding::ShiftJis,
                    ))
                });
            },
        );

        group.bench_function(
            BenchmarkId::new("load_text_file_windows1252", format!("{size_mb}MB")),
            |b| {
                b.iter_batched(
                    || {
                        let dir = TempDir::new().expect("expected operation to succeed");
                        let path = dir.path().join("bench-1252.txt");
                        let (bytes, _, _) = DocumentEncoding::Windows1252
                            .codec()
                            .encode(&windows_1252_text);
                        fixture::write_bytes(&path, bytes.as_ref());
                        (dir, path, AtomicBool::new(false))
                    },
                    |(dir, path, cancel)| {
                        let _loaded = editor_io::load_text_file_with_encoding(
                            black_box(path.as_path()),
                            black_box(&cancel),
                            None,
                        )
                        .expect("expected operation to succeed");
                        dir
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_function(
            BenchmarkId::new("write_document_windows1252_crlf", format!("{size_mb}MB")),
            |b| {
                b.iter_batched(
                    || {
                        let dir = TempDir::new().expect("expected operation to succeed");
                        let path = dir.path().join("bench-1252.txt");
                        (dir, path, windows_1252_text.clone())
                    },
                    |(dir, path, text)| {
                        let _written = editor_io::write_document_to_path(
                            black_box(&path),
                            black_box(&text),
                            DocumentEncoding::Windows1252,
                            LineEnding::Crlf,
                            false,
                        )
                        .expect("expected operation to succeed");
                        dir
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_line_ending_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_ending_detection");
    let fixtures = [
        ("empty", String::new()),
        ("lf_100k", "plain line\n".repeat(100_000)),
        ("crlf_100k", "plain line\r\n".repeat(100_000)),
        ("mixed_large", "plain🙂\r\nplain\nplain\r".repeat(100_000)),
    ];

    for (label, text) in fixtures {
        group.bench_function(label, |b| {
            b.iter(|| editor_io::detect_line_endings(black_box(&text)));
        });
    }
    group.finish();
}

fn bench_replace_undo_workflows(c: &mut Criterion) {
    if std::env::args().any(|argument| argument.contains("replace_undo_workflows")) {
        let (evidence_dir, evidence_replacements, source_lines) =
            make_replace_all_dense_10mb_fixture();
        let evidence_cancel = AtomicBool::new(false);
        let evidence = content_search::apply_replacements(
            &evidence_replacements,
            &HashSet::new(),
            &evidence_cancel,
            Some(evidence_dir.path()),
        )
        .expect("dense-line Replace All evidence should complete");
        let evidence_metrics = evidence.metrics();
        assert_eq!(
            evidence_metrics.source_lines,
            u64::try_from(source_lines).expect("dense fixture line count should fit u64")
        );
        assert_eq!(
            evidence_metrics.accepted_replacements,
            MAX_REPLACE_PREVIEW_ROWS
        );
        assert_eq!(
            evidence_metrics.retained_edit_records,
            MAX_REPLACE_PREVIEW_ROWS
        );
        assert!(evidence_metrics.retained_edit_records < source_lines);
        eprintln!(
            "replace-all-streaming-evidence source_lines={} accepted_replacements={} retained_edit_records={} retained_edit_bytes={} output_bytes={} undo_bytes={} replacement_cap={} file_byte_cap={}",
            evidence_metrics.source_lines,
            evidence_metrics.accepted_replacements,
            evidence_metrics.retained_edit_records,
            evidence_metrics.retained_edit_bytes,
            evidence_metrics.output_bytes,
            evidence_metrics.undo_bytes,
            MAX_REPLACE_PREVIEW_ROWS,
            content_search::MAX_REPLACE_FILE_BYTES,
        );
    }

    let mut group = c.benchmark_group("replace_undo_workflows");
    group.sample_size(10);

    for &(label, file_count, lines_per_file) in &[
        ("small", 10usize, 20usize),
        ("medium", 100, 20),
        ("many_files_journal", 1_000, 1),
    ] {
        group.bench_function(BenchmarkId::new("replace_all", label), |b| {
            b.iter_batched(
                || make_replace_all_fixture(file_count, lines_per_file),
                |(dir, replacements)| {
                    let cancel = AtomicBool::new(false);
                    let (result, backup) = content_search::apply_replacements(
                        black_box(&replacements),
                        black_box(&HashSet::new()),
                        black_box(&cancel),
                        Some(dir.path()),
                    )
                    .expect("expected operation to succeed")
                    .into_parts();
                    black_box((result.replaced_count, backup.len(), dir));
                },
                BatchSize::SmallInput,
            );
        });

        group.bench_function(BenchmarkId::new("undo_replace_all", label), |b| {
            b.iter_batched(
                || {
                    let (dir, replacements) = make_replace_all_fixture(file_count, lines_per_file);
                    let cancel = AtomicBool::new(false);
                    let (_, backup) = content_search::apply_replacements(
                        &replacements,
                        &HashSet::new(),
                        &cancel,
                        Some(dir.path()),
                    )
                    .expect("expected operation to succeed")
                    .into_parts();
                    (dir, backup)
                },
                |(dir, backup)| {
                    let outcome = content_search::undo_replacements(black_box(&backup));
                    black_box((outcome.restored_count(), outcome.remaining_count(), dir));
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.bench_function("replace_all/accepted_10mb_file", |b| {
        b.iter_batched(
            make_replace_all_10mb_fixture,
            |(dir, replacements)| {
                let cancel = AtomicBool::new(false);
                let (result, backup) = content_search::apply_replacements(
                    black_box(&replacements),
                    black_box(&HashSet::new()),
                    black_box(&cancel),
                    Some(dir.path()),
                )
                .expect("expected operation to succeed")
                .into_parts();
                black_box((result.replaced_count, backup.len(), dir));
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("replace_all/accepted_10mb_dense_short_lines", |b| {
        b.iter_batched(
            make_replace_all_dense_10mb_fixture,
            |(dir, replacements, source_lines)| {
                let cancel = AtomicBool::new(false);
                let outcome = content_search::apply_replacements(
                    black_box(&replacements),
                    black_box(&HashSet::new()),
                    black_box(&cancel),
                    Some(dir.path()),
                )
                .expect("expected operation to succeed");
                let metrics = outcome.metrics();
                black_box((
                    metrics.source_lines,
                    metrics.accepted_replacements,
                    metrics.retained_edit_records,
                    metrics.output_bytes,
                    metrics.undo_bytes,
                    source_lines,
                    dir,
                ));
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("replace_all/skipped_over_cap_file", |b| {
        b.iter_batched(
            make_replace_all_over_cap_fixture,
            |(dir, replacements)| {
                let cancel = AtomicBool::new(false);
                let (result, backup) = content_search::apply_replacements(
                    black_box(&replacements),
                    black_box(&HashSet::new()),
                    black_box(&cancel),
                    Some(dir.path()),
                )
                .expect("expected operation to succeed")
                .into_parts();
                black_box((result.skipped_count, backup.len(), dir));
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_replace_preview_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("replace_preview_generation");
    group.sample_size(10);

    for &match_count in &[1_000usize, 10_000usize] {
        group.bench_with_input(
            BenchmarkId::new("literal_matches", match_count),
            &match_count,
            |b, &match_count| {
                let matches = make_replace_preview_matches(match_count);
                let options = ContentSearchOptions::default();
                b.iter(|| {
                    let previews = generate_replacement_preview(
                        black_box(&matches),
                        black_box("needle"),
                        black_box("thread"),
                        black_box(&options),
                    );
                    black_box(previews.len());
                });
            },
        );
    }

    let matches = make_replace_preview_matches(10_000);
    let outcome = generate_replacement_preview(
        &matches,
        "needle",
        "thread",
        &ContentSearchOptions::default(),
    );
    group.bench_function("dense_identity_lookup_10k", |b| {
        b.iter(|| {
            for index in 0..10_000 {
                black_box(outcome.preview_index(SearchMatchId::from_index(index)));
            }
        });
    });
    group.bench_function("checked_identity_toggle_10k", |b| {
        b.iter_batched(
            || {
                (0..10_000)
                    .map(SearchMatchId::from_index)
                    .collect::<HashSet<_>>()
            },
            |mut checked| {
                for index in 0..10_000 {
                    let id = SearchMatchId::from_index(index);
                    checked.remove(&id);
                    checked.insert(id);
                }
                black_box(checked);
            },
            BatchSize::SmallInput,
        );
    });
    let checked = (0..10_000)
        .step_by(2)
        .map(SearchMatchId::from_index)
        .collect::<HashSet<_>>();
    let selected = outcome.clone().into_checked_replacements(&checked);
    assert_eq!(selected.len(), 5_000);
    eprintln!(
        "replace-preview-selection-bound-evidence preview_rows={} checked_identities={} selected_rows={} gtk_payload_clones=0",
        outcome.len(),
        checked.len(),
        selected.len(),
    );
    group.bench_function("checked_selection_10k_half", |b| {
        b.iter_batched(
            || (outcome.clone(), checked.clone()),
            |(outcome, checked)| black_box(outcome.into_checked_replacements(black_box(&checked))),
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

fn bench_tree_population(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_population");
    group.sample_size(20);
    // Match `scan_directory`'s default lookahead so the benchmark tracks the
    // real sidebar scan contract instead of silently skipping empty-dir probes.
    const BENCH_LOOKAHEAD_CAP: usize = 1000;

    for &(label, entry_count, max_entries) in &[
        ("full", 10_000usize, 10_000usize),
        ("truncated", 12_000usize, 10_000usize),
    ] {
        group.bench_function(BenchmarkId::new("scan_and_splice", label), |b| {
            b.iter_batched(
                || {
                    let dir = make_flat_dir(entry_count);
                    let cancel = AtomicBool::new(false);
                    let scan = file_tree::scan_directory_bounded(
                        dir.path(),
                        max_entries,
                        BENCH_LOOKAHEAD_CAP,
                        Some(&cancel),
                    );
                    (dir, scan)
                },
                |(dir, scan)| {
                    let store = populate_tree_store(scan.entries, scan.truncated);
                    (store, dir)
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_file_index_incremental(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_index_incremental");

    for size in [10_000, 100_000] {
        // add_file
        group.bench_function(BenchmarkId::new("add_file", size), |b| {
            b.iter_batched(
                || {
                    let index = make_synthetic_index(size);
                    let root = Arc::new(PathBuf::from("/synthetic/project"));
                    let new_file = IndexedFile {
                        path: PathBuf::from("/synthetic/project/src/new_file.rs"),
                        identity: PaletteFileIdentity::canonical(PathBuf::from(
                            "/synthetic/project/src/new_file.rs",
                        )),
                        name: "new_file.rs".to_string(),
                        workspace_folder: root,
                    };
                    (index, new_file)
                },
                |(mut index, file)| {
                    index.add_file(black_box(file));
                    index
                },
                BatchSize::SmallInput,
            );
        });

        // remove_path (file)
        group.bench_function(BenchmarkId::new("remove_path_file", size), |b| {
            b.iter_batched(
                || {
                    let index = make_synthetic_index(size);
                    let target = index.files()[size / 2].path.clone();
                    (index, target)
                },
                |(mut index, target)| {
                    index.remove_path(black_box(&target));
                    index
                },
                BatchSize::SmallInput,
            );
        });

        // remove_path (directory prefix)
        group.bench_function(BenchmarkId::new("remove_path_dir", size), |b| {
            b.iter_batched(
                || {
                    let index = make_synthetic_index(size);
                    let prefix = PathBuf::from("/synthetic/project/src/model");
                    (index, prefix)
                },
                |(mut index, prefix)| {
                    index.remove_path(black_box(&prefix));
                    index
                },
                BatchSize::SmallInput,
            );
        });

        // rename_path (file)
        group.bench_function(BenchmarkId::new("rename_path_file", size), |b| {
            b.iter_batched(
                || {
                    let index = make_synthetic_index(size);
                    let old = index.files()[size / 2].path.clone();
                    let new = old.with_file_name("renamed.rs");
                    (index, old, new)
                },
                |(mut index, old, new)| {
                    index.rename_path(black_box(&old), black_box(&new));
                    index
                },
                BatchSize::SmallInput,
            );
        });

        // rename_path (directory)
        group.bench_function(BenchmarkId::new("rename_path_dir", size), |b| {
            b.iter_batched(
                || {
                    let index = make_synthetic_index(size);
                    let old = PathBuf::from("/synthetic/project/src/model");
                    let new = PathBuf::from("/synthetic/project/src/domain");
                    (index, old, new)
                },
                |(mut index, old, new)| {
                    index.rename_path(black_box(&old), black_box(&new));
                    index
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_file_size_classify(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_size_classify");

    let sizes: &[(&str, u64)] = &[
        ("normal", 500_000),
        ("large_toast", 5_000_000),
        ("disable_syntax", 25_000_000),
        ("disable_undo", 100_000_000),
        ("too_large", 1_000_000_000),
    ];

    for &(label, size) in sizes {
        group.bench_with_input(BenchmarkId::new("size", label), &size, |b, &s| {
            b.iter(|| FileSizeCheck::classify(black_box(s)));
        });
    }
    group.finish();
}

/// Benchmark the scalar-only editor memory policy at deliberately large tab counts.
fn bench_editor_memory_policy(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor_memory_policy");
    // One percent makes the 10k-tab clean case traverse many candidates rather
    // than ending after only a handful of evictions.
    let per_page = EDITOR_MEMORY_UPPER_BUDGET_BYTES / 100;

    for tab_count in [1_000usize, 10_000, 100_000] {
        for &(label, eligible) in &[("clean", true), ("protected", false)] {
            let pages = (0..tab_count)
                .map(|editor_id| EditorResidency {
                    editor_id,
                    estimated_bytes: per_page,
                    access_generation: u64::try_from(editor_id).expect("benchmark generation"),
                    policy_generation: 1,
                    eligible_for_eviction: eligible,
                })
                .collect::<Vec<_>>();
            group.bench_function(BenchmarkId::new(format!("{tab_count}_tabs"), label), |b| {
                b.iter(|| evaluate_editor_memory_budget(black_box(&pages)));
            });
        }
    }

    let bookkeeping_heavy = (0..10_000usize)
        .map(|editor_id| EditorResidency {
            editor_id,
            estimated_bytes: lushtext_core::model::editor_memory::EVICTED_EDITOR_BOOKKEEPING_BYTES
                + 1,
            access_generation: u64::try_from(editor_id).expect("benchmark generation"),
            policy_generation: 1,
            eligible_for_eviction: true,
        })
        .chain(std::iter::once(EditorResidency {
            editor_id: 10_000,
            estimated_bytes: EDITOR_MEMORY_UPPER_BUDGET_BYTES,
            access_generation: 10_000,
            policy_generation: 1,
            eligible_for_eviction: false,
        }))
        .collect::<Vec<_>>();
    group.bench_function("10k_tabs/bookkeeping_heavy", |b| {
        b.iter(|| evaluate_editor_memory_budget(black_box(&bookkeeping_heavy)));
    });

    let incremental_pages = (0..100_000usize)
        .map(|editor_id| EditorResidency {
            editor_id,
            estimated_bytes: 1024,
            access_generation: u64::try_from(editor_id).expect("benchmark generation"),
            policy_generation: 1,
            eligible_for_eviction: false,
        })
        .collect::<Vec<_>>();
    let mut incremental = EditorResidencyLedger::default();
    incremental.reconcile(incremental_pages.iter().copied());
    let edited_generation = Cell::new(1u64);
    group.bench_function("100k_tabs/incremental_edit", |b| {
        b.iter(|| {
            let generation = edited_generation.get().wrapping_add(1);
            edited_generation.set(generation);
            black_box(incremental.upsert(EditorResidency {
                editor_id: 50_000,
                estimated_bytes: 1025,
                access_generation: 50_000,
                policy_generation: generation,
                eligible_for_eviction: false,
            }))
        });
    });
    eprintln!(
        "editor-memory-incremental-evidence records={} records_touched_per_edit=1 full_scans_below_threshold=0 aggregate_bytes={}",
        incremental.len(),
        incremental.total_bytes(),
    );

    for &(label, characters, file_bytes) in &[
        ("ascii_growth", 10_000_000, Some(10_000_000)),
        ("unicode_growth", 10_000_000, Some(30_000_000)),
    ] {
        group.bench_function(BenchmarkId::new("live_estimate", label), |b| {
            b.iter(|| {
                estimate_live_editor_bytes(
                    black_box(characters),
                    black_box(file_bytes),
                    black_box(false),
                )
            });
        });
    }
    group.finish();
}

/// Benchmark startup draft preload and conservative orphan cleanup.
///
/// Batched fixtures exclude setup from cleanup timing, while the scale group
/// isolates bounded inspection and linear exact-fingerprint merging.
fn bench_draft_restore(c: &mut Criterion) {
    {
        let mut group = c.benchmark_group("draft_restore");
        group.sample_size(30);

        // Benchmark the full startup preload pipeline:
        // load manifest + load session + resolve draft restore state.
        // This mirrors the background work in load_session_and_drafts.
        for &(label, n_tabs, n_drafts, draft_kb) in &[
            ("5_tabs_1_draft_1kb", 5, 1, 1),
            ("10_tabs_5_drafts_1kb", 10, 5, 1),
            ("20_tabs_10_drafts_10kb", 20, 10, 10),
            ("50_tabs_20_drafts_10kb", 50, 20, 10),
        ] {
            group.bench_function(BenchmarkId::new("startup_preload", label), |b| {
                b.iter_batched(
                    || make_draft_fixtures(n_tabs, n_drafts, draft_kb * 1024),
                    |(dir, _session)| {
                        let restore = draft_service::load_restore_state(black_box(dir.path()));
                        (
                            restore.manifest,
                            restore.session,
                            restore.preloaded_drafts,
                            dir,
                        )
                    },
                    BatchSize::SmallInput,
                );
            });
        }

        for &(label, n_valid, n_orphan_entries, n_orphan_files) in &[
            ("clean_5", 5, 0, 0),
            ("5_orphan_entries", 5, 5, 0),
            ("5_orphan_files", 5, 0, 5),
            ("mixed_20", 10, 5, 5),
            ("mixed_1000", 500, 250, 250),
            ("mixed_at_cap", 1535, 512, 512),
        ] {
            group.bench_function(BenchmarkId::new("orphan_cleanup", label), |b| {
                b.iter_batched(
                    || {
                        let dir = TempDir::new().expect("expected operation to succeed");
                        let mut manifest = DraftManifest::default();

                        for i in 0..n_valid {
                            let id = format!("valid-{i}");
                            draft_service::write_draft(dir.path(), &id, "content")
                                .expect("expected operation to succeed");
                            manifest.upsert(DraftEntry {
                                draft_id: id,
                                original_path: None,
                                original_mtime_secs: None,
                                saved_at_secs: 1000,
                            });
                        }
                        for i in 0..n_orphan_entries {
                            manifest.upsert(DraftEntry {
                                draft_id: format!("orphan-entry-{i}"),
                                original_path: None,
                                original_mtime_secs: None,
                                saved_at_secs: 1000,
                            });
                        }
                        fixture::create_dir_all(&draft_service::drafts_dir(dir.path()));
                        for i in 0..n_orphan_files {
                            draft_service::write_draft(
                                dir.path(),
                                &format!("orphan-file-{i}"),
                                "stale",
                            )
                            .expect("expected operation to succeed");
                        }
                        draft_service::save_manifest(dir.path(), &manifest)
                            .expect("expected operation to succeed");

                        (dir, manifest)
                    },
                    |(dir, manifest)| {
                        let plan = draft_service::inspect_orphan_cleanup(
                            black_box(dir.path()),
                            black_box(&manifest),
                        )
                        .expect("expected operation to succeed");
                        let outcome = draft_service::execute_orphan_cleanup(
                            black_box(dir.path()),
                            black_box(plan),
                        );
                        black_box((dir, manifest, outcome))
                    },
                    BatchSize::SmallInput,
                );
            });
        }

        group.finish();
    }

    {
        let mut group = c.benchmark_group("draft_restore_policy_sizes");
        group.sample_size(10);
        for &(label, size) in &[
            ("1_mib", 1024 * 1024),
            ("10_mib", 10 * 1024 * 1024),
            ("50_mib", 50 * 1024 * 1024),
            ("exact_64_mib", draft_service::MAX_AUTOMATIC_DRAFT_BYTES),
        ] {
            let dir = make_policy_sized_draft_fixtures(&[size]);
            group.throughput(criterion::Throughput::Bytes(size));
            group.bench_function(BenchmarkId::new("bounded_read", label), |b| {
                b.iter(|| {
                    draft_service::read_draft(black_box(dir.path()), "policy-sized-0")
                        .expect("read policy-sized draft")
                });
            });
        }

        let over_half = draft_service::MAX_EAGER_DRAFT_PRELOAD_BYTES / 2 + 1;
        // Each body fits alone, but the pair exceeds the aggregate budget by
        // two bytes and forces the second body through lazy admission.
        let dir = make_policy_sized_draft_fixtures(&[over_half, over_half]);
        group.throughput(criterion::Throughput::Bytes(over_half));
        group.bench_function("aggregate_cap_lazy_transition", |b| {
            b.iter(|| draft_service::load_restore_state(black_box(dir.path())));
        });
        group.finish();
    }

    let mut scale = c.benchmark_group("draft_cleanup_scale");
    // Large-manifest fixture cloning is expensive, so ten samples bound suite
    // time while still exposing order-of-growth regressions.
    scale.sample_size(10);
    for manifest_size in [2_048usize, 10_000, 100_000] {
        scale.bench_with_input(
            BenchmarkId::new("inspect_manifest_page", manifest_size),
            &manifest_size,
            |b, &size| {
                let dir = TempDir::new().expect("expected operation to succeed");
                let manifest = DraftManifest {
                    drafts: (0..size)
                        .map(|index| DraftEntry {
                            draft_id: format!("manifest-{index}"),
                            original_path: None,
                            original_mtime_secs: None,
                            saved_at_secs: 1000,
                        })
                        .collect(),
                    cleanup_continuation: None,
                };
                b.iter(|| {
                    draft_service::inspect_orphan_cleanup(
                        black_box(dir.path()),
                        black_box(&manifest),
                    )
                });
            },
        );

        // Cover no-op, single-removal, and one full cleanup-page merge costs.
        for committed_count in [0usize, 1, manifest_size.min(2_048)] {
            scale.bench_function(
                BenchmarkId::new(
                    "merge_committed_fingerprints",
                    format!("{manifest_size}_entries_{committed_count}_committed"),
                ),
                |b| {
                    let entries = (0..manifest_size)
                        .map(|index| DraftEntry {
                            draft_id: format!("merge-{index}"),
                            original_path: None,
                            original_mtime_secs: None,
                            saved_at_secs: 1000,
                        })
                        .collect::<Vec<_>>();
                    let committed = entries
                        .iter()
                        .take(committed_count)
                        .map(|entry| {
                            (
                                entry.draft_id.clone(),
                                draft_service::DraftEntryFingerprint::from_entry(entry),
                            )
                        })
                        .collect();
                    b.iter_batched(
                        || DraftManifest {
                            drafts: entries.clone(),
                            cleanup_continuation: None,
                        },
                        |mut manifest| {
                            draft_service::merge_committed_orphan_removals(
                                black_box(&mut manifest),
                                black_box(&committed),
                            );
                            black_box(manifest)
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }
    scale.finish();
}

fn bench_recovery_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("recovery_performance");
    group.sample_size(10);

    group.bench_function("malformed_metadata/startup_and_sidecar_diagnostics", |b| {
        b.iter_batched(
            || make_malformed_recovery_fixture(12),
            |(dir, workspace)| {
                let restore = draft_service::load_restore_state(black_box(dir.path()));
                let ledger = migration_ledger::load_recovering(black_box(dir.path()));
                let scope_snapshot = WorkspacesFile {
                    current_scope: WorkspaceScope::All,
                    workspaces: vec![WorkspaceConfig {
                        id: WorkspaceId::new("recovery-benchmark"),
                        name: "Recovery Benchmark".to_string(),
                        folders: vec![WorkspaceFolder::new(workspace)],
                    }],
                }
                .current_scope_snapshot();
                let bookmarks = palette::load_note_entries_bounded_for_scope(
                    black_box(dir.path()),
                    black_box(&scope_snapshot),
                    &[],
                    false,
                    palette::NotesBrowserMode::Bookmarks,
                    palette::PALETTE_NOTE_SOURCE_LIMITS,
                    &palette::PaletteSearchCancellation::default(),
                )
                .expect("expected operation to succeed");
                let palette::PaletteNoteSourceOutcome::Complete { load, .. } = bookmarks else {
                    panic!("fresh recovery benchmark cannot cancel");
                };
                black_box((
                    restore.diagnostics.len(),
                    ledger.diagnostics.len(),
                    load.diagnostics.len(),
                    dir,
                ));
            },
            BatchSize::SmallInput,
        );
    });

    for entry_count in [10usize, 100usize] {
        group.bench_function(
            BenchmarkId::new("pending_migrations/reconcile", entry_count),
            |b| {
                b.iter_batched(
                    || make_pending_migration_fixture(entry_count),
                    |dir| {
                        let report = migration_ledger::reconcile_pending(black_box(dir.path()))
                            .expect("expected operation to succeed");
                        black_box((report.considered, report.attempted, report.completed, dir));
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.bench_function("duplicate_sidecars/bookmark_merge", |b| {
        b.iter_batched(
            make_duplicate_bookmark_sidecar_fixture,
            |(dir, old_path, new_path)| {
                let migrated = bookmark_service::move_path_tree(
                    black_box(dir.path()),
                    black_box(&old_path),
                    black_box(&new_path),
                )
                .expect("expected operation to succeed");
                black_box((migrated, dir));
            },
            BatchSize::SmallInput,
        );
    });

    for lineage_count in [24usize, 120usize] {
        group.bench_function(
            BenchmarkId::new("local_history_many_lineages/move_tree", lineage_count),
            |b| {
                b.iter_batched(
                    || make_many_local_history_lineages_fixture(lineage_count),
                    |(dir, old_root, new_root)| {
                        let migrated = local_history_service::move_path_tree(
                            black_box(dir.path()),
                            black_box(&old_root),
                            black_box(&new_root),
                        )
                        .expect("expected operation to succeed");
                        black_box((migrated, dir));
                    },
                    BatchSize::SmallInput,
                );
            },
        );
        group.bench_function(
            BenchmarkId::new(
                "local_history_many_lineages/reconcile_bounded",
                lineage_count,
            ),
            |b| {
                b.iter_batched(
                    || make_mismatched_local_history_lineages_fixture(lineage_count),
                    |dir| {
                        let budget = local_history_service::LocalHistoryReconcileBudget::new(
                            lineage_count / 2,
                            Duration::from_secs(60),
                        );
                        let report = local_history_service::reconcile_lineages_with_budget(
                            black_box(dir.path()),
                            black_box(budget),
                        )
                        .expect("expected operation to succeed");
                        black_box((
                            report.scanned_lineages,
                            report.reconciled_lineages,
                            report.deferred_lineages,
                            dir,
                        ));
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.bench_function("first_dirty_autosave/persist_manifest_batch", |b| {
        b.iter_batched(
            || make_first_dirty_autosave_fixture(20, 4 * 1024),
            |(dir, ids)| {
                let content = "x".repeat(4 * 1024);
                let mut manifest = DraftManifest::default();
                for draft_id in ids {
                    draft_service::write_draft(black_box(dir.path()), &draft_id, &content)
                        .expect("expected operation to succeed");
                    manifest.upsert(DraftEntry {
                        draft_id,
                        original_path: None,
                        original_mtime_secs: None,
                        saved_at_secs: 2000,
                    });
                }
                draft_service::save_manifest(black_box(dir.path()), black_box(&manifest))
                    .expect("expected operation to succeed");
                black_box((manifest.drafts.len(), dir));
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_content_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_search");
    group.sample_size(20);

    // --- Fixture: 10k files for literal & regex search ---
    let search_dir = TempDir::new().expect("expected operation to succeed");
    let search_root = search_dir.path();
    for i in 0..10_000 {
        let subdir = format!("dir_{}", i / 500);
        let dir_path = search_root.join(&subdir);
        fixture::create_dir_all(&dir_path);
        // Every 5th file contains the search target.
        let content = if i % 5 == 0 {
            format!("fn handler_{i}() {{ TODO: implement }}\nlet x = {i};\n")
        } else {
            format!("let value_{i} = {i};\nlet other = true;\n")
        };
        fixture::write_text(&dir_path.join(format!("file_{i}.rs")), &content);
    }

    let overlap_child = search_root.join("dir_0");
    let traversal_plan = WorkspaceSearchTraversalPlan::build(
        [overlap_child.clone(), search_root.to_path_buf()],
        fs_metadata::canonical_path,
    );
    let single_no_match = run_content_search_benchmark(
        "definitely-absent-search-needle",
        &[search_root],
        &ContentSearchOptions::default(),
    );
    let overlap_no_match = run_content_search_benchmark(
        "definitely-absent-search-needle",
        &[overlap_child.as_path(), search_root],
        &ContentSearchOptions::default(),
    );
    let missing_root = search_root.join("missing-unresolved-root");
    let fallback_plan = WorkspaceSearchTraversalPlan::build(
        [missing_root.clone(), search_root.to_path_buf()],
        fs_metadata::canonical_path,
    );
    let fallback_no_match = run_content_search_benchmark(
        "definitely-absent-search-needle",
        &[missing_root.as_path(), search_root],
        &ContentSearchOptions::default(),
    );
    let single_matches =
        run_content_search_benchmark("TODO", &[search_root], &ContentSearchOptions::default());
    let overlap_matches = run_content_search_benchmark(
        "TODO",
        &[overlap_child.as_path(), search_root],
        &ContentSearchOptions::default(),
    );
    let cancelled = run_content_search_benchmark_with_cancel(
        "definitely-absent-search-needle",
        &[search_root],
        &ContentSearchOptions::default(),
        true,
    );
    assert_eq!(single_no_match.matches, 0);
    assert_eq!(overlap_no_match.matches, single_no_match.matches);
    assert_eq!(single_no_match.incomplete, 0);
    assert_eq!(overlap_no_match.incomplete, 0);
    assert!(fallback_plan.fallback_identity_required());
    assert_eq!(fallback_no_match.incomplete, 0);
    assert_eq!(fallback_no_match.fallback_metrics.entries, 10_000);
    assert!(fallback_no_match.fallback_metrics.path_bytes > 0);
    assert_eq!(traversal_plan.traversal_roots().len(), 2);
    assert_eq!(
        traversal_plan.traversal_roots()[1].excluded_paths(),
        std::slice::from_ref(&overlap_child)
    );
    let mut single_identities = single_matches.match_identities.clone();
    let mut overlap_identities = overlap_matches.match_identities.clone();
    single_identities.sort();
    overlap_identities.sort();
    let semantic_equivalent = single_identities == overlap_identities;
    assert!(semantic_equivalent);
    assert!(
        overlap_matches
            .match_identities
            .first()
            .is_some_and(|(path, _)| path.starts_with(&overlap_child))
    );
    println!(
        "workspace-search-traversal-evidence files=10000 configured_roots=2 traversal_roots={} excluded_roots={} display_roots={} fallback_required={} fallback_fixture_required={} fallback_entries_high_water={} fallback_path_bytes_high_water={} plan_retained_bytes={} fallback_plan_retained_bytes={} single_matches={} overlap_matches={} semantic_equivalent={} child_partition_first={} cancellation_events={}",
        traversal_plan.traversal_roots().len(),
        traversal_plan.traversal_roots()[1].excluded_paths().len(),
        traversal_plan.display_roots().len(),
        traversal_plan.fallback_identity_required(),
        fallback_plan.fallback_identity_required(),
        fallback_no_match.fallback_metrics.entries,
        fallback_no_match.fallback_metrics.path_bytes,
        workspace_search_plan_retained_bytes(&traversal_plan),
        workspace_search_plan_retained_bytes(&fallback_plan),
        single_matches.matches,
        overlap_matches.matches,
        semantic_equivalent,
        overlap_matches
            .match_identities
            .first()
            .is_some_and(|(path, _)| path.starts_with(&overlap_child)),
        cancelled.events,
    );

    group.bench_function("no_match_10k_single_root", |b| {
        b.iter(|| {
            black_box(run_content_search_benchmark(
                "definitely-absent-search-needle",
                &[search_root],
                &ContentSearchOptions::default(),
            ));
        });
    });
    group.bench_function("no_match_10k_overlapping_roots", |b| {
        b.iter(|| {
            black_box(run_content_search_benchmark(
                "definitely-absent-search-needle",
                &[overlap_child.as_path(), search_root],
                &ContentSearchOptions::default(),
            ));
        });
    });

    // 1. Literal search across 10k files.
    group.bench_function("literal_10k_files", |b| {
        b.iter(|| {
            black_box(run_content_search_benchmark(
                "TODO",
                &[search_root],
                &ContentSearchOptions::default(),
            ));
        });
    });

    // 2. Regex search across 10k files.
    group.bench_function("regex_10k_files", |b| {
        let opts = ContentSearchOptions {
            regex: true,
            ..Default::default()
        };
        b.iter(|| {
            black_box(run_content_search_benchmark(
                r"fn\s+\w+",
                &[search_root],
                &opts,
            ));
        });
    });

    // --- Fixture: single large file (100k lines) ---
    let large_dir = TempDir::new().expect("expected operation to succeed");
    let large_root = large_dir.path();
    {
        // 100k lines, needle every 1000 lines.
        let mut content = String::with_capacity(100_000 * 30);
        for i in 0..100_000 {
            if i % 1000 == 0 {
                content.push_str(&format!("line {i}: TODO fix this\n"));
            } else {
                content.push_str(&format!("line {i}: normal content here\n"));
            }
        }
        fixture::write_text(&large_root.join("huge.txt"), &content);
    }

    // 3. Large file search (100k lines).
    group.bench_function("large_file_100k_lines", |b| {
        b.iter(|| {
            black_box(run_content_search_benchmark(
                "TODO",
                &[large_root],
                &ContentSearchOptions::default(),
            ));
        });
    });

    // --- Fixture: gitignore filtering (10k files, half in ignored dirs) ---
    let gitignore_dir = TempDir::new().expect("expected operation to succeed");
    let gitignore_root = gitignore_dir.path();
    {
        fixture::create_dir(&gitignore_root.join(".git"));
        fixture::write_text(&gitignore_root.join(".gitignore"), "ignored_*/\n");
        for i in 0..20 {
            let name = if i < 10 {
                format!("ignored_{i}")
            } else {
                format!("visible_{i}")
            };
            let dir_path = gitignore_root.join(&name);
            fixture::create_dir_all(&dir_path);
            for j in 0..500 {
                fixture::write_text(&dir_path.join(format!("file_{j}.rs")), "fn needle() {}\n");
            }
        }
    }

    // 4. Gitignore-filtered search.
    group.bench_function("gitignore_10k_files", |b| {
        b.iter(|| {
            black_box(run_content_search_benchmark(
                "needle",
                &[gitignore_root],
                &ContentSearchOptions::default(),
            ));
        });
    });

    group.finish();
}

fn bench_content_search_smoke(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_search_smoke");

    let search_dir = TempDir::new().expect("expected operation to succeed");
    let search_root = search_dir.path();
    for i in 0..200 {
        let dir_path = search_root.join(format!("dir_{}", i / 50));
        fixture::create_dir_all(&dir_path);
        let content = if i % 5 == 0 {
            format!("fn handler_{i}() {{ TODO: implement }}\nlet x = {i};\n")
        } else {
            format!("let value_{i} = {i};\nlet other = true;\n")
        };
        fixture::write_text(&dir_path.join(format!("file_{i}.rs")), &content);
    }

    group.bench_function("literal_200_files", |b| {
        b.iter(|| {
            black_box(run_content_search_benchmark(
                "TODO",
                &[search_root],
                &ContentSearchOptions::default(),
            ));
        });
    });

    let large_dir = TempDir::new().expect("expected operation to succeed");
    let large_root = large_dir.path();
    {
        let mut content = String::with_capacity(10_000 * 30);
        for i in 0..10_000 {
            if i % 500 == 0 {
                content.push_str(&format!("line {i}: TODO fix this\n"));
            } else {
                content.push_str(&format!("line {i}: normal content here\n"));
            }
        }
        fixture::write_text(&large_root.join("medium.txt"), &content);
    }

    group.bench_function("medium_file_10k_lines", |b| {
        b.iter(|| {
            black_box(run_content_search_benchmark(
                "TODO",
                &[large_root],
                &ContentSearchOptions::default(),
            ));
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Transient file-load admission and bounded installation planning
// ---------------------------------------------------------------------------

fn run_transient_load_policy(weights: &[u64], cancel_even: bool) -> (u64, usize) {
    let mut policy = FileLoadAdmissionPolicy::default();
    for (index, weight) in weights.iter().copied().enumerate() {
        let request_id = u64::try_from(index + 1).expect("benchmark request id");
        policy.queue(FileLoadAdmissionRequest {
            request_id,
            owner_id: request_id,
            sequence: request_id,
            weight,
            priority: if index % 7 == 0 {
                FileLoadPriority::Active
            } else {
                FileLoadPriority::Normal
            },
        });
        if cancel_even && index % 2 == 0 {
            assert!(policy.cancel_queued(request_id));
        }
    }

    let mut active = VecDeque::new();
    loop {
        if let Some(grant) = policy.admit_next(false) {
            active.push_back(grant.request_id);
            continue;
        }
        if let Some(request_id) = active.pop_front() {
            assert!(policy.release(request_id));
            continue;
        }
        break;
    }
    let snapshot = policy.snapshot();
    (snapshot.high_water_weight, snapshot.queued_count)
}

fn bench_transient_file_load(c: &mut Criterion) {
    let evidence_weights = [transient_load_weight(8 * 1024 * 1024); 8];
    let mut evidence_policy = FileLoadAdmissionPolicy::default();
    for (index, weight) in evidence_weights.iter().copied().enumerate() {
        let request_id = u64::try_from(index + 1).expect("evidence request id");
        evidence_policy.queue(FileLoadAdmissionRequest {
            request_id,
            owner_id: request_id,
            sequence: request_id,
            weight,
            priority: FileLoadPriority::Normal,
        });
    }
    while evidence_policy.admit_next(false).is_some() {}
    let evidence = evidence_policy.snapshot();
    eprintln!(
        "transient-load-policy-evidence active_payload_weight={} queued_scalar_count={} high_water_weight={} shared_budget={}",
        evidence.active_weight,
        evidence.queued_count,
        evidence.high_water_weight,
        TRANSIENT_LOAD_SHARED_BUDGET_BYTES
    );

    let mut group = c.benchmark_group("transient_file_load");
    let many_small = vec![transient_load_weight(64 * 1024); 512];
    group.bench_function("admission/many_small_512", |b| {
        b.iter(|| black_box(run_transient_load_policy(&many_small, false)));
    });

    let concurrent_large = vec![transient_load_weight(8 * 1024 * 1024); 8];
    group.bench_function("admission/concurrent_large_8", |b| {
        b.iter(|| black_box(run_transient_load_policy(&concurrent_large, false)));
    });

    let exclusive_near_limit = [transient_load_weight(
        lushtext_core::services::file_limits::REFUSE_TO_OPEN - 1,
    )];
    group.bench_function("admission/exclusive_near_supported_limit", |b| {
        b.iter(|| black_box(run_transient_load_policy(&exclusive_near_limit, false)));
    });

    let stale_queue = vec![transient_load_weight(256 * 1024); 1024];
    group.bench_function("admission/stale_queued_1024", |b| {
        b.iter(|| black_box(run_transient_load_policy(&stale_queue, true)));
    });

    let pattern = "🙂é\n";
    let unicode_text = pattern.repeat((50 * 1024 * 1024) / pattern.len());
    group.bench_function("install_boundaries/unicode_50_mib", |b| {
        b.iter(|| {
            let mut start = 0;
            let mut slices = 0u64;
            while start < unicode_text.len() {
                start = next_install_boundary(black_box(&unicode_text), start);
                slices = slices.saturating_add(1);
            }
            black_box(slices)
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Bounded workspace watcher normalization and mailbox pressure
// ---------------------------------------------------------------------------

fn awkward_watch_paths(count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|index| {
            PathBuf::from(format!(
                "/tmp/workspace-🙂/deep/one/two/three/four/five/six/seven/é-{index}.rs"
            ))
        })
        .collect()
}

fn raw_watch_events(paths: &[PathBuf]) -> Vec<Event> {
    let mut events = Vec::with_capacity(paths.len() * 3);
    for path in paths {
        let created = Event::new(EventKind::Create(CreateKind::File)).add_path(path.clone());
        events.push(created.clone());
        events.push(created);
        events.push(
            Event::new(EventKind::Access(AccessKind::Open(AccessMode::Read)))
                .add_path(path.clone()),
        );
    }
    events
}

fn run_watch_pressure(batches: &[Vec<PathBuf>], poll_every: usize) -> (usize, usize, usize) {
    let mailbox = WorkspaceWatchMailbox::new();
    let mut promotions = 0usize;
    let mut notices = 0usize;
    let mut max_retained = 0usize;
    let mut full_refresh_pending = false;
    for (index, batch) in batches.iter().enumerate() {
        mailbox.merge_paths(batch.iter().cloned());
        let snapshot = mailbox.snapshot();
        if snapshot.full_refresh && !full_refresh_pending {
            promotions += 1;
        }
        full_refresh_pending = snapshot.full_refresh;
        max_retained = max_retained.max(snapshot.retained_paths);
        if (index + 1) % poll_every == 0 && mailbox.take_notice().is_some() {
            notices += 1;
            full_refresh_pending = false;
        }
    }
    notices += usize::from(mailbox.take_notice().is_some());
    (promotions, notices, max_retained)
}

fn bench_workspace_watch_pressure(c: &mut Criterion) {
    let evidence = WorkspaceWatchMailbox::new();
    evidence.merge_paths(awkward_watch_paths(WORKSPACE_WATCH_PATH_CAP + 1));
    let snapshot = evidence.snapshot();
    eprintln!(
        "workspace-watch-pressure-evidence path_cap={} retained_paths={} full_refresh={} notices_per_poll_max=1",
        WORKSPACE_WATCH_PATH_CAP, snapshot.retained_paths, snapshot.full_refresh
    );

    let mut group = c.benchmark_group("workspace_watch_pressure");
    let awkward = raw_watch_events(&awkward_watch_paths(WORKSPACE_WATCH_PATH_CAP / 2));
    group.bench_function("normalize_merge/duplicate_unicode_deep_512", |b| {
        b.iter_batched(
            || awkward.clone(),
            |events: Vec<Event>| {
                let mailbox = WorkspaceWatchMailbox::new();
                for event in events {
                    mailbox.merge_backend_result_for_benchmark(Ok(event));
                }
                black_box(mailbox.snapshot())
            },
            BatchSize::SmallInput,
        );
    });

    let raw_overflow = raw_watch_events(&awkward_watch_paths(WORKSPACE_WATCH_PATH_CAP + 1));
    group.bench_function("normalize_merge/raw_cap_plus_one_promotes", |b| {
        b.iter_batched(
            || raw_overflow.clone(),
            |events: Vec<Event>| {
                let mailbox = WorkspaceWatchMailbox::new();
                for event in events {
                    mailbox.merge_backend_result_for_benchmark(Ok(event));
                }
                black_box(mailbox.snapshot())
            },
            BatchSize::SmallInput,
        );
    });

    let batches = (0..32)
        .map(|batch| {
            (0..64)
                .map(|index| PathBuf::from(format!("/tmp/batch-{batch}/path-{index}")))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for (name, poll_every) in [
        ("consumer_faster/poll_each_batch", 1usize),
        ("producer_equal/poll_every_four_batches", 4usize),
        ("producer_faster/no_poll_until_end", usize::MAX),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(run_watch_pressure(black_box(&batches), poll_every)));
        });
    }

    let overflow = awkward_watch_paths(WORKSPACE_WATCH_PATH_CAP + 1);
    group.bench_function("promotion/unique_paths_cap_plus_one", |b| {
        b.iter(|| {
            let mailbox = WorkspaceWatchMailbox::new();
            mailbox.merge_paths(overflow.iter().cloned());
            black_box(mailbox.snapshot())
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Remaining quality-gap scale evidence
// ---------------------------------------------------------------------------

fn synthetic_notes_browser_entries(count: usize) -> Vec<PaletteNoteEntry> {
    (0..count)
        .map(|index| PaletteNoteEntry {
            category: PaletteNoteCategory::DocumentNotes,
            title: format!("Document Note {index:05}"),
            subtitle: format!("Workspace / synthetic-{index:05}.md"),
            detail: Some("calibration row".to_string()),
            note_text: Some(format!(
                "bounded searchable body {index:05} {}",
                "x".repeat(192)
            )),
            target: PaletteNoteTarget::DocumentNote {
                path: PathBuf::from(format!("/benchmark/synthetic-{index:05}.md")),
                workspace_folders: vec![PathBuf::from("/benchmark")],
            },
        })
        .collect()
}

fn bench_quality_gap_scale(c: &mut Criterion) {
    const NOTE_ROWS: usize = 10_000;
    const NOTE_RENDER_CAP: usize = 500;
    const PREVIEW_BYTES: usize = 4 * 1024 * 1024;
    const CACHE_ROWS: usize = 10_000;
    const MINIMAP_ANALYSIS_SLICE_CHARS: usize = 32 * 1024;
    const MINIMAP_SHORT_LINE_CHARS: usize = 96;
    const MINIMAP_SHORT_LINES: usize = 23_000;

    fn workspace_scan_pressure(
        requests: usize,
    ) -> lushtext_core::model::workspace_scan::WorkspaceScanFlightMetrics {
        let mut flight = WorkspaceScanFlight::default();
        for _ in 0..requests {
            let _ = flight.submit(1);
        }
        let first = flight
            .active()
            .expect("pressure fixture should own an active scan");
        let _ = flight.finish(first);
        if let Some(latest) = flight.active() {
            let _ = flight.finish(latest);
        }
        flight.metrics()
    }

    fn minimap_analysis(text: &str) -> (MinimapAnalysisResult, usize, usize) {
        let policy = MinimapAnalysisPolicy {
            warning_line_chars: 120,
            wrapped_line_chars: 8_000,
            marker_limit: 2_000,
        };
        let mut analysis = MinimapAnalysisAccumulator::new(policy, true);
        let mut characters = text.chars();
        let mut slices = 0usize;
        let mut slice_high_water = 0usize;
        loop {
            let inspected =
                analysis.inspect_slice(characters.by_ref(), MINIMAP_ANALYSIS_SLICE_CHARS);
            if inspected == 0 {
                break;
            }
            slices = slices.saturating_add(1);
            slice_high_water = slice_high_water.max(inspected);
        }
        (analysis.finish(), slices, slice_high_water)
    }

    let note_entries = synthetic_notes_browser_entries(NOTE_ROWS);
    let note_bytes = note_entries
        .iter()
        .map(|entry| {
            entry.title.len()
                + entry.subtitle.len()
                + entry.detail.as_ref().map_or(0, String::len)
                + entry.note_text.as_ref().map_or(0, String::len)
        })
        .sum::<usize>();
    let note_request = palette::NotesBrowserQueryRequest {
        query: "needle that is absent".to_string(),
        mode: palette::NotesBrowserMode::AllNotes,
    };
    let note_outcome = palette::query_notes_browser_source(
        &note_entries,
        &note_request,
        NOTE_RENDER_CAP,
        &palette::PaletteSearchCancellation::default(),
    );
    let note_metrics = note_outcome.metrics();
    let large_scoring_body = format!("{} trailing calibration needle", "x".repeat(4 * 1024 + 128));
    let note_scoring_entries = (0..NOTE_ROWS)
        .map(|index| PaletteNoteEntry {
            category: PaletteNoteCategory::DocumentNotes,
            title: format!("Calibration needle {index:05}"),
            subtitle: format!("Workspace / scoring-{index:05}.md"),
            detail: None,
            note_text: Some(large_scoring_body.clone()),
            target: PaletteNoteTarget::DocumentNote {
                path: PathBuf::from(format!("/benchmark/scoring-{index:05}.md")),
                workspace_folders: vec![PathBuf::from("/benchmark")],
            },
        })
        .collect::<Vec<_>>();
    let empty_index = FileIndex::default();
    let run_note_scoring = |query: &str| {
        palette::grouped_search(
            palette::GroupedSearchInput {
                index: &empty_index,
                open_tabs: &[],
                note_entries: &note_scoring_entries,
                workspace_group_label: "All Workspaces",
                query,
                mode: SearchMode::Notes,
                max_per_source: NOTE_RENDER_CAP,
            },
            &palette::PaletteSearchCancellation::default(),
        )
    };
    let direct_note_scoring = run_note_scoring("calibration needle");
    let direct_note_scoring_metrics = direct_note_scoring.metrics();
    let palette::PaletteSearchOutcome::Complete {
        value: direct_note_rows,
        ..
    } = direct_note_scoring
    else {
        panic!("fresh scoring benchmark must complete");
    };
    let retained_note_rows = direct_note_rows
        .iter()
        .filter(|row| matches!(row, PaletteSearchRow::Note { .. }))
        .count();
    assert_eq!(direct_note_scoring_metrics.candidates_scored, NOTE_ROWS);
    assert_eq!(direct_note_scoring_metrics.note_bodies_examined, 0);
    assert_eq!(
        direct_note_scoring_metrics.note_bodies_safely_pruned,
        NOTE_ROWS
    );
    assert_eq!(retained_note_rows, NOTE_RENDER_CAP);

    let mut scoring_coordinator = palette::NotesBrowserQueryCoordinator::default();
    let first_scoring = scoring_coordinator
        .submit(palette::NotesBrowserQueryRequest {
            query: "obsolete-0".to_string(),
            mode: palette::NotesBrowserMode::AllNotes,
        })
        .expect("first scoring request starts");
    for index in 1..32 {
        let _ = scoring_coordinator.submit(palette::NotesBrowserQueryRequest {
            query: format!("obsolete-{index}"),
            mode: palette::NotesBrowserMode::AllNotes,
        });
    }
    let _ = scoring_coordinator.submit(palette::NotesBrowserQueryRequest {
        query: "calibration needle".to_string(),
        mode: palette::NotesBrowserMode::AllNotes,
    });
    let scoring_ownership = scoring_coordinator.snapshot();
    let latest_scoring = scoring_coordinator
        .finish(first_scoring.generation)
        .expect("latest scoring request starts");
    let latest_note_scoring = run_note_scoring(&latest_scoring.request.query);
    let palette::PaletteSearchOutcome::Complete {
        value: latest_note_rows,
        ..
    } = latest_note_scoring
    else {
        panic!("fresh latest scoring benchmark must complete");
    };
    let final_query_equivalent = latest_note_rows == direct_note_rows;
    assert!(final_query_equivalent);
    eprintln!(
        "note-scoring-pruning-evidence source_rows={} candidates_scored={} bodies_examined={} bodies_safely_pruned={} retained_results={} active_queries={} pending_queries={} active_high_water={} pending_high_water={} cancellation_requests={} final_query_equivalent={}",
        note_scoring_entries.len(),
        direct_note_scoring_metrics.candidates_scored,
        direct_note_scoring_metrics.note_bodies_examined,
        direct_note_scoring_metrics.note_bodies_safely_pruned,
        retained_note_rows,
        scoring_ownership.active,
        scoring_ownership.pending,
        scoring_ownership.active_high_water,
        scoring_ownership.pending_high_water,
        scoring_ownership.cancellation_requests,
        final_query_equivalent,
    );
    let mut note_coordinator = palette::NotesBrowserQueryCoordinator::default();
    let active_note = note_coordinator
        .submit(note_request.clone())
        .expect("first Notes query starts");
    for index in 0..32 {
        let _ = note_coordinator.submit(palette::NotesBrowserQueryRequest {
            query: format!("latest-{index}"),
            mode: palette::NotesBrowserMode::AllNotes,
        });
    }
    let note_ownership = note_coordinator.snapshot();
    let _ = note_coordinator.finish(active_note.generation);

    let preview_dir = TempDir::new().expect("preview benchmark data dir");
    let preview_path = preview_dir.path().join("preview-source.txt");
    fixture::write_text(&preview_path, "current\n");
    let preview_text = format!("preview 🙂 {}", "p".repeat(PREVIEW_BYTES));
    local_history_service::capture_snapshot_for_path(
        preview_dir.path(),
        &preview_path,
        &preview_text,
        LocalHistorySnapshotOrigin::Save,
        LocalHistoryCapturePolicy::DeduplicateLatest,
    )
    .expect("seed preview benchmark snapshot");
    let preview_id =
        local_history_service::list_snapshots_for_path(preview_dir.path(), &preview_path)
            .expect("list preview benchmark snapshots")
            .into_iter()
            .next()
            .expect("preview benchmark snapshot")
            .snapshot_id;
    let preview_cancellation = local_history_service::LocalHistoryPreviewCancellation::default();
    let preview_outcome = local_history_service::load_snapshot_for_path_cancellable(
        preview_dir.path(),
        &preview_path,
        &preview_id,
        &preview_cancellation,
    )
    .expect("load preview benchmark snapshot");
    let local_history_service::LocalHistoryPreviewLoadOutcome::Loaded(preview) = preview_outcome
    else {
        panic!("preview benchmark snapshot must load");
    };
    let mut preview_offset = 0;
    let mut preview_slices = 0usize;
    while preview_offset < preview.text.len() {
        preview_offset = next_install_boundary(&preview.text, preview_offset);
        preview_slices = preview_slices.saturating_add(1);
    }
    let mut preview_coordinator = local_history_service::LocalHistoryPreviewCoordinator::default();
    let active_preview = preview_coordinator
        .submit(local_history_service::LocalHistoryPreviewRequest {
            path: preview_path.clone(),
            snapshot_id: preview_id.clone(),
        })
        .expect("first preview starts");
    for index in 0..32 {
        let _ = preview_coordinator.submit(local_history_service::LocalHistoryPreviewRequest {
            path: preview_path.clone(),
            snapshot_id: format!("latest-{index}"),
        });
    }
    let preview_ownership = preview_coordinator.snapshot();
    let _ = preview_coordinator.finish(active_preview.generation);

    let raw_events = raw_watch_events(&awkward_watch_paths(WORKSPACE_WATCH_PATH_CAP / 2));
    let watcher = WorkspaceWatchMailbox::new();
    for event in raw_events.iter().cloned() {
        watcher.merge_backend_result_for_benchmark(Ok(event));
    }
    let watcher_snapshot = watcher.snapshot();
    let (cache_input_rows, cache_operations) =
        child_cache_rebuild_operation_evidence_for_benchmark(CACHE_ROWS);
    assert!(cache_operations <= cache_input_rows.saturating_mul(8));
    let scan_flight_metrics = workspace_scan_pressure(10_000);
    assert_eq!(scan_flight_metrics.active_high_water, 1);
    assert_eq!(scan_flight_metrics.pending_high_water, 1);
    assert_eq!(scan_flight_metrics.starts, 2);
    let minimap_text =
        format!("{}\n", "s".repeat(MINIMAP_SHORT_LINE_CHARS)).repeat(MINIMAP_SHORT_LINES);
    let (minimap_result, minimap_slices, minimap_slice_high_water) =
        minimap_analysis(&minimap_text);
    assert_eq!(
        minimap_result.characters_examined,
        minimap_text.chars().count() as u64
    );
    assert!(minimap_slices > 1);
    assert!(minimap_slice_high_water <= MINIMAP_ANALYSIS_SLICE_CHARS);
    assert!(!minimap_result.wrapped_layout_too_large);
    assert!(minimap_result.long_line_lines.is_empty());

    eprintln!(
        "quality-gap-scale-evidence notes_entries={} notes_searchable_bytes={} notes_examined={} notes_active={} notes_pending={} note_scoring_candidates={} note_bodies_examined={} note_bodies_safely_pruned={} note_retained_results={} note_scoring_active={} note_scoring_pending={} note_final_query_equivalent={} preview_bytes={} preview_slices={} preview_retained_payloads=1 preview_active={} preview_pending={} raw_watcher_events={} watcher_retained_paths={} cache_input_rows={} cache_operations={} scan_requests=10000 scan_active_high_water={} scan_pending_high_water={} scan_starts={} scan_pending_replacements={} scan_terminals={} minimap_bytes={} minimap_characters={} minimap_lines={} minimap_slices={} minimap_slice_high_water={} minimap_marker_rows={}",
        note_entries.len(),
        note_bytes,
        note_metrics.candidates_examined,
        note_ownership.active,
        note_ownership.pending,
        direct_note_scoring_metrics.candidates_scored,
        direct_note_scoring_metrics.note_bodies_examined,
        direct_note_scoring_metrics.note_bodies_safely_pruned,
        retained_note_rows,
        scoring_ownership.active,
        scoring_ownership.pending,
        final_query_equivalent,
        preview.text.len(),
        preview_slices,
        preview_ownership.active,
        preview_ownership.pending,
        raw_events.len(),
        watcher_snapshot.retained_paths,
        cache_input_rows,
        cache_operations,
        scan_flight_metrics.active_high_water,
        scan_flight_metrics.pending_high_water,
        scan_flight_metrics.starts,
        scan_flight_metrics.pending_replacements,
        scan_flight_metrics.terminals,
        minimap_text.len(),
        minimap_result.characters_examined,
        minimap_result.lines_examined,
        minimap_slices,
        minimap_slice_high_water,
        minimap_result.long_line_lines.len(),
    );

    let preview_request =
        |index: u32| lushtext_core::services::bookmark_excerpt::BookmarkExcerptPreviewRequest {
            path: PathBuf::from(format!("/bench/notes/bookmark-{index}.md")),
            line: index,
        };
    let mut preview_coordinator =
        lushtext_core::services::bookmark_excerpt::BookmarkExcerptPreviewCoordinator::default();
    let first_preview = preview_coordinator
        .submit(preview_request(0))
        .expect("first preview submission starts");
    for index in 1..=512 {
        assert!(
            preview_coordinator.submit(preview_request(index)).is_none(),
            "superseding submissions must retain only the latest pending request"
        );
    }
    assert!(first_preview.cancellation.is_cancelled());
    let latest_preview = preview_coordinator
        .finish(first_preview.generation)
        .expect("latest pending preview starts after the active terminal");
    assert!(
        preview_coordinator
            .finish(latest_preview.generation)
            .is_none()
    );
    let preview_evidence = preview_coordinator.snapshot();
    if preview_evidence.active_high_water != 1
        || preview_evidence.pending_high_water != 1
        || preview_evidence.started != 2
    {
        panic!("bookmark preview churn must stay one-active/one-latest: {preview_evidence:?}");
    }
    eprintln!(
        "bookmark-preview-coordinator-evidence submissions=513 started={} active_high_water={} pending_high_water={} cancellation_requests={}",
        preview_evidence.started,
        preview_evidence.active_high_water,
        preview_evidence.pending_high_water,
        preview_evidence.cancellation_requests
    );

    let mut group = c.benchmark_group("quality_gap_scale");
    group.sample_size(10);
    group.bench_function("bookmark_preview_flight/rapid_10000", |b| {
        b.iter(|| {
            let mut coordinator = lushtext_core::services::bookmark_excerpt::BookmarkExcerptPreviewCoordinator::default();
            let mut active = coordinator.submit(preview_request(0));
            for index in 1..10_000u32 {
                assert!(coordinator.submit(preview_request(index)).is_none());
            }
            while let Some(start) = active.take() {
                active = coordinator.finish(start.generation);
            }
            black_box(coordinator.snapshot())
        });
    });
    group.bench_function("notes_browser/no_match_10000", |b| {
        b.iter(|| {
            black_box(palette::query_notes_browser_source(
                black_box(&note_entries),
                black_box(&note_request),
                NOTE_RENDER_CAP,
                &palette::PaletteSearchCancellation::default(),
            ))
        });
    });
    group.bench_function("note_scoring/metadata_dominates_large_bodies_10000", |b| {
        b.iter(|| black_box(run_note_scoring(black_box("calibration needle"))));
    });
    group.bench_function("local_history_preview/read_4_mib", |b| {
        b.iter(|| {
            black_box(
                local_history_service::load_snapshot_for_path_cancellable(
                    preview_dir.path(),
                    &preview_path,
                    &preview_id,
                    &local_history_service::LocalHistoryPreviewCancellation::default(),
                )
                .expect("benchmark preview read"),
            )
        });
    });
    group.bench_function("workspace_cache/terminal_rebuild_10000", |b| {
        b.iter(|| {
            black_box(child_cache_rebuild_operation_evidence_for_benchmark(
                CACHE_ROWS,
            ))
        });
    });
    group.bench_function("workspace_scan_flight/rapid_10000", |b| {
        b.iter(|| black_box(workspace_scan_pressure(black_box(10_000))));
    });
    group.bench_function("minimap_analysis/many_short_lines_2_mib", |b| {
        b.iter(|| black_box(minimap_analysis(black_box(&minimap_text))));
    });
    group.finish();
}

/// Largest event count any single projection batch of one plan carries.
fn max_batch_events_of(
    plan: &lushtext_core::services::markdown_render::MarkdownRenderPlan,
) -> usize {
    plan.batches
        .iter()
        .map(lushtext_core::services::markdown_render::MarkdownEventBatch::len)
        .max()
        .unwrap_or(0)
}

/// Benchmark GTK-free Markdown planning and emit the direct projection bounds.
fn bench_markdown_render_planning(c: &mut Criterion) {
    let mut dense = String::new();
    for index in 0..10_000 {
        writeln!(dense, "paragraph {index}\n").expect("write Markdown benchmark fixture");
    }
    let dense_plan = plan_markdown(&dense);
    assert!(dense_plan.is_complete());
    let max_batch_events = max_batch_events_of(&dense_plan);
    assert!(max_batch_events <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE);
    eprintln!(
        "markdown-render-bound-evidence source_bytes={} events={} batches={} max_events_per_slice={} retained_bytes={} embeds={} limit=none",
        dense_plan.metrics.source_bytes,
        dense_plan.metrics.events,
        dense_plan.batches.len(),
        max_batch_events,
        dense_plan.metrics.retained_bytes,
        dense_plan.metrics.embed_descriptors,
    );

    // One indivisible dense block: planning now omits it and keeps going, so
    // the interesting counters are the omission count and the surviving tail
    // rather than a terminal limit.
    let omitted = format!(
        "{}\n\ntail paragraph\n",
        (0..300).map(|_| "**x** ").collect::<String>()
    );
    let omitted_plan = plan_markdown(&omitted);
    assert!(omitted_plan.is_complete());
    assert_eq!(omitted_plan.omissions(), 1);
    assert!(max_batch_events_of(&omitted_plan) <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE);
    eprintln!(
        "markdown-render-omission-evidence source_bytes={} events={} batches={} omissions={} limit={:?}",
        omitted_plan.metrics.source_bytes,
        omitted_plan.metrics.events,
        omitted_plan.batches.len(),
        omitted_plan.omissions(),
        omitted_plan.limit,
    );

    // The two shapes that used to lose the document tail. Both now sub-slice
    // across turns, so the counters that matter are the batch count and the
    // per-slice high water: they are what bounds one GTK projection turn.
    let mut oversized_table =
        String::from("| key | default | description |\n| --- | --- | --- |\n");
    for row in 0..300 {
        writeln!(
            oversized_table,
            "| image.tag{row} | v1.{row}.0 | container image tag for component {row} |"
        )
        .expect("write Markdown table benchmark fixture");
    }
    oversized_table.push_str("\ntail paragraph\n");
    let table_plan = plan_markdown(&oversized_table);
    assert!(table_plan.is_complete());
    assert_eq!(table_plan.omissions(), 0);
    assert!(max_batch_events_of(&table_plan) <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE);
    eprintln!(
        "markdown-render-oversized-table-evidence source_bytes={} events={} batches={} max_events_per_slice={} retained_bytes={} omissions={} limit={:?}",
        table_plan.metrics.source_bytes,
        table_plan.metrics.events,
        table_plan.batches.len(),
        max_batch_events_of(&table_plan),
        table_plan.metrics.retained_bytes,
        table_plan.omissions(),
        table_plan.limit,
    );

    // Indented, not fenced: the pinned parser coalesces a fenced body into one
    // event, while an indented block emits one event per line, which is the
    // shape that actually sub-slices at text-run boundaries.
    let mut oversized_code = String::new();
    for line in 0..600 {
        writeln!(oversized_code, "    let value_{line} = compute({line});")
            .expect("write Markdown code benchmark fixture");
    }
    oversized_code.push_str("\ntail paragraph\n");
    let code_plan = plan_markdown(&oversized_code);
    assert!(code_plan.is_complete());
    assert_eq!(code_plan.omissions(), 0);
    assert!(max_batch_events_of(&code_plan) <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE);
    eprintln!(
        "markdown-render-oversized-code-evidence source_bytes={} events={} batches={} max_events_per_slice={} retained_bytes={} omissions={} limit={:?}",
        code_plan.metrics.source_bytes,
        code_plan.metrics.events,
        code_plan.batches.len(),
        max_batch_events_of(&code_plan),
        code_plan.metrics.retained_bytes,
        code_plan.omissions(),
        code_plan.limit,
    );

    let mut group = c.benchmark_group("markdown_render_planning");
    group.sample_size(10);
    group.bench_function("10000_paragraphs", |b| {
        b.iter(|| black_box(plan_markdown(black_box(&dense))));
    });
    group.bench_function("dense_single_block_omitted", |b| {
        b.iter(|| black_box(plan_markdown(black_box(&omitted))));
    });
    group.bench_function("oversized_table_sub_sliced", |b| {
        b.iter(|| black_box(plan_markdown(black_box(&oversized_table))));
    });
    group.bench_function("oversized_indented_code_sub_sliced", |b| {
        b.iter(|| black_box(plan_markdown(black_box(&oversized_code))));
    });
    group.finish();
}

/// Benchmark compact single-flight ownership and deterministic result retirement.
fn bench_search_interactive_policies(c: &mut Criterion) {
    fn rapid_queries(query_count: usize) -> (usize, usize) {
        let mut flight = WorkspaceSearchFlight::default();
        for index in 0..query_count {
            let submission = flight.submit(WorkspaceSearchRequest {
                spec: lushtext_core::model::content_search::SearchQuerySpec::new(
                    format!("query-{index}"),
                    ContentSearchOptions::default(),
                ),
                folders: Arc::from([PathBuf::from("/workspace")]),
            });
            if index == 0 {
                assert!(matches!(submission, WorkspaceSearchSubmission::Start(_)));
            }
        }
        let snapshot = flight.snapshot();
        (snapshot.active, snapshot.pending)
    }

    fn retire_cap() -> (usize, usize) {
        let mut categories = [1usize, 10_000, 1, 10_000, 1];
        let mut slices = 0usize;
        let mut high_water = 0usize;
        while categories.iter().any(|count| *count > 0) {
            let mut budget = SearchRetirementSliceBudget::new(250);
            for count in &mut categories {
                let retired = budget.take(*count);
                *count = count.saturating_sub(retired);
            }
            high_water = high_water.max(budget.retired());
            slices = slices.saturating_add(1);
        }
        (slices, high_water)
    }

    let (active, pending) = rapid_queries(1_000);
    let (retirement_slices, retirement_high_water) = retire_cap();
    assert_eq!((active, pending), (1, 1));
    assert!(retirement_high_water <= 250);
    eprintln!(
        "search-interactive-bound-evidence rapid_queries=1000 active_groups={active} pending_queries={pending} whole_result_clones=0 result_cap=10000 retirement_slices={retirement_slices} retired_rows_per_slice_high_water={retirement_high_water}",
    );

    let mut group = c.benchmark_group("search_interactive_policies");
    group.sample_size(10);
    group.bench_function("rapid_queries_1000", |b| {
        b.iter(|| black_box(rapid_queries(black_box(1_000))));
    });
    group.bench_function("retirement_budget_arithmetic_10000", |b| {
        b.iter(|| black_box(retire_cap()));
    });
    group.finish();
}

/// Benchmark compact save queue admission without constructing document payloads.
fn bench_save_admission_policy(c: &mut Criterion) {
    fn ordinary_burst() -> lushtext_core::model::save_admission::SaveAdmissionSnapshot {
        let mut policy = SaveAdmissionPolicy::default();
        let weight = SAVE_PAYLOAD_SHARED_BUDGET_BYTES / 8;
        for request_id in 0..8u64 {
            policy.queue(SaveAdmissionRequest {
                request_id,
                owner_id: request_id,
                save_generation: 1,
                destination_identity: request_id,
                close_session_identity: None,
                sequence: request_id,
                weight,
                priority: SaveAdmissionPriority::Ordinary,
            });
        }
        while policy
            .admit_next(ExternalTransientPressure::default())
            .is_some()
        {}
        policy.snapshot()
    }

    let snapshot = ordinary_burst();
    assert_eq!(snapshot.queued_count, 0);
    assert_eq!(snapshot.active_count, 8);
    assert!(snapshot.high_water_weight <= SAVE_PAYLOAD_SHARED_BUDGET_BYTES);
    eprintln!(
        "save-admission-bound-evidence queued_compact_saves={} active_payloads={} admitted_bytes={} admitted_bytes_high_water={} shared_budget={}",
        snapshot.queued_count,
        snapshot.active_count,
        snapshot.active_weight,
        snapshot.high_water_weight,
        SAVE_PAYLOAD_SHARED_BUDGET_BYTES,
    );

    let mut group = c.benchmark_group("save_admission_policy");
    group.sample_size(10);
    group.bench_function("ordinary_burst_8", |b| {
        b.iter(|| black_box(ordinary_burst()));
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_fuzzy_score,
    bench_recent_document_search,
    bench_file_index_search,
    bench_palette_pipeline_hardening,
    bench_file_index_rebuild,
    bench_end_to_end_boundedness,
    bench_file_index_incremental,
    bench_search_all,
    bench_scan_directory,
    bench_json_persistence,
    bench_utf8_validation,
    bench_editor_file_io,
    bench_line_ending_detection,
    bench_replace_preview_generation,
    bench_replace_undo_workflows,
    bench_tree_population,
    bench_file_size_classify,
    bench_editor_memory_policy,
    bench_draft_restore,
    bench_recovery_performance,
    bench_content_search,
    bench_content_search_smoke,
    bench_transient_file_load,
    bench_workspace_watch_pressure,
    bench_quality_gap_scale,
    bench_markdown_render_planning,
    bench_search_interactive_policies,
    bench_save_admission_policy,
);
criterion_main!(benches);

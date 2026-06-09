// SPDX-License-Identifier: GPL-3.0-or-later

//! Criterion benchmarks for LushText performance-sensitive code paths.
//!
//! All benchmarked functions are GTK-free — no display server needed.
//! Run with: `cargo bench -p lushtext-core` or `make bench`

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use gtk4::gio;
use gtk4::prelude::ListModelExt;
use std::collections::{HashSet, VecDeque};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tempfile::TempDir;

use lushtext_core::model::bookmark::BookmarkRecord;
use lushtext_core::model::content_search::{
    ContentSearchOptions, Replacement, SearchMatch, generate_replacement_preview,
};
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::encoding::{DocumentEncoding, LineEnding};
use lushtext_core::model::local_history::LocalHistorySnapshotOrigin;
use lushtext_core::model::local_history::{LocalHistoryDocument, LocalHistorySnapshotMeta};
use lushtext_core::model::migration_ledger::MigrationKind;
use lushtext_core::model::palette::IndexedFile;
use lushtext_core::model::palette::SearchMode;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::model::sidecar_identity::{next_record_id, now_epoch_millis, stable_bytes_hash};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceFolder, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::content_search;
use lushtext_core::services::editor_io;
use lushtext_core::services::file_limits::FileSizeCheck;
use lushtext_core::services::file_tree::{self, DirectoryEntry};
use lushtext_core::services::filesystem::{fixture, read as fs_read};
use lushtext_core::services::json_format::KIND_LOCAL_HISTORY_INDEX;
use lushtext_core::services::palette::{self, FileIndex};
use lushtext_core::services::recovery_metadata::{
    RecoveryLoadConfig, RecoveryMetadataClass, save_enveloped_json_path,
};
use lushtext_core::services::workspace_manager;
use lushtext_core::services::{
    bookmark_service, draft_service,
    local_history_service::{self, LocalHistoryCapturePolicy},
    migration_ledger, session_service,
};
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

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
            let name = format!("file_{i}.{ext}");
            let path = PathBuf::from(format!("/synthetic/project/{dir}/{name}"));
            IndexedFile {
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

fn populate_tree_store(entries: Vec<DirectoryEntry>, truncated: bool) -> gio::ListStore {
    const MAX_DIR_ENTRIES: usize = 10_000;
    const CHILD_APPEND_BATCH_SIZE: usize = 256;

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
                original_mtime_secs: Some(1000),
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
                path: path.clone(),
                line_number: u64::try_from(line_index + 1)
                    .expect("benchmark line index fits in u64"),
                original_line: original_line.to_string(),
                replaced_line: replaced_line.to_string(),
                replacement: "thread".to_string(),
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
            path,
            line_number: 1,
            original_line: original_line.to_string(),
            replaced_line: "prefix thread suffix".to_string(),
            replacement: "thread".to_string(),
            match_range: 7..13,
        }],
    )
}

/// Create one sparse file just over the Replace All cap so the benchmark tracks skip cost.
fn make_replace_all_over_cap_fixture() -> (TempDir, Vec<Replacement>) {
    let dir = TempDir::new().expect("expected operation to succeed");
    let path = dir.path().join("over-cap.txt");
    fixture::create_sparse_file(&path, content_search::MAX_REPLACE_FILE_BYTES + 1);
    (
        dir,
        vec![Replacement {
            path,
            line_number: 1,
            original_line: "needle".to_string(),
            replaced_line: "thread".to_string(),
            replacement: "thread".to_string(),
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

fn bench_file_index_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_index_rebuild");
    group.sample_size(20);

    for file_count in [50, 500, 5_000, 10_000, 100_000] {
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

fn bench_replace_undo_workflows(c: &mut Criterion) {
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
                    .expect("expected operation to succeed");
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
                    .expect("expected operation to succeed");
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
                .expect("expected operation to succeed");
                black_box((result.replaced_count, backup.len(), dir));
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
                .expect("expected operation to succeed");
                black_box((result.skipped_paths.len(), backup.len(), dir));
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

fn bench_draft_restore(c: &mut Criterion) {
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

    // Benchmark orphan cleanup in isolation.
    for &(label, n_valid, n_orphan_entries, n_orphan_files) in &[
        ("clean_5", 5, 0, 0),
        ("5_orphan_entries", 5, 5, 0),
        ("5_orphan_files", 5, 0, 5),
        ("mixed_20", 10, 5, 5),
    ] {
        group.bench_function(BenchmarkId::new("cleanup_orphans", label), |b| {
            b.iter_batched(
                || {
                    let dir = TempDir::new().expect("expected operation to succeed");
                    let mut manifest = DraftManifest::default();

                    // Valid entries (with draft files).
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
                    // Orphan manifest entries (no draft files).
                    for i in 0..n_orphan_entries {
                        manifest.upsert(DraftEntry {
                            draft_id: format!("orphan-entry-{i}"),
                            original_path: None,
                            original_mtime_secs: None,
                            saved_at_secs: 1000,
                        });
                    }
                    // Create the drafts directory for orphan files.
                    fixture::create_dir_all(&draft_service::drafts_dir(dir.path()));
                    // Orphan draft files (no manifest entries).
                    for i in 0..n_orphan_files {
                        draft_service::write_draft(
                            dir.path(),
                            &format!("orphan-file-{i}"),
                            "stale",
                        )
                        .expect("expected operation to succeed");
                    }

                    (dir, manifest)
                },
                |(dir, mut manifest)| {
                    let _ = draft_service::cleanup_orphans(black_box(dir.path()), &mut manifest);
                    (dir, manifest)
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
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
                let bookmarks = bookmark_service::list_workspace_bookmarks_recovering(
                    black_box(dir.path()),
                    black_box(&[workspace]),
                )
                .expect("expected operation to succeed");
                black_box((
                    restore.diagnostics.len(),
                    ledger.diagnostics.len(),
                    bookmarks.diagnostics.len(),
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

    // 1. Literal search across 10k files.
    group.bench_function("literal_10k_files", |b| {
        b.iter(|| {
            let (tx, rx) = crossbeam_channel::bounded(1024);
            let cancel = Arc::new(AtomicBool::new(false));
            content_search::search(
                black_box("TODO"),
                black_box(&[search_root]),
                &ContentSearchOptions::default(),
                tx,
                cancel,
                None,
                None,
            );
            // Drain to avoid backpressure stalls.
            for _ in &rx {}
        });
    });

    // 2. Regex search across 10k files.
    group.bench_function("regex_10k_files", |b| {
        let opts = ContentSearchOptions {
            regex: true,
            ..Default::default()
        };
        b.iter(|| {
            let (tx, rx) = crossbeam_channel::bounded(1024);
            let cancel = Arc::new(AtomicBool::new(false));
            content_search::search(
                black_box(r"fn\s+\w+"),
                black_box(&[search_root]),
                &opts,
                tx,
                cancel,
                None,
                None,
            );
            for _ in &rx {}
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
            let (tx, rx) = crossbeam_channel::bounded(1024);
            let cancel = Arc::new(AtomicBool::new(false));
            content_search::search(
                black_box("TODO"),
                black_box(&[large_root]),
                &ContentSearchOptions::default(),
                tx,
                cancel,
                None,
                None,
            );
            for _ in &rx {}
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
            let (tx, rx) = crossbeam_channel::bounded(1024);
            let cancel = Arc::new(AtomicBool::new(false));
            content_search::search(
                black_box("needle"),
                black_box(&[gitignore_root]),
                &ContentSearchOptions::default(),
                tx,
                cancel,
                None,
                None,
            );
            for _ in &rx {}
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
            let (tx, rx) = crossbeam_channel::bounded(1024);
            let cancel = Arc::new(AtomicBool::new(false));
            content_search::search(
                black_box("TODO"),
                black_box(&[search_root]),
                &ContentSearchOptions::default(),
                tx,
                cancel,
                None,
                None,
            );
            for _ in &rx {}
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
            let (tx, rx) = crossbeam_channel::bounded(1024);
            let cancel = Arc::new(AtomicBool::new(false));
            content_search::search(
                black_box("TODO"),
                black_box(&[large_root]),
                &ContentSearchOptions::default(),
                tx,
                cancel,
                None,
                None,
            );
            for _ in &rx {}
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_fuzzy_score,
    bench_file_index_search,
    bench_file_index_rebuild,
    bench_file_index_incremental,
    bench_search_all,
    bench_scan_directory,
    bench_json_persistence,
    bench_utf8_validation,
    bench_editor_file_io,
    bench_replace_preview_generation,
    bench_replace_undo_workflows,
    bench_tree_population,
    bench_file_size_classify,
    bench_draft_restore,
    bench_recovery_performance,
    bench_content_search,
    bench_content_search_smoke,
);
criterion_main!(benches);

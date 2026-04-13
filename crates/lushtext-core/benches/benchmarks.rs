// SPDX-License-Identifier: GPL-3.0-or-later

//! Criterion benchmarks for LushText performance-sensitive code paths.
//!
//! All benchmarked functions are GTK-free — no display server needed.
//! Run with: `cargo bench -p lushtext-core` or `make bench`

use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use gtk4::gio;
use gtk4::prelude::ListModelExt;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

use lushtext_core::model::content_search::ContentSearchOptions;
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::palette::IndexedFile;
use lushtext_core::model::palette::SearchMode;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceEntry, WorkspaceId, WorkspacesFile,
};
use lushtext_core::services::content_search;
use lushtext_core::services::editor_io;
use lushtext_core::services::file_limits::FileSizeCheck;
use lushtext_core::services::file_tree::{self, DirectoryEntry};
use lushtext_core::services::json_store;
use lushtext_core::services::palette::{self, FileIndex};
use lushtext_core::services::workspace_manager;
use lushtext_core::services::{draft_service, session_service};
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
                workspace_root: Arc::clone(&root),
            }
        })
        .collect();

    FileIndex::from(files)
}

/// Create a temp directory tree with the given number of files spread across subdirs.
fn make_temp_dir_tree(file_count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    let subdirs = ["src", "src/model", "src/services", "tests", "docs"];
    for subdir in &subdirs {
        std::fs::create_dir_all(dir.path().join(subdir)).unwrap();
    }
    for i in 0..file_count {
        let subdir = subdirs[i % subdirs.len()];
        std::fs::write(dir.path().join(format!("{subdir}/file_{i}.rs")), "").unwrap();
    }
    dir
}

/// Create a flat temp directory with mixed files and subdirs for `scan_directory` benchmarks.
fn make_flat_dir(entry_count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    let n_dirs = entry_count / 2;
    for i in 0..n_dirs {
        std::fs::create_dir(dir.path().join(format!("dir_{i}"))).unwrap();
    }
    for i in 0..(entry_count - n_dirs) {
        std::fs::write(dir.path().join(format!("file_{i}.rs")), "").unwrap();
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
            entries: (0..entries_per)
                .map(|e| WorkspaceEntry::Directory {
                    path: PathBuf::from(format!("/home/user/project_{w}/dir_{e}")),
                })
                .collect(),
        })
        .collect();

    WorkspacesFile {
        active_workspace: Some(WorkspaceId::new("ws-0")),
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
    let dir = TempDir::new().unwrap();

    // Create real files for session tabs (filter_existing_tabs will stat them).
    let tab_dir = dir.path().join("project");
    std::fs::create_dir_all(&tab_dir).unwrap();
    let mut tabs = Vec::with_capacity(n_tabs);
    for i in 0..n_tabs {
        let file_path = tab_dir.join(format!("file_{i}.rs"));
        std::fs::write(&file_path, "fn main() {}").unwrap();
        tabs.push(SessionTab {
            path: Some(file_path),
            draft_id: None,
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
        });
    }

    // Create draft files + manifest for the first n_drafts tabs.
    let draft_content = "x".repeat(draft_size);
    let mut manifest = DraftManifest::default();
    for tab in tabs.iter().take(n_drafts) {
        if let Some(ref path) = tab.path {
            let draft_id = draft_service::draft_id_for_path(path);
            draft_service::write_draft(dir.path(), &draft_id, &draft_content).unwrap();
            manifest.upsert(DraftEntry {
                draft_id,
                original_path: Some(path.clone()),
                original_mtime_secs: Some(1000),
                saved_at_secs: 2000,
            });
        }
    }
    draft_service::save_manifest(dir.path(), &manifest).unwrap();
    session_service::save(
        dir.path(),
        &SessionData {
            tabs: tabs.clone(),
            active_tab_index: Some(0),
        },
    )
    .unwrap();

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
                cursor_line: (i as u32) % 500,
                cursor_col: 0,
                scroll_line: 0,
            })
            .collect(),
        active_tab_index: Some(0),
    }
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
            b.iter(|| idx.search(black_box("file_42"), 50))
        });

        group.bench_with_input(BenchmarkId::new("empty_query", size), &index, |b, idx| {
            b.iter(|| idx.search(black_box(""), 50))
        });

        group.bench_with_input(BenchmarkId::new("no_match", size), &index, |b, idx| {
            b.iter(|| idx.search(black_box("zzzzz"), 50))
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
        for mode in [SearchMode::Files, SearchMode::Commands, SearchMode::All] {
            let label = match mode {
                SearchMode::Files => "files",
                SearchMode::Commands => "commands",
                SearchMode::All => "all",
            };
            group.bench_with_input(BenchmarkId::new(label, size), &index, |b, idx| {
                b.iter(|| palette::search_all(black_box(idx), black_box("file_42"), mode, 50))
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
            || TempDir::new().unwrap(),
            |dir| {
                workspace_manager::save(dir.path(), black_box(&small)).unwrap();
                dir // keep TempDir alive past timing
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("save/large", |b| {
        b.iter_batched(
            || TempDir::new().unwrap(),
            |dir| {
                workspace_manager::save(dir.path(), black_box(&large)).unwrap();
                dir
            },
            BatchSize::SmallInput,
        );
    });

    // Load benchmarks — pre-write the file, then benchmark reads
    group.bench_function("load/small", |b| {
        b.iter_batched(
            || {
                let dir = TempDir::new().unwrap();
                workspace_manager::save(dir.path(), &small).unwrap();
                dir
            },
            |dir| {
                let _: WorkspacesFile =
                    json_store::load(black_box(dir.path()), "workspaces.json").unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("load/large", |b| {
        b.iter_batched(
            || {
                let dir = TempDir::new().unwrap();
                workspace_manager::save(dir.path(), &large).unwrap();
                dir
            },
            |dir| {
                let _: WorkspacesFile =
                    json_store::load(black_box(dir.path()), "workspaces.json").unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    // Session save/load
    let session = make_session_data(50);
    group.bench_function("session_save/50_tabs", |b| {
        b.iter_batched(
            || TempDir::new().unwrap(),
            |dir| {
                json_store::save(dir.path(), "session-bench.json", black_box(&session)).unwrap();
                dir // keep TempDir alive past timing
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("session_load/50_tabs", |b| {
        b.iter_batched(
            || {
                let dir = TempDir::new().unwrap();
                json_store::save(dir.path(), "session-bench.json", &session).unwrap();
                dir
            },
            |dir| {
                let _: SessionData =
                    json_store::load(black_box(dir.path()), "session-bench.json").unwrap();
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
            BenchmarkId::new("read_to_string", format!("{size_mb}MB")),
            |b| {
                b.iter_batched(
                    || {
                        let dir = TempDir::new().unwrap();
                        let path = dir.path().join("bench.txt");
                        std::fs::write(&path, &content).unwrap();
                        (dir, path)
                    },
                    |(dir, path)| {
                        let _s = std::fs::read_to_string(black_box(&path)).unwrap();
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
                        let dir = TempDir::new().unwrap();
                        let path = dir.path().join("bench.txt");
                        std::fs::write(&path, &content).unwrap();
                        (dir, path)
                    },
                    |(dir, path)| {
                        let bytes = std::fs::read(black_box(&path)).unwrap();
                        simdutf8::basic::from_utf8(&bytes).unwrap();
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
                        let dir = TempDir::new().unwrap();
                        let path = dir.path().join("bench.txt");
                        std::fs::write(&path, &content).unwrap();
                        (dir, path, AtomicBool::new(false))
                    },
                    |(dir, path, cancel)| {
                        let _loaded = editor_io::load_text_file(
                            black_box(path.as_path()),
                            black_box(&cancel),
                        )
                        .unwrap();
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
                        let dir = TempDir::new().unwrap();
                        let path = dir.path().join("bench.txt");
                        (dir, path, content.clone())
                    },
                    |(dir, path, text)| {
                        let _written =
                            editor_io::write_snapshot_to_path(black_box(&path), black_box(&text))
                                .unwrap();
                        dir
                    },
                    BatchSize::SmallInput,
                );
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
                        workspace_root: root,
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
            b.iter(|| FileSizeCheck::classify(black_box(s)))
        });
    }
    group.finish();
}

fn bench_draft_restore(c: &mut Criterion) {
    let mut group = c.benchmark_group("draft_restore");
    group.sample_size(30);

    // Benchmark the full startup preload pipeline:
    // load manifest + load session + filter_existing_tabs + batch-read drafts.
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
                    // Simulate the background thread work from load_session_and_drafts.
                    let manifest =
                        draft_service::load_manifest(black_box(dir.path())).unwrap_or_default();
                    let mut session =
                        session_service::load(black_box(dir.path())).unwrap_or_default();
                    session_service::filter_existing_tabs(&mut session);

                    let mut preloaded = std::collections::HashMap::new();
                    for tab in &session.tabs {
                        if let Some(ref path) = tab.path {
                            let draft_id = draft_service::draft_id_for_path(path);
                            if manifest.find_by_id(&draft_id).is_some()
                                && let Ok(Some(content)) =
                                    draft_service::read_draft(dir.path(), &draft_id)
                            {
                                preloaded.insert(draft_id, content);
                            }
                        }
                    }

                    (manifest, session, preloaded, dir)
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
                    let dir = TempDir::new().unwrap();
                    let mut manifest = DraftManifest::default();

                    // Valid entries (with draft files).
                    for i in 0..n_valid {
                        let id = format!("valid-{i}");
                        draft_service::write_draft(dir.path(), &id, "content").unwrap();
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
                    std::fs::create_dir_all(draft_service::drafts_dir(dir.path())).unwrap();
                    // Orphan draft files (no manifest entries).
                    for i in 0..n_orphan_files {
                        draft_service::write_draft(
                            dir.path(),
                            &format!("orphan-file-{i}"),
                            "stale",
                        )
                        .unwrap();
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

fn bench_content_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_search");
    group.sample_size(20);

    // --- Fixture: 10k files for literal & regex search ---
    let search_dir = TempDir::new().unwrap();
    let search_root = search_dir.path();
    for i in 0..10_000 {
        let subdir = format!("dir_{}", i / 500);
        let dir_path = search_root.join(&subdir);
        std::fs::create_dir_all(&dir_path).unwrap();
        // Every 5th file contains the search target.
        let content = if i % 5 == 0 {
            format!("fn handler_{i}() {{ TODO: implement }}\nlet x = {i};\n")
        } else {
            format!("let value_{i} = {i};\nlet other = true;\n")
        };
        std::fs::write(dir_path.join(format!("file_{i}.rs")), content).unwrap();
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
            for _ in rx.iter() {}
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
            for _ in rx.iter() {}
        });
    });

    // --- Fixture: single large file (100k lines) ---
    let large_dir = TempDir::new().unwrap();
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
        std::fs::write(large_root.join("huge.txt"), content).unwrap();
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
            for _ in rx.iter() {}
        });
    });

    // --- Fixture: gitignore filtering (10k files, half in ignored dirs) ---
    let gitignore_dir = TempDir::new().unwrap();
    let gitignore_root = gitignore_dir.path();
    {
        std::fs::create_dir(gitignore_root.join(".git")).unwrap();
        std::fs::write(gitignore_root.join(".gitignore"), "ignored_*/\n").unwrap();
        for i in 0..20 {
            let name = if i < 10 {
                format!("ignored_{i}")
            } else {
                format!("visible_{i}")
            };
            let dir_path = gitignore_root.join(&name);
            std::fs::create_dir_all(&dir_path).unwrap();
            for j in 0..500 {
                std::fs::write(dir_path.join(format!("file_{j}.rs")), "fn needle() {}\n").unwrap();
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
            for _ in rx.iter() {}
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
    bench_tree_population,
    bench_file_size_classify,
    bench_draft_restore,
    bench_content_search,
);
criterion_main!(benches);

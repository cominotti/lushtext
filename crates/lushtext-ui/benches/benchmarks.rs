// SPDX-License-Identifier: GPL-3.0-or-later

//! Criterion benchmarks for UI policies, GTK model assembly, and minimap scans.

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use gtk4::gio;
use gtk4::prelude::ListModelExt;
use notify_debouncer_full::notify::event::{AccessKind, AccessMode, CreateKind};
use notify_debouncer_full::notify::{Event, EventKind};
use std::collections::{HashSet, VecDeque};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

use lushtext_model::document::file_load::next_install_boundary;
use lushtext_model::persistence::local_history::LocalHistorySnapshotOrigin;
use lushtext_model::values::{SharedPath, SharedPaths};
use lushtext_model::workspace::content_search::ContentSearchOptions;
use lushtext_model::workspace::palette::{
    FileOrdinal, PaletteCategory, PaletteFileEntry, PaletteFileIdentity, PaletteNoteCategory,
    PaletteNoteEntry, PaletteNoteTarget, PaletteSearchRow, SearchMode,
};
use lushtext_model::workspace::workspace_visibility::WorkspaceEntryVisibility;
use lushtext_services::filesystem::fixture;
use lushtext_services::palette::{self, FileIndex, OwnedIndexedFile};
use lushtext_services::persistence::local_history_service::{self, LocalHistoryCapturePolicy};
use lushtext_services::watch::workspace_watch::{WORKSPACE_WATCH_PATH_CAP, WorkspaceWatchMailbox};
use lushtext_services::workspace::file_tree::{self, DirectoryEntry, DirectoryPlaceholder};
use lushtext_ui::ui::command_palette::policy::{MAX_CATEGORY_VIEW_ROWS, category_row_limit};
use lushtext_ui::ui::editor_page::save::policy::{
    ExternalTransientPressure, SAVE_PAYLOAD_SHARED_BUDGET_BYTES, SaveAdmissionPolicy,
    SaveAdmissionPriority, SaveAdmissionRequest,
};
use lushtext_ui::ui::editor_page::{
    MinimapAnalysisAccumulator, MinimapAnalysisPolicy, MinimapAnalysisResult,
};
use lushtext_ui::ui::search_panel::policy::{
    SearchRetirementSliceBudget, WorkspaceSearchFlight, WorkspaceSearchRequest,
    WorkspaceSearchSubmission,
};
use lushtext_ui::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_ui::ui::sidebar::policy::WorkspaceScanFlight;
use lushtext_ui::ui::sidebar::workspace_section::child_cache_rebuild_operation_evidence_for_benchmark;

/// Build a synthetic in-memory file index with realistic file names.
fn make_synthetic_index(n: usize) -> FileIndex {
    let root = SharedPath::from("/synthetic/project");
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

    // Streamed into the index: 2^20 owned rows are never held at once.
    FileIndex::from_paths((0..n).map(|i| {
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
        (
            path.clone(),
            PaletteFileIdentity::canonical(path),
            root.clone(),
        )
    }))
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

/// Mirror bounded breadth-first sidebar model population and batching costs.
///
/// `limited` carries the budget row's counts when the sidebar's retained-byte
/// budget cut the listing (`bound-lists-by-budget-not-count`; the former
/// 10,000-entry mirror constant is gone with the cap).
fn populate_tree_store(
    entries: Vec<DirectoryEntry>,
    limited: Option<(usize, usize)>,
) -> gio::ListStore {
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

    if let Some((shown, total)) = limited {
        let placeholder = [FileTreeItem::new_placeholder(
            format!("Showing {shown} of {total} entries"),
            DirectoryPlaceholder::Budget { shown, total },
        )];
        store.splice(store.n_items(), 0, &placeholder);
    }

    store
}

/// The All Tabs popover's ranking over 1,024 open tabs: the ranking's share
/// of one keystroke on the GTK thread (`tab-strip-overflow-navigation` D6);
/// the keystroke's candidate build, row publish and band re-projection are
/// not measured here. Regression evidence, not a timing verdict.
fn bench_all_tabs_filter(c: &mut Criterion) {
    let titles: Vec<String> = (0..1_024)
        .map(|index| {
            format!(
                "document-{index:04}-{}.rs",
                ["mod", "notes", "résumé", "main"][index % 4]
            )
        })
        .collect();
    let folders: Vec<String> = (0..1_024)
        .map(|index| format!("~/workspace/projet-équipe/section-{}/nested", index % 32))
        .collect();
    // The absolute folders behind the shown `~` labels, which the filter
    // matches too.
    let folder_paths: Vec<String> = folders
        .iter()
        .map(|folder| folder.replacen('~', "/var/home/user", 1))
        .collect();
    let candidates: Vec<lushtext_ui::ui::window::AllTabsCandidate<'_>> = titles
        .iter()
        .zip(folders.iter().zip(&folder_paths))
        .map(
            |(title, (folder, folder_path))| lushtext_ui::ui::window::AllTabsCandidate {
                title,
                folder: Some(folder),
                folder_path: Some(folder_path),
            },
        )
        .collect();
    let mut group = c.benchmark_group("all_tabs_filter_1024");
    for (id, query) in [
        ("empty", ""),
        ("prefix", "document-07"),
        ("substring", "notes"),
        ("folder", "section-31"),
        ("fuzzy", "dcmnt"),
        ("no_match", "zzqvv"),
    ] {
        group.bench_with_input(BenchmarkId::new("query", id), query, |b, query| {
            b.iter(|| {
                lushtext_ui::ui::window::rank_all_tabs(black_box(query), black_box(&candidates))
            });
        });
    }
    group.finish();
}

/// The open tabs a query excludes: every 4,096th file's canonical path, 32 at
/// most (`scale-file-index` task 4.1).
fn bench_open_tab_paths(index: &FileIndex) -> HashSet<PathBuf> {
    index
        .files()
        .step_by(4096)
        .take(32)
        .filter_map(|file| file.identity().canonical_path().map(Path::to_path_buf))
        .collect()
}

/// One query, the empty query, no match, and the query and empty query with
/// 32 open tabs excluded.
/// The environment variable that admits the 2^20-file groups
/// (`file_index_scale`, `file_index_scale_rebuild`,
/// `palette_pipeline_hardening_1048576`). Their setup runs in the group
/// function, before Criterion's filter, so without the gate every filtered
/// run (`make performance-smoke` runs one per filter) would build a
/// 2^20-file index and write the D9 tree to disk.
const SCALE_BENCH_ENV: &str = "LUSHTEXT_BENCH_SCALE";

fn scale_benches_enabled(group: &str) -> bool {
    let enabled = std::env::var_os(SCALE_BENCH_ENV).is_some_and(|value| value == "1");
    if !enabled {
        eprintln!("skipping {group}: set {SCALE_BENCH_ENV}=1 (make bench) to run it");
    }
    enabled
}

fn bench_file_index_queries(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    index: &FileIndex,
    size: usize,
) {
    let open = bench_open_tab_paths(index);
    let cancellation = palette::PaletteSearchCancellation::default();
    group.bench_function(BenchmarkId::new("query_match", size), |b| {
        b.iter(|| index.search(black_box("file_42"), 50));
    });
    group.bench_function(BenchmarkId::new("empty_query", size), |b| {
        b.iter(|| index.search(black_box(""), 50));
    });
    group.bench_function(BenchmarkId::new("no_match", size), |b| {
        b.iter(|| index.search(black_box("zzzzz"), 50));
    });
    group.bench_function(BenchmarkId::new("query_open32", size), |b| {
        b.iter(|| {
            index.search_cancellable_excluding(black_box("file_42"), 50, &open, &cancellation)
        });
    });
    group.bench_function(BenchmarkId::new("empty_query_open32", size), |b| {
        b.iter(|| index.search_cancellable_excluding(black_box(""), 64, &open, &cancellation));
    });
}

fn bench_file_path(index: &FileIndex, position: usize) -> PathBuf {
    index
        .file(FileOrdinal(
            u32::try_from(position).expect("a bench position"),
        ))
        .expect("a file at the position")
        .path()
}

/// `drill-down-palette-categories` M2 and D3: the empty query's count pass
/// over the rest of a 131,072-file index (0, 32 and 256 open tabs), and one
/// category ranked at the row cap (`scale-file-index` D8).
fn bench_palette_categories(c: &mut Criterion) {
    let index = make_synthetic_index(MAX_CATEGORY_VIEW_ROWS);
    let open_tabs = |count: usize| -> Vec<PaletteFileEntry> {
        index
            .files()
            .take(count)
            .map(|file| {
                PaletteFileEntry::new(
                    file.name().to_string(),
                    file.relative_display(),
                    file.path(),
                    file.identity(),
                )
            })
            .collect()
    };
    let cancellation = palette::PaletteSearchCancellation::default();
    let mut group = c.benchmark_group("palette_categories");
    group.sample_size(20);
    for open in [0usize, 32, 256] {
        let tabs = open_tabs(open);
        group.bench_with_input(
            BenchmarkId::new("empty_query_grouped_files_131072", open),
            &tabs,
            |b, tabs| {
                b.iter(|| {
                    palette::grouped_search(
                        palette::GroupedSearchInput {
                            index: &index,
                            open_tabs: black_box(tabs),
                            note_entries: &[],
                            workspace_group_label: "All Workspaces",
                            query: "",
                            mode: SearchMode::Files,
                            max_per_source: 64,
                        },
                        &cancellation,
                    )
                });
            },
        );
    }
    for query in ["", "file"] {
        group.bench_with_input(
            BenchmarkId::new(
                "category_ranking_131072",
                if query.is_empty() { "empty" } else { query },
            ),
            query,
            |b, query| {
                b.iter(|| {
                    palette::category_search(
                        palette::CategorySearchInput {
                            index: &index,
                            open_tabs: &[],
                            note_entries: &[],
                            category: PaletteCategory::WorkspaceFiles,
                            query: black_box(query),
                            row_limit: category_row_limit(),
                        },
                        &cancellation,
                    )
                });
            },
        );
    }
    group.finish();
}

/// `scale-file-index` task 4.1: the 2^20-file index, run only when
/// [`SCALE_BENCH_ENV`] is `1` (`make bench` sets it), because its setup builds
/// the index and writes a million files before Criterion applies its filter. One query, the
/// empty query, no match and 32 open tabs; the base clone and each mutation
/// kind of the incremental worker; the category view's ranking at the row
/// cap of a query matching every file.
fn bench_file_index_scale(c: &mut Criterion) {
    if !scale_benches_enabled("file_index_scale") {
        return;
    }
    let size = palette::MAX_INDEXED_FILES;
    let index = make_synthetic_index(size);
    // `make_synthetic_index` repeats one tie name per directory (every
    // 997th file), so those paths collapse; every other path is indexed.
    let ties = (0..size).filter(|i| i % 997 == 0 && i % 1_000 != 0).count();
    assert!(
        index.len() >= size - ties,
        "the synthetic index is not truncated: {} of {size}",
        index.len()
    );
    eprintln!(
        "file-index-scale files={} retained_index_bytes={} retained_b_per_file={}",
        index.len(),
        index.retained_byte_weight(),
        index.retained_byte_weight() / index.len() as u64,
    );
    bench_file_index_scale_operations(c, &index, size);

    // The real traversal of the D9 tree on disk: no truncation, and both
    // byte ledgers within their caps at 2^20 files (the spec's
    // 1,048,576-file scenario).
    let tree = tempfile::TempDir::new().expect("D9 tree temp directory");
    let summary = palette::workspace_index_fixture::write(
        tree.path(),
        size,
        palette::workspace_index_fixture::DEFAULT_SEED,
        true,
    );
    let folders = [tree.path().to_path_buf()];
    let build = || {
        let palette::FileIndexBuildOutcome::Complete { index, metrics } =
            FileIndex::rebuild_cancellable_with_hint(
                &folders,
                0,
                &palette::PaletteSearchCancellation::default(),
                &WorkspaceEntryVisibility::default(),
            )
        else {
            panic!("a fresh token cannot cancel");
        };
        (index, metrics)
    };
    let (built, metrics) = build();
    eprintln!(
        "file-index-scale-d9 files_written={} directories_written={} indexed={} truncation={:?} peak_build_bytes={} retained_index_bytes={} canonical_resolutions={}",
        summary.files,
        summary.directories,
        built.len(),
        metrics.truncation,
        metrics.peak_build_bytes,
        metrics.retained_index_bytes,
        metrics.canonical_resolutions,
    );
    assert_eq!(
        metrics.truncation, None,
        "the 2^20 D9 tree is indexed whole"
    );
    assert!(metrics.peak_build_bytes <= 128 * 1024 * 1024);
    assert!(metrics.retained_index_bytes <= 64 * 1024 * 1024);
    drop(built);
    let mut group = c.benchmark_group("file_index_scale_rebuild");
    group.sample_size(10);
    group.bench_function(BenchmarkId::new("d9_tree", size), |b| {
        b.iter(|| black_box(build()));
    });
    group.finish();
}

/// The query, clone, mutation and category-ranking cases of
/// [`bench_file_index_scale`], on the synthetic 2^20-file index.
fn bench_file_index_scale_operations(c: &mut Criterion, index: &FileIndex, size: usize) {
    let mut group = c.benchmark_group("file_index_scale");
    group.sample_size(10);
    bench_file_index_queries(&mut group, index, size);
    group.bench_function(BenchmarkId::new("clone", size), |b| {
        b.iter_with_large_drop(|| index.clone());
    });
    let root = SharedPath::from("/synthetic/project");
    let added = PathBuf::from("/synthetic/project/src/new_file.rs");
    // The index is full at 2^20, so each repetition first frees one file
    // (untimed): the timed add is admitted, not refused at the cap.
    let freed = bench_file_path(index, index.len() - 1);
    let below_cap = || {
        let mut copy = index.clone();
        copy.remove_path(&freed);
        copy
    };
    let add = |index: &mut FileIndex| {
        index.add_file(OwnedIndexedFile::new(
            added.clone(),
            PaletteFileIdentity::canonical(added.clone()),
            root.clone(),
        ));
    };
    let mut admitted = below_cap();
    add(&mut admitted);
    assert_eq!(
        admitted.len(),
        index.len(),
        "the add below the cap is admitted"
    );
    drop(admitted);
    group.bench_function(BenchmarkId::new("add_file", size), |b| {
        b.iter_batched(
            below_cap,
            |mut index| {
                add(&mut index);
                index
            },
            BatchSize::LargeInput,
        );
    });
    let middle = bench_file_path(index, size / 2);
    group.bench_function(BenchmarkId::new("remove_path_file", size), |b| {
        b.iter_batched(
            || index.clone(),
            |mut index| {
                index.remove_path(black_box(&middle));
                index
            },
            BatchSize::LargeInput,
        );
    });
    let model = PathBuf::from("/synthetic/project/src/model");
    group.bench_function(BenchmarkId::new("remove_path_dir", size), |b| {
        b.iter_batched(
            || index.clone(),
            |mut index| {
                index.remove_path(black_box(&model));
                index
            },
            BatchSize::LargeInput,
        );
    });
    let renamed = middle.with_file_name("renamed.rs");
    group.bench_function(BenchmarkId::new("rename_path_file", size), |b| {
        b.iter_batched(
            || index.clone(),
            |mut index| {
                index.rename_path(black_box(&middle), black_box(&renamed));
                index
            },
            BatchSize::LargeInput,
        );
    });
    let model_renamed = PathBuf::from("/synthetic/project/src/model_renamed");
    group.bench_function(BenchmarkId::new("rename_path_dir", size), |b| {
        b.iter_batched(
            || index.clone(),
            |mut index| {
                index.rename_path(black_box(&model), black_box(&model_renamed));
                index
            },
            BatchSize::LargeInput,
        );
    });
    let cancellation = palette::PaletteSearchCancellation::default();
    group.bench_function(BenchmarkId::new("category_ranking_row_cap", size), |b| {
        b.iter(|| {
            palette::category_search(
                palette::CategorySearchInput {
                    index,
                    open_tabs: &[],
                    note_entries: &[],
                    category: PaletteCategory::WorkspaceFiles,
                    query: black_box("file"),
                    row_limit: category_row_limit(),
                },
                &cancellation,
            )
        });
    });
    group.finish();
}

fn bench_tree_population(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_population");
    group.sample_size(20);
    // Match `scan_directory`'s default lookahead so the benchmark tracks the
    // real sidebar scan contract instead of silently skipping empty-dir probes.
    const BENCH_LOOKAHEAD_CAP: usize = 1000;

    // `full` fits the production budget (one scan pass); `budget_cut` crosses
    // a lowered budget, so the scan falls back to the two-pass top-k
    // (`bound-lists-by-budget-not-count` D6).
    for &(label, entry_count, retained_bytes) in &[
        (
            "full",
            16 * 1024usize,
            lushtext_ui::ui::sidebar::policy::SIDEBAR_DIRECTORY_RETAINED_BYTES,
        ),
        ("budget_cut", 12_000usize, 8 * 1024 * 1024u64),
    ] {
        group.bench_function(BenchmarkId::new("sidebar_scan", label), |b| {
            let dir = make_flat_dir(entry_count);
            b.iter(|| {
                black_box(file_tree::scan_sidebar_directory(
                    dir.path(),
                    lushtext_ui::ui::sidebar::policy::sidebar_scan_budget(retained_bytes),
                    BENCH_LOOKAHEAD_CAP,
                    || false,
                    &WorkspaceEntryVisibility::default(),
                ))
            });
        });
        group.bench_function(BenchmarkId::new("scan_and_splice", label), |b| {
            b.iter_batched(
                || {
                    let dir = make_flat_dir(entry_count);
                    let scan = file_tree::scan_sidebar_directory(
                        dir.path(),
                        lushtext_ui::ui::sidebar::policy::sidebar_scan_budget(retained_bytes),
                        BENCH_LOOKAHEAD_CAP,
                        || false,
                        &WorkspaceEntryVisibility::default(),
                    );
                    (dir, scan)
                },
                |(dir, scan)| {
                    let limited = scan
                        .truncated
                        .then_some((scan.entries.len(), scan.examined_entries));
                    let store = populate_tree_store(scan.entries, limited);
                    (store, dir)
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Remaining quality-gap scale evidence
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
    const NOTE_RENDER_CAP: usize = 512;
    const PREVIEW_BYTES: usize = 4 * 1024 * 1024;
    const CACHE_ROWS: usize = 10_000;
    const MINIMAP_ANALYSIS_SLICE_CHARS: usize = 32 * 1024;
    const MINIMAP_SHORT_LINE_CHARS: usize = 96;
    const MINIMAP_SHORT_LINES: usize = 23_000;

    fn workspace_scan_pressure(
        requests: usize,
    ) -> lushtext_ui::ui::sidebar::policy::WorkspaceScanFlightMetrics {
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
        |index: u32| lushtext_services::notes::bookmark_excerpt::BookmarkExcerptPreviewRequest {
            path: PathBuf::from(format!("/bench/notes/bookmark-{index}.md")),
            line: index,
        };
    let mut preview_coordinator =
        lushtext_services::notes::bookmark_excerpt::BookmarkExcerptPreviewCoordinator::default();
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
            let mut coordinator =
                lushtext_services::notes::bookmark_excerpt::BookmarkExcerptPreviewCoordinator::default();
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

/// Benchmark compact single-flight ownership and deterministic result retirement.
fn bench_search_interactive_policies(c: &mut Criterion) {
    fn rapid_queries(query_count: usize) -> (usize, usize) {
        let mut flight = WorkspaceSearchFlight::default();
        for index in 0..query_count {
            let submission = flight.submit(WorkspaceSearchRequest {
                spec: lushtext_model::workspace::content_search::SearchQuerySpec::new(
                    format!("query-{index}"),
                    ContentSearchOptions::default(),
                ),
                folders: SharedPaths::from([PathBuf::from("/workspace")]),
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
    fn ordinary_burst() -> lushtext_ui::ui::editor_page::save::policy::SaveAdmissionSnapshot {
        let mut policy = SaveAdmissionPolicy::default();
        let weight = SAVE_PAYLOAD_SHARED_BUDGET_BYTES / 8;
        for request_id in 0..8u64 {
            policy.queue(SaveAdmissionRequest {
                request_id,
                owner_id: request_id,
                save_generation: lushtext_model::values::SaveGeneration::new(1),
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
// Text scans (`simd-line-scans`, design D7)
// ---------------------------------------------------------------------------

/// Seeded xorshift generator for the text-scan fixtures, so every side of an
/// interleaved comparison measures the same bytes.
struct TextScanRng(u64);

impl TextScanRng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).expect("pool sizes fit u64");
        usize::try_from(self.next() % bound).expect("a remainder below a usize bound fits usize")
    }
}

const TEXT_SCAN_MIB: usize = 1024 * 1024;
/// The GTK-side minimap analysis turn (`analysis_execution`): 32 Ki characters.
const TEXT_SCAN_MINIMAP_SLICE_CHARS: usize = 32 * 1024;
/// A production-like minimap analysis policy for both minimap cases.
const TEXT_SCAN_MINIMAP_POLICY: MinimapAnalysisPolicy = MinimapAnalysisPolicy {
    warning_line_chars: 120,
    wrapped_line_chars: 8_000,
    marker_limit: 2_000,
};
const TEXT_SCAN_WORDS: &[&str] = &[
    "the",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    "editor",
    "buffer",
    "workspace",
    "a",
    "of",
    "and",
    "responsiveness",
    "line",
    "text",
];

/// Prose wrapped at 80 columns with `\n`.
fn text_scan_ascii(rng: &mut TextScanRng, size: usize) -> String {
    let mut text = String::with_capacity(size + 128);
    let mut column = 0;
    while text.len() < size {
        let word = TEXT_SCAN_WORDS[rng.below(TEXT_SCAN_WORDS.len())];
        if column + word.len() + 1 > 80 {
            text.push('\n');
            column = 0;
        }
        text.push_str(word);
        text.push(' ');
        column += word.len() + 1;
    }
    text
}

/// Two-, three-, and four-byte scalars with rare NBSP and U+200B, 80 per line.
fn text_scan_multibyte(rng: &mut TextScanRng, size: usize) -> String {
    let pool = [
        'é', 'ã', 'ç', 'ü', 'ñ', '日', '本', '語', '中', '文', '😀', '🚀', 'a', 'b', 'e', ' ', 'o',
        's', 't',
    ];
    let mut text = String::with_capacity(size + 128);
    let mut column = 0;
    while text.len() < size {
        let character = match rng.below(10_000) {
            0 => '\u{00A0}',
            1 => '\u{200B}',
            _ => pool[rng.below(pool.len())],
        };
        text.push(character);
        column += 1;
        if column >= 80 {
            text.push('\n');
            column = 0;
        }
    }
    text
}

/// Words and typographic marks (`’`, `—`, `…`): many `0xE2` leads, none a hit.
fn text_scan_typographic(rng: &mut TextScanRng, size: usize) -> String {
    let marks = ['—', '–', '“', '”', '‘', '’', '…'];
    let mut text = String::with_capacity(size + 128);
    let mut column = 0;
    while text.len() < size {
        if rng.below(3) == 0 {
            text.push(marks[rng.below(marks.len())]);
        } else {
            text.push_str(TEXT_SCAN_WORDS[rng.below(TEXT_SCAN_WORDS.len())]);
        }
        column += 1;
        if column >= 16 {
            text.push('\n');
            column = 0;
        }
    }
    text
}

/// Short lines ending in `\r\n`, `\n`, or `\r` at random.
fn text_scan_mixed_eol(rng: &mut TextScanRng, size: usize) -> String {
    let mut text = String::with_capacity(size + 128);
    while text.len() < size {
        for _ in 0..=rng.below(12) {
            text.push_str(TEXT_SCAN_WORDS[rng.below(TEXT_SCAN_WORDS.len())]);
            text.push(' ');
        }
        text.push_str(["\r\n", "\n", "\r"][rng.below(3)]);
    }
    text
}

/// One line of `chars` scalars: mostly ASCII, some `é`, rare U+2060/U+FEFF.
fn text_scan_long_line(rng: &mut TextScanRng, chars: usize) -> String {
    let mut text = String::with_capacity(chars * 2);
    for _ in 0..chars {
        let character = match rng.below(1000) {
            0 => '\u{2060}',
            1 => '\u{FEFF}',
            2..=60 => 'é',
            _ => char::from(b'a' + u8::try_from(rng.below(26)).expect("below 26 fits u8")),
        };
        text.push(character);
    }
    text
}

fn text_scan_minimap(text: &str) -> MinimapAnalysisResult {
    let mut analysis = MinimapAnalysisAccumulator::new(TEXT_SCAN_MINIMAP_POLICY, true);
    let mut characters = text.chars();
    while analysis.inspect_slice(characters.by_ref(), TEXT_SCAN_MINIMAP_SLICE_CHARS) > 0 {}
    analysis.finish()
}

/// [`text_scan_minimap`] through the slice entry point, in calls of at most
/// 128 KiB (32 Ki four-byte characters, one GTK turn's worth), cut on scalar
/// boundaries by byte offset so the cutting itself costs no character walk.
fn text_scan_minimap_str(text: &str) -> MinimapAnalysisResult {
    let mut analysis = MinimapAnalysisAccumulator::new(TEXT_SCAN_MINIMAP_POLICY, true);
    let mut rest = text;
    while !rest.is_empty() {
        let end = rest.floor_char_boundary(4 * TEXT_SCAN_MINIMAP_SLICE_CHARS);
        analysis.inspect_str(&rest[..end]);
        rest = &rest[end..];
    }
    analysis.finish()
}

struct MinimapTextScanFixtures {
    many_short_lines: String,
    long_line: String,
}

static MINIMAP_TEXT_SCAN_FIXTURES: std::sync::LazyLock<MinimapTextScanFixtures> =
    std::sync::LazyLock::new(|| {
        let mut rng = TextScanRng(42);
        let many_short_lines = text_scan_ascii(&mut rng, 50 * TEXT_SCAN_MIB);
        let _ = text_scan_multibyte(&mut rng, 50 * TEXT_SCAN_MIB);
        let _ = text_scan_typographic(&mut rng, 50 * TEXT_SCAN_MIB);
        let _ = text_scan_mixed_eol(&mut rng, 50 * TEXT_SCAN_MIB);
        let long_line = text_scan_long_line(&mut rng, 1_900_000);
        MinimapTextScanFixtures {
            many_short_lines,
            long_line,
        }
    });

fn bench_text_scans_minimap(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_scans");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    for (label, text) in [
        (
            "many_short_lines",
            &MINIMAP_TEXT_SCAN_FIXTURES.many_short_lines,
        ),
        ("long_line", &MINIMAP_TEXT_SCAN_FIXTURES.long_line),
    ] {
        group.bench_function(format!("minimap/{label}"), |b| {
            b.iter(|| text_scan_minimap(black_box(text)));
        });
        group.bench_function(format!("minimap/{label}_str"), |b| {
            b.iter(|| text_scan_minimap_str(black_box(text)));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_all_tabs_filter,
    bench_palette_categories,
    bench_file_index_scale,
    bench_tree_population,
    bench_quality_gap_scale,
    bench_search_interactive_policies,
    bench_save_admission_policy,
    bench_text_scans_minimap,
);
criterion_main!(benches);

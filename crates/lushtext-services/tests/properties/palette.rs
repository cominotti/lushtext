// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for command-palette merge ordering.
//!
//! The generated inputs model two already-scored result streams so the test can
//! focus on max truncation, descending score order, and left-side tie priority.

use std::path::PathBuf;
use std::sync::Arc;

use lushtext_model::workspace::palette::{
    CommandCategory, CommandDef, PaletteFileEntry, PaletteFileIdentity, PaletteNoteCategory,
    PaletteNoteEntry, PaletteNoteTarget, PaletteSearchRow, ScoredResult, SearchMode,
    SearchResultItem,
};
use lushtext_model::workspace::{WorkspaceScope, WorkspacesFile};
use lushtext_services::palette::{
    FileIndex, GroupedSearchInput, NoteSourceRefreshCoordinator, NoteSourceRefreshRequest,
    NotesBrowserMode, NotesBrowserQueryCoordinator, NotesBrowserQueryRequest, OwnedIndexedFile,
    PaletteSearchCancellation, PaletteSearchOutcome, grouped_search,
    merge_sorted_for_property_test, note_scoring_equivalence_for_property_test,
    open_file_selection_equivalence_for_property_test,
};
use proptest::prelude::*;

use crate::support;

/// Synthetic left-side command used to identify merge tie precedence.
static LEFT_COMMAND: CommandDef = CommandDef {
    id: "property.left",
    label: "Left Property Command",
    category: CommandCategory::App,
    shortcut: None,
};
/// Synthetic right-side command used to identify merge tie precedence.
static RIGHT_COMMAND: CommandDef = CommandDef {
    id: "property.right",
    label: "Right Property Command",
    category: CommandCategory::App,
    shortcut: None,
};

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn merge_preserves_order_limit_and_left_ties(
        left_scores in score_stream(),
        right_scores in score_stream(),
        max in 0usize..=support::MAX_VECTOR_LEN * 2,
    ) {
        let left_scores = sort_descending(left_scores);
        let right_scores = sort_descending(right_scores);
        let left = scored_results(&LEFT_COMMAND, &left_scores);
        let right = scored_results(&RIGHT_COMMAND, &right_scores);
        let expected = expected_merge(&left_scores, &right_scores, max);

        let actual = merge_sorted_for_property_test(left, right, max);
        let actual_pairs = result_pairs(&actual);

        prop_assert!(actual.len() <= max);
        prop_assert_eq!(actual_pairs, expected);
        prop_assert!(actual.windows(2).all(|window| window[0].score >= window[1].score));
    }

    #[test]
    fn bounded_palette_selection_matches_full_sort_across_generated_corpora(
        names in candidate_names(),
        query in palette_query(),
        max in 0usize..=support::MAX_VECTOR_LEN * 2,
    ) {
        let files = names
            .into_iter()
            .enumerate()
            .map(|(ordinal, display_name)| {
                let path = PathBuf::from(format!("/workspace/{ordinal}"));
                PaletteFileEntry::new(
                    display_name,
                    format!("/workspace/{ordinal}"),
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                )
            })
            .collect::<Vec<_>>();

        let (bounded, reference, metrics) =
            open_file_selection_equivalence_for_property_test(&files, &query, max);

        prop_assert_eq!(&bounded, &reference);
        prop_assert!(bounded.len() <= max);
        prop_assert!(metrics.peak_retained_per_source <= max);
        if query.is_empty() {
            prop_assert_eq!(metrics.candidates_examined, files.len().min(max));
            prop_assert!(bounded
                .iter()
                .enumerate()
                .all(|(position, (_, score, ordinal))| *score == 0 && *ordinal == position));
        }
    }

    #[test]
    fn note_source_supersession_retains_one_active_and_one_latest_request(
        request_count in 1usize..=support::MAX_VECTOR_LEN * 4,
    ) {
        let scope = WorkspacesFile {
            current_scope: WorkspaceScope::All,
            workspaces: Vec::new(),
        }
        .current_scope_snapshot();
        let mut coordinator = NoteSourceRefreshCoordinator::default();
        let mut active_generation = None;

        for index in 0..request_count {
            let start = coordinator.submit(NoteSourceRefreshRequest {
                data_dir: PathBuf::from(format!("request-{index}")),
                scope_snapshot: scope.clone(),
                open_editor_snapshots: Arc::from([]),
                open_editor_snapshots_truncated: false,
                open_editor_rows_not_retained: 0,
                mode: NotesBrowserMode::AllNotes,
                limits: lushtext_services::palette::PALETTE_NOTE_SOURCE_LIMITS,
            });
            if let Some(start) = start {
                active_generation = Some(start.generation);
            }
            let snapshot = coordinator.snapshot();
            prop_assert!(snapshot.active <= 1);
            prop_assert!(snapshot.pending <= 1);
        }

        let active_generation = active_generation.expect("the first request starts");
        let next = coordinator.finish(active_generation);
        if request_count == 1 {
            prop_assert!(next.is_none());
        } else {
            let next = next.expect("the latest pending request starts");
            prop_assert_eq!(
                next.request.data_dir,
                PathBuf::from(format!("request-{}", request_count - 1))
            );
            prop_assert!(coordinator.is_current(next.generation));
        }
    }

    #[test]
    fn notes_browser_query_supersession_retains_only_the_latest_compact_request(
        request_count in 1usize..=support::MAX_VECTOR_LEN * 4,
    ) {
        let mut coordinator = NotesBrowserQueryCoordinator::default();
        let mut active_generation = None;
        for index in 0..request_count {
            let start = coordinator.submit(NotesBrowserQueryRequest {
                query: format!("query-{index}"),
                mode: NotesBrowserMode::AllNotes,
            });
            if let Some(start) = start {
                active_generation = Some(start.generation);
            }
            let snapshot = coordinator.snapshot();
            prop_assert!(snapshot.active <= 1);
            prop_assert!(snapshot.pending <= 1);
        }

        let active_generation = active_generation.expect("the first request starts");
        let next = coordinator.finish(active_generation);
        if request_count == 1 {
            prop_assert!(next.is_none());
        } else {
            prop_assert_eq!(
                next.expect("latest query starts").request.query,
                format!("query-{}", request_count - 1)
            );
        }
    }

    #[test]
    fn optimized_note_scoring_matches_unpruned_reference_after_rapid_supersession(
        cases in note_scoring_cases(),
        queries in note_query_sequence(),
        max in 0usize..=support::MAX_VECTOR_LEN * 2,
        category_selector in 0usize..=PaletteNoteCategory::ALL.len(),
    ) {
        let entries = generated_note_entries(cases);
        let category = (category_selector < PaletteNoteCategory::ALL.len())
            .then(|| PaletteNoteCategory::ALL[category_selector]);
        let mut coordinator = NotesBrowserQueryCoordinator::default();
        let mut first_start = None;
        for query in &queries {
            if let Some(start) = coordinator.submit(NotesBrowserQueryRequest {
                query: query.clone(),
                mode: NotesBrowserMode::AllNotes,
            }) {
                first_start = Some(start);
            }
            let snapshot = coordinator.snapshot();
            prop_assert!(snapshot.active <= 1);
            prop_assert!(snapshot.pending <= 1);
        }

        let first = first_start.expect("non-empty query sequence starts one generation");
        let final_request = if queries.len() == 1 {
            first.request
        } else {
            coordinator
                .finish(first.generation)
                .expect("latest superseding query starts")
                .request
        };
        prop_assert_eq!(&final_request.query, queries.last().expect("non-empty queries"));

        let evidence = note_scoring_equivalence_for_property_test(
            &entries,
            category,
            &final_request.query,
            max,
        );
        prop_assert_eq!(&evidence.optimized, &evidence.unpruned_reference);
        prop_assert!(evidence.optimized.len() <= max);
        prop_assert!(evidence.optimized_metrics.peak_retained_per_source <= max);
        if !final_request.query.trim().is_empty() {
            prop_assert_eq!(
                evidence.optimized_metrics.note_bodies_examined
                    + evidence.optimized_metrics.note_bodies_safely_pruned,
                evidence.optimized_metrics.candidates_scored,
            );
        }
    }

    #[test]
    fn mixed_all_mode_groups_stay_bounded_ordered_and_deduplicated(
        open_count in 0usize..=support::MAX_VECTOR_LEN,
        workspace_count in 0usize..=support::MAX_VECTOR_LEN,
        note_count in 0usize..=support::MAX_VECTOR_LEN,
        duplicate_first in any::<bool>(),
        max in 0usize..=support::MAX_VECTOR_LEN,
    ) {
        let root = lushtext_model::values::SharedPath::from("/workspace");
        let open_tabs = (0..open_count)
            .map(|index| {
                let path = root.join(format!("open-{index}.rs"));
                PaletteFileEntry::new(
                    format!("open-{index}.rs"),
                    path.display().to_string(),
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                )
            })
            .collect::<Vec<_>>();
        let indexed = (0..workspace_count)
            .map(|index| {
                let path = if duplicate_first && index == 0 && !open_tabs.is_empty() {
                    open_tabs[0].path.clone()
                } else {
                    root.join(format!("workspace-{index}.rs"))
                };
                OwnedIndexedFile::new(
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                    root.clone(),
                )
            })
            .collect::<Vec<_>>();
        let notes = (0..note_count)
            .map(|index| PaletteNoteEntry {
                category: PaletteNoteCategory::ALL[index % PaletteNoteCategory::ALL.len()],
                title: format!("note-{index}"),
                subtitle: "property note".to_string(),
                detail: None,
                note_text: Some(format!("body-{index}")),
                target: PaletteNoteTarget::DocumentNote {
                    path: root.join(format!("note-{index}.rs")),
                    workspace_folders: vec![root.to_path_buf()],
                },
            })
            .collect::<Vec<_>>();
        let cancellation = PaletteSearchCancellation::default();
        let index = FileIndex::from(indexed);

        let outcome = grouped_search(
            GroupedSearchInput {
                index: &index,
                open_tabs: &open_tabs,
                note_entries: &notes,
                workspace_group_label: "All Workspaces",
                query: "",
                mode: SearchMode::All,
                max_per_source: max,
            },
            &cancellation,
        );
        let PaletteSearchOutcome::Complete { value: rows, metrics } = outcome else {
            prop_assert!(false, "fresh grouped search must complete");
            unreachable!();
        };

        prop_assert!(metrics.peak_retained_per_source <= max);
        let headers = rows
            .iter()
            .filter_map(|row| match row {
                PaletteSearchRow::Header { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected_order = [
            "Open Tabs",
            "All Workspaces",
            "Bookmarks",
            "Folder Notes",
            "Document Notes",
            "Open Tab Notes",
            "Commands",
        ];
        let positions = headers
            .iter()
            .map(|header| {
                expected_order
                    .iter()
                    .position(|expected| expected == header)
                    .expect("grouped search returned an unexpected header")
            })
            .collect::<Vec<_>>();
        prop_assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));

        let file_paths = rows
            .iter()
            .filter_map(|row| match row {
                PaletteSearchRow::File { file_path, .. } => Some(file_path),
                _ => None,
            })
            .collect::<Vec<_>>();
        let unique_paths = file_paths.iter().copied().collect::<std::collections::HashSet<_>>();
        prop_assert_eq!(file_paths.len(), unique_paths.len());
    }
}

fn candidate_names() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop::sample::select(vec![
            "same".to_string(),
            "main.rs".to_string(),
            "mañana.rs".to_string(),
            "re\u{301}sume\u{301}.md".to_string(),
            "résumé.md".to_string(),
            "東京-notes.txt".to_string(),
            "emoji-🌍.md".to_string(),
            "alpha_beta.rs".to_string(),
            "ALPHA-beta.rs".to_string(),
        ]),
        0..=support::MAX_VECTOR_LEN,
    )
}

fn palette_query() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        String::new(),
        "a".to_string(),
        "same".to_string(),
        "résumé".to_string(),
        "re\u{301}sume\u{301}".to_string(),
        "東京".to_string(),
        "🌍".to_string(),
        "ab".to_string(),
        "missing".to_string(),
    ])
}

fn note_query_sequence() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(palette_query(), 1..=support::MAX_VECTOR_LEN.min(8))
}

fn note_scoring_cases() -> impl Strategy<Value = Vec<(usize, String, String, bool)>> {
    let token = prop::sample::select(vec![
        String::new(),
        "alpha".to_string(),
        "same".to_string(),
        "résumé".to_string(),
        "re\u{301}sume\u{301}".to_string(),
        "東京".to_string(),
        "🌍".to_string(),
        "missing".to_string(),
    ]);
    prop::collection::vec(
        (
            0usize..PaletteNoteCategory::ALL.len(),
            token.clone(),
            token,
            any::<bool>(),
        ),
        0..=support::MAX_VECTOR_LEN,
    )
}

fn generated_note_entries(cases: Vec<(usize, String, String, bool)>) -> Vec<PaletteNoteEntry> {
    cases
        .into_iter()
        .enumerate()
        .map(|(ordinal, (category, metadata, body_token, long_body))| {
            let body = if long_body {
                format!("{} {body_token}", "x".repeat(4 * 1024 + 1))
            } else {
                format!("body {body_token}")
            };
            PaletteNoteEntry {
                category: PaletteNoteCategory::ALL[category],
                // Deliberately omit the ordinal so generated duplicates exercise ties.
                title: format!("note {metadata}"),
                subtitle: "property metadata · Café · 東京".to_string(),
                detail: (ordinal % 2 == 0).then(|| format!("detail {metadata}")),
                note_text: Some(body),
                target: PaletteNoteTarget::DocumentNote {
                    path: PathBuf::from(format!("/workspace/property-note-{ordinal}.md")),
                    workspace_folders: vec![PathBuf::from("/workspace")],
                },
            }
        })
        .collect()
}

/// Generate a bounded stream of relevance scores.
fn score_stream() -> impl Strategy<Value = Vec<u32>> {
    prop::collection::vec(0u32..=10_000, 0..=support::MAX_VECTOR_LEN)
}

/// Sort generated scores into the precondition expected by the merge helper.
fn sort_descending(mut scores: Vec<u32>) -> Vec<u32> {
    scores.sort_unstable_by(|left, right| right.cmp(left));
    scores
}

/// Wrap generated scores with a synthetic command identity.
fn scored_results<'a>(command: &'a CommandDef, scores: &[u32]) -> Vec<ScoredResult<'a>> {
    scores
        .iter()
        .copied()
        .map(|score| ScoredResult {
            item: SearchResultItem::Command(command),
            score,
            source_ordinal: 0,
        })
        .collect()
}

/// Independently model the merge policy, including left-side tie priority.
fn expected_merge(
    left_scores: &[u32],
    right_scores: &[u32],
    max: usize,
) -> Vec<(&'static str, u32)> {
    let mut expected = Vec::new();
    let mut left_index = 0usize;
    let mut right_index = 0usize;

    while expected.len() < max {
        match (
            left_scores.get(left_index).copied(),
            right_scores.get(right_index).copied(),
        ) {
            (Some(left), Some(right)) if left >= right => {
                expected.push((LEFT_COMMAND.id, left));
                left_index += 1;
            }
            (Some(_) | None, Some(right)) => {
                expected.push((RIGHT_COMMAND.id, right));
                right_index += 1;
            }
            (Some(left), None) => {
                expected.push((LEFT_COMMAND.id, left));
                left_index += 1;
            }
            (None, None) => break,
        }
    }

    expected
}

/// Convert palette results into simple source-and-score pairs for assertions.
fn result_pairs(results: &[ScoredResult<'_>]) -> Vec<(&'static str, u32)> {
    results
        .iter()
        .map(|result| match result.item {
            SearchResultItem::Command(command) => (command.id, result.score),
            SearchResultItem::OpenFile(_)
            | SearchResultItem::File(_)
            | SearchResultItem::Note(_) => ("unexpected.palette.result.kind", result.score),
        })
        .collect()
}

// --- Arena and per-file reference parity (`scale-file-index` D6) ----------

mod index_parity {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    use lushtext_model::workspace::palette::{PaletteFileIdentity, ScoredResult, SearchResultItem};
    use lushtext_model::workspace::workspace_visibility::WorkspaceEntryVisibility;
    use lushtext_services::filesystem::fixture;
    use lushtext_services::palette::reference::{ReferenceBuildOutcome, ReferenceFileIndex};
    use lushtext_services::palette::{
        FileIndex, FileIndexBuildOutcome, FileIndexTruncationReason,
        MAX_FILE_INDEX_BUILD_RETAINED_BYTES, MAX_FILE_INDEX_RETAINED_BYTES,
        MAX_INDEXED_DIRECTORIES, MAX_INDEXED_FILES, PaletteSearchCancellation,
        PaletteSearchOutcome,
    };
    use proptest::prelude::*;

    use crate::support;

    const DIR_NAMES: [&str; 4] = ["alpha", "beta", "gamma", "delta"];
    const FILE_NAMES: [&str; 6] = [
        "one.rs",
        "two.md",
        "three.toml",
        "four.rs",
        "five.txt",
        "six.rs",
    ];
    const QUERIES: [&str; 5] = ["", "rs", "on", "fv", "zz"];

    /// One generated tree: directories as name-index chains, files as
    /// `(directory, name)` pairs (directory 0 is the root).
    #[derive(Clone, Debug)]
    struct TreeSpec {
        dirs: Vec<Vec<usize>>,
        files: Vec<(usize, usize)>,
        /// A symlinked file to the n-th file, named to sort first or last.
        file_link: Option<(usize, bool)>,
        /// A symlinked directory aliasing the n-th directory.
        dir_link: Option<usize>,
        non_utf8: bool,
        /// A second folder (a nested directory), listed before or after.
        overlap: Option<(usize, bool)>,
        mutations: Vec<Mutation>,
    }

    #[derive(Clone, Debug)]
    enum Mutation {
        Create {
            dir: usize,
            name: usize,
        },
        RemoveFile(usize),
        RemoveDir(usize),
        RenameFile {
            file: usize,
            dir: usize,
            name: usize,
        },
        RenameDir {
            dir: usize,
            name: usize,
        },
    }

    fn tree_spec() -> impl Strategy<Value = TreeSpec> {
        let dirs = prop::collection::vec(prop::collection::vec(0usize..4, 1..=3), 0..=5);
        let files = prop::collection::vec((0usize..6, 0usize..6), 0..=10);
        let mutation =
            prop_oneof![
                (0usize..6, 0usize..6).prop_map(|(dir, name)| Mutation::Create { dir, name }),
                (0usize..10).prop_map(Mutation::RemoveFile),
                (0usize..6).prop_map(Mutation::RemoveDir),
                (0usize..10, 0usize..6, 0usize..6)
                    .prop_map(|(file, dir, name)| Mutation::RenameFile { file, dir, name }),
                (0usize..6, 0usize..4).prop_map(|(dir, name)| Mutation::RenameDir { dir, name }),
            ];
        (
            dirs,
            files,
            proptest::option::of((0usize..10, any::<bool>())),
            proptest::option::of(0usize..6),
            any::<bool>(),
            proptest::option::of((0usize..6, any::<bool>())),
            prop::collection::vec(mutation, 0..=4),
        )
            .prop_map(
                |(dirs, files, file_link, dir_link, non_utf8, overlap, mutations)| TreeSpec {
                    dirs,
                    files,
                    file_link,
                    dir_link,
                    non_utf8,
                    overlap,
                    mutations,
                },
            )
    }

    /// The tree on disk plus the real (non-alias) paths the test created.
    struct Tree {
        _dir: tempfile::TempDir,
        root: PathBuf,
        folders: Vec<PathBuf>,
        dirs: Vec<PathBuf>,
        files: Vec<PathBuf>,
    }

    fn write_tree(spec: &TreeSpec) -> Tree {
        let dir = tempfile::TempDir::new().expect("temp directory");
        let root = dir.path().join("ws");
        fixture::create_dir_all(&root);
        let mut dirs = vec![root.clone()];
        for chain in &spec.dirs {
            let path = chain
                .iter()
                .fold(root.clone(), |path, name| path.join(DIR_NAMES[*name]));
            fixture::create_dir_all(&path);
            if !dirs.contains(&path) {
                dirs.push(path);
            }
        }
        // Every ancestor created above is a directory too.
        let mut all_dirs = Vec::new();
        for path in &dirs {
            for ancestor in path.ancestors() {
                if ancestor.starts_with(&root)
                    && !all_dirs.iter().any(|known: &PathBuf| known == ancestor)
                {
                    all_dirs.push(ancestor.to_path_buf());
                }
            }
        }
        all_dirs.sort();
        let mut files = Vec::new();
        for (dir_index, name) in &spec.files {
            let path = all_dirs[dir_index % all_dirs.len()].join(FILE_NAMES[*name]);
            if !files.contains(&path) {
                fixture::write_text(&path, "");
                files.push(path);
            }
        }
        if let Some((target, early)) = spec.file_link
            && !files.is_empty()
        {
            let target = files[target % files.len()].clone();
            let link = root.join(if early { "aa_link.rs" } else { "zz_link.rs" });
            fixture::symlink(&target, &link);
        }
        if let Some(alias) = spec.dir_link
            && all_dirs.len() > 1
        {
            let target = all_dirs[1 + alias % (all_dirs.len() - 1)].clone();
            fixture::symlink(&target, &root.join("linked"));
        }
        #[cfg(unix)]
        if spec.non_utf8 {
            use std::os::unix::ffi::OsStrExt;
            for bytes in [&b"caf\xe9.rs"[..], &b"caf\xea.rs"[..]] {
                let path = root.join(std::ffi::OsStr::from_bytes(bytes));
                fixture::write_text(&path, "");
                files.push(path);
            }
        }
        let mut folders = vec![root.clone()];
        if let Some((nested, before)) = spec.overlap
            && all_dirs.len() > 1
        {
            let nested = all_dirs[1 + nested % (all_dirs.len() - 1)].clone();
            if before {
                folders.insert(0, nested);
            } else {
                folders.push(nested);
            }
        }
        Tree {
            _dir: dir,
            root,
            folders,
            dirs: all_dirs,
            files,
        }
    }

    type Row = (PathBuf, String, String, PaletteFileIdentity, PathBuf);

    fn arena_rows(index: &FileIndex) -> Vec<Row> {
        index
            .files()
            .map(|file| {
                (
                    file.path(),
                    file.relative_display(),
                    file.name().to_string(),
                    file.identity(),
                    file.workspace_folder().to_path_buf(),
                )
            })
            .collect()
    }

    fn reference_rows(index: &ReferenceFileIndex) -> Vec<Row> {
        index
            .files()
            .iter()
            .map(|file| {
                (
                    file.path.clone(),
                    file.relative_display(),
                    file.name.clone(),
                    file.identity.clone(),
                    file.workspace_folder.to_path_buf(),
                )
            })
            .collect()
    }

    fn ranked(outcome: PaletteSearchOutcome<Vec<ScoredResult<'_>>>) -> (Vec<(u32, u32)>, usize) {
        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("a fresh token cannot cancel");
        };
        let rows = value
            .iter()
            .map(|result| match result.item {
                SearchResultItem::File(ordinal) => (ordinal.0, result.score),
                _ => (u32::MAX, result.score),
            })
            .collect();
        (rows, metrics.matching_candidates)
    }

    fn build_arena(
        folders: &[PathBuf],
        files: usize,
        directories: usize,
        build_bytes: u64,
    ) -> (FileIndex, Option<FileIndexTruncationReason>, u64, u64) {
        match FileIndex::rebuild_cancellable_with_limits(
            folders,
            files,
            directories,
            build_bytes,
            MAX_FILE_INDEX_RETAINED_BYTES,
            &PaletteSearchCancellation::default(),
        ) {
            FileIndexBuildOutcome::Complete { index, metrics } => (
                index,
                metrics.truncation,
                metrics.peak_build_bytes,
                metrics.retained_index_bytes,
            ),
            FileIndexBuildOutcome::Cancelled { .. } => panic!("a fresh token cannot cancel"),
        }
    }

    fn build_reference(
        folders: &[PathBuf],
        files: usize,
        directories: usize,
    ) -> (ReferenceFileIndex, Option<FileIndexTruncationReason>) {
        match ReferenceFileIndex::rebuild_cancellable_with_limits(
            folders,
            0,
            files,
            directories,
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            &PaletteSearchCancellation::default(),
            &WorkspaceEntryVisibility::default(),
        ) {
            ReferenceBuildOutcome::Complete { index, metrics } => (index, metrics.truncation),
            ReferenceBuildOutcome::Cancelled { .. } => panic!("a fresh token cannot cancel"),
        }
    }

    fn assert_same(arena: &FileIndex, reference: &ReferenceFileIndex) -> Result<(), TestCaseError> {
        prop_assert_eq!(arena_rows(arena), reference_rows(reference));
        let cancellation = PaletteSearchCancellation::default();
        for query in QUERIES {
            prop_assert_eq!(
                ranked(arena.search_cancellable(query, 8, &cancellation)),
                ranked(reference.search_cancellable(query, 8, &cancellation)),
                "query {:?}",
                query
            );
        }
        // Open tabs: every other canonical identity the reference holds.
        let open: HashSet<PathBuf> = reference
            .files()
            .iter()
            .filter_map(|file| file.identity.canonical_path().map(Path::to_path_buf))
            .step_by(2)
            .collect();
        for query in ["", "rs"] {
            for max in [1, 3, 64] {
                prop_assert_eq!(
                    ranked(arena.search_cancellable_excluding(query, max, &open, &cancellation)),
                    ranked(reference.search_cancellable_excluding(
                        query,
                        max,
                        &open,
                        &cancellation
                    )),
                    "excluding, query {:?}, max {}",
                    query,
                    max
                );
            }
        }
        Ok(())
    }

    /// Apply one mutation on disk first, then to both indexes.
    fn mutate(
        tree: &mut Tree,
        mutation: &Mutation,
        arena: &mut FileIndex,
        reference: &mut ReferenceFileIndex,
    ) {
        match mutation {
            Mutation::Create { dir, name } => {
                let path =
                    tree.dirs[dir % tree.dirs.len()].join(format!("new_{}", FILE_NAMES[*name]));
                if fixture::exists(&path) {
                    return;
                }
                fixture::write_text(&path, "");
                tree.files.push(path.clone());
                if let Some(folder) = reference.workspace_folder_for(&path) {
                    arena.add_path(&path, &folder);
                    reference.add_path(path, folder);
                }
            }
            Mutation::RemoveFile(file) => {
                if tree.files.is_empty() {
                    return;
                }
                let path = tree.files.remove(file % tree.files.len());
                fixture::remove_file(&path);
                arena.remove_path(&path);
                reference.remove_path(&path);
            }
            Mutation::RemoveDir(dir) => {
                if tree.dirs.len() < 2 {
                    return;
                }
                let path = tree.dirs[1 + dir % (tree.dirs.len() - 1)].clone();
                fixture::remove_dir_all(&path);
                tree.dirs.retain(|known| !known.starts_with(&path));
                tree.files.retain(|known| !known.starts_with(&path));
                arena.remove_path(&path);
                reference.remove_path(&path);
            }
            Mutation::RenameFile { file, dir, name } => {
                if tree.files.is_empty() {
                    return;
                }
                let index = file % tree.files.len();
                let old = tree.files[index].clone();
                let new =
                    tree.dirs[dir % tree.dirs.len()].join(format!("moved_{}", FILE_NAMES[*name]));
                if fixture::exists(&new) {
                    return;
                }
                fixture::rename(&old, &new);
                tree.files[index] = new.clone();
                arena.rename_path(&old, &new);
                reference.rename_path(&old, &new);
            }
            Mutation::RenameDir { dir, name } => {
                if tree.dirs.len() < 2 {
                    return;
                }
                let old = tree.dirs[1 + dir % (tree.dirs.len() - 1)].clone();
                let new = old.with_file_name(format!("renamed_{}", DIR_NAMES[*name]));
                if fixture::exists(&new) {
                    return;
                }
                fixture::rename(&old, &new);
                let rebase = |path: &PathBuf| {
                    path.strip_prefix(&old)
                        .map_or_else(|_| path.clone(), |suffix| new.join(suffix))
                };
                tree.dirs = tree.dirs.iter().map(rebase).collect();
                tree.files = tree.files.iter().map(rebase).collect();
                arena.rename_path(&old, &new);
                reference.rename_path(&old, &new);
            }
        }
    }

    proptest! {
        #![proptest_config(support::property_config())]

        /// The arena and the per-file reference agree on the file list, its
        /// order, identities, folders, query results and empty-query counts,
        /// after the build and after every mutation.
        #[test]
        fn arena_matches_the_per_file_reference(spec in tree_spec()) {
            let mut tree = write_tree(&spec);
            let (mut arena, arena_truncation, _, _) = build_arena(
                &tree.folders,
                MAX_INDEXED_FILES,
                MAX_INDEXED_DIRECTORIES,
                MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            );
            let (mut reference, reference_truncation) =
                build_reference(&tree.folders, MAX_INDEXED_FILES, MAX_INDEXED_DIRECTORIES);
            prop_assert_eq!(arena_truncation, reference_truncation);
            assert_same(&arena, &reference)?;
            for mutation in &spec.mutations {
                mutate(&mut tree, mutation, &mut arena, &mut reference);
                assert_same(&arena, &reference)?;
            }
            prop_assert!(fixture::exists(&tree.root));
        }

        /// Under scaled count limits both stop at the same file with the same
        /// reason; under scaled byte limits the arena's list is a prefix of
        /// the reference's untruncated list, with a byte reason when cut.
        #[test]
        fn truncation_matches_the_reference(
            spec in tree_spec(),
            files in 1usize..=12,
            directories in 1usize..=6,
            build_bytes in 256u64..=16 * 1024,
        ) {
            let tree = write_tree(&spec);
            let (arena, arena_truncation, _, _) =
                build_arena(&tree.folders, files, directories, MAX_FILE_INDEX_BUILD_RETAINED_BYTES);
            let (reference, reference_truncation) =
                build_reference(&tree.folders, files, directories);
            prop_assert_eq!(arena_truncation, reference_truncation);
            prop_assert_eq!(arena_rows(&arena), reference_rows(&reference));

            let (limited, reason, peak, retained) = build_arena(
                &tree.folders,
                MAX_INDEXED_FILES,
                MAX_INDEXED_DIRECTORIES,
                build_bytes,
            );
            let (full, _) =
                build_reference(&tree.folders, MAX_INDEXED_FILES, MAX_INDEXED_DIRECTORIES);
            let limited_rows = arena_rows(&limited);
            let full_rows = reference_rows(&full);
            prop_assert!(limited_rows.len() <= full_rows.len());
            prop_assert_eq!(&limited_rows[..], &full_rows[..limited_rows.len()]);
            if limited_rows.len() < full_rows.len() {
                prop_assert!(matches!(
                    reason,
                    Some(
                        FileIndexTruncationReason::BuildByteLimit
                            | FileIndexTruncationReason::RetainedByteLimit
                    )
                ));
            }
            prop_assert!(peak <= build_bytes);
            prop_assert!(retained <= MAX_FILE_INDEX_RETAINED_BYTES);
        }
    }
}

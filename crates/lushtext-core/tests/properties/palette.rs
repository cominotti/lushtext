// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for command-palette merge ordering.
//!
//! The generated inputs model two already-scored result streams so the test can
//! focus on max truncation, descending score order, and left-side tie priority.

use std::path::PathBuf;
use std::sync::Arc;

use lushtext_core::model::palette::{
    CommandCategory, CommandDef, IndexedFile, PaletteFileEntry, PaletteFileIdentity,
    PaletteNoteCategory, PaletteNoteEntry, PaletteNoteTarget, PaletteSearchRow, ScoredResult,
    SearchMode, SearchResultItem,
};
use lushtext_core::model::workspace::{WorkspaceScope, WorkspacesFile};
use lushtext_core::services::palette::{
    FileIndex, GroupedSearchInput, NoteSourceRefreshCoordinator, NoteSourceRefreshRequest,
    NotesBrowserMode, NotesBrowserQueryCoordinator, NotesBrowserQueryRequest,
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
                mode: NotesBrowserMode::AllNotes,
                limits: lushtext_core::services::palette::PALETTE_NOTE_SOURCE_LIMITS,
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
        let root = Arc::new(PathBuf::from("/workspace"));
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
                IndexedFile::new(
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                    Arc::clone(&root),
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
                    workspace_folders: vec![root.as_ref().clone()],
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
                PaletteSearchRow::Header { label } => Some(label.as_str()),
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

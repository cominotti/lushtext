// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for search/replace persistence safety.
//!
//! These tests stay GTK-free while exercising the same app-data journal files
//! the search panel uses to keep Replace All undo state crash-aware.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use lushtext_core::model::content_search::{
    ContentSearchOptions, ReplacePreviewSkipReason, Replacement, SearchMatch, SearchMatchId,
    generate_replacement_preview,
};
use lushtext_core::services::{
    content_search::{self, ReplaceUndoBackup, ReplaceUndoEntry},
    filesystem::{fixture, metadata as fs_metadata},
    search_backup,
};

use crate::common::TestContext;

const JOURNAL_DIR: &str = "replace-backup-journal";
const CLEANUP_MARKER_FILE: &str = "cleanup-in-progress.json";

#[test]
fn pre_rename_failure_reclaims_one_entry_budget_for_later_sorted_target() {
    let ctx = TestContext::new();
    let first = ctx.write_file("workspace/a-first.txt", "needle\n");
    let later = ctx.write_file("workspace/b-later.txt", "needle\n");
    let replacements = vec![
        replacement_for(&first, SearchMatchId::from_index(0)),
        replacement_for(&later, SearchMatchId::from_index(1)),
    ];
    let one_entry =
        u64::try_from("needle\n".len() + "done\n".len()).expect("fixture payload should fit u64");
    content_search::set_max_replace_undo_bytes_for_test(Some(one_entry));
    content_search::fail_next_replace_before_rename_for_path_for_test(&first);

    let outcome = content_search::apply_replacements(
        &replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        Some(ctx.data_dir()),
    )
    .expect("later target should use the reclaimed one-entry budget");
    content_search::set_max_replace_undo_bytes_for_test(None);

    assert_eq!(fixture::read_text(&first), "needle\n");
    assert_eq!(fixture::read_text(&later), "done\n");
    assert_eq!(outcome.result.files_affected, 1);
    assert_eq!(outcome.result.error_count, 1);
    assert_eq!(outcome.metrics().undo_live_bytes, one_entry);
    assert_eq!(outcome.metrics().undo_bytes, one_entry);
    assert!(!outcome.undo_backup.contains_key(&first));
    assert!(outcome.undo_backup.contains_key(&later));
    let persisted = search_backup::load(ctx.data_dir()).expect("load incremental journal");
    assert!(!persisted.contains_key(&first));
    assert!(persisted.contains_key(&later));
}

#[test]
fn caller_skip_paths_preserve_open_tab_bytes_and_report_the_skip() {
    let ctx = TestContext::new();
    let path = ctx.write_file("workspace/modified-open-tab.txt", "needle\n");
    let replacements = [replacement_for(&path, SearchMatchId::from_index(0))];
    let skip_paths = HashSet::from([path.clone()]);

    let outcome = content_search::apply_replacements(
        &replacements,
        &skip_paths,
        &AtomicBool::new(false),
        Some(ctx.data_dir()),
    )
    .expect("caller-supplied skip identity should remain a successful no-op");

    assert_eq!(fixture::read_text(&path), "needle\n");
    assert_eq!(outcome.result.replaced_count, 0);
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(outcome.result.skipped_count, 1);
    assert_eq!(outcome.result.error_count, 0);
    assert_eq!(outcome.result.skipped_sample[0], path.display().to_string());
    assert!(outcome.undo_backup.is_empty());
    assert!(
        search_backup::load(ctx.data_dir())
            .expect("empty journal state")
            .is_empty()
    );
}

#[test]
fn replacement_output_accepts_exact_file_limit_and_rejects_one_byte_over() {
    let file_limit = usize::try_from(content_search::MAX_REPLACE_FILE_BYTES)
        .expect("Replace All file limit should fit usize");

    {
        let ctx = TestContext::new();
        let path = ctx.write_file("workspace/exact-output.txt", "x");
        let replacement = expanding_replacement(
            &path,
            SearchMatchId::from_index(0),
            Arc::from("a".repeat(file_limit)),
        );

        let outcome = content_search::apply_replacements(
            &[replacement],
            &HashSet::new(),
            &AtomicBool::new(false),
            Some(ctx.data_dir()),
        )
        .expect("exact per-file output limit should remain replaceable and undoable");

        assert_eq!(fixture::read_bytes(&path).len(), file_limit);
        assert_eq!(outcome.result.files_affected, 1);
        assert_eq!(outcome.result.skipped_count, 0);
        assert!(outcome.undo_backup.contains_key(&path));
    }

    let ctx = TestContext::new();
    let path = ctx.write_file("workspace/over-output.txt", "x");
    let replacement = expanding_replacement(
        &path,
        SearchMatchId::from_index(1),
        Arc::from("b".repeat(file_limit.saturating_add(1))),
    );

    let outcome = content_search::apply_replacements(
        &[replacement],
        &HashSet::new(),
        &AtomicBool::new(false),
        Some(ctx.data_dir()),
    )
    .expect("one-byte-over output should be a bounded skipped target");

    assert_eq!(fixture::read_text(&path), "x");
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(outcome.result.skipped_count, 1);
    assert_eq!(outcome.result.error_count, 1);
    assert!(
        outcome
            .result
            .error_sample
            .iter()
            .any(|message| message.contains("replacement output would exceed"))
    );
    assert!(outcome.undo_backup.is_empty());
}

#[test]
fn dense_short_line_replace_all_reports_replacement_bounded_construction() {
    const SOURCE_LINES: usize = 512 * 1_024;
    const REPLACEMENTS: usize = 1_000;

    let ctx = TestContext::new();
    let path = ctx.write_file("workspace/dense.txt", &"x\n".repeat(SOURCE_LINES));
    let original_line: Arc<str> = Arc::from("x");
    let replacement_text: Arc<str> = Arc::from("y");
    let replacements: Vec<_> = (0..REPLACEMENTS)
        .map(|index| {
            let line_index = index.saturating_mul(SOURCE_LINES.saturating_sub(1))
                / REPLACEMENTS.saturating_sub(1);
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
    let cancel = AtomicBool::new(false);

    let outcome = content_search::apply_replacements(
        &replacements,
        &HashSet::new(),
        &cancel,
        Some(ctx.data_dir()),
    )
    .expect("dense-line Replace All should succeed");
    let metrics = outcome.metrics();
    let (result, backup) = outcome.into_parts();

    assert_eq!(result.replaced_count, REPLACEMENTS);
    assert_eq!(result.files_affected, 1);
    assert_eq!(
        metrics.source_lines,
        u64::try_from(SOURCE_LINES).expect("source-line fixture count should fit u64")
    );
    assert_eq!(metrics.accepted_replacements, REPLACEMENTS);
    assert_eq!(metrics.retained_edit_records, REPLACEMENTS);
    assert!(metrics.retained_edit_records < SOURCE_LINES);
    assert_eq!(
        metrics.output_bytes,
        u64::try_from(SOURCE_LINES * 2).expect("output fixture bytes should fit u64")
    );
    assert_eq!(
        metrics.undo_bytes,
        u64::try_from(SOURCE_LINES * 4).expect("undo fixture bytes should fit u64")
    );
    assert_eq!(
        search_backup::load(ctx.data_dir()).expect("load active dense-line undo journal"),
        backup,
        "journal-before-mutation state must retain the exact before/after payload"
    );
}

#[test]
fn replace_preview_keeps_only_valid_rows_and_reports_content_free_reason_counts() {
    let valid = SearchMatch::new(PathBuf::from("/tmp/valid.txt"), 1, "alpha", 0..5)
        .with_id(SearchMatchId::from_index(0));
    let invalid_range = SearchMatch::new(
        PathBuf::from("/tmp/invalid.txt"),
        2,
        "private-source-sentinel",
        0..7,
    )
    .with_id(SearchMatchId::from_index(1));
    let options = ContentSearchOptions {
        regex: true,
        ..ContentSearchOptions::default()
    };

    let outcome = generate_replacement_preview(
        &[valid, invalid_range],
        "alpha",
        "private-replacement-sentinel",
        &options,
    );

    assert_eq!(outcome.len(), 1);
    assert_eq!(outcome.preview_index(SearchMatchId::from_index(0)), Some(0));
    assert_eq!(outcome.preview_index(SearchMatchId::from_index(1)), None);
    assert_eq!(
        outcome
            .skipped
            .count(ReplacePreviewSkipReason::RegexRangeMismatch),
        1
    );
    assert!(!format!("{outcome:?}").contains("private-source-sentinel"));
    assert!(!format!("{:?}", outcome.skipped).contains("private-replacement-sentinel"));
}

#[test]
fn interrupted_startup_cleanup_never_reactivates_replace_undo() {
    let ctx = TestContext::new();
    let backup = sample_backup();
    search_backup::save(ctx.data_dir(), &backup).expect("save active replace journal");
    fixture::write_text(
        &ctx.data_dir().join(JOURNAL_DIR).join(CLEANUP_MARKER_FILE),
        r#"{"reason":"simulated interrupted startup cleanup"}"#,
    );

    let after_restart = search_backup::load_recovering(ctx.data_dir());

    assert!(!after_restart.active);
    assert!(after_restart.backup.is_empty());
    assert!(!after_restart.diagnostics.is_empty());
    assert!(
        search_backup::load(ctx.data_dir())
            .expect("interrupted cleanup should load as inactive")
            .is_empty()
    );

    let cleanup = search_backup::cleanup_stale(ctx.data_dir());
    assert!(cleanup.diagnostics.is_empty());
    assert!(!fs_metadata::exists(&ctx.data_dir().join(JOURNAL_DIR)));
}

#[test]
fn undo_completion_cleanup_remains_empty_across_restart() {
    let ctx = TestContext::new();
    let backup = sample_backup();
    search_backup::save(ctx.data_dir(), &backup).expect("save active replace journal");
    assert_eq!(
        search_backup::load(ctx.data_dir()).expect("active journal loads"),
        backup
    );

    search_backup::delete(ctx.data_dir()).expect("undo completion cleanup");

    let after_restart = search_backup::load_recovering(ctx.data_dir());
    assert!(!after_restart.active);
    assert!(after_restart.backup.is_empty());
    assert!(after_restart.diagnostics.is_empty());
}

fn sample_backup() -> ReplaceUndoBackup {
    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        PathBuf::from("/tmp/lushtext-replace-a.txt"),
        ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec()),
    );
    backup.insert(
        PathBuf::from("/tmp/lushtext-replace-b.txt"),
        ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec()),
    );
    backup
}

fn replacement_for(path: &std::path::Path, match_id: SearchMatchId) -> Replacement {
    Replacement {
        match_id,
        path: path.to_path_buf(),
        line_number: 1,
        original_line: Arc::from("needle"),
        replaced_line: "done".to_string(),
        replacement: Arc::from("done"),
        match_range: 0..6,
    }
}

fn expanding_replacement(
    path: &std::path::Path,
    match_id: SearchMatchId,
    replacement: Arc<str>,
) -> Replacement {
    Replacement {
        match_id,
        path: path.to_path_buf(),
        line_number: 1,
        original_line: Arc::from("x"),
        replaced_line: String::new(),
        replacement,
        match_range: 0..1,
    }
}

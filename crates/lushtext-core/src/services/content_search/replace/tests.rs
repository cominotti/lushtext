// SPDX-License-Identifier: GPL-3.0-or-later

use super::stream::{
    NoDigest, RecordingSink, ReplacementTextOutcome, build_replaced_text_reference, plan_edits,
    stream_whole,
};
use super::*;
use crate::model::content_search::{
    MAX_REPLACE_PREVIEW_ROWS, ReplacePreviewIneligibleFile, ReplaceSkipReason, Replacement,
};
use crate::services::filesystem::fixture;
use crate::services::filesystem::{metadata as fs_metadata, read as fs_read, write as fs_write};
use crate::services::search_backup;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use tempfile::{TempDir, tempdir};

/// Helper: create a Replacement struct for testing.
fn make_replacement(
    path: &Path,
    line_number: u64,
    original_line: &str,
    replacement: &str,
    match_range: std::ops::Range<usize>,
) -> Replacement {
    let mut replaced_line = original_line.to_string();
    let start = match_range.start.min(replaced_line.len());
    let end = match_range.end.min(replaced_line.len());
    replaced_line.replace_range(start..end, replacement);
    Replacement {
        match_id: crate::model::content_search::SearchMatchId::from_index(0),
        path: path.to_path_buf(),
        line_number,
        original_line: original_line.to_string().into(),
        replaced_line,
        replacement: replacement.to_string().into(),
        match_range,
    }
}

/// Plenty of free space, so the reserve never decides a test by accident.
const AMPLE_FREE: u64 = 1 << 40;

/// Apply with a fresh journal directory and ample free space.
fn apply(
    replacements: &[Replacement],
    skip: &HashSet<PathBuf>,
) -> (anyhow::Result<ApplyReplacementsOutcome>, TempDir) {
    let journal = tempdir().expect("journal tempdir");
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let outcome = apply_replacements(replacements, skip, &AtomicBool::new(false), journal.path());
    (outcome, journal)
}

fn sized(dir: &Path, name: &str, len: usize) -> PathBuf {
    let file = dir.join(name);
    let mut content = "needle\n".to_string();
    content.push_str(&"a".repeat(len - content.len()));
    fixture::write_text(&file, &content);
    file
}

fn body_of(entry: &ReplaceUndoEntry) -> Vec<u8> {
    match &entry.original_source {
        UndoOriginal::Body(body) => fs_read::bytes(body).expect("read undo body"),
        UndoOriginal::LegacyEntry => panic!("a run writes only streamed entries"),
    }
}

// --- limits -------------------------------------------------------------

#[test]
fn replace_all_limits_are_the_open_tier_and_mains_budgets() {
    assert_eq!(
        MAX_REPLACE_FILE_BYTES,
        crate::model::document_scale::REFUSE_TO_OPEN
    );
    assert_eq!(MAIN_REPLACE_FILE_BYTES, 10 * 1024 * 1024);
    assert_eq!(MAIN_MAX_REPLACE_FILE_BYTES, 16 * 1024 * 1024);
    assert_eq!(MAIN_REPLACE_UNDO_BYTES, 64 * 1024 * 1024);
    assert_eq!(REPLACE_DISK_RESERVE_BYTES, 1024 * 1024 * 1024);
}

#[test]
fn parallel_before_rename_seams_are_keyed_and_cleaned_up() {
    let dir = tempdir().expect("tempdir");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    {
        let first_seam = fail_next_replace_before_rename_for_path_for_test(&first);
        let second_seam = fail_next_replace_before_rename_for_path_for_test(&second);
        assert!(replace_before_rename_failure_is_armed_for_test(&first));
        assert!(take_replace_before_rename_failure_for_test(&first));
        assert!(!replace_before_rename_failure_is_armed_for_test(&first));
        assert!(replace_before_rename_failure_is_armed_for_test(&second));
        assert!(!take_replace_before_rename_failure_for_test(&first));
        drop(first_seam);
        drop(second_seam);
    }
    assert!(!replace_before_rename_failure_is_armed_for_test(&second));
}

#[test]
fn undo_descriptors_retain_paths_only_never_file_bytes() {
    let dir = tempdir().expect("tempdir");
    let file = sized(dir.path(), "big.txt", 4 * 1024 * 1024);
    let (outcome, _journal) = apply(
        &[make_replacement(&file, 1, "needle", "thread", 0..6)],
        &HashSet::new(),
    );
    let outcome = outcome.expect("replace");
    let weight = replace_undo_retained_byte_weight(&outcome.undo_backup);
    assert!(weight > 0);
    assert!(
        weight < 4 * 1024,
        "a descriptor holds paths and digests, not the 4 MiB file: {weight}"
    );
    assert_eq!(outcome.metrics.undo_descriptor_bytes, weight);
}

#[test]
fn replacement_count_cap_is_rejected_before_any_target_mutation() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("count-cap.txt");
    fixture::write_text(&file, "needle\n");
    let replacement = make_replacement(&file, 1, "needle", "thread", 0..6);
    let replacements = vec![replacement; MAX_REPLACE_PREVIEW_ROWS + 1];
    let (outcome, _journal) = apply(&replacements, &HashSet::new());
    let error = outcome.expect_err("over-cap replacement selections must be rejected");
    assert!(error.to_string().contains("replacement limit"));
    assert_eq!(fixture::read_text(&file), "needle\n");
}

// --- apply ----------------------------------------------------------------

#[test]
fn apply_replaces_every_file_and_records_digests_and_bodies() {
    let dir = tempdir().expect("tempdir");
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");
    fixture::write_text(&file_a, "let hello = 1;\nlet world = 2;\n");
    fixture::write_text(&file_b, "fn hello() {}\n");
    let (outcome, journal) = apply(
        &[
            make_replacement(&file_a, 1, "let hello = 1;", "goodbye", 4..9),
            make_replacement(&file_b, 1, "fn hello() {}", "goodbye", 3..8),
        ],
        &HashSet::new(),
    );
    let outcome = outcome.expect("replace");

    assert_eq!(outcome.result.replaced_count, 2);
    assert_eq!(outcome.result.files_affected, 2);
    assert!(outcome.skip_report.is_empty());
    assert_eq!(
        fixture::read_text(&file_a),
        "let goodbye = 1;\nlet world = 2;\n"
    );
    assert_eq!(fixture::read_text(&file_b), "fn goodbye() {}\n");
    let entry = &outcome.undo_backup[&file_a];
    assert_eq!(body_of(entry), b"let hello = 1;\nlet world = 2;\n");
    assert_eq!(
        entry.original,
        ContentDigest::of(b"let hello = 1;\nlet world = 2;\n")
    );
    assert_eq!(
        entry.replaced,
        ContentDigest::of(b"let goodbye = 1;\nlet world = 2;\n")
    );
    // The persisted journal is the same set of descriptors.
    assert_eq!(
        search_backup::load(journal.path()).expect("load journal"),
        outcome.undo_backup
    );
}

#[test]
fn apply_preserves_crlf_and_unterminated_last_lines() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("mixed.txt");
    fixture::write_text(&file, "alpha needle\r\nbeta needle");
    let (outcome, _journal) = apply(
        &[
            make_replacement(&file, 1, "alpha needle", "hay", 6..12),
            make_replacement(&file, 2, "beta needle", "stack", 5..11),
        ],
        &HashSet::new(),
    );
    assert_eq!(outcome.expect("replace").result.replaced_count, 2);
    assert_eq!(fixture::read_text(&file), "alpha hay\r\nbeta stack");
}

#[test]
fn the_journal_entry_is_written_after_the_body_and_before_the_target() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let file = dir.path().join("order.txt");
    fixture::write_text(&file, "needle\n");
    let journal_path = journal.path().to_path_buf();
    let observed = std::sync::Arc::new(std::sync::Mutex::new(None));
    let observed_in_hook = observed.clone();
    register_after_pass_a_hook_for_test(&file, move |path| {
        *observed_in_hook.lock().expect("hook lock") = Some((
            fs_metadata::exists(&search_backup::body_path(&journal_path, path)),
            fs_metadata::exists(&search_backup::entry_path(&journal_path, path)),
            fixture::read_text(path),
        ));
    });
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    apply_replacements(
        &[make_replacement(&file, 1, "needle", "thread", 0..6)],
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("replace");
    let (body, entry, target) = observed.lock().expect("observed").take().expect("hook ran");
    assert!(body, "pass A's body is durable before its entry");
    assert!(!entry, "the entry is written after pass A");
    assert_eq!(target, "needle\n", "the target is rewritten last");
    assert_eq!(fixture::read_text(&file), "thread\n");
}

#[test]
fn a_target_changed_during_pass_b_is_skipped_and_left_alone() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("raced.txt");
    fixture::write_text(&file, "needle\n");
    register_after_pass_a_hook_for_test(&file, |path| {
        fixture::write_text(path, "needle edited elsewhere\n");
    });
    let (outcome, journal) = apply(
        &[make_replacement(&file, 1, "needle", "thread", 0..6)],
        &HashSet::new(),
    );
    let outcome = outcome.expect("a changed file is a skip, not a failure");
    assert_eq!(fixture::read_text(&file), "needle edited elsewhere\n");
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::ChangedDuringReplace),
        1
    );
    assert!(outcome.undo_backup.is_empty());
    // Pre-rename failure removed the entry, then the body.
    assert!(!fs_metadata::exists(&search_backup::entry_path(
        journal.path(),
        &file
    )));
    assert!(!fs_metadata::exists(&search_backup::body_path(
        journal.path(),
        &file
    )));
}

#[test]
fn post_rename_durability_failure_keeps_changed_bytes_and_undo_evidence() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("durability-ambiguous.txt");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "thread", 0..6)];
    let journal = tempdir().expect("journal");
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    // Fail the parent sync of the target's own write, after its rename.
    super::register_before_target_write_hook_for_test(&file, || {
        crate::services::filesystem::write::fail_next_parent_sync_for_test(
            std::io::ErrorKind::Other,
        );
    });
    let outcome = apply_replacements(
        &replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("post-rename durability ambiguity still changed the target");
    assert_eq!(fixture::read_text(&file), "thread\n");
    assert_eq!(outcome.result.files_affected, 1);
    assert!(outcome.undo_backup.contains_key(&file));
}

#[test]
fn stale_reserved_apply_cannot_overwrite_newer_committed_journal_or_file() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "needle\n");
    let newer_path = dir.path().join("newer.rs");
    let newer = search_backup::fixture::write_stream_journal(
        journal.path(),
        &[(&newer_path, b"before", b"after")],
    )
    .expect("commit newer replacement journal");

    let generation = std::sync::Arc::new(AtomicU32::new(2));
    let stale = ReplaceJournalFreshness::new(generation, 1);
    let outcome = apply_replacements_if_current(
        &[make_replacement(&file, 1, "needle", "stale", 0..6)],
        &HashSet::new(),
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
        &stale,
    )
    .expect("stale apply freshness check");

    assert!(outcome.is_none());
    assert_eq!(fixture::read_text(&file), "needle\n");
    assert_eq!(
        search_backup::load(journal.path()).expect("load newer journal"),
        newer
    );
}

#[test]
fn apply_waits_for_an_editor_save_guard_on_the_same_target() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
    let save_guard =
        fs_write::TargetWriteGuard::acquire(&file).expect("simulate in-flight save guard");
    let (tx, rx) = mpsc::channel();
    let journal_path = journal.path().to_path_buf();
    let worker = std::thread::spawn(move || {
        let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
        let result = apply_replacements(
            &replacements,
            &HashSet::new(),
            &AtomicBool::new(false),
            &journal_path,
        );
        tx.send(result).expect("send replace result");
    });
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    assert_eq!(fixture::read_text(&file), "needle\n");
    drop(save_guard);
    let result = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("replace finishes after the save guard drops")
        .expect("replace succeeds");
    worker.join().expect("replace worker joins");
    assert_eq!(result.result.files_affected, 1);
    assert_eq!(fixture::read_text(&file), "replaced\n");
}

#[test]
fn apply_waits_for_the_journal_guard_and_can_be_cancelled_while_waiting() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
    let recovery_guard =
        search_backup::acquire_journal_guard().expect("simulate startup journal recovery");
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let journal_path = journal.path().to_path_buf();
    let cancel_for_worker = cancel.clone();
    let worker = std::thread::spawn(move || {
        let result = apply_replacements(
            &replacements,
            &HashSet::new(),
            &cancel_for_worker,
            &journal_path,
        );
        tx.send(result).expect("send replace result");
    });
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    cancel.store(true, Ordering::Relaxed);
    let result = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a cancelled wait returns while the guard is still held");
    worker.join().expect("worker joins");
    drop(recovery_guard);
    assert!(
        result
            .expect_err("cancelled")
            .to_string()
            .contains("cancelled")
    );
    assert_eq!(fixture::read_text(&file), "needle\n");
}

#[test]
fn a_file_above_the_open_tier_is_skipped_before_reading() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("huge.rs");
    fixture::create_sparse_file(&file, MAX_REPLACE_FILE_BYTES + 1);
    let (outcome, _journal) = apply(
        &[make_replacement(&file, 1, "needle", "replaced", 0..6)],
        &HashSet::new(),
    );
    let outcome = outcome.expect("oversized files are skipped, not fatal");
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(outcome.result.skipped_count, 1);
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::LargerThanOpenTier),
        1
    );
    assert!(outcome.undo_backup.is_empty());
}

#[test]
fn a_lowered_file_limit_admits_a_file_at_it_and_skips_one_byte_over() {
    let dir = tempdir().expect("tempdir");
    let at = sized(dir.path(), "at.txt", 4096);
    let over = sized(dir.path(), "over.txt", 4097);
    set_max_replace_file_bytes_for_test(Some(4096));
    let (outcome, _journal) = apply(
        &[
            make_replacement(&at, 1, "needle", "thread", 0..6),
            make_replacement(&over, 1, "needle", "thread", 0..6),
        ],
        &HashSet::new(),
    );
    set_max_replace_file_bytes_for_test(None);
    let outcome = outcome.expect("replace");
    assert!(outcome.undo_backup.contains_key(&at));
    assert!(!outcome.undo_backup.contains_key(&over));
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::LargerThanOpenTier),
        1
    );
}

/// No loss against `main` (design D4): a run `main` processed in full, every
/// file at most 10 MiB and the payload within 64 MiB, is processed in full.
#[test]
fn no_loss_replace_all_processes_every_file_main_processed() {
    let dir = tempdir().expect("tempdir");
    let files: Vec<_> = (0..3)
        .map(|index| {
            sized(
                dir.path(),
                &format!("ten-mib-{index}.txt"),
                10 * 1024 * 1024,
            )
        })
        .collect();
    let replacements: Vec<_> = files
        .iter()
        .map(|file| make_replacement(file, 1, "needle", "thread", 0..6))
        .collect();
    let (outcome, _journal) = apply(&replacements, &HashSet::new());
    let outcome = outcome.expect("a run main processed in full");
    assert_eq!(outcome.result.files_affected, 3);
    assert!(outcome.skip_report.is_empty());
    for file in &files {
        assert!(fixture::read_text(file).starts_with("thread\n"));
    }
}

/// No loss with no disk to spare: every file `main` replaced is replaced even
/// when the journal filesystem has only the raw bytes free (the reserve is
/// waived for exactly `main`'s files), while a file `main` skipped needs the
/// reserve and is reported (design D4).
#[test]
fn no_loss_without_the_reserve_still_replaces_mains_files() {
    let dir = tempdir().expect("tempdir");
    let small = sized(dir.path(), "a-small.txt", 1024 * 1024);
    // 17 MiB is past `main`'s 16 MiB per-file limit: `main` skipped it.
    let large = sized(dir.path(), "b-large.txt", 17 * 1024 * 1024);
    let journal = tempdir().expect("journal");
    let _free = fs_metadata::fake_free_space_for_test(64 * 1024 * 1024);
    let outcome = apply_replacements(
        &[
            make_replacement(&small, 1, "needle", "thread", 0..6),
            make_replacement(&large, 1, "needle", "thread", 0..6),
        ],
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("replace");
    assert!(outcome.undo_backup.contains_key(&small));
    assert!(!outcome.undo_backup.contains_key(&large));
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::NotEnoughDiskSpace),
        1
    );
    assert!(fixture::read_text(&large).starts_with("needle\n"));
}

/// Beyond `main`: with the reserve free, a file `main` skipped for its size
/// (here 17 MiB, past its 16 MiB per-file limit) is replaced too.
#[test]
fn files_main_skipped_for_size_are_replaced_in_the_second_pass() {
    let dir = tempdir().expect("tempdir");
    let large = sized(dir.path(), "a-large.txt", 17 * 1024 * 1024);
    let small = sized(dir.path(), "b-small.txt", 1024);
    let (outcome, _journal) = apply(
        &[
            make_replacement(&large, 1, "needle", "thread", 0..6),
            make_replacement(&small, 1, "needle", "thread", 0..6),
        ],
        &HashSet::new(),
    );
    let outcome = outcome.expect("replace");
    assert_eq!(outcome.result.files_affected, 2);
    assert!(fixture::read_text(&large).starts_with("thread\n"));
}

#[test]
fn not_enough_disk_space_skips_the_file_and_leaves_it_alone() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("a.txt");
    fixture::write_text(&file, "needle\n");
    let journal = tempdir().expect("journal");
    let _free = fs_metadata::fake_free_space_for_test(3);
    let outcome = apply_replacements(
        &[make_replacement(&file, 1, "needle", "thread", 0..6)],
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("replace");
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::NotEnoughDiskSpace),
        1
    );
    assert_eq!(fixture::read_text(&file), "needle\n");
}

#[test]
fn pre_rename_failure_cleans_the_entry_and_body_and_later_files_proceed() {
    let dir = tempdir().expect("tempdir");
    let first = dir.path().join("a-first.txt");
    let later = dir.path().join("b-later.txt");
    fixture::write_text(&first, "needle\n");
    fixture::write_text(&later, "needle\n");
    let _seam = fail_next_replace_before_rename_for_path_for_test(&first);
    let (outcome, journal) = apply(
        &[
            make_replacement(&first, 1, "needle", "done", 0..6),
            make_replacement(&later, 1, "needle", "done", 0..6),
        ],
        &HashSet::new(),
    );
    let outcome = outcome.expect("later target proceeds");
    assert_eq!(fixture::read_text(&first), "needle\n");
    assert_eq!(fixture::read_text(&later), "done\n");
    assert_eq!(outcome.skip_report.count(ReplaceSkipReason::WriteFailed), 1);
    assert!(!fs_metadata::exists(&search_backup::body_path(
        journal.path(),
        &first
    )));
    let persisted = search_backup::load(journal.path()).expect("active journal");
    assert!(!persisted.contains_key(&first));
    assert!(persisted.contains_key(&later));
}

#[test]
fn stale_lines_invalid_utf8_and_skip_paths_are_reported_by_reason() {
    let dir = tempdir().expect("tempdir");
    let stale = dir.path().join("a-stale.txt");
    let binary = dir.path().join("b-binary.txt");
    let unsaved = dir.path().join("c-unsaved.txt");
    let past_eof = dir.path().join("d-past-eof.txt");
    fixture::write_text(&stale, "something else\n");
    fixture::write_bytes(&binary, b"needle\n\xff\xfe\n");
    fixture::write_text(&unsaved, "needle\n");
    fixture::write_text(&past_eof, "needle\n");
    let skip = HashSet::from([unsaved.clone()]);
    let (outcome, _journal) = apply(
        &[
            make_replacement(&stale, 1, "needle", "x", 0..6),
            make_replacement(&binary, 1, "needle", "x", 0..6),
            make_replacement(&unsaved, 1, "needle", "x", 0..6),
            make_replacement(&past_eof, 9, "needle", "x", 0..6),
        ],
        &skip,
    );
    let outcome = outcome.expect("replace");
    let report = &outcome.skip_report;
    assert_eq!(report.count(ReplaceSkipReason::ChangedSinceSearch), 2);
    assert_eq!(report.count(ReplaceSkipReason::NotUtf8), 1);
    assert_eq!(report.count(ReplaceSkipReason::UnsavedChanges), 1);
    let first = report
        .files()
        .iter()
        .find(|file| file.path == stale)
        .expect("stale file listed");
    assert_eq!(first.first_line, Some(1));
    assert_eq!(fixture::read_text(&unsaved), "needle\n");
    assert_eq!(outcome.result.files_affected, 0);
}

#[test]
fn preview_ineligible_files_join_the_report_partly_or_wholly() {
    let dir = tempdir().expect("tempdir");
    let partly = dir.path().join("a-partly.txt");
    let wholly = dir.path().join("b-wholly.txt");
    fixture::write_text(&partly, "needle\n");
    fixture::write_text(&wholly, "needle\n");
    let ineligible = [
        ReplacePreviewIneligibleFile {
            path: partly.clone(),
            reason: ReplaceSkipReason::LineTooLong,
            matches: 3,
            first_line: 7,
        },
        ReplacePreviewIneligibleFile {
            path: wholly.clone(),
            reason: ReplaceSkipReason::LineTooLong,
            matches: 2,
            first_line: 1,
        },
    ];
    let journal = tempdir().expect("journal");
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let outcome = apply_replacements_with(ReplaceRunRequest {
        replacements: &[make_replacement(&partly, 1, "needle", "x", 0..6)],
        skip_paths: &HashSet::new(),
        open_canonical_identities: &HashSet::new(),
        ineligible: &ineligible,
        cancel: &AtomicBool::new(false),
        progress: None,
        journal_data_dir: journal.path(),
    })
    .expect("replace");
    assert_eq!(fixture::read_text(&partly), "x\n");
    let report = &outcome.skip_report;
    // The wholly skipped file counts under its reason; the partly replaced
    // one counts apart, and both are listed.
    assert_eq!(report.count(ReplaceSkipReason::LineTooLong), 1);
    assert_eq!(report.partly_replaced(), 1);
    assert_eq!(report.total(), 2);
    let listed = report
        .files()
        .iter()
        .find(|file| file.path == partly)
        .expect("partly replaced file listed");
    assert_eq!(listed.matches_left, Some(3));
    let listed = report
        .files()
        .iter()
        .find(|file| file.path == wholly)
        .expect("wholly skipped file listed");
    assert_eq!(listed.matches_left, None);
}

#[test]
fn a_run_that_replaces_nothing_removes_the_superseded_journal() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let older = dir.path().join("older.txt");
    search_backup::fixture::write_stream_journal(journal.path(), &[(&older, b"a", b"b")])
        .expect("older journal");
    let file = dir.path().join("stale.txt");
    fixture::write_text(&file, "changed\n");
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let outcome = apply_replacements(
        &[make_replacement(&file, 1, "needle", "x", 0..6)],
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("replace");
    assert!(outcome.undo_backup.is_empty());
    assert!(
        search_backup::load(journal.path())
            .expect("load")
            .is_empty()
    );
}

#[test]
fn replace_all_failure_heavy_summary_is_exact_bounded_and_content_free() {
    let dir = tempdir().expect("tempdir");
    let private_line: std::sync::Arc<str> = std::sync::Arc::from("private-document-sentinel");
    let replacement: std::sync::Arc<str> = std::sync::Arc::from("done");
    let replacements = (0..MAX_REPLACE_PREVIEW_ROWS)
        .map(|index| Replacement {
            match_id: crate::model::content_search::SearchMatchId::from_index(index),
            path: dir.path().join(format!("missing-{index:05}.txt")),
            line_number: 1,
            original_line: private_line.clone(),
            replaced_line: "done".to_string(),
            replacement: replacement.clone(),
            match_range: 0..private_line.len(),
        })
        .collect::<Vec<_>>();
    let (outcome, _journal) = apply(&replacements, &HashSet::new());
    let outcome = outcome.expect("missing targets are reported, not fatal");
    assert_eq!(
        outcome.skip_report.count(ReplaceSkipReason::Unreadable),
        MAX_REPLACE_PREVIEW_ROWS
    );
    assert_eq!(outcome.result.error_count, MAX_REPLACE_PREVIEW_ROWS);
    assert!(outcome.result.error_sample.entries().len() <= 32);
    let summary = outcome.result.error_sample.summary("Replace All");
    assert!(!summary.contains(private_line.as_ref()));
    assert!(summary.len() < 1_024);
}

// --- undo -----------------------------------------------------------------

#[test]
fn undo_restores_each_file_and_retires_its_entry_then_body() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "let needle = 42;\n");
    let (outcome, journal) = apply(
        &[make_replacement(&file, 1, "let needle = 42;", "hay", 4..10)],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    let entry = backup[&file].clone();

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), 1);
    assert!(undone.remaining_backup.is_empty());
    assert_eq!(fixture::read_text(&file), "let needle = 42;\n");
    assert!(!fs_metadata::exists(&entry.entry_file));
    assert!(!fs_metadata::exists(&search_backup::body_path(
        journal.path(),
        &file
    )));
}

#[test]
fn undo_recognises_an_already_restored_file_and_skips_a_diverged_one() {
    let dir = tempdir().expect("tempdir");
    let restored = dir.path().join("a.txt");
    let diverged = dir.path().join("b.txt");
    fixture::write_text(&restored, "needle\n");
    fixture::write_text(&diverged, "needle\n");
    let (outcome, _journal) = apply(
        &[
            make_replacement(&restored, 1, "needle", "done", 0..6),
            make_replacement(&diverged, 1, "needle", "done", 0..6),
        ],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    fixture::write_text(&restored, "needle\n");
    fixture::write_text(&diverged, "edited after\n");

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), 1);
    assert_eq!(undone.skipped_count, 1);
    assert!(undone.remaining_backup.contains_key(&diverged));
    assert_eq!(fixture::read_text(&diverged), "edited after\n");
}

#[test]
fn undo_of_a_body_corrupted_after_the_run_writes_nothing_and_stays_retryable() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("corrupt.txt");
    fixture::write_text(&file, "needle\n");
    let (outcome, journal) = apply(
        &[make_replacement(&file, 1, "needle", "done", 0..6)],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    // Same length, other bytes: only the digest can tell.
    fixture::write_bytes(
        &search_backup::body_path(journal.path(), &file),
        b"nEedle\n",
    );

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), 0);
    assert!(undone.remaining_backup.contains_key(&file));
    assert_eq!(fixture::read_text(&file), "done\n");
    assert!(fs_metadata::exists(&backup[&file].entry_file));
}

#[test]
fn undo_with_less_free_space_than_the_reserve_restores_every_file() {
    let dir = tempdir().expect("tempdir");
    let files: Vec<_> = (0..3)
        .map(|index| {
            let file = dir.path().join(format!("f{index}.txt"));
            fixture::write_text(&file, "needle\n");
            file
        })
        .collect();
    let replacements: Vec<_> = files
        .iter()
        .map(|file| make_replacement(file, 1, "needle", "done", 0..6))
        .collect();
    let (outcome, _journal) = apply(&replacements, &HashSet::new());
    let backup = outcome.expect("replace").undo_backup;
    // Far below the 1 GiB reserve, far above what three restores write.
    let _free = fs_metadata::fake_free_space_for_test(1024 * 1024);

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), files.len());
    assert!(undone.remaining_backup.is_empty());
    for file in &files {
        assert_eq!(fixture::read_text(file), "needle\n");
    }
}

#[test]
fn undo_growth_after_metadata_is_skipped_without_reading_it_whole() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("grown.txt");
    fixture::write_text(&file, "needle\n");
    let (outcome, _journal) = apply(
        &[make_replacement(&file, 1, "needle", "after", 0..6)],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    let _hook = register_undo_after_metadata_hook_for_test(&file, |path| {
        fixture::write_repeated_bytes(path, b"x", 64 * 1024);
    });

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), 0);
    assert_eq!(undone.skipped_count, 1);
    assert_eq!(undone.remaining_backup, backup);
}

#[test]
fn undo_of_a_legacy_journal_restores_from_the_inline_entry() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let file = dir.path().join("legacy.txt");
    fixture::write_text(&file, "after\n");
    let backup = search_backup::fixture::write_legacy_journal(
        journal.path(),
        &[(&file, b"before\n", b"after\n")],
        false,
    )
    .expect("legacy journal");
    assert!(matches!(
        backup[&file].original_source,
        UndoOriginal::LegacyEntry
    ));

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), 1);
    assert_eq!(fixture::read_text(&file), "before\n");
}

#[test]
fn undo_failure_heavy_terminal_keeps_exact_totals_and_bounded_ordered_samples() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let seed = dir.path().join("seed.txt");
    let template =
        search_backup::fixture::write_stream_journal(journal.path(), &[(&seed, b"a", b"b")])
            .expect("journal")[&seed]
            .clone();
    let mut backup = ReplaceUndoBackup::new();
    for index in 0..10_000 {
        backup.insert(
            dir.path().join(format!("missing-{index:05}.txt")),
            template.clone(),
        );
    }

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.failed_count, 10_000);
    assert_eq!(outcome.failed_sample.entries().len(), 32);
    assert!(outcome.failed_sample[0].contains("missing-00000.txt"));
    assert_eq!(outcome.remaining_count(), 10_000);
}

#[test]
fn undo_returns_only_the_restored_open_identity_intersection() {
    let dir = tempdir().expect("tempdir");
    let open = dir.path().join("open.txt");
    let closed = dir.path().join("closed.txt");
    fixture::write_text(&open, "needle\n");
    fixture::write_text(&closed, "needle\n");
    let (outcome, _journal) = apply(
        &[
            make_replacement(&open, 1, "needle", "x", 0..6),
            make_replacement(&closed, 1, "needle", "x", 0..6),
        ],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    let identity = fs_metadata::file_facts(&open)
        .expect("facts")
        .canonical_path
        .expect("canonical");
    let undone = undo_replacements_for_open_identities(&backup, &HashSet::from([identity]));
    assert_eq!(undone.restored_open_paths, vec![open]);
}

#[test]
fn replace_all_and_undo_preserve_file_mode() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("private.rs");
    fixture::write_text(&file, "let needle = 42;\n");
    fixture::set_mode(&file, 0o600);
    let (outcome, _journal) = apply(
        &[make_replacement(
            &file,
            1,
            "let needle = 42;",
            "found",
            4..10,
        )],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    assert_eq!(fixture::mode(&file) & 0o777, 0o600);
    assert_eq!(undo_replacements(&backup).restored_count(), 1);
    assert_eq!(fixture::mode(&file) & 0o777, 0o600);
}

// --- cancellation -----------------------------------------------------------

#[test]
fn a_cancelled_run_rolls_back_the_files_it_wrote_and_removes_its_journal() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let file_a = dir.path().join("a.txt");
    let file_b = dir.path().join("b.txt");
    fixture::write_text(&file_a, "needle\n");
    fixture::write_text(&file_b, "needle\n");
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let cancel_in_hook = cancel.clone();
    // Cancel once the second file's body is written: the first is replaced.
    register_after_pass_a_hook_for_test(&file_b, move |_| {
        cancel_in_hook.store(true, Ordering::Relaxed);
    });
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let result = apply_replacements(
        &[
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ],
        &HashSet::new(),
        &cancel,
        journal.path(),
    );
    assert!(
        result
            .expect_err("cancelled")
            .to_string()
            .contains("cancelled")
    );
    assert_eq!(fixture::read_text(&file_a), "needle\n");
    assert_eq!(fixture::read_text(&file_b), "needle\n");
    assert!(
        search_backup::load(journal.path())
            .expect("load")
            .is_empty()
    );
}

#[test]
fn a_rollback_with_less_free_space_than_the_reserve_restores_every_file() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let files: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|name| {
            let file = dir.path().join(format!("{name}.txt"));
            fixture::write_text(&file, "needle\n");
            file
        })
        .collect();
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let cancel_in_hook = cancel.clone();
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    // Once the third file's body is written, the first two are replaced: the
    // disk then drops far below the 1 GiB reserve (still far above what two
    // restores write), and the run is cancelled. The seam is per thread and
    // this apply runs on the test thread; the outer guard restores it.
    register_after_pass_a_hook_for_test(&files[2], move |_| {
        std::mem::forget(fs_metadata::fake_free_space_for_test(1024 * 1024));
        cancel_in_hook.store(true, Ordering::Relaxed);
    });
    let replacements: Vec<_> = files
        .iter()
        .map(|file| make_replacement(file, 1, "needle", "replaced", 0..6))
        .collect();

    let result = apply_replacements(&replacements, &HashSet::new(), &cancel, journal.path());

    assert!(
        result
            .expect_err("cancelled")
            .to_string()
            .contains("cancelled")
    );
    for file in &files {
        assert_eq!(fixture::read_text(file), "needle\n");
    }
    assert!(
        search_backup::load(journal.path())
            .expect("load")
            .is_empty()
    );
}

// --- stream engine ------------------------------------------------------------

fn engine(
    text: &str,
    replacements: &[Replacement],
    chunk: usize,
) -> (ReplacementTextOutcome, Vec<u8>) {
    let mut refs: Vec<&Replacement> = replacements.iter().collect();
    let edits = match plan_edits(&mut refs, || false) {
        Ok(edits) => edits,
        Err(outcome) => return (outcome, Vec::new()),
    };
    let (summary, out) = stream_whole(
        text.as_bytes(),
        &edits,
        chunk,
        u64::MAX,
        RecordingSink::default(),
        NoDigest,
    );
    assert_eq!(summary.source_digest.0, text.as_bytes());
    (summary.outcome, out)
}

#[test]
fn the_engine_matches_the_reference_at_every_chunk_size() {
    let path = PathBuf::from("/t.txt");
    let cases: [(&str, Vec<Replacement>); 4] = [
        (
            "alpha needle\r\nbeta needle\n",
            vec![
                make_replacement(&path, 1, "alpha needle", "hay", 6..12),
                make_replacement(&path, 2, "beta needle", "stack", 5..11),
            ],
        ),
        (
            "é needle ü\nlast",
            vec![make_replacement(&path, 1, "é needle ü", "x", 3..9)],
        ),
        (
            "\nneedle",
            vec![make_replacement(&path, 2, "needle", "", 0..6)],
        ),
        (
            "a\nb\nneedle needle\n",
            vec![
                make_replacement(&path, 3, "needle needle", "1", 0..6),
                make_replacement(&path, 3, "needle needle", "2", 7..13),
            ],
        ),
    ];
    for (text, replacements) in &cases {
        let mut refs: Vec<&Replacement> = replacements.iter().collect();
        let (expected, count) =
            build_replaced_text_reference(text, &mut refs).expect("reference replaces");
        for chunk in 1..=8 {
            let (outcome, out) = engine(text, replacements, chunk);
            assert_eq!(
                outcome,
                ReplacementTextOutcome::Replaced {
                    replacement_count: count
                },
                "{text:?} chunk {chunk}"
            );
            assert_eq!(String::from_utf8(out).expect("utf-8"), expected);
        }
    }
}

#[test]
fn the_engine_reports_a_stale_line_and_invalid_utf8_split_across_chunks() {
    let path = PathBuf::from("/t.txt");
    let stale = [make_replacement(&path, 2, "needle", "x", 0..6)];
    for chunk in 1..=4 {
        assert_eq!(
            engine("needle\nother\n", &stale, chunk).0,
            ReplacementTextOutcome::StaleLine { line_number: 2 }
        );
    }
    let replacements = [make_replacement(&path, 1, "needle", "x", 0..6)];
    let mut refs: Vec<&Replacement> = replacements.iter().collect();
    let edits = plan_edits(&mut refs, || false).expect("edits");
    for chunk in 1..=4 {
        let (summary, _) = stream_whole(
            b"needle\n\xe2\x82",
            &edits,
            chunk,
            u64::MAX,
            NoDigest,
            NoDigest,
        );
        assert_eq!(
            summary.outcome,
            ReplacementTextOutcome::NotUtf8,
            "chunk {chunk}"
        );
    }
}

#[test]
fn the_engine_stops_at_its_output_limit_without_passing_it() {
    let path = PathBuf::from("/t.txt");
    let replacements = [make_replacement(&path, 1, "needle", "longer text", 0..6)];
    let mut refs: Vec<&Replacement> = replacements.iter().collect();
    let edits = plan_edits(&mut refs, || false).expect("edits");
    let (summary, out) = stream_whole(b"needle\n", &edits, 3, 8, NoDigest, NoDigest);
    assert_eq!(summary.outcome, ReplacementTextOutcome::OutputLimitExceeded);
    assert!(out.len() <= 8);
}

#[test]
fn malformed_ranges_are_rejected_before_any_byte_is_read() {
    let path = PathBuf::from("/t.txt");
    let mut reversed = make_replacement(&path, 1, "needle", "x", 0..6);
    // Reversed on purpose: the planner must reject it.
    reversed.match_range = std::ops::Range { start: 4, end: 2 };
    let overlapping = [
        make_replacement(&path, 1, "needle", "x", 0..4),
        make_replacement(&path, 1, "needle", "y", 2..6),
    ];
    let zero_line = make_replacement(&path, 0, "needle", "x", 0..6);
    for replacements in [vec![reversed], overlapping.to_vec(), vec![zero_line]] {
        let mut refs: Vec<&Replacement> = replacements.iter().collect();
        assert!(matches!(
            plan_edits(&mut refs, || false),
            Err(ReplacementTextOutcome::InvalidPreview { .. })
        ));
    }
    let mut none: Vec<&Replacement> = Vec::new();
    assert!(matches!(
        plan_edits(&mut none, || false),
        Err(ReplacementTextOutcome::Unchanged)
    ));
}

#[test]
fn the_engine_buffers_only_targeted_lines() {
    let path = PathBuf::from("/t.txt");
    let mut text = "x".repeat(1024 * 1024);
    text.push('\n');
    text.push_str("needle\n");
    let replacements = [make_replacement(&path, 2, "needle", "y", 0..6)];
    let mut refs: Vec<&Replacement> = replacements.iter().collect();
    let edits = plan_edits(&mut refs, || false).expect("edits");
    let (summary, _) = stream_whole(text.as_bytes(), &edits, 4096, u64::MAX, NoDigest, NoDigest);
    assert!(matches!(
        summary.outcome,
        ReplacementTextOutcome::Replaced { .. }
    ));
    assert!(
        summary.metrics.retained_line_bytes <= 16,
        "an untargeted 1 MiB line is streamed, not buffered: {}",
        summary.metrics.retained_line_bytes
    );
}

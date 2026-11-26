// SPDX-License-Identifier: GPL-3.0-or-later

use super::stream::{
    NoDigest, RecordingSink, ReplacementTextOutcome, StreamEdit, build_replaced_text_reference,
    plan_edits, stream_whole,
};
use super::*;
use crate::filesystem::fixture;
use crate::filesystem::{metadata as fs_metadata, read as fs_read, write as fs_write};
use crate::search_backup;
use lushtext_model::content_search::{
    MAX_REPLACE_PREVIEW_ROWS, ReplacePreviewIneligibleFile, ReplaceSkipReason, Replacement,
};
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
        match_id: lushtext_model::content_search::SearchMatchId::from_index(0),
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
        lushtext_model::document_scale::REFUSE_TO_OPEN
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
        crate::filesystem::write::fail_next_parent_sync_for_test(std::io::ErrorKind::Other);
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
fn test_apply_replacements_waits_for_existing_save_guard_on_same_target() {
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
            match_id: lushtext_model::content_search::SearchMatchId::from_index(index),
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
    // The replaced length, other bytes: only the digest tells it diverged.
    fixture::write_text(&diverged, "dOne\n");

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), 1);
    assert_eq!(undone.skipped_count, 1);
    assert!(undone.remaining_backup.contains_key(&diverged));
    assert_eq!(fixture::read_text(&diverged), "dOne\n");
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

/// Growth after U0's metadata, past the file limit (the test limit), is
/// caught by the bounded hash and skipped: the file is never restored over,
/// even when its first bytes are still exactly what Replace All wrote.
#[test]
fn undo_growth_after_metadata_is_skipped_without_unbounded_allocation() {
    const LIMIT: u64 = 4096;
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("grown.txt");
    fixture::write_text(&file, "needle\n");
    let (outcome, _journal) = apply(
        &[make_replacement(&file, 1, "needle", "after", 0..6)],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    let _hook = register_undo_after_metadata_hook_for_test(&file, |path| {
        // The replaced bytes, then more, up to one byte past the limit.
        let mut grown = b"after\n".to_vec();
        grown.resize(usize::try_from(LIMIT + 1).expect("small"), b'x');
        fixture::write_bytes(path, &grown);
    });
    set_max_replace_file_bytes_for_test(Some(LIMIT));

    let undone = undo_replacements(&backup);
    set_max_replace_file_bytes_for_test(None);

    assert_eq!(undone.restored_count(), 0);
    assert_eq!(undone.skipped_count, 1);
    assert_eq!(undone.failed_count, 0);
    assert_eq!(undone.remaining_backup, backup);
    assert_eq!(
        fs_metadata::file_facts(&file)
            .expect("grown target facts")
            .byte_size,
        LIMIT + 1
    );
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

/// The recorded lines of the bounded check's edit tables: the line classes the
/// engine distinguishes, and `aé`, whose two-byte character a seam can cut
/// inside a targeted line.
const BOUNDED_LINES: [&str; 5] = ["", "a", "ab", "\r", "a\u{e9}"];
/// The bounded check's replacement texts: an empty one and one that adds a
/// line.
const BOUNDED_REPLACEMENTS: [&str; 2] = ["", "x\n"];

/// Every range of `line` that starts at or after `from` and whose ends are
/// character boundaries (a preview never records a range inside a character).
fn bounded_ranges(line: &str, from: usize) -> Vec<std::ops::Range<usize>> {
    let bounds: Vec<usize> = (from..=line.len())
        .filter(|&index| line.is_char_boundary(index))
        .collect();
    bounds
        .iter()
        .flat_map(|&start| {
            bounds
                .iter()
                .filter(move |&&end| end >= start)
                .map(move |&end| start..end)
        })
        .collect()
}

/// Every single edit of the bounded tables, on line 1 or 2.
fn bounded_domain_single_edits(path: &Path) -> Vec<Replacement> {
    let mut singles = Vec::new();
    for line_number in 1..=2u64 {
        for line in BOUNDED_LINES {
            for range in bounded_ranges(line, 0) {
                for replacement in BOUNDED_REPLACEMENTS {
                    singles.push(make_replacement(
                        path,
                        line_number,
                        line,
                        replacement,
                        range.clone(),
                    ));
                }
            }
        }
    }
    singles
}

/// Every edit set of at most two edits over the bounded tables: by line
/// (lines 1 to 3), and without overlap on one line.
fn bounded_domain_edit_sets(singles: &[Replacement]) -> Vec<Vec<Replacement>> {
    let mut sets: Vec<Vec<Replacement>> = singles.iter().cloned().map(|one| vec![one]).collect();
    for first in singles {
        for replacement in BOUNDED_REPLACEMENTS {
            for range in bounded_ranges(&first.original_line, first.match_range.end) {
                let second = make_replacement(
                    &first.path,
                    first.line_number,
                    &first.original_line,
                    replacement,
                    range,
                );
                sets.push(vec![first.clone(), second]);
            }
            for line_number in first.line_number + 1..=3 {
                for line in BOUNDED_LINES {
                    for range in bounded_ranges(line, 0) {
                        let second =
                            make_replacement(&first.path, line_number, line, replacement, range);
                        sets.push(vec![first.clone(), second]);
                    }
                }
            }
        }
    }
    sets
}

/// Every source of up to `max_len` symbols over `alphabet`, the empty one
/// included.
fn bounded_sources(alphabet: &[&[u8]], max_len: u32) -> Vec<Vec<u8>> {
    let mut sources = vec![Vec::new()];
    for len in 1..=max_len {
        for code in 0..alphabet.len().pow(len) {
            let mut rest = code;
            let mut source = Vec::new();
            for _ in 0..len {
                source.extend_from_slice(alphabet[rest % alphabet.len()]);
                rest /= alphabet.len();
            }
            sources.push(source);
        }
    }
    sources
}

/// One edit set of the bounded check: the reference builder's text and count
/// (`None` when it does not replace, or the source is not UTF-8), and the
/// engine's planned edits.
type PlannedEdits<'a> = (Option<(String, usize)>, Vec<StreamEdit<'a>>);

/// How often the bounded check reached each case the retired engine harness
/// covered with `kani::cover!`; each must be reached at least once.
#[derive(Debug, Default)]
struct BoundedCoverage {
    replaced_runs: usize,
    two_edits_applied: usize,
    limit_stops: usize,
    stale_lines: usize,
    crlf_seams: usize,
    unterminated_final_line_across_a_seam: usize,
    cut_character_in_a_replaced_line: usize,
    invalid_sources: usize,
    invalid_source_split_by_a_seam: usize,
}

/// The one-based line that byte `index` of `source` is in.
fn line_of(source: &[u8], index: usize) -> u64 {
    1 + source[..index]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count() as u64
}

/// Check every edit set, chunk size and output limit of one source.
fn check_bounded_source(
    source: &[u8],
    edit_sets: &[Vec<Replacement>],
    coverage: &mut BoundedCoverage,
) {
    use ReplacementTextOutcome as Outcome;
    let text = std::str::from_utf8(source).ok();
    if text.is_none() {
        coverage.invalid_sources += 1;
    }
    let mut planned: Vec<PlannedEdits<'_>> = vec![(text.map(|t| (t.to_string(), 0)), Vec::new())];
    for set in edit_sets {
        let mut refs: Vec<&Replacement> = set.iter().collect();
        let Ok(edits) = plan_edits(&mut refs, || false) else {
            continue;
        };
        let reference = text.and_then(|text| {
            let mut refs: Vec<&Replacement> = set.iter().collect();
            build_replaced_text_reference(text, &mut refs).ok()
        });
        planned.push((reference, edits));
    }
    let whole_len = source.len().max(1);
    for (reference, edits) in &planned {
        let unlimited = stream_whole(
            source,
            edits,
            whole_len,
            u64::MAX,
            RecordingSink::default(),
            RecordingSink::default(),
        );
        let replaced = matches!(unlimited.0.outcome, Outcome::Replaced { .. });
        if text.is_some() {
            assert_eq!(
                replaced.then(|| (unlimited.1.clone(), unlimited.0.outcome)),
                reference.as_ref().map(|(expected, count)| (
                    expected.as_bytes().to_vec(),
                    Outcome::Replaced {
                        replacement_count: *count
                    }
                )),
                "{source:?} against the reference"
            );
        }
        let mut limits = vec![u64::MAX];
        if let Some(below) = (unlimited.1.len() as u64).checked_sub(1) {
            limits.push(below);
        }
        for limit in limits {
            let whole = stream_whole(
                source,
                edits,
                whole_len,
                limit,
                RecordingSink::default(),
                RecordingSink::default(),
            );
            match whole.0.outcome {
                Outcome::OutputLimitExceeded if limit != u64::MAX => coverage.limit_stops += 1,
                Outcome::StaleLine { .. } => coverage.stale_lines += 1,
                _ => {}
            }
            for chunk in 1..=whole_len {
                let split = stream_whole(
                    source,
                    edits,
                    chunk,
                    limit,
                    RecordingSink::default(),
                    RecordingSink::default(),
                );
                for (summary, out) in [&whole, &split] {
                    assert!(summary.output_len <= limit);
                    assert_eq!(summary.output_len, out.len() as u64);
                    assert_eq!(&summary.output_digest.0, out);
                }
                let seams: Vec<usize> = (chunk..source.len()).step_by(chunk).collect();
                if text.is_none() {
                    // A source that is not UTF-8 is never replaced, at any chunk
                    // size; where it stops may depend on the chunking.
                    for outcome in [whole.0.outcome, split.0.outcome] {
                        assert!(
                            matches!(
                                outcome,
                                Outcome::NotUtf8
                                    | Outcome::StaleLine { .. }
                                    | Outcome::OutputLimitExceeded
                            ),
                            "{source:?} chunk {chunk} limit {limit}: {outcome:?}"
                        );
                    }
                    if edits.is_empty() && limit == u64::MAX {
                        assert_eq!(
                            split.0.outcome,
                            Outcome::NotUtf8,
                            "{source:?} chunk {chunk}"
                        );
                    }
                    if !seams.is_empty() {
                        coverage.invalid_source_split_by_a_seam += 1;
                    }
                    continue;
                }
                // On valid UTF-8 the outcome itself does not depend on the
                // chunking; the bytes written before a stop may.
                assert_eq!(
                    split.0.outcome, whole.0.outcome,
                    "{source:?} chunk {chunk} limit {limit}"
                );
                let Outcome::Replaced { replacement_count } = whole.0.outcome else {
                    continue;
                };
                assert_eq!(split.1, whole.1, "{source:?} chunk {chunk}");
                assert_eq!(split.0.source_digest.0, *source);
                assert_eq!(whole.0.source_digest.0, *source);
                coverage.replaced_runs += 1;
                if replacement_count == 2 {
                    coverage.two_edits_applied += 1;
                }
                if edits.is_empty() {
                    continue;
                }
                let targeted = |index: usize| {
                    let line = line_of(source, index);
                    edits.iter().any(|edit| edit.line_number == line)
                };
                if seams
                    .iter()
                    .any(|&seam| source[seam - 1] == b'\r' && source[seam] == b'\n')
                {
                    coverage.crlf_seams += 1;
                }
                if seams
                    .iter()
                    .any(|&seam| source[seam] & 0xC0 == 0x80 && targeted(seam))
                {
                    coverage.cut_character_in_a_replaced_line += 1;
                }
                if source.last().is_some_and(|&last| last != b'\n') {
                    let final_start = source
                        .iter()
                        .rposition(|&byte| byte == b'\n')
                        .map_or(0, |newline| newline + 1);
                    if targeted(final_start) && seams.iter().any(|&seam| seam > final_start) {
                        coverage.unterminated_final_line_across_a_seam += 1;
                    }
                }
            }
        }
    }
}

/// The bounded exhaustive check that stands in for the engine's Kani harness
/// (design D11, "Kani memory": no engine form that was tried fitted the shard
/// margin). Two domains:
///
/// - every source of at most four characters over `a`, `b`, `\r`, `\n` and
///   `é`, no edit and every edit set of at most two edits from
///   [`bounded_domain_edit_sets`] (recorded lines `""`, `a`, `ab`, `\r`, `aé`;
///   replacements `""`, `x\n`; lines 1 to 3);
/// - every source of at most three symbols over those five plus the single
///   bytes `0x80`, `0xC3` and `0xFF`, no edit and every single edit. Validity
///   is computed per source: `0xC3 0x80` is U+00C0.
///
/// Each point runs every chunk size from 1 to the source length and two
/// output limits: none, and one byte below the whole output. On a valid
/// source the chunked feed's outcome equals the whole feed's, a replaced
/// file's output equals the whole feed's and the reference builder's, the
/// output stays within the limit and the digests record exactly the bytes fed
/// and written. A source that is not UTF-8 is never `Replaced`: it ends
/// `NotUtf8`, `StaleLine` or `OutputLimitExceeded`, and with no edit and no
/// limit `NotUtf8`, at every chunk size. A coverage guard asserts that each
/// case the retired harness covered is reached.
#[test]
fn the_engine_does_not_depend_on_its_chunk_size_over_a_bounded_domain() {
    let path = PathBuf::from("/t.txt");
    let singles = bounded_domain_single_edits(&path);
    let edit_sets = bounded_domain_edit_sets(&singles);
    let valid: [&[u8]; 5] = [b"a", b"b", b"\r", b"\n", "\u{e9}".as_bytes()];
    let mut coverage = BoundedCoverage::default();
    for source in bounded_sources(&valid, 4) {
        check_bounded_source(&source, &edit_sets, &mut coverage);
    }
    let with_invalid: [&[u8]; 8] = [
        b"a",
        b"b",
        b"\r",
        b"\n",
        "\u{e9}".as_bytes(),
        b"\x80",
        b"\xC3",
        b"\xFF",
    ];
    let single_sets: Vec<Vec<Replacement>> = singles.into_iter().map(|one| vec![one]).collect();
    for source in bounded_sources(&with_invalid, 3) {
        check_bounded_source(&source, &single_sets, &mut coverage);
    }
    for (case, count) in [
        ("replaced runs", coverage.replaced_runs),
        ("two edits applied", coverage.two_edits_applied),
        ("the limit stopping a file", coverage.limit_stops),
        ("a stale line", coverage.stale_lines),
        ("a CRLF split by a seam", coverage.crlf_seams),
        (
            "an unterminated final line replaced across a seam",
            coverage.unterminated_final_line_across_a_seam,
        ),
        (
            "a cut character inside a replaced targeted line",
            coverage.cut_character_in_a_replaced_line,
        ),
        ("an invalid source", coverage.invalid_sources),
        (
            "an invalid source split by a seam",
            coverage.invalid_source_split_by_a_seam,
        ),
    ] {
        assert!(
            count > 0,
            "bounded check never reached {case}: {coverage:?}"
        );
    }
}

// --- continuity with `main`'s tests (task 4.10) and recovery (3.5a, 3.5b, 4.11)

/// Copy the journal under `data_dir` (entries, manifest and bodies) into
/// `snapshot`: the disk state a crash at this instant would leave.
fn snapshot_journal(data_dir: &Path, snapshot: &Path) {
    for (from, to) in [
        (
            search_backup::journal_dir(data_dir),
            search_backup::journal_dir(snapshot),
        ),
        (
            search_backup::bodies_dir(data_dir),
            search_backup::bodies_dir(snapshot),
        ),
    ] {
        fixture::create_dir_all(&to);
        for name in fixture::entry_names(&from) {
            let source = from.join(&name);
            if fs_metadata::path_status(&source)
                .is_ok_and(|status| status.is_present() && !status.is_directory())
            {
                fixture::write_bytes(&to.join(&name), fixture::read_bytes(&source));
            }
        }
    }
}

#[test]
fn test_apply_replacements_waits_for_startup_journal_recovery_guard() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
    let recovery_guard =
        search_backup::acquire_journal_guard().expect("simulate startup journal recovery");
    let journal_path = journal.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
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
    assert!(
        rx.recv_timeout(Duration::from_millis(50)).is_err(),
        "Replace All must wait while startup owns journal recovery"
    );
    assert_eq!(fixture::read_text(&file), "needle\n");

    drop(recovery_guard);
    let result = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("replace finishes after the recovery guard drops")
        .expect("replace succeeds");
    worker.join().expect("replace worker joins");
    assert_eq!(result.result.files_affected, 1);
    assert_eq!(fixture::read_text(&file), "replaced\n");
    assert!(
        search_backup::load(journal.path())
            .expect("load active journal")
            .contains_key(&file)
    );
}

#[test]
fn test_apply_replacements_cancel() {
    let dir = tempdir().expect("tempdir");
    let file_a = dir.path().join("a.txt");
    let file_b = dir.path().join("b.txt");
    fixture::write_text(&file_a, "needle\n");
    fixture::write_text(&file_b, "needle\n");
    let journal = tempdir().expect("journal");
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let result = apply_replacements(
        &[
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ],
        &HashSet::new(),
        &AtomicBool::new(true),
        journal.path(),
    );
    assert!(result.is_err(), "a cancelled Replace All aborts");
    assert_eq!(fixture::read_text(&file_a), "needle\n");
    assert_eq!(fixture::read_text(&file_b), "needle\n");
    assert!(
        search_backup::load(journal.path())
            .expect("load")
            .is_empty()
    );

    // A cancel inside the last file, after `a` was replaced: the run still
    // aborts, `b` keeps its bytes and `a` is rolled back.
    let journal = tempdir().expect("journal");
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let cancel_in_hook = cancel.clone();
    super::register_before_target_write_hook_for_test(&file_b, move || {
        cancel_in_hook.store(true, Ordering::Relaxed);
    });
    let result = apply_replacements(
        &[
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ],
        &HashSet::new(),
        &cancel,
        journal.path(),
    );
    assert!(result.is_err(), "a cancel in the last file aborts the run");
    assert_eq!(fixture::read_text(&file_a), "needle\n", "a is rolled back");
    assert_eq!(fixture::read_text(&file_b), "needle\n");
    assert!(
        search_backup::load(journal.path())
            .expect("load")
            .is_empty()
    );
}

/// No target is written when its undo state cannot be made durable: neither
/// when the journal cannot be prepared (so no body is written), nor when the
/// entry cannot be written after its body. `main` aborted the run here; the
/// branch reports the file under `UndoJournalFailed` and leaves it alone.
#[test]
fn test_apply_replacements_aborts_when_undo_journal_cannot_be_persisted() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("test.rs");
    let journal_path = dir.path().join("journal-is-a-file");
    fixture::write_text(&file, "let needle = 42;\n");
    fixture::write_text(&journal_path, "not a directory");
    let replacements = [make_replacement(
        &file,
        1,
        "let needle = 42;",
        "haystack",
        4..10,
    )];
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let unprepared = apply_replacements(
        &replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        &journal_path,
    );
    let unprepared = unprepared.expect("an unpreparable journal is a skip");
    assert_eq!(unprepared.result.files_affected, 0);
    assert_eq!(
        unprepared
            .skip_report
            .count(ReplaceSkipReason::UndoJournalFailed),
        1
    );
    assert!(unprepared.undo_backup.is_empty());
    assert_eq!(
        fixture::read_text(&file),
        "let needle = 42;\n",
        "file bytes stay unchanged when the journal cannot be prepared"
    );

    // The entry's path is taken by a non-empty directory once the body is
    // written: the entry write fails, and the target is never renamed.
    let journal = tempdir().expect("journal");
    let blocked = search_backup::entry_path(journal.path(), &file);
    register_after_pass_a_hook_for_test(&file, move |_| {
        fixture::create_dir_all(&blocked);
        fixture::write_text(&blocked.join("occupied"), "x");
    });
    let outcome = apply_replacements(
        &replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("a failed entry is a skip");
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::UndoJournalFailed),
        1
    );
    assert!(outcome.undo_backup.is_empty());
    assert_eq!(
        fixture::read_text(&file),
        "let needle = 42;\n",
        "file bytes stay unchanged when the entry cannot be saved"
    );
}

/// `main` failed a run whose only file was missing; the branch reports it as
/// `Unreadable` in the skip report (design D7), and writes nothing.
#[test]
fn test_apply_replacements_nonexistent_file() {
    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("does_not_exist.rs");
    let (outcome, journal) = apply(
        &[make_replacement(
            &missing,
            1,
            "phantom line",
            "replacement",
            0..7,
        )],
        &HashSet::new(),
    );
    let outcome = outcome.expect("a missing file is reported, not fatal");
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(outcome.skip_report.count(ReplaceSkipReason::Unreadable), 1);
    assert_eq!(outcome.result.error_count, 1);
    assert!(outcome.undo_backup.is_empty());
    assert!(!fs_metadata::exists(&missing));
    assert!(
        search_backup::load(journal.path())
            .expect("load")
            .is_empty()
    );
}

#[test]
fn test_apply_replacements_skip_paths() {
    let dir = tempdir().expect("tempdir");
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");
    fixture::write_text(&file_a, "needle\n");
    fixture::write_text(&file_b, "needle\n");
    let skip = HashSet::from([file_b.clone()]);
    let (outcome, _journal) = apply(
        &[
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ],
        &skip,
    );
    let outcome = outcome.expect("replace");
    assert_eq!(outcome.result.replaced_count, 1, "only a.rs is replaced");
    assert_eq!(outcome.result.files_affected, 1);
    assert_eq!(outcome.result.skipped_count, 1);
    assert_eq!(
        outcome.result.skipped_sample[0],
        file_b.display().to_string()
    );
    assert_eq!(
        outcome.skip_report.count(ReplaceSkipReason::UnsavedChanges),
        1
    );
    assert!(outcome.undo_backup.contains_key(&file_a));
    assert!(!outcome.undo_backup.contains_key(&file_b));
    assert_eq!(fixture::read_text(&file_b), "needle\n");
}

/// At the test limit: a file exactly at the per-file limit is replaced.
#[test]
fn test_apply_replacements_allows_file_at_exact_size_cap() {
    const LIMIT: u64 = 4096;
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("exact.rs");
    let original_line = "a".repeat(usize::try_from(LIMIT).expect("small"));
    fixture::write_text(&file, &original_line);
    set_max_replace_file_bytes_for_test(Some(LIMIT));
    let (outcome, _journal) = apply(
        &[make_replacement(&file, 1, &original_line, "b", 0..1)],
        &HashSet::new(),
    );
    set_max_replace_file_bytes_for_test(None);
    let outcome = outcome.expect("a file at the limit is replaceable");
    assert_eq!(outcome.result.files_affected, 1);
    assert_eq!(outcome.result.replaced_count, 1);
    assert_eq!(outcome.result.skipped_count, 0);
    assert_eq!(outcome.result.error_count, 0);
    assert_eq!(
        fs_metadata::file_facts(&file)
            .expect("stat replaced file")
            .byte_size,
        LIMIT
    );
    assert!(outcome.undo_backup.contains_key(&file));
}

/// At the test limit: a file one byte over it is skipped from its metadata,
/// before the body admission that precedes every read (P0 before P1 and P2).
/// With no free space at all, any later decision would read
/// `NotEnoughDiskSpace`, and its bytes, which are not UTF-8, would read
/// `NotUtf8`: the reason proves the size check came first.
#[test]
fn test_apply_replacements_skips_over_file_size_cap_before_reading() {
    const LIMIT: u64 = 4096;
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("huge.rs");
    fixture::write_repeated_bytes(&file, b"\xff", LIMIT + 1);
    let journal = tempdir().expect("journal");
    let read = std::sync::Arc::new(AtomicBool::new(false));
    let read_in_hook = read.clone();
    register_after_pass_a_hook_for_test(&file, move |_| {
        read_in_hook.store(true, Ordering::Relaxed);
    });
    set_max_replace_file_bytes_for_test(Some(LIMIT));
    let _free = fs_metadata::fake_free_space_for_test(0);
    let outcome = apply_replacements(
        &[make_replacement(&file, 1, "needle", "replaced", 0..6)],
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    );
    set_max_replace_file_bytes_for_test(None);
    let outcome = outcome.expect("an oversized file is a skip, not fatal");
    assert_eq!(outcome.result.replaced_count, 0);
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(outcome.result.skipped_count, 1);
    assert_eq!(outcome.result.skipped_sample[0], file.display().to_string());
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::LargerThanOpenTier),
        1
    );
    assert!(
        outcome.result.error_sample[0].contains("Replace All limit"),
        "{}",
        outcome.result.error_sample[0]
    );
    assert!(!read.load(Ordering::Relaxed), "no pass A ran");
    assert!(!fs_metadata::exists(&search_backup::body_path(
        journal.path(),
        &file
    )));
    assert!(outcome.undo_backup.is_empty());
}

/// U1's verdict on what a target holds now: replaced (restore it), already
/// original (no write), or changed since the run (never written over); and a
/// missing target fails, keeping its entry.
#[test]
fn rollback_disposition_refuses_a_file_changed_after_replacement() {
    use super::protocol::CurrentContent;
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "original\n");
    let (outcome, _journal) = apply(
        &[make_replacement(&file, 1, "original", "replaced", 0..8)],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    let entry = &backup[&file];
    let verdict = |path: &Path| {
        let facts = fs_metadata::file_facts(path).expect("facts");
        super::undo::current_content(path, entry, &facts)
    };

    assert_eq!(verdict(&file), CurrentContent::Replaced);
    fixture::write_text(&file, "original\n");
    assert_eq!(verdict(&file), CurrentContent::Original);
    // An editor save of a later target can land mid-rollback, because the
    // per-file guard is released as rollback moves on.
    fixture::write_text(&file, "user saved this after Replace All\n");
    assert_eq!(verdict(&file), CurrentContent::Diverged);
    // Same length as the replaced bytes, other content: only the digest tells.
    fixture::write_text(&file, "replaceD\n");
    assert_eq!(verdict(&file), CurrentContent::Diverged);

    fixture::remove_file(&file);
    let rollback = super::undo::rollback_applied_files(&backup);
    assert!(
        rollback.remaining.contains_key(&file),
        "an unreadable target is reported, never written on a guess"
    );
    assert!(!fs_metadata::exists(&file));
}

#[test]
fn rollback_keeps_the_journal_when_a_target_changed_mid_rollback() {
    let dir = tempdir().expect("tempdir");
    let restorable = dir.path().join("restorable.rs");
    let changed = dir.path().join("changed.rs");
    fixture::write_text(&restorable, "original\n");
    fixture::write_text(&changed, "original\n");
    let (outcome, journal) = apply(
        &[
            make_replacement(&restorable, 1, "original", "replaced", 0..8),
            make_replacement(&changed, 1, "original", "replaced", 0..8),
        ],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    // The replaced length, other bytes: only the digest tells it changed.
    fixture::write_text(&changed, "rEplaced\n");

    let rollback = super::undo::rollback_applied_files(&backup);

    assert_eq!(
        fixture::read_text(&restorable),
        "original\n",
        "an unchanged target is still rolled back"
    );
    assert_eq!(
        fixture::read_text(&changed),
        "rEplaced\n",
        "a target changed after replacement is not overwritten"
    );
    assert_eq!(rollback.errors.total_count(), 1);
    assert_eq!(rollback.remaining.len(), 1);
    assert!(rollback.remaining.contains_key(&changed));
    // The unrestored target keeps its entry and body: the journal stays.
    assert!(fs_metadata::exists(&backup[&changed].entry_file));
    assert!(fs_metadata::exists(&search_backup::body_path(
        journal.path(),
        &changed
    )));
    assert!(!fs_metadata::exists(&backup[&restorable].entry_file));
}

/// One target's apply in its own journal directory: the undo descriptors.
fn applied_backup(target: &Path) -> (ReplaceUndoBackup, TempDir) {
    fixture::write_text(target, "before\n");
    let (outcome, journal) = apply(
        &[make_replacement(target, 1, "before", "after", 0..6)],
        &HashSet::new(),
    );
    (outcome.expect("replace").undo_backup, journal)
}

#[test]
fn undo_hooks_fire_only_for_their_own_target_across_interleaved_undos() {
    use std::sync::{Arc, Mutex};
    let dir = tempdir().expect("tempdir");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    let (first_backup, _first_journal) = applied_backup(&first);
    let (second_backup, _second_journal) = applied_backup(&second);
    let fired = Arc::new(Mutex::new(Vec::new()));
    let first_recorder = Arc::clone(&fired);
    let _first_guard = register_undo_after_metadata_hook_for_test(&first, move |path| {
        first_recorder
            .lock()
            .expect("record first hook")
            .push(path.to_path_buf());
    });
    let second_recorder = Arc::clone(&fired);
    let _second_guard = register_undo_after_metadata_hook_for_test(&second, move |path| {
        second_recorder
            .lock()
            .expect("record second hook")
            .push(path.to_path_buf());
    });

    // Undo the second target first: a single-slot seam would let this
    // operation steal and consume the first target's hook.
    assert_eq!(undo_replacements(&second_backup).restored_count(), 1);
    assert_eq!(
        fired.lock().expect("fired after second undo").as_slice(),
        std::slice::from_ref(&second)
    );
    assert!(undo_after_metadata_hook_is_registered_for_test(&first));
    assert!(!undo_after_metadata_hook_is_registered_for_test(&second));

    assert_eq!(undo_replacements(&first_backup).restored_count(), 1);
    assert_eq!(
        fired.lock().expect("fired after both undos").as_slice(),
        &[second, first.clone()]
    );
    assert!(!undo_after_metadata_hook_is_registered_for_test(&first));
}

#[test]
fn parallel_undo_operations_consume_only_their_own_target_hooks() {
    use std::sync::{Arc, Mutex};
    let dir = tempdir().expect("tempdir");
    let fired = Arc::new(Mutex::new(Vec::new()));
    let mut targets = Vec::new();
    let mut backups = Vec::new();
    let mut journals = Vec::new();
    let mut guards = Vec::new();
    for index in 0..8 {
        let target = dir.path().join(format!("target-{index}.txt"));
        let (backup, journal) = applied_backup(&target);
        let recorder = Arc::clone(&fired);
        let owner = target.clone();
        guards.push(register_undo_after_metadata_hook_for_test(
            &target,
            move |path| {
                // A hook that ran for another target's undo is a leak.
                assert_eq!(path, owner.as_path(), "a hook ran for another target");
                recorder
                    .lock()
                    .expect("record parallel hook")
                    .push(path.to_path_buf());
            },
        ));
        targets.push(target);
        backups.push(backup);
        journals.push(journal);
    }

    let leaked = std::thread::scope(|scope| {
        // Every undo is spawned before any is joined, so they overlap.
        let mut undos = Vec::with_capacity(backups.len());
        for backup in &backups {
            undos.push(scope.spawn(move || {
                assert_eq!(undo_replacements(backup).restored_count(), 1);
            }));
        }
        undos
            .into_iter()
            .map(std::thread::ScopedJoinHandle::join)
            .filter(Result::is_err)
            .count()
    });
    assert_eq!(leaked, 0, "every undo ran only its own target's hook");

    let mut fired_paths = fired.lock().expect("parallel fired paths").clone();
    fired_paths.sort();
    assert_eq!(fired_paths, targets);
    for target in &targets {
        assert!(!undo_after_metadata_hook_is_registered_for_test(target));
        assert_eq!(fixture::read_text(target), "before\n");
    }
    eprintln!(
        "replace-undo-hook-isolation-evidence targets={} fired={} leaked=0",
        targets.len(),
        fired_paths.len()
    );
    drop(guards);
}

/// The entry and its body are durable, and the journal active, before the
/// target's rename (P2 and P4 before P5).
#[test]
fn test_apply_replacements_persists_undo_journal_before_success() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "let needle = 42;\n");
    let journal_path = journal.path().to_path_buf();
    let target = file.clone();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let seen_in_hook = seen.clone();
    super::register_before_target_write_hook_for_test(&file, move || {
        let persisted = search_backup::load(&journal_path).expect("load before the rename");
        *seen_in_hook.lock().expect("hook lock") = Some((
            persisted.contains_key(&target),
            fs_metadata::exists(&search_backup::body_path(&journal_path, &target)),
            fixture::read_text(&target),
        ));
    });
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let outcome = apply_replacements(
        &[make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )],
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("replace");
    let (active, body, target_bytes) = seen.lock().expect("seen").take().expect("hook ran");
    assert!(active, "the journal lists the entry before the rename");
    assert!(body, "the body is on disk before the rename");
    assert_eq!(target_bytes, "let needle = 42;\n");
    assert_eq!(
        search_backup::load(journal.path()).expect("load"),
        outcome.undo_backup
    );
}

/// At the test limit: Undo restores a file whose current size is exactly the
/// per-file limit.
#[test]
fn test_undo_replacements_allows_current_file_at_exact_size_cap() {
    const LIMIT: u64 = 4096;
    let dir = tempdir().expect("tempdir");
    let file = sized(dir.path(), "exact-cap.txt", 4096);
    set_max_replace_file_bytes_for_test(Some(LIMIT));
    let (outcome, _journal) = apply(
        &[make_replacement(&file, 1, "needle", "thread", 0..6)],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    assert_eq!(
        fs_metadata::file_facts(&file).expect("facts").byte_size,
        LIMIT
    );

    let undone = undo_replacements(&backup);
    set_max_replace_file_bytes_for_test(None);

    assert_eq!(undone.restored_count(), 1);
    assert_eq!(undone.skipped_count, 0);
    assert_eq!(undone.failed_count, 0);
    assert!(undone.remaining_backup.is_empty());
    assert!(fixture::read_text(&file).starts_with("needle\n"));
}

/// A bounded-read I/O failure during a partial restore (the second file's
/// body is gone) keeps that entry, and only it, for a retry.
#[test]
fn test_undo_replacements_keeps_failed_entries_after_partial_restore() {
    let dir = tempdir().expect("tempdir");
    let restored = dir.path().join("a-restored.rs");
    let failed = dir.path().join("b-failed.rs");
    fixture::write_text(&restored, "before\n");
    fixture::write_text(&failed, "before\n");
    let (outcome, journal) = apply(
        &[
            make_replacement(&restored, 1, "before", "after", 0..6),
            make_replacement(&failed, 1, "before", "after", 0..6),
        ],
        &HashSet::new(),
    );
    let backup = outcome.expect("replace").undo_backup;
    fixture::remove_file(&search_backup::body_path(journal.path(), &failed));

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), 1);
    assert_eq!(undone.failed_count, 1);
    assert_eq!(undone.failed_sample[0], failed.display().to_string());
    assert_eq!(undone.skipped_count, 0);
    assert_eq!(undone.remaining_count(), 1);
    assert_eq!(undone.remaining_backup.get(&failed), backup.get(&failed));
    assert_eq!(fixture::read_text(&restored), "before\n");
    assert_eq!(fixture::read_text(&failed), "after\n");
}

/// Task 3.5a (design D9): a crash between the fourth file's body and its
/// entry leaves an active journal of the first three, whose Undo restores
/// them, and cleanup removes the orphan body. The crash state is the journal
/// copied at the `replace-body-before-entry` window (the after-pass-A hook
/// sits at the same point as that kill point).
#[test]
fn a_crash_between_the_fourth_body_and_its_entry_keeps_undo_for_the_first_three() {
    let dir = tempdir().expect("tempdir");
    let journal = tempdir().expect("journal");
    let crashed = tempdir().expect("crash snapshot");
    let files: Vec<_> = (0..5)
        .map(|index| {
            let file = dir.path().join(format!("f{index}.txt"));
            fixture::write_text(&file, "needle\n");
            file
        })
        .collect();
    let data_dir = journal.path().to_path_buf();
    let snapshot = crashed.path().to_path_buf();
    // At the window the first three targets are replaced and the fourth is
    // not yet: record their bytes as the crash leaves them.
    let at_crash = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let at_crash_in_hook = at_crash.clone();
    let targets = files.clone();
    register_after_pass_a_hook_for_test(&files[3], move |_| {
        snapshot_journal(&data_dir, &snapshot);
        *at_crash_in_hook.lock().expect("crash bytes") = targets
            .iter()
            .map(|file| fixture::read_bytes(file))
            .collect();
    });
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let replacements: Vec<_> = files
        .iter()
        .map(|file| make_replacement(file, 1, "needle", "thread", 0..6))
        .collect();
    let outcome = apply_replacements(
        &replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    );
    outcome.expect("the uncrashed run completes");
    // Put the targets back as the crash left them.
    for (file, bytes) in files.iter().zip(at_crash.lock().expect("bytes").iter()) {
        fixture::write_bytes(file, bytes);
    }
    let orphan = search_backup::body_path(crashed.path(), &files[3]);
    assert!(
        fs_metadata::exists(&orphan),
        "the crash left the fourth body"
    );
    assert!(!fs_metadata::exists(&search_backup::entry_path(
        crashed.path(),
        &files[3]
    )));

    let recovered = search_backup::load_recovering(crashed.path());
    assert!(recovered.active, "an orphan body never costs Undo");
    let backup = recovered.active_backup();
    assert_eq!(
        backup.keys().cloned().collect::<Vec<_>>(),
        files[..3].to_vec()
    );

    // Undo the first two, then retire them: the shrink keeps the third
    // file's entry and body and removes the orphan (design D9).
    let first_two: ReplaceUndoBackup = backup
        .iter()
        .take(2)
        .map(|(path, entry)| (path.clone(), entry.clone()))
        .collect();
    assert_eq!(undo_replacements(&first_two).restored_count(), 2);
    let third: ReplaceUndoBackup = backup
        .iter()
        .skip(2)
        .map(|(path, entry)| (path.clone(), entry.clone()))
        .collect();
    search_backup::shrink_journal_to(crashed.path(), &third).expect("shrink to the third");
    assert!(
        !fs_metadata::exists(&orphan),
        "cleanup removes the orphan body"
    );
    assert!(fs_metadata::exists(&search_backup::body_path(
        crashed.path(),
        &files[2]
    )));
    assert_eq!(undo_replacements(&third).restored_count(), 1);
    for file in &files {
        assert_eq!(fixture::read_text(file), "needle\n");
    }
}

/// Task 3.5b (design D9): a journal of three stream entries whose second body
/// is truncated offers no Undo, copies that entry to the recovery quarantine
/// and writes the cleanup marker, so a relaunch before cleanup offers none
/// either; startup cleanup then removes the journal with every body, and the
/// three targets keep their replaced bytes.
#[test]
fn a_truncated_body_costs_the_runs_undo_and_never_a_files_bytes() {
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
        .map(|file| make_replacement(file, 1, "needle", "thread", 0..6))
        .collect();
    let (outcome, journal) = apply(&replacements, &HashSet::new());
    assert_eq!(outcome.expect("replace").result.files_affected, 3);
    let truncated = search_backup::body_path(journal.path(), &files[1]);
    fixture::write_bytes(&truncated, b"need");

    let first = search_backup::load_recovering(journal.path());
    assert!(!first.active, "no Undo for a run with a damaged body");
    assert!(
        first.diagnostics.iter().any(|diagnostic| diagnostic
            .preservation
            .quarantine_path()
            .is_some_and(fs_metadata::exists)),
        "the damaged entry is preserved in quarantine: {:?}",
        first.diagnostics
    );
    assert!(first.active_backup().is_empty());
    assert!(fs_metadata::exists(
        &search_backup::journal_dir(journal.path()).join("cleanup-in-progress.json")
    ));

    let relaunch = search_backup::load_recovering(journal.path());
    assert!(!relaunch.active, "a relaunch before cleanup offers no Undo");

    assert!(
        search_backup::cleanup_stale(journal.path())
            .diagnostics
            .is_empty()
    );
    assert!(!fs_metadata::exists(&search_backup::journal_dir(
        journal.path()
    )));
    for file in &files {
        assert_eq!(fixture::read_text(file), "thread\n");
        assert!(!fs_metadata::exists(&search_backup::body_path(
            journal.path(),
            file
        )));
    }
}

/// The run of task 4.11: `a` is replaced and then changed by someone else
/// (so rollback keeps it and the journal), and `b` is cancelled in pass B,
/// after its entry and body exist, with `fault` failing one of its cleanup
/// removals. Returns the journal and both paths.
fn cancelled_run_with_failed_cleanup(
    dir: &Path,
    fault: super::CleanupRemovalFault,
) -> (TempDir, PathBuf, PathBuf) {
    let journal = tempdir().expect("journal");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    fixture::write_text(&a, "needle\n");
    fixture::write_text(&b, "needle\n");
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let cancel_in_hook = cancel.clone();
    let a_in_hook = a.clone();
    super::register_before_target_write_hook_for_test(&b, move || {
        fixture::write_text(&a_in_hook, "edited elsewhere\n");
        cancel_in_hook.store(true, Ordering::Relaxed);
    });
    super::fail_next_cleanup_removal_for_test(&b, fault);
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let outcome = apply_replacements(
        &[
            make_replacement(&a, 1, "needle", "thread", 0..6),
            make_replacement(&b, 1, "needle", "thread", 0..6),
        ],
        &HashSet::new(),
        &cancel,
        journal.path(),
    );
    super::clear_cleanup_removal_faults_for_test();
    let outcome = outcome.expect("a rollback that keeps a file reports it");
    assert!(outcome.cancelled);
    assert_eq!(
        outcome.undo_backup.keys().cloned().collect::<Vec<_>>(),
        vec![a.clone()]
    );
    assert_eq!(fixture::read_text(&b), "needle\n", "b keeps its original");
    (journal, a, b)
}

/// Task 4.11 (decision 21): when a cancelled file's entry removal fails, its
/// entry and body both stay, so no entry references a missing body; the
/// journal stays active, and the next Undo finds the target already original
/// (U1) and retires the entry, then the body.
#[test]
fn a_cancelled_file_whose_entry_removal_fails_keeps_both_and_undo_retires_them() {
    let dir = tempdir().expect("tempdir");
    let (journal, a, b) =
        cancelled_run_with_failed_cleanup(dir.path(), super::CleanupRemovalFault::Entry);
    let entry = search_backup::entry_path(journal.path(), &b);
    let body = search_backup::body_path(journal.path(), &b);
    assert!(fs_metadata::exists(&entry));
    assert!(
        fs_metadata::exists(&body),
        "never an entry without its body"
    );

    let recovered = search_backup::load_recovering(journal.path());
    assert!(recovered.active);
    let backup = recovered.active_backup();
    assert!(backup.contains_key(&a) && backup.contains_key(&b));

    let undone = undo_replacements(&backup);
    assert!(!undone.remaining_backup.contains_key(&b));
    assert!(
        undone.remaining_backup.contains_key(&a),
        "a, changed elsewhere, stays"
    );
    assert_eq!(fixture::read_text(&b), "needle\n");
    assert!(!fs_metadata::exists(&entry), "U3 retires the entry");
    assert!(!fs_metadata::exists(&body), "then the body");
}

/// Task 4.11: when the entry removal succeeds and the body removal fails, the
/// body is an orphan: benign, never listed, removed by cleanup (design D9).
#[test]
fn a_cancelled_file_whose_body_removal_fails_leaves_only_an_orphan_body() {
    let dir = tempdir().expect("tempdir");
    let (journal, a, b) =
        cancelled_run_with_failed_cleanup(dir.path(), super::CleanupRemovalFault::Body);
    let body = search_backup::body_path(journal.path(), &b);
    assert!(!fs_metadata::exists(&search_backup::entry_path(
        journal.path(),
        &b
    )));
    assert!(
        fs_metadata::exists(&body),
        "the failed removal left the body"
    );

    let recovered = search_backup::load_recovering(journal.path());
    assert!(recovered.active, "an orphan body never costs Undo");
    let backup = recovered.active_backup();
    assert_eq!(backup.keys().cloned().collect::<Vec<_>>(), vec![a]);
    search_backup::shrink_journal_to(journal.path(), &backup).expect("shrink");
    assert!(
        !fs_metadata::exists(&body),
        "cleanup removes the orphan body"
    );
}

// --- no loss against `main` with little free space (task 4.3, design D4) ----

/// Little free space: far below the 1 GiB reserve, far above what the waived
/// files of these runs need.
const LITTLE_FREE: u64 = 64 * 1024 * 1024;
const MIB: usize = 1024 * 1024;

/// Apply with `free` bytes free on every filesystem and `main`'s undo payload
/// overridden to `undo_budget` when given.
fn apply_with_free(
    replacements: &[Replacement],
    free: u64,
    undo_budget: Option<u64>,
) -> (ApplyReplacementsOutcome, TempDir) {
    let journal = tempdir().expect("journal");
    let _free = fs_metadata::fake_free_space_for_test(free);
    super::TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(undo_budget));
    let outcome = apply_replacements(
        replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    );
    super::TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));
    (outcome.expect("replace"), journal)
}

fn needle_to_thread(files: &[&PathBuf]) -> Vec<Replacement> {
    files
        .iter()
        .map(|file| make_replacement(file, 1, "needle", "thread", 0..6))
        .collect()
}

/// Every file `main` replaced, in both of its passes, is replaced with less
/// free space than the reserve: P1's provisional waiver admits each body and
/// P3's final charge confirms it.
#[test]
fn no_loss_every_file_main_replaced_is_replaced_with_little_free_space() {
    let dir = tempdir().expect("tempdir");
    let small: Vec<_> = (0..3)
        .map(|index| sized(dir.path(), &format!("a-small-{index}.txt"), MIB))
        .collect();
    // 12 MiB: `main` replaced it in its second pass.
    let twelve = sized(dir.path(), "b-twelve.txt", 12 * MIB);
    let all: Vec<_> = small.iter().chain(std::iter::once(&twelve)).collect();
    let (outcome, _journal) = apply_with_free(&needle_to_thread(&all), LITTLE_FREE, None);
    assert_eq!(
        outcome.result.files_affected, 4,
        "{:?}",
        outcome.skip_report
    );
    assert!(outcome.skip_report.is_empty());
    for file in all {
        assert!(fixture::read_text(file).starts_with("thread\n"));
    }
}

/// A file `main` skipped (17 MiB, past its 16 MiB limit) earlier in path
/// order needs the reserve and is skipped, and the files `main` replaced
/// after it are replaced.
#[test]
fn no_loss_a_large_file_early_never_displaces_a_file_main_replaced() {
    let dir = tempdir().expect("tempdir");
    let large = sized(dir.path(), "a-large.txt", 17 * MIB);
    let twelve = sized(dir.path(), "b-twelve.txt", 12 * MIB);
    let small = sized(dir.path(), "c-small.txt", MIB);
    // `main`'s two files exactly fill this payload: 12 + 12 + 1 + 1 MiB.
    let budget = Some(26 * 1024 * 1024);
    let (outcome, _journal) = apply_with_free(
        &needle_to_thread(&[&large, &twelve, &small]),
        LITTLE_FREE,
        budget,
    );
    assert!(outcome.undo_backup.contains_key(&twelve));
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

/// A 9 MiB file whose output grows past 10 MiB is deferred to the second
/// pass, where `main` deferred it, so the smaller file after it in path order
/// is charged first. With a payload that fits only one of them, `main`'s
/// choice wins: the smaller file is replaced and the grown one needs the
/// reserve.
#[test]
fn no_loss_a_small_file_whose_output_grows_past_10_mib_is_deferred_as_on_main() {
    let dir = tempdir().expect("tempdir");
    let grows = sized(dir.path(), "a-grows.txt", 9 * MIB);
    let small = sized(dir.path(), "b-small.txt", MIB);
    let growth = "g".repeat(2 * MIB);
    let replacements = vec![
        make_replacement(&grows, 1, "needle", &growth, 0..6),
        make_replacement(&small, 1, "needle", "thread", 0..6),
    ];
    // The grown file charges 9 + 11 MiB, the small one 2 MiB: 21 MiB holds
    // either alone, not both.
    let (outcome, _journal) = apply_with_free(&replacements, LITTLE_FREE, Some(21 * 1024 * 1024));
    assert!(
        outcome.undo_backup.contains_key(&small),
        "{:?}",
        outcome.skip_report
    );
    assert!(!outcome.undo_backup.contains_key(&grows));
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::NotEnoughDiskSpace),
        1
    );
    assert!(fixture::read_text(&grows).starts_with("needle\n"));
}

/// The other half of the deferral: with room in the payload for both, the
/// grown file is replaced in the second pass with the reserve waived, as
/// `main` replaced it, even with little free space. A first pass that took it
/// at its grown output would find it outside `main`'s 10 MiB and refuse it.
#[test]
fn no_loss_a_small_file_whose_output_grows_past_10_mib_is_replaced_in_the_second_pass() {
    let dir = tempdir().expect("tempdir");
    let grows = sized(dir.path(), "a-grows.txt", 9 * MIB);
    let small = sized(dir.path(), "b-small.txt", MIB);
    let growth = "g".repeat(2 * MIB);
    let replacements = vec![
        make_replacement(&grows, 1, "needle", &growth, 0..6),
        make_replacement(&small, 1, "needle", "thread", 0..6),
    ];
    // 9 + 11 MiB and 2 MiB: 26 MiB holds both.
    let (outcome, _journal) = apply_with_free(&replacements, LITTLE_FREE, Some(26 * 1024 * 1024));
    assert!(
        outcome.undo_backup.contains_key(&small) && outcome.undo_backup.contains_key(&grows),
        "{:?}",
        outcome.skip_report
    );
    assert!(fixture::read_text(&grows).starts_with("ggg"));
}

/// The first pass takes a file of exactly 10 MiB, the boundary itself; only a
/// file past it waits for the second pass. With a payload that holds one of
/// the two, the 10 MiB file wins although a 10 MiB + 1 byte file comes first
/// in path order.
#[test]
fn no_loss_a_file_of_exactly_10_mib_is_replaced_in_the_first_pass() {
    let dir = tempdir().expect("tempdir");
    let past = sized(dir.path(), "a-past.txt", 10 * MIB + 1);
    let exact = sized(dir.path(), "b-exact.txt", 10 * MIB);
    // Each charges its source plus its output, about 20 MiB: 21 MiB holds
    // one of them.
    let (outcome, _journal) = apply_with_free(
        &needle_to_thread(&[&past, &exact]),
        LITTLE_FREE,
        Some(21 * 1024 * 1024),
    );
    assert!(
        outcome.undo_backup.contains_key(&exact),
        "{:?}",
        outcome.skip_report
    );
    assert!(!outcome.undo_backup.contains_key(&past));
    assert!(fixture::read_text(&exact).starts_with("thread\n"));
    assert!(fixture::read_text(&past).starts_with("needle\n"));
}

/// `main`'s test, end to end: an 11 MiB file first in path order, two 1 MiB
/// files after it and a 24 MiB payload. `main` replaced the two small files
/// in its first pass and then had no room for the large one; with little free
/// space the branch does the same.
#[test]
fn no_loss_a_larger_file_never_displaces_a_file_main_replaced() {
    let dir = tempdir().expect("tempdir");
    let large = sized(dir.path(), "a-large.txt", 11 * MIB);
    let small = [
        sized(dir.path(), "b-small-0.txt", MIB),
        sized(dir.path(), "b-small-1.txt", MIB),
    ];
    let (outcome, _journal) = apply_with_free(
        &needle_to_thread(&[&large, &small[0], &small[1]]),
        LITTLE_FREE,
        Some(24 * 1024 * 1024),
    );
    for file in &small {
        assert!(
            outcome.undo_backup.contains_key(file),
            "{:?}",
            outcome.result.error_sample
        );
        assert!(fixture::read_text(file).starts_with("thread\n"));
    }
    assert_eq!(outcome.result.files_affected, 2);
    assert_eq!(outcome.result.skipped_count, 1);
    assert_eq!(
        outcome.result.skipped_sample[0],
        large.display().to_string()
    );
    assert!(fixture::read_text(&large).starts_with("needle\n"));
}

#[test]
fn dropped_hook_guard_removes_unconsumed_registration() {
    let dir = tempdir().expect("tempdir");
    let target = dir.path().join("unconsumed.txt");
    let (backup, _journal) = applied_backup(&target);
    let fired = std::sync::Arc::new(AtomicBool::new(false));
    let hook_fired = fired.clone();
    let guard = register_undo_after_metadata_hook_for_test(&target, move |_| {
        hook_fired.store(true, Ordering::SeqCst);
    });
    assert!(undo_after_metadata_hook_is_registered_for_test(&target));
    drop(guard);
    assert!(!undo_after_metadata_hook_is_registered_for_test(&target));

    assert_eq!(undo_replacements(&backup).restored_count(), 1);
    assert!(
        !fired.load(Ordering::SeqCst),
        "a dropped guard's hook never fires"
    );
}

/// The engine is fed by a bounded stream that checks cancellation before
/// every chunk: a run cancelled after a few dense chunks stops there, before
/// the targeted last line, and never reports a replacement.
#[test]
fn streaming_line_discovery_observes_cancellation_between_dense_chunks() {
    use crate::filesystem::read::{BoundedStreamError, stream_bounded};
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("cancel-dense.txt");
    let original = "x\n".repeat(5_000);
    fixture::write_text(&file, &original);
    let path = file.clone();
    let replacements = [make_replacement(&path, 5_000, "x", "y", 0..1)];
    let mut refs: Vec<&Replacement> = replacements.iter().collect();
    let edits = plan_edits(&mut refs, || false).expect("edits");
    let mut stream =
        super::stream::ReplaceStream::new(&edits, u64::MAX, Vec::new(), NoDigest, NoDigest);
    let checks = std::cell::Cell::new(0usize);
    let streamed = stream_bounded(
        &file,
        u64::MAX,
        64,
        || {
            let next = checks.get() + 1;
            checks.set(next);
            next >= 4
        },
        |chunk| stream.feed(chunk),
    );
    assert!(matches!(streamed, Err(BoundedStreamError::Cancelled)));
    let (summary, out) = stream.finish().expect("a Vec sink cannot fail");
    assert_eq!(summary.source_len, 3 * 64, "three chunks before the cancel");
    assert!(summary.metrics.source_lines < 5_000);
    assert!(!matches!(
        summary.outcome,
        ReplacementTextOutcome::Replaced { .. }
    ));
    assert!(out.len() <= 3 * 64);
}

#[test]
fn test_apply_replacements_reverse_order() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "ab cd\nef gh\n");
    // Listed last edit first: the planner sorts them into source order.
    let (outcome, _journal) = apply(
        &[
            make_replacement(&file, 2, "ef gh", "UV", 3..5),
            make_replacement(&file, 1, "ab cd", "ZW", 3..5),
            make_replacement(&file, 1, "ab cd", "XY", 0..2),
        ],
        &HashSet::new(),
    );
    assert_eq!(outcome.expect("replace").result.replaced_count, 3);
    assert_eq!(fixture::read_text(&file), "XY ZW\nef UV\n");
}

/// A file that changed size after the run is skipped from its metadata,
/// before its bytes are hashed. The after-metadata hook then puts the replaced
/// bytes back: a check that read the file instead of trusting U0's size would
/// see them, try to restore, and fail on the changed facts rather than skip.
#[test]
fn test_undo_replacements_skips_externally_grown_file_before_reading() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("test.rs");
    let (backup, _journal) = applied_backup(&file);
    fixture::write_text(&file, "after plus a large external edit\n");
    let _hook = register_undo_after_metadata_hook_for_test(&file, |path| {
        fixture::write_text(path, "after\n");
    });

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.restored_count(), 0);
    assert_eq!(outcome.skipped_count, 1);
    assert_eq!(outcome.skipped_sample[0], file.display().to_string());
    assert_eq!(outcome.failed_count, 0);
    assert_eq!(outcome.remaining_backup, backup);
    assert_eq!(fixture::read_text(&file), "after\n");
}

/// `main`'s undo payload, exactly full, still admits the file: with little
/// free space its reserve is waived (design D4).
#[test]
fn test_apply_replacements_allows_entry_at_exact_undo_cap() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("exact-undo.rs");
    fixture::write_text(&file, "needle\n");
    let exact = u64::try_from("needle\n".len() + "done\n".len()).expect("tiny");
    let (outcome, _journal) = apply_with_free(
        &[make_replacement(&file, 1, "needle", "done", 0..6)],
        LITTLE_FREE,
        Some(exact),
    );
    assert_eq!(outcome.result.files_affected, 1);
    assert_eq!(fixture::read_text(&file), "done\n");
}

/// One byte over `main`'s undo payload: `main` skipped the file, so it needs
/// the reserve, and with little free space it is skipped without a write.
#[test]
fn test_apply_replacements_rejects_entry_one_byte_over_undo_cap_without_write() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("one-over-undo.rs");
    fixture::write_text(&file, "needle\n");
    let exact = u64::try_from("needle\n".len() + "done\n".len()).expect("tiny");
    let (outcome, journal) = apply_with_free(
        &[make_replacement(&file, 1, "needle", "done", 0..6)],
        LITTLE_FREE,
        Some(exact - 1),
    );
    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(outcome.result.replaced_count, 0);
    assert_eq!(outcome.result.skipped_count, 1);
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::NotEnoughDiskSpace),
        1
    );
    assert!(outcome.undo_backup.is_empty());
    assert_eq!(outcome.metrics.undo_live_bytes, 0);
    assert_eq!(fixture::read_text(&file), "needle\n");
    assert!(!fs_metadata::exists(&search_backup::body_path(
        journal.path(),
        &file
    )));
}

/// Design D6: an Undo in a later launch than the manifest records re-arms the
/// manifest for this launch before it restores, so the journal's owner is the
/// launch whose restore temps may be left beside a target. The first entry
/// is already original and restores nothing: the re-arm is still owed to the
/// next one, which restores. The third has diverged, so the journal stays.
#[test]
fn undo_in_a_later_launch_rearms_the_manifest_for_this_launch() {
    use crate::filesystem::leftovers::LaunchId;
    let dir = tempdir().expect("tempdir");
    let files = ["a.txt", "b.txt", "c.txt"].map(|name| dir.path().join(name));
    for file in &files {
        fixture::write_text(file, "needle\n");
    }
    let replacements: Vec<_> = files
        .iter()
        .map(|file| make_replacement(file, 1, "needle", "thread", 0..6))
        .collect();
    let (outcome, journal) = apply(&replacements, &HashSet::new());
    let mut backup = outcome.expect("replace").undo_backup;
    let current = LaunchId::current();
    let earlier = LaunchId {
        pid: current.pid.wrapping_add(1),
        nonce: current.nonce.wrapping_add(1),
    };
    // The journal as an earlier launch left it.
    search_backup::mark_incremental_journal_active(journal.path(), earlier).expect("re-own");
    for entry in backup.values_mut() {
        entry.launch = earlier;
    }
    let manifest_launch = || {
        let text = fixture::read_text(&journal.path().join("replace-backup-journal/manifest.json"));
        let value: serde_json::Value = serde_json::from_str(&text).expect("manifest json");
        fn find(value: &serde_json::Value, key: &str) -> Option<u64> {
            match value {
                serde_json::Value::Object(map) => map
                    .get(key)
                    .and_then(serde_json::Value::as_u64)
                    .or_else(|| map.values().find_map(|v| find(v, key))),
                _ => None,
            }
        }
        (find(&value, "pid"), find(&value, "launch_nonce"))
    };
    assert_eq!(
        manifest_launch(),
        (Some(u64::from(earlier.pid)), Some(u64::from(earlier.nonce)))
    );
    fixture::write_text(&files[0], "needle\n");
    fixture::write_text(&files[2], "thrEad\n");

    let undone = undo_replacements(&backup);

    assert_eq!(undone.restored_count(), 2);
    assert_eq!(undone.skipped_count, 1);
    assert_eq!(fixture::read_text(&files[1]), "needle\n");
    assert_eq!(
        manifest_launch(),
        (Some(u64::from(current.pid)), Some(u64::from(current.nonce))),
        "the later launch's Undo owns the journal it restores through"
    );
}

/// A file `main` did not admit keeps the reserve on the target filesystem
/// for its *output*: with free space enough for its source plus the reserve
/// but not for its larger output plus the reserve, it is skipped for disk
/// space and left alone, although the journal filesystem keeps the reserve.
#[test]
fn a_growing_file_main_skipped_needs_the_reserve_beyond_its_output() {
    let dir = tempdir().expect("tempdir");
    // 17 MiB is past `main`'s 16 MiB per-file limit: never waived.
    let large = sized(dir.path(), "a-large.txt", 17 * MIB);
    let growth = "x".repeat(1024);
    let journal = tempdir().expect("journal");
    let source_len = 17 * 1024 * 1024;
    // Enough for the source and the reserve, not for the output and the
    // reserve: the two admission checks of the output stage disagree.
    let _free =
        fs_metadata::fake_free_space_for_test(REPLACE_DISK_RESERVE_BYTES + source_len + 512);
    let outcome = apply_replacements(
        &[make_replacement(&large, 1, "needle", &growth, 0..6)],
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("a skipped file is not a failed run");
    assert!(outcome.undo_backup.is_empty());
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::NotEnoughDiskSpace),
        1,
        "{:?}",
        outcome.skip_report
    );
    assert!(fixture::read_text(&large).starts_with("needle\n"));
}

/// A pass A whose body cannot be renamed into place (here a non-empty
/// directory occupies the body's path) fails before rename with no stop
/// reason and no cancel: the file is skipped as an undo-journal failure, not
/// treated as a cancelled run, and the files before it stay replaced.
#[test]
fn a_body_that_cannot_be_installed_skips_its_file_without_cancelling_the_run() {
    let dir = tempdir().expect("tempdir");
    let first = dir.path().join("a-first.txt");
    let second = dir.path().join("b-second.txt");
    fixture::write_text(&first, "needle\n");
    fixture::write_text(&second, "needle\n");
    let journal = tempdir().expect("journal");
    let blocked_body = search_backup::body_path(journal.path(), &second);
    register_after_pass_a_hook_for_test(&first, move |_| {
        fixture::create_dir_all(&blocked_body.join("occupied"));
    });
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let outcome = apply_replacements(
        &[
            make_replacement(&first, 1, "needle", "thread", 0..6),
            make_replacement(&second, 1, "needle", "thread", 0..6),
        ],
        &HashSet::new(),
        &AtomicBool::new(false),
        journal.path(),
    )
    .expect("a body failure is a skip, not a cancelled run");
    assert_eq!(fixture::read_text(&first), "thread\n");
    assert_eq!(fixture::read_text(&second), "needle\n");
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::UndoJournalFailed),
        1,
        "{:?}",
        outcome.skip_report
    );
    assert!(outcome.undo_backup.contains_key(&first));
    assert!(!outcome.undo_backup.contains_key(&second));
}

// --- survivors only a before-action hook reaches (D8) ---------------------

/// The tap's before-action hook at this file's `WriteBody`.
fn before_write_body(
    hook: impl FnOnce() + 'static,
) -> crate::filesystem::backend_tap::TapGuard {
    use crate::filesystem::backend_tap::{self as tap, ActionAt, ProtocolKind, TapPlan};
    tap::install(TapPlan::record().before_action(
        ActionAt::Named {
            kind: ProtocolKind::ReplaceFile,
            name: "WriteBody",
            nth: 0,
        },
        hook,
    ))
}

/// Mutant 649 (`||` to `&&` in pass A's facts check): a target rewritten
/// with the same bytes and a new mtime after the target was stat'ed is
/// caught by pass A, so its entry and target writes never run. With equal
/// sizes only the mtime half of the check fires; the mutant lets pass A pass
/// and leaves the change to pass B.
#[test]
fn a_same_size_change_before_the_body_write_is_caught_by_pass_a() {
    use crate::filesystem::backend_tap::ProtocolKind;
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("touched.txt");
    fixture::write_text(&file, "needle\n");
    fixture::set_modified(
        &file,
        std::time::SystemTime::now() - std::time::Duration::from_secs(3600),
    );
    let touched = file.clone();
    let guard = before_write_body(move || {
        // The same bytes, so pass A's dry run sees no stale line.
        fixture::write_text(&touched, "needle\n");
        fixture::set_modified(&touched, std::time::SystemTime::now());
    });
    let (outcome, journal) = apply(
        &[make_replacement(&file, 1, "needle", "thread", 0..6)],
        &HashSet::new(),
    );
    let trace = guard.finish();
    let outcome = outcome.expect("a changed file is a skip");
    assert_eq!(fixture::read_text(&file), "needle\n");
    assert_eq!(
        outcome
            .skip_report
            .count(ReplaceSkipReason::ChangedDuringReplace),
        1
    );
    let actions: Vec<&str> = trace
        .steps
        .iter()
        .filter(|step| step.kind == ProtocolKind::ReplaceFile)
        .map(|step| step.action_name.as_str())
        .collect();
    assert!(actions.contains(&"WriteBody"), "{actions:?}");
    assert!(
        !actions.contains(&"WriteEntry") && !actions.contains(&"WriteTarget"),
        "pass A, not pass B, must refuse the changed file: {actions:?}"
    );
    assert!(!fs_metadata::exists(&search_backup::body_path(
        journal.path(),
        &file
    )));
}

/// Mutants 689 (`||` to `&&`) and 691 (delete `!` before `verdict.defer`):
/// a cancel raised just before the body write, whose write then fails
/// before its producer runs, is reported as a cancel. The stream never sees
/// the flag, so only the raised flag can make the outcome `Cancelled`; both
/// mutants report the file as failed and let the run finish.
#[test]
fn a_cancel_before_a_body_write_that_fails_early_cancels_the_run() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("cancelled.txt");
    fixture::write_text(&file, "needle\n");
    let journal = tempdir().expect("journal");
    let bodies = search_backup::bodies_dir(journal.path());
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let raised = std::sync::Arc::clone(&cancel);
    let guard = before_write_body(move || {
        raised.store(true, Ordering::Relaxed);
        // A file where the bodies directory was: the body write's metadata
        // probe fails, before its producer reads anything.
        fixture::remove_dir_all(&bodies);
        fixture::write_text(&bodies, "not a directory\n");
    });
    let _free = fs_metadata::fake_free_space_for_test(AMPLE_FREE);
    let result = apply_replacements(
        &[make_replacement(&file, 1, "needle", "thread", 0..6)],
        &HashSet::new(),
        &cancel,
        journal.path(),
    );
    drop(guard);
    match result {
        Err(error) => assert!(error.to_string().contains("cancelled"), "{error}"),
        Ok(outcome) => assert!(
            outcome.cancelled,
            "the raised cancel must cancel the run, not skip the file: {:?}",
            outcome.skip_report
        ),
    }
    assert_eq!(fixture::read_text(&file), "needle\n");
}

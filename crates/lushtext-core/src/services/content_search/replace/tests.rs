// SPDX-License-Identifier: GPL-3.0-or-later

use super::apply::{ReplaceWriteError, persist_undo_backup, record_replacement_success_counts};
use super::stream::{
    PendingEdit, ReplacementTextOutcome, build_replaced_text, build_replaced_text_reference,
    line_spans_reference, replaced_capacity,
};
use super::undo::{RollbackDisposition, rollback_applied_files, rollback_file_disposition};
use super::*;
use crate::model::content_search::{MAX_REPLACE_PREVIEW_ROWS, Replacement};
use crate::services::filesystem::fixture;
use crate::services::filesystem::{metadata as fs_metadata, read as fs_read, write as fs_write};
use crate::services::search_backup;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tempfile::tempdir;

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

#[test]
fn replace_all_byte_budgets_match_documented_limits() {
    assert_eq!(MAX_REPLACE_FILE_BYTES, 16 * 1024 * 1024);
    assert_eq!(MAX_REPLACE_UNDO_BYTES, 64 * 1024 * 1024);
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
        assert!(replace_before_rename_failure_is_armed_for_test(&second));

        // Consuming one target's seam must not clobber the other's.
        assert!(take_replace_before_rename_failure_for_test(&first));
        assert!(!replace_before_rename_failure_is_armed_for_test(&first));
        assert!(replace_before_rename_failure_is_armed_for_test(&second));
        // Consuming the same target twice is a no-op.
        assert!(!take_replace_before_rename_failure_for_test(&first));

        drop(first_seam);
        drop(second_seam);
    }

    // Per-target assertions (rather than whole-registry emptiness) stay
    // race-free when other tests share the process-global registry.
    assert!(!replace_before_rename_failure_is_armed_for_test(&first));
    assert!(!replace_before_rename_failure_is_armed_for_test(&second));
}

#[test]
fn undo_retained_weight_charges_table_paths_and_vector_capacities() {
    let mut backup = ReplaceUndoBackup::new();
    let mut path = PathBuf::from("/workspace/retained.rs");
    path.reserve(8 * 1024);
    let path_capacity = path.capacity();
    let mut original_bytes = Vec::with_capacity(16 * 1024);
    original_bytes.extend_from_slice(b"before");
    let original_capacity = original_bytes.capacity();
    let mut replaced_bytes = Vec::with_capacity(32 * 1024);
    replaced_bytes.extend_from_slice(b"after");
    let replaced_capacity = replaced_bytes.capacity();
    backup.insert(path, ReplaceUndoEntry::new(original_bytes, replaced_bytes));

    let nested_capacity = path_capacity
        .saturating_add(original_capacity)
        .saturating_add(replaced_capacity);
    assert!(
        replace_undo_retained_byte_weight(&backup)
            > u64::try_from(nested_capacity).unwrap_or(u64::MAX),
        "the hash table allocation must be charged in addition to nested owners"
    );
}

#[test]
fn replacement_count_cap_is_rejected_before_any_target_mutation() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("count-cap.txt");
    fixture::write_text(&file, "needle\n");
    let replacement = make_replacement(&file, 1, "needle", "thread", 0..6);
    let replacements = vec![replacement; MAX_REPLACE_PREVIEW_ROWS + 1];
    let cancel = AtomicBool::new(false);

    let error = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect_err("over-cap replacement selections must be rejected");

    assert!(error.to_string().contains("replacement limit"));
    assert_eq!(fixture::read_text(&file), "needle\n");
}

#[test]
fn test_apply_replacements_literal() {
    let dir = tempdir().expect("expected operation to succeed");
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");
    fixture::write_text(&file_a, "let hello = 1;\nlet world = 2;\n");
    fixture::write_text(&file_b, "fn hello() {}\n");

    let replacements = vec![
        make_replacement(&file_a, 1, "let hello = 1;", "goodbye", 4..9),
        make_replacement(&file_b, 1, "fn hello() {}", "goodbye", 3..8),
    ];

    let cancel = AtomicBool::new(false);
    let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("expected operation to succeed")
        .into_parts();

    assert_eq!(result.replaced_count, 2);
    assert_eq!(result.files_affected, 2);
    assert_eq!(result.skipped_count, 0);

    let content_a = fixture::read_text(&file_a);
    assert!(
        content_a.contains("goodbye"),
        "a.rs should have replacement"
    );
    assert!(
        !content_a.contains("hello"),
        "a.rs should not have original"
    );

    let content_b = fixture::read_text(&file_b);
    assert!(
        content_b.contains("goodbye"),
        "b.rs should have replacement"
    );

    assert_eq!(backup.len(), 2, "backup should contain both files");
}

#[test]
fn test_apply_replacements_preserves_backup() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    let original = "let needle = 42;\n";
    fixture::write_text(&file, original);

    let replacements = vec![make_replacement(
        &file,
        1,
        "let needle = 42;",
        "haystack",
        4..10,
    )];

    let cancel = AtomicBool::new(false);
    let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("expected operation to succeed")
        .into_parts();

    assert_eq!(backup[&file].original_bytes, original.as_bytes());
    assert_eq!(
        backup[&file].replaced_bytes,
        b"let haystack = 42;\n".as_slice()
    );
}

#[test]
fn test_apply_replacements_persists_undo_journal_before_success() {
    let dir = tempdir().expect("expected operation to succeed");
    let journal_dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "let needle = 42;\n");

    let replacements = vec![make_replacement(
        &file,
        1,
        "let needle = 42;",
        "haystack",
        4..10,
    )];

    TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE.with(|required| required.set(true));
    let cancel = AtomicBool::new(false);
    let (_, backup) = apply_replacements(
        &replacements,
        &HashSet::new(),
        &cancel,
        Some(journal_dir.path()),
    )
    .expect("expected operation to succeed")
    .into_parts();
    TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE.with(|required| required.set(false));

    let persisted = search_backup::load(journal_dir.path()).expect("expected operation to succeed");
    assert_eq!(persisted, backup);
}

#[test]
fn post_rename_durability_failure_keeps_changed_bytes_and_undo_evidence() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("durability-ambiguous.txt");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "thread", 0..6)];
    let cancel = AtomicBool::new(false);
    crate::services::filesystem::write::fail_next_parent_sync_for_test();

    let outcome = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("post-rename durability ambiguity still changed the target");
    let (result, backup) = outcome.into_parts();

    assert_eq!(fixture::read_text(&file), "thread\n");
    assert_eq!(result.replaced_count, 1);
    assert_eq!(result.files_affected, 1);
    assert_eq!(result.error_count, 1);
    assert!(result.error_sample[0].contains("durability sync failed"));
    assert_eq!(backup[&file].original_bytes, b"needle\n");
    assert_eq!(backup[&file].replaced_bytes, b"thread\n");
}

#[test]
fn stale_reserved_apply_cannot_overwrite_newer_committed_journal_or_file() {
    let dir = tempdir().expect("replace target tempdir");
    let journal_dir = tempdir().expect("replace journal tempdir");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "stale", 0..6)];

    let newer_path = dir.path().join("newer.rs");
    let mut newer_backup = ReplaceUndoBackup::new();
    newer_backup.insert(
        newer_path,
        ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec()),
    );
    search_backup::save(journal_dir.path(), &newer_backup)
        .expect("commit newer replacement journal");

    let generation = Arc::new(AtomicU32::new(2));
    let stale = ReplaceJournalFreshness::new(generation, 1);
    let cancel = AtomicBool::new(false);
    let outcome = apply_replacements_if_current(
        &replacements,
        &HashSet::new(),
        &HashSet::new(),
        &cancel,
        journal_dir.path(),
        &stale,
    )
    .expect("stale apply freshness check");

    assert!(outcome.is_none());
    assert_eq!(fixture::read_text(&file), "needle\n");
    assert_eq!(
        search_backup::load(journal_dir.path()).expect("load newer journal"),
        newer_backup
    );
}

#[test]
fn test_apply_replacements_waits_for_existing_save_guard_on_same_target() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
    let save_guard =
        fs_write::TargetWriteGuard::acquire(&file).expect("simulate in-flight save guard");
    let (tx, rx) = mpsc::channel();

    let worker = std::thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);
        tx.send(result).expect("send replace result");
    });

    assert!(
        rx.recv_timeout(Duration::from_millis(50)).is_err(),
        "Replace All should wait while an editor save holds the stable target guard"
    );
    assert_eq!(
        fixture::read_text(&file),
        "needle\n",
        "Replace All must not read/write through the held save guard"
    );

    drop(save_guard);
    let result = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("replace should finish after save guard drops")
        .expect("replace should succeed");
    worker.join().expect("replace worker should join");

    assert_eq!(result.result.files_affected, 1);
    assert_eq!(fixture::read_text(&file), "replaced\n");
}

#[test]
fn test_apply_replacements_waits_for_startup_journal_recovery_guard() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempdir().expect("replace target tempdir");
    let journal_dir = tempdir().expect("replace journal tempdir");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
    let recovery_guard =
        search_backup::acquire_journal_guard().expect("simulate startup journal recovery");
    let journal_path = journal_dir.path().to_path_buf();
    let (tx, rx) = mpsc::channel();

    let worker = std::thread::spawn(move || {
        let cancel = AtomicBool::new(false);
        let result =
            apply_replacements(&replacements, &HashSet::new(), &cancel, Some(&journal_path));
        tx.send(result).expect("send replace result");
    });

    assert!(
        rx.recv_timeout(Duration::from_millis(50)).is_err(),
        "Replace All must wait while startup owns journal recovery"
    );
    assert_eq!(fixture::read_text(&file), "needle\n");

    drop(recovery_guard);
    let result = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("replace should finish after recovery guard drops")
        .expect("replace should succeed");
    worker.join().expect("replace worker should join");

    assert_eq!(result.result.files_affected, 1);
    assert_eq!(fixture::read_text(&file), "replaced\n");
    assert!(
        search_backup::load(journal_dir.path())
            .expect("load active journal")
            .contains_key(&file)
    );
}

#[test]
fn test_apply_replacements_skips_over_file_size_cap_before_reading() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("huge.rs");
    fixture::create_sparse_file(&file, MAX_REPLACE_FILE_BYTES + 1);

    let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];
    let cancel = AtomicBool::new(false);
    let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("oversized files should be reported as skipped, not fatal")
        .into_parts();

    assert_eq!(result.replaced_count, 0);
    assert_eq!(result.files_affected, 0);
    assert_eq!(result.skipped_count, 1);
    assert_eq!(result.skipped_sample[0], file.display().to_string());
    assert!(
        result.error_sample[0].contains("larger than the 16 MiB Replace All limit"),
        "{}",
        result.error_sample[0]
    );
    assert!(backup.is_empty());
}

/// No loss against `main` (`power-of-two-thresholds` D12): `main` replaced
/// in files up to 10 MiB, and a run of files `main` processed in full (each
/// at most 10 MiB, the undo payload within 64 MiB) is processed in full.
#[test]
fn no_loss_replace_all_processes_every_file_main_processed() {
    let dir = tempdir().expect("expected operation to succeed");
    let ten_mib = 10 * 1024 * 1024;
    let files: Vec<_> = (0..3)
        .map(|index| {
            let file = dir.path().join(format!("ten-mib-{index}.txt"));
            let mut content = "needle\n".to_string();
            content.push_str(&"a".repeat(ten_mib - content.len()));
            fixture::write_text(&file, &content);
            file
        })
        .collect();
    let replacements: Vec<_> = files
        .iter()
        .map(|file| make_replacement(file, 1, "needle", "thread", 0..6))
        .collect();
    let cancel = AtomicBool::new(false);
    let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("a run main processed in full")
        .into_parts();

    assert_eq!(result.files_affected, 3);
    assert_eq!(result.skipped_count, 0, "{:?}", result.error_sample);
    assert_eq!(result.error_count, 0);
    for file in &files {
        assert!(backup.contains_key(file));
        assert!(fixture::read_text(file).starts_with("thread\n"));
    }
    assert_eq!(MAX_REPLACE_FILE_BYTES, 16 * 1024 * 1024);
}

/// No loss against `main` when the undo payload runs out: `main` skipped a
/// file above 10 MiB without charging it, so a later file of `main`'s size
/// still fit. Files `main` could replace are replaced first, so one of
/// 10–16 MiB earlier in path order never takes their budget.
#[test]
fn no_loss_a_larger_file_never_displaces_a_file_main_replaced() {
    let dir = tempdir().expect("expected operation to succeed");
    let mib = 1024 * 1024;
    let sized = |name: &str, len: usize| {
        let file = dir.path().join(name);
        let mut content = "needle\n".to_string();
        content.push_str(&"a".repeat(len - content.len()));
        fixture::write_text(&file, &content);
        file
    };
    // Path order puts the 11 MiB file first.
    let large = sized("a-large.txt", 11 * mib);
    let small = [sized("b-small-0.txt", mib), sized("b-small-1.txt", mib)];
    let replacements: Vec<_> = std::iter::once(&large)
        .chain(small.iter())
        .map(|file| make_replacement(file, 1, "needle", "thread", 0..6))
        .collect();
    // 24 MiB of undo: the large file's 22 MiB plus one small file's 2 MiB,
    // or both small files and nothing else.
    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(24 * 1024 * 1024)));
    let cancel = AtomicBool::new(false);
    let outcome = apply_replacements(&replacements, &HashSet::new(), &cancel, None);
    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));
    let (result, backup) = outcome.expect("a bounded run").into_parts();

    for file in &small {
        assert!(backup.contains_key(file), "{:?}", result.error_sample);
        assert!(fixture::read_text(file).starts_with("thread\n"));
    }
    assert_eq!(result.files_affected, 2);
    assert_eq!(result.skipped_count, 1);
    assert_eq!(result.skipped_sample[0], large.display().to_string());
    assert!(
        result.error_sample[0].contains("undo data would exceed"),
        "{:?}",
        result.error_sample
    );
    assert!(fixture::read_text(&large).starts_with("needle\n"));
}

#[test]
fn test_apply_replacements_allows_file_at_exact_size_cap() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("exact.rs");
    let original_line = "a".repeat(
        usize::try_from(MAX_REPLACE_FILE_BYTES).expect("replace file byte cap should fit in usize"),
    );
    fixture::write_text(&file, &original_line);
    let replacements = vec![make_replacement(&file, 1, &original_line, "b", 0..1)];
    let cancel = AtomicBool::new(false);

    let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("exact file-size cap should still be replaceable")
        .into_parts();

    assert_eq!(result.files_affected, 1);
    assert_eq!(result.replaced_count, 1);
    assert_eq!(result.skipped_count, 0);
    assert_eq!(result.error_count, 0);
    assert_eq!(
        fs_metadata::file_facts(&file)
            .expect("stat replaced file")
            .byte_size,
        MAX_REPLACE_FILE_BYTES
    );
    assert!(backup.contains_key(&file));
}

#[test]
fn test_apply_replacements_skips_file_that_would_exceed_undo_cap() {
    let dir = tempdir().expect("expected operation to succeed");
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");
    fixture::write_text(&file_a, "needle-a\n");
    fixture::write_text(&file_b, "needle-b\n");

    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(24)));
    let replacements = vec![
        make_replacement(&file_a, 1, "needle-a", "done-a", 0..8),
        make_replacement(&file_b, 1, "needle-b", "done-b", 0..8),
    ];
    let cancel = AtomicBool::new(false);
    let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("undo cap should skip later files without failing prior writes")
        .into_parts();
    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

    assert_eq!(result.files_affected, 1);
    assert_eq!(result.skipped_count, 1);
    assert_eq!(result.skipped_sample[0], file_b.display().to_string());
    assert!(
        result.error_sample[0].contains("undo data would exceed the 64 MiB Replace All limit"),
        "{}",
        result.error_sample[0]
    );
    assert_eq!(fixture::read_text(&file_a), "done-a\n");
    assert_eq!(fixture::read_text(&file_b), "needle-b\n");
    assert!(backup.contains_key(&file_a));
    assert!(!backup.contains_key(&file_b));
}

#[test]
fn test_apply_replacements_allows_entry_at_exact_undo_cap() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("exact-undo.rs");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "done", 0..6)];
    let exact_payload = u64::try_from("needle\n".len() + "done\n".len())
        .expect("tiny undo payload should fit in u64");

    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(exact_payload)));
    let cancel = AtomicBool::new(false);
    let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("exact undo cap should still be replaceable")
        .into_parts();
    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

    assert_eq!(result.files_affected, 1);
    assert_eq!(result.replaced_count, 1);
    assert_eq!(result.skipped_count, 0);
    assert_eq!(result.error_count, 0);
    assert_eq!(fixture::read_text(&file), "done\n");
    assert!(backup.contains_key(&file));
}

#[test]
fn test_apply_replacements_rejects_entry_one_byte_over_undo_cap_without_write() {
    let dir = tempdir().expect("replace tempdir");
    let file = dir.path().join("one-over-undo.rs");
    fixture::write_text(&file, "needle\n");
    let replacements = vec![make_replacement(&file, 1, "needle", "done", 0..6)];
    let exact_payload = u64::try_from("needle\n".len() + "done\n".len())
        .expect("tiny undo payload should fit in u64");

    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(exact_payload - 1)));
    let outcome = apply_replacements(
        &replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        None,
    )
    .expect("one-byte-over undo payload should be skipped safely");
    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

    assert_eq!(outcome.result.files_affected, 0);
    assert_eq!(outcome.result.replaced_count, 0);
    assert_eq!(outcome.result.skipped_count, 1);
    assert_eq!(outcome.result.error_count, 1);
    assert!(outcome.result.error_sample[0].contains("undo data would exceed"));
    assert!(outcome.undo_backup.is_empty());
    assert_eq!(outcome.metrics.undo_live_bytes, 0);
    assert_eq!(fixture::read_text(&file), "needle\n");
}

#[test]
fn pre_rename_failure_reclaims_live_charge_and_allows_later_sorted_target() {
    let dir = tempdir().expect("replace tempdir");
    let journal_dir = tempdir().expect("journal tempdir");
    let first = dir.path().join("a-first.txt");
    let later = dir.path().join("b-later.txt");
    fixture::write_text(&first, "needle\n");
    fixture::write_text(&later, "needle\n");
    let replacements = vec![
        make_replacement(&first, 1, "needle", "done", 0..6),
        make_replacement(&later, 1, "needle", "done", 0..6),
    ];
    let one_entry =
        u64::try_from("needle\n".len() + "done\n".len()).expect("fixture payload fits u64");
    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(one_entry)));
    let _before_rename_seam = fail_next_replace_before_rename_for_path_for_test(&first);

    let outcome = apply_replacements(
        &replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        Some(journal_dir.path()),
    )
    .expect("later target should proceed after reclaim");
    TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

    assert_eq!(fixture::read_text(&first), "needle\n");
    assert_eq!(fixture::read_text(&later), "done\n");
    assert_eq!(outcome.result.files_affected, 1);
    assert_eq!(outcome.result.error_count, 1);
    assert_eq!(outcome.metrics.undo_live_bytes, one_entry);
    assert_eq!(outcome.metrics.undo_bytes, one_entry);
    assert!(!outcome.undo_backup.contains_key(&first));
    assert!(outcome.undo_backup.contains_key(&later));
    let persisted = search_backup::load(journal_dir.path()).expect("active journal");
    assert!(!persisted.contains_key(&first));
    assert!(persisted.contains_key(&later));
}

#[test]
fn test_build_replaced_text_preserves_original_line_endings() {
    let path = PathBuf::from("/mixed.txt");
    let original = "alpha needle\r\nbeta needle\n";
    let replacements = [
        make_replacement(&path, 1, "alpha needle", "hay", 6..12),
        make_replacement(&path, 2, "beta needle", "stack", 5..11),
    ];
    let mut refs: Vec<&Replacement> = replacements.iter().collect();

    let build = build_replaced_text(original, &mut refs, usize::MAX, || false);

    assert_eq!(
        build.outcome,
        ReplacementTextOutcome::Replaced {
            new_content: "alpha hay\r\nbeta stack\n".to_string(),
            replacement_count: 2,
        }
    );
    assert_eq!(build.metrics.source_lines, 2);
    assert_eq!(build.metrics.accepted_replacements, 2);
}

#[test]
fn streaming_builder_matches_reference_for_empty_unicode_and_unterminated_lines() {
    let path = PathBuf::from("/semantic-fixture.txt");
    let original = "\nα needle\r\nlast🙂needle";
    let replacements = [
        make_replacement(&path, 1, "", "empty", 0..0),
        make_replacement(&path, 2, "α needle", "thread", 3..9),
        make_replacement(&path, 3, "last🙂needle", "done", 8..14),
    ];
    let mut streaming_refs: Vec<&Replacement> = replacements.iter().collect();
    let mut reference_refs: Vec<&Replacement> = replacements.iter().collect();

    let streaming = build_replaced_text(original, &mut streaming_refs, usize::MAX, || false);
    let reference = build_replaced_text_reference(original, &mut reference_refs);

    assert_eq!(streaming.outcome, reference);
    assert_eq!(
        streaming.outcome,
        ReplacementTextOutcome::Replaced {
            new_content: "empty\nα thread\r\nlast🙂done".to_string(),
            replacement_count: 3,
        }
    );
    assert_eq!(streaming.metrics.source_lines, 3);
}

#[test]
fn dense_short_line_fixture_retains_only_replacement_sized_metadata() {
    let byte_limit = usize::try_from(MAX_REPLACE_FILE_BYTES)
        .expect("Replace All file-byte limit should fit usize");
    let source_line_count = byte_limit / 2;
    let original = "x\n".repeat(source_line_count);
    let path = PathBuf::from("/dense-short-lines.txt");
    let original_line: Arc<str> = Arc::from("x");
    let replacement_text: Arc<str> = Arc::from("y");
    let replacements: Vec<_> = (0..MAX_REPLACE_PREVIEW_ROWS)
        .map(|index| {
            let line_index = index.saturating_mul(source_line_count.saturating_sub(1))
                / MAX_REPLACE_PREVIEW_ROWS.saturating_sub(1);
            Replacement {
                match_id: crate::model::content_search::SearchMatchId::from_index(index),
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
    let mut refs: Vec<&Replacement> = replacements.iter().collect();

    let build = build_replaced_text(&original, &mut refs, byte_limit, || false);

    let ReplacementTextOutcome::Replaced {
        new_content,
        replacement_count,
    } = build.outcome
    else {
        panic!("near-cap dense-line fixture should construct output");
    };
    assert_eq!(replacement_count, MAX_REPLACE_PREVIEW_ROWS);
    assert_eq!(new_content.len(), original.len());
    assert_eq!(
        build.metrics.source_lines,
        u64::try_from(source_line_count).expect("source-line fixture count should fit u64")
    );
    assert_eq!(
        build.metrics.retained_edit_records,
        MAX_REPLACE_PREVIEW_ROWS
    );
    assert!(
        build.metrics.retained_edit_records < source_line_count,
        "retained metadata must follow replacements, not source lines"
    );
    assert_eq!(
        build.metrics.retained_edit_bytes,
        MAX_REPLACE_PREVIEW_ROWS * std::mem::size_of::<PendingEdit<'static>>()
    );
    assert_eq!(
        build.metrics.output_bytes,
        u64::try_from(original.len()).expect("output fixture bytes should fit u64")
    );
}

#[test]
fn malformed_ranges_are_rejected_before_line_discovery_or_output_allocation() {
    let path = PathBuf::from("/malformed.txt");
    let mut reversed = make_replacement(&path, 1, "abcdef", "x", 2..4);
    reversed.match_range = std::ops::Range { start: 4, end: 2 };
    let mut reversed_refs = vec![&reversed];

    let reversed_build = build_replaced_text("abcdef\n", &mut reversed_refs, usize::MAX, || false);

    assert_eq!(
        reversed_build.outcome,
        ReplacementTextOutcome::InvalidPreview {
            reason: "replacement range endpoints are reversed",
        }
    );
    assert_eq!(reversed_build.metrics.source_lines, 0);
    assert_eq!(reversed_build.metrics.retained_edit_records, 0);

    let overlapping = [
        make_replacement(&path, 1, "abcdef", "x", 0..3),
        make_replacement(&path, 1, "abcdef", "y", 2..5),
    ];
    let mut overlapping_refs: Vec<&Replacement> = overlapping.iter().collect();
    let overlapping_build =
        build_replaced_text("abcdef\n", &mut overlapping_refs, usize::MAX, || false);

    assert_eq!(
        overlapping_build.outcome,
        ReplacementTextOutcome::InvalidPreview {
            reason: "replacement ranges overlap",
        }
    );
    assert_eq!(overlapping_build.metrics.source_lines, 0);
    assert_eq!(overlapping_build.metrics.retained_edit_records, 0);
}

#[test]
fn output_limit_is_checked_before_output_allocation() {
    let path = PathBuf::from("/output-limit.txt");
    let replacement = make_replacement(&path, 1, "x", "expanded", 0..1);
    let mut refs = vec![&replacement];

    let build = build_replaced_text("x\n", &mut refs, 2, || false);

    assert_eq!(build.outcome, ReplacementTextOutcome::OutputLimitExceeded);
    assert_eq!(build.metrics.source_lines, 1);
    assert_eq!(build.metrics.output_bytes, 0);
}

#[test]
fn streaming_line_discovery_observes_cancellation_between_dense_chunks() {
    let path = PathBuf::from("/cancel-dense.txt");
    let original = "x\n".repeat(5_000);
    let replacement = make_replacement(&path, 5_000, "x", "y", 0..1);
    let mut refs = vec![&replacement];
    let cancellation_checks = Cell::new(0usize);

    let build = build_replaced_text(&original, &mut refs, usize::MAX, || {
        let next = cancellation_checks.get().saturating_add(1);
        cancellation_checks.set(next);
        next >= 4
    });

    assert_eq!(build.outcome, ReplacementTextOutcome::Cancelled);
    assert!(build.metrics.source_lines < 5_000);
    assert_eq!(build.metrics.output_bytes, 0);
}

#[test]
fn test_line_spans_handles_leading_newline_without_underflow() {
    assert_eq!(line_spans_reference("\nnext"), vec![0..0, 1..5]);
    assert_eq!(line_spans_reference("\r\nnext"), vec![0..0, 2..6]);
}

#[test]
fn test_line_spans_does_not_add_empty_trailing_line() {
    assert_eq!(line_spans_reference("first\n"), vec![0..5]);
    assert_eq!(line_spans_reference("first\r\n"), vec![0..5]);
    assert_eq!(line_spans_reference("\n"), vec![0..0]);
}

#[test]
fn test_replaced_capacity_tracks_exact_growth_and_shrink() {
    let edits = [
        PendingEdit {
            start: 0,
            end: 5,
            replacement: "hi",
        },
        PendingEdit {
            start: 8,
            end: 10,
            replacement: "there!",
        },
    ];

    assert_eq!(replaced_capacity(12, &edits), 13);
}

#[test]
fn test_apply_replacements_aborts_when_undo_journal_cannot_be_persisted() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    let journal_path = dir.path().join("journal-is-a-file");
    fixture::write_text(&file, "let needle = 42;\n");
    fixture::write_text(&journal_path, "not a directory");

    let replacements = vec![make_replacement(
        &file,
        1,
        "let needle = 42;",
        "haystack",
        4..10,
    )];

    let cancel = AtomicBool::new(false);
    let result = apply_replacements(&replacements, &HashSet::new(), &cancel, Some(&journal_path));

    assert!(
        result
            .expect_err("journal failure should abort before file mutation")
            .to_string()
            .contains("undo journal"),
    );
    assert_eq!(
        fixture::read_text(&file),
        "let needle = 42;\n",
        "file bytes must stay unchanged when the undo journal cannot be saved",
    );
}

#[test]
fn test_apply_replacements_treats_line_numbers_past_eof_as_stale() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "needle\n");

    let replacements = vec![make_replacement(&file, 2, "needle", "replaced", 0..6)];

    let cancel = AtomicBool::new(false);
    let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);

    assert!(
        result
            .expect_err("out-of-range search row should make the preview stale")
            .to_string()
            .contains("line 2 changed since search")
    );
    assert_eq!(fixture::read_text(&file), "needle\n");
}

#[test]
fn test_apply_replacements_keeps_partial_success_errors_in_result() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    let missing = dir.path().join("missing.rs");
    fixture::write_text(&file, "needle\n");

    let replacements = vec![
        make_replacement(&file, 1, "needle", "replaced", 0..6),
        make_replacement(&missing, 1, "needle", "replaced", 0..6),
    ];

    let cancel = AtomicBool::new(false);
    let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("successful files should keep the Replace All operation successful")
        .into_parts();

    assert_eq!(result.replaced_count, 1);
    assert_eq!(result.files_affected, 1);
    assert_eq!(result.error_count, 1);
    assert!(result.error_sample[0].contains("Failed to stat"));
    assert_eq!(fixture::read_text(&file), "replaced\n");
    assert!(backup.contains_key(&file));
    assert!(!backup.contains_key(&missing));
}

#[test]
fn record_replacement_success_counts_adds_file_and_replacement_totals() {
    let mut replaced_count = 3;
    let mut files_affected = 2;

    record_replacement_success_counts(&mut replaced_count, &mut files_affected, 4);

    assert_eq!(replaced_count, 7);
    assert_eq!(files_affected, 3);
}

#[test]
fn atomic_write_error_display_includes_inner_error() {
    let error = ReplaceWriteError::AfterRename(anyhow::anyhow!("directory sync failed"));

    assert_eq!(error.to_string(), "directory sync failed");
}

#[test]
fn remaining_count_reports_actual_backup_len() {
    let dir = tempdir().expect("expected operation to succeed");
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");
    let mut remaining_backup = ReplaceUndoBackup::new();
    remaining_backup.insert(
        file_a,
        ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec()),
    );
    remaining_backup.insert(
        file_b,
        ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec()),
    );
    let outcome = UndoReplaceOutcome {
        restored_count: 0,
        skipped_count: 0,
        failed_count: 0,
        restored_open_paths: Vec::new(),
        skipped_sample: BoundedDiagnosticSample::default(),
        failed_sample: BoundedDiagnosticSample::default(),
        remaining_backup,
    };

    assert_eq!(outcome.remaining_count(), 2);
}

#[test]
fn test_undo_replacements_restores_content() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    let original = "let needle = 42;\n";
    fixture::write_text(&file, original);

    let replacements = vec![make_replacement(
        &file,
        1,
        "let needle = 42;",
        "haystack",
        4..10,
    )];

    let cancel = AtomicBool::new(false);
    let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("expected operation to succeed")
        .into_parts();

    assert!(fixture::read_text(&file).contains("haystack"));

    let outcome = undo_replacements(&backup);
    assert_eq!(outcome.restored_count(), 1);
    assert!(outcome.remaining_backup.is_empty());
    assert_eq!(fixture::read_text(&file), original);
}

#[test]
fn test_undo_replacements_drops_entry_when_file_is_already_original() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "before\n");

    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        file,
        ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
    );

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.restored_count(), 1);
    assert_eq!(outcome.skipped_count, 0);
    assert_eq!(outcome.failed_count, 0);
    assert!(outcome.remaining_backup.is_empty());
}

#[test]
fn test_undo_replacements_skips_diverged_file_and_keeps_backup() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "let needle = 42;\n");

    let replacements = vec![make_replacement(
        &file,
        1,
        "let needle = 42;",
        "haystack",
        4..10,
    )];

    let cancel = AtomicBool::new(false);
    let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("expected operation to succeed")
        .into_parts();
    fixture::write_text(&file, "let user_edit = 42;\n");

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.restored_count(), 0);
    assert_eq!(outcome.skipped_count, 1);
    assert_eq!(outcome.skipped_sample[0], file.display().to_string());
    assert_eq!(outcome.failed_count, 0);
    assert_eq!(outcome.remaining_backup, backup);
    assert_eq!(
        fixture::read_text(&file),
        "let user_edit = 42;\n",
        "undo must not overwrite edits made after Replace All",
    );
}

#[test]
fn test_undo_replacements_skips_externally_grown_file_before_reading() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "after\n");
    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        file.clone(),
        ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
    );
    fixture::write_text(&file, "after plus a large external edit\n");

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.restored_count(), 0);
    assert_eq!(outcome.skipped_count, 1);
    assert_eq!(outcome.skipped_sample[0], file.display().to_string());
    assert_eq!(outcome.failed_count, 0);
    assert_eq!(outcome.remaining_backup, backup);
    assert_eq!(
        fixture::read_text(&file),
        "after plus a large external edit\n"
    );
}

#[test]
fn test_undo_replacements_allows_current_file_at_exact_size_cap() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("exact-cap.txt");
    let replaced_bytes = vec![
        b'x';
        usize::try_from(MAX_REPLACE_FILE_BYTES)
            .expect("replace file byte cap should fit in usize")
    ];
    fixture::write_bytes(&file, &replaced_bytes);
    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        file.clone(),
        ReplaceUndoEntry::new(b"before\n".to_vec(), replaced_bytes),
    );

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.restored_count(), 1);
    assert_eq!(outcome.skipped_count, 0);
    assert_eq!(outcome.failed_count, 0);
    assert!(outcome.remaining_backup.is_empty());
    assert_eq!(fixture::read_text(&file), "before\n");
}

#[test]
fn undo_growth_after_metadata_is_skipped_without_unbounded_allocation() {
    let dir = tempdir().expect("undo growth tempdir");
    let file = dir.path().join("grown.txt");
    fixture::write_text(&file, "after\n");
    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        file.clone(),
        ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
    );
    let _hook_guard = register_undo_after_metadata_hook_for_test(&file, |path| {
        fixture::write_repeated_bytes(path, b"x", MAX_REPLACE_FILE_BYTES + 1);
    });

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.restored_count(), 0);
    assert_eq!(outcome.skipped_count, 1);
    assert_eq!(outcome.failed_count, 0);
    assert_eq!(outcome.remaining_backup, backup);
    assert_eq!(
        fs_metadata::file_facts(&file)
            .expect("grown target facts")
            .byte_size,
        MAX_REPLACE_FILE_BYTES + 1
    );
}

#[test]
fn undo_hooks_fire_only_for_their_own_target_across_interleaved_undos() {
    let dir = tempdir().expect("undo hook isolation tempdir");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    fixture::write_text(&first, "after\n");
    fixture::write_text(&second, "after\n");
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
    let mut second_backup = ReplaceUndoBackup::new();
    second_backup.insert(
        second.clone(),
        ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
    );
    let second_outcome = undo_replacements(&second_backup);
    assert_eq!(second_outcome.restored_count(), 1);
    assert_eq!(
        fired.lock().expect("fired after second undo").as_slice(),
        std::slice::from_ref(&second)
    );
    assert!(undo_after_metadata_hook_is_registered_for_test(&first));
    assert!(!undo_after_metadata_hook_is_registered_for_test(&second));

    let mut first_backup = ReplaceUndoBackup::new();
    first_backup.insert(
        first.clone(),
        ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
    );
    let first_outcome = undo_replacements(&first_backup);
    assert_eq!(first_outcome.restored_count(), 1);
    assert_eq!(
        fired.lock().expect("fired after both undos").as_slice(),
        &[second, first.clone()]
    );
    assert!(!undo_after_metadata_hook_is_registered_for_test(&first));
}

#[test]
fn parallel_undo_operations_consume_only_their_own_target_hooks() {
    let dir = tempdir().expect("parallel undo hooks tempdir");
    let fired = Arc::new(Mutex::new(Vec::new()));
    let mut targets = Vec::new();
    let mut guards = Vec::new();
    for index in 0..8 {
        let target = dir.path().join(format!("target-{index}.txt"));
        fixture::write_text(&target, "after\n");
        let recorder = Arc::clone(&fired);
        guards.push(register_undo_after_metadata_hook_for_test(
            &target,
            move |path| {
                recorder
                    .lock()
                    .expect("record parallel hook")
                    .push(path.to_path_buf());
            },
        ));
        targets.push(target);
    }

    std::thread::scope(|scope| {
        for target in &targets {
            scope.spawn(move || {
                let mut backup = ReplaceUndoBackup::new();
                backup.insert(
                    target.clone(),
                    ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
                );
                let outcome = undo_replacements(&backup);
                assert_eq!(outcome.restored_count(), 1);
            });
        }
    });

    let mut fired_paths = fired.lock().expect("parallel fired paths").clone();
    fired_paths.sort();
    assert_eq!(fired_paths, targets);
    for target in &targets {
        assert!(!undo_after_metadata_hook_is_registered_for_test(target));
    }
    eprintln!(
        "replace-undo-hook-isolation-evidence targets={} fired={} leaked=0",
        targets.len(),
        fired_paths.len()
    );
    drop(guards);
}

#[test]
fn dropped_hook_guard_removes_unconsumed_registration() {
    let dir = tempdir().expect("hook guard cleanup tempdir");
    let target = dir.path().join("unconsumed.txt");
    fixture::write_text(&target, "after\n");
    let fired = Arc::new(AtomicBool::new(false));
    let hook_fired = Arc::clone(&fired);
    let guard = register_undo_after_metadata_hook_for_test(&target, move |_| {
        hook_fired.store(true, Ordering::SeqCst);
    });
    assert!(undo_after_metadata_hook_is_registered_for_test(&target));
    drop(guard);
    assert!(!undo_after_metadata_hook_is_registered_for_test(&target));

    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        target,
        ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
    );
    let outcome = undo_replacements(&backup);
    assert_eq!(outcome.restored_count(), 1);
    assert!(
        !fired.load(Ordering::SeqCst),
        "a dropped guard's hook must never fire"
    );
}

#[test]
fn undo_failure_heavy_terminal_keeps_exact_totals_and_bounded_ordered_samples() {
    let dir = tempdir().expect("undo failures tempdir");
    let mut backup = ReplaceUndoBackup::new();
    for index in 0..10_000 {
        let path = dir.path().join(format!("missing-{index:05}.txt"));
        backup.insert(
            path,
            ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec()),
        );
    }

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.failed_count, 10_000);
    assert_eq!(outcome.failed_sample.entries().len(), 32);
    assert!(outcome.failed_sample.retained_bytes() <= 32 * 1024);
    assert!(outcome.failed_sample[0].contains("missing-00000.txt"));
    assert!(outcome.failed_sample[31].contains("missing-00031.txt"));
    assert_eq!(outcome.remaining_count(), 10_000);
}

#[test]
fn undo_returns_only_the_restored_open_identity_intersection() {
    let dir = tempdir().expect("undo open intersection tempdir");
    let open = dir.path().join("open.txt");
    let closed = dir.path().join("closed.txt");
    fixture::write_text(&open, "after\n");
    fixture::write_text(&closed, "after\n");
    let mut backup = ReplaceUndoBackup::new();
    for path in [&open, &closed] {
        backup.insert(
            path.clone(),
            ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
        );
    }
    let open_identity = fs_metadata::canonical_path(&open).expect("open canonical path");

    let outcome = undo_replacements_for_open_identities(&backup, &HashSet::from([open_identity]));

    assert_eq!(outcome.restored_count(), 2);
    assert_eq!(outcome.restored_open_paths, vec![open]);
}

#[test]
fn persist_undo_backup_saves_nonempty_journals_and_deletes_empty_ones() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        file,
        ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
    );

    persist_undo_backup(dir.path(), &backup).expect("persist nonempty undo backup");
    assert_eq!(
        search_backup::load(dir.path()).expect("load persisted undo backup"),
        backup
    );

    persist_undo_backup(dir.path(), &ReplaceUndoBackup::new()).expect("delete empty undo backup");
    assert!(
        search_backup::load(dir.path())
            .expect("empty persisted undo backup should load")
            .is_empty()
    );
}

#[test]
fn test_undo_replacements_keeps_failed_entries_after_partial_restore() {
    let dir = tempdir().expect("expected operation to succeed");
    let restored_file = dir.path().join("restored.rs");
    let missing_file = dir.path().join("missing.rs");
    fixture::write_text(&restored_file, "after\n");

    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        restored_file.clone(),
        ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
    );
    backup.insert(
        missing_file.clone(),
        ReplaceUndoEntry::new(b"missing-before\n".to_vec(), b"missing-after\n".to_vec()),
    );

    let outcome = undo_replacements(&backup);

    assert_eq!(outcome.restored_count(), 1);
    assert_eq!(outcome.failed_count, 1);
    assert_eq!(outcome.failed_sample[0], missing_file.display().to_string());
    assert_eq!(outcome.skipped_count, 0);
    assert_eq!(outcome.remaining_count(), 1);
    assert_eq!(
        outcome.remaining_backup.get(&missing_file),
        backup.get(&missing_file),
    );
    assert_eq!(fixture::read_text(&restored_file), "before\n");
}

#[test]
fn test_apply_replacements_reverse_order() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    fixture::write_text(&file, "ab cd\n");

    let replacements = vec![
        make_replacement(&file, 1, "ab cd", "XY", 0..2),
        make_replacement(&file, 1, "ab cd", "ZW", 3..5),
    ];

    let cancel = AtomicBool::new(false);
    let (result, _) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("expected operation to succeed")
        .into_parts();
    assert_eq!(result.replaced_count, 2);

    let content = fixture::read_text(&file);
    assert_eq!(
        content, "XY ZW\n",
        "both replacements should apply correctly"
    );
}

#[test]
fn test_apply_replacements_cancel() {
    let dir = tempdir().expect("expected operation to succeed");
    let file_a = dir.path().join("a.txt");
    let file_b = dir.path().join("b.txt");
    fixture::write_text(&file_a, "needle\n");
    fixture::write_text(&file_b, "needle\n");

    let replacements = vec![
        make_replacement(&file_a, 1, "needle", "replaced", 0..6),
        make_replacement(&file_b, 1, "needle", "replaced", 0..6),
    ];

    let cancel = AtomicBool::new(true);
    let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);
    assert!(result.is_err(), "cancelled replace should abort");
    assert_eq!(fixture::read_text(&file_a), "needle\n");
    assert_eq!(fixture::read_text(&file_b), "needle\n");
}

#[test]
#[cfg(unix)]
fn test_apply_replacements_cancel_rolls_back_applied_files() {
    let dir = tempdir().expect("expected operation to succeed");
    let file_a = dir.path().join("a.txt");
    let file_b = dir.path().join("b.txt");
    fixture::write_text(&file_a, "needle\n");
    fixture::write_text(&file_b, "needle\n");
    let replacements = vec![
        make_replacement(&file_a, 1, "needle", "replaced", 0..6),
        make_replacement(&file_b, 1, "needle", "replaced", 0..6),
    ];

    let cancel = Arc::new(AtomicBool::new(false));
    let lock_b =
        fs_write::TargetWriteGuard::acquire(&file_b).expect("expected operation to succeed");
    let cancel_for_worker = cancel.clone();
    let worker = std::thread::spawn(move || {
        apply_replacements(
            &replacements,
            &HashSet::new(),
            cancel_for_worker.as_ref(),
            None,
        )
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if fs_read::text(&file_a).is_ok_and(|content| content.contains("replaced")) {
            cancel.store(true, Ordering::Relaxed);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    drop(lock_b);

    let result = worker.join().expect("expected operation to succeed");

    assert!(result.is_err(), "cancelled replace should roll back");
    assert_eq!(fixture::read_text(&file_a), "needle\n");
    assert_eq!(fixture::read_text(&file_b), "needle\n");
}

#[test]
fn rollback_disposition_refuses_a_file_changed_after_replacement() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("test.rs");
    let entry = ReplaceUndoEntry::new(b"original\n".to_vec(), b"replaced\n".to_vec());

    fixture::write_text(&file, "replaced\n");
    assert_eq!(
        rollback_file_disposition(&file, &entry),
        RollbackDisposition::Restore,
        "a file still holding this run's replacement bytes is restorable",
    );

    fixture::write_text(&file, "original\n");
    assert_eq!(
        rollback_file_disposition(&file, &entry),
        RollbackDisposition::AlreadyOriginal,
        "a file already back at its original bytes needs no write",
    );

    // The per-file target guard is released as rollback moves on, so an
    // editor save of a later target can land mid-rollback. Restoring
    // `original_bytes` over it would discard content the user just saved,
    // and a clean rollback deletes the journal.
    fixture::write_text(&file, "user saved this after Replace All\n");
    assert_eq!(
        rollback_file_disposition(&file, &entry),
        RollbackDisposition::ChangedSinceReplacement,
    );

    fixture::remove_file(&file);
    assert_eq!(
        rollback_file_disposition(&file, &entry),
        RollbackDisposition::ChangedSinceReplacement,
        "an unreadable target must be reported, never overwritten on a guess",
    );
}

#[test]
fn rollback_keeps_the_journal_when_a_target_changed_mid_rollback() {
    let dir = tempdir().expect("expected operation to succeed");
    let restorable = dir.path().join("restorable.rs");
    let changed = dir.path().join("changed.rs");
    fixture::write_text(&restorable, "replaced\n");
    fixture::write_text(&changed, "user saved this after Replace All\n");

    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        restorable.clone(),
        ReplaceUndoEntry::new(b"original\n".to_vec(), b"replaced\n".to_vec()),
    );
    backup.insert(
        changed.clone(),
        ReplaceUndoEntry::new(b"original\n".to_vec(), b"replaced\n".to_vec()),
    );

    let errors = rollback_applied_files(&backup);

    assert_eq!(
        fixture::read_text(&restorable),
        "original\n",
        "an unchanged target is still rolled back",
    );
    assert_eq!(
        fixture::read_text(&changed),
        "user saved this after Replace All\n",
        "a target changed after replacement must not be overwritten",
    );
    assert_eq!(
        errors.total_count(),
        1,
        "the unrestored target must be reported so the journal is kept",
    );
}

#[test]
fn test_apply_replacements_nonexistent_file() {
    let dir = tempdir().expect("expected operation to succeed");
    let missing_file = dir.path().join("does_not_exist.rs");

    let replacements = vec![make_replacement(
        &missing_file,
        1,
        "phantom line",
        "replacement",
        0..7,
    )];

    let cancel = AtomicBool::new(false);
    let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);
    assert!(result.is_err(), "should fail when only file is nonexistent");
}

#[test]
fn replace_all_failure_heavy_summary_is_exact_bounded_and_content_free() {
    let dir = tempdir().expect("replace failure tempdir");
    let private_line: Arc<str> = Arc::from("private-document-sentinel");
    let replacement: Arc<str> = Arc::from("done");
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

    let error = apply_replacements(
        &replacements,
        &HashSet::new(),
        &AtomicBool::new(false),
        None,
    )
    .expect_err("all missing targets should produce one bounded terminal error")
    .to_string();

    assert!(error.contains(&format!(
        "Replace All failed: {MAX_REPLACE_PREVIEW_ROWS} issue(s)"
    )));
    assert!(error.contains("missing-00000.txt"));
    assert!(error.contains(&format!("{} more omitted", MAX_REPLACE_PREVIEW_ROWS - 1)));
    assert!(!error.contains(&format!("missing-{:05}.txt", MAX_REPLACE_PREVIEW_ROWS - 1)));
    assert!(!error.contains(private_line.as_ref()));
    assert!(error.len() < 1_024);
}

#[test]
fn test_apply_replacements_skip_paths() {
    let dir = tempdir().expect("expected operation to succeed");
    let file_a = dir.path().join("a.rs");
    let file_b = dir.path().join("b.rs");
    fixture::write_text(&file_a, "needle\n");
    fixture::write_text(&file_b, "needle\n");

    let replacements = vec![
        make_replacement(&file_a, 1, "needle", "replaced", 0..6),
        make_replacement(&file_b, 1, "needle", "replaced", 0..6),
    ];

    let mut skip = HashSet::new();
    skip.insert(file_b.clone());

    let cancel = AtomicBool::new(false);
    let (result, backup) = apply_replacements(&replacements, &skip, &cancel, None)
        .expect("expected operation to succeed")
        .into_parts();

    assert_eq!(result.replaced_count, 1, "only a.rs should be replaced");
    assert_eq!(result.files_affected, 1);
    assert_eq!(result.skipped_count, 1);
    assert_eq!(result.skipped_sample[0], file_b.display().to_string());
    assert!(backup.contains_key(&file_a));
    assert!(!backup.contains_key(&file_b));
    assert_eq!(fixture::read_text(&file_b), "needle\n");
}

#[test]
fn test_apply_replacements_skips_stale_search_result() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("stale.rs");
    fixture::write_text(&file, "needle changed\n");

    let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];

    let cancel = AtomicBool::new(false);
    let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);

    assert!(
        result
            .expect_err("all-stale replace should report why nothing was written")
            .to_string()
            .contains("changed since search")
    );
    assert_eq!(fixture::read_text(&file), "needle changed\n");
}

#[test]
fn test_apply_replacements_skips_whole_file_when_preview_line_is_missing() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("stale-missing-line.rs");
    fixture::write_text(&file, "needle one\n");

    let replacements = vec![
        make_replacement(&file, 1, "needle one", "replaced one", 0..6),
        make_replacement(&file, 2, "needle two", "replaced two", 0..6),
    ];

    let cancel = AtomicBool::new(false);
    let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);

    assert!(
        result
            .expect_err("missing preview line should make the file stale")
            .to_string()
            .contains("line 2 changed since search")
    );
    assert_eq!(
        fixture::read_text(&file),
        "needle one\n",
        "stale preview data must not partially replace earlier lines"
    );
}

#[cfg(unix)]
#[test]
fn replace_all_and_undo_preserve_file_mode() {
    let dir = tempdir().expect("expected operation to succeed");
    let file = dir.path().join("private.rs");
    fixture::write_text(&file, "let needle = 42;\n");
    fixture::set_mode(&file, 0o600);

    let replacements = vec![make_replacement(
        &file,
        1,
        "let needle = 42;",
        "found",
        4..10,
    )];
    let cancel = AtomicBool::new(false);
    let (_result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
        .expect("replace should succeed")
        .into_parts();

    let mode_after_replace = fixture::mode(&file) & 0o777;
    assert_eq!(
        mode_after_replace, 0o600,
        "Replace All must keep the rewritten file's restrictive mode"
    );

    let outcome = undo_replacements(&backup);
    assert_eq!(outcome.restored_count(), 1, "undo should restore the file");
    let mode_after_undo = fixture::mode(&file) & 0o777;
    assert_eq!(
        mode_after_undo, 0o600,
        "undoing Replace All must also keep the file's restrictive mode"
    );
}

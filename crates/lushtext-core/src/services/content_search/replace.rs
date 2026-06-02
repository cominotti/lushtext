// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace-all and undo flows for workspace content search.
//!
//! This is the command side of the content-search service. It performs file
//! locking, atomic writes, rollback on cancellation, and undo backup handling
//! without depending on any GTK types.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::model::content_search::{ReplaceResult, Replacement};
use crate::services::{durable_write, search_backup};

/// Per-file bytes needed to safely undo a Replace All.
///
/// The undo path compares `replaced_bytes` with the file's current contents
/// before restoring `original_bytes`, so edits made after Replace All are not
/// overwritten by a stale undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceUndoEntry {
    /// File bytes before Replace All changed this file.
    pub original_bytes: Vec<u8>,
    /// File bytes immediately after Replace All changed this file.
    pub replaced_bytes: Vec<u8>,
}

impl ReplaceUndoEntry {
    /// Build one undo entry from the before/after byte snapshots.
    #[must_use]
    pub fn new(original_bytes: Vec<u8>, replaced_bytes: Vec<u8>) -> Self {
        Self {
            original_bytes,
            replaced_bytes,
        }
    }
}

/// In-memory Replace All undo backup keyed by absolute file path.
pub type ReplaceUndoBackup = HashMap<PathBuf, ReplaceUndoEntry>;

/// Outcome of one undo attempt across a Replace All backup.
///
/// `remaining_backup` contains only entries that were not restored, letting the
/// UI persist a smaller retryable backup instead of clearing undo state after a
/// partial success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoReplaceOutcome {
    /// Paths restored to their pre-replace bytes.
    pub restored_paths: Vec<PathBuf>,
    /// Paths left untouched because the current bytes no longer matched the
    /// Replace All output snapshot.
    pub skipped_paths: Vec<PathBuf>,
    /// Paths that could not be read, locked, or written.
    pub failed_paths: Vec<PathBuf>,
    /// Retryable backup entries for skipped or failed paths.
    pub remaining_backup: ReplaceUndoBackup,
}

impl UndoReplaceOutcome {
    /// Number of files restored by this undo attempt.
    #[must_use]
    pub fn restored_count(&self) -> usize {
        self.restored_paths.len()
    }

    /// Number of files still retained for a future undo attempt.
    #[must_use]
    pub fn remaining_count(&self) -> usize {
        self.remaining_backup.len()
    }
}

/// Apply replacements to files on disk.
///
/// Groups replacements by file, reads each file, applies replacements in reverse order
/// (to avoid offset shifting), and writes atomically (temp file + rename). Returns the
/// replacement summary and a backup mapping file paths to their before/after content
/// snapshots for undo.
///
/// Per-file errors are collected (not early-returned) so that already-replaced files
/// remain in the backup for undo. Only returns `Err` if zero files could be processed.
///
/// `skip_paths` lists files that should NOT be replaced (e.g., open tabs with unsaved changes).
/// Skipped files are excluded from the result count but included in `ReplaceResult::skipped_paths`.
///
/// # Errors
///
/// Returns an error if every candidate file fails to process or if the replace
/// operation is cancelled and rollback cannot complete cleanly.
pub fn apply_replacements(
    replacements: &[Replacement],
    skip_paths: &HashSet<PathBuf>,
    cancel: &AtomicBool,
    journal_data_dir: Option<&Path>,
) -> anyhow::Result<(ReplaceResult, ReplaceUndoBackup)> {
    let mut by_file: BTreeMap<PathBuf, Vec<&Replacement>> = BTreeMap::new();
    for r in replacements {
        by_file.entry(r.path.clone()).or_default().push(r);
    }

    let mut backup: ReplaceUndoBackup = HashMap::new();
    let mut replaced_count = 0usize;
    let mut files_affected = 0usize;
    let mut skipped_paths = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut applied_paths: Vec<PathBuf> = Vec::new();
    let mut held_locks: Vec<Option<durable_write::FileWriteLock>> = Vec::new();
    let mut cancelled = false;

    for (path, mut file_replacements) in by_file {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        if skip_paths.contains(&path) {
            skipped_paths.push(path);
            continue;
        }

        let lock = match durable_write::FileWriteLock::acquire(&path) {
            Ok(lock) => lock,
            Err(e) => {
                errors.push(format!("Failed to lock {}: {e}", path.display()));
                continue;
            }
        };
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        let original_bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(format!("Failed to read {}: {e}", path.display()));
                continue;
            }
        };

        let original_text = match String::from_utf8(original_bytes.clone()) {
            Ok(text) => text,
            Err(e) => {
                errors.push(format!("Non-UTF8 file {}: {e}", path.display()));
                continue;
            }
        };

        let text_outcome = build_replaced_text(&original_text, &mut file_replacements);
        let (new_content, file_replaced) = match text_outcome {
            ReplacementTextOutcome::Replaced {
                new_content,
                replacement_count,
            } => (new_content, replacement_count),
            ReplacementTextOutcome::StaleLine { line_number } => {
                errors.push(format!(
                    "Skipped {}: line {} changed since search",
                    path.display(),
                    line_number,
                ));
                continue;
            }
            ReplacementTextOutcome::Unchanged => continue,
        };
        let replaced_bytes = new_content.as_bytes().to_vec();
        backup.insert(
            path.clone(),
            ReplaceUndoEntry::new(original_bytes, replaced_bytes),
        );
        if let Some(data_dir) = journal_data_dir
            && let Err(e) = persist_undo_backup(data_dir, &backup)
        {
            backup.remove(&path);
            errors.push(format!(
                "Failed to persist undo backup before replacing {}: {e}",
                path.display()
            ));
            continue;
        }

        match atomic_write(&path, &backup[&path].replaced_bytes) {
            Ok(()) => {
                applied_paths.push(path.clone());
                held_locks.push(lock);
                record_replacement_success_counts(
                    &mut replaced_count,
                    &mut files_affected,
                    file_replaced,
                );
            }
            Err(AtomicWriteError::BeforeRename(e)) => {
                errors.push(format!("Failed to write {}: {e}", path.display()));
                backup.remove(&path);
                if let Some(data_dir) = journal_data_dir
                    && let Err(journal_error) = persist_undo_backup(data_dir, &backup)
                {
                    errors.push(format!(
                        "Failed to update undo backup after write failure for {}: {journal_error}",
                        path.display()
                    ));
                }
                continue;
            }
            Err(AtomicWriteError::AfterRename(e)) => {
                errors.push(format!(
                    "Replaced {}, but durability sync failed: {e}",
                    path.display()
                ));
                applied_paths.push(path.clone());
                held_locks.push(lock);
                record_replacement_success_counts(
                    &mut replaced_count,
                    &mut files_affected,
                    file_replaced,
                );
            }
        }
    }

    if cancelled {
        let rollback_errors = rollback_applied_files(&backup, &applied_paths);
        if rollback_errors.is_empty() {
            if let Some(data_dir) = journal_data_dir
                && let Err(e) = persist_undo_backup(data_dir, &ReplaceUndoBackup::new())
            {
                return Err(anyhow::anyhow!(
                    "Replace cancelled; undo backup cleanup failed: {e}"
                ));
            }
            return Err(anyhow::anyhow!("Replace cancelled"));
        }
        return Err(anyhow::anyhow!(
            "Replace cancelled; rollback failed: {}",
            rollback_errors.join("; ")
        ));
    }

    if files_affected == 0 && !errors.is_empty() {
        return Err(anyhow::anyhow!("{}", errors.join("; ")));
    }

    let result = ReplaceResult {
        replaced_count,
        files_affected,
        skipped_paths,
        errors,
    };

    Ok((result, backup))
}

/// Fold one successfully written file into the public Replace All counters.
fn record_replacement_success_counts(
    replaced_count: &mut usize,
    files_affected: &mut usize,
    file_replaced: usize,
) {
    *replaced_count += file_replaced;
    *files_affected += 1;
}

/// Pure result of applying one file's replacement preview data to text.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplacementTextOutcome {
    /// At least one bounded replacement changed the file text.
    Replaced {
        /// Full file text after replacements are applied.
        new_content: String,
        /// Number of individual replacement previews consumed.
        replacement_count: usize,
    },
    /// No generated replacement targeted an existing line or valid range.
    Unchanged,
    /// The line text no longer matches the preview captured during search.
    StaleLine {
        /// Original 1-based line number reported by the stale search result.
        line_number: u64,
    },
}

/// Apply one file's replacement previews to already-loaded text.
///
/// The helper owns the deterministic range clipping and reverse-order policy
/// used by the I/O command above, making that behavior property-testable
/// without opening files or touching undo journals.
fn build_replaced_text(
    original_text: &str,
    file_replacements: &mut [&Replacement],
) -> ReplacementTextOutcome {
    // Apply replacements from the bottom of the file upward so byte ranges on
    // earlier lines stay valid even after later replacements change length.
    file_replacements.sort_by(|a, b| {
        b.line_number
            .cmp(&a.line_number)
            .then(b.match_range.start.cmp(&a.match_range.start))
    });

    let line_ending = detect_line_ending(original_text);
    let mut lines: Vec<String> = original_text.lines().map(String::from).collect();
    let has_trailing_newline = original_text.ends_with('\n');

    // Validate against the original line snapshot before mutating anything so
    // stale search results skip the whole file instead of partially applying.
    for replacement in file_replacements.iter() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Search result line numbers stay within usize indexing limits on supported editor workloads"
        )]
        let line_idx = replacement.line_number.saturating_sub(1) as usize;
        if line_idx < lines.len() && lines[line_idx] != replacement.original_line {
            return ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            };
        }
    }

    let mut replacement_count = 0usize;
    for replacement in file_replacements.iter() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Search result line numbers stay within usize indexing limits on supported editor workloads"
        )]
        let line_idx = replacement.line_number.saturating_sub(1) as usize;
        if line_idx < lines.len() {
            let line = &mut lines[line_idx];
            let start = line.floor_char_boundary(replacement.match_range.start.min(line.len()));
            let end = line.ceil_char_boundary(replacement.match_range.end.min(line.len()));
            if start <= end {
                line.replace_range(start..end, &replacement.replacement);
                replacement_count += 1;
            }
        }
    }

    if replacement_count == 0 {
        return ReplacementTextOutcome::Unchanged;
    }

    let mut new_content = lines.join(line_ending);
    if has_trailing_newline {
        new_content.push_str(line_ending);
    }

    ReplacementTextOutcome::Replaced {
        new_content,
        replacement_count,
    }
}

/// Apply replacement preview data to text through the production pure helper.
///
/// This feature-only hook lets the property target exercise clipping and
/// ordering without touching files, locks, or undo-backup persistence.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn apply_replacements_to_text_for_property_test(
    original_text: &str,
    replacements: &[Replacement],
) -> Option<(String, usize)> {
    let mut file_replacements: Vec<&Replacement> = replacements.iter().collect();
    match build_replaced_text(original_text, &mut file_replacements) {
        ReplacementTextOutcome::Replaced {
            new_content,
            replacement_count,
        } => Some((new_content, replacement_count)),
        ReplacementTextOutcome::Unchanged | ReplacementTextOutcome::StaleLine { .. } => None,
    }
}

/// Restore files from backup (undo Replace All).
///
/// Writes each file atomically (temp file + rename), but only when the file's
/// current bytes still match the Replace All output snapshot. Per-file failures
/// stay in `remaining_backup` so the UI can keep undo available for retry.
#[must_use]
pub fn undo_replacements(backup: &ReplaceUndoBackup) -> UndoReplaceOutcome {
    let mut restored_paths = Vec::new();
    let mut skipped_paths = Vec::new();
    let mut failed_paths = Vec::new();
    let mut remaining_backup = ReplaceUndoBackup::new();

    for (path, entry) in backup {
        let Ok(_lock) = durable_write::FileWriteLock::acquire(path) else {
            failed_paths.push(path.clone());
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        };

        let Ok(current_bytes) = std::fs::read(path) else {
            failed_paths.push(path.clone());
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        };

        if current_bytes == entry.original_bytes {
            restored_paths.push(path.clone());
            continue;
        }

        if current_bytes != entry.replaced_bytes {
            skipped_paths.push(path.clone());
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        if atomic_write(path, &entry.original_bytes).is_err() {
            failed_paths.push(path.clone());
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        restored_paths.push(path.clone());
    }

    UndoReplaceOutcome {
        restored_paths,
        skipped_paths,
        failed_paths,
        remaining_backup,
    }
}

/// Detect the predominant line ending style in a string.
/// Returns `"\r\n"` if CRLF is found, otherwise `"\n"`.
fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Distinguishes write failures before and after the destination rename.
#[derive(Debug)]
enum AtomicWriteError {
    /// The final path should still contain its previous bytes.
    BeforeRename(anyhow::Error),
    /// The rename already succeeded, but making the directory entry durable failed.
    AfterRename(anyhow::Error),
}

impl std::fmt::Display for AtomicWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BeforeRename(error) | Self::AfterRename(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for AtomicWriteError {}

/// Atomically write bytes to a file (temp file + rename, matching json_store::save pattern).
fn atomic_write(path: &Path, content: &[u8]) -> Result<(), AtomicWriteError> {
    use std::io::Write;

    let tmp_path = durable_write::unique_temp_path(path, "replace");
    let file = std::fs::File::create(&tmp_path).map_err(|e| {
        AtomicWriteError::BeforeRename(anyhow::anyhow!(
            "Failed to create {}: {}",
            tmp_path.display(),
            e
        ))
    })?;
    let mut writer = std::io::BufWriter::new(file);
    let write_result = writer
        .write_all(content)
        .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", tmp_path.display(), e))
        .and_then(|()| {
            writer
                .flush()
                .map_err(|e| anyhow::anyhow!("Failed to flush {}: {}", tmp_path.display(), e))
                .and_then(|()| {
                    writer.get_ref().sync_all().map_err(|e| {
                        anyhow::anyhow!("Failed to sync {}: {}", tmp_path.display(), e)
                    })
                })
        });
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(AtomicWriteError::BeforeRename(e));
    }
    std::fs::rename(&tmp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        AtomicWriteError::BeforeRename(anyhow::anyhow!(
            "Failed to rename {} to {}: {}",
            tmp_path.display(),
            path.display(),
            e
        ))
    })?;
    durable_write::sync_parent_dir(path).map_err(|e| {
        AtomicWriteError::AfterRename(anyhow::anyhow!(
            "Failed to sync parent directory for {}: {}",
            path.display(),
            e
        ))
    })
}

/// Persist the current undo backup snapshot or delete the journal when empty.
fn persist_undo_backup(data_dir: &Path, backup: &ReplaceUndoBackup) -> anyhow::Result<()> {
    if backup.is_empty() {
        search_backup::delete(data_dir)
    } else {
        search_backup::save(data_dir, backup)
    }
}

/// Restore already-written files in reverse order when cancellation interrupts a run.
fn rollback_applied_files(backup: &ReplaceUndoBackup, applied_paths: &[PathBuf]) -> Vec<String> {
    let mut errors = Vec::new();
    for path in applied_paths.iter().rev() {
        let Some(entry) = backup.get(path) else {
            continue;
        };
        if let Err(e) = atomic_write(path, &entry.original_bytes) {
            errors.push(format!("Failed to restore {}: {e}", path.display()));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::sync::Arc;
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
            path: path.to_path_buf(),
            line_number,
            original_line: original_line.to_string(),
            replaced_line,
            replacement: replacement.to_string(),
            match_range,
        }
    }

    #[test]
    fn test_apply_replacements_literal() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fs::write(&file_a, "let hello = 1;\nlet world = 2;\n")
            .expect("expected operation to succeed");
        fs::write(&file_b, "fn hello() {}\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file_a, 1, "let hello = 1;", "goodbye", 4..9),
            make_replacement(&file_b, 1, "fn hello() {}", "goodbye", 3..8),
        ];

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed");

        assert_eq!(result.replaced_count, 2);
        assert_eq!(result.files_affected, 2);
        assert!(result.skipped_paths.is_empty());

        let content_a = fs::read_to_string(&file_a).expect("expected operation to succeed");
        assert!(
            content_a.contains("goodbye"),
            "a.rs should have replacement"
        );
        assert!(
            !content_a.contains("hello"),
            "a.rs should not have original"
        );

        let content_b = fs::read_to_string(&file_b).expect("expected operation to succeed");
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
        fs::write(&file, original).expect("expected operation to succeed");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed");

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
        fs::write(&file, "let needle = 42;\n").expect("expected operation to succeed");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(
            &replacements,
            &HashSet::new(),
            &cancel,
            Some(journal_dir.path()),
        )
        .expect("expected operation to succeed");

        let persisted =
            search_backup::load(journal_dir.path()).expect("expected operation to succeed");
        assert_eq!(persisted, backup);
    }

    #[test]
    fn test_apply_replacements_aborts_when_undo_journal_cannot_be_persisted() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let journal_path = dir.path().join("journal-is-a-file");
        fs::write(&file, "let needle = 42;\n").expect("expected operation to succeed");
        fs::write(&journal_path, "not a directory").expect("expected operation to succeed");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let result =
            apply_replacements(&replacements, &HashSet::new(), &cancel, Some(&journal_path));

        assert!(
            result
                .expect_err("journal failure should abort before file mutation")
                .to_string()
                .contains("persist undo backup"),
        );
        assert_eq!(
            fs::read_to_string(&file).expect("expected operation to succeed"),
            "let needle = 42;\n",
            "file bytes must stay unchanged when the undo journal cannot be saved",
        );
    }

    #[test]
    fn test_apply_replacements_ignores_line_numbers_past_eof() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fs::write(&file, "needle\n").expect("expected operation to succeed");

        let replacements = vec![make_replacement(&file, 2, "needle", "replaced", 0..6)];

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("out-of-range search rows should be ignored without panicking");

        assert_eq!(result.replaced_count, 0);
        assert_eq!(result.files_affected, 0);
        assert!(result.errors.is_empty());
        assert!(backup.is_empty());
        assert_eq!(
            fs::read_to_string(&file).expect("expected operation to succeed"),
            "needle\n"
        );
    }

    #[test]
    fn test_apply_replacements_keeps_partial_success_errors_in_result() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let missing = dir.path().join("missing.rs");
        fs::write(&file, "needle\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file, 1, "needle", "replaced", 0..6),
            make_replacement(&missing, 1, "needle", "replaced", 0..6),
        ];

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("successful files should keep the Replace All operation successful");

        assert_eq!(result.replaced_count, 1);
        assert_eq!(result.files_affected, 1);
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("Failed to read"));
        assert_eq!(
            fs::read_to_string(&file).expect("expected operation to succeed"),
            "replaced\n"
        );
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
        let error = AtomicWriteError::AfterRename(anyhow::anyhow!("directory sync failed"));

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
            restored_paths: Vec::new(),
            skipped_paths: Vec::new(),
            failed_paths: Vec::new(),
            remaining_backup,
        };

        assert_eq!(outcome.remaining_count(), 2);
    }

    #[test]
    fn test_undo_replacements_restores_content() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let original = "let needle = 42;\n";
        fs::write(&file, original).expect("expected operation to succeed");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed");

        assert!(
            fs::read_to_string(&file)
                .expect("expected operation to succeed")
                .contains("haystack")
        );

        let outcome = undo_replacements(&backup);
        assert_eq!(outcome.restored_count(), 1);
        assert!(outcome.remaining_backup.is_empty());
        assert_eq!(
            fs::read_to_string(&file).expect("expected operation to succeed"),
            original
        );
    }

    #[test]
    fn test_undo_replacements_drops_entry_when_file_is_already_original() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fs::write(&file, "before\n").expect("expected operation to succeed");

        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            file.clone(),
            ReplaceUndoEntry::new(b"before\n".to_vec(), b"after\n".to_vec()),
        );

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.restored_paths, vec![file]);
        assert!(outcome.skipped_paths.is_empty());
        assert!(outcome.failed_paths.is_empty());
        assert!(outcome.remaining_backup.is_empty());
    }

    #[test]
    fn test_undo_replacements_skips_diverged_file_and_keeps_backup() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fs::write(&file, "let needle = 42;\n").expect("expected operation to succeed");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed");
        fs::write(&file, "let user_edit = 42;\n").expect("expected operation to succeed");

        let outcome = undo_replacements(&backup);

        assert_eq!(outcome.restored_count(), 0);
        assert_eq!(outcome.skipped_paths, vec![file.clone()]);
        assert!(outcome.failed_paths.is_empty());
        assert_eq!(outcome.remaining_backup, backup);
        assert_eq!(
            fs::read_to_string(&file).expect("expected operation to succeed"),
            "let user_edit = 42;\n",
            "undo must not overwrite edits made after Replace All",
        );
    }

    #[test]
    fn test_undo_replacements_keeps_failed_entries_after_partial_restore() {
        let dir = tempdir().expect("expected operation to succeed");
        let restored_file = dir.path().join("restored.rs");
        let missing_file = dir.path().join("missing.rs");
        fs::write(&restored_file, "after\n").expect("expected operation to succeed");

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

        assert_eq!(outcome.restored_paths, vec![restored_file.clone()]);
        assert_eq!(outcome.failed_paths, vec![missing_file.clone()]);
        assert!(outcome.skipped_paths.is_empty());
        assert_eq!(outcome.remaining_count(), 1);
        assert_eq!(
            outcome.remaining_backup.get(&missing_file),
            backup.get(&missing_file),
        );
        assert_eq!(
            fs::read_to_string(&restored_file).expect("expected operation to succeed"),
            "before\n"
        );
    }

    #[test]
    fn test_apply_replacements_reverse_order() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fs::write(&file, "ab cd\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file, 1, "ab cd", "XY", 0..2),
            make_replacement(&file, 1, "ab cd", "ZW", 3..5),
        ];

        let cancel = AtomicBool::new(false);
        let (result, _) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("expected operation to succeed");
        assert_eq!(result.replaced_count, 2);

        let content = fs::read_to_string(&file).expect("expected operation to succeed");
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
        fs::write(&file_a, "needle\n").expect("expected operation to succeed");
        fs::write(&file_b, "needle\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let cancel = AtomicBool::new(true);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);
        assert!(result.is_err(), "cancelled replace should abort");
        assert_eq!(
            fs::read_to_string(&file_a).expect("expected operation to succeed"),
            "needle\n"
        );
        assert_eq!(
            fs::read_to_string(&file_b).expect("expected operation to succeed"),
            "needle\n"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_apply_replacements_cancel_rolls_back_applied_files() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        fs::write(&file_a, "needle\n").expect("expected operation to succeed");
        fs::write(&file_b, "needle\n").expect("expected operation to succeed");
        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let cancel = Arc::new(AtomicBool::new(false));
        let lock_b =
            durable_write::FileWriteLock::acquire(&file_b).expect("expected operation to succeed");
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
            if fs::read_to_string(&file_a).is_ok_and(|content| content.contains("replaced")) {
                cancel.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        drop(lock_b);

        let result = worker.join().expect("expected operation to succeed");

        assert!(result.is_err(), "cancelled replace should roll back");
        assert_eq!(
            fs::read_to_string(&file_a).expect("expected operation to succeed"),
            "needle\n"
        );
        assert_eq!(
            fs::read_to_string(&file_b).expect("expected operation to succeed"),
            "needle\n"
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
    fn test_apply_replacements_skip_paths() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fs::write(&file_a, "needle\n").expect("expected operation to succeed");
        fs::write(&file_b, "needle\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let mut skip = HashSet::new();
        skip.insert(file_b.clone());

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &skip, &cancel, None)
            .expect("expected operation to succeed");

        assert_eq!(result.replaced_count, 1, "only a.rs should be replaced");
        assert_eq!(result.files_affected, 1);
        assert_eq!(result.skipped_paths.len(), 1);
        assert_eq!(result.skipped_paths[0], file_b);
        assert!(backup.contains_key(&file_a));
        assert!(!backup.contains_key(&file_b));
        assert_eq!(
            fs::read_to_string(&file_b).expect("expected operation to succeed"),
            "needle\n"
        );
    }

    #[test]
    fn test_apply_replacements_skips_stale_search_result() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("stale.rs");
        fs::write(&file, "needle changed\n").expect("expected operation to succeed");

        let replacements = vec![make_replacement(&file, 1, "needle", "replaced", 0..6)];

        let cancel = AtomicBool::new(false);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel, None);

        assert!(
            result
                .expect_err("all-stale replace should report why nothing was written")
                .to_string()
                .contains("changed since search")
        );
        assert_eq!(
            fs::read_to_string(&file).expect("expected operation to succeed"),
            "needle changed\n"
        );
    }
}

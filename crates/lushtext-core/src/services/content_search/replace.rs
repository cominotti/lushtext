// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace-all and undo flows for workspace content search.
//!
//! This is the command side of the content-search service. It performs file
//! locking, atomic writes, rollback on cancellation, and undo backup handling
//! without depending on any GTK types.

#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::model::content_search::{ReplaceResult, Replacement};
use crate::services::{
    filesystem::{WriteLabel, metadata as fs_metadata, read as fs_read, write as fs_write},
    search_backup,
};

/// Largest single file Replace All will read and rewrite.
///
/// Ten megabytes keeps the whole-file validation and undo snapshot path bounded
/// on ordinary laptops while still covering typical source and notes files.
pub const MAX_REPLACE_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Largest total undo payload retained for one Replace All run.
///
/// The payload stores before and after bytes for each touched file. Sixty-four
/// megabytes keeps rollback useful without letting one operation consume the
/// editor's broader buffer-memory budget.
pub const MAX_REPLACE_UNDO_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(test)]
thread_local! {
    static TEST_MAX_REPLACE_UNDO_BYTES: Cell<Option<u64>> = const { Cell::new(None) };
    static TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE: Cell<bool> = const { Cell::new(false) };
}

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

/// Result of applying Replace All plus the undo payload needed by the caller.
#[derive(Debug)]
pub struct ApplyReplacementsOutcome {
    /// User-facing replacement counts, skips, and recoverable errors.
    pub result: ReplaceResult,
    /// Per-file before/after bytes retained for the active undo window.
    pub undo_backup: ReplaceUndoBackup,
}

impl ApplyReplacementsOutcome {
    /// Split the outcome into its two public payloads for callers that need both.
    #[must_use]
    pub fn into_parts(self) -> (ReplaceResult, ReplaceUndoBackup) {
        (self.result, self.undo_backup)
    }
}

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
) -> anyhow::Result<ApplyReplacementsOutcome> {
    let _journal_guard = journal_data_dir
        .map(|_| search_backup::acquire_journal_guard())
        .transpose()?;
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
    let mut cancelled = false;
    let mut undo_payload_bytes = 0u64;
    let mut journal_prepared = false;
    let mut journal_armed = false;

    for (path, mut file_replacements) in by_file {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        if skip_paths.contains(&path) {
            skipped_paths.push(path);
            continue;
        }

        let _guard = match fs_write::TargetWriteGuard::acquire(&path) {
            Ok(guard) => guard,
            Err(e) => {
                errors.push(format!("Failed to lock {}: {e}", path.display()));
                continue;
            }
        };
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        let facts = match fs_metadata::file_facts(&path) {
            Ok(facts) => facts,
            Err(e) => {
                errors.push(format!("Failed to stat {}: {e}", path.display()));
                continue;
            }
        };
        if facts.byte_size > MAX_REPLACE_FILE_BYTES {
            skipped_paths.push(path.clone());
            errors.push(format!(
                "Skipped {}: file is larger than the 10 MB Replace All limit",
                path.display()
            ));
            continue;
        }

        let original_bytes = match fs_read::bytes(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(format!("Failed to read {}: {e}", path.display()));
                continue;
            }
        };

        let original_text = match simdutf8::basic::from_utf8(&original_bytes) {
            Ok(text) => text,
            Err(e) => {
                errors.push(format!("Non-UTF8 file {}: {e}", path.display()));
                continue;
            }
        };

        let text_outcome = build_replaced_text(original_text, &mut file_replacements);
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
        let replaced_bytes = new_content.into_bytes();
        let entry_payload_bytes = u64::try_from(original_bytes.len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(replaced_bytes.len()).unwrap_or(u64::MAX));
        if undo_payload_bytes.saturating_add(entry_payload_bytes)
            > effective_max_replace_undo_bytes()
        {
            skipped_paths.push(path.clone());
            errors.push(format!(
                "Skipped {}: undo data would exceed the 64 MB Replace All limit",
                path.display()
            ));
            continue;
        }

        let entry = ReplaceUndoEntry::new(original_bytes, replaced_bytes);
        if let Some(data_dir) = journal_data_dir {
            if !journal_prepared {
                if let Err(e) = search_backup::begin_incremental_journal(data_dir) {
                    errors.push(format!(
                        "Failed to prepare undo journal before replacing {}: {e}",
                        path.display()
                    ));
                    continue;
                }
                journal_prepared = true;
            }
            if let Err(e) = search_backup::save_entry(data_dir, &path, &entry) {
                errors.push(format!(
                    "Failed to persist undo journal before replacing {}: {e}",
                    path.display()
                ));
                continue;
            }
            if !journal_armed {
                if let Err(e) = search_backup::mark_incremental_journal_active(data_dir) {
                    errors.push(format!(
                        "Failed to activate undo journal before replacing {}: {e}",
                        path.display()
                    ));
                    if let Err(cleanup_error) = search_backup::delete_entry(data_dir, &path) {
                        errors.push(format!(
                            "Failed to remove inactive undo entry for {}: {cleanup_error}",
                            path.display()
                        ));
                    }
                    continue;
                }
                journal_armed = true;
            }
        }
        undo_payload_bytes = undo_payload_bytes.saturating_add(entry_payload_bytes);
        backup.insert(path.clone(), entry);

        assert_active_journal_before_write_for_test(journal_data_dir, &path);

        match atomic_write(&path, &backup[&path].replaced_bytes) {
            Ok(()) => {
                applied_paths.push(path.clone());
                record_replacement_success_counts(
                    &mut replaced_count,
                    &mut files_affected,
                    file_replaced,
                );
            }
            Err(ReplaceWriteError::BeforeRename(e)) => {
                errors.push(format!("Failed to write {}: {e}", path.display()));
                backup.remove(&path);
                if let Some(data_dir) = journal_data_dir
                    && let Err(journal_error) = search_backup::delete_entry(data_dir, &path)
                {
                    errors.push(format!(
                        "Failed to remove undo journal entry after write failure for {}: {journal_error}",
                        path.display()
                    ));
                }
                continue;
            }
            Err(ReplaceWriteError::AfterRename(e)) => {
                errors.push(format!(
                    "Replaced {}, but durability sync failed: {e}",
                    path.display()
                ));
                applied_paths.push(path.clone());
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

    if backup.is_empty()
        && journal_prepared
        && let Some(data_dir) = journal_data_dir
        && let Err(e) = search_backup::delete(data_dir)
    {
        errors.push(format!("Failed to clean empty undo journal: {e}"));
    }

    if files_affected == 0 && skipped_paths.is_empty() && !errors.is_empty() {
        return Err(anyhow::anyhow!("{}", errors.join("; ")));
    }

    let result = ReplaceResult {
        replaced_count,
        files_affected,
        skipped_paths,
        errors,
    };
    Ok(ApplyReplacementsOutcome {
        result,
        undo_backup: backup,
    })
}

fn assert_active_journal_before_write_for_test(journal_data_dir: Option<&Path>, path: &Path) {
    #[cfg(test)]
    TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE.with(|required| {
        if required.get() {
            let data_dir = journal_data_dir.expect("journal data dir required by test");
            let persisted = search_backup::load(data_dir)
                .expect("active undo journal must be readable before target write");
            assert!(
                persisted.contains_key(path),
                "target undo entry must be active before target write"
            );
        }
    });
    #[cfg(not(test))]
    let _ = (journal_data_dir, path);
}

fn effective_max_replace_undo_bytes() -> u64 {
    #[cfg(test)]
    {
        if let Some(override_value) = TEST_MAX_REPLACE_UNDO_BYTES.with(Cell::get) {
            return override_value;
        }
    }
    MAX_REPLACE_UNDO_BYTES
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
    file_replacements.sort_by(|a, b| {
        a.line_number
            .cmp(&b.line_number)
            .then(a.match_range.start.cmp(&b.match_range.start))
    });

    let line_spans = line_spans(original_text);
    let mut edits = Vec::with_capacity(file_replacements.len());

    // Validate against the original line snapshot before mutating anything so
    // stale search results skip the whole file instead of partially applying.
    for replacement in file_replacements.iter() {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Search result line numbers stay within usize indexing limits on supported editor workloads"
        )]
        let line_idx = replacement.line_number.saturating_sub(1) as usize;
        let Some(line_span) = line_spans.get(line_idx).cloned() else {
            return ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            };
        };
        let line = &original_text[line_span.clone()];
        if line != replacement.original_line.as_ref() {
            return ReplacementTextOutcome::StaleLine {
                line_number: replacement.line_number,
            };
        }
        let start = line.floor_char_boundary(replacement.match_range.start.min(line.len()));
        let end = line.ceil_char_boundary(replacement.match_range.end.min(line.len()));
        if start <= end {
            edits.push((
                line_span.start + start,
                line_span.start + end,
                replacement.replacement.as_ref(),
            ));
        }
    }

    if edits.is_empty() {
        return ReplacementTextOutcome::Unchanged;
    }

    let mut new_content = String::with_capacity(replaced_capacity(original_text.len(), &edits));
    let mut cursor = 0usize;
    for (start, end, replacement) in &edits {
        new_content.push_str(&original_text[cursor..*start]);
        new_content.push_str(replacement);
        cursor = *end;
    }
    new_content.push_str(&original_text[cursor..]);

    ReplacementTextOutcome::Replaced {
        new_content,
        replacement_count: edits.len(),
    }
}

/// Return byte ranges for each line without allocating owned line strings.
fn line_spans(text: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut line_start = 0usize;

    for index in memchr::memchr_iter(b'\n', bytes) {
        let line_end = if index > line_start && bytes[index - 1] == b'\r' {
            index - 1
        } else {
            index
        };
        spans.push(line_start..line_end);
        line_start = index + 1;
    }

    if line_start < bytes.len() {
        spans.push(line_start..bytes.len());
    }
    spans
}

/// Estimate final output capacity from the exact replacement ranges.
fn replaced_capacity(original_len: usize, edits: &[(usize, usize, &str)]) -> usize {
    let mut capacity = original_len;
    for (start, end, replacement) in edits {
        capacity = capacity
            .saturating_sub(end.saturating_sub(*start))
            .saturating_add(replacement.len());
    }
    capacity
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
        let Ok(_lock) = fs_write::TargetWriteGuard::acquire(path) else {
            failed_paths.push(path.clone());
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        };

        let Ok(current_facts) = fs_metadata::file_facts(path) else {
            failed_paths.push(path.clone());
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        };
        let original_len = u64::try_from(entry.original_bytes.len()).unwrap_or(u64::MAX);
        let replaced_len = u64::try_from(entry.replaced_bytes.len()).unwrap_or(u64::MAX);
        if current_facts.byte_size != original_len && current_facts.byte_size != replaced_len {
            skipped_paths.push(path.clone());
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }
        if current_facts.byte_size > MAX_REPLACE_FILE_BYTES {
            skipped_paths.push(path.clone());
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        let Ok(current_bytes) = fs_read::bytes(path) else {
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

/// Distinguishes write failures before and after the destination rename.
#[derive(Debug)]
enum ReplaceWriteError {
    /// The final path should still contain its previous bytes.
    BeforeRename(anyhow::Error),
    /// The rename already succeeded, but making the directory entry durable failed.
    AfterRename(anyhow::Error),
}

impl std::fmt::Display for ReplaceWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BeforeRename(error) | Self::AfterRename(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ReplaceWriteError {}

/// Atomically write bytes to a file through the shared durable-write helper.
///
/// Delegating to `durable_write` means Replace All inherits the same identity
/// metadata preservation (mode bits, ownership, ACLs, xattrs) and the same
/// before/after-rename failure classification as in-editor saves, instead of
/// re-implementing the contract here.
fn atomic_write(path: &Path, content: &[u8]) -> Result<(), ReplaceWriteError> {
    let write_path = fs_write::resolve_target_identity(path)
        .map_err(|source| {
            ReplaceWriteError::BeforeRename(anyhow::anyhow!(
                "Failed to resolve write target {}: {source}",
                path.display()
            ))
        })?
        .into_path_buf();
    fs_write::atomic_replace(&write_path, WriteLabel::REPLACE, content).map_err(|error| match error
    {
        fs_write::DurableWriteError::BeforeRename(source) => ReplaceWriteError::BeforeRename(
            anyhow::anyhow!("Failed to write {}: {source}", path.display()),
        ),
        fs_write::DurableWriteError::AfterRename(source) => {
            ReplaceWriteError::AfterRename(anyhow::anyhow!(
                "Failed to sync parent directory for {}: {source}",
                path.display()
            ))
        }
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
        let Ok(_guard) = fs_write::TargetWriteGuard::acquire(path) else {
            errors.push(format!("Failed to lock {} for rollback", path.display()));
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
    use crate::services::filesystem::fixture;
    use std::collections::HashSet;
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
        assert_eq!(MAX_REPLACE_FILE_BYTES, 10 * 1024 * 1024);
        assert_eq!(MAX_REPLACE_UNDO_BYTES, 64 * 1024 * 1024);
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
        assert!(result.skipped_paths.is_empty());

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

        let persisted =
            search_backup::load(journal_dir.path()).expect("expected operation to succeed");
        assert_eq!(persisted, backup);
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
        assert_eq!(result.skipped_paths, vec![file]);
        assert!(result.errors[0].contains("larger than the 10 MB"));
        assert!(backup.is_empty());
    }

    #[test]
    fn test_apply_replacements_allows_file_at_exact_size_cap() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("exact.rs");
        let original_line = "a".repeat(
            usize::try_from(MAX_REPLACE_FILE_BYTES)
                .expect("replace file byte cap should fit in usize"),
        );
        fixture::write_text(&file, &original_line);
        let replacements = vec![make_replacement(&file, 1, &original_line, "b", 0..0)];
        let cancel = AtomicBool::new(false);

        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel, None)
            .expect("exact file-size cap should still be replaceable")
            .into_parts();

        assert_eq!(result.files_affected, 1);
        assert_eq!(result.replaced_count, 1);
        assert!(result.skipped_paths.is_empty());
        assert!(result.errors.is_empty());
        assert_eq!(
            fs_metadata::file_facts(&file)
                .expect("stat replaced file")
                .byte_size,
            MAX_REPLACE_FILE_BYTES + 1
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
        assert_eq!(result.skipped_paths, vec![file_b.clone()]);
        assert!(result.errors[0].contains("undo data would exceed"));
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
        assert!(result.skipped_paths.is_empty());
        assert!(result.errors.is_empty());
        assert_eq!(fixture::read_text(&file), "done\n");
        assert!(backup.contains_key(&file));
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

        let outcome = build_replaced_text(original, &mut refs);

        assert_eq!(
            outcome,
            ReplacementTextOutcome::Replaced {
                new_content: "alpha hay\r\nbeta stack\n".to_string(),
                replacement_count: 2,
            }
        );
    }

    #[test]
    fn test_line_spans_handles_leading_newline_without_underflow() {
        assert_eq!(line_spans("\nnext"), vec![0..0, 1..5]);
        assert_eq!(line_spans("\r\nnext"), vec![0..0, 2..6]);
    }

    #[test]
    fn test_line_spans_does_not_add_empty_trailing_line() {
        assert_eq!(line_spans("first\n"), vec![0..5]);
        assert_eq!(line_spans("first\r\n"), vec![0..5]);
        assert_eq!(line_spans("\n"), vec![0..0]);
    }

    #[test]
    fn test_replaced_capacity_tracks_exact_growth_and_shrink() {
        let edits = [(0, 5, "hi"), (8, 10, "there!")];

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
        let result =
            apply_replacements(&replacements, &HashSet::new(), &cancel, Some(&journal_path));

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
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("Failed to stat"));
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
        assert_eq!(outcome.skipped_paths, vec![file.clone()]);
        assert!(outcome.failed_paths.is_empty());
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
        assert_eq!(outcome.skipped_paths, vec![file.clone()]);
        assert!(outcome.failed_paths.is_empty());
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

        assert_eq!(outcome.restored_paths, vec![file.clone()]);
        assert!(outcome.skipped_paths.is_empty());
        assert!(outcome.failed_paths.is_empty());
        assert!(outcome.remaining_backup.is_empty());
        assert_eq!(fixture::read_text(&file), "before\n");
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

        persist_undo_backup(dir.path(), &ReplaceUndoBackup::new())
            .expect("delete empty undo backup");
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

        assert_eq!(outcome.restored_paths, vec![restored_file.clone()]);
        assert_eq!(outcome.failed_paths, vec![missing_file.clone()]);
        assert!(outcome.skipped_paths.is_empty());
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
        assert_eq!(result.skipped_paths.len(), 1);
        assert_eq!(result.skipped_paths[0], file_b);
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
        assert!(
            outcome.restored_paths.contains(&file),
            "undo should restore the file"
        );
        let mode_after_undo = fixture::mode(&file) & 0o777;
        assert_eq!(
            mode_after_undo, 0o600,
            "undoing Replace All must also keep the file's restrictive mode"
        );
    }
}

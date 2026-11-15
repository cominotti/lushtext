// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace All undo and cancellation rollback.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::apply::atomic_write;
use super::{
    MAX_REPLACE_FILE_BYTES, ReplaceUndoBackup, ReplaceUndoEntry, UndoReplaceOutcome,
    run_undo_after_metadata_hook_for_test,
};
use crate::model::content_search::BoundedDiagnosticSample;
use crate::services::filesystem::{metadata as fs_metadata, read as fs_read, write as fs_write};

/// Restore files from backup (undo Replace All).
///
/// Writes each file atomically (temp file + rename), but only when the file's
/// current bytes still match the Replace All output snapshot. Per-file failures
/// stay in `remaining_backup` so the UI can keep undo available for retry.
#[must_use]
pub fn undo_replacements(backup: &ReplaceUndoBackup) -> UndoReplaceOutcome {
    undo_replacements_for_open_identities(backup, &HashSet::new())
}

/// Restore a backup and return only restored paths that intersect open tabs.
#[must_use]
pub fn undo_replacements_for_open_identities(
    backup: &ReplaceUndoBackup,
    open_canonical_identities: &HashSet<PathBuf>,
) -> UndoReplaceOutcome {
    let mut restored_count = 0usize;
    let mut skipped_paths = BoundedDiagnosticSample::default();
    let mut failed_paths = BoundedDiagnosticSample::default();
    let mut restored_open_paths = Vec::new();
    let mut remaining_backup = ReplaceUndoBackup::new();

    for (path, entry) in backup {
        let Ok(_lock) = fs_write::TargetWriteGuard::acquire(path) else {
            failed_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        };

        let Ok(current_facts) = fs_metadata::file_facts(path) else {
            failed_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        };
        let original_len = u64::try_from(entry.original_bytes.len()).unwrap_or(u64::MAX);
        let replaced_len = u64::try_from(entry.replaced_bytes.len()).unwrap_or(u64::MAX);
        if current_facts.byte_size != original_len && current_facts.byte_size != replaced_len {
            skipped_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }
        if current_facts.byte_size > MAX_REPLACE_FILE_BYTES {
            skipped_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        run_undo_after_metadata_hook_for_test(path);
        let current_bytes = match fs_read::bounded_bytes(
            path,
            MAX_REPLACE_FILE_BYTES,
            current_facts.byte_size,
            || false,
        ) {
            Ok(bytes) => bytes,
            Err(fs_read::BoundedFileReadError::LimitExceeded { .. }) => {
                skipped_paths.record_path(path);
                remaining_backup.insert(path.clone(), entry.clone());
                continue;
            }
            Err(fs_read::BoundedFileReadError::Cancelled) => {
                unreachable!("Undo uses a non-cancelling bounded reader")
            }
            Err(fs_read::BoundedFileReadError::Io(_)) => {
                failed_paths.record_path(path);
                remaining_backup.insert(path.clone(), entry.clone());
                continue;
            }
        };

        if current_bytes == entry.original_bytes {
            restored_count = restored_count.saturating_add(1);
            record_open_path_intersection(
                &mut restored_open_paths,
                path,
                current_facts.canonical_path.as_deref(),
                open_canonical_identities,
            );
            continue;
        }

        if current_bytes != entry.replaced_bytes {
            skipped_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        if atomic_write(path, &entry.original_bytes).is_err() {
            failed_paths.record_path(path);
            remaining_backup.insert(path.clone(), entry.clone());
            continue;
        }

        restored_count = restored_count.saturating_add(1);
        record_open_path_intersection(
            &mut restored_open_paths,
            path,
            current_facts.canonical_path.as_deref(),
            open_canonical_identities,
        );
    }

    UndoReplaceOutcome {
        restored_count,
        skipped_count: skipped_paths.total_count(),
        failed_count: failed_paths.total_count(),
        restored_open_paths,
        skipped_sample: skipped_paths,
        failed_sample: failed_paths,
        remaining_backup,
    }
}

pub(super) fn record_open_path_intersection(
    out: &mut Vec<PathBuf>,
    path: &Path,
    canonical_path: Option<&Path>,
    open_canonical_identities: &HashSet<PathBuf>,
) {
    let identity = canonical_path.unwrap_or(path);
    if open_canonical_identities.contains(identity) {
        out.push(path.to_path_buf());
    }
}

/// Restore already-written files in reverse order when cancellation interrupts a run.
///
/// Each file is validated before it is rewritten, for the same reason
/// [`undo_replacements_for_open_identities`] validates: the per-file
/// [`fs_write::TargetWriteGuard`] is released as the loop moves on, so an editor
/// save of a later target can land while rollback is still working. Writing
/// `original_bytes` unconditionally would then discard content the user just
/// saved — and a fully "successful" rollback deletes the journal, leaving no
/// copy of it. A file that no longer holds this run's `replaced_bytes` is
/// therefore reported as an unrestored path, which keeps the journal.
pub(super) fn rollback_applied_files(backup: &ReplaceUndoBackup) -> BoundedDiagnosticSample {
    let mut errors = BoundedDiagnosticSample::default();
    for (path, entry) in backup.iter().rev() {
        let Ok(_guard) = fs_write::TargetWriteGuard::acquire(path) else {
            errors.push(format!("Failed to lock {} for rollback", path.display()));
            continue;
        };
        match rollback_file_disposition(path, entry) {
            RollbackDisposition::AlreadyOriginal => {}
            RollbackDisposition::Restore => {
                if let Err(e) = atomic_write(path, &entry.original_bytes) {
                    errors.push(format!("Failed to restore {}: {e}", path.display()));
                }
            }
            RollbackDisposition::ChangedSinceReplacement => {
                errors.push(format!(
                    "Skipped rollback of {}: changed after Replace All wrote it",
                    path.display()
                ));
            }
        }
    }
    errors
}

/// What rollback may do with one already-written file.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum RollbackDisposition {
    /// The file already holds its pre-replacement bytes; nothing to write.
    AlreadyOriginal,
    /// The file still holds this run's replacement bytes; restore it.
    Restore,
    /// The file holds neither; restoring would destroy newer content.
    ChangedSinceReplacement,
}

/// Classify one rollback target by its current bytes.
///
/// Any read or metadata failure classifies as changed rather than restorable, so
/// an unverifiable file is reported and its journal entry kept instead of being
/// overwritten on a guess.
pub(super) fn rollback_file_disposition(
    path: &Path,
    entry: &ReplaceUndoEntry,
) -> RollbackDisposition {
    let Ok(facts) = fs_metadata::file_facts(path) else {
        return RollbackDisposition::ChangedSinceReplacement;
    };
    // No separate size pre-check: `bounded_bytes` already refuses a file larger
    // than the limit, and every read failure classifies the same way here.
    let Ok(current_bytes) =
        fs_read::bounded_bytes(path, MAX_REPLACE_FILE_BYTES, facts.byte_size, || false)
    else {
        return RollbackDisposition::ChangedSinceReplacement;
    };
    if current_bytes == entry.original_bytes {
        return RollbackDisposition::AlreadyOriginal;
    }
    if current_bytes == entry.replaced_bytes {
        return RollbackDisposition::Restore;
    }
    RollbackDisposition::ChangedSinceReplacement
}

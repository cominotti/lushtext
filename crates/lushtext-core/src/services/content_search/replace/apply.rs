// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace All apply orchestration: per-file admission, journal, and durable writes.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(any(test, feature = "test-utils"))]
use super::TEST_MAX_REPLACE_UNDO_BYTES;
#[cfg(test)]
use super::TEST_REQUIRE_ACTIVE_JOURNAL_BEFORE_WRITE;
use super::stream::{ReplacementTextOutcome, build_replaced_text};
use super::undo::rollback_applied_files;
use super::{
    ApplyReplacementsOutcome, MAIN_REPLACE_FILE_BYTES, MAX_REPLACE_FILE_BYTES,
    MAX_REPLACE_UNDO_BYTES, MAX_REPLACE_UNDO_RETAINED_BYTES, ReplaceConstructionMetrics,
    ReplaceJournalFreshness, ReplaceUndoBackup, ReplaceUndoEntry,
    replace_undo_entry_retained_byte_weight, replace_undo_retained_byte_weight,
    take_replace_before_rename_failure_for_test,
};
use crate::model::content_search::{
    BoundedDiagnosticSample, MAX_REPLACE_PREVIEW_ROWS, ReplaceResult, Replacement,
    UndoPayloadLedger,
};
use crate::model::size_units::iec_size;
use crate::services::{
    filesystem::{WriteLabel, metadata as fs_metadata, read as fs_read, write as fs_write},
    search_backup,
};
#[cfg(any(test, feature = "test-utils"))]
use std::cell::Cell;

/// Apply replacements to files on disk.
///
/// Groups replacements by file, streams sorted source ranges into changed output,
/// and writes atomically (temp file + rename). Returns the replacement summary
/// and a backup mapping file paths to their before/after content snapshots for undo.
///
/// Per-file errors are collected (not early-returned) so that already-replaced files
/// remain in the backup for undo. Only returns `Err` if zero files could be processed.
///
/// `skip_paths` lists files that should NOT be replaced (e.g., open tabs with unsaved changes).
/// Skipped files are excluded from the result count but included in the exact
/// count and bounded `ReplaceResult::skipped_sample` diagnostic projection.
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
    apply_replacements_inner(
        replacements,
        skip_paths,
        &HashSet::new(),
        cancel,
        journal_data_dir,
        None,
    )
    .and_then(|outcome| {
        outcome.ok_or_else(|| anyhow::anyhow!("unguarded Replace All became stale"))
    })
}

/// Apply only if the UI reservation is still current after acquiring the journal lock.
pub(crate) fn apply_replacements_if_current(
    replacements: &[Replacement],
    skip_paths: &HashSet<PathBuf>,
    open_canonical_identities: &HashSet<PathBuf>,
    cancel: &AtomicBool,
    journal_data_dir: &Path,
    freshness: &ReplaceJournalFreshness,
) -> anyhow::Result<Option<ApplyReplacementsOutcome>> {
    apply_replacements_inner(
        replacements,
        skip_paths,
        open_canonical_identities,
        cancel,
        Some(journal_data_dir),
        Some(freshness),
    )
}

pub(super) fn apply_replacements_inner(
    replacements: &[Replacement],
    skip_paths: &HashSet<PathBuf>,
    open_canonical_identities: &HashSet<PathBuf>,
    cancel: &AtomicBool,
    journal_data_dir: Option<&Path>,
    freshness: Option<&ReplaceJournalFreshness>,
) -> anyhow::Result<Option<ApplyReplacementsOutcome>> {
    if replacements.len() > MAX_REPLACE_PREVIEW_ROWS {
        anyhow::bail!(
            "Replace All selection exceeds the {MAX_REPLACE_PREVIEW_ROWS}-replacement limit"
        );
    }
    let _journal_guard = journal_data_dir
        .map(|_| search_backup::acquire_journal_guard())
        .transpose()?;
    // The journal lock serializes this decision with startup recovery and all
    // journal commits. A stale UI generation must exit before preparing a new
    // journal or mutating any target file.
    if freshness.is_some_and(|freshness| !freshness.is_current()) {
        return Ok(None);
    }
    let mut by_file: BTreeMap<PathBuf, Vec<&Replacement>> = BTreeMap::new();
    for r in replacements {
        by_file.entry(r.path.clone()).or_default().push(r);
    }

    let mut backup = ReplaceUndoBackup::new();
    let mut replaced_count = 0usize;
    let mut files_affected = 0usize;
    let mut skipped_sample = BoundedDiagnosticSample::default();
    let mut errors = BoundedDiagnosticSample::default();
    let mut affected_open_paths = Vec::new();
    let mut cancelled = false;
    let mut undo_ledger = UndoPayloadLedger::new(effective_max_replace_undo_bytes());
    let mut retained_undo_ledger = UndoPayloadLedger::new(MAX_REPLACE_UNDO_RETAINED_BYTES);
    let mut metrics = ReplaceConstructionMetrics::default();
    let mut journal_prepared = false;
    let mut journal_armed = false;

    // Two passes in path order: files `main` could replace (source and output
    // within its 10 MiB), then the files of up to 16 MiB the first pass defers.
    let mut pending: Vec<_> = by_file.into_iter().collect();
    let mut deferred = Vec::new();
    'passes: for pass_file_bytes in [MAIN_REPLACE_FILE_BYTES, MAX_REPLACE_FILE_BYTES] {
        let first_pass = pass_file_bytes < MAX_REPLACE_FILE_BYTES;
        for (path, mut file_replacements) in std::mem::take(&mut pending) {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break 'passes;
            }

            if skip_paths.contains(&path) {
                skipped_sample.record_path(&path);
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
                break 'passes;
            }

            let facts = match fs_metadata::file_facts(&path) {
                Ok(facts) => facts,
                Err(e) => {
                    errors.push(format!("Failed to stat {}: {e}", path.display()));
                    continue;
                }
            };
            if facts.byte_size > MAX_REPLACE_FILE_BYTES {
                skipped_sample.record_path(&path);
                errors.push(format!(
                    "Skipped {}: file is larger than the {} Replace All limit",
                    path.display(),
                    iec_size(MAX_REPLACE_FILE_BYTES),
                ));
                continue;
            }
            if first_pass && facts.byte_size > pass_file_bytes {
                deferred.push((path, file_replacements));
                continue;
            }

            let original_bytes =
                match fs_read::bounded_bytes(&path, MAX_REPLACE_FILE_BYTES, facts.byte_size, || {
                    cancel.load(Ordering::Relaxed)
                }) {
                    Ok(bytes) => bytes,
                    Err(fs_read::BoundedFileReadError::Cancelled) => {
                        cancelled = true;
                        break 'passes;
                    }
                    Err(fs_read::BoundedFileReadError::LimitExceeded { .. }) => {
                        skipped_sample.record_path(&path);
                        errors.push(format!(
                            "Skipped {}: file is larger than the {} Replace All limit",
                            path.display(),
                            iec_size(MAX_REPLACE_FILE_BYTES),
                        ));
                        continue;
                    }
                    Err(fs_read::BoundedFileReadError::Io(e)) => {
                        errors.push(format!("Failed to read {}: {e}", path.display()));
                        continue;
                    }
                };

            if first_pass
                && u64::try_from(original_bytes.len()).unwrap_or(u64::MAX) > pass_file_bytes
            {
                deferred.push((path, file_replacements));
                continue;
            }

            let original_text = match simdutf8::basic::from_utf8(&original_bytes) {
                Ok(text) => text,
                Err(e) => {
                    errors.push(format!("Non-UTF8 file {}: {e}", path.display()));
                    continue;
                }
            };

            let remaining_undo_bytes = undo_ledger
                .remaining_bytes()
                .saturating_sub(u64::try_from(original_bytes.len()).unwrap_or(u64::MAX));
            let undo_output_limit = usize::try_from(remaining_undo_bytes).unwrap_or(usize::MAX);
            let file_output_limit = usize::try_from(pass_file_bytes).unwrap_or(usize::MAX);
            let output_limit = undo_output_limit.min(file_output_limit);
            let output_limited_by_file_size = file_output_limit <= undo_output_limit;
            let text_build =
                build_replaced_text(original_text, &mut file_replacements, output_limit, || {
                    cancel.load(Ordering::Relaxed)
                });
            metrics.absorb_construction(text_build.metrics);
            let (new_content, file_replaced) = match text_build.outcome {
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
                ReplacementTextOutcome::InvalidPreview { reason } => {
                    errors.push(format!(
                        "Skipped {}: invalid replacement preview ({reason})",
                        path.display(),
                    ));
                    continue;
                }
                ReplacementTextOutcome::OutputLimitExceeded
                    if first_pass && output_limited_by_file_size =>
                {
                    deferred.push((path, file_replacements));
                    continue;
                }
                ReplacementTextOutcome::OutputLimitExceeded => {
                    skipped_sample.record_path(&path);
                    if output_limited_by_file_size {
                        errors.push(format!(
                            "Skipped {}: replacement output would exceed the {} per-file limit",
                            path.display(),
                            iec_size(MAX_REPLACE_FILE_BYTES),
                        ));
                    } else {
                        errors.push(format!(
                            "Skipped {}: undo data would exceed the {} Replace All limit",
                            path.display(),
                            iec_size(MAX_REPLACE_UNDO_BYTES),
                        ));
                    }
                    continue;
                }
                ReplacementTextOutcome::Cancelled => {
                    cancelled = true;
                    break 'passes;
                }
                ReplacementTextOutcome::Unchanged => continue,
            };
            let replaced_bytes = new_content.into_bytes();
            let entry_payload_bytes = u64::try_from(original_bytes.len())
                .unwrap_or(u64::MAX)
                .saturating_add(u64::try_from(replaced_bytes.len()).unwrap_or(u64::MAX));
            if !undo_ledger.try_charge(entry_payload_bytes) {
                skipped_sample.record_path(&path);
                errors.push(format!(
                    "Skipped {}: undo data would exceed the {} Replace All limit",
                    path.display(),
                    iec_size(MAX_REPLACE_UNDO_BYTES),
                ));
                continue;
            }

            let entry = ReplaceUndoEntry::new(original_bytes, replaced_bytes);
            let backup_path = path.clone();
            let retained_entry_bytes =
                replace_undo_entry_retained_byte_weight(&backup_path, &entry);
            if !retained_undo_ledger.try_charge(retained_entry_bytes) {
                let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                debug_assert_eq!(reclaimed, entry_payload_bytes);
                skipped_sample.record_path(&path);
                errors.push(format!(
                    "Skipped {}: complete undo state would exceed the {} Replace All limit",
                    path.display(),
                    iec_size(MAX_REPLACE_UNDO_BYTES),
                ));
                continue;
            }
            if let Some(data_dir) = journal_data_dir {
                if !journal_prepared {
                    if let Err(e) = search_backup::begin_incremental_journal(data_dir) {
                        errors.push(format!(
                            "Failed to prepare undo journal before replacing {}: {e}",
                            path.display()
                        ));
                        let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                        debug_assert_eq!(reclaimed, entry_payload_bytes);
                        let retained_reclaimed = retained_undo_ledger.reclaim(retained_entry_bytes);
                        debug_assert_eq!(retained_reclaimed, retained_entry_bytes);
                        continue;
                    }
                    journal_prepared = true;
                }
                if let Err(e) = search_backup::save_entry(data_dir, &path, &entry) {
                    errors.push(format!(
                        "Failed to persist undo journal before replacing {}: {e}",
                        path.display()
                    ));
                    let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                    debug_assert_eq!(reclaimed, entry_payload_bytes);
                    let retained_reclaimed = retained_undo_ledger.reclaim(retained_entry_bytes);
                    debug_assert_eq!(retained_reclaimed, retained_entry_bytes);
                    if let Err(cleanup_error) = search_backup::delete_entry(data_dir, &path) {
                        errors.push(format!(
                            "Failed to remove incomplete undo entry for {}: {cleanup_error}",
                            path.display()
                        ));
                    }
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
                        let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                        debug_assert_eq!(reclaimed, entry_payload_bytes);
                        let retained_reclaimed = retained_undo_ledger.reclaim(retained_entry_bytes);
                        debug_assert_eq!(retained_reclaimed, retained_entry_bytes);
                        continue;
                    }
                    journal_armed = true;
                }
            }
            backup.insert(backup_path, entry);
            metrics.undo_bytes = undo_ledger.high_water_bytes();

            assert_active_journal_before_write_for_test(journal_data_dir, &path);

            match atomic_write(&path, &backup[&path].replaced_bytes) {
                Ok(()) => {
                    let identity = facts.canonical_path.as_deref().unwrap_or(&path);
                    if open_canonical_identities.contains(identity) {
                        affected_open_paths.push(path.clone());
                    }
                    record_replacement_success_counts(
                        &mut replaced_count,
                        &mut files_affected,
                        file_replaced,
                    );
                }
                Err(ReplaceWriteError::BeforeRename(e)) => {
                    errors.push(format!("Failed to write {}: {e}", path.display()));
                    backup.remove(&path);
                    let reclaimed = undo_ledger.reclaim(entry_payload_bytes);
                    debug_assert_eq!(reclaimed, entry_payload_bytes);
                    let retained_reclaimed = retained_undo_ledger.reclaim(retained_entry_bytes);
                    debug_assert_eq!(retained_reclaimed, retained_entry_bytes);
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
                    let identity = facts.canonical_path.as_deref().unwrap_or(&path);
                    if open_canonical_identities.contains(identity) {
                        affected_open_paths.push(path.clone());
                    }
                    record_replacement_success_counts(
                        &mut replaced_count,
                        &mut files_affected,
                        file_replaced,
                    );
                }
            }
        }
        pending = std::mem::take(&mut deferred);
    }

    if cancelled {
        let rollback_errors = rollback_applied_files(&backup);
        if rollback_errors.total_count() == 0 {
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
            rollback_errors.summary("Replace cancelled; rollback failed")
        ));
    }

    if backup.is_empty()
        && journal_prepared
        && let Some(data_dir) = journal_data_dir
        && let Err(e) = search_backup::delete(data_dir)
    {
        errors.push(format!("Failed to clean empty undo journal: {e}"));
    }

    if files_affected == 0 && skipped_sample.total_count() == 0 && errors.total_count() > 0 {
        return Err(anyhow::anyhow!(errors.summary("Replace All failed")));
    }

    debug_assert_eq!(
        retained_undo_ledger.live_bytes(),
        replace_undo_retained_byte_weight(&backup)
    );
    debug_assert!(retained_undo_ledger.live_bytes() <= MAX_REPLACE_UNDO_RETAINED_BYTES);
    metrics.undo_live_bytes = undo_ledger.live_bytes();

    let result = ReplaceResult {
        replaced_count,
        files_affected,
        skipped_count: skipped_sample.total_count(),
        error_count: errors.total_count(),
        skipped_sample,
        error_sample: errors,
        affected_open_paths,
    };
    Ok(Some(ApplyReplacementsOutcome {
        result,
        undo_backup: backup,
        metrics,
    }))
}

pub(super) fn assert_active_journal_before_write_for_test(
    journal_data_dir: Option<&Path>,
    path: &Path,
) {
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

pub(super) fn effective_max_replace_undo_bytes() -> u64 {
    #[cfg(any(test, feature = "test-utils"))]
    {
        if let Some(override_value) = TEST_MAX_REPLACE_UNDO_BYTES.with(Cell::get) {
            return override_value;
        }
    }
    MAX_REPLACE_UNDO_BYTES
}

/// Fold one successfully written file into the public Replace All counters.
pub(super) fn record_replacement_success_counts(
    replaced_count: &mut usize,
    files_affected: &mut usize,
    file_replaced: usize,
) {
    *replaced_count += file_replaced;
    *files_affected += 1;
}

/// Distinguishes write failures before and after the destination rename.
#[derive(Debug)]
pub(super) enum ReplaceWriteError {
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
pub(super) fn atomic_write(path: &Path, content: &[u8]) -> Result<(), ReplaceWriteError> {
    if take_replace_before_rename_failure_for_test(path) {
        return Err(ReplaceWriteError::BeforeRename(anyhow::anyhow!(
            "Injected pre-rename Replace All failure for {}",
            path.display()
        )));
    }
    let write_path = fs_write::resolve_target_identity(path)
        .map_err(|source| {
            ReplaceWriteError::BeforeRename(anyhow::anyhow!(
                "Failed to resolve write target {}: {source}",
                path.display()
            ))
        })?
        .into_path_buf();
    fs_write::atomic_replace_workspace_file(&write_path, WriteLabel::REPLACE, content).map_err(
        |error| match error {
            fs_write::DurableWriteError::BeforeRename(source) => ReplaceWriteError::BeforeRename(
                anyhow::anyhow!("Failed to write {}: {source}", path.display()),
            ),
            fs_write::DurableWriteError::AfterRename(source) => {
                ReplaceWriteError::AfterRename(anyhow::anyhow!(
                    "Failed to sync parent directory for {}: {source}",
                    path.display()
                ))
            }
        },
    )
}

/// Persist the current undo backup snapshot or delete the journal when empty.
pub(super) fn persist_undo_backup(
    data_dir: &Path,
    backup: &ReplaceUndoBackup,
) -> anyhow::Result<()> {
    if backup.is_empty() {
        search_backup::delete(data_dir)
    } else {
        search_backup::save(data_dir, backup)
    }
}

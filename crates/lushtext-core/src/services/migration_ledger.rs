// SPDX-License-Identifier: GPL-3.0-or-later

//! Durable retry ledger for post-rename sidecar and local-history migrations.
//!
//! Rename workflows update user files first, then migrate app-owned sidecars in
//! background workers. This service records that follow-up work before it runs
//! and retries incomplete kinds on later startup without a tight retry loop.

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::model::migration_ledger::{
    MigrationEntry, MigrationKind, MigrationKindState, MigrationLedgerDocument,
};
use crate::model::sidecar_identity::now_epoch_secs;
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass,
    load_json_or_default, save_json_path,
};
use crate::services::{
    bookmark_service, document_note_service, local_history_service, workspace_note_service,
};

/// Persistent ledger filename under the app data directory.
const MIGRATION_LEDGER_FILENAME: &str = "migration-ledger.json";
/// Stop automatic startup retry after this many failures for one kind.
pub const MAX_MIGRATION_ATTEMPTS: u32 = 3;

fn ledger_lock() -> &'static Mutex<()> {
    // Process-local mutex serializes the JSON load/mutate/save cycle so
    // concurrent completions do not overwrite attempts or stale generations.
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Result of a ledger state mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerUpdateOutcome {
    /// The ledger entry and kind were found and updated.
    Updated,
    /// The generation no longer exists, usually because newer work completed it.
    EntryMissing,
    /// The entry exists but does not track the requested kind.
    KindMissing,
}

/// One retry or skip diagnostic emitted while reconciling pending migrations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationDiagnostic {
    /// Entry generation that produced the diagnostic.
    pub generation: u64,
    /// Data category affected by the diagnostic.
    pub kind: MigrationKind,
    /// Human-readable diagnostic summary.
    pub message: String,
}

/// Aggregate result from one reconciliation pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MigrationReconcileReport {
    /// Number of incomplete kinds considered for retry.
    pub considered: usize,
    /// Number of kinds retried during this pass.
    pub attempted: usize,
    /// Number of kinds completed during this pass.
    pub completed: usize,
    /// Number of kinds skipped because their failure budget is exhausted.
    pub skipped: usize,
    /// Retry failures and skipped-kind summaries.
    pub diagnostics: Vec<MigrationDiagnostic>,
}

/// Path to the durable migration ledger file.
#[must_use]
pub fn ledger_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(MIGRATION_LEDGER_FILENAME)
}

/// Load the migration ledger through recovery-aware metadata handling.
#[must_use]
pub fn load_recovering(data_dir: &Path) -> RecoveryLoad<MigrationLedgerDocument> {
    let path = ledger_path(data_dir);
    load_json_or_default(&RecoveryLoadConfig::new(
        data_dir,
        &path,
        RecoveryMetadataClass::MigrationLedger,
    ))
}

/// Record or extend a pending rename migration entry.
///
/// Returns the entry generation that callers should use when reporting kind
/// completion or failure.
///
/// # Errors
///
/// Returns an error if the ledger is unsafe to replace or cannot be written.
pub fn record_pending(
    data_dir: &Path,
    old_path: &Path,
    new_path: &Path,
    kinds: &[MigrationKind],
) -> Result<u64> {
    let _guard = ledger_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("migration-ledger lock poisoned"))?;
    let now = now_epoch_secs();
    let mut ledger = load_for_update(data_dir)?;
    ledger.remove_completed();

    if let Some(entry) = ledger
        .entries
        .iter_mut()
        .find(|entry| entry.matches_paths(old_path, new_path))
    {
        entry.ensure_kinds(kinds, now);
        let generation = entry.generation;
        save(data_dir, &ledger)?;
        return Ok(generation);
    }

    let generation = ledger.allocate_generation();
    ledger.entries.push(MigrationEntry::new(
        generation,
        old_path.to_path_buf(),
        new_path.to_path_buf(),
        kinds,
        now,
    ));
    save(data_dir, &ledger)?;
    Ok(generation)
}

/// Mark one migration kind as completed.
///
/// # Errors
///
/// Returns an error if the ledger cannot be written.
pub fn mark_kind_completed(
    data_dir: &Path,
    generation: u64,
    kind: MigrationKind,
) -> Result<LedgerUpdateOutcome> {
    update_kind_state(data_dir, generation, kind, |state, now| {
        state.completed = true;
        state.last_attempt_secs = Some(now);
        state.last_error = None;
    })
}

/// Mark one migration kind as failed and increment its attempt count.
///
/// # Errors
///
/// Returns an error if the ledger cannot be written.
pub fn mark_kind_failed(
    data_dir: &Path,
    generation: u64,
    kind: MigrationKind,
    error: &anyhow::Error,
) -> Result<LedgerUpdateOutcome> {
    let detail = error.to_string();
    update_kind_state(data_dir, generation, kind, move |state, now| {
        state.completed = false;
        state.attempts = state.attempts.saturating_add(1);
        state.last_attempt_secs = Some(now);
        state.last_error = Some(detail);
    })
}

/// Run one migration operation and update the ledger with its result.
///
/// # Errors
///
/// Returns the migration error, or a ledger update error if state persistence
/// fails after the migration operation succeeds.
pub fn run_tracked_kind<T>(
    data_dir: &Path,
    generation: u64,
    kind: MigrationKind,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    match operation() {
        Ok(value) => {
            mark_kind_completed(data_dir, generation, kind)?;
            Ok(value)
        }
        Err(error) => {
            if let Err(ledger_error) = mark_kind_failed(data_dir, generation, kind, &error) {
                tracing::warn!(
                    "Failed to update migration ledger after {} failure: {ledger_error}",
                    kind.label()
                );
            }
            Err(error)
        }
    }
}

/// Retry all pending migration kinds that are still below the attempt cap.
///
/// # Errors
///
/// Returns an error if the ledger cannot be loaded or updated. Individual
/// migration failures are reported in the returned diagnostics and left pending.
pub fn reconcile_pending(data_dir: &Path) -> Result<MigrationReconcileReport> {
    let load = load_recovering(data_dir);
    if !load.replacement_allowed() {
        return Err(anyhow::anyhow!(
            "migration ledger is not safe to update after recovery diagnostics"
        ));
    }

    let mut report = MigrationReconcileReport::default();
    // Take a ledger snapshot, run filesystem migrations without holding the
    // ledger lock, then mark by generation so slow I/O does not block unrelated
    // updates and stale completions are ignored.
    let entries = load.value.entries;
    for entry in entries {
        for state in entry.kinds.iter().filter(|state| !state.completed) {
            report.considered += 1;
            if state.attempts >= MAX_MIGRATION_ATTEMPTS {
                report.skipped += 1;
                report.diagnostics.push(MigrationDiagnostic {
                    generation: entry.generation,
                    kind: state.kind,
                    message: format!(
                        "{} migration reached retry limit after {} attempts",
                        state.kind.label(),
                        state.attempts
                    ),
                });
                continue;
            }

            report.attempted += 1;
            match run_migration_kind(data_dir, &entry, state.kind) {
                Ok(()) => {
                    let outcome = mark_kind_completed(data_dir, entry.generation, state.kind)?;
                    if outcome == LedgerUpdateOutcome::Updated {
                        report.completed += 1;
                    }
                }
                Err(error) => {
                    let _ = mark_kind_failed(data_dir, entry.generation, state.kind, &error)?;
                    report.diagnostics.push(MigrationDiagnostic {
                        generation: entry.generation,
                        kind: state.kind,
                        message: error.to_string(),
                    });
                }
            }
        }
    }

    Ok(report)
}

fn load_for_update(data_dir: &Path) -> Result<MigrationLedgerDocument> {
    let load = load_recovering(data_dir);
    if !load.replacement_allowed() {
        let detail = load
            .diagnostics
            .iter()
            .map(RecoveryDiagnostic::summary)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(anyhow::anyhow!(
            "migration ledger is not safe to replace: {detail}"
        ));
    }
    Ok(load.value)
}

fn save(data_dir: &Path, ledger: &MigrationLedgerDocument) -> Result<()> {
    save_json_path(&ledger_path(data_dir), ledger)
        .with_context(|| format!("failed to save {}", ledger_path(data_dir).display()))
}

fn update_kind_state(
    data_dir: &Path,
    generation: u64,
    kind: MigrationKind,
    update: impl FnOnce(&mut MigrationKindState, u64),
) -> Result<LedgerUpdateOutcome> {
    let _guard = ledger_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("migration-ledger lock poisoned"))?;
    let now = now_epoch_secs();
    let mut ledger = load_for_update(data_dir)?;
    let Some(entry) = ledger.entry_mut(generation) else {
        return Ok(LedgerUpdateOutcome::EntryMissing);
    };
    let Some(state) = entry.kind_state_mut(kind) else {
        return Ok(LedgerUpdateOutcome::KindMissing);
    };

    update(state, now);
    entry.updated_at_secs = now;
    ledger.remove_completed();
    save(data_dir, &ledger)?;
    Ok(LedgerUpdateOutcome::Updated)
}

fn run_migration_kind(data_dir: &Path, entry: &MigrationEntry, kind: MigrationKind) -> Result<()> {
    match kind {
        MigrationKind::Bookmarks => {
            bookmark_service::move_path_tree(data_dir, &entry.old_path, &entry.new_path)?;
        }
        MigrationKind::DocumentNotes => {
            document_note_service::move_path_tree(data_dir, &entry.old_path, &entry.new_path)?;
        }
        MigrationKind::WorkspaceNotes => {
            workspace_note_service::move_root_tree(data_dir, &entry.old_path, &entry.new_path)?;
        }
        MigrationKind::LocalHistory => {
            local_history_service::move_path_tree(data_dir, &entry.old_path, &entry.new_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::bookmark::BookmarkRecord;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn seed_file(root: &Path, relative: &str) -> std::path::PathBuf {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fixture::create_dir_all(parent);
        }
        fixture::write_text(&path, "contents\n");
        path
    }

    #[test]
    fn record_pending_merges_kinds_for_same_rename() {
        let dir = TempDir::new().expect("tempdir");
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        let generation = record_pending(
            dir.path(),
            &old_path,
            &new_path,
            &[MigrationKind::Bookmarks],
        )
        .expect("record bookmark");
        let second_generation = record_pending(
            dir.path(),
            &old_path,
            &new_path,
            &[MigrationKind::LocalHistory],
        )
        .expect("record local history");

        assert_eq!(generation, second_generation);
        let ledger = load_recovering(dir.path()).value;
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(
            ledger.entries[0].incomplete_kinds(),
            vec![MigrationKind::Bookmarks, MigrationKind::LocalHistory]
        );
    }

    #[test]
    fn completed_kind_removes_entry_when_all_work_is_done() {
        let dir = TempDir::new().expect("tempdir");
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");
        let generation = record_pending(
            dir.path(),
            &old_path,
            &new_path,
            &[MigrationKind::Bookmarks],
        )
        .expect("record pending");

        let outcome = mark_kind_completed(dir.path(), generation, MigrationKind::Bookmarks)
            .expect("mark completed");

        assert_eq!(outcome, LedgerUpdateOutcome::Updated);
        assert!(load_recovering(dir.path()).value.entries.is_empty());
    }

    #[test]
    fn stale_generation_completion_is_ignored() {
        let dir = TempDir::new().expect("tempdir");

        let outcome =
            mark_kind_completed(dir.path(), 99, MigrationKind::Bookmarks).expect("stale update");

        assert_eq!(outcome, LedgerUpdateOutcome::EntryMissing);
    }

    #[test]
    fn failed_kind_tracks_attempts_and_retry_limit_skips_reconcile() {
        let dir = TempDir::new().expect("tempdir");
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");
        let generation = record_pending(
            dir.path(),
            &old_path,
            &new_path,
            &[MigrationKind::Bookmarks],
        )
        .expect("record pending");

        for _ in 0..MAX_MIGRATION_ATTEMPTS {
            mark_kind_failed(
                dir.path(),
                generation,
                MigrationKind::Bookmarks,
                &anyhow::anyhow!("simulated failure"),
            )
            .expect("mark failed");
        }
        let report = reconcile_pending(dir.path()).expect("reconcile");

        assert_eq!(report.considered, 1);
        assert_eq!(report.attempted, 0);
        assert_eq!(report.skipped, 1);
        let ledger = load_recovering(dir.path()).value;
        assert_eq!(ledger.entries[0].kinds[0].attempts, MAX_MIGRATION_ATTEMPTS);
    }

    #[test]
    fn reconcile_pending_retries_bookmark_migration_and_clears_entry() {
        let dir = TempDir::new().expect("tempdir");
        let old_path = seed_file(dir.path(), "workspace/old.txt");
        bookmark_service::save_for_path(
            dir.path(),
            &old_path,
            &[BookmarkRecord::new(3, Some("important".to_string()))],
        )
        .expect("seed bookmark");
        let new_path = dir.path().join("workspace/new.txt");
        fixture::rename(&old_path, &new_path);
        record_pending(
            dir.path(),
            &old_path,
            &new_path,
            &[MigrationKind::Bookmarks],
        )
        .expect("record pending");

        let report = reconcile_pending(dir.path()).expect("reconcile pending");

        assert_eq!(report.attempted, 1);
        assert_eq!(report.completed, 1);
        assert!(report.diagnostics.is_empty());
        assert!(load_recovering(dir.path()).value.entries.is_empty());
        let migrated =
            bookmark_service::load_for_path(dir.path(), &new_path).expect("load moved bookmarks");
        assert_eq!(migrated.bookmarks.len(), 1);
        assert_eq!(migrated.bookmarks[0].line, 3);
    }
}

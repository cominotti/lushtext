// SPDX-License-Identifier: GPL-3.0-or-later

//! Durable retry ledger for post-rename sidecar and local-history migrations.
//!
//! Rename workflows update user files first, then migrate app-owned sidecars in
//! background workers. This service records that follow-up work before it runs
//! and retries incomplete kinds on later startup without a tight retry loop.

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::model::migration_ledger::{
    MigrationEntry, MigrationKind, MigrationKindState, MigrationLedgerDocument,
};
use crate::model::sidecar_identity::now_epoch_secs;
use crate::services::json_format::KIND_MIGRATION_LEDGER;
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass,
    load_enveloped_json_or_default, save_enveloped_json_path,
};
use crate::services::{
    bookmark_service, document_note_service, folder_note_service, local_history_service,
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

/// Serializes a **whole tracked rename** — its ledger entry and every kind — not
/// just ledger writes, and not merely one kind at a time.
///
/// `ledger_lock` protects the ledger's own load/mutate/save cycle, which is a
/// different and much shorter critical section than the migration itself.
///
/// **Per-kind granularity is not enough, and is in fact worse than none.**
/// Consider rename A→B on worker W1 and B→C on worker W2, each moving bookmarks,
/// document notes, and folder notes. With the lock taken once per kind, W2 can
/// slip between two of W1's kinds: W2's document-note scan under `B` finds
/// nothing, because W1 has not moved document notes yet, so
/// `rebase_identity_paths` returns `None`, the kind completes with `Ok(0)`, and
/// its ledger entry is retired. W1 then moves the document-note sidecar A→B. The
/// file now lives at `C`, its only sidecar sits at `B`, **both ledger entries are
/// retired**, and startup reconcile has nothing left to retry — the note is
/// permanently invisible from every UI surface. Holding the lock per kind makes
/// that window *more* reachable than not locking at all, because it guarantees a
/// release point between kinds.
///
/// The critical section therefore spans [`run_tracked_rename`]: `record_pending`
/// plus every [`TrackedRename::run_kind`] call, so a second rename of the same
/// tree observes either none of the first rename's moves or all of them.
///
/// This is deliberately **serialization, not supersession**: a superseding
/// coordinator would drop the first hop and strand the sidecar at A. Renames are
/// a human-rate operation, so serializing them costs nothing measurable.
///
/// Lock ordering is always **operation → ledger**, never the reverse: nothing
/// holding `ledger_lock` acquires this one.
fn migration_operation_lock() -> &'static Mutex<()> {
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
    load_enveloped_json_or_default(
        &RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::MigrationLedger),
        KIND_MIGRATION_LEDGER,
    )
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

/// A rename whose ledger entry and every kind run inside one critical section.
///
/// Held for the whole rename, so a concurrent rename of the same tree cannot
/// observe a half-migrated sidecar set. See `migration_operation_lock` for the
/// data-loss scenario per-kind locking leaves open.
pub struct TrackedRename {
    /// Live operation-lock guard. Dropping this ends the rename's exclusivity.
    _guard: MutexGuard<'static, ()>,
    /// Ledger generation `record_pending` allocated for this rename.
    generation: u64,
}

impl TrackedRename {
    /// Return the ledger generation this rename's kinds complete against.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Run one migration kind and record its result against this rename.
    ///
    /// Takes no lock: the caller already holds it for the whole rename, and this
    /// process's operation mutex is not reentrant.
    ///
    /// # Errors
    ///
    /// Returns the migration error, or a ledger update error if state
    /// persistence fails after the migration operation succeeds.
    pub fn run_kind<T>(
        &self,
        data_dir: &Path,
        kind: MigrationKind,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        match operation() {
            Ok(value) => {
                mark_kind_completed(data_dir, self.generation, kind)?;
                Ok(value)
            }
            Err(error) => {
                if let Err(ledger_error) = mark_kind_failed(data_dir, self.generation, kind, &error)
                {
                    tracing::warn!(
                        "Failed to update migration ledger after {} failure: {ledger_error}",
                        kind.label()
                    );
                }
                Err(error)
            }
        }
    }
}

/// Record a rename's pending kinds and run all of them under one lock.
///
/// This is the only supported way to run a tracked rename. `record_pending` and
/// every [`TrackedRename::run_kind`] call share one critical section, which is
/// what closes the interleaving window documented on
/// `migration_operation_lock`.
///
/// # Errors
///
/// Returns an error if the ledger entry cannot be recorded, or whatever `body`
/// returns.
pub fn run_tracked_rename<T>(
    data_dir: &Path,
    old_path: &Path,
    new_path: &Path,
    kinds: &[MigrationKind],
    body: impl FnOnce(&TrackedRename) -> Result<T>,
) -> Result<T> {
    // The guard moves straight into `TrackedRename`, which owns it for the whole
    // rename. It is deliberately not bound to a named local first: a local would
    // be a temporary with a significant `Drop` that Clippy asks to be dropped
    // early, and dropping it early is precisely the bug this function exists to
    // prevent.
    let rename = TrackedRename {
        _guard: migration_operation_lock()
            .lock()
            .map_err(|_| anyhow::anyhow!("migration-operation lock poisoned"))?,
        generation: record_pending(data_dir, old_path, new_path, kinds)?,
    };
    body(&rename)
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
            // Startup reconcile runs the same operations as a live rename and
            // must share their serialization, or a reconcile scan can interleave
            // with an in-flight rename exactly as two renames could.
            let migration_result = {
                let _operation_guard = migration_operation_lock()
                    .lock()
                    .map_err(|_| anyhow::anyhow!("migration-operation lock poisoned"))?;
                run_migration_kind(data_dir, &entry, state.kind)
            };
            match migration_result {
                Ok(()) => {
                    let outcome = mark_kind_completed(data_dir, entry.generation, state.kind)?;
                    if outcome == LedgerUpdateOutcome::Updated {
                        report.completed += 1;
                    }
                }
                Err(error) => {
                    mark_kind_failed(data_dir, entry.generation, state.kind, &error)?;
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
    let path = ledger_path(data_dir);
    let config = RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::MigrationLedger);
    let diagnostics = save_enveloped_json_path(&config, KIND_MIGRATION_LEDGER, ledger)
        .with_context(|| format!("failed to save {}", path.display()))?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
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
        MigrationKind::FolderNotes => {
            folder_note_service::move_folder_tree(data_dir, &entry.old_path, &entry.new_path)?;
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

    fn seed_file(folder: &Path, relative: &str) -> std::path::PathBuf {
        let path = folder.join(relative);
        if let Some(parent) = path.parent() {
            fixture::create_dir_all(parent);
        }
        fixture::write_text(&path, "contents\n");
        path
    }

    #[test]
    fn ledger_lock_is_process_wide_singleton() {
        assert!(
            std::ptr::eq(ledger_lock(), ledger_lock()),
            "ledger updates must share one mutex so load/mutate/save cycles stay serialized"
        );
    }

    #[test]
    fn migration_operation_lock_is_a_process_wide_singleton_distinct_from_the_ledger_lock() {
        // Regression guard for a confirmed pre-existing defect: `operation()` ran
        // outside every lock, so two overlapping renames could interleave and
        // strand a sidecar while both ledger entries were retired. The migration
        // operation needs its own, coarser critical section — and it must not be
        // the ledger lock, which is re-entered by `mark_kind_completed` *inside*
        // the operation's own scope and would deadlock.
        assert!(
            std::ptr::eq(migration_operation_lock(), migration_operation_lock()),
            "tracked migrations must share one mutex so overlapping renames serialize"
        );
        assert!(
            !std::ptr::eq(migration_operation_lock(), ledger_lock()),
            "the operation lock must be distinct from the ledger lock, which the \
             operation's own completion re-enters"
        );
    }

    #[test]
    fn run_tracked_rename_admits_no_second_rename_between_its_kinds() {
        // The B1 regression: per-kind locking guarantees a release point
        // *between* kinds, which is exactly where a second rename of the same
        // tree strands a sidecar. This test drives the two-rename interleaving
        // and asserts the property that makes it impossible: while one rename is
        // between two of its kinds, no other rename may start.
        //
        // It fails against a per-kind lock. With the lock taken inside each kind,
        // rename 2 acquires it during rename 1's inter-kind gap, so
        // `peer_started_mid_rename` is observed `true`.
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::time::Duration;

        let dir = TempDir::new().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let all_kinds = [
            MigrationKind::Bookmarks,
            MigrationKind::DocumentNotes,
            MigrationKind::FolderNotes,
        ];

        // `renames_active` counts renames inside their critical section.
        // `peer_started_mid_rename` latches if a second rename ever begins while
        // a first one is partway through its kinds.
        let renames_active = Arc::new(AtomicUsize::new(0));
        let peer_started_mid_rename = Arc::new(AtomicBool::new(false));
        let first_rename_between_kinds = Arc::new(AtomicBool::new(false));

        let mut handles = Vec::new();
        for (index, (old_name, new_name)) in [("a", "b"), ("b", "c")].into_iter().enumerate() {
            let data_dir = data_dir.clone();
            let renames_active = Arc::clone(&renames_active);
            let peer_started_mid_rename = Arc::clone(&peer_started_mid_rename);
            let first_rename_between_kinds = Arc::clone(&first_rename_between_kinds);
            handles.push(std::thread::spawn(move || {
                // Stagger so rename 1 reaches its inter-kind gap first.
                if index == 1 {
                    std::thread::sleep(Duration::from_millis(40));
                }
                run_tracked_rename(
                    &data_dir,
                    &data_dir.join(old_name),
                    &data_dir.join(new_name),
                    &all_kinds,
                    |rename| {
                        let concurrent = renames_active.fetch_add(1, Ordering::SeqCst) + 1;
                        if concurrent > 1 || first_rename_between_kinds.load(Ordering::SeqCst) {
                            peer_started_mid_rename.store(true, Ordering::SeqCst);
                        }
                        for (kind_index, kind) in all_kinds.into_iter().enumerate() {
                            rename.run_kind(&data_dir, kind, || Ok(()))?;
                            if index == 0 && kind_index == 0 {
                                // The inter-kind gap a per-kind lock would open.
                                first_rename_between_kinds.store(true, Ordering::SeqCst);
                                std::thread::sleep(Duration::from_millis(120));
                                first_rename_between_kinds.store(false, Ordering::SeqCst);
                            }
                        }
                        renames_active.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    },
                )
            }));
        }
        for handle in handles {
            handle.join().expect("thread").expect("tracked rename");
        }

        assert!(
            !peer_started_mid_rename.load(Ordering::SeqCst),
            "a second rename started while the first was between two of its kinds; \
             the sidecar-stranding window is open"
        );
    }

    #[test]
    fn run_tracked_rename_records_pending_inside_its_own_critical_section() {
        // `record_pending` must be inside the lock, not before it: a rename that
        // records its entry unlocked and then locks per kind leaves the same gap
        // at the start. Observable property: the entry exists by the time the
        // body runs, and the generation the body sees is the one recorded.
        let dir = TempDir::new().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let observed = run_tracked_rename(
            &data_dir,
            &data_dir.join("old"),
            &data_dir.join("new"),
            &[MigrationKind::Bookmarks],
            |rename| {
                let ledger = load_for_update(&data_dir).expect("ledger inside rename");
                let entry = ledger
                    .entries
                    .iter()
                    .find(|entry| entry.generation == rename.generation())
                    .expect("pending entry visible to the rename body");
                assert_eq!(entry.kinds.len(), 1);
                assert!(!entry.kinds[0].completed);
                Ok(rename.generation())
            },
        )
        .expect("tracked rename");
        assert!(observed > 0, "a generation must have been allocated");
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

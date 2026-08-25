// SPDX-License-Identifier: GPL-3.0-or-later

//! Persisted undo backup for Replace All.
//!
//! The backup maps file paths to original and post-replace bytes so a Replace
//! All can be reverted without overwriting files edited after the replacement.

use crate::model::sidecar_identity::stable_path_hash;
use crate::services::content_search::{
    MAX_REPLACE_UNDO_BYTES, MAX_REPLACE_UNDO_RETAINED_BYTES, ReplaceUndoBackup, ReplaceUndoEntry,
    replace_undo_retained_byte_weight,
};
use crate::services::{
    filesystem::{
        DirectoryScanPolicy, metadata as fs_metadata, mutate as fs_mutate, tree as fs_tree,
        write as fs_write,
    },
    json_format::{
        KIND_REPLACE_UNDO_CLEANUP_MARKER, KIND_REPLACE_UNDO_ENTRY, KIND_REPLACE_UNDO_MANIFEST,
        KIND_RETIRED_REPLACE_UNDO_BACKUP,
    },
    recovery_metadata::{
        RecoveryDiagnostic, RecoveryLoadConfig, RecoveryMetadataClass, RecoveryPreservation,
        RecoveryProblem, load_enveloped_json_optional, save_enveloped_json_path,
    },
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Pre-public single-file backup path, retained only for clean-break recovery diagnostics.
const BACKUP_FILE: &str = "replace-backup.json";
/// Directory containing one pre-write undo snapshot per replaced file.
const JOURNAL_DIR: &str = "replace-backup-journal";
/// Commit marker proving a journal run has active, undoable entries.
const JOURNAL_MANIFEST_FILE: &str = "manifest.json";
/// Inactive marker written before cleanup so interrupted deletion cannot revive undo.
const CLEANUP_MARKER_FILE: &str = "cleanup-in-progress.json";
/// Maximum journal entries scanned for orphan diagnostics in one recovery pass.
///
/// Ten thousand bounds damaged app-data startup work while staying far above a
/// realistic Replace All batch in an editor session.
const JOURNAL_SCAN_MAX_ENTRIES: usize = 10_000;
#[cfg(test)]
thread_local! {
    static TEST_MAX_REPLACE_UNDO_BYTES: Cell<Option<u64>> = const { Cell::new(None) };
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ReplaceBackupDiskEntry {
    path: PathBuf,
    original_content: String,
    replaced_content: String,
}

/// Commit record listing the per-file entries that make one complete undo run.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ReplaceJournalManifest {
    /// Incremental manifests make every durable entry in the journal active.
    ///
    /// This lets Replace All arm one bounded per-file snapshot before each
    /// target rename without rewriting an ever-growing manifest.
    #[serde(default)]
    incremental: bool,
    entries: Vec<ReplaceJournalManifestEntry>,
}

/// Manifest entry tying a target path to its hashed per-file journal file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplaceJournalManifestEntry {
    path: PathBuf,
    entry_file: String,
}

/// Marker that makes a journal inactive before destructive cleanup begins.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ReplaceJournalCleanupMarker {
    reason: String,
}

/// Recovery-aware Replace All undo-journal load result.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct ReplaceBackupRecoveryLoad {
    /// Undo entries that are complete enough to use only when `active` is true.
    pub backup: ReplaceUndoBackup,
    /// Whether the loaded state may be exposed as an undo affordance.
    pub active: bool,
    /// Recovery diagnostics for malformed, partial, stale, or cleanup states.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

impl ReplaceBackupRecoveryLoad {
    /// Return the backup only when it represents a complete active journal.
    #[must_use]
    pub fn active_backup(self) -> ReplaceUndoBackup {
        if self.active {
            self.backup
        } else {
            ReplaceUndoBackup::new()
        }
    }
}

/// Diagnostics returned when stale Replace All undo state cannot be cleaned.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct ReplaceBackupCleanupReport {
    /// Cleanup problems that should be logged or surfaced by higher layers.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

/// Acquire the process-wide coordinator for Replace All journal inspection and mutation.
///
/// Startup recovery, async panel persistence, cleanup, and the replacement
/// writer share this guard so a snapshot-based inactive decision cannot delete
/// a newly activated journal.
///
/// # Errors
///
/// Returns an error if a previous holder panicked while owning the coordinator.
pub fn acquire_journal_guard() -> Result<MutexGuard<'static, ()>> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| anyhow::anyhow!("replace undo backup disk lock poisoned"))
}

/// Load the persisted Replace All undo backup from disk.
///
/// # Errors
///
/// Returns an error if a complete active backup cannot be converted back into
/// UTF-8 file content. Malformed or inactive persisted state is treated as
/// recovery metadata and returns an empty backup.
pub fn load(data_dir: &Path) -> Result<ReplaceUndoBackup> {
    Ok(load_recovering(data_dir).active_backup())
}

/// Load Replace All undo state with recovery diagnostics.
///
/// Callers must only expose the returned backup when `active` is true. Stale,
/// malformed, or incomplete journals are kept diagnostic and inactive so a
/// crash during cleanup cannot resurrect an unsafe undo action.
#[must_use]
pub fn load_recovering(data_dir: &Path) -> ReplaceBackupRecoveryLoad {
    let journal_dir = data_dir.join(JOURNAL_DIR);
    match fs_metadata::path_status(&journal_dir) {
        Ok(status) if status.is_directory() => return load_journal(data_dir, &journal_dir),
        Ok(status) if status.is_present() => {
            return ReplaceBackupRecoveryLoad {
                backup: ReplaceUndoBackup::new(),
                active: false,
                diagnostics: vec![RecoveryDiagnostic::with_preservation(
                    RecoveryMetadataClass::ReplaceAllUndoJournal,
                    &journal_dir,
                    RecoveryProblem::UnsupportedFileKind { status },
                    RecoveryPreservation::PreservedInPlace,
                )],
            };
        }
        Ok(_) => {}
        Err(error) => {
            return ReplaceBackupRecoveryLoad {
                backup: ReplaceUndoBackup::new(),
                active: false,
                diagnostics: vec![RecoveryDiagnostic::with_preservation(
                    RecoveryMetadataClass::ReplaceAllUndoJournal,
                    &journal_dir,
                    RecoveryProblem::Unreadable {
                        detail: error.to_string(),
                    },
                    RecoveryPreservation::PreservedInPlace,
                )],
            };
        }
    }

    load_retired_backup(data_dir)
}

/// Preserve the retired single-file backup without exposing it as undo state.
fn load_retired_backup(data_dir: &Path) -> ReplaceBackupRecoveryLoad {
    let path = data_dir.join(BACKUP_FILE);
    let load = load_enveloped_json_optional::<serde_json::Value>(
        &RecoveryLoadConfig::new(
            data_dir,
            &path,
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        ),
        KIND_RETIRED_REPLACE_UNDO_BACKUP,
    );
    let mut diagnostics = load.diagnostics;
    if load.value.is_some() {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            &path,
            "retired single-file replace undo backup is not a supported runtime format",
        ));
    }
    ReplaceBackupRecoveryLoad {
        backup: ReplaceUndoBackup::new(),
        active: false,
        diagnostics,
    }
}

/// Save a complete Replace All undo journal.
///
/// Each file entry is written before its target file changes; the manifest is
/// the commit marker that makes the staged set active for undo.
///
/// # Errors
///
/// Returns an error if any backup entry is not valid UTF-8 or the backup file
/// cannot be serialized or written.
pub fn save(data_dir: &Path, backup: &ReplaceUndoBackup) -> Result<()> {
    delete(data_dir)?;
    if backup.is_empty() {
        return Ok(());
    }
    for (path, entry) in backup {
        save_entry(data_dir, path, entry)?;
    }
    mark_journal_active(data_dir, backup)?;
    Ok(())
}

/// Re-arm a journal that a partial undo reduced, without any inactive window.
///
/// [`save`] is a delete-then-rebuild: it removes the whole journal directory
/// before writing the new one. For a partial undo that is a data-loss window —
/// the files it could **not** restore still hold Replace All output, and while
/// the rebuild is in flight there is no durable rollback copy for them at all,
/// which is the in-memory-only failure the journal exists to prevent.
///
/// A partial undo only ever *shrinks* the journal, and every retained entry file
/// is already on disk and byte-identical, so this does the shrink in the safe
/// order instead: write the smaller active manifest first, then remove the entry
/// files it no longer lists. Neither window is ever inactive.
///
/// - Interrupted before the new manifest lands: the previous manifest stays
///   active and lists a superset. Undo re-validates each file's current bytes,
///   so an already-restored entry is recognised as restored and never rewritten.
/// - Interrupted after it lands: the smaller manifest is active and the
///   already-restored entry files are orphans, which recovery reports as
///   diagnostics without deactivating the journal.
///
/// Falls back to [`save`] when the on-disk journal is not a superset of
/// `backup`, which is the one case a shrink cannot express.
///
/// # Errors
///
/// Returns an error when the smaller active manifest cannot be written durably,
/// or when the fallback full rewrite fails.
pub fn shrink_journal_to(data_dir: &Path, backup: &ReplaceUndoBackup) -> Result<()> {
    if backup.is_empty() {
        return delete(data_dir);
    }

    let journal_dir = data_dir.join(JOURNAL_DIR);
    let retained = backup
        .keys()
        .map(|path| entry_file_name(path))
        .collect::<HashSet<_>>();
    let is_superset = fs_metadata::path_status(&journal_dir)
        .is_ok_and(super::filesystem::types::PathStatus::is_directory)
        && !fs_metadata::exists(&journal_dir.join(CLEANUP_MARKER_FILE))
        && retained
            .iter()
            .all(|entry_file| fs_metadata::exists(&journal_dir.join(entry_file)));
    if !is_superset {
        return save(data_dir, backup);
    }

    mark_journal_active(data_dir, backup)?;
    remove_unlisted_journal_entries(&journal_dir, &retained);
    Ok(())
}

/// Delete payload entries the freshly written active manifest no longer lists.
///
/// Best-effort by design: the manifest is already durable and correct, and a
/// leftover entry is only an orphan diagnostic, so a removal failure must not be
/// reported as a failed journal write.
fn remove_unlisted_journal_entries(journal_dir: &Path, retained: &HashSet<String>) {
    let entries = match fs_tree::scan_directory(
        journal_dir,
        DirectoryScanPolicy {
            max_entries: JOURNAL_SCAN_MAX_ENTRIES,
            include_hidden: false,
        },
    ) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!("Failed to scan replace undo journal for shrink cleanup: {error}");
            return;
        }
    };
    for entry in entries {
        if !is_journal_payload_file(&entry.path) {
            continue;
        }
        let Some(file_name) = entry.path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if retained.contains(file_name) {
            continue;
        }
        if let Err(error) = fs_mutate::remove_file_if_exists(&entry.path) {
            tracing::warn!(
                "Failed to remove superseded replace undo journal entry {}: {error}",
                entry.path.display()
            );
        }
    }
}

/// Remove any previous undo run before staging an incremental Replace All journal.
///
/// # Errors
///
/// Returns an error when prior journal state cannot be made inactive and removed.
pub fn begin_incremental_journal(data_dir: &Path) -> Result<()> {
    delete(data_dir)
}

/// Save one per-file undo journal entry before that file is modified.
///
/// # Errors
///
/// Returns an error if the entry is not valid UTF-8 or cannot be durably
/// written into the journal directory.
pub fn save_entry(data_dir: &Path, path: &Path, entry: &ReplaceUndoEntry) -> Result<()> {
    let disk = disk_entry_from_memory(path, entry)?;
    let journal_dir = data_dir.join(JOURNAL_DIR);
    let entry_path = journal_dir.join(entry_file_name(path));
    let config = RecoveryLoadConfig::new(
        data_dir,
        &entry_path,
        RecoveryMetadataClass::ReplaceAllUndoJournal,
    );
    let diagnostics = save_enveloped_json_path(&config, KIND_REPLACE_UNDO_ENTRY, &disk)?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
}

/// Arm durable per-file entries as an incrementally recoverable undo run.
///
/// Once this marker is durable, every valid entry in the journal directory is
/// recovery evidence. Replace All writes each entry before its corresponding
/// target rename, so a crash can expose an entry for an unchanged target; undo
/// validation safely ignores that entry unless the target matches its replaced
/// bytes.
///
/// # Errors
///
/// Returns an error when the active marker cannot be written durably.
pub fn mark_incremental_journal_active(data_dir: &Path) -> Result<()> {
    let journal_dir = data_dir.join(JOURNAL_DIR);
    fs_write::create_dir_all_durable(&journal_dir)
        .with_context(|| format!("failed to create {}", journal_dir.display()))?;
    fs_mutate::remove_file_if_exists(&journal_dir.join(CLEANUP_MARKER_FILE)).with_context(
        || {
            format!(
                "failed to clear replace journal cleanup marker in {}",
                journal_dir.display()
            )
        },
    )?;
    let manifest = ReplaceJournalManifest {
        incremental: true,
        entries: Vec::new(),
    };
    save_journal_manifest(data_dir, &journal_dir, &manifest)
}

/// Delete one per-file journal entry. Missing entries are treated as already gone.
///
/// # Errors
///
/// Returns an error if an existing entry cannot be removed.
pub fn delete_entry(data_dir: &Path, path: &Path) -> Result<()> {
    let entry_path = data_dir.join(JOURNAL_DIR).join(entry_file_name(path));
    match fs_mutate::remove_file_if_exists(&entry_path) {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete replace journal entry {}: {}",
            entry_path.display(),
            e
        )),
    }
}

/// Mark already-written per-file entries as the complete active undo journal.
///
/// Replace All writes entries before touching each file. This manifest is the
/// separate commit point: until it exists and validates, startup treats the
/// directory as preserved evidence rather than a user-visible undo source.
///
/// # Errors
///
/// Returns an error when the journal directory cannot be created, an old cleanup
/// marker cannot be removed, the manifest exceeds the supported entry cap, or
/// the active manifest cannot be written durably.
pub fn mark_journal_active(data_dir: &Path, backup: &ReplaceUndoBackup) -> Result<()> {
    if backup.is_empty() {
        return delete(data_dir);
    }
    if entry_count_exceeds_cap(backup.len()) {
        anyhow::bail!(
            "replace undo journal has {} entries, above the {JOURNAL_SCAN_MAX_ENTRIES} entry cap",
            backup.len()
        );
    }

    let journal_dir = data_dir.join(JOURNAL_DIR);
    fs_write::create_dir_all_durable(&journal_dir)
        .with_context(|| format!("failed to create {}", journal_dir.display()))?;
    fs_mutate::remove_file_if_exists(&journal_dir.join(CLEANUP_MARKER_FILE)).with_context(
        || {
            format!(
                "failed to clear replace journal cleanup marker in {}",
                journal_dir.display()
            )
        },
    )?;

    let entries = backup
        .keys()
        .map(|path| ReplaceJournalManifestEntry {
            path: path.clone(),
            entry_file: entry_file_name(path),
        })
        .collect();
    let manifest = ReplaceJournalManifest {
        incremental: false,
        entries,
    };
    save_journal_manifest(data_dir, &journal_dir, &manifest)
}

fn save_journal_manifest(
    data_dir: &Path,
    journal_dir: &Path,
    manifest: &ReplaceJournalManifest,
) -> Result<()> {
    let manifest_path = journal_dir.join(JOURNAL_MANIFEST_FILE);
    let config = RecoveryLoadConfig::new(
        data_dir,
        &manifest_path,
        RecoveryMetadataClass::ReplaceAllUndoJournal,
    );
    let diagnostics = save_enveloped_json_path(&config, KIND_REPLACE_UNDO_MANIFEST, manifest)?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
}

/// Clean stale Replace All undo state and return diagnostics instead of
/// spinning retries in the caller.
#[must_use]
pub fn cleanup_stale(data_dir: &Path) -> ReplaceBackupCleanupReport {
    match delete(data_dir) {
        Ok(()) => ReplaceBackupCleanupReport::default(),
        Err(error) => ReplaceBackupCleanupReport {
            diagnostics: vec![RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                data_dir.join(JOURNAL_DIR),
                format!("replace undo journal cleanup failed: {error}"),
            )],
        },
    }
}

/// Load a journal directory only when its active manifest and entries agree.
fn load_journal(data_dir: &Path, journal_dir: &Path) -> ReplaceBackupRecoveryLoad {
    let cleanup_marker = journal_dir.join(CLEANUP_MARKER_FILE);
    if fs_metadata::exists(&cleanup_marker) {
        return ReplaceBackupRecoveryLoad {
            backup: ReplaceUndoBackup::new(),
            active: false,
            diagnostics: vec![RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                &cleanup_marker,
                "replace undo journal cleanup was interrupted; journal remains inactive",
            )],
        };
    }

    let manifest_path = journal_dir.join(JOURNAL_MANIFEST_FILE);
    let manifest_load = load_enveloped_json_optional::<ReplaceJournalManifest>(
        &RecoveryLoadConfig::new(
            data_dir,
            &manifest_path,
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        ),
        KIND_REPLACE_UNDO_MANIFEST,
    );
    let Some(manifest) = manifest_load.value else {
        let mut diagnostics = manifest_load.diagnostics;
        if diagnostics.is_empty() {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                &manifest_path,
                "replace undo journal has entries but no active manifest",
            ));
        }
        return ReplaceBackupRecoveryLoad {
            backup: ReplaceUndoBackup::new(),
            active: false,
            diagnostics,
        };
    };

    let mut diagnostics = manifest_load.diagnostics;
    if manifest.incremental {
        return load_incremental_journal(data_dir, journal_dir, diagnostics);
    }
    if entry_count_exceeds_cap(manifest.entries.len()) {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            &manifest_path,
            format!(
                "replace undo journal manifest lists {} entries, above the {JOURNAL_SCAN_MAX_ENTRIES} entry cap",
                manifest.entries.len()
            ),
        ));
        return ReplaceBackupRecoveryLoad {
            backup: ReplaceUndoBackup::new(),
            active: false,
            diagnostics,
        };
    }

    let mut backup = ReplaceUndoBackup::new();
    let mut dedup = ManifestEntryDedup::default();
    let mut undo_payload_bytes = 0u64;

    for entry in &manifest.entries {
        match dedup.admit(&entry.entry_file, &entry.path) {
            Ok(()) => {}
            Err(ManifestDuplicate::EntryFile) => {
                diagnostics.push(RecoveryDiagnostic::repair_skipped(
                    RecoveryMetadataClass::ReplaceAllUndoJournal,
                    journal_dir.join(&entry.entry_file),
                    "replace undo journal manifest contains a duplicate entry file",
                ));
                continue;
            }
            Err(ManifestDuplicate::TargetPath) => {
                diagnostics.push(RecoveryDiagnostic::repair_skipped(
                    RecoveryMetadataClass::ReplaceAllUndoJournal,
                    &entry.path,
                    "replace undo journal manifest contains a duplicate target path",
                ));
                continue;
            }
        }

        let entry_path = journal_dir.join(&entry.entry_file);
        let entry_load = load_enveloped_json_optional::<ReplaceBackupDiskEntry>(
            &RecoveryLoadConfig::new(
                data_dir,
                &entry_path,
                RecoveryMetadataClass::ReplaceAllUndoJournal,
            ),
            KIND_REPLACE_UNDO_ENTRY,
        );
        let Some(disk) = entry_load.value else {
            if entry_load.diagnostics.is_empty() {
                diagnostics.push(RecoveryDiagnostic::repair_skipped(
                    RecoveryMetadataClass::ReplaceAllUndoJournal,
                    &entry_path,
                    "replace undo journal manifest references a missing entry",
                ));
            } else {
                diagnostics.extend(entry_load.diagnostics);
            }
            continue;
        };
        diagnostics.extend(entry_load.diagnostics);
        if disk.path != entry.path {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                &entry_path,
                "replace undo journal entry path does not match the active manifest",
            ));
            continue;
        }
        let entry_payload_bytes = replace_entry_payload_bytes(&disk);
        if payload_budget_exceeded(
            undo_payload_bytes,
            entry_payload_bytes,
            effective_max_replace_undo_bytes(),
        ) {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                &entry_path,
                "replace undo journal exceeds the Replace All undo payload limit",
            ));
            break;
        }
        undo_payload_bytes = undo_payload_bytes.saturating_add(entry_payload_bytes);
        backup.insert(
            disk.path,
            ReplaceUndoEntry::new(
                disk.original_content.into_bytes(),
                disk.replaced_content.into_bytes(),
            ),
        );
    }

    if manifest.entries.is_empty() {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            &manifest_path,
            "replace undo journal manifest is empty; empty undo state should be deleted",
        ));
    }

    let active = journal_is_active(
        diagnostics.len(),
        Some((backup.len(), manifest.entries.len())),
    );
    if active {
        diagnostics.extend(detect_orphan_journal_entries(
            data_dir,
            journal_dir,
            dedup.admitted_entry_files(),
        ));
        ReplaceBackupRecoveryLoad {
            backup,
            active: true,
            diagnostics,
        }
    } else {
        diagnostics.extend(detect_orphan_journal_entries(
            data_dir,
            journal_dir,
            dedup.admitted_entry_files(),
        ));
        ReplaceBackupRecoveryLoad {
            backup: ReplaceUndoBackup::new(),
            active: false,
            diagnostics,
        }
    }
}

/// Load an in-progress or completed incremental journal by scanning its bounded
/// set of durable per-file entries.
fn load_incremental_journal(
    data_dir: &Path,
    journal_dir: &Path,
    mut diagnostics: Vec<RecoveryDiagnostic>,
) -> ReplaceBackupRecoveryLoad {
    let scan_cap = JOURNAL_SCAN_MAX_ENTRIES.saturating_add(3);
    let entries = match fs_tree::scan_directory(
        journal_dir,
        DirectoryScanPolicy {
            max_entries: scan_cap,
            include_hidden: false,
        },
    ) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                journal_dir,
                format!("failed to scan incremental replace undo journal: {error}"),
            ));
            return ReplaceBackupRecoveryLoad {
                backup: ReplaceUndoBackup::new(),
                active: false,
                diagnostics,
            };
        }
    };

    if entries.len() == scan_cap {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            journal_dir,
            "incremental replace undo journal scan reached its entry cap",
        ));
    }

    let mut entry_paths = entries
        .into_iter()
        .filter_map(|entry| is_journal_payload_file(&entry.path).then_some(entry.path))
        .collect::<Vec<_>>();
    entry_paths.sort();
    if entry_count_exceeds_cap(entry_paths.len()) {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            journal_dir,
            format!(
                "incremental replace undo journal has {} entries, above the {JOURNAL_SCAN_MAX_ENTRIES} entry cap",
                entry_paths.len()
            ),
        ));
    }

    let mut backup = ReplaceUndoBackup::new();
    let mut undo_payload_bytes = 0u64;
    for entry_path in entry_paths.into_iter().take(JOURNAL_SCAN_MAX_ENTRIES) {
        let entry_load = load_enveloped_json_optional::<ReplaceBackupDiskEntry>(
            &RecoveryLoadConfig::new(
                data_dir,
                &entry_path,
                RecoveryMetadataClass::ReplaceAllUndoJournal,
            ),
            KIND_REPLACE_UNDO_ENTRY,
        );
        let Some(disk) = entry_load.value else {
            if entry_load.diagnostics.is_empty() {
                diagnostics.push(RecoveryDiagnostic::repair_skipped(
                    RecoveryMetadataClass::ReplaceAllUndoJournal,
                    &entry_path,
                    "incremental replace undo journal entry could not be loaded",
                ));
            } else {
                diagnostics.extend(entry_load.diagnostics);
            }
            continue;
        };
        diagnostics.extend(entry_load.diagnostics);
        let entry_payload_bytes = replace_entry_payload_bytes(&disk);
        if payload_budget_exceeded(
            undo_payload_bytes,
            entry_payload_bytes,
            effective_max_replace_undo_bytes(),
        ) {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                &entry_path,
                "incremental replace undo journal exceeds the Replace All undo payload limit",
            ));
            break;
        }
        undo_payload_bytes = undo_payload_bytes.saturating_add(entry_payload_bytes);
        let target_path = disk.path.clone();
        if backup
            .insert(
                target_path.clone(),
                ReplaceUndoEntry::new(
                    disk.original_content.into_bytes(),
                    disk.replaced_content.into_bytes(),
                ),
            )
            .is_some()
        {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                target_path,
                "incremental replace undo journal contains duplicate target paths",
            ));
        }
        if retained_weight_exceeds_cap(replace_undo_retained_byte_weight(&backup)) {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                &entry_path,
                "incremental replace undo journal exceeds the complete retained-memory limit",
            ));
            break;
        }
    }

    if backup.is_empty() {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            journal_dir,
            "incremental replace undo journal contains no usable entries",
        ));
    }
    let active = journal_is_active(diagnostics.len(), None);
    ReplaceBackupRecoveryLoad {
        backup: if active {
            backup
        } else {
            ReplaceUndoBackup::new()
        },
        active,
        diagnostics,
    }
}

/// Whether a loaded journal may be exposed as a user-visible undo affordance.
///
/// Two loader shapes share this rule. A manifest-backed journal must have no
/// diagnostics **and** must have loaded exactly as many entries as its manifest
/// lists; an incremental journal has no manifest list to agree with, so only the
/// diagnostic clause applies. Any diagnostic at all is disqualifying: a journal
/// that needed repair is preserved as evidence rather than offered as undo.
fn journal_is_active(diagnostic_count: usize, manifest_agreement: Option<(usize, usize)>) -> bool {
    diagnostic_count == 0 && manifest_agreement.is_none_or(|(loaded, listed)| loaded == listed)
}

/// Whether admitting one more entry would exceed the undo payload budget.
///
/// Saturating, so an implausibly large accumulated total rejects rather than
/// wrapping into an accepting value.
fn payload_budget_exceeded(accumulated: u64, entry: u64, cap: u64) -> bool {
    accumulated.saturating_add(entry) > cap
}

/// Whether an entry count is above the supported journal entry cap.
///
/// Applied on the write side before a manifest is committed and on both read
/// sides before entries are walked, so one cap decision governs all three.
fn entry_count_exceeds_cap(count: usize) -> bool {
    count > JOURNAL_SCAN_MAX_ENTRIES
}

/// Whether a journal-directory file is a per-file undo payload entry.
///
/// The manifest and the cleanup marker live in the same directory and are not
/// payloads, and anything that is not a `.json` file is not ours.
fn is_journal_payload_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if file_name == JOURNAL_MANIFEST_FILE || file_name == CLEANUP_MARKER_FILE {
        return false;
    }
    path.extension().and_then(|ext| ext.to_str()) == Some("json")
}

/// Whether the complete retained journal is above the in-memory retention cap.
fn retained_weight_exceeds_cap(weight: u64) -> bool {
    weight > MAX_REPLACE_UNDO_RETAINED_BYTES
}

/// Whether recovery diagnostics permit destroying the state they describe.
///
/// Cleanup replaces user-recoverable state, so it may only proceed when every
/// diagnostic explicitly allows replacement.
fn cleanup_replacement_allowed(diagnostics: &[RecoveryDiagnostic]) -> bool {
    diagnostics
        .iter()
        .all(|diagnostic| diagnostic.replacement_allowed)
}

/// Which uniqueness rule rejected a manifest entry row.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ManifestDuplicate {
    /// Two manifest rows name the same per-file journal entry file.
    EntryFile,
    /// Two manifest rows name the same replacement target path.
    TargetPath,
}

/// One-pass uniqueness rejection for an active manifest's entry rows.
///
/// Entry-file uniqueness is checked first and is recorded even when the target
/// path then rejects the row, because that entry file is still accounted for by
/// the manifest and must not later be reported as an orphan.
#[derive(Debug, Default)]
struct ManifestEntryDedup {
    admitted_entry_files: HashSet<String>,
    admitted_paths: HashSet<PathBuf>,
}

impl ManifestEntryDedup {
    fn admit(&mut self, entry_file: &str, path: &Path) -> Result<(), ManifestDuplicate> {
        if !self.admitted_entry_files.insert(entry_file.to_string()) {
            return Err(ManifestDuplicate::EntryFile);
        }
        if !self.admitted_paths.insert(path.to_path_buf()) {
            return Err(ManifestDuplicate::TargetPath);
        }
        Ok(())
    }

    fn admitted_entry_files(&self) -> &HashSet<String> {
        &self.admitted_entry_files
    }
}

fn disk_entry_from_memory(path: &Path, entry: &ReplaceUndoEntry) -> Result<ReplaceBackupDiskEntry> {
    let original_content = String::from_utf8(entry.original_bytes.clone()).with_context(|| {
        format!(
            "replace backup original content for {} is not valid UTF-8",
            path.display()
        )
    })?;
    let replaced_content = String::from_utf8(entry.replaced_bytes.clone()).with_context(|| {
        format!(
            "replace backup replacement content for {} is not valid UTF-8",
            path.display()
        )
    })?;
    Ok(ReplaceBackupDiskEntry {
        path: path.to_path_buf(),
        original_content,
        replaced_content,
    })
}

fn replace_entry_payload_bytes(entry: &ReplaceBackupDiskEntry) -> u64 {
    u64::try_from(entry.original_content.len())
        .unwrap_or(u64::MAX)
        .saturating_add(u64::try_from(entry.replaced_content.len()).unwrap_or(u64::MAX))
}

/// Return the configured undo payload cap, allowing small test-only overrides.
fn effective_max_replace_undo_bytes() -> u64 {
    #[cfg(test)]
    {
        if let Some(override_value) = TEST_MAX_REPLACE_UNDO_BYTES.with(Cell::get) {
            return override_value;
        }
    }
    MAX_REPLACE_UNDO_BYTES
}

/// Delete the persisted Replace All undo backup, if it exists.
///
/// # Errors
///
/// Returns an error if an existing backup file cannot be deleted.
pub fn delete(data_dir: &Path) -> Result<()> {
    mark_cleanup_in_progress(data_dir)?;

    let legacy = data_dir.join(BACKUP_FILE);
    match fs_mutate::remove_file_if_exists(&legacy) {
        Ok(_) => {}
        Err(e) => {
            return Err(anyhow::anyhow!(
                "failed to delete replace backup {}: {}",
                legacy.display(),
                e
            ));
        }
    }

    let journal_dir = data_dir.join(JOURNAL_DIR);
    match fs_mutate::remove_dir_all_if_exists(&journal_dir) {
        Ok(_) => {
            if fs_metadata::exists(data_dir)
                && let Err(error) = fs_write::sync_parent_dir(&journal_dir)
            {
                tracing::warn!("Failed to sync replace journal cleanup: {error}");
            }
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete replace journal {}: {}",
            journal_dir.display(),
            e
        )),
    }
}

/// Write the inactive cleanup marker before removing legacy or journal state.
fn mark_cleanup_in_progress(data_dir: &Path) -> Result<()> {
    let legacy = data_dir.join(BACKUP_FILE);
    let journal_dir = data_dir.join(JOURNAL_DIR);
    let has_legacy = fs_metadata::path_status(&legacy)?.is_present();
    let has_journal = fs_metadata::path_status(&journal_dir)?.is_present();
    if !has_legacy && !has_journal {
        return Ok(());
    }

    if has_legacy {
        let recovery = load_retired_backup(data_dir);
        let replacement_safe = cleanup_replacement_allowed(&recovery.diagnostics);
        for diagnostic in recovery.diagnostics {
            tracing::warn!("{}", diagnostic.summary());
        }
        if !replacement_safe {
            anyhow::bail!(
                "retired replace backup is not safe to delete after recovery diagnostics"
            );
        }
    }

    fs_write::create_dir_all_durable(&journal_dir)
        .with_context(|| format!("failed to create {}", journal_dir.display()))?;
    let marker = ReplaceJournalCleanupMarker {
        reason: "stale replace undo journal cleanup started".to_string(),
    };
    let marker_path = journal_dir.join(CLEANUP_MARKER_FILE);
    let config = RecoveryLoadConfig::new(
        data_dir,
        &marker_path,
        RecoveryMetadataClass::ReplaceAllUndoJournal,
    );
    let diagnostics = save_enveloped_json_path(&config, KIND_REPLACE_UNDO_CLEANUP_MARKER, &marker)
        .with_context(|| {
            format!(
                "failed to mark replace journal cleanup in {}",
                journal_dir.display()
            )
        })?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
}

/// Report extra journal entries that are outside the active manifest.
fn detect_orphan_journal_entries(
    data_dir: &Path,
    journal_dir: &Path,
    active_entry_files: &HashSet<String>,
) -> Vec<RecoveryDiagnostic> {
    let entries = match fs_tree::scan_directory(
        journal_dir,
        DirectoryScanPolicy {
            max_entries: JOURNAL_SCAN_MAX_ENTRIES,
            include_hidden: false,
        },
    ) {
        Ok(entries) => entries,
        Err(error) => {
            return vec![RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                journal_dir,
                format!("failed to scan replace undo journal: {error}"),
            )];
        }
    };

    let mut diagnostics = Vec::new();
    if entries.len() == JOURNAL_SCAN_MAX_ENTRIES {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            journal_dir,
            "replace undo journal orphan scan reached its entry cap",
        ));
    }
    for entry in entries {
        if !is_journal_payload_file(&entry.path) {
            continue;
        }
        let Some(file_name) = entry.path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if active_entry_files.contains(file_name) {
            continue;
        }

        let orphan_load = load_enveloped_json_optional::<ReplaceBackupDiskEntry>(
            &RecoveryLoadConfig::new(
                data_dir,
                &entry.path,
                RecoveryMetadataClass::ReplaceAllUndoJournal,
            ),
            KIND_REPLACE_UNDO_ENTRY,
        );
        diagnostics.extend(orphan_load.diagnostics);
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            &entry.path,
            "replace undo journal contains an orphaned entry outside the active manifest",
        ));
    }
    diagnostics
}

/// Derive the per-file journal filename from the target path without leaking it in names.
fn entry_file_name(path: &Path) -> String {
    format!("{}.json", stable_path_hash(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    // --- pure rule tests: no tempdir, no disk ---

    #[test]
    fn activation_requires_no_diagnostics_and_manifest_agreement() {
        assert!(journal_is_active(0, Some((3, 3))));
        assert!(!journal_is_active(1, Some((3, 3))));
        assert!(!journal_is_active(0, Some((2, 3))));
        assert!(!journal_is_active(0, Some((3, 2))));
    }

    #[test]
    fn incremental_activation_has_no_manifest_to_agree_with() {
        assert!(journal_is_active(0, None));
        assert!(!journal_is_active(1, None));
    }

    #[test]
    fn payload_budget_rejects_only_above_the_cap() {
        assert!(!payload_budget_exceeded(0, 10, 10));
        assert!(!payload_budget_exceeded(4, 6, 10));
        assert!(payload_budget_exceeded(5, 6, 10));
        assert!(payload_budget_exceeded(0, 11, 10));
        // A zero cap rejects any nonzero entry but admits an empty one.
        assert!(!payload_budget_exceeded(0, 0, 0));
        assert!(payload_budget_exceeded(0, 1, 0));
    }

    #[test]
    fn payload_budget_saturates_instead_of_wrapping_into_acceptance() {
        assert!(payload_budget_exceeded(u64::MAX, 1, u64::MAX - 1));
        assert!(!payload_budget_exceeded(u64::MAX, 1, u64::MAX));
    }

    #[test]
    fn entry_count_cap_admits_the_exact_limit_and_rejects_one_over() {
        assert!(!entry_count_exceeds_cap(0));
        assert!(!entry_count_exceeds_cap(JOURNAL_SCAN_MAX_ENTRIES - 1));
        assert!(!entry_count_exceeds_cap(JOURNAL_SCAN_MAX_ENTRIES));
        assert!(entry_count_exceeds_cap(JOURNAL_SCAN_MAX_ENTRIES + 1));
    }

    #[test]
    fn retained_weight_cap_admits_the_exact_limit_and_rejects_one_over() {
        assert!(!retained_weight_exceeds_cap(0));
        assert!(!retained_weight_exceeds_cap(
            MAX_REPLACE_UNDO_RETAINED_BYTES
        ));
        assert!(retained_weight_exceeds_cap(
            MAX_REPLACE_UNDO_RETAINED_BYTES + 1
        ));
    }

    #[test]
    fn payload_filter_accepts_entries_and_rejects_journal_bookkeeping() {
        let dir = Path::new("/journal");
        assert!(is_journal_payload_file(&dir.join("abcd1234.json")));
        assert!(!is_journal_payload_file(&dir.join(JOURNAL_MANIFEST_FILE)));
        assert!(!is_journal_payload_file(&dir.join(CLEANUP_MARKER_FILE)));
        assert!(!is_journal_payload_file(&dir.join("notes.txt")));
        assert!(!is_journal_payload_file(&dir.join("no-extension")));
        assert!(!is_journal_payload_file(Path::new("/")));
    }

    #[test]
    fn dedup_rejects_a_duplicate_entry_file_and_a_duplicate_target_path() {
        let mut dedup = ManifestEntryDedup::default();
        assert_eq!(dedup.admit("a.json", Path::new("/tmp/a.rs")), Ok(()));
        assert_eq!(
            dedup.admit("a.json", Path::new("/tmp/other.rs")),
            Err(ManifestDuplicate::EntryFile)
        );
        assert_eq!(
            dedup.admit("b.json", Path::new("/tmp/a.rs")),
            Err(ManifestDuplicate::TargetPath)
        );
        assert_eq!(dedup.admit("c.json", Path::new("/tmp/c.rs")), Ok(()));
    }

    #[test]
    fn dedup_accounts_for_an_entry_file_whose_target_path_was_rejected() {
        // The entry file is still named by the manifest, so it must not later be
        // reported as an orphan outside the active manifest.
        let mut dedup = ManifestEntryDedup::default();
        assert_eq!(dedup.admit("a.json", Path::new("/tmp/a.rs")), Ok(()));
        assert_eq!(
            dedup.admit("b.json", Path::new("/tmp/a.rs")),
            Err(ManifestDuplicate::TargetPath)
        );
        assert!(dedup.admitted_entry_files().contains("b.json"));
    }

    #[test]
    fn cleanup_is_refused_when_any_diagnostic_disallows_replacement() {
        let allowed = RecoveryDiagnostic::repaired(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            Path::new("/tmp/a.json"),
            "quarantined and replaced",
        );
        // `repair_skipped` deliberately preserves the file in place, so it must
        // never be treated as safe to delete.
        let disallowed = RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            Path::new("/tmp/b.json"),
            "preserved in place",
        );
        assert!(allowed.replacement_allowed);
        assert!(!disallowed.replacement_allowed);
        assert!(cleanup_replacement_allowed(&[]));
        assert!(cleanup_replacement_allowed(std::slice::from_ref(&allowed)));
        assert!(!cleanup_replacement_allowed(std::slice::from_ref(
            &disallowed
        )));
        assert!(!cleanup_replacement_allowed(&[allowed, disallowed]));
    }

    // --- shrink path ---

    fn journal_with(paths: &[&str]) -> ReplaceUndoBackup {
        let mut backup = ReplaceUndoBackup::new();
        for path in paths {
            backup.insert(
                PathBuf::from(path),
                ReplaceUndoEntry::new(
                    format!("before-{path}").into_bytes(),
                    format!("after-{path}").into_bytes(),
                ),
            );
        }
        backup
    }

    #[test]
    fn active_journal_with_a_duplicate_target_path_is_inactive() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let target = PathBuf::from("/tmp/a.rs");
        let entry = ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec());
        save_entry(dir.path(), &target, &entry).expect("save the single entry file");
        let manifest_path = dir.path().join(JOURNAL_DIR).join(JOURNAL_MANIFEST_FILE);
        let manifest = ReplaceJournalManifest {
            incremental: false,
            entries: vec![
                ReplaceJournalManifestEntry {
                    path: target.clone(),
                    entry_file: entry_file_name(&target),
                },
                ReplaceJournalManifestEntry {
                    path: target.clone(),
                    entry_file: "duplicate-target.json".to_string(),
                },
            ],
        };
        let config = RecoveryLoadConfig::new(
            dir.path(),
            &manifest_path,
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        );
        save_enveloped_json_path(&config, KIND_REPLACE_UNDO_MANIFEST, &manifest)
            .expect("write duplicate-target manifest fixture");

        let load = load_recovering(dir.path());

        assert!(!load.active);
        assert!(load.backup.is_empty());
        assert!(load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("duplicate target path")
            )
        }));
    }

    #[test]
    fn incremental_journal_over_the_retained_memory_cap_is_inactive() {
        let dir = TempDir::new().expect("expected operation to succeed");
        mark_incremental_journal_active(dir.path()).expect("arm incremental journal");
        // Each entry carries enough bytes that a bounded number of them crosses
        // the retained-memory ceiling without needing a huge fixture.
        let entry_bytes = 1usize << 20;
        let mut index = 0usize;
        let mut retained = ReplaceUndoBackup::new();
        while retained_byte_weight_probe(&retained) <= MAX_REPLACE_UNDO_RETAINED_BYTES {
            let path = PathBuf::from(format!("/tmp/retained-{index}.rs"));
            let entry = ReplaceUndoEntry::new(vec![b'o'; entry_bytes], vec![b'r'; entry_bytes]);
            save_entry(dir.path(), &path, &entry).expect("save retained-cap entry");
            retained.insert(path, entry);
            index = index.saturating_add(1);
            assert!(index < 4_096, "retained-cap fixture should stay bounded");
        }

        // The payload cap must not be what rejects this fixture.
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(u64::MAX)));
        let load = load_recovering(dir.path());
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

        assert!(!load.active);
        assert!(load.backup.is_empty());
        assert!(load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("retained-memory limit")
            )
        }));
    }

    fn retained_byte_weight_probe(backup: &ReplaceUndoBackup) -> u64 {
        replace_undo_retained_byte_weight(backup)
    }

    #[test]
    fn shrink_keeps_the_journal_active_and_drops_only_restored_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let full = journal_with(&["/tmp/a.rs", "/tmp/b.rs", "/tmp/c.rs"]);
        save(dir.path(), &full).expect("save full journal");
        let retained_entry = dir
            .path()
            .join(JOURNAL_DIR)
            .join(entry_file_name(Path::new("/tmp/b.rs")));
        let retained_inode = fs_metadata::inode(&retained_entry).expect("stat retained entry");

        let remaining = journal_with(&["/tmp/b.rs"]);
        shrink_journal_to(dir.path(), &remaining).expect("shrink journal");

        let load = load_recovering(dir.path());
        assert!(load.active, "a shrunken journal must stay active");
        assert_eq!(load.backup, remaining);
        assert!(!fs_metadata::exists(
            &dir.path()
                .join(JOURNAL_DIR)
                .join(entry_file_name(Path::new("/tmp/a.rs")))
        ));
        // The load-bearing property, and what distinguishes a shrink from a full
        // rewrite: the retained entry file is never destroyed and recreated, so
        // there is no window in which the unrestored file has no durable
        // rollback copy. A `save` would replace this inode.
        assert_eq!(
            fs_metadata::inode(&retained_entry).expect("stat retained entry after shrink"),
            retained_inode,
            "a shrink must leave every retained entry file in place",
        );
    }

    #[test]
    fn shrink_interrupted_before_cleanup_still_loads_an_active_journal() {
        // Simulates the crash window: the smaller manifest is durable but the
        // superseded entry files have not been removed yet. Recovery must report
        // the orphans and still activate, or a partial undo would leave the
        // unrestored files with no usable rollback copy.
        let dir = TempDir::new().expect("expected operation to succeed");
        let full = journal_with(&["/tmp/a.rs", "/tmp/b.rs"]);
        save(dir.path(), &full).expect("save full journal");
        let remaining = journal_with(&["/tmp/b.rs"]);
        mark_journal_active(dir.path(), &remaining).expect("commit smaller manifest only");

        let load = load_recovering(dir.path());

        assert!(load.active);
        assert_eq!(load.backup, remaining);
        assert!(load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("orphaned entry")
            )
        }));
    }

    #[test]
    fn shrink_falls_back_to_a_full_rewrite_when_the_journal_is_not_a_superset() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let unrelated = journal_with(&["/tmp/x.rs"]);
        save(dir.path(), &unrelated).expect("save unrelated journal");

        let fresh = journal_with(&["/tmp/a.rs", "/tmp/b.rs"]);
        shrink_journal_to(dir.path(), &fresh).expect("shrink falls back to save");

        let load = load_recovering(dir.path());
        assert!(load.active);
        assert_eq!(load.backup, fresh);
    }

    #[test]
    fn shrink_falls_back_when_a_cleanup_marker_is_present() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let full = journal_with(&["/tmp/a.rs", "/tmp/b.rs"]);
        save(dir.path(), &full).expect("save full journal");
        mark_cleanup_in_progress(dir.path()).expect("mark interrupted cleanup");

        let remaining = journal_with(&["/tmp/b.rs"]);
        shrink_journal_to(dir.path(), &remaining).expect("shrink falls back to save");

        let load = load_recovering(dir.path());
        assert!(load.active, "the fallback rewrite must clear the marker");
        assert_eq!(load.backup, remaining);
    }

    #[test]
    fn shrink_to_an_empty_journal_deletes_it() {
        let dir = TempDir::new().expect("expected operation to succeed");
        save(dir.path(), &journal_with(&["/tmp/a.rs"])).expect("save journal");

        shrink_journal_to(dir.path(), &ReplaceUndoBackup::new()).expect("shrink to empty");

        assert!(
            load(dir.path())
                .expect("empty journal loads empty")
                .is_empty()
        );
        assert!(!fs_metadata::exists(&dir.path().join(JOURNAL_DIR)));
    }

    #[test]
    fn save_load_delete_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            PathBuf::from("/tmp/a.rs"),
            ReplaceUndoEntry::new(b"alpha".to_vec(), b"ALPHA".to_vec()),
        );
        backup.insert(
            PathBuf::from("/tmp/b.rs"),
            ReplaceUndoEntry::new(b"beta".to_vec(), b"BETA".to_vec()),
        );

        save(dir.path(), &backup).expect("expected operation to succeed");
        let loaded = load(dir.path()).expect("expected operation to succeed");
        assert_eq!(loaded, backup);

        delete(dir.path()).expect("expected operation to succeed");
        let after_delete = load(dir.path()).expect("expected operation to succeed");
        assert!(after_delete.is_empty());
    }

    #[test]
    fn delete_missing_backup_is_noop() {
        let dir = TempDir::new().expect("expected operation to succeed");

        delete(dir.path()).expect("expected missing backup delete to be a no-op");
    }

    #[test]
    fn delete_entry_removes_only_the_requested_journal_entry() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path_a = PathBuf::from("/tmp/a.rs");
        let path_b = PathBuf::from("/tmp/b.rs");
        let entry_a = ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec());
        let entry_b = ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec());
        save_entry(dir.path(), &path_a, &entry_a).expect("save first journal entry");
        save_entry(dir.path(), &path_b, &entry_b).expect("save second journal entry");

        delete_entry(dir.path(), &path_a).expect("delete first journal entry");

        assert!(!fs_metadata::exists(
            &dir.path().join(JOURNAL_DIR).join(entry_file_name(&path_a))
        ));
        assert!(fs_metadata::exists(
            &dir.path().join(JOURNAL_DIR).join(entry_file_name(&path_b))
        ));
    }

    #[test]
    fn delete_blocks_retired_backup_cleanup_when_preservation_is_unsafe() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&dir.path().join(BACKUP_FILE));

        let error = delete(dir.path()).expect_err("directory backup should fail deletion");

        assert!(
            error.to_string().contains("not safe to delete"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn load_recovering_keeps_entry_without_manifest_inactive() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = PathBuf::from("/tmp/a.rs");
        let entry = ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec());

        save_entry(dir.path(), &path, &entry).expect("save pre-write journal entry");

        let recovery = load_recovering(dir.path());
        assert!(!recovery.active);
        assert!(recovery.backup.is_empty());
        assert!(recovery.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("no active manifest")
            )
        }));
        assert!(
            load(dir.path())
                .expect("inactive partial journal should load as empty")
                .is_empty()
        );
    }

    #[test]
    fn malformed_journal_entry_is_quarantined_and_not_active() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path_a = PathBuf::from("/tmp/a.rs");
        let path_b = PathBuf::from("/tmp/b.rs");
        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            path_a.clone(),
            ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec()),
        );
        backup.insert(
            path_b,
            ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec()),
        );
        save(dir.path(), &backup).expect("save valid journal");
        let corrupt_entry = dir.path().join(JOURNAL_DIR).join(entry_file_name(&path_a));
        fixture::write_text(&corrupt_entry, "not valid json {{{");

        let recovery = load_recovering(dir.path());

        assert!(!recovery.active);
        assert!(recovery.backup.is_empty());
        assert!(
            recovery.diagnostics.iter().any(|diagnostic| {
                matches!(diagnostic.problem, RecoveryProblem::Malformed { .. })
            })
        );
        assert!(!fs_metadata::exists(&corrupt_entry));
        assert!(
            load(dir.path())
                .expect("malformed journal should not be exposed as undo")
                .is_empty()
        );
    }

    #[test]
    fn malformed_legacy_backup_is_quarantined_and_inactive() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let legacy = dir.path().join(BACKUP_FILE);
        fixture::write_text(&legacy, "not valid json {{{");

        let load = load_recovering(dir.path());

        assert!(!load.active);
        assert!(load.backup.is_empty());
        assert!(
            load.diagnostics.iter().any(|diagnostic| {
                matches!(diagnostic.problem, RecoveryProblem::Malformed { .. })
            })
        );
        assert!(!fs_metadata::exists(&legacy));
    }

    #[test]
    fn cleanup_marker_keeps_valid_journal_inactive_until_cleanup_finishes() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = PathBuf::from("/tmp/a.rs");
        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            path,
            ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec()),
        );
        save(dir.path(), &backup).expect("save valid journal");
        mark_cleanup_in_progress(dir.path()).expect("mark interrupted cleanup");

        let load = load_recovering(dir.path());

        assert!(!load.active);
        assert!(load.backup.is_empty());
        assert!(load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("cleanup was interrupted")
            )
        }));
    }

    #[test]
    fn cleanup_stale_reports_cleanup_failure_as_diagnostic() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&dir.path().join(BACKUP_FILE));

        let report = cleanup_stale(dir.path());

        assert_eq!(report.diagnostics.len(), 1);
        assert!(matches!(
            report.diagnostics[0].problem,
            RecoveryProblem::RepairSkipped { .. }
        ));
        assert!(fs_metadata::exists(&dir.path().join(BACKUP_FILE)));
    }

    #[test]
    fn unsupported_journal_path_is_inactive() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join(JOURNAL_DIR), "not a directory");

        let load = load_recovering(dir.path());

        assert!(!load.active);
        assert!(load.backup.is_empty());
        assert!(load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::UnsupportedFileKind { .. }
            )
        }));
    }

    #[test]
    fn active_journal_over_undo_payload_cap_is_inactive() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = PathBuf::from("/tmp/a.rs");
        let mut backup = ReplaceUndoBackup::new();
        backup.insert(
            path,
            ReplaceUndoEntry::new(b"before-content".to_vec(), b"after-content".to_vec()),
        );
        save(dir.path(), &backup).expect("save valid journal");

        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(4)));
        let load = load_recovering(dir.path());
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

        assert!(!load.active);
        assert!(load.backup.is_empty());
        assert!(load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("payload limit")
            )
        }));
    }

    #[test]
    fn active_journal_at_exact_undo_payload_cap_remains_active() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = PathBuf::from("/tmp/a.rs");
        let mut backup = ReplaceUndoBackup::new();
        let entry = ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec());
        let exact_payload = u64::try_from(entry.original_bytes.len() + entry.replaced_bytes.len())
            .expect("tiny undo payload should fit in u64");
        backup.insert(path, entry);
        save(dir.path(), &backup).expect("save valid journal");

        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(Some(exact_payload)));
        let load = load_recovering(dir.path());
        TEST_MAX_REPLACE_UNDO_BYTES.with(|cap| cap.set(None));

        assert!(load.active);
        assert_eq!(load.backup, backup);
        assert!(load.diagnostics.is_empty());
    }

    #[test]
    fn active_journal_over_manifest_entry_cap_is_inactive() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest_path = dir.path().join(JOURNAL_DIR).join(JOURNAL_MANIFEST_FILE);
        let entries = (0..=JOURNAL_SCAN_MAX_ENTRIES)
            .map(|index| ReplaceJournalManifestEntry {
                path: PathBuf::from(format!("/tmp/file-{index}.rs")),
                entry_file: format!("{index}.json"),
            })
            .collect();
        let manifest = ReplaceJournalManifest {
            incremental: false,
            entries,
        };
        let config = RecoveryLoadConfig::new(
            dir.path(),
            &manifest_path,
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        );
        save_enveloped_json_path(&config, KIND_REPLACE_UNDO_MANIFEST, &manifest)
            .expect("write oversized manifest fixture");

        let load = load_recovering(dir.path());

        assert!(!load.active);
        assert!(load.backup.is_empty());
        assert!(load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("entry cap")
            )
        }));
    }

    #[test]
    fn exact_manifest_entry_cap_is_not_reported_as_over_cap() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest_path = dir.path().join(JOURNAL_DIR).join(JOURNAL_MANIFEST_FILE);
        let entries = (0..JOURNAL_SCAN_MAX_ENTRIES)
            .map(|index| ReplaceJournalManifestEntry {
                path: PathBuf::from(format!("/tmp/file-{index}.rs")),
                entry_file: "shared-entry.json".to_string(),
            })
            .collect();
        let manifest = ReplaceJournalManifest {
            incremental: false,
            entries,
        };
        let config = RecoveryLoadConfig::new(
            dir.path(),
            &manifest_path,
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        );
        save_enveloped_json_path(&config, KIND_REPLACE_UNDO_MANIFEST, &manifest)
            .expect("write exact-cap manifest fixture");

        let load = load_recovering(dir.path());

        assert!(!load.active);
        assert!(!load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("entry cap")
            )
        }));
    }

    #[test]
    fn empty_manifest_with_matching_entry_count_is_not_active() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest_path = dir.path().join(JOURNAL_DIR).join(JOURNAL_MANIFEST_FILE);
        let manifest = ReplaceJournalManifest {
            incremental: false,
            entries: Vec::new(),
        };
        let config = RecoveryLoadConfig::new(
            dir.path(),
            &manifest_path,
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        );
        save_enveloped_json_path(&config, KIND_REPLACE_UNDO_MANIFEST, &manifest)
            .expect("write empty manifest fixture");

        let load = load_recovering(dir.path());

        assert!(!load.active);
        assert!(load.backup.is_empty());
        assert!(load.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::RepairSkipped { ref detail }
                    if detail.contains("manifest is empty")
            )
        }));
    }

    #[test]
    fn mark_journal_active_rejects_manifest_entry_count_above_cap() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut backup = ReplaceUndoBackup::new();
        for index in 0..=JOURNAL_SCAN_MAX_ENTRIES {
            backup.insert(
                PathBuf::from(format!("/tmp/file-{index}.rs")),
                ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec()),
            );
        }

        let error = mark_journal_active(dir.path(), &backup)
            .expect_err("oversized manifest should not be written");

        assert!(
            error.to_string().contains("entry cap"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn mark_journal_active_accepts_exact_manifest_entry_cap() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut backup = ReplaceUndoBackup::new();
        for index in 0..JOURNAL_SCAN_MAX_ENTRIES {
            backup.insert(
                PathBuf::from(format!("/tmp/file-{index}.rs")),
                ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec()),
            );
        }

        mark_journal_active(dir.path(), &backup).expect("exact cap should be accepted");

        assert!(fs_metadata::exists(
            &dir.path().join(JOURNAL_DIR).join(JOURNAL_MANIFEST_FILE)
        ));
    }

    #[test]
    fn orphan_journal_detection_reports_only_extra_json_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let active_path = PathBuf::from("/tmp/active.rs");
        let orphan_path = PathBuf::from("/tmp/orphan.rs");
        let active_entry = ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec());
        let orphan_entry = ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec());
        save_entry(dir.path(), &active_path, &active_entry).expect("save active journal entry");
        save_entry(dir.path(), &orphan_path, &orphan_entry).expect("save orphan journal entry");
        let journal_dir = dir.path().join(JOURNAL_DIR);
        fixture::write_text(&journal_dir.join(JOURNAL_MANIFEST_FILE), "{}");
        fixture::write_text(&journal_dir.join(CLEANUP_MARKER_FILE), "{}");
        fixture::write_text(&journal_dir.join("notes.txt"), "not a journal entry");
        let active_files = HashSet::from([entry_file_name(&active_path)]);

        let diagnostics = detect_orphan_journal_entries(dir.path(), &journal_dir, &active_files);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].original_path,
            journal_dir.join(entry_file_name(&orphan_path))
        );
        assert!(matches!(
            diagnostics[0].problem,
            RecoveryProblem::RepairSkipped { ref detail }
                if detail.contains("orphaned entry")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn save_entry_does_not_rewrite_existing_journal_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path_a = PathBuf::from("/tmp/a.rs");
        let path_b = PathBuf::from("/tmp/b.rs");
        let entry_a = ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec());
        let entry_b = ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec());

        save_entry(dir.path(), &path_a, &entry_a).expect("save first journal entry");
        let entry_a_path = dir.path().join(JOURNAL_DIR).join(entry_file_name(&path_a));
        let inode_before = fs_metadata::inode(&entry_a_path).expect("stat first entry before");

        save_entry(dir.path(), &path_b, &entry_b).expect("save second journal entry");

        let inode_after = fs_metadata::inode(&entry_a_path).expect("stat first entry after");
        assert_eq!(
            inode_after, inode_before,
            "saving a new file's journal entry must not rewrite older entries"
        );
        assert!(
            fs_metadata::exists(&dir.path().join(JOURNAL_DIR).join(entry_file_name(&path_b))),
            "the second per-file journal entry should be created independently"
        );
    }
}

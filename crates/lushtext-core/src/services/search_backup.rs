// SPDX-License-Identifier: GPL-3.0-or-later

//! Persisted undo backup for Replace All.
//!
//! Each replaced file has one entry: a descriptor naming the target, the
//! length and SHA-256 of its original and replaced bytes, and a raw body file
//! in `bodies/` holding the original bytes (design D3). Undo recognises a file
//! by its digest and streams the body back, so no layer holds file content.
//! Legacy inline entries (`replace-all-undo-entry`) still load and undo after
//! an upgrade; new runs never write them.

use crate::model::sidecar_identity::stable_path_hash;
use crate::services::app_data_layout::{AppDataLayout, AppDataLocation};
use crate::services::content_search::{
    ContentDigest, MAIN_REPLACE_UNDO_BYTES, MAX_REPLACE_UNDO_RETAINED_BYTES, ReplaceUndoBackup,
    ReplaceUndoEntry, UndoOriginal, replace_undo_retained_byte_weight,
};
use crate::services::content_search::{ShrinkAction, shrink_step};
use crate::services::filesystem::leftovers::LaunchId;
use crate::services::protocol_drive::{Protocol, drive};
use crate::services::{
    filesystem::{
        DirectoryScanPolicy, metadata as fs_metadata, mutate as fs_mutate, tree as fs_tree,
        write as fs_write,
    },
    json_format::{
        KIND_REPLACE_UNDO_CLEANUP_MARKER, KIND_REPLACE_UNDO_ENTRY, KIND_REPLACE_UNDO_MANIFEST,
        KIND_REPLACE_UNDO_STREAM_ENTRY, KIND_RETIRED_REPLACE_UNDO_BACKUP,
    },
    recovery_metadata::{
        RecoveryDiagnostic, RecoveryLoadConfig, RecoveryMetadataClass, RecoveryPreservation,
        RecoveryProblem, load_enveloped_json_optional, load_enveloped_json_optional_accepting,
        save_enveloped_json_path,
    },
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::cell::Cell;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::Duration;

// The journal's names come from the app-data layout:
// - `RetiredReplaceBackup`, the pre-public single-file backup, retained only
//   for clean-break recovery diagnostics;
// - `ReplaceJournalDir`, one pre-write undo snapshot per replaced file;
// - `ReplaceJournalManifest`, the commit marker proving a journal run has
//   active, undoable entries;
// - `ReplaceCleanupMarker`, written before cleanup so interrupted deletion
//   cannot revive undo;
// - `ReplaceJournalBodies`, one raw original-bytes body per entry, in its own
//   directory so the entry scans keep their `JOURNAL_SCAN_MAX_ENTRIES`
//   coverage and the bodies get their own scan under the same bound
//   (design D3).

/// `location` under the data home `data_dir`.
fn layout_path(data_dir: &Path, location: AppDataLocation) -> PathBuf {
    AppDataLayout::new(data_dir).path(location)
}

/// The journal manifest's file name inside the journal directory.
fn manifest_file_name() -> &'static str {
    AppDataLocation::ReplaceJournalManifest.file_name()
}

/// The cleanup marker's file name inside the journal directory.
fn cleanup_marker_file_name() -> &'static str {
    AppDataLocation::ReplaceCleanupMarker.file_name()
}

/// The bodies directory's name inside the journal directory.
fn bodies_dir_name() -> &'static str {
    AppDataLocation::ReplaceJournalBodies.file_name()
}
/// Extension of a body file.
const BODY_EXTENSION: &str = "body";
/// How often a cancellable wait for the journal guard re-checks its flag.
const JOURNAL_GUARD_POLL: Duration = Duration::from_millis(20);
/// Maximum journal entries scanned for orphan diagnostics in one recovery pass.
///
/// It bounds damaged app-data startup work while staying at or above the files
/// one Replace All can touch: Replace All acts only on previewed rows, so at
/// most `MAX_REPLACE_PREVIEW_ROWS` files (`bound-lists-by-budget-not-count` D3).
///
/// Kind: presentation cap; 10,000 rounded up to a power of two
/// (`power-of-two-thresholds` D12; its bound measured in task 9.5 M12).
pub(crate) const JOURNAL_SCAN_MAX_ENTRIES: usize = 16 * 1024;

// One Replace All touches at most the files of its preview's rows, so the
// startup inspection always reaches every entry it can have written.
const _: () =
    assert!(JOURNAL_SCAN_MAX_ENTRIES >= crate::model::content_search::MAX_REPLACE_PREVIEW_ROWS);
#[cfg(test)]
thread_local! {
    static TEST_MAX_REPLACE_UNDO_BYTES: Cell<Option<u64>> = const { Cell::new(None) };
}

/// A legacy inline entry (`replace-all-undo-entry`), read but never written.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ReplaceBackupDiskEntry {
    path: PathBuf,
    original_content: String,
    replaced_content: String,
}

/// Length and hex SHA-256 of one byte stream, as an entry stores it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DiskDigest {
    len: u64,
    sha256: String,
}

impl DiskDigest {
    fn from_digest(digest: ContentDigest) -> Self {
        Self {
            len: digest.len,
            sha256: digest.sha256_hex(),
        }
    }

    fn to_digest(&self) -> Option<ContentDigest> {
        Some(ContentDigest {
            len: self.len,
            sha256: ContentDigest::sha256_from_hex(&self.sha256)?,
        })
    }
}

/// The target's identity facts when its entry was written (design D3).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetIdentity {
    /// Device id.
    pub dev: u64,
    /// Inode number.
    pub ino: u64,
    /// Byte size.
    pub size: u64,
    /// Modification time in nanoseconds since the epoch.
    pub mtime_ns: u64,
}

/// A streamed entry (`replace-all-undo-stream-entry`): a descriptor whose
/// original bytes are the raw body file `bodies/<body_file>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplaceStreamDiskEntry {
    path: PathBuf,
    body_file: String,
    original: DiskDigest,
    replaced: DiskDigest,
    identity: TargetIdentity,
}

/// Either entry kind, told apart by shape: only a streamed entry has a
/// `body_file` and only a legacy one has inline content.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum AnyDiskEntry {
    Stream(ReplaceStreamDiskEntry),
    Legacy(ReplaceBackupDiskEntry),
}

/// What one Replace All run records for a file it is about to replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamEntryRecord {
    /// The original bytes, which the body holds.
    pub original: ContentDigest,
    /// The bytes Replace All will write.
    pub replaced: ContentDigest,
    /// The target's facts before pass A.
    pub identity: TargetIdentity,
}

/// Which kind an entry was, for the one-kind-per-journal rule (design D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Stream,
    Legacy,
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
    /// Process id of the launch that last wrote targets through this journal
    /// (design D9). Absent in journals written before streamed entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    /// That launch's temp-name nonce (design D9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    launch_nonce: Option<u32>,
}

impl ReplaceJournalManifest {
    /// The recorded launch, or a launch no temp name carries when absent.
    fn launch(&self) -> LaunchId {
        LaunchId {
            pid: self.pid.unwrap_or(0),
            nonce: self.launch_nonce.unwrap_or(0),
        }
    }
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
    journal_lock()
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

/// The directory holding the Replace All undo journal under `data_dir`.
#[must_use]
pub(crate) fn journal_dir(data_dir: &Path) -> PathBuf {
    layout_path(data_dir, AppDataLocation::ReplaceJournalDir)
}

/// The directory holding the journal's raw undo bodies under `data_dir`.
#[must_use]
pub(crate) fn bodies_dir(data_dir: &Path) -> PathBuf {
    layout_path(data_dir, AppDataLocation::ReplaceJournalBodies)
}

/// Load Replace All undo state with recovery diagnostics.
///
/// Callers must only expose the returned backup when `active` is true. Stale,
/// malformed, or incomplete journals are kept diagnostic and inactive so a
/// crash during cleanup cannot resurrect an unsafe undo action.
#[must_use]
pub fn load_recovering(data_dir: &Path) -> ReplaceBackupRecoveryLoad {
    let journal_dir = layout_path(data_dir, AppDataLocation::ReplaceJournalDir);
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
    let path = layout_path(data_dir, AppDataLocation::RetiredReplaceBackup);
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

/// Wait for the journal guard while polling `cancel` (design D3a).
///
/// The guard's other holders are startup recovery and the journal disk jobs,
/// both brief; a run waits for them here, so Cancel, Stop and Close are seen
/// while it waits. Returns `Ok(None)` when cancelled first.
///
/// # Errors
///
/// Returns an error if a previous holder panicked while owning the guard.
pub fn acquire_journal_guard_cancellable(
    cancel: &AtomicBool,
) -> Result<Option<MutexGuard<'static, ()>>> {
    acquire_journal_guard_while_current(|| !cancel.load(Ordering::Relaxed))
}

/// Wait for the journal guard while `current` holds (design D3a).
///
/// A journal disk job passes its generation check here, so a job a newer
/// one superseded returns before it waits, and while it waits, instead of
/// holding a worker slot until the guard is free. `current` is checked
/// before every attempt; the caller checks it again once it holds the guard.
/// Returns `Ok(None)` when `current` stopped holding first.
///
/// # Errors
///
/// Returns an error if a previous holder panicked while owning the guard.
pub fn acquire_journal_guard_while_current(
    mut current: impl FnMut() -> bool,
) -> Result<Option<MutexGuard<'static, ()>>> {
    loop {
        if !current() {
            return Ok(None);
        }
        match journal_lock().try_lock() {
            Ok(guard) => return Ok(Some(guard)),
            Err(TryLockError::WouldBlock) => std::thread::sleep(JOURNAL_GUARD_POLL),
            Err(TryLockError::Poisoned(_)) => {
                anyhow::bail!("replace undo backup disk lock poisoned")
            }
        }
    }
}

fn journal_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// The entry JSON of `path` in the journal under `data_dir`.
#[must_use]
pub fn entry_path(data_dir: &Path, path: &Path) -> PathBuf {
    journal_dir(data_dir).join(entry_file_name(path))
}

/// The raw body file of `path` in the journal under `data_dir` (design D3).
#[must_use]
pub fn body_path(data_dir: &Path, path: &Path) -> PathBuf {
    bodies_dir(data_dir).join(body_file_name(path))
}

/// Shrink the journal to `backup`, never rebuilding it (design D3a).
///
/// A descriptor journal cannot be rebuilt from memory, so this only removes:
/// a partial undo or rollback leaves a subset of the on-disk entries, and
/// every retained entry and body is already durable.
///
/// - An incremental journal (every journal a run writes) needs no manifest
///   change: the entries and bodies `backup` no longer lists are removed,
///   each entry before its body.
/// - A manifest-listed legacy journal first gets the smaller active manifest,
///   then loses the entries it no longer lists.
///
/// Neither order has an inactive window. Orphan bodies are removed too.
///
/// # Errors
///
/// Returns an error, and changes nothing, when an entry `backup` retains is
/// missing on disk (it is reported, never rebuilt), or when the smaller
/// manifest cannot be written.
pub fn shrink_journal_to(data_dir: &Path, backup: &ReplaceUndoBackup) -> Result<()> {
    if backup.is_empty() {
        return delete(data_dir);
    }

    let journal_dir = layout_path(data_dir, AppDataLocation::ReplaceJournalDir);
    let retained = backup
        .keys()
        .map(|path| entry_file_name(path))
        .collect::<HashSet<_>>();
    let is_superset = fs_metadata::path_status(&journal_dir)
        .is_ok_and(super::filesystem::types::PathStatus::is_directory)
        && !fs_metadata::exists(&journal_dir.join(cleanup_marker_file_name()))
        && retained
            .iter()
            .all(|entry_file| fs_metadata::exists(&journal_dir.join(entry_file)));
    if !is_superset {
        anyhow::bail!(
            "replace undo journal no longer holds every retained entry; it is not rebuilt"
        );
    }

    let manifest = load_manifest_value(data_dir, &journal_dir);
    if manifest
        .as_ref()
        .is_some_and(|manifest| !manifest.incremental)
    {
        mark_journal_active(data_dir, backup)?;
    }
    let mut retained_bodies = backup
        .keys()
        .map(|path| body_file_name(path))
        .collect::<HashSet<_>>();
    // A body whose entry could not be removed is still referenced by that
    // entry, so it is not an orphan (design D9).
    retained_bodies.extend(remove_unlisted_journal_entries(&journal_dir, &retained));
    remove_unlisted_bodies(&journal_dir, &retained_bodies);
    Ok(())
}

/// Delete payload entries the journal no longer lists, each before its body.
///
/// Best-effort by design: the journal is already durable and correct, and a
/// leftover entry is only an orphan diagnostic, so a removal failure must not be
/// reported as a failed journal write. Returns the body file names kept
/// because their entry could not be removed, which the orphan-body cleanup
/// after it must keep too.
fn remove_unlisted_journal_entries(
    journal_dir: &Path,
    retained: &HashSet<String>,
) -> HashSet<String> {
    let mut kept_bodies = HashSet::new();
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
            return kept_bodies;
        }
    };
    for entry in entries {
        if !is_journal_payload_file(&entry.path) {
            continue;
        }
        let Some(file_name) = entry.path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if retained.contains(file_name) {
            continue;
        }
        let body = journal_dir.join(bodies_dir_name()).join(format!(
            "{}.{BODY_EXTENSION}",
            file_name.trim_end_matches(".json")
        ));
        // The entry goes before its body, and a body whose entry could not be
        // removed stays: no entry may reference a missing body (design D9).
        let mut run = ShrinkRun(ShrinkAction::RemoveEntry);
        let body_kept = drive(&mut run, ShrinkAction::RemoveEntry, |action| {
            let target = match action {
                ShrinkAction::RemoveEntry => &entry.path,
                ShrinkAction::RemoveBody => &body,
                ShrinkAction::Finish { .. } => {
                    unreachable!("`drive` never executes the terminal action")
                }
            };
            let removed = fs_mutate::remove_file_if_exists(target);
            if let Err(error) = &removed {
                tracing::warn!(
                    "Failed to remove superseded replace undo file {}: {error}",
                    target.display()
                );
            }
            removed.is_ok()
        });
        if body_kept && let Some(body_name) = body.file_name().and_then(OsStr::to_str) {
            kept_bodies.insert(body_name.to_owned());
        }
    }
    kept_bodies
}

/// One retired entry's shrink as [`drive`] runs it. The free
/// [`shrink_step`] maps the current action to the next, so the run keeps the
/// current action; its end is whether the body was kept.
struct ShrinkRun(ShrinkAction);

impl Protocol for ShrinkRun {
    type Action = ShrinkAction;
    type Outcome = bool;
    type End = bool;

    fn end(action: ShrinkAction) -> Option<bool> {
        match action {
            ShrinkAction::Finish { body_kept } => Some(body_kept),
            ShrinkAction::RemoveEntry | ShrinkAction::RemoveBody => None,
        }
    }

    fn advance(&mut self, succeeded: bool) -> ShrinkAction {
        self.0 = shrink_step(self.0, succeeded);
        self.0
    }
}

/// Remove bodies no retained entry names: orphans a crash between a body and
/// its entry, or between an entry's removal and its body's, left (design D9).
fn remove_unlisted_bodies(journal_dir: &Path, retained: &HashSet<String>) {
    let bodies_dir = journal_dir.join(bodies_dir_name());
    let Ok(entries) = fs_tree::scan_directory(
        &bodies_dir,
        DirectoryScanPolicy {
            max_entries: JOURNAL_SCAN_MAX_ENTRIES,
            include_hidden: false,
        },
    ) else {
        return;
    };
    for entry in entries {
        let Some(file_name) = entry.path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if entry.path.extension().and_then(OsStr::to_str) != Some(BODY_EXTENSION)
            || retained.contains(file_name)
        {
            continue;
        }
        if let Err(error) = fs_mutate::remove_file_if_exists(&entry.path) {
            tracing::warn!(
                "Failed to remove orphan replace undo body {}: {error}",
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
    delete(data_dir)?;
    let bodies = layout_path(data_dir, AppDataLocation::ReplaceJournalBodies);
    fs_write::create_dir_all_durable(&bodies)
        .with_context(|| format!("failed to create {}", bodies.display()))?;
    Ok(())
}

/// Save one streamed entry for `path`, after its body is durable (P4).
///
/// # Errors
///
/// Returns an error if the entry cannot be durably written.
pub fn save_stream_entry(data_dir: &Path, path: &Path, record: &StreamEntryRecord) -> Result<()> {
    let disk = ReplaceStreamDiskEntry {
        path: path.to_path_buf(),
        body_file: body_file_name(path),
        original: DiskDigest::from_digest(record.original),
        replaced: DiskDigest::from_digest(record.replaced),
        identity: record.identity,
    };
    let entry_path = entry_path(data_dir, path);
    let config = RecoveryLoadConfig::new(
        data_dir,
        &entry_path,
        RecoveryMetadataClass::ReplaceAllUndoJournal,
    );
    let diagnostics = save_enveloped_json_path(&config, KIND_REPLACE_UNDO_STREAM_ENTRY, &disk)?;
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
/// digest. The manifest records `launch`, whose temp files the next recovery
/// owns (design D9).
///
/// # Errors
///
/// Returns an error when the active marker cannot be written durably.
pub fn mark_incremental_journal_active(data_dir: &Path, launch: LaunchId) -> Result<()> {
    let journal_dir = layout_path(data_dir, AppDataLocation::ReplaceJournalDir);
    fs_write::create_dir_all_durable(&journal_dir)
        .with_context(|| format!("failed to create {}", journal_dir.display()))?;
    fs_mutate::remove_file_if_exists(&journal_dir.join(cleanup_marker_file_name())).with_context(
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
        pid: Some(launch.pid),
        launch_nonce: Some(launch.nonce),
    };
    save_journal_manifest(data_dir, &journal_dir, &manifest)
}

/// Re-arm the journal in `journal_dir` for `launch`, before the first restore
/// write of an Undo in a later launch than the manifest records (design D6).
///
/// A manifest-listed legacy journal keeps its list.
///
/// # Errors
///
/// Returns an error when the manifest is missing or cannot be rewritten.
pub fn rearm_journal(journal_dir: &Path, launch: LaunchId) -> Result<()> {
    let data_dir = journal_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("replace undo journal has no parent directory"))?;
    let mut manifest = load_manifest_value(data_dir, journal_dir)
        .ok_or_else(|| anyhow::anyhow!("replace undo journal has no active manifest"))?;
    manifest.pid = Some(launch.pid);
    manifest.launch_nonce = Some(launch.nonce);
    save_journal_manifest(data_dir, journal_dir, &manifest)
}

fn load_manifest_value(data_dir: &Path, journal_dir: &Path) -> Option<ReplaceJournalManifest> {
    load_enveloped_json_optional::<ReplaceJournalManifest>(
        &RecoveryLoadConfig::new(
            data_dir,
            &journal_dir.join(manifest_file_name()),
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        ),
        KIND_REPLACE_UNDO_MANIFEST,
    )
    .value
}

/// Delete one per-file journal entry. Missing entries are treated as already gone.
///
/// # Errors
///
/// Returns an error if an existing entry cannot be removed.
pub fn delete_entry(data_dir: &Path, path: &Path) -> Result<()> {
    let entry_path = entry_path(data_dir, path);
    match fs_mutate::remove_file_if_exists(&entry_path) {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete replace journal entry {}: {}",
            entry_path.display(),
            e
        )),
    }
}

/// Delete one body, after its entry. Missing bodies are treated as already gone.
///
/// # Errors
///
/// Returns an error if an existing body cannot be removed.
pub fn delete_body(data_dir: &Path, path: &Path) -> Result<()> {
    let body = body_path(data_dir, path);
    match fs_mutate::remove_file_if_exists(&body) {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete replace undo body {}: {}",
            body.display(),
            e
        )),
    }
}

/// Re-read a legacy entry's original bytes, under `limit` (design D6).
///
/// Undo calls this on its worker for one entry at a time.
///
/// # Errors
///
/// Returns an error when the entry is missing, malformed, not legacy, or its
/// payload exceeds `limit`.
pub fn load_legacy_original(entry_file: &Path, limit: u64) -> Result<Vec<u8>> {
    let journal_dir = entry_file
        .parent()
        .ok_or_else(|| anyhow::anyhow!("replace undo entry has no journal directory"))?;
    let data_dir = journal_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("replace undo journal has no parent directory"))?;
    let load = load_enveloped_json_optional_accepting::<AnyDiskEntry>(
        &RecoveryLoadConfig::new(
            data_dir,
            entry_file,
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        ),
        KIND_REPLACE_UNDO_STREAM_ENTRY,
        &[KIND_REPLACE_UNDO_ENTRY],
    );
    match load.value {
        Some(AnyDiskEntry::Legacy(disk)) => {
            if replace_entry_payload_bytes(&disk) > limit {
                anyhow::bail!("legacy replace undo entry exceeds the undo payload limit");
            }
            Ok(disk.original_content.into_bytes())
        }
        Some(AnyDiskEntry::Stream(_)) => anyhow::bail!("replace undo entry is not a legacy entry"),
        None => anyhow::bail!("legacy replace undo entry could not be loaded"),
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

    let journal_dir = layout_path(data_dir, AppDataLocation::ReplaceJournalDir);
    fs_write::create_dir_all_durable(&journal_dir)
        .with_context(|| format!("failed to create {}", journal_dir.display()))?;
    fs_mutate::remove_file_if_exists(&journal_dir.join(cleanup_marker_file_name())).with_context(
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
    // A shrink keeps the launch the manifest records (design D9).
    let prior = load_manifest_value(data_dir, &journal_dir);
    let manifest = ReplaceJournalManifest {
        incremental: false,
        entries,
        pid: prior.as_ref().and_then(|manifest| manifest.pid),
        launch_nonce: prior.as_ref().and_then(|manifest| manifest.launch_nonce),
    };
    save_journal_manifest(data_dir, &journal_dir, &manifest)
}

fn save_journal_manifest(
    data_dir: &Path,
    journal_dir: &Path,
    manifest: &ReplaceJournalManifest,
) -> Result<()> {
    let manifest_path = journal_dir.join(manifest_file_name());
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
                layout_path(data_dir, AppDataLocation::ReplaceJournalDir),
                format!("replace undo journal cleanup failed: {error}"),
            )],
        },
    }
}

/// Load a journal directory only when its active manifest and entries agree.
fn load_journal(data_dir: &Path, journal_dir: &Path) -> ReplaceBackupRecoveryLoad {
    let cleanup_marker = journal_dir.join(cleanup_marker_file_name());
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

    let manifest_path = journal_dir.join(manifest_file_name());
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
        return load_incremental_journal(data_dir, journal_dir, manifest.launch(), diagnostics);
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
    let mut kinds = JournalKinds::default();

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
        let Some(loaded) = load_descriptor(
            data_dir,
            journal_dir,
            &entry_path,
            manifest.launch(),
            &mut diagnostics,
            "replace undo journal manifest references a missing entry",
        ) else {
            continue;
        };
        if loaded.path != entry.path {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                &entry_path,
                "replace undo journal entry path does not match the active manifest",
            ));
            continue;
        }
        if payload_budget_exceeded(
            undo_payload_bytes,
            loaded.legacy_payload_bytes,
            effective_max_replace_undo_bytes(),
        ) {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                &entry_path,
                "replace undo journal exceeds the Replace All undo payload limit",
            ));
            break;
        }
        undo_payload_bytes = undo_payload_bytes.saturating_add(loaded.legacy_payload_bytes);
        kinds.record(loaded.kind, &entry_path, &mut diagnostics);
        backup.insert(loaded.path, loaded.descriptor);
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
    launch: LaunchId,
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
    let mut kinds = JournalKinds::default();
    let mut lost_entry = false;
    for entry_path in entry_paths.into_iter().take(JOURNAL_SCAN_MAX_ENTRIES) {
        let Some(loaded) = load_descriptor(
            data_dir,
            journal_dir,
            &entry_path,
            launch,
            &mut diagnostics,
            "incremental replace undo journal entry could not be loaded",
        ) else {
            lost_entry = true;
            continue;
        };
        let entry_payload_bytes = loaded.legacy_payload_bytes;
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
        kinds.record(loaded.kind, &entry_path, &mut diagnostics);
        let target_path = loaded.path.clone();
        if backup
            .insert(target_path.clone(), loaded.descriptor)
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
    if !active && lost_entry {
        // An incremental journal lists no entries, so once a damaged entry is
        // quarantined the rest would load as a complete journal and offer
        // Undo for a subset. The cleanup marker keeps it inactive until
        // cleanup removes it, as a missing entry does for a listed journal.
        if let Err(error) = mark_cleanup_in_progress(data_dir) {
            tracing::warn!("Failed to mark a damaged replace undo journal for cleanup: {error}");
        }
    }
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
    let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    if file_name == manifest_file_name() || file_name == cleanup_marker_file_name() {
        return false;
    }
    path.extension().and_then(OsStr::to_str) == Some("json")
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
    MAIN_REPLACE_UNDO_BYTES
}

/// Test-only journal writers: the states a run, a crash or an older build
/// leave, built from bytes (never compiled into a shipping build).
#[cfg(any(test, feature = "test-utils"))]
pub mod fixture {
    use super::{
        AppDataLocation, KIND_REPLACE_UNDO_ENTRY, LaunchId, RecoveryLoadConfig,
        RecoveryMetadataClass, ReplaceBackupDiskEntry, ReplaceJournalManifest, Result,
        StreamEntryRecord, TargetIdentity, body_path, entry_path, layout_path,
        save_enveloped_json_path, save_journal_manifest, save_stream_entry,
    };
    use crate::services::content_search::{
        ContentDigest, ReplaceUndoBackup, ReplaceUndoEntry, UndoOriginal,
    };
    use crate::services::filesystem::{WriteLabel, write as fs_write};
    use std::path::Path;

    /// Write an armed streamed journal for `files` (target, original,
    /// replaced) as a run of this launch would, and return its descriptors.
    /// The targets themselves are not touched.
    ///
    /// # Errors
    ///
    /// Returns an error when any journal file cannot be written.
    pub fn write_stream_journal(
        data_dir: &Path,
        files: &[(&Path, &[u8], &[u8])],
    ) -> Result<ReplaceUndoBackup> {
        super::begin_incremental_journal(data_dir)?;
        let launch = LaunchId::current();
        for (index, (path, original, replaced)) in files.iter().enumerate() {
            let body = body_path(data_dir, path);
            fs_write::atomic_replace(&body, WriteLabel::REPLACE_UNDO_BODY, original)?;
            let record = StreamEntryRecord {
                original: ContentDigest::of(original),
                replaced: ContentDigest::of(replaced),
                identity: TargetIdentity::default(),
            };
            save_stream_entry(data_dir, path, &record)?;
            if index == 0 {
                super::mark_incremental_journal_active(data_dir, launch)?;
            }
        }
        Ok(stream_descriptors(data_dir, files))
    }

    /// The descriptors [`write_stream_journal`] returns for `files`, without
    /// writing anything: what a run of this launch would hand the panel.
    #[must_use]
    pub fn stream_descriptors(
        data_dir: &Path,
        files: &[(&Path, &[u8], &[u8])],
    ) -> ReplaceUndoBackup {
        let launch = LaunchId::current();
        files
            .iter()
            .map(|(path, original, replaced)| {
                (
                    path.to_path_buf(),
                    ReplaceUndoEntry {
                        entry_file: entry_path(data_dir, path),
                        original_source: UndoOriginal::Body(body_path(data_dir, path)),
                        original: ContentDigest::of(original),
                        replaced: ContentDigest::of(replaced),
                        launch,
                    },
                )
            })
            .collect()
    }

    /// Write an active legacy journal (inline `replace-all-undo-entry`
    /// entries, as a build before streamed entries wrote it), manifest-listed
    /// or incremental, and return the descriptors its loader produces.
    ///
    /// # Errors
    ///
    /// Returns an error when any journal file cannot be written, or a payload
    /// is not UTF-8.
    pub fn write_legacy_journal(
        data_dir: &Path,
        files: &[(&Path, &[u8], &[u8])],
        incremental: bool,
    ) -> Result<ReplaceUndoBackup> {
        super::delete(data_dir)?;
        let journal_dir = layout_path(data_dir, AppDataLocation::ReplaceJournalDir);
        fs_write::create_dir_all_durable(&journal_dir)?;
        for (path, original, replaced) in files {
            let disk = ReplaceBackupDiskEntry {
                path: path.to_path_buf(),
                original_content: String::from_utf8(original.to_vec())?,
                replaced_content: String::from_utf8(replaced.to_vec())?,
            };
            let entry = entry_path(data_dir, path);
            let config = RecoveryLoadConfig::new(
                data_dir,
                &entry,
                RecoveryMetadataClass::ReplaceAllUndoJournal,
            );
            save_enveloped_json_path(&config, KIND_REPLACE_UNDO_ENTRY, &disk)?;
        }
        let manifest = ReplaceJournalManifest {
            incremental,
            entries: if incremental {
                Vec::new()
            } else {
                files
                    .iter()
                    .map(|(path, _, _)| super::ReplaceJournalManifestEntry {
                        path: path.to_path_buf(),
                        entry_file: super::entry_file_name(path),
                    })
                    .collect()
            },
            pid: None,
            launch_nonce: None,
        };
        save_journal_manifest(data_dir, &journal_dir, &manifest)?;
        Ok(super::load_recovering(data_dir).active_backup())
    }
}

/// Delete the persisted Replace All undo backup, if it exists.
///
/// # Errors
///
/// Returns an error if an existing backup file cannot be deleted.
pub fn delete(data_dir: &Path) -> Result<()> {
    mark_cleanup_in_progress(data_dir)?;

    let legacy = layout_path(data_dir, AppDataLocation::RetiredReplaceBackup);
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

    let journal_dir = layout_path(data_dir, AppDataLocation::ReplaceJournalDir);
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
    let legacy = layout_path(data_dir, AppDataLocation::RetiredReplaceBackup);
    let journal_dir = layout_path(data_dir, AppDataLocation::ReplaceJournalDir);
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
    let marker_path = journal_dir.join(cleanup_marker_file_name());
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
        let Some(file_name) = entry.path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if active_entry_files.contains(file_name) {
            continue;
        }

        let orphan_load = load_enveloped_json_optional_accepting::<AnyDiskEntry>(
            &RecoveryLoadConfig::new(
                data_dir,
                &entry.path,
                RecoveryMetadataClass::ReplaceAllUndoJournal,
            ),
            KIND_REPLACE_UNDO_STREAM_ENTRY,
            &[KIND_REPLACE_UNDO_ENTRY],
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

/// The body file name of `path`: the entry's hash with the body extension.
fn body_file_name(path: &Path) -> String {
    format!("{}.{BODY_EXTENSION}", stable_path_hash(path))
}

/// One entry turned into an undo descriptor.
struct LoadedDescriptor {
    path: PathBuf,
    descriptor: ReplaceUndoEntry,
    kind: EntryKind,
    /// The inline payload a legacy entry carried, charged against `main`'s
    /// undo payload bound; zero for a streamed entry.
    legacy_payload_bytes: u64,
}

/// Load one entry of either kind as a descriptor, reading no body.
///
/// A streamed entry's body is stat-ed: missing, or a size other than the
/// original's length, makes the entry malformed (design D9). A legacy entry's
/// digests are computed here, on the loading worker, and its inline strings
/// dropped.
fn load_descriptor(
    data_dir: &Path,
    journal_dir: &Path,
    entry_path: &Path,
    launch: LaunchId,
    diagnostics: &mut Vec<RecoveryDiagnostic>,
    missing: &'static str,
) -> Option<LoadedDescriptor> {
    let entry_load = load_enveloped_json_optional_accepting::<AnyDiskEntry>(
        &RecoveryLoadConfig::new(
            data_dir,
            entry_path,
            RecoveryMetadataClass::ReplaceAllUndoJournal,
        ),
        KIND_REPLACE_UNDO_STREAM_ENTRY,
        &[KIND_REPLACE_UNDO_ENTRY],
    );
    let Some(disk) = entry_load.value else {
        if entry_load.diagnostics.is_empty() {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::ReplaceAllUndoJournal,
                entry_path,
                missing,
            ));
        } else {
            diagnostics.extend(entry_load.diagnostics);
        }
        return None;
    };
    diagnostics.extend(entry_load.diagnostics);
    let malformed = |diagnostics: &mut Vec<RecoveryDiagnostic>, detail: &'static str| {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::ReplaceAllUndoJournal,
            entry_path,
            detail,
        ));
    };
    match disk {
        AnyDiskEntry::Stream(disk) => {
            let (Some(original), Some(replaced)) =
                (disk.original.to_digest(), disk.replaced.to_digest())
            else {
                malformed(
                    diagnostics,
                    "replace undo journal entry has a malformed digest",
                );
                return None;
            };
            if disk.body_file != body_file_name(&disk.path) {
                malformed(
                    diagnostics,
                    "replace undo journal entry names an unexpected body",
                );
                return None;
            }
            let body = journal_dir.join(bodies_dir_name()).join(&disk.body_file);
            if !fs_metadata::file_stat(&body).is_ok_and(|facts| facts.byte_size == original.len) {
                // The entry stays in place for the cleanup marker's pass; a
                // copy in the quarantine is the evidence (design D9).
                diagnostics.push(crate::services::recovery_metadata::copy_into_quarantine(
                    &RecoveryLoadConfig::new(
                        data_dir,
                        entry_path,
                        RecoveryMetadataClass::ReplaceAllUndoJournal,
                    ),
                    RecoveryProblem::RepairSkipped {
                        detail:
                            "replace undo journal entry's body is missing or has the wrong size"
                                .to_string(),
                    },
                ));
                return None;
            }
            Some(LoadedDescriptor {
                descriptor: ReplaceUndoEntry {
                    entry_file: entry_path.to_path_buf(),
                    original_source: UndoOriginal::Body(body),
                    original,
                    replaced,
                    launch,
                },
                path: disk.path,
                kind: EntryKind::Stream,
                legacy_payload_bytes: 0,
            })
        }
        AnyDiskEntry::Legacy(disk) => {
            let legacy_payload_bytes = replace_entry_payload_bytes(&disk);
            Some(LoadedDescriptor {
                descriptor: ReplaceUndoEntry {
                    entry_file: entry_path.to_path_buf(),
                    original_source: UndoOriginal::LegacyEntry,
                    original: ContentDigest::of(disk.original_content.as_bytes()),
                    replaced: ContentDigest::of(disk.replaced_content.as_bytes()),
                    launch,
                },
                path: disk.path,
                kind: EntryKind::Legacy,
                legacy_payload_bytes,
            })
        }
    }
}

/// The one-kind-per-journal rule: one run writes one kind, so a journal
/// holding both is malformed (design D6).
#[derive(Default)]
struct JournalKinds {
    first: Option<EntryKind>,
    reported: bool,
}

impl JournalKinds {
    fn record(
        &mut self,
        kind: EntryKind,
        entry_path: &Path,
        diagnostics: &mut Vec<RecoveryDiagnostic>,
    ) {
        match self.first {
            None => self.first = Some(kind),
            Some(first) if first != kind && !self.reported => {
                self.reported = true;
                diagnostics.push(RecoveryDiagnostic::repair_skipped(
                    RecoveryMetadataClass::ReplaceAllUndoJournal,
                    entry_path,
                    "replace undo journal mixes legacy and streamed entries",
                ));
            }
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    // Fixtures in the shape these tests were written in, over the streamed
    // format: an entry is built from its original and replaced bytes, written
    // as a body plus a streamed entry, and compared by its two digests (all a
    // descriptor keeps). These local items shadow the glob import.

    /// A test entry: the bytes it was built from, compared by digest.
    #[derive(Debug, Clone)]
    struct ReplaceUndoEntry {
        original: ContentDigest,
        replaced: ContentDigest,
        original_bytes: Option<Vec<u8>>,
    }

    impl ReplaceUndoEntry {
        #[expect(
            clippy::needless_pass_by_value,
            reason = "mirrors the retired `ReplaceUndoEntry::new(Vec<u8>, Vec<u8>)` the ported tests call"
        )]
        fn new(original: Vec<u8>, replaced: Vec<u8>) -> Self {
            Self {
                original: ContentDigest::of(&original),
                replaced: ContentDigest::of(&replaced),
                original_bytes: Some(original),
            }
        }
    }

    impl PartialEq for ReplaceUndoEntry {
        fn eq(&self, other: &Self) -> bool {
            self.original == other.original && self.replaced == other.replaced
        }
    }

    type ReplaceUndoBackup = std::collections::BTreeMap<PathBuf, ReplaceUndoEntry>;

    struct TestLoad {
        backup: ReplaceUndoBackup,
        active: bool,
        diagnostics: Vec<RecoveryDiagnostic>,
    }

    fn from_real(real: super::ReplaceUndoBackup) -> ReplaceUndoBackup {
        real.into_iter()
            .map(|(path, entry)| {
                (
                    path,
                    ReplaceUndoEntry {
                        original: entry.original,
                        replaced: entry.replaced,
                        original_bytes: None,
                    },
                )
            })
            .collect()
    }

    fn to_real(data_dir: &Path, backup: &ReplaceUndoBackup) -> super::ReplaceUndoBackup {
        backup
            .iter()
            .map(|(path, entry)| {
                (
                    path.clone(),
                    super::ReplaceUndoEntry {
                        entry_file: super::entry_path(data_dir, path),
                        original_source: UndoOriginal::Body(body_path(data_dir, path)),
                        original: entry.original,
                        replaced: entry.replaced,
                        launch: LaunchId::current(),
                    },
                )
            })
            .collect()
    }

    fn load_recovering(data_dir: &Path) -> TestLoad {
        let load = super::load_recovering(data_dir);
        TestLoad {
            backup: from_real(load.backup),
            active: load.active,
            diagnostics: load.diagnostics,
        }
    }

    fn load(data_dir: &Path) -> Result<ReplaceUndoBackup> {
        super::load(data_dir).map(from_real)
    }

    /// Write one body and its streamed entry, as pass A and P4 do.
    fn save_entry(data_dir: &Path, path: &Path, entry: &ReplaceUndoEntry) -> Result<()> {
        let bodies = layout_path(data_dir, AppDataLocation::ReplaceJournalBodies);
        fs_write::create_dir_all_durable(&bodies)?;
        let original = entry
            .original_bytes
            .as_deref()
            .expect("a fixture entry carries its bytes");
        fs_write::atomic_replace(
            &body_path(data_dir, path),
            crate::services::filesystem::WriteLabel::REPLACE_UNDO_BODY,
            original,
        )?;
        save_stream_entry(
            data_dir,
            path,
            &StreamEntryRecord {
                original: entry.original,
                replaced: entry.replaced,
                identity: TargetIdentity::default(),
            },
        )
    }

    /// A complete manifest-listed journal, as `main`'s `save` wrote one.
    fn save(data_dir: &Path, backup: &ReplaceUndoBackup) -> Result<()> {
        delete(data_dir)?;
        if backup.is_empty() {
            return Ok(());
        }
        for (path, entry) in backup {
            save_entry(data_dir, path, entry)?;
        }
        mark_journal_active(data_dir, backup)
    }

    fn mark_journal_active(data_dir: &Path, backup: &ReplaceUndoBackup) -> Result<()> {
        super::mark_journal_active(data_dir, &to_real(data_dir, backup))
    }

    fn shrink_journal_to(data_dir: &Path, backup: &ReplaceUndoBackup) -> Result<()> {
        super::shrink_journal_to(data_dir, &to_real(data_dir, backup))
    }

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

    /// No loss against `main` (`power-of-two-thresholds` D12): `main`'s
    /// startup inspection admitted a journal of 10,000 entries.
    #[test]
    fn no_loss_a_journal_main_inspected_in_full_stays_in_full() {
        assert!(!entry_count_exceeds_cap(10_000));
        assert!(!entry_count_exceeds_cap(16 * 1024));
        assert!(entry_count_exceeds_cap(16 * 1024 + 1));
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
        assert!(!is_journal_payload_file(&dir.join(manifest_file_name())));
        assert!(!is_journal_payload_file(
            &dir.join(cleanup_marker_file_name())
        ));
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
        let manifest_path =
            layout_path(dir.path(), AppDataLocation::ReplaceJournalDir).join(manifest_file_name());
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
            pid: None,
            launch_nonce: None,
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
    fn shrink_keeps_the_journal_active_and_drops_only_restored_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let full = journal_with(&["/tmp/a.rs", "/tmp/b.rs", "/tmp/c.rs"]);
        save(dir.path(), &full).expect("save full journal");
        let retained_entry = layout_path(dir.path(), AppDataLocation::ReplaceJournalDir)
            .join(entry_file_name(Path::new("/tmp/b.rs")));
        let retained_inode = fs_metadata::inode(&retained_entry).expect("stat retained entry");

        let remaining = journal_with(&["/tmp/b.rs"]);
        shrink_journal_to(dir.path(), &remaining).expect("shrink journal");

        let load = load_recovering(dir.path());
        assert!(load.active, "a shrunken journal must stay active");
        assert_eq!(load.backup, remaining);
        assert!(!fs_metadata::exists(
            &layout_path(dir.path(), AppDataLocation::ReplaceJournalDir)
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

    /// D3a: shrink never rebuilds a journal. A journal that no longer holds
    /// every retained entry is reported and left as it is.
    #[test]
    fn shrink_refuses_a_journal_that_is_not_a_superset() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let unrelated = journal_with(&["/tmp/x.rs"]);
        save(dir.path(), &unrelated).expect("save unrelated journal");

        let fresh = journal_with(&["/tmp/a.rs", "/tmp/b.rs"]);
        shrink_journal_to(dir.path(), &fresh).expect_err("shrink does not rebuild");

        let load = load_recovering(dir.path());
        assert!(load.active);
        assert_eq!(load.backup, unrelated);
    }

    /// D3a: an interrupted cleanup is never undone by a shrink, so its undo
    /// is never resurrected.
    #[test]
    fn shrink_refuses_after_an_interrupted_cleanup() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let full = journal_with(&["/tmp/a.rs", "/tmp/b.rs"]);
        save(dir.path(), &full).expect("save full journal");
        mark_cleanup_in_progress(dir.path()).expect("mark interrupted cleanup");

        let remaining = journal_with(&["/tmp/b.rs"]);
        shrink_journal_to(dir.path(), &remaining).expect_err("shrink does not rebuild");

        let load = load_recovering(dir.path());
        assert!(
            !load.active,
            "the cleanup marker keeps the journal inactive"
        );
        assert!(load.backup.is_empty());
    }

    /// D9 through `shrink_step`: a retired entry and its body are both
    /// removed, and the retained entry keeps both of its files.
    #[test]
    fn shrink_removes_both_files_of_each_retired_entry() {
        let dir = TempDir::new().expect("expected operation to succeed");
        save(dir.path(), &journal_with(&["/tmp/a.rs", "/tmp/b.rs"])).expect("save journal");

        shrink_journal_to(dir.path(), &journal_with(&["/tmp/b.rs"])).expect("shrink journal");

        let retired = Path::new("/tmp/a.rs");
        assert!(!fs_metadata::exists(&entry_path(dir.path(), retired)));
        assert!(!fs_metadata::exists(&body_path(dir.path(), retired)));
        let retained = Path::new("/tmp/b.rs");
        assert!(fs_metadata::exists(&entry_path(dir.path(), retained)));
        assert!(fs_metadata::exists(&body_path(dir.path(), retained)));
    }

    /// D9 through `shrink_step`: the entry goes before its body, so when a
    /// retired entry cannot be removed its body stays, and no entry is left
    /// referencing a missing body, not even through the orphan-body cleanup
    /// that follows.
    #[test]
    fn shrink_keeps_the_body_of_a_retired_entry_it_cannot_remove() {
        let dir = TempDir::new().expect("expected operation to succeed");
        save(dir.path(), &journal_with(&["/tmp/a.rs", "/tmp/b.rs"])).expect("save journal");
        let retired = Path::new("/tmp/a.rs");
        let entry = entry_path(dir.path(), retired);
        // A non-empty directory where the entry file was: unlinking it fails.
        fs_mutate::remove_file_if_exists(&entry).expect("remove the entry file");
        fixture::create_dir_all(&entry);
        fixture::write_text(&entry.join("keep"), "x");

        shrink_journal_to(dir.path(), &journal_with(&["/tmp/b.rs"])).expect("shrink journal");

        assert!(fs_metadata::exists(&entry), "the entry removal failed");
        assert!(
            fs_metadata::exists(&body_path(dir.path(), retired)),
            "a body whose entry could not be removed is kept"
        );
    }

    /// Re-arming needs the journal's manifest: without one it fails and
    /// writes none, and with one it records the new launch and keeps the
    /// manifest's entries.
    #[test]
    fn rearm_journal_refuses_a_journal_without_a_manifest() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let journal = journal_dir(dir.path());
        fixture::create_dir_all(&journal);
        let launch = LaunchId { pid: 7, nonce: 9 };
        assert!(
            rearm_journal(&journal, launch).is_err(),
            "a journal without a manifest is not re-armed"
        );
        assert!(
            !fs_metadata::exists(&journal.join(manifest_file_name())),
            "no manifest is invented"
        );

        save(dir.path(), &journal_with(&["/tmp/a.rs"])).expect("save journal");
        rearm_journal(&journal, launch).expect("re-arm the journal");
        let manifest = load_manifest_value(dir.path(), &journal).expect("manifest");
        assert_eq!(manifest.pid, Some(7));
        assert_eq!(manifest.launch_nonce, Some(9));
        assert_eq!(manifest.entries.len(), 1);
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
        assert!(!fs_metadata::exists(&layout_path(
            dir.path(),
            AppDataLocation::ReplaceJournalDir
        )));
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
            &layout_path(dir.path(), AppDataLocation::ReplaceJournalDir)
                .join(entry_file_name(&path_a))
        ));
        assert!(fs_metadata::exists(
            &layout_path(dir.path(), AppDataLocation::ReplaceJournalDir)
                .join(entry_file_name(&path_b))
        ));
    }

    #[test]
    fn delete_blocks_retired_backup_cleanup_when_preservation_is_unsafe() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&layout_path(
            dir.path(),
            AppDataLocation::RetiredReplaceBackup,
        ));

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
        let corrupt_entry = layout_path(dir.path(), AppDataLocation::ReplaceJournalDir)
            .join(entry_file_name(&path_a));
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
        let legacy = layout_path(dir.path(), AppDataLocation::RetiredReplaceBackup);
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
        fixture::create_dir(&layout_path(
            dir.path(),
            AppDataLocation::RetiredReplaceBackup,
        ));

        let report = cleanup_stale(dir.path());

        assert_eq!(report.diagnostics.len(), 1);
        assert!(matches!(
            report.diagnostics[0].problem,
            RecoveryProblem::RepairSkipped { .. }
        ));
        assert!(fs_metadata::exists(&layout_path(
            dir.path(),
            AppDataLocation::RetiredReplaceBackup
        )));
    }

    #[test]
    fn unsupported_journal_path_is_inactive() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &layout_path(dir.path(), AppDataLocation::ReplaceJournalDir),
            "not a directory",
        );

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
        // Only a legacy inline journal carries a payload the loader charges;
        // a streamed entry's original stays in its body file.
        super::fixture::write_legacy_journal(
            dir.path(),
            &[(&path, b"before-content", b"after-content")],
            false,
        )
        .expect("save valid legacy journal");

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
        let exact_payload = entry.original.len + entry.replaced.len;
        super::fixture::write_legacy_journal(dir.path(), &[(&path, b"before", b"after")], false)
            .expect("save valid legacy journal");
        backup.insert(path, entry);

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
        let manifest_path =
            layout_path(dir.path(), AppDataLocation::ReplaceJournalDir).join(manifest_file_name());
        let entries = (0..=JOURNAL_SCAN_MAX_ENTRIES)
            .map(|index| ReplaceJournalManifestEntry {
                path: PathBuf::from(format!("/tmp/file-{index}.rs")),
                entry_file: format!("{index}.json"),
            })
            .collect();
        let manifest = ReplaceJournalManifest {
            incremental: false,
            entries,
            pid: None,
            launch_nonce: None,
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
        let manifest_path =
            layout_path(dir.path(), AppDataLocation::ReplaceJournalDir).join(manifest_file_name());
        let entries = (0..JOURNAL_SCAN_MAX_ENTRIES)
            .map(|index| ReplaceJournalManifestEntry {
                path: PathBuf::from(format!("/tmp/file-{index}.rs")),
                entry_file: "shared-entry.json".to_string(),
            })
            .collect();
        let manifest = ReplaceJournalManifest {
            incremental: false,
            entries,
            pid: None,
            launch_nonce: None,
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
        let manifest_path =
            layout_path(dir.path(), AppDataLocation::ReplaceJournalDir).join(manifest_file_name());
        let manifest = ReplaceJournalManifest {
            incremental: false,
            entries: Vec::new(),
            pid: None,
            launch_nonce: None,
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
            &layout_path(dir.path(), AppDataLocation::ReplaceJournalDir).join(manifest_file_name())
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
        let journal_dir = layout_path(dir.path(), AppDataLocation::ReplaceJournalDir);
        fixture::write_text(&journal_dir.join(manifest_file_name()), "{}");
        fixture::write_text(&journal_dir.join(cleanup_marker_file_name()), "{}");
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
        let entry_a_path = layout_path(dir.path(), AppDataLocation::ReplaceJournalDir)
            .join(entry_file_name(&path_a));
        let inode_before = fs_metadata::inode(&entry_a_path).expect("stat first entry before");

        save_entry(dir.path(), &path_b, &entry_b).expect("save second journal entry");

        let inode_after = fs_metadata::inode(&entry_a_path).expect("stat first entry after");
        assert_eq!(
            inode_after, inode_before,
            "saving a new file's journal entry must not rewrite older entries"
        );
        assert!(
            fs_metadata::exists(
                &layout_path(dir.path(), AppDataLocation::ReplaceJournalDir)
                    .join(entry_file_name(&path_b))
            ),
            "the second per-file journal entry should be created independently"
        );
    }

    /// Design D3a: a journal disk job a newer one superseded returns before it
    /// waits for the guard, and one superseded while waiting returns too, so
    /// neither holds a worker slot behind a long Replace All run.
    #[test]
    fn a_superseded_journal_disk_job_returns_before_it_waits() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicU32;
        use std::sync::mpsc;
        use std::time::Duration;

        let held = acquire_journal_guard().expect("a run holds the journal guard");
        let generation = Arc::new(AtomicU32::new(2));
        let (tx, rx) = mpsc::channel();
        let stale = generation.clone();
        let worker = std::thread::spawn(move || {
            let acquired =
                acquire_journal_guard_while_current(|| stale.load(Ordering::Acquire) == 1)
                    .expect("not poisoned");
            tx.send(acquired.is_none()).expect("send");
        });
        assert!(
            rx.recv_timeout(Duration::from_secs(5))
                .expect("a superseded job returns while the guard is held"),
            "a superseded job never takes the guard"
        );
        worker.join().expect("worker joins");

        let (tx, rx) = mpsc::channel();
        let waiting = generation.clone();
        let worker = std::thread::spawn(move || {
            let acquired =
                acquire_journal_guard_while_current(|| waiting.load(Ordering::Acquire) == 2)
                    .expect("not poisoned");
            tx.send(acquired.is_none()).expect("send");
        });
        assert!(
            rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "a current job waits for the guard"
        );
        generation.store(3, Ordering::Release);
        assert!(
            rx.recv_timeout(Duration::from_secs(5))
                .expect("superseded while waiting, the job returns"),
        );
        worker.join().expect("worker joins");
        drop(held);
    }
}

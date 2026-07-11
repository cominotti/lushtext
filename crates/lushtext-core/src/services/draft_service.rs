// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft persistence, startup recovery, and conservative orphan cleanup.
//!
//! Filesystem-facing operations perform blocking I/O and must run through
//! `spawn_blocking_then`. Cleanup evidence types and merge helpers are GTK-free;
//! inspection reads persisted state but mutates nothing. Draft bodies are plain
//! UTF-8 files beside a JSON manifest under `$XDG_DATA_HOME/lushtext/drafts/`.

use crate::model::draft::{
    DraftEntry, DraftManifest, FileDraftRestoreResolution, PreloadedDraftRestore,
};
use crate::model::session::SessionData;
use crate::model::sidecar_identity::stable_path_hash;
use crate::services::json_format::KIND_DRAFT_MANIFEST;
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryLoad, RecoveryLoadConfig, RecoveryLoadOutcome,
    RecoveryMetadataClass, RecoveryRepair, RecoveryRepairContext, load_enveloped_json_or_default,
    load_enveloped_json_with_repair, save_enveloped_json_path,
};
use crate::services::{
    editor_io,
    filesystem::{
        DirectoryScanPolicy, FileKind, MutationOutcome, PathStatus, WriteLabel,
        metadata as fs_metadata, mutate as fs_mutate, read as fs_read, tree as fs_tree,
        write as fs_write,
    },
    session_service,
};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Groups manifest and bodies in one app-owned directory for bounded cleanup scans.
const DRAFTS_DIR: &str = "drafts";
/// Envelope-backed manifest filename stored beside draft bodies.
const MANIFEST_FILE: &str = "manifest.json";
/// Largest UTF-8 draft body that LushText can automatically write and restore.
///
/// Sixty-four MiB keeps one recovery body bounded on ordinary desktop systems.
/// Capture and read paths must share this policy so the app never promises a
/// draft that startup would later refuse to restore.
pub const MAX_AUTOMATIC_DRAFT_BYTES: u64 = 64 * 1024 * 1024;
/// Total draft-body bytes admitted eagerly during one startup restore.
///
/// This separate sixty-four MiB budget protects startup peak memory. Drafts
/// skipped only by this aggregate cap remain eligible for serialized lazy reads.
pub const MAX_EAGER_DRAFT_PRELOAD_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum draft files inspected while rebuilding a missing or corrupt manifest.
///
/// Repair runs during startup restore, so it must be bounded. The cap is large
/// enough for ordinary crash recovery while preventing one damaged data
/// directory from monopolizing the background restore task.
pub const MAX_MANIFEST_REPAIR_DRAFT_SCAN: usize = 2048;
/// Maximum draft files inspected while cleaning orphan draft bodies after restore.
///
/// Matches the 2,048-entry repair bound so deferred cleanup uses the same
/// bounded metadata and allocation budget.
pub const MAX_ORPHAN_CLEANUP_DRAFT_SCAN: usize = 2048;

mod cleanup_types;
#[cfg(test)]
use cleanup_types::saturating_confirmed_cleanup_count;
pub use cleanup_types::*;

/// Deterministic cleanup failure selected by property tests.
///
/// This seam is feature-gated so production callers cannot bypass real
/// filesystem behavior, while generated tests remain independent of process
/// privileges and platform permission semantics.
#[cfg(feature = "property-tests")]
#[derive(Clone, Copy, Debug)]
pub enum DraftOrphanCleanupFault {
    /// Fail an orphan-body deletion after its identity is revalidated.
    Delete,
    /// Fail the durable manifest commit after removals are selected.
    Manifest,
}

#[derive(Clone, Copy, Debug, Default)]
struct DraftOrphanCleanupFaults {
    fail_delete: bool,
    fail_manifest: bool,
}

/// Draft, session, and diagnostics loaded for startup restore.
#[derive(Debug)]
pub struct RestoreState {
    /// Manifest snapshot used by the window after startup.
    pub manifest: DraftManifest,
    /// Session snapshot used to recreate tabs.
    pub session: SessionData,
    /// Draft bodies or skip markers preloaded for restored tabs.
    pub preloaded_drafts: HashMap<String, PreloadedDraftRestore>,
    /// Recovery diagnostics that should be logged or surfaced after restore.
    pub diagnostics: Vec<RecoveryDiagnostic>,
    /// Whether it is safe to run orphan cleanup after this startup.
    pub orphan_cleanup_allowed: bool,
}

/// Typed failures from reading one draft body.
#[derive(Debug, thiserror::Error)]
pub enum DraftReadError {
    /// The draft exists but is above the startup/preload restore budget.
    #[error("draft {path} is too large to restore automatically ({size} bytes, limit {max} bytes)")]
    Oversized { path: PathBuf, size: u64, max: u64 },
    /// The draft exists but cannot be read or decoded as UTF-8.
    #[error("failed to read draft {path}: {detail}")]
    Read { path: PathBuf, detail: String },
}

/// Admission result under the per-draft and aggregate startup byte budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DraftPreloadDecision {
    /// Read now and charge the body's size to the eager budget.
    Read,
    /// Preserve on disk because the body exceeds automatic recovery policy.
    SkipOversized,
    /// Defer because only the aggregate eager budget is exhausted.
    SkipBudget,
}

fn manifest_write_lock() -> &'static Mutex<()> {
    // Process-local mutex serializes manifest read-modify-write sequences;
    // `OnceLock` creates it lazily without global constructor order.
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Returns the drafts directory: `{data_dir}/drafts/`.
#[must_use]
pub fn drafts_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DRAFTS_DIR)
}

fn manifest_path(data_dir: &Path) -> PathBuf {
    drafts_dir(data_dir).join(MANIFEST_FILE)
}

/// Generate a stable draft ID from an absolute file path.
///
/// Draft IDs are persisted and may be derived again during session restore, so
/// v1 uses the same explicit FNV-1a helper as sidecar identities instead of
/// Rust's implementation-defined `DefaultHasher`.
#[must_use]
pub fn draft_id_for_path(path: &Path) -> String {
    stable_path_hash(path)
}

/// Generate a unique draft ID for an untitled tab using a monotonic
/// counter value. Each untitled tab gets a different ID.
#[must_use]
pub fn draft_id_for_untitled(counter: u64) -> String {
    format!("untitled-{counter:016x}")
}

/// Load the draft manifest from disk. Returns an empty manifest if
/// the file doesn't exist.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if the manifest file exists but cannot be read or parsed.
pub fn load_manifest(data_dir: &Path) -> Result<DraftManifest> {
    Ok(load_manifest_recovering(data_dir).value)
}

/// Load the draft manifest through the public v1 envelope contract.
#[must_use]
pub fn load_manifest_recovering(data_dir: &Path) -> RecoveryLoad<DraftManifest> {
    let path = manifest_path(data_dir);
    load_enveloped_json_or_default(
        &RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::DraftManifest),
        KIND_DRAFT_MANIFEST,
    )
}

/// Save the draft manifest atomically (temp file + rename).
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if the manifest cannot be serialized or written.
///
/// # Panics
///
/// Panics if the process-wide manifest write lock is poisoned by an earlier
/// panic while the lock was held.
pub fn save_manifest(data_dir: &Path, manifest: &DraftManifest) -> Result<()> {
    let _guard = manifest_write_lock()
        .lock()
        .expect("draft manifest write lock poisoned");
    save_manifest_locked(data_dir, manifest)
}

/// Persist while the caller holds the process-wide manifest write lock.
fn save_manifest_locked(data_dir: &Path, manifest: &DraftManifest) -> Result<()> {
    let path = manifest_path(data_dir);
    let config = RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::DraftManifest);
    let diagnostics = save_enveloped_json_path(&config, KIND_DRAFT_MANIFEST, manifest)?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
}

/// Load, mutate, and save the draft manifest under a single lock.
/// Returns the final manifest snapshot written to disk.
///
/// # Errors
///
/// Returns an error if the manifest cannot be read or the updated manifest
/// cannot be written back to disk.
///
/// # Panics
///
/// Panics if the process-wide manifest write lock is poisoned by an earlier
/// panic while the lock was held.
pub fn update_manifest<F>(data_dir: &Path, update: F) -> Result<DraftManifest>
where
    F: FnOnce(&mut DraftManifest),
{
    let _guard = manifest_write_lock()
        .lock()
        .expect("draft manifest write lock poisoned");
    let mut manifest = load_manifest(data_dir)?;
    update(&mut manifest);
    save_manifest_locked(data_dir, &manifest)?;
    Ok(manifest)
}

/// Load the manifest, session, and any draft content needed for startup restore.
///
/// This intentionally preserves file-backed session tabs even when their paths
/// are temporarily unavailable, so startup does not turn a transient mount
/// outage into permanent session loss on the next save.
#[must_use]
pub fn load_restore_state(data_dir: &Path) -> RestoreState {
    load_restore_state_with_eager_limit(data_dir, MAX_EAGER_DRAFT_PRELOAD_BYTES)
}

/// Build startup state while charging bodies against a caller-supplied eager budget.
fn load_restore_state_with_eager_limit(data_dir: &Path, eager_limit: u64) -> RestoreState {
    let session_load = session_service::load_recovering(data_dir);
    let session = session_load.value;
    let mut diagnostics = session_load.diagnostics;

    let manifest_load = load_manifest_for_restore(data_dir, &session);
    let mut manifest = manifest_load.value;
    let orphan_cleanup_allowed = manifest_load.outcome == RecoveryLoadOutcome::Loaded
        || (manifest_load.outcome == RecoveryLoadOutcome::MissingDefault
            && manifest_load.diagnostics.is_empty());
    diagnostics.extend(manifest_load.diagnostics);

    let mut preloaded = HashMap::new();
    let mut stale_draft_ids = Vec::new();
    let mut preloaded_bytes = 0u64;
    for tab in &session.tabs {
        let draft_id = match &tab.path {
            Some(path) => draft_id_for_path(path),
            None => match &tab.draft_id {
                Some(id) => id.clone(),
                None => continue,
            },
        };
        let Some(entry) = manifest.find_by_id(&draft_id).cloned() else {
            continue;
        };
        if entry.original_path.is_some() {
            match draft_preload_decision(data_dir, &draft_id, &mut preloaded_bytes, eager_limit) {
                DraftPreloadDecision::Read => {}
                DraftPreloadDecision::SkipOversized => {
                    tracing::warn!("Skipped automatic restore for oversized draft {draft_id}");
                    preloaded.insert(draft_id, PreloadedDraftRestore::SkipOversized);
                    continue;
                }
                DraftPreloadDecision::SkipBudget => {
                    tracing::warn!("Skipped eager preload for large draft {draft_id}");
                    preloaded.insert(draft_id, PreloadedDraftRestore::LazyAggregateBudget);
                    continue;
                }
            }
            match resolve_file_draft_restore(data_dir, &entry) {
                Ok(FileDraftRestoreResolution::Restore { content }) => {
                    preloaded.insert(draft_id, PreloadedDraftRestore::Content(content));
                }
                Ok(FileDraftRestoreResolution::SkipStale) => {
                    preloaded.insert(draft_id.clone(), PreloadedDraftRestore::SkipStaleFile);
                    if !stale_draft_ids.contains(&draft_id) {
                        stale_draft_ids.push(draft_id);
                    }
                }
                Ok(FileDraftRestoreResolution::SkipOversized) => {
                    preloaded.insert(draft_id, PreloadedDraftRestore::SkipOversized);
                }
                Ok(
                    FileDraftRestoreResolution::SkipUnavailable
                    | FileDraftRestoreResolution::MissingDraft,
                ) => {}
                Err(e) => {
                    tracing::warn!("Failed to pre-resolve draft {draft_id}: {e}");
                }
            }
            continue;
        }

        match draft_preload_decision(data_dir, &draft_id, &mut preloaded_bytes, eager_limit) {
            DraftPreloadDecision::Read => {}
            DraftPreloadDecision::SkipOversized => {
                tracing::warn!("Skipped automatic restore for oversized draft {draft_id}");
                preloaded.insert(draft_id, PreloadedDraftRestore::SkipOversized);
                continue;
            }
            DraftPreloadDecision::SkipBudget => {
                tracing::warn!("Skipped eager preload for large draft {draft_id}");
                preloaded.insert(draft_id, PreloadedDraftRestore::LazyAggregateBudget);
                continue;
            }
        }
        match read_draft(data_dir, &draft_id) {
            Ok(Some(content)) => {
                preloaded.insert(draft_id, PreloadedDraftRestore::Content(content));
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Failed to pre-read draft {draft_id}: {e}");
            }
        }
    }

    cleanup_stale_restore_entries(data_dir, &mut manifest, &stale_draft_ids);
    RestoreState {
        manifest,
        session,
        preloaded_drafts: preloaded,
        diagnostics,
        orphan_cleanup_allowed,
    }
}

/// Load the manifest through recovery repair rules used only by startup restore.
fn load_manifest_for_restore(
    data_dir: &Path,
    session: &SessionData,
) -> RecoveryLoad<DraftManifest> {
    let path = manifest_path(data_dir);
    let config = RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::DraftManifest);
    let mut load = load_enveloped_json_with_repair(&config, KIND_DRAFT_MANIFEST, |context| {
        repair_manifest_from_draft_files(data_dir, session, &context)
    });

    if load.outcome == RecoveryLoadOutcome::MissingDefault {
        let missing_problem = crate::services::recovery_metadata::RecoveryProblem::RepairSkipped {
            detail: "manifest is missing".to_string(),
        };
        let missing_preservation =
            crate::services::recovery_metadata::RecoveryPreservation::NotNeeded;
        let missing_context = RecoveryRepairContext {
            class: RecoveryMetadataClass::DraftManifest,
            path: &path,
            bytes: None,
            problem: &missing_problem,
            preservation: &missing_preservation,
        };
        let repair = repair_manifest_from_draft_files(data_dir, session, &missing_context);
        apply_manifest_repair_to_missing_load(&mut load, repair);
    }

    if load.outcome == RecoveryLoadOutcome::Partial
        && load.replacement_allowed()
        && let Err(error) = save_manifest(data_dir, &load.value)
    {
        load.diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::DraftManifest,
            &path,
            format!("failed to write repaired manifest: {error}"),
        ));
    }

    load
}

/// Rebuild only draft manifest entries that can be proven safe from surviving draft files.
///
/// Untitled draft IDs are recoverable from session state or their `untitled-`
/// prefix. File-backed hash IDs are preserved but not trusted because the
/// original path was lost with the manifest.
fn repair_manifest_from_draft_files(
    data_dir: &Path,
    session: &SessionData,
    context: &RecoveryRepairContext<'_>,
) -> RecoveryRepair<DraftManifest> {
    let draft_ids = match recoverable_draft_file_ids(data_dir) {
        Ok(ids) => ids,
        Err(error) => {
            return RecoveryRepair::Skipped {
                diagnostics: vec![RecoveryDiagnostic::repair_skipped(
                    context.class,
                    context.path,
                    format!("could not scan draft files for repair: {error}"),
                )],
            };
        }
    };

    if draft_ids.is_empty() {
        return RecoveryRepair::Unavailable;
    }

    let session_untitled_ids = session_untitled_draft_ids(session);
    let mut manifest = DraftManifest::default();
    let mut skipped = 0usize;
    for draft_id in draft_ids {
        if session_untitled_ids.contains(&draft_id) || draft_id.starts_with("untitled-") {
            manifest.upsert(DraftEntry {
                draft_id,
                original_path: None,
                original_mtime_secs: None,
                saved_at_secs: editor_io::now_epoch_secs(),
            });
        } else {
            skipped += 1;
        }
    }

    let mut diagnostics = Vec::new();
    if !manifest.drafts.is_empty() {
        diagnostics.push(RecoveryDiagnostic::repaired(
            context.class,
            context.path,
            format!(
                "rebuilt {} untitled draft manifest entries from surviving draft files",
                manifest.drafts.len()
            ),
        ));
    }
    if skipped > 0 {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            context.class,
            context.path,
            format!(
                "preserved {skipped} draft files whose original paths could not be proven safely"
            ),
        ));
    }

    if manifest.drafts.is_empty() {
        RecoveryRepair::Skipped { diagnostics }
    } else {
        RecoveryRepair::Repaired {
            value: manifest,
            diagnostics,
        }
    }
}

/// Return draft IDs from a bounded scan that could participate in manifest repair.
fn recoverable_draft_file_ids(data_dir: &Path) -> Result<Vec<String>> {
    let dir = drafts_dir(data_dir);
    match fs_metadata::path_status(&dir) {
        Ok(crate::services::filesystem::PathStatus::Missing) => return Ok(Vec::new()),
        Ok(crate::services::filesystem::PathStatus::Directory) => {}
        Ok(status) => {
            return Err(anyhow::anyhow!(
                "drafts path is not a directory during repair: {status:?}"
            ));
        }
        Err(error) => {
            return Err(anyhow::anyhow!(
                "failed to inspect drafts directory {}: {error}",
                dir.display()
            ));
        }
    }

    let entries = fs_tree::scan_directory(
        &dir,
        DirectoryScanPolicy {
            max_entries: MAX_MANIFEST_REPAIR_DRAFT_SCAN,
            include_hidden: false,
        },
    )?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| draft_id_from_draft_file_name(&entry.file_name))
        .collect())
}

fn session_untitled_draft_ids(session: &SessionData) -> HashSet<String> {
    session
        .tabs
        .iter()
        .filter(|tab| tab.path.is_none())
        .filter_map(|tab| tab.draft_id.clone())
        .collect()
}

fn draft_id_from_draft_file_name(name: &str) -> Option<String> {
    if name.starts_with('.') {
        return None;
    }
    let name_path = Path::new(name);
    if !name_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("draft"))
    {
        return None;
    }
    name_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .map(ToOwned::to_owned)
}

/// Apply synthetic repair diagnostics for the missing-manifest path.
///
/// Missing files do not pass through the malformed-file recovery hook, so startup
/// creates an equivalent context before trusting any rebuilt manifest entries.
fn apply_manifest_repair_to_missing_load(
    load: &mut RecoveryLoad<DraftManifest>,
    repair: RecoveryRepair<DraftManifest>,
) {
    match repair {
        RecoveryRepair::Unavailable => {}
        RecoveryRepair::Skipped { mut diagnostics } => {
            load.diagnostics.append(&mut diagnostics);
        }
        RecoveryRepair::Repaired {
            value,
            mut diagnostics,
        } => {
            load.value = value;
            load.outcome = RecoveryLoadOutcome::Partial;
            load.diagnostics.append(&mut diagnostics);
        }
    }
}

/// Classify one body and charge its size only after eager admission succeeds.
fn draft_preload_decision(
    data_dir: &Path,
    draft_id: &str,
    preloaded_bytes: &mut u64,
    eager_limit: u64,
) -> DraftPreloadDecision {
    let path = drafts_dir(data_dir).join(format!("{draft_id}.draft"));
    // Metadata is an eager-admission hint. Probe failures fall through so the
    // bounded reader can still recover a body while enforcing the hard cap.
    let Ok(facts) = fs_metadata::file_facts(&path) else {
        return DraftPreloadDecision::Read;
    };
    let size = facts.byte_size;
    if size > MAX_AUTOMATIC_DRAFT_BYTES {
        return DraftPreloadDecision::SkipOversized;
    }
    if preloaded_bytes.saturating_add(size) > eager_limit {
        return DraftPreloadDecision::SkipBudget;
    }
    *preloaded_bytes = preloaded_bytes.saturating_add(size);
    DraftPreloadDecision::Read
}

fn oversized_draft_size(data_dir: &Path, draft_id: &str) -> Option<u64> {
    let path = drafts_dir(data_dir).join(format!("{draft_id}.draft"));
    fs_metadata::file_facts(&path)
        .ok()
        .map(|facts| facts.byte_size)
        .filter(|size| *size > MAX_AUTOMATIC_DRAFT_BYTES)
}

/// Resolve whether a file-backed draft is still safe to restore.
///
/// This helper keeps blocking metadata checks and draft-file reads inside the
/// service layer so both startup preload and later `check_draft_on_open()`
/// calls can share one decision path.
///
/// # Errors
///
/// Returns an error if the draft file exists but cannot be read as UTF-8 text.
pub fn resolve_file_draft_restore(
    data_dir: &Path,
    entry: &DraftEntry,
) -> Result<FileDraftRestoreResolution> {
    let Some(path) = entry.original_path.as_deref() else {
        return Ok(FileDraftRestoreResolution::SkipUnavailable);
    };

    if let Some(saved_mtime) = entry.original_mtime_secs {
        let Some(current_mtime) = editor_io::mtime_secs(path) else {
            return Ok(FileDraftRestoreResolution::SkipUnavailable);
        };
        if current_mtime != saved_mtime {
            return Ok(FileDraftRestoreResolution::SkipStale);
        }
    }

    if oversized_draft_size(data_dir, &entry.draft_id).is_some() {
        return Ok(FileDraftRestoreResolution::SkipOversized);
    }

    match read_draft(data_dir, &entry.draft_id)? {
        Some(content) => Ok(FileDraftRestoreResolution::Restore { content }),
        None => Ok(FileDraftRestoreResolution::MissingDraft),
    }
}

/// Resolve any manifest entry through the same bounded automatic-read policy.
///
/// File-backed entries retain mtime conflict checks; untitled entries need only
/// the shared size bound and durable body read. This is blocking service I/O.
///
/// # Errors
///
/// Returns an error when an eligible body cannot be read as bounded UTF-8 text.
pub fn resolve_draft_restore(
    data_dir: &Path,
    entry: &DraftEntry,
) -> Result<FileDraftRestoreResolution> {
    if entry.original_path.is_some() {
        return resolve_file_draft_restore(data_dir, entry);
    }
    if oversized_draft_size(data_dir, &entry.draft_id).is_some() {
        return Ok(FileDraftRestoreResolution::SkipOversized);
    }
    match read_draft(data_dir, &entry.draft_id)? {
        Some(content) => Ok(FileDraftRestoreResolution::Restore { content }),
        None => Ok(FileDraftRestoreResolution::MissingDraft),
    }
}

/// Delete stale draft files and remove their manifest entries after a confirmed
/// backing-file mismatch.
fn cleanup_stale_restore_entries(
    data_dir: &Path,
    manifest: &mut DraftManifest,
    stale_draft_ids: &[String],
) {
    if stale_draft_ids.is_empty() {
        return;
    }

    for draft_id in stale_draft_ids {
        if let Err(e) = delete_draft_file(data_dir, draft_id) {
            tracing::warn!("Failed to delete stale draft {draft_id}: {e}");
        }
    }

    match update_manifest(data_dir, |manifest| {
        for draft_id in stale_draft_ids {
            manifest.remove_by_id(draft_id);
        }
    }) {
        Ok(updated_manifest) => {
            *manifest = updated_manifest;
        }
        Err(e) => {
            tracing::warn!("Failed to persist stale draft cleanup: {e}");
            manifest
                .drafts
                .retain(|entry| !stale_draft_ids.iter().any(|id| id == &entry.draft_id));
        }
    }
}

/// Write a single draft file atomically (temp + rename). The draft
/// content is plain UTF-8 text, not wrapped in JSON.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if the drafts directory cannot be created or the draft file
/// cannot be written, flushed, synced, or renamed into place.
pub fn write_draft(data_dir: &Path, draft_id: &str, content: &str) -> Result<()> {
    let dir = drafts_dir(data_dir);
    fs_write::create_dir_all_durable(&dir)
        .with_context(|| format!("failed to create drafts dir: {}", dir.display()))?;

    let path = dir.join(format!("{draft_id}.draft"));
    // The shared helper owns the temp-file-then-rename ordering, the full fsync
    // contract, and identity-metadata preservation when overwriting an existing
    // draft file.
    fs_write::atomic_replace(&path, WriteLabel::DRAFT, content.as_bytes())
        .with_context(|| format!("failed to write draft: {}", path.display()))
}

/// Read a draft file's content. Returns `None` if the draft file
/// doesn't exist.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if an existing draft file cannot be read as UTF-8 text.
pub fn read_draft(data_dir: &Path, draft_id: &str) -> Result<Option<String>> {
    let path = drafts_dir(data_dir).join(format!("{draft_id}.draft"));
    if let Some(size) = oversized_draft_size(data_dir, draft_id) {
        return Err(DraftReadError::Oversized {
            path,
            size,
            max: MAX_AUTOMATIC_DRAFT_BYTES,
        }
        .into());
    }
    read_draft_path_bounded(&path, MAX_AUTOMATIC_DRAFT_BYTES)
}

/// Read one body with an allocation bound that remains valid after metadata races.
fn read_draft_path_bounded(path: &Path, max_bytes: u64) -> Result<Option<String>> {
    // One sentinel byte detects growth after the metadata probe without ever
    // allocating an unbounded body.
    let read_limit = usize::try_from(max_bytes)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    match fs_read::prefix_bytes(path, read_limit) {
        Ok(bytes) => {
            let observed_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            if observed_size > max_bytes {
                return Err(DraftReadError::Oversized {
                    path: path.to_path_buf(),
                    size: observed_size,
                    max: max_bytes,
                }
                .into());
            }
            let content = simdutf8::basic::from_utf8(&bytes)
                .map_err(|error| DraftReadError::Read {
                    path: path.to_path_buf(),
                    detail: error.to_string(),
                })?
                .to_string();
            Ok(Some(content))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(DraftReadError::Read {
            path: path.to_path_buf(),
            detail: e.to_string(),
        }
        .into()),
    }
}

/// Delete a single draft file from disk. No-op if the file doesn't exist.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if an existing draft file cannot be deleted.
pub fn delete_draft_file(data_dir: &Path, draft_id: &str) -> Result<()> {
    let path = drafts_dir(data_dir).join(format!("{draft_id}.draft"));
    match fs_mutate::remove_file_if_exists(&path) {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete draft {}: {}",
            path.display(),
            e
        )),
    }
}

/// Inspect one bounded set of draft artifacts without mutating filesystem state.
///
/// **Threading:** performs blocking metadata and directory I/O, so callers must
/// run it on a background thread. A directory-level failure returns no plan;
/// per-entry status failures stay inside a non-destructive plan so unaffected
/// recovery evidence remains visible.
///
/// # Errors
///
/// Returns a typed scan error when the drafts directory cannot be identified or
/// traversed consistently. The caller must not execute cleanup after this error.
pub fn inspect_orphan_cleanup(
    data_dir: &Path,
    manifest: &DraftManifest,
) -> std::result::Result<DraftOrphanCleanupPlan, DraftOrphanCleanupScanError> {
    inspect_orphan_cleanup_from(data_dir, manifest, 0)
}

/// Inspect one bounded manifest page and one bounded draft-directory scan.
///
/// `manifest_offset` advances through the supplied manifest snapshot without
/// allocating evidence for every accepted entry at once.
///
/// **Threading:** performs blocking metadata and directory I/O; call from a
/// background thread. Inspection is side-effect free.
///
/// # Errors
///
/// Returns the same directory-level failures as [`inspect_orphan_cleanup`].
pub fn inspect_orphan_cleanup_from(
    data_dir: &Path,
    manifest: &DraftManifest,
    manifest_offset: usize,
) -> std::result::Result<DraftOrphanCleanupPlan, DraftOrphanCleanupScanError> {
    let dir = drafts_dir(data_dir);
    let entries = match fs_metadata::path_status(&dir) {
        Ok(PathStatus::Missing) => Vec::new(),
        Ok(PathStatus::Directory) => fs_tree::scan_directory(
            &dir,
            DirectoryScanPolicy {
                max_entries: MAX_ORPHAN_CLEANUP_DRAFT_SCAN,
                include_hidden: false,
            },
        )
        .map_err(|error| DraftOrphanCleanupScanError::Read {
            path: dir.clone(),
            detail: error.to_string(),
        })?,
        Ok(status) => {
            return Err(DraftOrphanCleanupScanError::NotDirectory { path: dir, status });
        }
        Err(error) => {
            return Err(DraftOrphanCleanupStatusError {
                path: dir,
                detail: error.to_string(),
            }
            .into());
        }
    };

    let manifest_start = manifest_offset.min(manifest.drafts.len());
    let manifest_page_len = manifest
        .drafts
        .len()
        .saturating_sub(manifest_start)
        .min(MAX_ORPHAN_CLEANUP_DRAFT_SCAN);
    let manifest_entries = manifest
        .drafts
        .iter()
        .skip(manifest_start)
        .take(manifest_page_len)
        .collect::<Vec<_>>();
    let next_manifest_offset = manifest_start.saturating_add(manifest_page_len);
    let manifest_has_more = next_manifest_offset < manifest.drafts.len();
    let mut plan = DraftOrphanCleanupPlan {
        // A full-cap result cannot prove that the directory ended at the bound,
        // so it conservatively schedules a later normal deferred opportunity.
        has_more_work: entries.len() == MAX_ORPHAN_CLEANUP_DRAFT_SCAN || manifest_has_more,
        next_manifest_offset: if manifest_has_more {
            Some(next_manifest_offset)
        } else {
            None
        },
        ..DraftOrphanCleanupPlan::default()
    };
    let mut seen_fingerprints = HashSet::new();
    let mut id_counts = HashMap::<&str, usize>::new();
    for entry in &manifest_entries {
        *id_counts.entry(entry.draft_id.as_str()).or_default() += 1;
    }

    for (draft_id, _) in id_counts.iter().filter(|(_, count)| **count > 1) {
        // Duplicate IDs are ambiguous even when fields match, so preserve every
        // entry instead of selecting one generation for destructive cleanup.
        plan.retained.push(DraftOrphanCleanupRetained {
            draft_id: Some((*draft_id).to_string()),
            path: draft_body_path(data_dir, draft_id),
            reason: DraftOrphanCleanupRetentionReason::DuplicateManifestId,
        });
    }

    for entry in &manifest_entries {
        // Duplicate IDs are ambiguous even when their current persisted fields
        // match, so inspection preserves all generations instead of selecting one.
        if id_counts.get(entry.draft_id.as_str()).copied().unwrap_or(0) > 1 {
            continue;
        }
        let path = dir.join(format!("{}.draft", entry.draft_id));
        match fs_metadata::path_status(&path) {
            Ok(PathStatus::Missing) => {
                let fingerprint = DraftEntryFingerprint::from_entry(entry);
                if seen_fingerprints.insert(fingerprint.clone()) {
                    plan.missing_body_entries.push(fingerprint);
                }
            }
            Ok(PathStatus::File) => {}
            Ok(PathStatus::Directory | PathStatus::Other) => {
                plan.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(entry.draft_id.clone()),
                    path,
                    reason: DraftOrphanCleanupRetentionReason::BodyNotRegularFile,
                });
            }
            Err(error) => {
                plan.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(entry.draft_id.clone()),
                    path: path.clone(),
                    reason: DraftOrphanCleanupRetentionReason::StatusUncertain,
                });
                plan.failures.push(
                    DraftOrphanCleanupStatusError {
                        path,
                        detail: error.to_string(),
                    }
                    .into(),
                );
            }
        }
    }

    // This set covers only the current manifest page, so inspection may
    // nominate IDs found later. Execution rechecks the complete latest manifest.
    let manifest_ids = manifest_entries
        .iter()
        .map(|entry| entry.draft_id.as_str())
        .collect::<HashSet<_>>();

    for entry in entries {
        if entry.file_name == MANIFEST_FILE {
            continue;
        }
        let Some(draft_id) = draft_id_from_draft_file_name(&entry.file_name) else {
            plan.retained.push(DraftOrphanCleanupRetained {
                draft_id: None,
                path: entry.path,
                reason: DraftOrphanCleanupRetentionReason::UnknownFile,
            });
            continue;
        };
        if entry.kind != FileKind::File {
            plan.retained.push(DraftOrphanCleanupRetained {
                draft_id: Some(draft_id),
                path: entry.path,
                reason: DraftOrphanCleanupRetentionReason::BodyNotRegularFile,
            });
            continue;
        }
        if !manifest_ids.contains(draft_id.as_str()) {
            let inode = match fs_metadata::inode(&entry.path) {
                Ok(inode) => inode,
                Err(error) => {
                    plan.retained.push(DraftOrphanCleanupRetained {
                        draft_id: Some(draft_id),
                        path: entry.path.clone(),
                        reason: DraftOrphanCleanupRetentionReason::StatusUncertain,
                    });
                    plan.failures.push(
                        DraftOrphanCleanupStatusError {
                            path: entry.path,
                            detail: error.to_string(),
                        }
                        .into(),
                    );
                    continue;
                }
            };
            plan.orphan_bodies.push(DraftOrphanBodyCandidate {
                draft_id,
                path: entry.path,
                inode,
            });
        }
    }

    Ok(plan)
}

fn draft_body_path(data_dir: &Path, draft_id: &str) -> PathBuf {
    drafts_dir(data_dir).join(format!("{draft_id}.draft"))
}

/// Execute a previously inspected cleanup plan against freshly loaded state.
///
/// **Threading:** performs blocking metadata, deletion, and durable manifest I/O;
/// callers must run it on a background thread. The existing process-wide
/// manifest lock covers latest-state revalidation and the final durable commit,
/// preventing manifest publication from interleaving between those phases.
///
/// # Panics
///
/// Panics if an earlier panic poisoned the process-wide manifest write lock.
#[must_use]
pub fn execute_orphan_cleanup(
    data_dir: &Path,
    plan: DraftOrphanCleanupPlan,
) -> DraftOrphanCleanupOutcome {
    execute_orphan_cleanup_impl(data_dir, plan, DraftOrphanCleanupFaults::default())
}

/// Execute orphan cleanup with one deterministic, property-test-only failure.
///
/// The injected failure follows the same retention and retry reporting path as
/// a real filesystem error, without relying on permissions that privileged CI
/// users may bypass.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn execute_orphan_cleanup_with_fault_for_test(
    data_dir: &Path,
    plan: DraftOrphanCleanupPlan,
    fault: DraftOrphanCleanupFault,
) -> DraftOrphanCleanupOutcome {
    let faults = match fault {
        DraftOrphanCleanupFault::Delete => DraftOrphanCleanupFaults {
            fail_delete: true,
            fail_manifest: false,
        },
        DraftOrphanCleanupFault::Manifest => DraftOrphanCleanupFaults {
            fail_delete: false,
            fail_manifest: true,
        },
    };
    execute_orphan_cleanup_impl(data_dir, plan, faults)
}

/// Shared blocking cleanup implementation used by production and property tests.
///
/// Callers inherit [`execute_orphan_cleanup`]'s background-thread and manifest
/// lock requirements. `faults` is immutable per-invocation test control; the
/// production wrapper always supplies the no-fault default.
fn execute_orphan_cleanup_impl(
    data_dir: &Path,
    plan: DraftOrphanCleanupPlan,
    faults: DraftOrphanCleanupFaults,
) -> DraftOrphanCleanupOutcome {
    let DraftOrphanCleanupPlan {
        missing_body_entries,
        orphan_bodies,
        retained,
        failures,
        has_more_work,
        next_manifest_offset,
    } = plan;
    let retryable_inspection_failures = !failures.is_empty();
    let mut outcome = DraftOrphanCleanupOutcome {
        retained,
        failures,
        has_more_work: has_more_work || retryable_inspection_failures,
        next_manifest_offset,
        ..DraftOrphanCleanupOutcome::default()
    };

    let _guard = manifest_write_lock()
        .lock()
        .expect("draft manifest write lock poisoned");
    let mut latest = match load_trusted_manifest_for_cleanup(data_dir) {
        Ok(manifest) => manifest,
        Err(error) => {
            retain_unexecuted_plan(
                data_dir,
                &mut outcome,
                &missing_body_entries,
                &orphan_bodies,
                DraftOrphanCleanupRetentionReason::StatusUncertain,
            );
            outcome.failures.push(error.into());
            outcome.has_more_work = true;
            return outcome;
        }
    };
    // `Some` means the ID is unique; `None` marks ambiguous duplicates that
    // destructive cleanup must preserve.
    let mut latest_by_id = HashMap::<&str, Option<&DraftEntry>>::new();
    for entry in &latest.drafts {
        latest_by_id
            .entry(entry.draft_id.as_str())
            .and_modify(|value| *value = None)
            .or_insert(Some(entry));
    }
    for candidate in orphan_bodies {
        let expected_path = draft_body_path(data_dir, &candidate.draft_id);
        if candidate.path != expected_path {
            outcome.retained.push(DraftOrphanCleanupRetained {
                draft_id: Some(candidate.draft_id),
                path: candidate.path,
                reason: DraftOrphanCleanupRetentionReason::CandidatePathMismatch,
            });
            outcome.has_more_work = true;
            continue;
        }
        if latest_by_id.contains_key(candidate.draft_id.as_str()) {
            outcome.retained.push(DraftOrphanCleanupRetained {
                draft_id: Some(candidate.draft_id),
                path: candidate.path,
                reason: DraftOrphanCleanupRetentionReason::ManifestEntryPresent,
            });
            continue;
        }

        // The stable target guard closes the gap between identity validation
        // and deletion. Atomic draft replacement uses the same guard, so either
        // cleanup observes the newer inode or the newer write happens afterward.
        let _body_guard = match fs_write::TargetWriteGuard::acquire(&candidate.path) {
            Ok(guard) => guard,
            Err(error) => {
                outcome.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(candidate.draft_id),
                    path: candidate.path.clone(),
                    reason: DraftOrphanCleanupRetentionReason::StatusUncertain,
                });
                outcome.failures.push(
                    DraftOrphanCleanupStatusError {
                        path: candidate.path,
                        detail: error.to_string(),
                    }
                    .into(),
                );
                outcome.has_more_work = true;
                continue;
            }
        };
        match fs_metadata::path_status(&candidate.path) {
            Ok(PathStatus::Missing) => outcome.already_absent_files.push(candidate.path),
            Ok(PathStatus::File)
                if fs_metadata::inode(&candidate.path).ok() != Some(candidate.inode) =>
            {
                outcome.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(candidate.draft_id),
                    path: candidate.path,
                    reason: DraftOrphanCleanupRetentionReason::BodyGenerationChanged,
                });
            }
            Ok(PathStatus::File) => match if faults.fail_delete {
                Err(std::io::Error::other(
                    "injected orphan cleanup deletion failure",
                ))
            } else {
                fs_mutate::remove_file_if_exists(&candidate.path)
            } {
                Ok(MutationOutcome::Changed) => outcome.deleted_files.push(candidate.path),
                Ok(MutationOutcome::AlreadyAbsent) => {
                    outcome.already_absent_files.push(candidate.path);
                }
                Err(error) => {
                    outcome.retained.push(DraftOrphanCleanupRetained {
                        draft_id: Some(candidate.draft_id.clone()),
                        path: candidate.path.clone(),
                        reason: DraftOrphanCleanupRetentionReason::DeleteFailed,
                    });
                    outcome.failures.push(
                        DraftOrphanCleanupDeleteError {
                            draft_id: candidate.draft_id,
                            path: candidate.path,
                            detail: error.to_string(),
                        }
                        .into(),
                    );
                    outcome.has_more_work = true;
                }
            },
            Ok(PathStatus::Directory | PathStatus::Other) => {
                outcome.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(candidate.draft_id),
                    path: candidate.path,
                    reason: DraftOrphanCleanupRetentionReason::BodyNotRegularFile,
                });
            }
            Err(error) => {
                outcome.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(candidate.draft_id),
                    path: candidate.path.clone(),
                    reason: DraftOrphanCleanupRetentionReason::StatusUncertain,
                });
                outcome.failures.push(
                    DraftOrphanCleanupStatusError {
                        path: candidate.path,
                        detail: error.to_string(),
                    }
                    .into(),
                );
                outcome.has_more_work = true;
            }
        }
    }

    let mut pending_manifest_removals = HashMap::new();
    for fingerprint in missing_body_entries {
        let Some(entry) = latest_by_id.get(fingerprint.draft_id.as_str()) else {
            outcome.retained.push(DraftOrphanCleanupRetained {
                draft_id: Some(fingerprint.draft_id.clone()),
                path: draft_body_path(data_dir, &fingerprint.draft_id),
                reason: DraftOrphanCleanupRetentionReason::FingerprintChanged,
            });
            continue;
        };
        let Some(entry) = entry else {
            outcome.retained.push(DraftOrphanCleanupRetained {
                draft_id: Some(fingerprint.draft_id.clone()),
                path: draft_body_path(data_dir, &fingerprint.draft_id),
                reason: DraftOrphanCleanupRetentionReason::DuplicateManifestId,
            });
            continue;
        };
        if !fingerprint.matches(entry) {
            outcome.retained.push(DraftOrphanCleanupRetained {
                draft_id: Some(fingerprint.draft_id.clone()),
                path: draft_body_path(data_dir, &fingerprint.draft_id),
                reason: DraftOrphanCleanupRetentionReason::FingerprintChanged,
            });
            continue;
        }
        let path = draft_body_path(data_dir, &fingerprint.draft_id);
        match fs_metadata::path_status(&path) {
            Ok(PathStatus::Missing) => {
                pending_manifest_removals.insert(fingerprint.draft_id.clone(), fingerprint);
            }
            Ok(PathStatus::File | PathStatus::Directory | PathStatus::Other) => {
                outcome.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(fingerprint.draft_id),
                    path,
                    reason: DraftOrphanCleanupRetentionReason::BodyReappeared,
                });
            }
            Err(error) => {
                outcome.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(fingerprint.draft_id),
                    path: path.clone(),
                    reason: DraftOrphanCleanupRetentionReason::StatusUncertain,
                });
                outcome.failures.push(
                    DraftOrphanCleanupStatusError {
                        path,
                        detail: error.to_string(),
                    }
                    .into(),
                );
                outcome.has_more_work = true;
            }
        }
    }
    // Release the borrowed manifest index before retaining from the owned
    // manifest below; the explicit drop makes this mutation boundary visible.
    drop(latest_by_id);

    if pending_manifest_removals.is_empty() {
        outcome.latest_persisted_manifest = Some(latest);
        return outcome;
    }

    latest.drafts.retain(|entry| {
        !pending_manifest_removals
            .get(entry.draft_id.as_str())
            .is_some_and(|fingerprint| fingerprint.matches(entry))
    });

    let manifest_result = if faults.fail_manifest {
        Err(anyhow::anyhow!(
            "injected orphan cleanup manifest commit failure"
        ))
    } else {
        save_manifest_locked(data_dir, &latest)
    };
    match manifest_result {
        Ok(()) => {
            outcome.committed_manifest_removals = pending_manifest_removals.into_values().collect();
            outcome.latest_persisted_manifest = Some(latest);
        }
        Err(error) => {
            for fingerprint in pending_manifest_removals.values() {
                outcome.retained.push(DraftOrphanCleanupRetained {
                    draft_id: Some(fingerprint.draft_id.clone()),
                    path: draft_body_path(data_dir, &fingerprint.draft_id),
                    reason: DraftOrphanCleanupRetentionReason::ManifestCommitFailed,
                });
            }
            outcome.failures.push(
                DraftOrphanCleanupManifestError::Write {
                    path: manifest_path(data_dir),
                    detail: error.to_string(),
                }
                .into(),
            );
            // A post-rename durability error cannot prove which manifest is the
            // latest durable state, so do not label the pre-write snapshot current.
            outcome.latest_persisted_manifest = None;
            outcome.has_more_work = true;
        }
    }
    outcome
}

/// Load only manifest states that are safe inputs to destructive cleanup.
fn load_trusted_manifest_for_cleanup(
    data_dir: &Path,
) -> std::result::Result<DraftManifest, DraftOrphanCleanupManifestError> {
    let load = load_manifest_recovering(data_dir);
    if matches!(
        load.outcome,
        RecoveryLoadOutcome::Loaded | RecoveryLoadOutcome::MissingDefault
    ) && load.diagnostics.is_empty()
    {
        return Ok(load.value);
    }

    let detail = if load.diagnostics.is_empty() {
        format!("recovery outcome {:?} was not trusted", load.outcome)
    } else {
        load.diagnostics
            .iter()
            .map(RecoveryDiagnostic::summary)
            .collect::<Vec<_>>()
            .join("; ")
    };
    Err(DraftOrphanCleanupManifestError::Load {
        path: manifest_path(data_dir),
        detail,
    })
}

/// Preserve every unexecuted candidate when latest manifest state is untrusted.
fn retain_unexecuted_plan(
    data_dir: &Path,
    outcome: &mut DraftOrphanCleanupOutcome,
    fingerprints: &[DraftEntryFingerprint],
    bodies: &[DraftOrphanBodyCandidate],
    reason: DraftOrphanCleanupRetentionReason,
) {
    outcome.retained.extend(
        fingerprints
            .iter()
            .map(|fingerprint| DraftOrphanCleanupRetained {
                draft_id: Some(fingerprint.draft_id.clone()),
                path: draft_body_path(data_dir, &fingerprint.draft_id),
                reason,
            }),
    );
    outcome
        .retained
        .extend(bodies.iter().map(|candidate| DraftOrphanCleanupRetained {
            draft_id: Some(candidate.draft_id.clone()),
            path: candidate.path.clone(),
            reason,
        }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::draft::{DraftEntry, FileDraftRestoreResolution};
    use crate::model::session::{SessionData, SessionTab};
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn file_entry(id: &str, path: &Path, original_mtime_secs: Option<u64>) -> DraftEntry {
        DraftEntry {
            draft_id: id.into(),
            original_path: Some(path.to_path_buf()),
            original_mtime_secs,
            saved_at_secs: 1,
        }
    }

    fn session_tab(path: &Path) -> SessionTab {
        SessionTab {
            path: Some(path.to_path_buf()),
            draft_id: None,
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
            pinned: false,
        }
    }

    fn draft_path(data_dir: &Path, draft_id: &str) -> PathBuf {
        drafts_dir(data_dir).join(format!("{draft_id}.draft"))
    }

    #[test]
    fn draft_policy_constants_are_stable() {
        assert_eq!(DRAFTS_DIR, "drafts");
        assert_eq!(MANIFEST_FILE, "manifest.json");
        assert_eq!(MAX_AUTOMATIC_DRAFT_BYTES, 64 * 1024 * 1024);
        assert_eq!(MAX_EAGER_DRAFT_PRELOAD_BYTES, 64 * 1024 * 1024);
        assert_eq!(MAX_MANIFEST_REPAIR_DRAFT_SCAN, 2048);
        assert_eq!(MAX_ORPHAN_CLEANUP_DRAFT_SCAN, 2048);
    }

    #[test]
    fn draft_id_for_path_is_deterministic() {
        let path = Path::new("/home/user/project/src/main.rs");
        let id1 = draft_id_for_path(path);
        let id2 = draft_id_for_path(path);
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 16);
        assert_eq!(id1, stable_path_hash(path));
    }

    #[test]
    fn draft_id_differs_for_different_paths() {
        let id1 = draft_id_for_path(Path::new("/a.rs"));
        let id2 = draft_id_for_path(Path::new("/b.rs"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn draft_id_for_untitled_format() {
        let id = draft_id_for_untitled(42);
        assert!(id.starts_with("untitled-"));
        assert_eq!(id.len(), "untitled-".len() + 16);
    }

    #[test]
    fn write_and_read_draft_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        write_draft(dir.path(), "abc123", content).expect("expected operation to succeed");
        let read = read_draft(dir.path(), "abc123").expect("expected operation to succeed");
        assert_eq!(read, Some(content.to_string()));
    }

    #[test]
    fn read_draft_missing_returns_none() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let result = read_draft(dir.path(), "nonexistent").expect("expected operation to succeed");
        assert_eq!(result, None);
    }

    #[test]
    fn delete_draft_file_removes_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "abc123", "content").expect("expected operation to succeed");
        delete_draft_file(dir.path(), "abc123").expect("expected operation to succeed");
        let result = read_draft(dir.path(), "abc123").expect("expected operation to succeed");
        assert_eq!(result, None);
    }

    #[test]
    fn delete_draft_file_missing_is_noop() {
        let dir = TempDir::new().expect("expected operation to succeed");
        delete_draft_file(dir.path(), "nonexistent").expect("expected operation to succeed");
    }

    #[test]
    fn read_draft_reports_non_file_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = drafts_dir(dir.path()).join("blocked.draft");
        fixture::create_dir_all(&path);

        let error = read_draft(dir.path(), "blocked").expect_err("directory draft should fail");
        assert!(
            error.to_string().contains("failed to read draft"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn delete_draft_file_reports_non_file_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = drafts_dir(dir.path()).join("blocked.draft");
        fixture::create_dir_all(&path);

        let error =
            delete_draft_file(dir.path(), "blocked").expect_err("directory draft should fail");
        assert!(
            error.to_string().contains("failed to delete draft"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn manifest_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "abc".into(),
                original_path: Some(PathBuf::from("/a.rs")),
                original_mtime_secs: Some(1000),
                saved_at_secs: 2000,
            }],
        };
        save_manifest(dir.path(), &manifest).expect("expected operation to succeed");
        let text = fixture::read_text(&manifest_path(dir.path()));
        assert!(text.contains(r#""kind": "dev.cominotti.lushtext.draft-manifest""#));
        let loaded = load_manifest(dir.path()).expect("expected operation to succeed");
        assert_eq!(loaded, manifest);
    }

    #[test]
    fn load_manifest_missing_returns_default() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest = load_manifest(dir.path()).expect("expected operation to succeed");
        assert_eq!(manifest, DraftManifest::default());
    }

    #[test]
    fn load_manifest_recovering_preserves_pre_public_manifest() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir_all(&drafts_dir(dir.path()));
        fixture::write_text(&manifest_path(dir.path()), r#"{"drafts":[]}"#);

        let load = load_manifest_recovering(dir.path());

        assert!(load.value.drafts.is_empty());
        assert!(matches!(
            load.diagnostics[0].problem,
            crate::services::recovery_metadata::RecoveryProblem::UnsupportedFormat { .. }
        ));
        assert!(load.replacement_allowed());
    }

    #[test]
    fn loaded_manifest_does_not_run_missing_manifest_repair_from_orphaned_drafts() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "untitled-orphan", "orphan content").expect("write orphan draft");
        let manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "manifest-entry".into(),
                original_path: None,
                original_mtime_secs: None,
                saved_at_secs: 1,
            }],
        };
        save_manifest(dir.path(), &manifest).expect("save manifest");

        let load = load_manifest_for_restore(dir.path(), &SessionData::default());

        assert_eq!(load.outcome, RecoveryLoadOutcome::Loaded);
        assert_eq!(load.value, manifest);
        assert!(load.diagnostics.is_empty());
    }

    #[test]
    fn update_manifest_serializes_concurrent_read_modify_write() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let data_dir = dir.path().to_path_buf();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let first_data_dir = data_dir.clone();

        let first = std::thread::spawn(move || {
            update_manifest(&first_data_dir, |manifest| {
                manifest.upsert(DraftEntry {
                    draft_id: "first".into(),
                    original_path: Some(PathBuf::from("/first.rs")),
                    original_mtime_secs: None,
                    saved_at_secs: 1,
                });
                entered_tx.send(()).expect("expected operation to succeed");
                // Hold the read-modify-write lock long enough for the second
                // thread to prove it waits for the first saved snapshot.
                std::thread::sleep(std::time::Duration::from_millis(150));
            })
            .expect("expected operation to succeed");
        });

        entered_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("expected first update to enter critical section");
        let second_data_dir = data_dir.clone();
        let second = std::thread::spawn(move || {
            update_manifest(&second_data_dir, |manifest| {
                manifest.upsert(DraftEntry {
                    draft_id: "second".into(),
                    original_path: Some(PathBuf::from("/second.rs")),
                    original_mtime_secs: None,
                    saved_at_secs: 2,
                });
            })
            .expect("expected operation to succeed");
        });

        first.join().expect("first update should not panic");
        second.join().expect("second update should not panic");

        let manifest = load_manifest(&data_dir).expect("expected operation to succeed");
        assert_eq!(manifest.drafts.len(), 2);
        assert!(manifest.find_by_id("first").is_some());
        assert!(manifest.find_by_id("second").is_some());
    }

    #[test]
    fn resolve_file_draft_restore_returns_content_when_mtime_matches() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        fixture::write_text(&path, "disk content");
        write_draft(dir.path(), "draft", "restored content")
            .expect("expected operation to succeed");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");

        let resolution = resolve_file_draft_restore(
            dir.path(),
            &file_entry("draft", &path, Some(current_mtime)),
        )
        .expect("expected operation to succeed");

        assert_eq!(
            resolution,
            FileDraftRestoreResolution::Restore {
                content: "restored content".to_string()
            }
        );
    }

    #[test]
    fn resolve_file_draft_restore_skips_changed_file_mtime() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        fixture::write_text(&path, "disk content");
        write_draft(dir.path(), "draft", "stale content").expect("expected operation to succeed");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");
        let stale_mtime = current_mtime
            .checked_add(1)
            .unwrap_or_else(|| current_mtime.saturating_sub(1));

        let resolution =
            resolve_file_draft_restore(dir.path(), &file_entry("draft", &path, Some(stale_mtime)))
                .expect("expected operation to succeed");

        assert_eq!(resolution, FileDraftRestoreResolution::SkipStale);
    }

    #[test]
    fn resolve_file_draft_restore_allows_legacy_entries_without_stored_mtime() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        fixture::write_text(&path, "disk content");
        write_draft(dir.path(), "draft", "legacy content").expect("expected operation to succeed");

        let resolution = resolve_file_draft_restore(dir.path(), &file_entry("draft", &path, None))
            .expect("expected operation to succeed");

        assert_eq!(
            resolution,
            FileDraftRestoreResolution::Restore {
                content: "legacy content".to_string()
            }
        );
    }

    #[test]
    fn resolve_file_draft_restore_skips_when_metadata_cannot_be_read() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let missing_path = dir.path().join("missing.txt");
        write_draft(dir.path(), "draft", "content").expect("expected operation to succeed");

        let resolution =
            resolve_file_draft_restore(dir.path(), &file_entry("draft", &missing_path, Some(123)))
                .expect("expected operation to succeed");

        assert_eq!(resolution, FileDraftRestoreResolution::SkipUnavailable);
    }

    #[test]
    fn resolve_file_draft_restore_handles_missing_draft_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        fixture::write_text(&path, "disk content");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");

        let resolution = resolve_file_draft_restore(
            dir.path(),
            &file_entry("draft", &path, Some(current_mtime)),
        )
        .expect("expected operation to succeed");

        assert_eq!(resolution, FileDraftRestoreResolution::MissingDraft);
    }

    #[test]
    fn resolve_file_draft_restore_skips_oversized_draft() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        fixture::write_text(&path, "disk content");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");
        let draft_id = "draft";
        fixture::create_dir_all(&drafts_dir(dir.path()));
        fixture::create_sparse_file(
            &draft_path(dir.path(), draft_id),
            MAX_AUTOMATIC_DRAFT_BYTES + 1,
        );

        let resolution = resolve_file_draft_restore(
            dir.path(),
            &file_entry(draft_id, &path, Some(current_mtime)),
        )
        .expect("expected operation to succeed");

        assert_eq!(resolution, FileDraftRestoreResolution::SkipOversized);
    }

    #[test]
    fn draft_preload_decision_allows_exact_single_file_limit() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let draft_id = "exact-large";
        fixture::create_dir_all(&drafts_dir(dir.path()));
        fixture::create_sparse_file(&draft_path(dir.path(), draft_id), MAX_AUTOMATIC_DRAFT_BYTES);
        let mut preloaded_bytes = 0;

        let decision = draft_preload_decision(
            dir.path(),
            draft_id,
            &mut preloaded_bytes,
            MAX_EAGER_DRAFT_PRELOAD_BYTES,
        );

        assert_eq!(decision, DraftPreloadDecision::Read);
        assert_eq!(preloaded_bytes, MAX_EAGER_DRAFT_PRELOAD_BYTES);
        assert_eq!(oversized_draft_size(dir.path(), draft_id), None);
    }

    #[test]
    fn draft_preload_decision_allows_exact_cumulative_budget() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let draft_id = "last-byte";
        write_draft(dir.path(), draft_id, "x").expect("write small draft");
        let mut preloaded_bytes = MAX_EAGER_DRAFT_PRELOAD_BYTES - 1;

        let decision = draft_preload_decision(
            dir.path(),
            draft_id,
            &mut preloaded_bytes,
            MAX_EAGER_DRAFT_PRELOAD_BYTES,
        );

        assert_eq!(decision, DraftPreloadDecision::Read);
        assert_eq!(preloaded_bytes, MAX_EAGER_DRAFT_PRELOAD_BYTES);
    }

    #[test]
    fn load_restore_state_marks_only_aggregate_cap_skips_for_lazy_restore() {
        let dir = TempDir::new().expect("tempdir");
        let first = "untitled-first";
        let second = "untitled-second";
        write_draft(dir.path(), first, "four").expect("write first draft");
        write_draft(dir.path(), second, "also").expect("write second draft");
        save_manifest(
            dir.path(),
            &DraftManifest {
                drafts: vec![
                    DraftEntry {
                        draft_id: first.to_string(),
                        original_path: None,
                        original_mtime_secs: None,
                        saved_at_secs: 1,
                    },
                    DraftEntry {
                        draft_id: second.to_string(),
                        original_path: None,
                        original_mtime_secs: None,
                        saved_at_secs: 1,
                    },
                ],
            },
        )
        .expect("save manifest");
        session_service::save(
            dir.path(),
            &SessionData {
                tabs: vec![
                    SessionTab {
                        path: None,
                        draft_id: Some(first.to_string()),
                        cursor_line: 0,
                        cursor_col: 0,
                        scroll_line: 0,
                        pinned: false,
                    },
                    SessionTab {
                        path: None,
                        draft_id: Some(second.to_string()),
                        cursor_line: 0,
                        cursor_col: 0,
                        scroll_line: 0,
                        pinned: false,
                    },
                ],
                active_tab_index: Some(0),
            },
        )
        .expect("save session");

        let restore = load_restore_state_with_eager_limit(dir.path(), 4);

        assert_eq!(
            restore.preloaded_drafts.get(first),
            Some(&PreloadedDraftRestore::Content("four".to_string()))
        );
        assert_eq!(
            restore.preloaded_drafts.get(second),
            Some(&PreloadedDraftRestore::LazyAggregateBudget)
        );
        assert!(restore.manifest.find_by_id(second).is_some());
        assert_eq!(
            read_draft(dir.path(), second).expect("read preserved lazy draft"),
            Some("also".to_string())
        );
    }

    #[test]
    fn load_restore_state_removes_stale_file_draft_from_manifest_and_disk() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        fixture::write_text(&path, "disk content");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");
        let stale_mtime = current_mtime
            .checked_add(1)
            .unwrap_or_else(|| current_mtime.saturating_sub(1));
        let draft_id = draft_id_for_path(&path);

        write_draft(dir.path(), &draft_id, "stale content").expect("expected operation to succeed");
        save_manifest(
            dir.path(),
            &DraftManifest {
                drafts: vec![file_entry(&draft_id, &path, Some(stale_mtime))],
            },
        )
        .expect("expected operation to succeed");
        session_service::save(
            dir.path(),
            &SessionData {
                tabs: vec![session_tab(&path)],
                active_tab_index: Some(0),
            },
        )
        .expect("expected operation to succeed");

        let restore = load_restore_state(dir.path());

        assert_eq!(
            restore.preloaded_drafts.get(&draft_id),
            Some(&PreloadedDraftRestore::SkipStaleFile)
        );
        assert!(restore.manifest.find_by_id(&draft_id).is_none());
        assert!(
            load_manifest(dir.path())
                .expect("expected operation to succeed")
                .find_by_id(&draft_id)
                .is_none()
        );
        assert_eq!(
            read_draft(dir.path(), &draft_id).expect("expected operation to succeed"),
            None
        );
    }

    #[test]
    fn load_restore_state_allows_orphan_cleanup_for_clean_loaded_manifest() {
        let dir = TempDir::new().expect("expected operation to succeed");
        save_manifest(dir.path(), &DraftManifest::default()).expect("save manifest");

        let restore = load_restore_state(dir.path());

        assert!(
            restore.orphan_cleanup_allowed,
            "a clean loaded manifest may participate in normal orphan cleanup"
        );
        assert!(restore.diagnostics.is_empty());
    }

    #[test]
    fn load_restore_state_allows_orphan_cleanup_for_empty_missing_manifest() {
        let dir = TempDir::new().expect("expected operation to succeed");

        let restore = load_restore_state(dir.path());

        assert!(
            restore.orphan_cleanup_allowed,
            "a missing manifest with no surviving draft evidence may run normal orphan cleanup"
        );
        assert!(restore.manifest.drafts.is_empty());
        assert!(restore.diagnostics.is_empty());
    }

    #[test]
    fn load_restore_state_disables_orphan_cleanup_when_missing_repair_skips_drafts() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "ambiguous-file-backed-id", "ambiguous")
            .expect("write ambiguous draft");

        let restore = load_restore_state(dir.path());

        assert!(
            !restore.orphan_cleanup_allowed,
            "missing manifests with ambiguous surviving drafts must preserve evidence"
        );
        assert!(restore.manifest.drafts.is_empty());
        assert!(restore.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.problem,
                crate::services::recovery_metadata::RecoveryProblem::RepairSkipped { .. }
            )
        }));
    }

    #[test]
    fn load_restore_state_skips_oversized_untitled_draft_without_deleting_it() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let draft_id = "untitled-large";
        fixture::create_dir_all(&drafts_dir(dir.path()));
        let draft_path = draft_path(dir.path(), draft_id);
        fixture::create_sparse_file(&draft_path, MAX_AUTOMATIC_DRAFT_BYTES + 1);

        save_manifest(
            dir.path(),
            &DraftManifest {
                drafts: vec![DraftEntry {
                    draft_id: draft_id.to_string(),
                    original_path: None,
                    original_mtime_secs: None,
                    saved_at_secs: 1,
                }],
            },
        )
        .expect("save manifest");
        session_service::save(
            dir.path(),
            &SessionData {
                tabs: vec![SessionTab {
                    path: None,
                    draft_id: Some(draft_id.to_string()),
                    cursor_line: 2,
                    cursor_col: 3,
                    scroll_line: 4,
                    pinned: false,
                }],
                active_tab_index: Some(0),
            },
        )
        .expect("save session");

        let restore = load_restore_state(dir.path());

        assert_eq!(
            restore.preloaded_drafts.get(draft_id),
            Some(&PreloadedDraftRestore::SkipOversized)
        );
        assert_eq!(restore.session.tabs.len(), 1, "session tab still restores");
        assert_eq!(restore.session.tabs[0].draft_id.as_deref(), Some(draft_id));
        assert!(
            restore.manifest.find_by_id(draft_id).is_some(),
            "manifest entry should remain available for later recovery"
        );
        assert!(
            fs_metadata::exists(&draft_path),
            "oversized draft file must not be deleted"
        );
    }

    #[test]
    fn load_restore_state_repairs_corrupt_manifest_for_untitled_draft() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let draft_id = "untitled-0000000000000042";
        write_draft(dir.path(), draft_id, "restored untitled")
            .expect("expected operation to succeed");
        fixture::write_text(&manifest_path(dir.path()), "not json");
        session_service::save(
            dir.path(),
            &SessionData {
                tabs: vec![SessionTab {
                    path: None,
                    draft_id: Some(draft_id.to_string()),
                    cursor_line: 0,
                    cursor_col: 0,
                    scroll_line: 0,
                    pinned: false,
                }],
                active_tab_index: Some(0),
            },
        )
        .expect("save session");

        let restore = load_restore_state(dir.path());

        assert_eq!(
            restore.preloaded_drafts.get(draft_id),
            Some(&PreloadedDraftRestore::Content(
                "restored untitled".to_string()
            ))
        );
        assert!(
            restore.manifest.find_by_id(draft_id).is_some(),
            "repaired manifest should include the safe untitled draft"
        );
        assert!(
            !restore.orphan_cleanup_allowed,
            "startup cleanup stays disabled after manifest corruption"
        );
        assert!(restore.diagnostics.iter().any(|diagnostic| {
            diagnostic.class == RecoveryMetadataClass::DraftManifest
                && matches!(
                    diagnostic.problem,
                    crate::services::recovery_metadata::RecoveryProblem::Malformed { .. }
                )
        }));
        assert!(restore.diagnostics.iter().any(|diagnostic| {
            diagnostic.class == RecoveryMetadataClass::DraftManifest
                && matches!(
                    diagnostic.problem,
                    crate::services::recovery_metadata::RecoveryProblem::Repaired { .. }
                )
        }));
        assert!(
            load_manifest(dir.path())
                .expect("repaired manifest should be durable")
                .find_by_id(draft_id)
                .is_some()
        );
    }

    #[test]
    fn load_restore_state_repairs_missing_manifest_for_session_untitled_legacy_draft_id() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let draft_id = "session-legacy-draft";
        write_draft(dir.path(), draft_id, "restored legacy untitled").expect("write legacy draft");
        session_service::save(
            dir.path(),
            &SessionData {
                tabs: vec![SessionTab {
                    path: None,
                    draft_id: Some(draft_id.to_string()),
                    cursor_line: 0,
                    cursor_col: 0,
                    scroll_line: 0,
                    pinned: false,
                }],
                active_tab_index: Some(0),
            },
        )
        .expect("save session");

        let restore = load_restore_state(dir.path());

        assert_eq!(
            restore.preloaded_drafts.get(draft_id),
            Some(&PreloadedDraftRestore::Content(
                "restored legacy untitled".to_string()
            ))
        );
        assert!(restore.manifest.find_by_id(draft_id).is_some());
        assert!(
            !restore.orphan_cleanup_allowed,
            "repaired missing manifests stay conservative until persisted state is trusted"
        );
    }

    #[test]
    fn load_restore_state_preserves_ambiguous_draft_after_corrupt_manifest() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let draft_id = "abcdef0123456789";
        write_draft(dir.path(), draft_id, "ambiguous file-backed draft")
            .expect("expected operation to succeed");
        fixture::write_text(&manifest_path(dir.path()), "not json");
        session_service::save(
            dir.path(),
            &SessionData {
                tabs: Vec::new(),
                active_tab_index: None,
            },
        )
        .expect("save session");

        let restore = load_restore_state(dir.path());

        assert!(restore.manifest.drafts.is_empty());
        assert!(
            !restore.orphan_cleanup_allowed,
            "orphan cleanup must not delete ambiguous surviving drafts"
        );
        assert_eq!(
            read_draft(dir.path(), draft_id).expect("draft should still be readable"),
            Some("ambiguous file-backed draft".to_string())
        );
        assert!(restore.diagnostics.iter().any(|diagnostic| {
            diagnostic.class == RecoveryMetadataClass::DraftManifest
                && matches!(
                    diagnostic.problem,
                    crate::services::recovery_metadata::RecoveryProblem::RepairSkipped { .. }
                )
        }));
    }

    #[test]
    fn read_draft_rejects_oversized_draft_without_deleting_it() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let draft_id = "oversized-read";
        fixture::create_dir_all(&drafts_dir(dir.path()));
        let draft_path = draft_path(dir.path(), draft_id);
        fixture::create_sparse_file(&draft_path, MAX_AUTOMATIC_DRAFT_BYTES + 1);

        let error = read_draft(dir.path(), draft_id).expect_err("oversized draft should not load");

        assert!(
            error.to_string().contains("too large to restore"),
            "unexpected error: {error}"
        );
        assert!(
            fs_metadata::exists(&draft_path),
            "oversized draft file must remain available for manual recovery"
        );
    }

    #[test]
    fn bounded_read_rechecks_bytes_after_metadata_admission() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("racy-growth.draft");
        fixture::write_text(&path, "four");

        assert_eq!(
            read_draft_path_bounded(&path, 4).expect("exact bound should load"),
            Some("four".to_string())
        );
        let error = read_draft_path_bounded(&path, 3).expect_err("grown body must be rejected");
        assert!(matches!(
            error.downcast_ref::<DraftReadError>(),
            Some(DraftReadError::Oversized {
                size: 4,
                max: 3,
                ..
            })
        ));
        assert!(fs_metadata::exists(&path));
    }

    #[test]
    fn stale_restore_cleanup_fallback_retains_only_non_stale_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "stale", "stale content").expect("expected operation to succeed");
        write_draft(dir.path(), "keep", "keep content").expect("expected operation to succeed");
        let keep = DraftEntry {
            draft_id: "keep".into(),
            original_path: Some(PathBuf::from("/keep.rs")),
            original_mtime_secs: None,
            saved_at_secs: 2,
        };
        let mut manifest = DraftManifest {
            drafts: vec![
                DraftEntry {
                    draft_id: "stale".into(),
                    original_path: Some(PathBuf::from("/stale.rs")),
                    original_mtime_secs: None,
                    saved_at_secs: 1,
                },
                keep.clone(),
            ],
        };
        fixture::create_dir_all(&drafts_dir(dir.path()).join(MANIFEST_FILE));

        cleanup_stale_restore_entries(dir.path(), &mut manifest, &[String::from("stale")]);

        assert_eq!(manifest.drafts, vec![keep]);
        assert_eq!(
            read_draft(dir.path(), "stale").expect("expected operation to succeed"),
            None
        );
        assert_eq!(
            read_draft(dir.path(), "keep").expect("expected operation to succeed"),
            Some("keep content".to_string())
        );
    }

    #[test]
    fn orphan_inspection_plans_missing_entries_and_unreferenced_bodies() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "ghost".into(),
                original_path: Some(PathBuf::from("/gone.rs")),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            }],
        };
        write_draft(dir.path(), "orphan", "stale content").expect("expected operation to succeed");

        let plan = inspect_orphan_cleanup(dir.path(), &manifest)
            .expect("inspection should produce a trusted plan");

        assert_eq!(
            plan.missing_body_entries,
            vec![DraftEntryFingerprint::from_entry(&manifest.drafts[0])]
        );
        assert_eq!(
            plan.orphan_bodies,
            vec![DraftOrphanBodyCandidate {
                draft_id: "orphan".to_string(),
                path: draft_body_path(dir.path(), "orphan"),
                inode: fs_metadata::inode(&draft_body_path(dir.path(), "orphan"))
                    .expect("orphan inode"),
            }]
        );
        assert!(plan.failures.is_empty());
        assert!(!plan.has_more_work);
    }

    #[test]
    fn orphan_inspection_distinguishes_missing_from_status_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let drafts = drafts_dir(dir.path());
        fixture::create_dir_all(&drafts);
        let loop_path = draft_body_path(dir.path(), "loop");
        fixture::symlink(Path::new("loop.draft"), &loop_path);
        let missing = DraftEntry {
            draft_id: "missing".into(),
            original_path: None,
            original_mtime_secs: None,
            saved_at_secs: 1,
        };
        let loop_entry = DraftEntry {
            draft_id: "loop".into(),
            original_path: None,
            original_mtime_secs: None,
            saved_at_secs: 2,
        };
        let manifest = DraftManifest {
            drafts: vec![missing.clone(), loop_entry],
        };

        let plan = inspect_orphan_cleanup(dir.path(), &manifest)
            .expect("the directory scan itself should remain trusted");

        assert_eq!(
            plan.missing_body_entries,
            vec![DraftEntryFingerprint::from_entry(&missing)]
        );
        assert!(plan.failures.iter().any(|failure| matches!(
            failure,
            DraftOrphanCleanupFailure::Status(error) if error.path == loop_path
        )));
        assert!(plan.retained.iter().any(|retained| {
            retained.draft_id.as_deref() == Some("loop")
                && retained.reason == DraftOrphanCleanupRetentionReason::StatusUncertain
        }));
    }

    #[test]
    fn orphan_inspection_preserves_unknown_files_and_duplicate_ids() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let drafts = drafts_dir(dir.path());
        fixture::create_dir_all(&drafts);
        fixture::write_text(&drafts.join("notes.txt"), "preserve me");
        let manifest = DraftManifest {
            drafts: vec![
                DraftEntry {
                    draft_id: "same".into(),
                    original_path: None,
                    original_mtime_secs: None,
                    saved_at_secs: 1,
                },
                DraftEntry {
                    draft_id: "same".into(),
                    original_path: Some(PathBuf::from("/newer.rs")),
                    original_mtime_secs: Some(2),
                    saved_at_secs: 2,
                },
            ],
        };

        let plan = inspect_orphan_cleanup(dir.path(), &manifest)
            .expect("inspection should preserve ambiguous evidence");

        assert!(plan.missing_body_entries.is_empty());
        assert!(plan.retained.iter().any(|retained| {
            retained.path == drafts.join("notes.txt")
                && retained.reason == DraftOrphanCleanupRetentionReason::UnknownFile
        }));
        assert!(plan.retained.iter().any(|retained| {
            retained.draft_id.as_deref() == Some("same")
                && retained.reason == DraftOrphanCleanupRetentionReason::DuplicateManifestId
        }));
    }

    #[test]
    fn orphan_inspection_reports_exact_cap_as_unfinished() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let drafts = drafts_dir(dir.path());
        fixture::create_dir_all(&drafts);
        for index in 0..MAX_ORPHAN_CLEANUP_DRAFT_SCAN {
            fixture::write_text(&drafts.join(format!("orphan-{index:04}.draft")), "body");
        }

        let capped = inspect_orphan_cleanup(dir.path(), &DraftManifest::default())
            .expect("bounded scan should succeed");
        assert_eq!(capped.orphan_bodies.len(), MAX_ORPHAN_CLEANUP_DRAFT_SCAN);
        assert!(capped.has_more_work);

        fixture::remove_file(&drafts.join("orphan-0000.draft"));
        let below_cap = inspect_orphan_cleanup(dir.path(), &DraftManifest::default())
            .expect("below-cap scan should succeed");
        assert_eq!(
            below_cap.orphan_bodies.len(),
            MAX_ORPHAN_CLEANUP_DRAFT_SCAN - 1
        );
        assert!(!below_cap.has_more_work);
    }

    #[test]
    fn orphan_inspection_pages_large_manifest_without_revisiting_prefix() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest = DraftManifest {
            drafts: (0..MAX_ORPHAN_CLEANUP_DRAFT_SCAN + 2)
                .map(|index| DraftEntry {
                    draft_id: format!("manifest-{index}"),
                    original_path: None,
                    original_mtime_secs: None,
                    saved_at_secs: u64::try_from(index).expect("bounded fixture index fits u64"),
                })
                .collect(),
        };

        let first = inspect_orphan_cleanup(dir.path(), &manifest)
            .expect("first manifest page should succeed");
        assert_eq!(
            first.missing_body_entries.len(),
            MAX_ORPHAN_CLEANUP_DRAFT_SCAN
        );
        assert_eq!(
            first.next_manifest_offset,
            Some(MAX_ORPHAN_CLEANUP_DRAFT_SCAN)
        );
        assert!(first.has_more_work);

        let second = inspect_orphan_cleanup_from(
            dir.path(),
            &manifest,
            first.next_manifest_offset.expect("continuation offset"),
        )
        .expect("second manifest page should succeed");
        assert_eq!(second.missing_body_entries.len(), 2);
        assert_eq!(second.missing_body_entries[0].draft_id, "manifest-2048");
        assert_eq!(second.next_manifest_offset, None);
    }

    #[test]
    fn orphan_execution_preserves_duplicate_id_split_across_manifest_pages() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut drafts = (0..MAX_ORPHAN_CLEANUP_DRAFT_SCAN)
            .map(|index| DraftEntry {
                draft_id: format!("manifest-{index}"),
                original_path: None,
                original_mtime_secs: None,
                saved_at_secs: u64::try_from(index).expect("fixture index fits u64"),
            })
            .collect::<Vec<_>>();
        drafts[0].draft_id = "duplicate".to_string();
        drafts.push(DraftEntry {
            draft_id: "duplicate".to_string(),
            original_path: Some(PathBuf::from("/newer.rs")),
            original_mtime_secs: Some(2),
            saved_at_secs: 2,
        });
        let manifest = DraftManifest { drafts };
        save_manifest(dir.path(), &manifest).expect("seed duplicate manifest");
        let plan =
            inspect_orphan_cleanup(dir.path(), &manifest).expect("inspect first bounded page");

        let outcome = execute_orphan_cleanup(dir.path(), plan);

        assert!(
            !outcome
                .committed_manifest_removals
                .iter()
                .any(|fingerprint| fingerprint.draft_id == "duplicate")
        );
        assert!(outcome.retained.iter().any(|retained| {
            retained.draft_id.as_deref() == Some("duplicate")
                && retained.reason == DraftOrphanCleanupRetentionReason::DuplicateManifestId
        }));
        assert_eq!(
            load_manifest(dir.path())
                .expect("load preserved manifest")
                .drafts
                .iter()
                .filter(|entry| entry.draft_id == "duplicate")
                .count(),
            2
        );
    }

    #[cfg(unix)]
    #[test]
    fn orphan_inspection_scan_failure_returns_no_partial_plan() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let drafts = drafts_dir(dir.path());
        fixture::create_dir_all(&drafts);
        fixture::write_text(&drafts.join("orphan.draft"), "body");
        fixture::set_mode(&drafts, 0);

        let result = inspect_orphan_cleanup(dir.path(), &DraftManifest::default());

        fixture::set_mode(&drafts, 0o700);
        assert!(
            matches!(
                &result,
                Err(DraftOrphanCleanupScanError::Read { .. }
                    | DraftOrphanCleanupScanError::Status(_))
            ),
            "unexpected orphan inspection result: {result:?}"
        );
    }

    #[test]
    fn confirmed_cleanup_count_saturates() {
        assert_eq!(saturating_confirmed_cleanup_count(2, 3), 5);
        assert_eq!(
            saturating_confirmed_cleanup_count(usize::MAX, 1),
            usize::MAX
        );
    }

    #[test]
    fn orphan_inspection_keeps_valid_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "valid", "content").expect("expected operation to succeed");
        let manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "valid".into(),
                original_path: Some(PathBuf::from("/a.rs")),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            }],
        };

        let plan = inspect_orphan_cleanup(dir.path(), &manifest)
            .expect("valid draft should produce an empty plan");

        assert!(plan.missing_body_entries.is_empty());
        assert!(plan.orphan_bodies.is_empty());
        assert!(plan.failures.is_empty());
    }

    #[test]
    fn orphan_inspection_ignores_hidden_draft_files() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let hidden_path = drafts_dir(dir.path()).join(".hidden.draft");
        fixture::create_dir_all(hidden_path.parent().expect("expected drafts dir"));
        fixture::write_text(&hidden_path, "editor swap");

        let plan = inspect_orphan_cleanup(dir.path(), &DraftManifest::default())
            .expect("hidden files should stay outside cleanup");

        assert!(plan.orphan_bodies.is_empty());
        assert!(fs_metadata::exists(&hidden_path));
    }

    #[test]
    fn write_draft_creates_directory() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let nested = dir.path().join("deep/path");
        write_draft(&nested, "abc", "content").expect("expected operation to succeed");
        let result = read_draft(&nested, "abc").expect("expected operation to succeed");
        assert_eq!(result, Some("content".to_string()));
    }

    #[test]
    fn write_draft_overwrites_existing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "abc", "first").expect("expected operation to succeed");
        write_draft(dir.path(), "abc", "second").expect("expected operation to succeed");
        let result = read_draft(dir.path(), "abc").expect("expected operation to succeed");
        assert_eq!(result, Some("second".to_string()));
    }
}

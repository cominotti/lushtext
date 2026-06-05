// SPDX-License-Identifier: GPL-3.0-or-later

//! Local-history persistence helpers for saved documents.
//!
//! This service owns the filesystem-facing local-history workflow: resolve
//! stable saved-file identity, write normalized full-text snapshots, prune old
//! snapshots, migrate lineages after in-app renames, and load snapshot metadata
//! or bodies for the browser UI. Everything here stays GTK-free so capture and
//! browse work can run on background threads.

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(any(test, feature = "test-utils"))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::model::local_history::{
    LocalHistoryDocument, LocalHistorySnapshot, LocalHistorySnapshotMeta,
    LocalHistorySnapshotOrigin,
};
use crate::model::sidecar_identity::{DocumentSidecarIdentity, stable_bytes_hash};
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryLoad, RecoveryLoadOutcome, RecoveryMetadataClass,
};
use crate::services::{
    editor_io,
    file_limits::{DISABLE_UNDO_HISTORY, FileSizeCheck},
    filesystem::{
        DirectoryScanPolicy, FileKind, WriteLabel, metadata as fs_metadata, mutate as fs_mutate,
        read as fs_read, tree as fs_tree, write as fs_write,
    },
    json_store, note_storage,
};

/// Directory name that stores one local-history lineage per saved document.
const LOCAL_HISTORY_DIR: &str = "local-history";
/// Metadata filename stored inside each lineage directory.
const INDEX_FILENAME: &str = "index.json";
/// Snapshot files stay plain UTF-8 text so restore and debugging remain simple.
const SNAPSHOT_EXTENSION: &str = "txt";

/// Keep at most this many snapshots per document before older entries are trimmed.
///
/// Forty-eight entries comfortably covers a full work day of baseline, periodic,
/// save, and restore-safety points without letting one document dominate disk use.
const PER_DOCUMENT_SNAPSHOT_CAP: usize = 48;
/// Keep at most this many snapshots across the whole app data directory.
///
/// Two hundred forty entries keeps the MVP bounded even when the user touches
/// many files in one session, while still leaving enough room for several active
/// documents to retain rich history.
const GLOBAL_SNAPSHOT_CAP: usize = 240;
/// Maximum lineage directories scanned during one startup reconciliation pass.
///
/// This keeps recovery work bounded on very large history stores. Any deferred
/// work remains on disk for the next startup or browse-triggered pass.
const DEFAULT_RECONCILE_MAX_LINEAGES: usize = 512;
/// Maximum wall-clock time spent on one startup lineage reconciliation pass.
///
/// Fifty milliseconds is intentionally short because this command can run
/// before browse surfaces are ready; deeper manual or benchmark runs can pass a
/// larger budget.
const DEFAULT_RECONCILE_MAX_MILLIS: u64 = 50;
/// Maximum total snapshot body bytes read while repairing one corrupt index.
///
/// Local-history browse still allows one large-but-supported snapshot, but index
/// repair must not sequentially load a whole retained history for a damaged
/// lineage. When this budget is exceeded, all snapshot files stay preserved and
/// the index remains unavailable until a manual or future streaming repair path.
const MAX_INDEX_REPAIR_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

/// Test-only switch for deterministic obsolete-lineage cleanup failures.
///
/// Permission-bit fixtures do not fail when CI containers run tests as root, so
/// integration tests use this seam to exercise the retryable cleanup path
/// without depending on host user privileges.
#[cfg(any(test, feature = "test-utils"))]
static FAIL_NEXT_OBSOLETE_LINEAGE_CLEANUP: AtomicBool = AtomicBool::new(false);

/// Size-policy view used by the editor and window layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalHistoryAvailability {
    /// Full capture cadence and browse surface are available.
    Full,
    /// Only save-boundary capture is allowed, but browsing stored history is still allowed.
    SaveOnly,
    /// Local history is unavailable for this document size.
    Unavailable,
}

impl LocalHistoryAvailability {
    /// Whether baseline and periodic automatic capture should run.
    #[must_use]
    pub fn allows_automatic_capture(self) -> bool {
        self == Self::Full
    }

    /// Whether the browser and restore workflow should be available.
    #[must_use]
    pub fn allows_browsing(self) -> bool {
        self != Self::Unavailable
    }
}

/// Duplicate-handling policy for one capture boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalHistoryCapturePolicy {
    /// Skip storing the snapshot when it matches the newest stored text.
    DeduplicateLatest,
    /// Always keep a fresh snapshot even if it repeats the newest stored text.
    PreserveDuplicate,
}

/// Result of trying to capture one local-history snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalHistoryCaptureOutcome {
    /// A new snapshot was stored and kept after retention pruning.
    Stored(LocalHistorySnapshotMeta),
    /// The candidate matched the newest snapshot and was intentionally skipped.
    SkippedDuplicate,
}

/// Snapshot metadata plus diagnostics from a recovery-aware history listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHistorySnapshotListing {
    /// Snapshot metadata safe to show in the history browser.
    pub snapshots: Vec<LocalHistorySnapshotMeta>,
    /// Recovery diagnostics produced while loading or repairing the lineage index.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

/// Bounded work budget for local-history lineage reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalHistoryReconcileBudget {
    /// Maximum lineage directories to inspect in this pass.
    pub max_lineages: usize,
    /// Maximum elapsed time before remaining work is deferred.
    pub max_elapsed: Duration,
}

impl Default for LocalHistoryReconcileBudget {
    fn default() -> Self {
        Self {
            max_lineages: DEFAULT_RECONCILE_MAX_LINEAGES,
            max_elapsed: Duration::from_millis(DEFAULT_RECONCILE_MAX_MILLIS),
        }
    }
}

impl LocalHistoryReconcileBudget {
    /// Build an explicit reconciliation budget for tests, benchmarks, or tools.
    #[must_use]
    pub const fn new(max_lineages: usize, max_elapsed: Duration) -> Self {
        Self {
            max_lineages,
            max_elapsed,
        }
    }

    fn scan_policy(self) -> DirectoryScanPolicy {
        DirectoryScanPolicy {
            max_entries: self.max_lineages.saturating_add(1),
            include_hidden: false,
        }
    }

    fn elapsed(self, started_at: Instant) -> bool {
        self.max_elapsed.is_zero() || started_at.elapsed() >= self.max_elapsed
    }
}

/// Summary of one local-history lineage reconciliation pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LocalHistoryReconcileReport {
    /// Directory lineages inspected during this pass.
    pub scanned_lineages: usize,
    /// Lineages with no trusted index or a non-directory entry in the history root.
    pub orphaned_lineages: usize,
    /// Valid indexes stored in the wrong lineage directory.
    pub mismatched_lineages: usize,
    /// Mismatched or duplicate lineages merged into their canonical directory.
    pub reconciled_lineages: usize,
    /// Reconciliations whose target write succeeded but obsolete cleanup failed.
    pub cleanup_failures: usize,
    /// Work intentionally left for a later pass because a bound was reached.
    pub deferred_lineages: usize,
    /// Recovery diagnostics produced while loading or reconciling lineages.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

impl LocalHistoryReconcileReport {
    /// Return whether another reconciliation pass may still have useful work.
    #[must_use]
    pub const fn has_deferred_work(&self) -> bool {
        self.deferred_lineages > 0
    }
}

#[derive(Debug, Clone, Copy)]
struct RetentionPolicy {
    per_document_cap: usize,
    global_cap: usize,
}

const DEFAULT_RETENTION_POLICY: RetentionPolicy = RetentionPolicy {
    per_document_cap: PER_DOCUMENT_SNAPSHOT_CAP,
    global_cap: GLOBAL_SNAPSHOT_CAP,
};

#[derive(Debug)]
struct LoadedHistoryDocument {
    dir: PathBuf,
    document: LocalHistoryDocument,
}

/// Outcome of deterministic index repair from surviving snapshot text files.
enum LocalHistoryIndexRepair {
    Repaired(LocalHistoryDocument),
    Skipped(String),
}

/// Resolve the local-history base directory under the app data home.
#[must_use]
pub fn local_history_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(LOCAL_HISTORY_DIR)
}

/// Resolve the stable identity for a saved document path.
///
/// # Errors
///
/// Returns an error if the path cannot be canonicalized.
pub fn resolve_document_identity(path: &Path) -> Result<DocumentSidecarIdentity> {
    note_storage::resolve_document_identity(path)
}

/// Map the editor's existing large-file policy onto local-history behavior.
#[must_use]
pub fn availability_for_size_check(size_check: FileSizeCheck) -> LocalHistoryAvailability {
    match size_check {
        FileSizeCheck::Normal | FileSizeCheck::LargeFileToast => LocalHistoryAvailability::Full,
        FileSizeCheck::DisableSyntax => LocalHistoryAvailability::SaveOnly,
        FileSizeCheck::DisableUndoAndSyntax | FileSizeCheck::TooLarge => {
            LocalHistoryAvailability::Unavailable
        }
    }
}

/// Fail the next obsolete local-history lineage cleanup in test builds.
#[cfg(any(test, feature = "test-utils"))]
pub fn fail_next_obsolete_lineage_cleanup_for_test() {
    FAIL_NEXT_OBSOLETE_LINEAGE_CLEANUP.store(true, Ordering::Release);
}

/// Capture one snapshot for a saved document path using the default retention policy.
///
/// # Errors
///
/// Returns an error if the document identity cannot be resolved or the snapshot
/// metadata/text cannot be written.
pub fn capture_snapshot_for_path(
    data_dir: &Path,
    path: &Path,
    text: &str,
    origin: LocalHistorySnapshotOrigin,
    policy: LocalHistoryCapturePolicy,
) -> Result<LocalHistoryCaptureOutcome> {
    capture_snapshot_for_path_with_retention(
        data_dir,
        path,
        text,
        origin,
        policy,
        DEFAULT_RETENTION_POLICY,
    )
}

/// List snapshot metadata for the saved document, newest first.
///
/// # Errors
///
/// Returns an error if the document identity cannot be resolved. Malformed or
/// unreadable indexes are repaired or treated as empty here; use
/// [`list_snapshots_for_path_recovering`] when callers need the diagnostics.
pub fn list_snapshots_for_path(
    data_dir: &Path,
    path: &Path,
) -> Result<Vec<LocalHistorySnapshotMeta>> {
    Ok(list_snapshots_for_path_recovering(data_dir, path)?.snapshots)
}

/// List snapshot metadata and preserve recovery diagnostics for the browser UI.
///
/// # Errors
///
/// Returns an error if the document identity cannot be resolved or a
/// deterministic repair cannot be durably recorded.
pub fn list_snapshots_for_path_recovering(
    data_dir: &Path,
    path: &Path,
) -> Result<LocalHistorySnapshotListing> {
    let _guard = local_history_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("local-history lock poisoned"))?;
    let identity = resolve_document_identity(path)?;
    let load = load_document_for_identity_recovering(data_dir, identity)?;
    Ok(LocalHistorySnapshotListing {
        snapshots: load.value.snapshots,
        diagnostics: load.diagnostics,
    })
}

/// Load one snapshot body for the saved document.
///
/// # Errors
///
/// Returns an error if the document identity cannot be resolved, metadata
/// cannot be read, or the selected snapshot file cannot be read as bounded UTF-8.
pub fn load_snapshot_for_path(
    data_dir: &Path,
    path: &Path,
    snapshot_id: &str,
) -> Result<Option<LocalHistorySnapshot>> {
    let _guard = local_history_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("local-history lock poisoned"))?;
    let identity = resolve_document_identity(path)?;
    let document = load_document_for_identity(data_dir, identity.clone())?;
    let Some(meta) = document
        .snapshots
        .iter()
        .find(|meta| meta.snapshot_id == snapshot_id)
        .cloned()
    else {
        return Ok(None);
    };

    let snapshot_path = snapshot_path(&document_dir(data_dir, &identity), &meta.snapshot_id);
    let text = load_snapshot_text_bounded(&snapshot_path)?;
    Ok(Some(LocalHistorySnapshot { meta, text }))
}

/// Move local-history lineages after an in-app rename of a file or directory tree.
///
/// Returns the number of history documents that were migrated.
///
/// # Errors
///
/// Returns an error if history directories cannot be scanned, merged, or rewritten.
pub fn move_path_tree(data_dir: &Path, old_path: &Path, new_path: &Path) -> Result<usize> {
    let _guard = local_history_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("local-history lock poisoned"))?;
    let base_dir = local_history_dir(data_dir);
    if !fs_metadata::path_status(&base_dir)?.is_present() {
        return Ok(0);
    }

    let mut migrated = 0usize;
    let mut loaded_documents = load_all_documents_from_base(data_dir, &base_dir)?;
    for loaded in &mut loaded_documents {
        let Some((display_path, canonical_path)) =
            note_storage::rebase_identity_paths(&loaded.document.identity, old_path, new_path)
        else {
            continue;
        };

        let new_identity = DocumentSidecarIdentity::from_paths(display_path, canonical_path);
        migrate_loaded_document(data_dir, loaded, new_identity)?;
        migrated += 1;
    }

    cleanup_empty_fallback_lineage(data_dir, old_path)?;
    enforce_global_retention_locked(data_dir, DEFAULT_RETENTION_POLICY)?;
    Ok(migrated)
}

/// Reconcile interrupted or duplicate local-history lineages with the default startup budget.
///
/// # Errors
///
/// Returns an error if the history root cannot be scanned. Per-lineage merge or
/// cleanup failures are preserved in the returned diagnostics so startup can
/// continue with unaffected lineages.
pub fn reconcile_lineages(data_dir: &Path) -> Result<LocalHistoryReconcileReport> {
    reconcile_lineages_with_budget(data_dir, LocalHistoryReconcileBudget::default())
}

/// Reconcile interrupted or duplicate local-history lineages within an explicit budget.
///
/// This command only repairs evidence that is deterministic: a valid index in
/// the wrong directory is moved or merged into that index's canonical lineage
/// directory. Corrupt or orphaned directories are reported and preserved.
///
/// # Errors
///
/// Returns an error if the history root cannot be scanned.
pub fn reconcile_lineages_with_budget(
    data_dir: &Path,
    budget: LocalHistoryReconcileBudget,
) -> Result<LocalHistoryReconcileReport> {
    let _guard = local_history_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("local-history lock poisoned"))?;
    reconcile_lineages_locked(data_dir, budget)
}

fn capture_snapshot_for_path_with_retention(
    data_dir: &Path,
    path: &Path,
    text: &str,
    origin: LocalHistorySnapshotOrigin,
    capture_policy: LocalHistoryCapturePolicy,
    retention: RetentionPolicy,
) -> Result<LocalHistoryCaptureOutcome> {
    let _guard = local_history_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("local-history lock poisoned"))?;
    let identity = resolve_document_identity(path)?;
    capture_snapshot_for_identity_locked(
        data_dir,
        identity,
        text,
        origin,
        capture_policy,
        retention,
    )
}

fn capture_snapshot_for_identity_locked(
    data_dir: &Path,
    identity: DocumentSidecarIdentity,
    text: &str,
    origin: LocalHistorySnapshotOrigin,
    capture_policy: LocalHistoryCapturePolicy,
    retention: RetentionPolicy,
) -> Result<LocalHistoryCaptureOutcome> {
    let normalized = normalize_snapshot_text(text);
    let content_hash = stable_bytes_hash(normalized.as_bytes());
    let mut document = load_document_for_identity(data_dir, identity.clone())?;

    if capture_policy == LocalHistoryCapturePolicy::DeduplicateLatest
        && document
            .snapshots
            .first()
            .is_some_and(|latest| latest.content_hash == content_hash)
    {
        return Ok(LocalHistoryCaptureOutcome::SkippedDuplicate);
    }

    let meta = LocalHistorySnapshotMeta::new(origin, normalized.len() as u64, content_hash);
    let doc_dir = document_dir(data_dir, &identity);
    fs_write::create_dir_all_durable(&doc_dir)
        .with_context(|| format!("failed to create {}", doc_dir.display()))?;
    // Write the body before the index so a crash leaves repairable text
    // evidence instead of metadata that points at a missing snapshot.
    editor_io::write_snapshot_to_path(&snapshot_path(&doc_dir, &meta.snapshot_id), &normalized)
        .map(|_| ())
        .map_err(anyhow::Error::from)?;

    document.identity = identity;
    document.snapshots.push(meta.clone());
    document.sort_newest_first();
    trim_document_to_retention(&doc_dir, &mut document, retention.per_document_cap);
    save_document_index(&doc_dir, &document)?;
    enforce_global_retention_locked(data_dir, retention)?;

    Ok(LocalHistoryCaptureOutcome::Stored(meta))
}

fn reconcile_lineages_locked(
    data_dir: &Path,
    budget: LocalHistoryReconcileBudget,
) -> Result<LocalHistoryReconcileReport> {
    let mut report = LocalHistoryReconcileReport::default();
    let base_dir = local_history_dir(data_dir);
    if !fs_metadata::path_status(&base_dir)?.is_present() {
        return Ok(report);
    }

    let started_at = Instant::now();
    let mut entries = fs_tree::scan_directory(&base_dir, budget.scan_policy())
        .with_context(|| format!("failed to read {}", base_dir.display()))?;
    if entries.len() > budget.max_lineages {
        entries.truncate(budget.max_lineages);
        report.deferred_lineages += 1;
    }

    let mut mismatched = Vec::new();
    for entry in entries {
        if budget.elapsed(started_at) {
            report.deferred_lineages += 1;
            break;
        }

        if entry.kind != FileKind::Directory {
            report.orphaned_lineages += 1;
            report.diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::LocalHistoryIndex,
                &entry.path,
                "local-history root contained a non-directory entry; evidence was preserved",
            ));
            continue;
        }

        report.scanned_lineages += 1;
        let index_path = entry.path.join(INDEX_FILENAME);
        let load = load_history_index(data_dir, &index_path);
        note_storage::trace_recovery_diagnostics(&load.diagnostics);
        report.diagnostics.extend(load.diagnostics);
        let Some(mut document) = load.value else {
            report.orphaned_lineages += 1;
            if !fs_metadata::exists(&index_path)
                && remove_empty_lineage_dir_if_exists(&entry.path).with_context(|| {
                    format!("failed to clean empty lineage {}", entry.path.display())
                })?
            {
                report.reconciled_lineages += 1;
            } else if !fs_metadata::exists(&index_path) {
                report.diagnostics.push(RecoveryDiagnostic::repair_skipped(
                    RecoveryMetadataClass::LocalHistoryIndex,
                    &index_path,
                    "lineage directory has no trusted index; snapshot bodies were preserved",
                ));
            }
            continue;
        };

        document.sort_newest_first();
        let expected_dir = document_dir(data_dir, &document.identity);
        if expected_dir != entry.path {
            report.mismatched_lineages += 1;
            mismatched.push(LoadedHistoryDocument {
                dir: entry.path,
                document,
            });
        }
    }

    for mut loaded in mismatched {
        if budget.elapsed(started_at) {
            report.deferred_lineages += 1;
            break;
        }

        let source_dir = loaded.dir.clone();
        let index_path = source_dir.join(INDEX_FILENAME);
        let identity = loaded.document.identity.clone();
        match migrate_loaded_document(data_dir, &mut loaded, identity) {
            Ok(()) => report.reconciled_lineages += 1,
            Err(error) => {
                if is_obsolete_lineage_cleanup_error(&error) {
                    report.cleanup_failures += 1;
                }
                report.diagnostics.push(RecoveryDiagnostic::repair_skipped(
                    RecoveryMetadataClass::LocalHistoryIndex,
                    &index_path,
                    format!("lineage reconciliation was incomplete: {error}"),
                ));
            }
        }
    }

    Ok(report)
}

fn load_document_for_identity(
    data_dir: &Path,
    identity: DocumentSidecarIdentity,
) -> Result<LocalHistoryDocument> {
    Ok(load_document_for_identity_recovering(data_dir, identity)?.value)
}

fn load_document_for_identity_recovering(
    data_dir: &Path,
    identity: DocumentSidecarIdentity,
) -> Result<RecoveryLoad<LocalHistoryDocument>> {
    let dir = document_dir(data_dir, &identity);
    let index_path = dir.join(INDEX_FILENAME);
    let load = load_history_index(data_dir, &index_path);
    note_storage::trace_recovery_diagnostics(&load.diagnostics);
    let mut diagnostics = load.diagnostics;
    if let Some(mut document) = load.value {
        document.sort_newest_first();
        return Ok(RecoveryLoad {
            value: document,
            outcome: load.outcome,
            diagnostics,
        });
    }

    if diagnostics.is_empty() {
        return Ok(RecoveryLoad {
            value: LocalHistoryDocument::empty(identity),
            outcome: load.outcome,
            diagnostics,
        });
    }

    if !diagnostics
        .iter()
        .all(|diagnostic| diagnostic.replacement_allowed)
    {
        diagnostics.push(RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::LocalHistoryIndex,
            &index_path,
            "lineage index was preserved in place, so automatic repair cannot replace it safely",
        ));
        return Ok(RecoveryLoad {
            value: LocalHistoryDocument::empty(identity),
            outcome: RecoveryLoadOutcome::PreservedDefault,
            diagnostics,
        });
    }

    match repair_history_index_from_snapshots(&dir, &index_path, identity.clone())? {
        LocalHistoryIndexRepair::Repaired(mut document) => {
            document.sort_newest_first();
            diagnostics.push(RecoveryDiagnostic::repaired(
                RecoveryMetadataClass::LocalHistoryIndex,
                &index_path,
                format!(
                    "rebuilt local-history index from {} snapshot text file(s)",
                    document.snapshots.len()
                ),
            ));
            Ok(RecoveryLoad {
                value: document,
                outcome: RecoveryLoadOutcome::Partial,
                diagnostics,
            })
        }
        LocalHistoryIndexRepair::Skipped(detail) => {
            diagnostics.push(RecoveryDiagnostic::repair_skipped(
                RecoveryMetadataClass::LocalHistoryIndex,
                &index_path,
                detail,
            ));
            Ok(RecoveryLoad {
                value: LocalHistoryDocument::empty(identity),
                outcome: RecoveryLoadOutcome::QuarantinedDefault,
                diagnostics,
            })
        }
    }
}

fn document_dir(data_dir: &Path, identity: &DocumentSidecarIdentity) -> PathBuf {
    local_history_dir(data_dir).join(&identity.sidecar_id)
}

fn snapshot_path(document_dir: &Path, snapshot_id: &str) -> PathBuf {
    document_dir.join(format!("{snapshot_id}.{SNAPSHOT_EXTENSION}"))
}

fn repair_history_index_from_snapshots(
    document_dir: &Path,
    index_path: &Path,
    identity: DocumentSidecarIdentity,
) -> Result<LocalHistoryIndexRepair> {
    if !fs_metadata::path_status(document_dir)?.is_present() {
        return Ok(LocalHistoryIndexRepair::Skipped(
            "lineage directory is missing".to_string(),
        ));
    }

    let mut snapshots = Vec::new();
    let mut repair_body_bytes = 0u64;
    for entry in fs_tree::scan_directory(document_dir, DirectoryScanPolicy::visible_workspace())
        .with_context(|| format!("failed to read {}", document_dir.display()))?
    {
        if entry.kind != FileKind::File
            || entry.path.file_name().and_then(|name| name.to_str()) == Some(INDEX_FILENAME)
        {
            continue;
        }
        if entry.path.extension().and_then(|ext| ext.to_str()) != Some(SNAPSHOT_EXTENSION) {
            return Ok(LocalHistoryIndexRepair::Skipped(format!(
                "unsupported local-history file {} prevents deterministic repair",
                entry.path.display()
            )));
        }
        let Some(snapshot_id) = entry
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(ToOwned::to_owned)
        else {
            return Ok(LocalHistoryIndexRepair::Skipped(format!(
                "snapshot filename {} is not valid UTF-8",
                entry.path.display()
            )));
        };
        let Some(captured_at_millis) = captured_millis_from_snapshot_id(&snapshot_id) else {
            return Ok(LocalHistoryIndexRepair::Skipped(format!(
                "snapshot filename {} cannot prove capture time",
                entry.path.display()
            )));
        };
        let snapshot_bytes = match validate_snapshot_body_size(&entry.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Ok(LocalHistoryIndexRepair::Skipped(format!(
                    "snapshot body {} cannot be read for deterministic repair: {error}",
                    entry.path.display()
                )));
            }
        };
        if repair_body_bytes.saturating_add(snapshot_bytes) > MAX_INDEX_REPAIR_SNAPSHOT_BYTES {
            return Ok(LocalHistoryIndexRepair::Skipped(format!(
                "snapshot repair would exceed the {} MiB local-history repair read budget",
                MAX_INDEX_REPAIR_SNAPSHOT_BYTES / 1024 / 1024
            )));
        }
        repair_body_bytes = repair_body_bytes.saturating_add(snapshot_bytes);
        let text = match read_snapshot_text_after_size_check(&entry.path) {
            Ok(text) => text,
            Err(error) => {
                return Ok(LocalHistoryIndexRepair::Skipped(format!(
                    "snapshot body {} cannot be read for deterministic repair: {error}",
                    entry.path.display()
                )));
            }
        };
        snapshots.push(LocalHistorySnapshotMeta {
            snapshot_id,
            captured_at_millis,
            origin: LocalHistorySnapshotOrigin::Recovered,
            byte_len: text.len() as u64,
            content_hash: stable_bytes_hash(text.as_bytes()),
        });
    }

    if snapshots.is_empty() {
        return Ok(LocalHistoryIndexRepair::Skipped(
            "no snapshot text files were available to rebuild the lineage".to_string(),
        ));
    }

    let mut document = LocalHistoryDocument {
        identity,
        snapshots,
    };
    document.sort_newest_first();
    save_document_index(document_dir, &document).with_context(|| {
        format!(
            "failed to save repaired local-history index {}",
            index_path.display()
        )
    })?;
    Ok(LocalHistoryIndexRepair::Repaired(document))
}

fn load_snapshot_text_bounded(path: &Path) -> Result<String> {
    validate_snapshot_body_size(path)?;
    read_snapshot_text_after_size_check(path)
}

fn validate_snapshot_body_size(path: &Path) -> Result<u64> {
    let facts = fs_metadata::file_facts(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if facts.kind != FileKind::File {
        anyhow::bail!("local-history snapshot {} is not a file", path.display());
    }
    if !availability_for_size_check(FileSizeCheck::classify(facts.byte_size)).allows_browsing() {
        anyhow::bail!(
            "local-history snapshot {} is larger than the {} MB browse limit",
            path.display(),
            DISABLE_UNDO_HISTORY / 1_000_000
        );
    }
    Ok(facts.byte_size)
}

fn read_snapshot_text_after_size_check(path: &Path) -> Result<String> {
    let bytes =
        fs_read::bytes(path).with_context(|| format!("failed to read {}", path.display()))?;
    let text = simdutf8::basic::from_utf8(&bytes)
        .map_err(|error| anyhow::anyhow!("{} is not valid UTF-8: {error}", path.display()))?;
    Ok(text.to_string())
}

fn captured_millis_from_snapshot_id(snapshot_id: &str) -> Option<u64> {
    let mut parts = snapshot_id.split('-');
    if parts.next()? != "history" {
        return None;
    }
    let nanos_hex = parts.next()?;
    let counter_hex = parts.next()?;
    if parts.next().is_some() || nanos_hex.len() != 32 || counter_hex.len() != 16 {
        return None;
    }
    let nanos = u128::from_str_radix(nanos_hex, 16).ok()?;
    u64::try_from(nanos / 1_000_000).ok()
}

fn normalize_snapshot_text(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }

    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn save_document_index(document_dir: &Path, document: &LocalHistoryDocument) -> Result<()> {
    json_store::save(document_dir, INDEX_FILENAME, document)
}

fn trim_document_to_retention(
    document_dir: &Path,
    document: &mut LocalHistoryDocument,
    per_document_cap: usize,
) {
    if document.snapshots.len() <= per_document_cap {
        return;
    }

    let removed: Vec<_> = document.snapshots.drain(per_document_cap..).collect();
    remove_snapshot_files(document_dir, &removed);
}

fn enforce_global_retention_locked(data_dir: &Path, retention: RetentionPolicy) -> Result<()> {
    let base_dir = local_history_dir(data_dir);
    if !fs_metadata::path_status(&base_dir)?.is_present() {
        return Ok(());
    }

    let mut documents = load_all_documents_from_base(data_dir, &base_dir)?;
    let total_snapshots: usize = documents
        .iter()
        .map(|loaded| loaded.document.snapshots.len())
        .sum();
    if total_snapshots <= retention.global_cap {
        return Ok(());
    }

    let mut ordered = Vec::new();
    for (document_index, loaded) in documents.iter().enumerate() {
        for meta in &loaded.document.snapshots {
            ordered.push((
                document_index,
                meta.captured_at_millis,
                meta.snapshot_id.clone(),
            ));
        }
    }
    ordered.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| right.2.cmp(&left.2)));

    let mut keep_by_document: HashMap<usize, HashSet<String>> = HashMap::new();
    for (index, _, snapshot_id) in ordered.into_iter().take(retention.global_cap) {
        keep_by_document
            .entry(index)
            .or_default()
            .insert(snapshot_id);
    }

    for (index, loaded) in documents.iter_mut().enumerate() {
        let Some(keep_ids) = keep_by_document.get(&index) else {
            let _ = fs_mutate::remove_dir_all_if_exists(&loaded.dir);
            continue;
        };

        let removed: Vec<_> = loaded
            .document
            .snapshots
            .iter()
            .filter(|meta| !keep_ids.contains(&meta.snapshot_id))
            .cloned()
            .collect();
        if removed.is_empty() {
            continue;
        }

        loaded
            .document
            .snapshots
            .retain(|meta| keep_ids.contains(&meta.snapshot_id));
        if loaded.document.snapshots.is_empty() {
            let _ = fs_mutate::remove_dir_all_if_exists(&loaded.dir);
            continue;
        }

        save_document_index(&loaded.dir, &loaded.document)?;
        remove_snapshot_files(&loaded.dir, &removed);
    }

    Ok(())
}

fn load_all_documents_from_base(
    data_dir: &Path,
    base_dir: &Path,
) -> Result<Vec<LoadedHistoryDocument>> {
    let mut documents = Vec::new();
    for entry in fs_tree::scan_directory(base_dir, DirectoryScanPolicy::visible_workspace())
        .with_context(|| format!("failed to read {}", base_dir.display()))?
    {
        let path = entry.path;
        if entry.kind != FileKind::Directory {
            continue;
        }

        let load = load_history_index(data_dir, &path.join(INDEX_FILENAME));
        note_storage::trace_recovery_diagnostics(&load.diagnostics);
        let Some(mut document) = load.value else {
            continue;
        };
        document.sort_newest_first();
        documents.push(LoadedHistoryDocument {
            dir: path,
            document,
        });
    }
    Ok(documents)
}

fn migrate_loaded_document(
    data_dir: &Path,
    loaded: &mut LoadedHistoryDocument,
    new_identity: DocumentSidecarIdentity,
) -> Result<()> {
    let target_dir = document_dir(data_dir, &new_identity);
    if loaded.dir == target_dir {
        loaded.document.identity = new_identity;
        loaded.document.sort_newest_first();
        save_document_index(&loaded.dir, &loaded.document)?;
        return Ok(());
    }

    if !fs_metadata::path_status(&target_dir)?.is_present() {
        if let Some(parent) = target_dir.parent() {
            fs_write::create_dir_all_durable(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs_write::rename_durable(&loaded.dir, &target_dir).with_context(|| {
            format!(
                "failed to move {} to {}",
                loaded.dir.display(),
                target_dir.display()
            )
        })?;
        loaded.dir = target_dir;
        loaded.document.identity = new_identity;
        loaded.document.sort_newest_first();
        save_document_index(&loaded.dir, &loaded.document)?;
        return Ok(());
    }

    let target_index = target_dir.join(INDEX_FILENAME);
    let target_load = load_history_index(data_dir, &target_index);
    note_storage::trace_recovery_diagnostics(&target_load.diagnostics);
    let mut target_document = match target_load.value {
        Some(mut document) => {
            document.sort_newest_first();
            document
        }
        None => LocalHistoryDocument::empty(new_identity.clone()),
    };

    let mut migrated_snapshots = Vec::new();
    for meta in &loaded.document.snapshots {
        let from = snapshot_path(&loaded.dir, &meta.snapshot_id);
        let to = snapshot_path(&target_dir, &meta.snapshot_id);
        let source_present = fs_metadata::path_status(&from)?.is_present();
        let target_present = fs_metadata::path_status(&to)?.is_present();
        if !source_present && !target_present {
            continue;
        }
        if source_present && !target_present {
            // Prefer rename to preserve metadata. Copy fallback is safe because
            // the source lineage is deleted only after the target index has been
            // rewritten, leaving retryable evidence if cleanup later fails.
            fs_write::rename_durable(&from, &to)
                .or_else(|_| {
                    fs_write::copy_file_durable(&from, &to, WriteLabel::LOCAL_HISTORY_COPY)
                })
                .with_context(|| {
                    format!(
                        "failed to move snapshot {} to {}",
                        from.display(),
                        to.display()
                    )
                })?;
        }
        migrated_snapshots.push(meta.clone());
    }

    target_document.identity = new_identity;
    target_document.snapshots.extend(migrated_snapshots);
    deduplicate_snapshot_ids(&mut target_document.snapshots);
    target_document.sort_newest_first();
    trim_document_to_retention(&target_dir, &mut target_document, PER_DOCUMENT_SNAPSHOT_CAP);
    save_document_index(&target_dir, &target_document)?;
    remove_obsolete_lineage(&loaded.dir)?;
    loaded.dir = target_dir;
    loaded.document = target_document;
    Ok(())
}

fn remove_obsolete_lineage(path: &Path) -> Result<()> {
    maybe_fail_obsolete_lineage_cleanup_for_test(path)?;
    fs_mutate::remove_dir_all_if_exists(path)
        .map(|_| ())
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to remove obsolete local-history lineage {}: {}",
                path.display(),
                error
            )
        })
}

/// Inject the cleanup failure used by cross-environment retry tests.
#[cfg(any(test, feature = "test-utils"))]
fn maybe_fail_obsolete_lineage_cleanup_for_test(path: &Path) -> Result<()> {
    if FAIL_NEXT_OBSOLETE_LINEAGE_CLEANUP.swap(false, Ordering::AcqRel) {
        return Err(anyhow::anyhow!(
            "failed to remove obsolete local-history lineage {}: injected cleanup failure",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(any(test, feature = "test-utils")))]
fn maybe_fail_obsolete_lineage_cleanup_for_test(_path: &Path) -> Result<()> {
    Ok(())
}

fn is_obsolete_lineage_cleanup_error(error: &anyhow::Error) -> bool {
    error
        .to_string()
        .contains("failed to remove obsolete local-history lineage")
}

fn cleanup_empty_fallback_lineage(data_dir: &Path, old_path: &Path) -> Result<bool> {
    // Fallback identities used the displayed path as their canonical key. Only
    // an empty directory is removed here, so any non-empty or symlink-derived
    // evidence stays available for the general reconciliation pass.
    let fallback_identity =
        DocumentSidecarIdentity::from_paths(old_path.to_path_buf(), old_path.to_path_buf());
    let fallback_dir = document_dir(data_dir, &fallback_identity);
    remove_empty_lineage_dir_if_exists(&fallback_dir)
}

fn remove_empty_lineage_dir_if_exists(path: &Path) -> Result<bool> {
    if !fs_metadata::path_status(path)?.is_directory() {
        return Ok(false);
    }
    let entries = fs_tree::scan_directory(
        path,
        DirectoryScanPolicy {
            max_entries: 1,
            include_hidden: true,
        },
    )
    .with_context(|| format!("failed to inspect {}", path.display()))?;
    if !entries.is_empty() {
        return Ok(false);
    }
    remove_obsolete_lineage(path)?;
    Ok(true)
}

fn load_history_index(
    data_dir: &Path,
    index_path: &Path,
) -> crate::services::recovery_metadata::RecoveryLoad<Option<LocalHistoryDocument>> {
    note_storage::load_json_file_recovering(
        data_dir,
        index_path,
        RecoveryMetadataClass::LocalHistoryIndex,
    )
}

fn deduplicate_snapshot_ids(snapshots: &mut Vec<LocalHistorySnapshotMeta>) {
    let mut seen = HashSet::new();
    snapshots.retain(|meta| seen.insert(meta.snapshot_id.clone()));
}

fn remove_snapshot_files(document_dir: &Path, snapshots: &[LocalHistorySnapshotMeta]) {
    for meta in snapshots {
        let path = snapshot_path(document_dir, &meta.snapshot_id);
        if let Err(error) = fs_mutate::remove_file_if_exists(&path) {
            tracing::warn!(
                "Failed to delete pruned history snapshot {}: {error}",
                path.display()
            );
        }
    }
}

fn local_history_lock() -> &'static Mutex<()> {
    // Process-local mutex serializes read-modify-write sequences for history
    // indexes; `OnceLock` creates it lazily without global constructor order.
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;
    use crate::services::file_limits::FileSizeCheck;
    use crate::services::filesystem::fixture;

    fn seed_file(dir: &TempDir, rel: &str, content: &str) -> PathBuf {
        let path = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            fixture::create_dir_all(parent);
        }
        fixture::write_text(&path, content);
        path
    }

    fn stored_meta(outcome: LocalHistoryCaptureOutcome) -> LocalHistorySnapshotMeta {
        match outcome {
            LocalHistoryCaptureOutcome::Stored(meta) => meta,
            LocalHistoryCaptureOutcome::SkippedDuplicate => {
                panic!("capture should have stored a snapshot")
            }
        }
    }

    fn history_dir_for_path(data_dir: &Path, path: &Path) -> PathBuf {
        let identity = resolve_document_identity(path).expect("resolve identity");
        document_dir(data_dir, &identity)
    }

    fn seed_lineage_in_dir(document_dir: &Path, identity: DocumentSidecarIdentity, text: &str) {
        fixture::create_dir_all(document_dir);
        let meta = LocalHistorySnapshotMeta {
            snapshot_id: crate::model::sidecar_identity::next_record_id("history"),
            captured_at_millis: crate::model::sidecar_identity::now_epoch_millis(),
            origin: LocalHistorySnapshotOrigin::Save,
            byte_len: text.len() as u64,
            content_hash: stable_bytes_hash(text.as_bytes()),
        };
        fixture::write_text(&snapshot_path(document_dir, &meta.snapshot_id), text);
        save_document_index(
            document_dir,
            &LocalHistoryDocument {
                identity,
                snapshots: vec![meta],
            },
        )
        .expect("save seeded lineage index");
    }

    #[test]
    fn availability_policy_maps_file_sizes_to_capture_and_browse_modes() {
        let cases = [
            (
                FileSizeCheck::Normal,
                LocalHistoryAvailability::Full,
                true,
                true,
            ),
            (
                FileSizeCheck::LargeFileToast,
                LocalHistoryAvailability::Full,
                true,
                true,
            ),
            (
                FileSizeCheck::DisableSyntax,
                LocalHistoryAvailability::SaveOnly,
                false,
                true,
            ),
            (
                FileSizeCheck::DisableUndoAndSyntax,
                LocalHistoryAvailability::Unavailable,
                false,
                false,
            ),
            (
                FileSizeCheck::TooLarge,
                LocalHistoryAvailability::Unavailable,
                false,
                false,
            ),
        ];

        for (size_check, expected, allows_capture, allows_browsing) in cases {
            let availability = availability_for_size_check(size_check);

            assert_eq!(availability, expected);
            assert_eq!(
                availability.allows_automatic_capture(),
                allows_capture,
                "{size_check:?} automatic capture policy changed"
            );
            assert_eq!(
                availability.allows_browsing(),
                allows_browsing,
                "{size_check:?} browsing policy changed"
            );
        }
    }

    #[test]
    fn capture_snapshot_deduplicates_latest_text() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "one\n");

        let first = capture_snapshot_for_path_with_retention(
            dir.path(),
            &path,
            "one\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
            RetentionPolicy {
                per_document_cap: 4,
                global_cap: 8,
            },
        )
        .expect("capture first");
        let second = capture_snapshot_for_path_with_retention(
            dir.path(),
            &path,
            "one\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
            RetentionPolicy {
                per_document_cap: 4,
                global_cap: 8,
            },
        )
        .expect("capture duplicate");

        assert_matches!(first, LocalHistoryCaptureOutcome::Stored(_));
        assert_eq!(second, LocalHistoryCaptureOutcome::SkippedDuplicate);
        assert_eq!(
            list_snapshots_for_path(dir.path(), &path)
                .expect("list snapshots")
                .len(),
            1
        );
    }

    #[test]
    fn capture_snapshot_normalizes_carriage_returns_before_storing() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "one\n");

        let outcome = capture_snapshot_for_path(
            dir.path(),
            &path,
            "one\r\ntwo\rthree\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
        )
        .expect("capture snapshot");
        let meta = stored_meta(outcome);
        let loaded = load_snapshot_for_path(dir.path(), &path, &meta.snapshot_id)
            .expect("load snapshot")
            .expect("snapshot should exist");

        assert_eq!(loaded.text, "one\ntwo\nthree\n");
        assert_eq!(loaded.meta.byte_len, "one\ntwo\nthree\n".len() as u64);
        assert_eq!(
            loaded.meta.content_hash,
            stable_bytes_hash(b"one\ntwo\nthree\n")
        );
    }

    #[test]
    fn capture_snapshot_orders_newest_first() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "one\n");

        capture_snapshot_for_path(
            dir.path(),
            &path,
            "one\n",
            LocalHistorySnapshotOrigin::Baseline,
            LocalHistoryCapturePolicy::DeduplicateLatest,
        )
        .expect("capture baseline");
        std::thread::sleep(Duration::from_millis(2));
        capture_snapshot_for_path(
            dir.path(),
            &path,
            "two\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
        )
        .expect("capture save");

        let snapshots = list_snapshots_for_path(dir.path(), &path).expect("list snapshots");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].origin, LocalHistorySnapshotOrigin::Save);
        assert_eq!(snapshots[1].origin, LocalHistorySnapshotOrigin::Baseline);
    }

    #[test]
    fn corrupt_index_is_repaired_from_snapshot_text_without_deleting_body() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "one\n");
        let meta = stored_meta(
            capture_snapshot_for_path(
                dir.path(),
                &path,
                "recoverable body\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
            )
            .expect("capture snapshot"),
        );
        let doc_dir = history_dir_for_path(dir.path(), &path);
        let index_path = doc_dir.join(INDEX_FILENAME);
        let snapshot_file = snapshot_path(&doc_dir, &meta.snapshot_id);
        fixture::write_text(&index_path, "not local-history json");

        let listing =
            list_snapshots_for_path_recovering(dir.path(), &path).expect("list snapshots");

        assert_eq!(listing.snapshots.len(), 1);
        assert_eq!(listing.snapshots[0].snapshot_id, meta.snapshot_id);
        assert_eq!(
            listing.snapshots[0].origin,
            LocalHistorySnapshotOrigin::Recovered
        );
        assert_eq!(
            listing.snapshots[0].byte_len,
            "recoverable body\n".len() as u64
        );
        assert_eq!(
            listing.snapshots[0].content_hash,
            stable_bytes_hash(b"recoverable body\n")
        );
        assert!(
            listing
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.summary().contains("repaired")),
            "repair should be diagnostic"
        );
        assert!(
            fs_metadata::exists(&snapshot_file),
            "snapshot text must survive corrupt index recovery"
        );
        assert!(
            fs_metadata::exists(&index_path),
            "a repaired index should be durably written"
        );
    }

    #[test]
    fn load_snapshot_refuses_body_above_browse_limit_before_reading() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "one\n");
        let meta = stored_meta(
            capture_snapshot_for_path(
                dir.path(),
                &path,
                "small body\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
            )
            .expect("capture snapshot"),
        );
        let doc_dir = history_dir_for_path(dir.path(), &path);
        let snapshot_file = snapshot_path(&doc_dir, &meta.snapshot_id);
        fixture::create_sparse_file(&snapshot_file, DISABLE_UNDO_HISTORY + 1);

        let error = load_snapshot_for_path(dir.path(), &path, &meta.snapshot_id)
            .expect_err("oversized snapshot body should be refused before read");

        assert!(
            error.to_string().contains("browse limit"),
            "unexpected oversized snapshot error: {error}"
        );
        assert!(
            fs_metadata::exists(&snapshot_file),
            "oversized snapshot evidence should remain on disk"
        );
    }

    #[test]
    fn corrupt_index_repair_skips_oversized_snapshot_body_without_reading() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "one\n");
        let meta = stored_meta(
            capture_snapshot_for_path(
                dir.path(),
                &path,
                "recoverable body\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
            )
            .expect("capture snapshot"),
        );
        let doc_dir = history_dir_for_path(dir.path(), &path);
        let index_path = doc_dir.join(INDEX_FILENAME);
        let snapshot_file = snapshot_path(&doc_dir, &meta.snapshot_id);
        fixture::create_sparse_file(&snapshot_file, DISABLE_UNDO_HISTORY + 1);
        fixture::write_text(&index_path, "not local-history json");

        let listing =
            list_snapshots_for_path_recovering(dir.path(), &path).expect("list snapshots");

        assert!(
            listing.snapshots.is_empty(),
            "oversized repair should not expose guessed history"
        );
        assert!(
            listing.diagnostics.iter().any(|diagnostic| matches!(
                &diagnostic.problem,
                crate::services::recovery_metadata::RecoveryProblem::RepairSkipped { detail }
                    if detail.contains("browse limit")
            )),
            "oversized repair skip should preserve the detailed reason"
        );
        assert!(
            fs_metadata::exists(&snapshot_file),
            "oversized snapshot text must remain for manual inspection"
        );
    }

    #[test]
    fn ambiguous_corrupt_index_repair_preserves_snapshot_text_without_exposing() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "one\n");
        let meta = stored_meta(
            capture_snapshot_for_path(
                dir.path(),
                &path,
                "ambiguous body\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
            )
            .expect("capture snapshot"),
        );
        let doc_dir = history_dir_for_path(dir.path(), &path);
        let index_path = doc_dir.join(INDEX_FILENAME);
        let snapshot_file = snapshot_path(&doc_dir, &meta.snapshot_id);
        let ambiguous_snapshot = doc_dir.join("snapshot-without-time.txt");
        fixture::rename(&snapshot_file, &ambiguous_snapshot);
        fixture::write_text(&index_path, "not local-history json");

        let listing =
            list_snapshots_for_path_recovering(dir.path(), &path).expect("list snapshots");

        assert!(
            listing.snapshots.is_empty(),
            "ambiguous repair should not expose guessed history"
        );
        assert!(
            listing
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.summary().contains("repair-skipped")),
            "skipped repair should be diagnostic"
        );
        assert!(
            fs_metadata::exists(&ambiguous_snapshot),
            "ambiguous snapshot text must remain for manual inspection"
        );
    }

    #[test]
    fn retention_prunes_per_document_cap_and_snapshot_file() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "one\n");
        let retention = RetentionPolicy {
            per_document_cap: 2,
            global_cap: 10,
        };

        let first_meta = stored_meta(
            capture_snapshot_for_path_with_retention(
                dir.path(),
                &path,
                "v1\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
                retention,
            )
            .expect("capture first"),
        );
        std::thread::sleep(Duration::from_millis(2));
        capture_snapshot_for_path_with_retention(
            dir.path(),
            &path,
            "v2\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
            retention,
        )
        .expect("capture second");
        std::thread::sleep(Duration::from_millis(2));
        capture_snapshot_for_path_with_retention(
            dir.path(),
            &path,
            "v3\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
            retention,
        )
        .expect("capture third");

        let snapshots = list_snapshots_for_path(dir.path(), &path).expect("list snapshots");
        let doc_dir = history_dir_for_path(dir.path(), &path);

        assert_eq!(snapshots.len(), 2);
        assert!(
            !snapshots
                .iter()
                .any(|meta| meta.snapshot_id == first_meta.snapshot_id),
            "oldest metadata should be trimmed"
        );
        assert!(
            !fs_metadata::exists(&snapshot_path(&doc_dir, &first_meta.snapshot_id)),
            "oldest snapshot file should be deleted with its metadata"
        );
    }

    #[test]
    fn retention_prunes_global_cap_across_documents() {
        let dir = TempDir::new().expect("tempdir");
        let first = seed_file(&dir, "workspace/a.txt", "a0\n");
        let second = seed_file(&dir, "workspace/b.txt", "b0\n");
        let third = seed_file(&dir, "workspace/c.txt", "c0\n");
        let retention = RetentionPolicy {
            per_document_cap: 10,
            global_cap: 2,
        };

        let first_meta = stored_meta(
            capture_snapshot_for_path_with_retention(
                dir.path(),
                &first,
                "a1\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
                retention,
            )
            .expect("capture a1"),
        );
        std::thread::sleep(Duration::from_millis(2));
        let second_meta = stored_meta(
            capture_snapshot_for_path_with_retention(
                dir.path(),
                &second,
                "b1\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
                retention,
            )
            .expect("capture b1"),
        );
        std::thread::sleep(Duration::from_millis(2));
        let third_meta = stored_meta(
            capture_snapshot_for_path_with_retention(
                dir.path(),
                &third,
                "c1\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
                retention,
            )
            .expect("capture c1"),
        );

        let first_snapshots = list_snapshots_for_path(dir.path(), &first).expect("list a");
        let second_snapshots = list_snapshots_for_path(dir.path(), &second).expect("list b");
        let third_snapshots = list_snapshots_for_path(dir.path(), &third).expect("list c");
        let first_doc_dir = history_dir_for_path(dir.path(), &first);

        assert!(
            first_snapshots.is_empty(),
            "oldest document should be pruned"
        );
        assert_eq!(second_snapshots.len(), 1);
        assert_eq!(third_snapshots.len(), 1);
        assert!(
            !fs_metadata::exists(&first_doc_dir),
            "empty pruned lineage should be removed"
        );
        assert_eq!(stable_bytes_hash(b"b1\n"), second_snapshots[0].content_hash);
        assert_eq!(stable_bytes_hash(b"c1\n"), third_snapshots[0].content_hash);
        assert!(!fs_metadata::exists(&snapshot_path(
            &first_doc_dir,
            &first_meta.snapshot_id
        )));
        assert_eq!(
            load_snapshot_for_path(dir.path(), &second, &second_meta.snapshot_id)
                .expect("load kept second")
                .expect("second snapshot should remain")
                .text,
            "b1\n"
        );
        assert_eq!(
            load_snapshot_for_path(dir.path(), &third, &third_meta.snapshot_id)
                .expect("load kept third")
                .expect("third snapshot should remain")
                .text,
            "c1\n"
        );
    }

    #[test]
    fn retention_prunes_per_document_and_global_caps() {
        let dir = TempDir::new().expect("tempdir");
        let first = seed_file(&dir, "workspace/a.txt", "a0\n");
        let second = seed_file(&dir, "workspace/b.txt", "b0\n");
        let retention = RetentionPolicy {
            per_document_cap: 2,
            global_cap: 3,
        };

        capture_snapshot_for_path_with_retention(
            dir.path(),
            &first,
            "a1\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
            retention,
        )
        .expect("capture a1");
        std::thread::sleep(Duration::from_millis(2));
        capture_snapshot_for_path_with_retention(
            dir.path(),
            &first,
            "a2\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
            retention,
        )
        .expect("capture a2");
        std::thread::sleep(Duration::from_millis(2));
        capture_snapshot_for_path_with_retention(
            dir.path(),
            &first,
            "a3\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
            retention,
        )
        .expect("capture a3");
        std::thread::sleep(Duration::from_millis(2));
        capture_snapshot_for_path_with_retention(
            dir.path(),
            &second,
            "b1\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
            retention,
        )
        .expect("capture b1");

        let first_snapshots = list_snapshots_for_path(dir.path(), &first).expect("list a");
        let second_snapshots = list_snapshots_for_path(dir.path(), &second).expect("list b");

        assert_eq!(first_snapshots.len(), 2, "per-document cap should trim a1");
        assert_eq!(
            second_snapshots.len(),
            1,
            "global cap should keep newest b1"
        );
        assert_eq!(
            first_snapshots[0].content_hash,
            stable_bytes_hash(b"a3\n"),
            "newest entry should stay first"
        );
        assert!(
            !first_snapshots
                .iter()
                .any(|meta| meta.content_hash == stable_bytes_hash(b"a1\n")),
            "oldest snapshot should be pruned"
        );
    }

    #[test]
    fn move_path_tree_preserves_history_lineage_after_rename() {
        let dir = TempDir::new().expect("tempdir");
        let old_path = seed_file(&dir, "workspace/old.txt", "old\n");

        capture_snapshot_for_path(
            dir.path(),
            &old_path,
            "version one\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
        )
        .expect("capture history");

        let new_path = dir.path().join("workspace/new.txt");
        fixture::rename(&old_path, &new_path);
        let migrated = move_path_tree(dir.path(), &old_path, &new_path).expect("move tree");

        assert_eq!(migrated, 1);
        let snapshots = list_snapshots_for_path(dir.path(), &new_path).expect("list renamed");
        assert_eq!(snapshots.len(), 1);
        let loaded = load_snapshot_for_path(dir.path(), &new_path, &snapshots[0].snapshot_id)
            .expect("load renamed")
            .expect("snapshot should exist");
        assert_eq!(loaded.text, "version one\n");
    }

    #[test]
    fn move_path_tree_merges_existing_target_and_skips_missing_source_files() {
        let dir = TempDir::new().expect("tempdir");
        let old_path = seed_file(&dir, "workspace/old.txt", "old\n");
        let new_path = seed_file(&dir, "workspace/new.txt", "new\n");

        let moved_meta = stored_meta(
            capture_snapshot_for_path(
                dir.path(),
                &old_path,
                "moved body\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
            )
            .expect("capture moved snapshot"),
        );
        std::thread::sleep(Duration::from_millis(2));
        let missing_meta = stored_meta(
            capture_snapshot_for_path(
                dir.path(),
                &old_path,
                "missing body\n",
                LocalHistorySnapshotOrigin::Periodic,
                LocalHistoryCapturePolicy::DeduplicateLatest,
            )
            .expect("capture missing snapshot"),
        );
        capture_snapshot_for_path(
            dir.path(),
            &new_path,
            "target body\n",
            LocalHistorySnapshotOrigin::Baseline,
            LocalHistoryCapturePolicy::DeduplicateLatest,
        )
        .expect("capture target snapshot");

        let old_doc_dir = history_dir_for_path(dir.path(), &old_path);
        fixture::remove_file(&snapshot_path(&old_doc_dir, &missing_meta.snapshot_id));

        let migrated = move_path_tree(dir.path(), &old_path, &new_path).expect("move tree");

        assert_eq!(migrated, 1);
        assert!(
            !fs_metadata::exists(&old_doc_dir),
            "source lineage should be removed"
        );
        let snapshots = list_snapshots_for_path(dir.path(), &new_path).expect("list merged");
        assert!(
            snapshots
                .iter()
                .any(|meta| meta.snapshot_id == moved_meta.snapshot_id),
            "metadata for moved snapshot should be merged"
        );
        assert!(
            !snapshots
                .iter()
                .any(|meta| meta.snapshot_id == missing_meta.snapshot_id),
            "metadata without a surviving snapshot body should not be merged"
        );
        let loaded = load_snapshot_for_path(dir.path(), &new_path, &moved_meta.snapshot_id)
            .expect("load moved snapshot")
            .expect("moved snapshot should exist");
        assert_eq!(loaded.text, "moved body\n");
    }

    #[test]
    fn reconcile_lineages_moves_mismatched_index_into_canonical_directory() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "file\n");
        let identity = resolve_document_identity(&path).expect("identity");
        let mismatched_dir = local_history_dir(dir.path()).join("stale-lineage");
        seed_lineage_in_dir(&mismatched_dir, identity.clone(), "recovered body\n");

        let report = reconcile_lineages_with_budget(
            dir.path(),
            LocalHistoryReconcileBudget::new(8, Duration::from_secs(60)),
        )
        .expect("reconcile lineages");

        assert_eq!(report.scanned_lineages, 1);
        assert_eq!(report.mismatched_lineages, 1);
        assert_eq!(report.reconciled_lineages, 1);
        assert!(!fs_metadata::exists(&mismatched_dir));
        let snapshots = list_snapshots_for_path(dir.path(), &path).expect("list reconciled");
        assert_eq!(snapshots.len(), 1);
        let loaded = load_snapshot_for_path(dir.path(), &path, &snapshots[0].snapshot_id)
            .expect("load reconciled")
            .expect("snapshot exists");
        assert_eq!(loaded.text, "recovered body\n");
        assert_eq!(
            history_dir_for_path(dir.path(), &path),
            document_dir(dir.path(), &identity)
        );
    }

    #[test]
    fn reconcile_lineages_merges_duplicate_identity_before_cleanup() {
        let dir = TempDir::new().expect("tempdir");
        let path = seed_file(&dir, "workspace/file.txt", "file\n");
        capture_snapshot_for_path(
            dir.path(),
            &path,
            "target body\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::PreserveDuplicate,
        )
        .expect("capture target");
        let identity = resolve_document_identity(&path).expect("identity");
        let duplicate_dir = local_history_dir(dir.path()).join("duplicate-lineage");
        seed_lineage_in_dir(&duplicate_dir, identity, "duplicate body\n");

        let report = reconcile_lineages_with_budget(
            dir.path(),
            LocalHistoryReconcileBudget::new(8, Duration::from_secs(60)),
        )
        .expect("reconcile duplicate");

        assert_eq!(report.mismatched_lineages, 1);
        assert_eq!(report.reconciled_lineages, 1);
        assert!(!fs_metadata::exists(&duplicate_dir));
        let snapshots = list_snapshots_for_path(dir.path(), &path).expect("list merged");
        assert_eq!(snapshots.len(), 2);
        let bodies = snapshots
            .iter()
            .map(|meta| {
                load_snapshot_for_path(dir.path(), &path, &meta.snapshot_id)
                    .expect("load snapshot")
                    .expect("snapshot exists")
                    .text
            })
            .collect::<Vec<_>>();
        assert!(bodies.iter().any(|body| body == "target body\n"));
        assert!(bodies.iter().any(|body| body == "duplicate body\n"));
    }

    #[test]
    fn reconcile_lineages_preserves_orphan_directory_and_reports_diagnostic() {
        let dir = TempDir::new().expect("tempdir");
        let orphan_dir = local_history_dir(dir.path()).join("orphan-lineage");
        fixture::create_dir_all(&orphan_dir);
        fixture::write_text(&orphan_dir.join("history-without-index.txt"), "body\n");

        let report = reconcile_lineages_with_budget(
            dir.path(),
            LocalHistoryReconcileBudget::new(8, Duration::from_secs(60)),
        )
        .expect("reconcile orphan");

        assert_eq!(report.scanned_lineages, 1);
        assert_eq!(report.orphaned_lineages, 1);
        assert!(
            fs_metadata::exists(&orphan_dir),
            "orphaned evidence must remain on disk"
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.summary().contains("repair-skipped")),
            "orphan should be diagnostic"
        );
    }

    #[test]
    fn reconcile_lineages_defers_work_when_scan_budget_is_reached() {
        let dir = TempDir::new().expect("tempdir");
        for index in 0..3 {
            let path = seed_file(&dir, &format!("workspace/file-{index}.txt"), "file\n");
            let identity = resolve_document_identity(&path).expect("identity");
            let mismatched_dir = local_history_dir(dir.path()).join(format!("stale-{index}"));
            seed_lineage_in_dir(&mismatched_dir, identity, "body\n");
        }

        let report = reconcile_lineages_with_budget(
            dir.path(),
            LocalHistoryReconcileBudget::new(1, Duration::from_secs(60)),
        )
        .expect("bounded reconcile");

        assert_eq!(report.scanned_lineages, 1);
        assert!(report.has_deferred_work());
    }

    #[test]
    fn move_path_tree_reports_cleanup_failure_after_target_write() {
        let dir = TempDir::new().expect("tempdir");
        let old_path = seed_file(&dir, "workspace/old.txt", "old\n");
        let new_path = seed_file(&dir, "workspace/new.txt", "new\n");
        let moved_meta = stored_meta(
            capture_snapshot_for_path(
                dir.path(),
                &old_path,
                "moved body\n",
                LocalHistorySnapshotOrigin::Save,
                LocalHistoryCapturePolicy::DeduplicateLatest,
            )
            .expect("capture source"),
        );
        capture_snapshot_for_path(
            dir.path(),
            &new_path,
            "target body\n",
            LocalHistorySnapshotOrigin::Save,
            LocalHistoryCapturePolicy::DeduplicateLatest,
        )
        .expect("capture target");
        let old_doc_dir = history_dir_for_path(dir.path(), &old_path);

        fail_next_obsolete_lineage_cleanup_for_test();
        let result = move_path_tree(dir.path(), &old_path, &new_path);
        let error = result.expect_err("source cleanup should fail");

        assert!(
            error
                .to_string()
                .contains("failed to remove obsolete local-history lineage"),
            "unexpected error: {error}"
        );
        assert!(
            fs_metadata::exists(&old_doc_dir),
            "cleanup failure should leave source lineage for retry"
        );
        let snapshots = list_snapshots_for_path(dir.path(), &new_path).expect("list target");
        assert!(
            snapshots
                .iter()
                .any(|meta| meta.snapshot_id == moved_meta.snapshot_id),
            "target index should already contain the moved snapshot before cleanup"
        );
    }

    #[test]
    fn deduplicate_snapshot_ids_keeps_first_seen_metadata() {
        let mut snapshots = vec![
            LocalHistorySnapshotMeta {
                snapshot_id: "history-a".to_string(),
                captured_at_millis: 30,
                origin: LocalHistorySnapshotOrigin::Save,
                byte_len: 3,
                content_hash: "first".to_string(),
            },
            LocalHistorySnapshotMeta {
                snapshot_id: "history-b".to_string(),
                captured_at_millis: 20,
                origin: LocalHistorySnapshotOrigin::Baseline,
                byte_len: 3,
                content_hash: "second".to_string(),
            },
            LocalHistorySnapshotMeta {
                snapshot_id: "history-a".to_string(),
                captured_at_millis: 10,
                origin: LocalHistorySnapshotOrigin::Periodic,
                byte_len: 3,
                content_hash: "duplicate".to_string(),
            },
        ];

        deduplicate_snapshot_ids(&mut snapshots);

        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].snapshot_id, "history-a");
        assert_eq!(snapshots[0].content_hash, "first");
        assert_eq!(snapshots[1].snapshot_id, "history-b");
    }

    #[test]
    fn remove_snapshot_files_deletes_present_files_and_ignores_missing() {
        let dir = TempDir::new().expect("tempdir");
        let present = LocalHistorySnapshotMeta {
            snapshot_id: "history-present".to_string(),
            captured_at_millis: 1,
            origin: LocalHistorySnapshotOrigin::Save,
            byte_len: 4,
            content_hash: "present".to_string(),
        };
        let missing = LocalHistorySnapshotMeta {
            snapshot_id: "history-missing".to_string(),
            captured_at_millis: 2,
            origin: LocalHistorySnapshotOrigin::Save,
            byte_len: 4,
            content_hash: "missing".to_string(),
        };
        let present_path = snapshot_path(dir.path(), &present.snapshot_id);
        fixture::write_text(&present_path, "body");

        remove_snapshot_files(dir.path(), &[present, missing]);

        assert!(!fs_metadata::exists(&present_path));
    }

    #[test]
    fn local_history_lock_is_singleton() {
        assert!(std::ptr::eq(local_history_lock(), local_history_lock()));
    }
}

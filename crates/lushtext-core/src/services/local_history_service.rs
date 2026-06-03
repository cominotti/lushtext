// SPDX-License-Identifier: GPL-3.0-or-later

//! Local-history persistence helpers for saved documents.
//!
//! This service owns the filesystem-facing local-history workflow: resolve
//! stable saved-file identity, write normalized full-text snapshots, prune old
//! snapshots, migrate lineages after in-app renames, and load snapshot metadata
//! or bodies for the browser UI. Everything here stays GTK-free so capture and
//! browse work can run on background threads.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::model::local_history::{
    LocalHistoryDocument, LocalHistorySnapshot, LocalHistorySnapshotMeta,
    LocalHistorySnapshotOrigin,
};
use crate::model::sidecar_identity::{DocumentSidecarIdentity, stable_bytes_hash};
use crate::services::{
    editor_io,
    file_limits::FileSizeCheck,
    filesystem::{
        DirectoryScanPolicy, FileKind, WriteLabel, metadata as fs_metadata, mutate as fs_mutate,
        read as fs_read, tree as fs_tree, write as fs_write,
    },
    json_store,
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
    let display_path = path.to_path_buf();
    let canonical_path = fs_metadata::canonical_path(path)
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    Ok(DocumentSidecarIdentity::from_paths(
        display_path,
        canonical_path,
    ))
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
/// Returns an error if the document identity cannot be resolved or the stored
/// metadata cannot be read.
pub fn list_snapshots_for_path(
    data_dir: &Path,
    path: &Path,
) -> Result<Vec<LocalHistorySnapshotMeta>> {
    let _guard = local_history_lock()
        .lock()
        .map_err(|_| anyhow::anyhow!("local-history lock poisoned"))?;
    let identity = resolve_document_identity(path)?;
    let document = load_document_for_identity(data_dir, identity)?;
    Ok(document.snapshots)
}

/// Load one snapshot body for the saved document.
///
/// # Errors
///
/// Returns an error if the document identity cannot be resolved, metadata
/// cannot be read, or the selected snapshot file cannot be read as UTF-8.
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
    let text = fs_read::text(&snapshot_path)
        .with_context(|| format!("failed to read {}", snapshot_path.display()))?;
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
    if fs_metadata::file_facts(&base_dir).is_err() {
        return Ok(0);
    }

    let mut migrated = 0usize;
    let mut loaded_documents = load_all_documents_from_base(&base_dir)?;
    for loaded in &mut loaded_documents {
        let Some((display_path, canonical_path)) =
            rebase_identity_paths(&loaded.document.identity, old_path, new_path)
        else {
            continue;
        };

        let new_identity = DocumentSidecarIdentity::from_paths(display_path, canonical_path);
        migrate_loaded_document(data_dir, loaded, new_identity)?;
        migrated += 1;
    }

    enforce_global_retention_locked(data_dir, DEFAULT_RETENTION_POLICY)?;
    Ok(migrated)
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

    let meta = LocalHistorySnapshotMeta::new(origin, normalized.len() as u64, content_hash.clone());
    let doc_dir = document_dir(data_dir, &identity);
    fs_write::create_dir_all_durable(&doc_dir)
        .with_context(|| format!("failed to create {}", doc_dir.display()))?;
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

fn load_document_for_identity(
    data_dir: &Path,
    identity: DocumentSidecarIdentity,
) -> Result<LocalHistoryDocument> {
    let dir = document_dir(data_dir, &identity);
    match load_json_file::<LocalHistoryDocument>(&dir.join(INDEX_FILENAME))? {
        Some(mut document) => {
            document.sort_newest_first();
            Ok(document)
        }
        None => Ok(LocalHistoryDocument::empty(identity)),
    }
}

fn document_dir(data_dir: &Path, identity: &DocumentSidecarIdentity) -> PathBuf {
    local_history_dir(data_dir).join(&identity.sidecar_id)
}

fn snapshot_path(document_dir: &Path, snapshot_id: &str) -> PathBuf {
    document_dir.join(format!("{snapshot_id}.{SNAPSHOT_EXTENSION}"))
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
    if fs_metadata::file_facts(&base_dir).is_err() {
        return Ok(());
    }

    let mut documents = load_all_documents_from_base(&base_dir)?;
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

fn load_all_documents_from_base(base_dir: &Path) -> Result<Vec<LoadedHistoryDocument>> {
    let mut documents = Vec::new();
    for entry in fs_tree::scan_directory(base_dir, DirectoryScanPolicy::visible_workspace())
        .with_context(|| format!("failed to read {}", base_dir.display()))?
    {
        let path = entry.path;
        if entry.kind != FileKind::Directory {
            continue;
        }

        let Some(mut document) =
            load_json_file::<LocalHistoryDocument>(&path.join(INDEX_FILENAME))?
        else {
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

    if fs_metadata::file_facts(&target_dir).is_err() {
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

    let mut target_document =
        match load_json_file::<LocalHistoryDocument>(&target_dir.join(INDEX_FILENAME))? {
            Some(mut document) => {
                document.sort_newest_first();
                document
            }
            None => LocalHistoryDocument::empty(new_identity.clone()),
        };

    for meta in &loaded.document.snapshots {
        let from = snapshot_path(&loaded.dir, &meta.snapshot_id);
        let to = snapshot_path(&target_dir, &meta.snapshot_id);
        if fs_metadata::file_facts(&from).is_err() || fs_metadata::file_facts(&to).is_ok() {
            continue;
        }
        fs_write::rename_durable(&from, &to)
            .or_else(|_| fs_write::copy_file_durable(&from, &to, WriteLabel::LOCAL_HISTORY_COPY))
            .with_context(|| {
                format!(
                    "failed to move snapshot {} to {}",
                    from.display(),
                    to.display()
                )
            })?;
    }

    target_document.identity = new_identity;
    target_document
        .snapshots
        .extend(loaded.document.snapshots.iter().cloned());
    deduplicate_snapshot_ids(&mut target_document.snapshots);
    target_document.sort_newest_first();
    trim_document_to_retention(&target_dir, &mut target_document, PER_DOCUMENT_SNAPSHOT_CAP);
    save_document_index(&target_dir, &target_document)?;
    let _ = fs_mutate::remove_dir_all_if_exists(&loaded.dir);
    loaded.dir = target_dir;
    loaded.document = target_document;
    Ok(())
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

fn rebase_identity_paths(
    identity: &DocumentSidecarIdentity,
    old_path: &Path,
    new_path: &Path,
) -> Option<(PathBuf, PathBuf)> {
    if identity.display_path == old_path || identity.display_path.starts_with(old_path) {
        let suffix = identity
            .display_path
            .strip_prefix(old_path)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_default();
        let display_path = if suffix.as_os_str().is_empty() {
            new_path.to_path_buf()
        } else {
            new_path.join(suffix)
        };
        let canonical_path =
            fs_metadata::canonical_path(&display_path).unwrap_or_else(|_| display_path.clone());
        return Some((display_path, canonical_path));
    }

    if identity.canonical_path == old_path || identity.canonical_path.starts_with(old_path) {
        let suffix = identity
            .canonical_path
            .strip_prefix(old_path)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_default();
        let display_path = if suffix.as_os_str().is_empty() {
            new_path.to_path_buf()
        } else {
            new_path.join(suffix)
        };
        let canonical_path =
            fs_metadata::canonical_path(&display_path).unwrap_or_else(|_| display_path.clone());
        return Some((display_path, canonical_path));
    }

    None
}

fn load_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs_read::bytes(path) {
        Ok(bytes) => {
            let value = serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            Ok(Some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(anyhow::anyhow!(
            "failed to read {}: {}",
            path.display(),
            error
        )),
    }
}

fn local_history_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
mod tests {
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

        assert!(matches!(first, LocalHistoryCaptureOutcome::Stored(_)));
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
            fs_metadata::file_facts(&snapshot_path(&doc_dir, &first_meta.snapshot_id)).is_err(),
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
            fs_metadata::file_facts(&first_doc_dir).is_err(),
            "empty pruned lineage should be removed"
        );
        assert_eq!(stable_bytes_hash(b"b1\n"), second_snapshots[0].content_hash);
        assert_eq!(stable_bytes_hash(b"c1\n"), third_snapshots[0].content_hash);
        assert!(
            fs_metadata::file_facts(&snapshot_path(&first_doc_dir, &first_meta.snapshot_id))
                .is_err()
        );
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
            fs_metadata::file_facts(&old_doc_dir).is_err(),
            "source lineage should be removed"
        );
        let snapshots = list_snapshots_for_path(dir.path(), &new_path).expect("list merged");
        assert!(
            snapshots
                .iter()
                .any(|meta| meta.snapshot_id == moved_meta.snapshot_id),
            "metadata for moved snapshot should be merged"
        );
        let loaded = load_snapshot_for_path(dir.path(), &new_path, &moved_meta.snapshot_id)
            .expect("load moved snapshot")
            .expect("moved snapshot should exist");
        assert_eq!(loaded.text, "moved body\n");
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

        assert!(fs_metadata::file_facts(&present_path).is_err());
    }

    #[test]
    fn rebase_identity_paths_handles_display_and_canonical_prefixes() {
        let old_root = Path::new("/project/old");
        let new_root = Path::new("/project/new");
        let display_nested = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/project/old/src/file.txt"),
            PathBuf::from("/canonical/elsewhere/file.txt"),
        );
        let canonical_nested = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/visible/elsewhere/file.txt"),
            PathBuf::from("/project/old/src/file.txt"),
        );
        let unrelated = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/project/other/file.txt"),
            PathBuf::from("/canonical/other/file.txt"),
        );

        let (display_path, canonical_path) =
            rebase_identity_paths(&display_nested, old_root, new_root)
                .expect("display path should rebase");
        assert_eq!(display_path, PathBuf::from("/project/new/src/file.txt"));
        assert_eq!(canonical_path, PathBuf::from("/project/new/src/file.txt"));

        let (display_path, canonical_path) =
            rebase_identity_paths(&canonical_nested, old_root, new_root)
                .expect("canonical path should rebase");
        assert_eq!(display_path, PathBuf::from("/project/new/src/file.txt"));
        assert_eq!(canonical_path, PathBuf::from("/project/new/src/file.txt"));

        assert!(rebase_identity_paths(&unrelated, old_root, new_root).is_none());
    }

    #[test]
    fn load_json_file_reports_non_missing_read_errors() {
        let dir = TempDir::new().expect("tempdir");

        let error = load_json_file::<serde_json::Value>(dir.path()).expect_err("directory read");

        assert!(
            error.to_string().contains("failed to read"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn local_history_lock_is_singleton() {
        assert!(std::ptr::eq(local_history_lock(), local_history_lock()));
    }
}

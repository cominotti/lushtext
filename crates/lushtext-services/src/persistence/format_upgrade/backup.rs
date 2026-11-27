// SPDX-License-Identifier: GPL-3.0-or-later

//! Backup manifest and preservation helpers for format-upgrade actions.
//!
//! Backups live under the app data directory so conversion and Start Fresh
//! never touch user document folders. The manifest records app-data-relative
//! paths and enough classification detail to support retry and support triage.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::filesystem::{
    PathStatus, WriteLabel, metadata as fs_metadata, mutate as fs_mutate, read as fs_read,
    write as fs_write,
};
use crate::persistence::app_data_layout::AppDataLocation;
use crate::persistence::json_format::{JsonEnvelopeRef, KIND_FORMAT_UPGRADE_BACKUP_MANIFEST};
use lushtext_model::persistence::sidecar_identity::stable_path_hash;

use super::diagnostics::FormatClassification;
use super::inventory::FormatInventoryItem;

/// Typed errors returned when creating or executing format-upgrade backups.
#[derive(Debug, thiserror::Error)]
pub enum FormatBackupError {
    /// Creating a backup directory failed.
    #[error("failed to create {}: {source}", path.display())]
    CreateDir {
        /// Directory path that could not be created.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Every candidate timestamped backup directory attempt already existed.
    #[error("failed to reserve a unique format-upgrade backup directory after {attempts} attempts")]
    BackupDirAttemptsExhausted {
        /// Maximum directory-reservation attempts tried.
        attempts: u32,
    },
    /// Copying a scanned item into the backup directory failed.
    #[error("failed to back up {}: {source}", path.display())]
    WriteBackup {
        /// Original item path being backed up.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Writing the durable backup manifest failed.
    #[error("failed to write {}: {source}", path.display())]
    WriteManifest {
        /// Backup manifest path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Inspecting an original item before backup or removal failed.
    #[error("failed to inspect {}: {source}", path.display())]
    Inspect {
        /// Item path being inspected.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// An item path was no longer a regular file.
    #[error("expected regular file, found {status:?}")]
    NotRegularFile {
        /// Item path that was not a regular file.
        path: PathBuf,
        /// Observed path status.
        status: PathStatus,
    },
    /// The item changed on disk after the format-upgrade scan captured its facts.
    #[error("{relative_path} changed after the format scan; scan again before applying")]
    ChangedAfterScan {
        /// App-data-relative path of the changed item.
        relative_path: String,
    },
    /// The item exceeded the maximum byte size admitted for backup reads.
    #[error("{relative_path} is {byte_size} bytes, above the {limit} byte backup limit")]
    BackupLimitExceeded {
        /// App-data-relative path of the oversized item.
        relative_path: String,
        /// Observed byte size of the item.
        byte_size: u64,
        /// Configured backup byte limit.
        limit: u64,
    },
    /// Reading the original item bytes failed.
    #[error("failed to read {}: {source}", path.display())]
    Read {
        /// Original item path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Removing the original item after durable backup failed.
    #[error("failed to remove {}: {source}", path.display())]
    Remove {
        /// Original item path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
}

/// App-data directory containing format-upgrade backup attempts.
///
/// Spelled by the app-data layout; kept as a name for callers outside
/// `services` (UI and tests).
pub const FORMAT_UPGRADE_BACKUP_DIR: &str = AppDataLocation::FormatUpgradeBackups.relative_str();
/// Per-run directory holding the backed-up item copies.
///
/// Spelled by the app-data layout; kept as a name for callers outside
/// `services` (UI and tests).
pub const FORMAT_UPGRADE_BACKUP_ITEMS_DIR: &str =
    crate::persistence::app_data_layout::run_items_dir_name();
/// Maximum attempts to find a unique backup directory for one operation.
///
/// Timestamp collisions are rare, but the bound keeps a hostile prefilled
/// backup directory from turning user startup into an unbounded loop.
///
/// Kind: retry budget; a power of two, rounded up.
const MAX_BACKUP_DIR_ATTEMPTS: u32 = 64;
/// Maximum bytes read into memory for one preserved app-data item.
///
/// Backups currently use the filesystem boundary's whole-byte read API. Keeping
/// the cap aligned with recovery metadata prevents a damaged app-data file from
/// exhausting memory; too-low values make large items retryable, too-high values
/// can OOM the worker.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
const MAX_BACKUP_ITEM_BYTES: u64 =
    crate::persistence::recovery_metadata::DEFAULT_MAX_METADATA_BYTES;

/// Manifest describing one conversion or Start Fresh preservation run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatBackupManifest {
    /// LushText version that created this backup.
    pub lushtext_version: String,
    /// Unix timestamp in seconds when the backup attempt started.
    pub created_at_unix_secs: u64,
    /// User action that required preservation.
    pub action: String,
    /// Per-item preservation records.
    pub records: Vec<FormatBackupRecord>,
}

/// Backup record for one affected app-data item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FormatBackupRecord {
    /// App-data-relative path of the original item.
    pub original_relative_path: String,
    /// Human-readable metadata kind.
    pub metadata_kind: String,
    /// Classification that made the item actionable.
    pub original_classification: String,
    /// Version found in the original envelope when known.
    pub original_version: Option<u32>,
    /// App-data-relative backup path containing the original bytes.
    pub backup_relative_path: Option<String>,
    /// Preservation result for this item.
    pub result: String,
    /// Failure detail when preservation did not complete.
    pub detail: Option<String>,
}

/// Filesystem location for one in-progress backup run.
#[derive(Debug)]
pub(crate) struct BackupSession {
    root: PathBuf,
    items_dir: PathBuf,
    created_at_unix_secs: u64,
    action: &'static str,
    records: Vec<FormatBackupRecord>,
}

impl BackupSession {
    /// Create a unique backup directory and its item-storage child directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the app-data backup directory cannot be created or
    /// no unique attempt directory can be reserved within the bounded retry
    /// count.
    pub(crate) fn create(data_dir: &Path, action: &'static str) -> Result<Self, FormatBackupError> {
        let created_at_unix_secs = current_unix_secs();
        let base = data_dir.join(FORMAT_UPGRADE_BACKUP_DIR);
        fs_write::create_dir_all_durable(&base).map_err(|source| FormatBackupError::CreateDir {
            path: base.clone(),
            source,
        })?;

        for attempt in 0..MAX_BACKUP_DIR_ATTEMPTS {
            let name = format!("{created_at_unix_secs}-{action}-{attempt:02}");
            let root = base.join(name);
            match fs_write::create_dir_durable(&root) {
                Ok(()) => {
                    let items_dir = root.join(FORMAT_UPGRADE_BACKUP_ITEMS_DIR);
                    fs_write::create_dir_durable(&items_dir).map_err(|source| {
                        FormatBackupError::CreateDir {
                            path: items_dir.clone(),
                            source,
                        }
                    })?;
                    return Ok(Self {
                        root,
                        items_dir,
                        created_at_unix_secs,
                        action,
                        records: Vec::new(),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(FormatBackupError::CreateDir { path: root, source });
                }
            }
        }

        Err(FormatBackupError::BackupDirAttemptsExhausted {
            attempts: MAX_BACKUP_DIR_ATTEMPTS,
        })
    }

    /// Preserve one item by copying its original bytes into this backup run.
    ///
    /// # Errors
    ///
    /// Returns an error when the item is not a regular file, cannot be read, or
    /// cannot be written durably to the backup directory.
    pub(crate) fn copy_item(
        &mut self,
        data_dir: &Path,
        item: &FormatInventoryItem,
    ) -> Result<(), FormatBackupError> {
        let backup_path = self.backup_path_for(item);
        let bytes = read_preservable_file(item)?;
        fs_write::atomic_replace(&backup_path, WriteLabel::FORMAT_UPGRADE_BACKUP, &bytes)
            .map_err(fs_write::DurableWriteError::into_io_error)
            .map_err(|source| FormatBackupError::WriteBackup {
                path: item.absolute_path.clone(),
                source,
            })?;
        self.records.push(success_record(
            data_dir,
            item,
            &backup_path,
            "copied-to-backup",
        ));
        Ok(())
    }

    /// Record a preservation failure without mutating the original item.
    pub(crate) fn record_failure(&mut self, item: &FormatInventoryItem, detail: String) {
        self.records.push(FormatBackupRecord {
            original_relative_path: item.path.display(),
            metadata_kind: item.kind.label().to_string(),
            original_classification: classification_label(&item.classification).to_string(),
            original_version: classification_version(&item.classification),
            backup_relative_path: None,
            result: "failed".to_string(),
            detail: Some(detail),
        });
    }

    /// Write the backup manifest and return the persisted manifest value.
    ///
    /// # Errors
    ///
    /// Returns an error when manifest serialization or durable write fails.
    pub(crate) fn finish(self, data_dir: &Path) -> Result<FormatBackupManifest, FormatBackupError> {
        let manifest = FormatBackupManifest {
            lushtext_version: env!("CARGO_PKG_VERSION").to_string(),
            created_at_unix_secs: self.created_at_unix_secs,
            action: self.action.to_string(),
            records: self.records,
        };
        let manifest_path = self.root.join("manifest.json");
        let envelope = JsonEnvelopeRef::new(KIND_FORMAT_UPGRADE_BACKUP_MANIFEST, &manifest);
        fs_write::atomic_replace_stream(
            &manifest_path,
            WriteLabel::FORMAT_UPGRADE_MANIFEST,
            |writer| serde_json::to_writer_pretty(writer, &envelope).map_err(io::Error::other),
        )
        .map_err(fs_write::DurableWriteError::into_io_error)
        .map_err(|source| FormatBackupError::WriteManifest {
            path: manifest_path.clone(),
            source,
        })?;

        let relative_manifest = manifest_path.strip_prefix(data_dir).map_or_else(
            |_| manifest_path.display().to_string(),
            |path| path.display().to_string(),
        );
        tracing::info!("format-upgrade backup manifest written to {relative_manifest}");
        Ok(manifest)
    }

    fn backup_path_for(&self, item: &FormatInventoryItem) -> PathBuf {
        // Store backup files under hashed leaf names so arbitrary app-data
        // paths cannot create nested backup trees; the manifest keeps the
        // readable original path.
        let hash = stable_path_hash(item.path.relative());
        let extension = item
            .absolute_path
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or("bin");
        self.items_dir.join(format!("{hash}.{extension}"))
    }
}

/// Read one inventory item after proving it still matches the scan facts.
///
/// # Errors
///
/// Returns an error when the source changed, is no longer regular, or cannot be read.
pub(crate) fn read_preservable_item_bytes(
    item: &FormatInventoryItem,
) -> Result<Vec<u8>, FormatBackupError> {
    read_preservable_file(item)
}

/// Remove a copied item from active app data after the manifest is durable.
///
/// # Errors
///
/// Returns an error when the source changed since scan or cannot be removed.
pub(crate) fn remove_original_after_manifest(
    item: &FormatInventoryItem,
) -> Result<(), FormatBackupError> {
    ensure_regular_file_unchanged(item)?;
    fs_mutate::remove_file_if_exists(&item.absolute_path).map_err(|source| {
        FormatBackupError::Remove {
            path: item.absolute_path.clone(),
            source,
        }
    })?;
    Ok(())
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn read_preservable_file(item: &FormatInventoryItem) -> Result<Vec<u8>, FormatBackupError> {
    let byte_size = ensure_regular_file_unchanged(item)?;
    if byte_size > MAX_BACKUP_ITEM_BYTES {
        return Err(FormatBackupError::BackupLimitExceeded {
            relative_path: item.path.display(),
            byte_size,
            limit: MAX_BACKUP_ITEM_BYTES,
        });
    }
    fs_read::bytes(&item.absolute_path).map_err(|source| FormatBackupError::Read {
        path: item.absolute_path.clone(),
        source,
    })
}

/// Re-check the scanned file before copy or removal so an old plan cannot
/// overwrite or discard metadata that changed after inventory.
fn ensure_regular_file_unchanged(item: &FormatInventoryItem) -> Result<u64, FormatBackupError> {
    let path = &item.absolute_path;
    match fs_metadata::path_status(path) {
        Ok(PathStatus::File) => {}
        Ok(status) => {
            return Err(FormatBackupError::NotRegularFile {
                path: path.clone(),
                status,
            });
        }
        Err(source) => {
            return Err(FormatBackupError::Inspect {
                path: path.clone(),
                source,
            });
        }
    }

    let current = fs_metadata::file_facts(path).map_err(|source| FormatBackupError::Inspect {
        path: path.clone(),
        source,
    })?;
    if let Some(scanned) = item.file_facts
        && (current.byte_size != scanned.byte_size
            || current.modified_at_secs != scanned.modified_at_secs)
    {
        return Err(FormatBackupError::ChangedAfterScan {
            relative_path: item.path.display(),
        });
    }

    Ok(current.byte_size)
}

fn success_record(
    data_dir: &Path,
    item: &FormatInventoryItem,
    backup_path: &Path,
    result: &'static str,
) -> FormatBackupRecord {
    let backup_relative_path = backup_path.strip_prefix(data_dir).map_or_else(
        |_| backup_path.display().to_string(),
        |path| path.display().to_string(),
    );
    FormatBackupRecord {
        original_relative_path: item.path.display(),
        metadata_kind: item.kind.label().to_string(),
        original_classification: classification_label(&item.classification).to_string(),
        original_version: classification_version(&item.classification),
        backup_relative_path: Some(backup_relative_path),
        result: result.to_string(),
        detail: None,
    }
}

fn classification_label(classification: &FormatClassification) -> &'static str {
    match classification {
        FormatClassification::Missing => "missing",
        FormatClassification::Current { .. } => "current",
        FormatClassification::Upgradeable { .. } => "upgradeable",
        FormatClassification::FutureVersion { .. } => "future-version",
        FormatClassification::UnsupportedOld { .. } => "unsupported-old",
        FormatClassification::Damaged { .. } => "damaged",
        FormatClassification::UnsafeToReplace { .. } => "unsafe-to-replace",
    }
}

fn classification_version(classification: &FormatClassification) -> Option<u32> {
    match classification {
        FormatClassification::Current { version }
        | FormatClassification::UnsupportedOld { version, .. } => *version,
        FormatClassification::Upgradeable { from_version, .. } => Some(*from_version),
        FormatClassification::FutureVersion { version, .. } => Some(*version),
        FormatClassification::Missing
        | FormatClassification::Damaged { .. }
        | FormatClassification::UnsafeToReplace { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::fixture;
    use crate::persistence::format_upgrade::diagnostics::{FormatItemPath, FormatMetadataKind};
    use crate::persistence::format_upgrade::inventory::FormatItemFileFacts;
    use tempfile::TempDir;

    fn item(
        data_dir: &Path,
        relative: &str,
        classification: FormatClassification,
    ) -> FormatInventoryItem {
        let absolute_path = data_dir.join(relative);
        let facts = fs_metadata::file_facts(&absolute_path).expect("fixture facts");
        FormatInventoryItem {
            kind: FormatMetadataKind::Session,
            path: FormatItemPath::from_relative(relative),
            absolute_path,
            file_facts: Some(FormatItemFileFacts {
                byte_size: facts.byte_size,
                modified_at_secs: facts.modified_at_secs,
            }),
            classification,
        }
    }

    #[test]
    fn backup_session_skips_existing_attempt_directory_and_records_timestamp() {
        let dir = TempDir::new().expect("temp dir");
        let base = dir.path().join(FORMAT_UPGRADE_BACKUP_DIR);
        fixture::create_dir_all(&base);
        let now = current_unix_secs();
        for timestamp in now..=now.saturating_add(3) {
            fixture::create_dir_all(&base.join(format!("{timestamp}-convert-00")));
        }

        let session = BackupSession::create(dir.path(), "convert").expect("create backup session");

        assert!(session.created_at_unix_secs > 1);
        assert!(
            session
                .root
                .ends_with(format!("{}-convert-01", session.created_at_unix_secs)),
            "expected attempt 01 after prefilled attempt 00, got {}",
            session.root.display()
        );
        assert!(fs_metadata::exists(&session.items_dir));
    }

    #[test]
    fn backup_session_create_reports_non_collision_directory_errors() {
        let dir = TempDir::new().expect("temp dir");
        let base = dir.path().join(FORMAT_UPGRADE_BACKUP_DIR);
        fixture::write_text(&base, "not a directory");

        let error = BackupSession::create(dir.path(), "convert")
            .expect_err("non-collision create errors must stop backup creation");
        assert!(
            matches!(
                &error,
                FormatBackupError::CreateDir { source, .. }
                    if matches!(
                        source.kind(),
                        io::ErrorKind::AlreadyExists | io::ErrorKind::NotADirectory
                    )
            ),
            "directory creation failure should preserve the underlying I/O error: {error:?}"
        );
        let message = error.to_string();

        assert!(
            message.contains("failed to create"),
            "unexpected error: {message}"
        );
        assert!(
            !message.contains("unique format-upgrade backup directory"),
            "permission errors must not be treated as timestamp collisions: {message}"
        );
    }

    #[test]
    fn backup_session_copy_item_records_backup_manifest_entry() {
        let dir = TempDir::new().expect("temp dir");
        fixture::write_text(&dir.path().join("session.json"), "session bytes");
        let item = item(
            dir.path(),
            "session.json",
            FormatClassification::Upgradeable {
                from_version: 0,
                to_version: 1,
            },
        );
        let mut session = BackupSession::create(dir.path(), "convert").expect("create session");

        session.copy_item(dir.path(), &item).expect("copy item");
        let manifest = session.finish(dir.path()).expect("finish manifest");

        assert_eq!(manifest.action, "convert");
        assert_eq!(manifest.records.len(), 1);
        let record = &manifest.records[0];
        assert_eq!(record.original_relative_path, "session.json");
        assert_eq!(record.metadata_kind, "session");
        assert_eq!(record.original_classification, "upgradeable");
        assert_eq!(record.original_version, Some(0));
        assert_eq!(record.result, "copied-to-backup");
        let backup_path = dir.path().join(
            record
                .backup_relative_path
                .as_ref()
                .expect("backup path recorded"),
        );
        fixture::assert_text(&backup_path, "session bytes");
    }

    #[test]
    fn backup_session_record_failure_preserves_classification_detail() {
        let dir = TempDir::new().expect("temp dir");
        fixture::write_text(&dir.path().join("session.json"), "session bytes");
        let item = item(
            dir.path(),
            "session.json",
            FormatClassification::FutureVersion {
                version: 99,
                supported_version: 1,
            },
        );
        let mut session = BackupSession::create(dir.path(), "start-fresh").expect("create session");

        session.record_failure(&item, "permission denied".to_string());
        let manifest = session.finish(dir.path()).expect("finish manifest");

        assert_eq!(manifest.records.len(), 1);
        let record = &manifest.records[0];
        assert_eq!(record.original_classification, "future-version");
        assert_eq!(record.original_version, Some(99));
        assert_eq!(record.result, "failed");
        assert_eq!(record.detail.as_deref(), Some("permission denied"));
        assert!(record.backup_relative_path.is_none());
    }

    #[test]
    fn read_preservable_item_bytes_returns_original_bytes_and_detects_stale_scan() {
        let dir = TempDir::new().expect("temp dir");
        fixture::write_text(&dir.path().join("session.json"), "session bytes");
        let mut item = item(
            dir.path(),
            "session.json",
            FormatClassification::Current { version: Some(1) },
        );

        assert_eq!(
            read_preservable_item_bytes(&item).expect("read preserved bytes"),
            b"session bytes"
        );

        let facts = item.file_facts.as_mut().expect("file facts");
        facts.byte_size += 1;
        let error =
            read_preservable_item_bytes(&item).expect_err("stale scan should block backup reads");
        assert!(
            matches!(error, FormatBackupError::ChangedAfterScan { .. }),
            "expected ChangedAfterScan, got {error:?}"
        );
        assert!(
            error.to_string().contains("changed after the format scan"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn read_preservable_item_bytes_allows_file_at_exact_backup_limit() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("session.json");
        fixture::create_sparse_file(&path, MAX_BACKUP_ITEM_BYTES);
        let item = item(
            dir.path(),
            "session.json",
            FormatClassification::Current { version: Some(1) },
        );

        let bytes = read_preservable_item_bytes(&item).expect("exact-limit read should pass");

        assert_eq!(bytes.len() as u64, MAX_BACKUP_ITEM_BYTES);
    }

    #[test]
    fn read_preservable_item_bytes_rejects_oversized_and_directory_items() {
        let dir = TempDir::new().expect("temp dir");
        let huge_path = dir.path().join("huge.json");
        fixture::create_sparse_file(&huge_path, MAX_BACKUP_ITEM_BYTES + 1);
        let huge_item = item(
            dir.path(),
            "huge.json",
            FormatClassification::Current { version: Some(1) },
        );
        assert!(matches!(
            read_preservable_item_bytes(&huge_item),
            Err(FormatBackupError::BackupLimitExceeded { .. })
        ));

        let folder_path = dir.path().join("dir.json");
        fixture::create_dir(&folder_path);
        let folder_item = FormatInventoryItem {
            kind: FormatMetadataKind::Session,
            path: FormatItemPath::from_relative("dir.json"),
            absolute_path: folder_path,
            file_facts: None,
            classification: FormatClassification::UnsafeToReplace {
                detail: "directory".to_string(),
            },
        };
        assert!(matches!(
            read_preservable_item_bytes(&folder_item),
            Err(FormatBackupError::NotRegularFile { .. })
        ));
    }

    #[test]
    fn classification_labels_and_versions_cover_manifest_cases() {
        let cases = [
            (FormatClassification::Missing, "missing", None),
            (
                FormatClassification::Current { version: Some(1) },
                "current",
                Some(1),
            ),
            (
                FormatClassification::Upgradeable {
                    from_version: 0,
                    to_version: 1,
                },
                "upgradeable",
                Some(0),
            ),
            (
                FormatClassification::FutureVersion {
                    version: 99,
                    supported_version: 1,
                },
                "future-version",
                Some(99),
            ),
            (
                FormatClassification::UnsupportedOld {
                    version: Some(0),
                    detail: "old".to_string(),
                },
                "unsupported-old",
                Some(0),
            ),
            (
                FormatClassification::Damaged {
                    detail: "bad json".to_string(),
                },
                "damaged",
                None,
            ),
            (
                FormatClassification::UnsafeToReplace {
                    detail: "not a file".to_string(),
                },
                "unsafe-to-replace",
                None,
            ),
        ];

        for (classification, expected_label, expected_version) in cases {
            assert_eq!(classification_label(&classification), expected_label);
            assert_eq!(classification_version(&classification), expected_version);
        }
    }
}

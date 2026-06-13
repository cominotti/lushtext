// SPDX-License-Identifier: GPL-3.0-or-later

//! Backup manifest and preservation helpers for format-upgrade actions.
//!
//! Backups live under the app data directory so conversion and Start Fresh
//! never touch user document folders. The manifest records app-data-relative
//! paths and enough classification detail to support retry and support triage.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::model::sidecar_identity::stable_path_hash;
use crate::services::filesystem::{
    PathStatus, WriteLabel, metadata as fs_metadata, mutate as fs_mutate, read as fs_read,
    write as fs_write,
};
use crate::services::json_format::{JsonEnvelopeRef, KIND_FORMAT_UPGRADE_BACKUP_MANIFEST};

use super::diagnostics::FormatClassification;
use super::inventory::FormatInventoryItem;

/// App-data directory containing format-upgrade backup attempts.
pub const FORMAT_UPGRADE_BACKUP_DIR: &str = "format-upgrade-backups";
/// Maximum attempts to find a unique backup directory for one operation.
///
/// Timestamp collisions are rare, but the bound keeps a hostile prefilled
/// backup directory from turning user startup into an unbounded loop.
const MAX_BACKUP_DIR_ATTEMPTS: u32 = 64;
/// Maximum bytes read into memory for one preserved app-data item.
///
/// Backups currently use the filesystem boundary's whole-byte read API. Keeping
/// the cap aligned with recovery metadata prevents a damaged app-data file from
/// exhausting memory; too-low values make large items retryable, too-high values
/// can OOM the worker.
const MAX_BACKUP_ITEM_BYTES: u64 = crate::services::recovery_metadata::DEFAULT_MAX_METADATA_BYTES;

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
    pub(crate) fn create(data_dir: &Path, action: &'static str) -> Result<Self> {
        let created_at_unix_secs = current_unix_secs();
        let base = data_dir.join(FORMAT_UPGRADE_BACKUP_DIR);
        fs_write::create_dir_all_durable(&base)
            .with_context(|| format!("failed to create {}", base.display()))?;

        for attempt in 0..MAX_BACKUP_DIR_ATTEMPTS {
            let name = format!("{created_at_unix_secs}-{action}-{attempt:02}");
            let root = base.join(name);
            match fs_write::create_dir_durable(&root) {
                Ok(()) => {
                    let items_dir = root.join("items");
                    fs_write::create_dir_durable(&items_dir).with_context(|| {
                        format!("failed to create backup items dir {}", items_dir.display())
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
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to create {}", root.display()));
                }
            }
        }

        anyhow::bail!(
            "failed to reserve a unique format-upgrade backup directory after {MAX_BACKUP_DIR_ATTEMPTS} attempts"
        )
    }

    /// Preserve one item by copying its original bytes into this backup run.
    ///
    /// # Errors
    ///
    /// Returns an error when the item is not a regular file, cannot be read, or
    /// cannot be written durably to the backup directory.
    pub(crate) fn copy_item(&mut self, data_dir: &Path, item: &FormatInventoryItem) -> Result<()> {
        let backup_path = self.backup_path_for(item);
        let bytes = read_preservable_file(item)?;
        fs_write::atomic_replace(
            &backup_path,
            WriteLabel::from("format-upgrade-backup"),
            &bytes,
        )
        .map_err(fs_write::DurableWriteError::into_io_error)
        .with_context(|| format!("failed to back up {}", item.absolute_path.display()))?;
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
    pub(crate) fn finish(self, data_dir: &Path) -> Result<FormatBackupManifest> {
        let manifest = FormatBackupManifest {
            lushtext_version: config::VERSION.to_string(),
            created_at_unix_secs: self.created_at_unix_secs,
            action: self.action.to_string(),
            records: self.records,
        };
        let manifest_path = self.root.join("manifest.json");
        let envelope = JsonEnvelopeRef::new(KIND_FORMAT_UPGRADE_BACKUP_MANIFEST, &manifest);
        fs_write::atomic_replace_stream(
            &manifest_path,
            WriteLabel::from("format-upgrade-manifest"),
            |writer| serde_json::to_writer_pretty(writer, &envelope).map_err(io::Error::other),
        )
        .map_err(fs_write::DurableWriteError::into_io_error)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

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
            .and_then(|ext| ext.to_str())
            .unwrap_or("bin");
        self.items_dir.join(format!("{hash}.{extension}"))
    }
}

/// Read one inventory item after proving it still matches the scan facts.
///
/// # Errors
///
/// Returns an error when the source changed, is no longer regular, or cannot be read.
pub(crate) fn read_preservable_item_bytes(item: &FormatInventoryItem) -> Result<Vec<u8>> {
    read_preservable_file(item)
}

/// Remove a copied item from active app data after the manifest is durable.
///
/// # Errors
///
/// Returns an error when the source changed since scan or cannot be removed.
pub(crate) fn remove_original_after_manifest(item: &FormatInventoryItem) -> Result<()> {
    ensure_regular_file_unchanged(item)?;
    fs_mutate::remove_file_if_exists(&item.absolute_path)
        .with_context(|| format!("failed to remove {}", item.absolute_path.display()))?;
    Ok(())
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn read_preservable_file(item: &FormatInventoryItem) -> Result<Vec<u8>> {
    let byte_size = ensure_regular_file_unchanged(item)?;
    if byte_size > MAX_BACKUP_ITEM_BYTES {
        anyhow::bail!(
            "{} is {} bytes, above the {} byte backup limit",
            item.path.display(),
            byte_size,
            MAX_BACKUP_ITEM_BYTES
        );
    }
    fs_read::bytes(&item.absolute_path)
        .with_context(|| format!("failed to read {}", item.absolute_path.display()))
}

/// Re-check the scanned file before copy or removal so an old plan cannot
/// overwrite or discard metadata that changed after inventory.
fn ensure_regular_file_unchanged(item: &FormatInventoryItem) -> Result<u64> {
    let path = &item.absolute_path;
    match fs_metadata::path_status(path) {
        Ok(PathStatus::File) => Ok(()),
        Ok(status) => anyhow::bail!("expected regular file, found {status:?}"),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }?;

    let current = fs_metadata::file_facts(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if let Some(scanned) = item.file_facts
        && (current.byte_size != scanned.byte_size
            || current.modified_at_secs != scanned.modified_at_secs)
    {
        anyhow::bail!(
            "{} changed after the format scan; scan again before applying",
            item.path.display()
        );
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

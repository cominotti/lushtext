// SPDX-License-Identifier: GPL-3.0-or-later

//! Persisted undo backup for Replace All.
//!
//! The backup maps file paths to original and post-replace bytes so a Replace
//! All can be reverted without overwriting files edited after the replacement.

use crate::services::content_search::{ReplaceUndoBackup, ReplaceUndoEntry};
use crate::services::{durable_write, json_store};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const BACKUP_FILE: &str = "replace-backup.json";
const JOURNAL_DIR: &str = "replace-backup-journal";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ReplaceBackupDisk {
    files: Vec<ReplaceBackupDiskEntry>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ReplaceBackupDiskEntry {
    path: PathBuf,
    original_content: String,
    replaced_content: String,
}

/// Load the persisted Replace All undo backup from disk.
///
/// # Errors
///
/// Returns an error if the backup file exists but cannot be read, parsed, or
/// converted back into UTF-8 file content.
pub fn load(data_dir: &Path) -> Result<ReplaceUndoBackup> {
    let journal_dir = data_dir.join(JOURNAL_DIR);
    if journal_dir.is_dir() {
        return load_journal(&journal_dir);
    }

    let disk: ReplaceBackupDisk = json_store::load(data_dir, BACKUP_FILE)?;
    let mut backup = ReplaceUndoBackup::with_capacity(disk.files.len());
    for entry in disk.files {
        backup.insert(
            entry.path,
            ReplaceUndoEntry::new(
                entry.original_content.into_bytes(),
                entry.replaced_content.into_bytes(),
            ),
        );
    }
    Ok(backup)
}

/// Save the Replace All undo backup atomically.
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
    Ok(())
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
    json_store::save(&journal_dir, &entry_file_name(path), &disk)
}

/// Delete one per-file journal entry. Missing entries are treated as already gone.
///
/// # Errors
///
/// Returns an error if an existing entry cannot be removed.
pub fn delete_entry(data_dir: &Path, path: &Path) -> Result<()> {
    let entry_path = data_dir.join(JOURNAL_DIR).join(entry_file_name(path));
    match std::fs::remove_file(&entry_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete replace journal entry {}: {}",
            entry_path.display(),
            e
        )),
    }
}

fn load_journal(journal_dir: &Path) -> Result<ReplaceUndoBackup> {
    let mut backup = ReplaceUndoBackup::new();
    for entry in std::fs::read_dir(journal_dir)
        .with_context(|| format!("failed to read replace journal {}", journal_dir.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to inspect replace journal entry in {}",
                journal_dir.display()
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let disk: ReplaceBackupDiskEntry = json_store::load(
            journal_dir,
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        )?;
        backup.insert(
            disk.path,
            ReplaceUndoEntry::new(
                disk.original_content.into_bytes(),
                disk.replaced_content.into_bytes(),
            ),
        );
    }
    Ok(backup)
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

/// Delete the persisted Replace All undo backup, if it exists.
///
/// # Errors
///
/// Returns an error if an existing backup file cannot be deleted.
pub fn delete(data_dir: &Path) -> Result<()> {
    let legacy = data_dir.join(BACKUP_FILE);
    match std::fs::remove_file(&legacy) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(anyhow::anyhow!(
                "failed to delete replace backup {}: {}",
                legacy.display(),
                e
            ));
        }
    }

    let journal_dir = data_dir.join(JOURNAL_DIR);
    match std::fs::remove_dir_all(&journal_dir) {
        Ok(()) => {
            if data_dir.exists()
                && let Err(error) = durable_write::sync_parent_dir(&journal_dir)
            {
                tracing::warn!("Failed to sync replace journal cleanup: {error}");
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete replace journal {}: {}",
            journal_dir.display(),
            e
        )),
    }
}

fn entry_file_name(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}.json", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
    fn delete_reports_non_file_backup_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join(BACKUP_FILE)).expect("expected operation to succeed");

        let error = delete(dir.path()).expect_err("directory backup should fail deletion");

        assert!(
            error
                .to_string()
                .contains("failed to delete replace backup"),
            "unexpected error: {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn save_entry_does_not_rewrite_existing_journal_entries() {
        use std::os::unix::fs::MetadataExt;

        let dir = TempDir::new().expect("expected operation to succeed");
        let path_a = PathBuf::from("/tmp/a.rs");
        let path_b = PathBuf::from("/tmp/b.rs");
        let entry_a = ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec());
        let entry_b = ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec());

        save_entry(dir.path(), &path_a, &entry_a).expect("save first journal entry");
        let entry_a_path = dir.path().join(JOURNAL_DIR).join(entry_file_name(&path_a));
        let inode_before = std::fs::metadata(&entry_a_path)
            .expect("stat first entry before")
            .ino();

        save_entry(dir.path(), &path_b, &entry_b).expect("save second journal entry");

        let inode_after = std::fs::metadata(&entry_a_path)
            .expect("stat first entry after")
            .ino();
        assert_eq!(
            inode_after, inode_before,
            "saving a new file's journal entry must not rewrite older entries"
        );
        assert!(
            dir.path()
                .join(JOURNAL_DIR)
                .join(entry_file_name(&path_b))
                .exists(),
            "the second per-file journal entry should be created independently"
        );
    }
}

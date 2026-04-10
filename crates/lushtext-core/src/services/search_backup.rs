// SPDX-License-Identifier: GPL-3.0-or-later

//! Persisted undo backup for Replace All.
//!
//! The backup maps file paths to original file bytes so a Replace All can be
//! reverted during the current app session. The search panel clears any stale
//! backup on close and discards leftovers from earlier sessions on startup.

use crate::services::json_store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const BACKUP_FILE: &str = "replace-backup.json";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ReplaceBackupDisk {
    files: Vec<ReplaceBackupDiskEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplaceBackupDiskEntry {
    path: PathBuf,
    content: String,
}

/// Load the persisted Replace All undo backup from disk.
pub fn load(data_dir: &Path) -> Result<HashMap<PathBuf, Vec<u8>>> {
    let disk: ReplaceBackupDisk = json_store::load(data_dir, BACKUP_FILE)?;
    let mut backup = HashMap::with_capacity(disk.files.len());
    for entry in disk.files {
        backup.insert(entry.path, entry.content.into_bytes());
    }
    Ok(backup)
}

/// Save the Replace All undo backup atomically.
pub fn save(data_dir: &Path, backup: &HashMap<PathBuf, Vec<u8>>) -> Result<()> {
    let mut disk = ReplaceBackupDisk {
        files: Vec::with_capacity(backup.len()),
    };
    for (path, bytes) in backup {
        let content = String::from_utf8(bytes.clone())
            .with_context(|| format!("replace backup for {} is not valid UTF-8", path.display()))?;
        disk.files.push(ReplaceBackupDiskEntry {
            path: path.clone(),
            content,
        });
    }
    json_store::save(data_dir, BACKUP_FILE, &disk)
}

/// Delete the persisted Replace All undo backup, if it exists.
pub fn delete(data_dir: &Path) -> Result<()> {
    let path = data_dir.join(BACKUP_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete replace backup {}: {}",
            path.display(),
            e
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_load_delete_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut backup = HashMap::new();
        backup.insert(PathBuf::from("/tmp/a.rs"), b"alpha".to_vec());
        backup.insert(PathBuf::from("/tmp/b.rs"), b"beta".to_vec());

        save(dir.path(), &backup).unwrap();
        let loaded = load(dir.path()).unwrap();
        assert_eq!(loaded, backup);

        delete(dir.path()).unwrap();
        let after_delete = load(dir.path()).unwrap();
        assert!(after_delete.is_empty());
    }
}

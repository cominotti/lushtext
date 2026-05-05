// SPDX-License-Identifier: GPL-3.0-or-later

//! Filesystem durability helpers for persistence services.
//!
//! Linux filesystems such as ext4, XFS, and Btrfs only make a temp-file rename
//! crash-durable after the containing directory has also been synced. Keeping
//! that rule in one GTK-free service helper prevents each persistence caller
//! from remembering the filesystem contract by hand.

use std::path::{Path, PathBuf};

/// Atomically replace `path` with `bytes` and sync the renamed directory entry.
///
/// The caller supplies `tmp_tag` so different workflows do not collide if they
/// happen to replace the same path in quick succession.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread unless it is part of a synchronous shutdown safety path.
///
/// # Errors
///
/// Returns an error if the temp file cannot be written and synced, the rename
/// fails, or the parent directory cannot be synced after the rename.
pub fn atomic_write_bytes(path: &Path, tmp_tag: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map_or_else(|| "untitled".into(), |name| name.to_string_lossy());
    let tmp_path = parent.join(format!(".{file_name}.{tmp_tag}.tmp"));
    let file = std::fs::File::create(&tmp_path)?;
    let mut writer = std::io::BufWriter::new(file);
    let write_result = writer
        .write_all(bytes)
        .and_then(|()| writer.flush())
        .and_then(|()| writer.get_ref().sync_all());

    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error);
    }

    drop(writer);
    if let Err(error) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error);
    }
    sync_parent_dir(path)
}

/// Rename a file or directory and sync both affected parent directories.
///
/// `rename()` changes directory entries, so syncing only the moved file or
/// directory would not make the namespace update durable across power loss.
///
/// # Errors
///
/// Returns an error if the rename fails or either affected parent directory
/// cannot be synced.
pub fn rename_durable(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)?;
    sync_parent_dir(from)?;
    if from.parent() != to.parent() {
        sync_parent_dir(to)?;
    }
    Ok(())
}

/// Copy a file with a durable target write, then remove and sync the source.
///
/// This is a cross-filesystem fallback for `rename_durable()`. The source is not
/// removed until the destination bytes and destination directory entry are
/// durable.
///
/// # Errors
///
/// Returns an error if the source cannot be read, the destination cannot be
/// atomically written, the source cannot be removed, or the source directory
/// cannot be synced after removal.
pub fn copy_file_durable(from: &Path, to: &Path, tmp_tag: &str) -> std::io::Result<()> {
    let bytes = std::fs::read(from)?;
    atomic_write_bytes(to, tmp_tag, &bytes)?;
    std::fs::remove_file(from)?;
    sync_parent_dir(from)
}

/// Create a directory tree and sync each directory entry that was newly created.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread unless it is part of a synchronous shutdown safety path.
///
/// # Errors
///
/// Returns an error when directory creation fails or when a newly-created
/// directory or its parent cannot be synced.
pub fn create_dir_all_durable(path: &Path) -> std::io::Result<()> {
    let missing = missing_ancestors(path);
    std::fs::create_dir_all(path)?;

    for created in missing.iter().rev() {
        sync_parent_dir(created)?;
        sync_dir(created)?;
    }

    Ok(())
}

/// Sync the directory containing `path`.
///
/// Call this after a successful `rename()` into place. Syncing the file itself
/// before rename is not enough: the parent directory owns the name-to-inode link
/// that must survive power loss.
///
/// # Errors
///
/// Returns an error if the parent directory cannot be opened or synced.
pub fn sync_parent_dir(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    sync_dir(parent)
}

/// Sync a directory handle on Unix, where LushText's GTK target platforms live.
#[cfg(unix)]
fn sync_dir(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

/// Keep non-Unix builds compiling even though the shipped target is Linux.
#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Collect ancestors that do not exist yet, starting at `path`.
fn missing_ancestors(path: &Path) -> Vec<PathBuf> {
    path.ancestors()
        .take_while(|ancestor| !ancestor.exists())
        .map(Path::to_path_buf)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_dir_all_durable_creates_nested_tree() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let nested = dir.path().join("a/b/c");

        create_dir_all_durable(&nested).expect("expected operation to succeed");

        assert!(nested.is_dir());
    }

    #[test]
    fn sync_parent_dir_accepts_existing_parent() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("data.json");
        std::fs::write(&path, "{}").expect("expected operation to succeed");

        sync_parent_dir(&path).expect("expected operation to succeed");
    }

    #[test]
    fn atomic_write_bytes_replaces_file_and_removes_temp() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("data.txt");

        atomic_write_bytes(&path, "test", b"new").expect("expected operation to succeed");

        assert_eq!(
            std::fs::read(&path).expect("expected operation to succeed"),
            b"new"
        );
        assert!(
            std::fs::read_dir(dir.path())
                .expect("expected operation to succeed")
                .all(|entry| !entry
                    .expect("expected operation to succeed")
                    .file_name()
                    .to_string_lossy()
                    .contains(".test.tmp"))
        );
    }

    #[test]
    fn copy_file_durable_writes_destination_before_removing_source() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        std::fs::write(&from, "snapshot").expect("expected operation to succeed");

        copy_file_durable(&from, &to, "copy").expect("expected operation to succeed");

        assert!(!from.exists());
        assert_eq!(
            std::fs::read_to_string(&to).expect("expected operation to succeed"),
            "snapshot"
        );
    }
}

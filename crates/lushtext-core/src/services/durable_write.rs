// SPDX-License-Identifier: GPL-3.0-or-later

//! Filesystem durability helpers for persistence services.
//!
//! Linux filesystems such as ext4, XFS, and Btrfs only make a temp-file rename
//! crash-durable after the containing directory has also been synced. Keeping
//! that rule in one GTK-free service helper prevents each persistence caller
//! from remembering the filesystem contract by hand.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-local counter for temp-file names that may be created concurrently.
///
/// Including the process ID and this counter keeps overlapping writes from
/// reusing the same temp path while still leaving recognizable filenames for
/// crash leftovers.
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Build a unique hidden temp path next to the final destination.
#[must_use]
pub fn unique_temp_path(path: &Path, tmp_tag: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map_or_else(|| "untitled".into(), |name| name.to_string_lossy());
    let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{file_name}.{tmp_tag}.{}.{}.tmp",
        std::process::id(),
        sequence
    ))
}

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

    let tmp_path = unique_temp_path(path, tmp_tag);
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

/// Exclusive advisory lock for one existing file path.
///
/// LushText's own save and Replace All paths both acquire this before replacing
/// file bytes, so an in-app save cannot race a workspace-wide replacement for
/// the same path. The lock is advisory: external editors that ignore `flock`
/// can still write concurrently, so callers must keep their own content
/// validation where stale search results matter.
#[cfg(unix)]
pub struct FileWriteLock(std::fs::File);

#[cfg(unix)]
impl FileWriteLock {
    /// Acquire an exclusive advisory lock on `path`.
    ///
    /// Returns `Ok(None)` when the file does not exist yet. New Save As targets
    /// have no existing search result to coordinate with, and creating the final
    /// path just to lock it would break atomic-save semantics.
    ///
    /// # Errors
    ///
    /// Returns an error if the existing file cannot be opened or locked.
    pub fn acquire(path: &Path) -> std::io::Result<Option<Self>> {
        use std::fs::OpenOptions;
        use std::os::unix::io::AsRawFd;

        let file = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };

        // SAFETY: `fd` comes from a live `File` owned by the returned lock, and
        // `flock` only borrows that descriptor for the duration of the syscall.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Some(Self(file)))
    }
}

#[cfg(unix)]
impl Drop for FileWriteLock {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;

        // SAFETY: the descriptor still belongs to `self.0`, and releasing the
        // advisory lock is valid while that file handle remains open in `Drop`.
        let _ = unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// No-op advisory lock for non-Unix builds.
#[cfg(not(unix))]
pub struct FileWriteLock;

#[cfg(not(unix))]
impl FileWriteLock {
    /// Keep call sites portable; non-Unix builds do not participate in locking.
    pub fn acquire(_path: &Path) -> std::io::Result<Option<Self>> {
        Ok(Some(Self))
    }
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
    fn sync_parent_dir_reports_missing_parent() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("missing-parent/data.json");

        assert!(sync_parent_dir(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn sync_dir_reports_missing_directory() {
        let dir = TempDir::new().expect("expected operation to succeed");

        assert!(sync_dir(&dir.path().join("missing")).is_err());
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
                    .contains(".test."))
        );
    }

    #[test]
    fn unique_temp_path_changes_between_calls() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("session.json");

        let first = unique_temp_path(&path, "json");
        let second = unique_temp_path(&path, "json");

        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(dir.path()));
        assert_eq!(second.parent(), Some(dir.path()));
    }

    #[test]
    fn file_write_lock_accepts_existing_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("locked.txt");
        std::fs::write(&path, "content").expect("expected operation to succeed");

        let lock = FileWriteLock::acquire(&path).expect("expected operation to succeed");

        assert!(lock.is_some());
    }

    #[test]
    fn file_write_lock_skips_missing_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("missing.txt");

        let lock = FileWriteLock::acquire(&path).expect("expected operation to succeed");

        assert!(lock.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn file_write_lock_reports_non_missing_open_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");

        assert!(FileWriteLock::acquire(dir.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn file_write_lock_releases_on_drop() {
        use std::fs::OpenOptions;
        use std::os::unix::io::AsRawFd;

        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("locked.txt");
        std::fs::write(&path, "content").expect("expected operation to succeed");

        let lock = FileWriteLock::acquire(&path)
            .expect("lock file")
            .expect("existing file should lock");
        let second = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open second handle");

        // SAFETY: `second` owns a live file descriptor for the duration of the syscall.
        let locked_result =
            unsafe { libc::flock(second.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        assert_ne!(locked_result, 0, "second lock should fail while held");

        drop(lock);
        // SAFETY: `second` still owns the same live descriptor after the first lock is dropped.
        let unlocked_result =
            unsafe { libc::flock(second.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        assert_eq!(unlocked_result, 0, "second lock should succeed after drop");
        // SAFETY: unlocking the descriptor acquired above is valid while `second` is alive.
        let _ = unsafe { libc::flock(second.as_raw_fd(), libc::LOCK_UN) };
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

    #[test]
    fn missing_ancestors_collects_only_missing_prefix() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let nested = dir.path().join("a/b/c");
        let missing = missing_ancestors(&nested);

        assert_eq!(
            missing,
            vec![nested.clone(), dir.path().join("a/b"), dir.path().join("a")]
        );

        std::fs::create_dir_all(dir.path().join("a")).expect("create first missing ancestor");
        let missing = missing_ancestors(&nested);

        assert_eq!(missing, vec![nested, dir.path().join("a/b")]);
    }
}

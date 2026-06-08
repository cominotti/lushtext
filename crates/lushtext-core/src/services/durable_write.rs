// SPDX-License-Identifier: GPL-3.0-or-later

//! Filesystem durability helpers for persistence services.
//!
//! Linux filesystems such as ext4, XFS, and Btrfs only make a temp-file rename
//! crash-durable after the containing directory has also been synced. Keeping
//! that rule in one GTK-free service helper prevents each persistence caller
//! from remembering the filesystem contract by hand.
//!
//! Two invariants this module guarantees for every caller:
//!
//! - **Identity preservation.** A temp-file-then-rename replaces the destination
//!   inode, so without intervention an overwrite would silently reset
//!   permissions, ownership, ACLs, and extended attributes. Before the rename we
//!   copy that metadata from the existing destination onto the temp file, matching
//!   GNOME's `g_file_replace`.
//! - **No swallowed sync failures (fsyncgate).** Every `fsync`/`sync_all` on the
//!   temp file, the destination directory, and newly created directory entries is
//!   propagated. A failed sync surfaces as a [`DurableWriteError`] (classified as
//!   before- or after-rename) or an `io::Error`; it is never turned into a silent
//!   success. Because the temp-file sync happens *before* the rename, a failure
//!   there leaves the previous destination bytes intact, so the safe outcome is
//!   always "report failure, previous content preserved."

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};

use crate::services::filesystem::sys;

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

/// Distinguishes a write failure before the destination rename from one after it.
///
/// A `BeforeRename` failure (temp write, flush, temp `fsync`, metadata copy, or
/// the rename itself) leaves the destination's previous bytes intact. An
/// `AfterRename` failure means the new bytes are already live at the destination
/// but the parent-directory `fsync` that proves the rename durable did not
/// complete — the change is on disk yet not yet crash-durable, which callers must
/// report differently from a lost write.
#[derive(Debug)]
pub enum DurableWriteError {
    /// The destination still holds its previous bytes; nothing was committed.
    BeforeRename(std::io::Error),
    /// The new bytes are in place, but durability could not be confirmed.
    AfterRename(std::io::Error),
}

impl DurableWriteError {
    /// Flatten the classification back into a plain I/O error for callers that
    /// do not need to tell the two failure phases apart.
    #[must_use]
    pub fn into_io_error(self) -> std::io::Error {
        match self {
            Self::BeforeRename(error) | Self::AfterRename(error) => error,
        }
    }

    /// Borrow the underlying I/O error regardless of failure phase.
    #[must_use]
    pub fn io_error(&self) -> &std::io::Error {
        match self {
            Self::BeforeRename(error) | Self::AfterRename(error) => error,
        }
    }
}

impl std::fmt::Display for DurableWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.io_error().fmt(f)
    }
}

impl std::error::Error for DurableWriteError {}

#[cfg(test)]
fn atomic_write_bytes(path: &Path, tmp_tag: &str, bytes: &[u8]) -> std::io::Result<()> {
    atomic_write_bytes_classified(path, tmp_tag, bytes).map_err(DurableWriteError::into_io_error)
}

/// Atomically replace `path` with `bytes`, classifying any failure by phase.
///
/// The full durability contract runs in order: write the bytes to a uniquely
/// named temp file in the destination's own directory, flush and `fsync` that
/// temp file, copy the destination's identity metadata onto the temp file when
/// the destination already exists, rename the temp file over the destination,
/// and finally `fsync` the destination's parent directory.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread unless it is part of a synchronous shutdown safety path.
///
/// # Errors
///
/// Returns [`DurableWriteError::BeforeRename`] when the destination's previous
/// bytes are still intact, or [`DurableWriteError::AfterRename`] when the new
/// bytes are in place but the directory `fsync` failed.
pub fn atomic_write_bytes_classified(
    path: &Path,
    tmp_tag: &str,
    bytes: &[u8],
) -> Result<(), DurableWriteError> {
    atomic_write_stream_classified(path, tmp_tag, |writer| writer.write_all(bytes))
}

/// Atomically replace `path` with bytes while preserving metadata from `metadata_source`.
///
/// This is used by cross-filesystem copy fallback: the destination should take
/// the source file's mode and supported identity metadata, not keep whatever
/// happened to be at the destination before the fallback copy started.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread unless it is part of a synchronous shutdown safety path.
///
/// # Errors
///
/// Returns a before-rename failure for every content, metadata, or temp-sync
/// error, and an after-rename failure only when the parent directory cannot be
/// synced after the new file is visible.
pub fn atomic_write_bytes_with_metadata_source_classified(
    path: &Path,
    tmp_tag: &str,
    bytes: &[u8],
    metadata_source: &Path,
) -> Result<(), DurableWriteError> {
    atomic_write_stream_with_metadata_source_classified(path, tmp_tag, metadata_source, |writer| {
        writer.write_all(bytes)
    })
}

/// Stream content into a crash-durable atomic replacement.
///
/// The closure writes into a buffered temp-file writer. The helper then flushes,
/// applies required metadata, performs the final temp-file `sync_all()`, renames
/// into place, and syncs the destination parent directory. This lets JSON and
/// journal callers serialize directly into the temp file without building a
/// complete `Vec<u8>` first.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread unless it is part of a synchronous shutdown safety path.
///
/// # Errors
///
/// Returns [`DurableWriteError::BeforeRename`] when the destination's previous
/// bytes are still intact, or [`DurableWriteError::AfterRename`] when the new
/// bytes are in place but the directory `fsync` failed.
pub fn atomic_write_stream_classified<F>(
    path: &Path,
    tmp_tag: &str,
    write_content: F,
) -> Result<(), DurableWriteError>
where
    F: FnOnce(&mut dyn Write) -> std::io::Result<()>,
{
    atomic_write_stream_with_metadata(
        path,
        tmp_tag,
        MetadataSource::ExistingDestination,
        write_content,
    )
}

/// Stream content into `path` while preserving metadata from an explicit source path.
///
/// See [`atomic_write_bytes_with_metadata_source_classified`] for the copy
/// fallback use case.
///
/// # Errors
///
/// Returns a before-rename failure for writer, metadata, or final temp-sync
/// errors, and an after-rename failure only when the parent directory cannot be
/// synced after the new file is visible.
pub fn atomic_write_stream_with_metadata_source_classified<F>(
    path: &Path,
    tmp_tag: &str,
    metadata_source: &Path,
    write_content: F,
) -> Result<(), DurableWriteError>
where
    F: FnOnce(&mut dyn Write) -> std::io::Result<()>,
{
    atomic_write_stream_with_metadata(
        path,
        tmp_tag,
        MetadataSource::Explicit(metadata_source),
        write_content,
    )
}

/// Core state machine for streaming temp-file writes, metadata, rename, and sync.
///
/// This private helper owns the exact failure-boundary classification: every
/// operation before the rename reports `BeforeRename`, while parent-directory
/// sync after a successful rename reports `AfterRename`.
fn atomic_write_stream_with_metadata<F>(
    path: &Path,
    tmp_tag: &str,
    metadata_source: MetadataSource<'_>,
    write_content: F,
) -> Result<(), DurableWriteError>
where
    F: FnOnce(&mut dyn Write) -> std::io::Result<()>,
{
    let metadata_plan = match MetadataPlan::probe(path, metadata_source) {
        Ok(plan) => plan,
        Err(error) => return Err(DurableWriteError::BeforeRename(error)),
    };
    let tmp_path = unique_temp_path(path, tmp_tag);
    let file = match sys::create_temp_file(&tmp_path, metadata_plan.create_mode()) {
        Ok(file) => file,
        Err(error) => return Err(DurableWriteError::BeforeRename(error)),
    };

    let write_result = {
        let mut writer = std::io::BufWriter::new(&file);
        write_content(&mut writer).and_then(|()| writer.flush())
    }
    .and_then(|()| observe_temp_after_content_for_test(&tmp_path))
    .and_then(|()| metadata_plan.apply(&file))
    .and_then(|()| sync_temp_after_metadata(&file));

    if let Err(error) = write_result {
        let _ = sys::remove_file(&tmp_path);
        return Err(DurableWriteError::BeforeRename(error));
    }

    if let Err(error) = sys::rename(&tmp_path, path) {
        let _ = sys::remove_file(&tmp_path);
        return Err(DurableWriteError::BeforeRename(error));
    }
    // The rename has landed: the new bytes are now the destination. A failure
    // here means the change is visible but not yet crash-durable.
    sync_parent_dir(path).map_err(DurableWriteError::AfterRename)
}

/// Stable resolved identity for a write target.
///
/// Existing files and symlinks resolve to their canonical target. Missing files
/// use the canonical parent directory plus the requested file name, so Save As
/// can coordinate a not-yet-created target without opening the destination.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WriteTargetIdentity(PathBuf);

impl WriteTargetIdentity {
    /// Return the resolved path key used for write coordination.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Consume the identity and return its path key.
    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

/// Resolve a path into the stable identity shared by editor save and Replace All.
///
/// # Errors
///
/// Returns an error when neither the target nor its parent directory can be
/// canonicalized. Broken symlinks fail here, which keeps callers from replacing
/// the link itself by accident.
pub fn resolve_write_target_identity(path: &Path) -> std::io::Result<WriteTargetIdentity> {
    match sys::canonicalize(path) {
        Ok(canonical) => Ok(WriteTargetIdentity(canonical)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if sys::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("symlink target is unavailable: {}", path.display()),
                ));
            }
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let canonical_parent = sys::canonicalize(parent)?;
            let Some(file_name) = path.file_name() else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("write target has no file name: {}", path.display()),
                ));
            };
            Ok(WriteTargetIdentity(canonical_parent.join(file_name)))
        }
        Err(error) => Err(error),
    }
}

/// Process-local stable write guard for one resolved target path.
///
/// Atomic rename replaces destination inodes, so locking the old file handle is
/// the wrong coordination primitive for in-app races. This guard instead keys
/// on the resolved target path and stays independent of destination permissions.
#[derive(Debug)]
pub struct TargetWriteGuard {
    key: PathBuf,
}

impl TargetWriteGuard {
    /// Acquire the stable write guard for `path`, blocking until any in-app
    /// writer for the same resolved target has completed.
    ///
    /// # Errors
    ///
    /// Returns an error when the target identity cannot be resolved.
    pub fn acquire(path: &Path) -> std::io::Result<Self> {
        let identity = resolve_write_target_identity(path)?;
        Ok(Self::from_identity(identity))
    }

    /// Acquire the guard from a pre-resolved identity.
    #[must_use]
    pub fn from_identity(identity: WriteTargetIdentity) -> Self {
        let key = identity.into_path_buf();
        let locks = write_target_locks();
        let mut active = locks
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while active.contains(&key) {
            // Condition variables may wake spuriously; the active set is the
            // actual write-exclusion contract, so recheck it on every wake.
            active = locks
                .available
                .wait(active)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        active.insert(key.clone());
        drop(active);
        Self { key }
    }
}

impl Drop for TargetWriteGuard {
    fn drop(&mut self) {
        let locks = write_target_locks();
        {
            let mut active = locks
                .active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            active.remove(&self.key);
        }
        locks.available.notify_all();
    }
}

struct TargetWriteLocks {
    active: Mutex<HashSet<PathBuf>>,
    available: Condvar,
}

fn write_target_locks() -> &'static TargetWriteLocks {
    static LOCKS: OnceLock<TargetWriteLocks> = OnceLock::new();
    LOCKS.get_or_init(|| TargetWriteLocks {
        active: Mutex::new(HashSet::new()),
        available: Condvar::new(),
    })
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
    sys::rename(from, to)?;
    sync_parent_dir(from)?;
    if from.parent() != to.parent() {
        sync_parent_dir(to)?;
    }
    Ok(())
}

/// Create one directory and sync the parent directory that received the entry.
///
/// `create_dir()` mutates a directory namespace just like `rename()`, so callers
/// that create user-visible folders need the same parent-sync policy.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or the parent directory
/// cannot be synced after creation.
pub fn create_dir_durable(path: &Path) -> std::io::Result<()> {
    sys::create_dir(path)?;
    sync_parent_dir(path)
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
    let bytes = sys::read(from)?;
    atomic_write_bytes_with_metadata_source_classified(to, tmp_tag, &bytes, from)
        .map_err(DurableWriteError::into_io_error)?;
    sys::remove_file(from)?;
    sync_parent_dir(from)
}

#[derive(Clone, Copy)]
enum MetadataSource<'a> {
    ExistingDestination,
    Explicit(&'a Path),
}

#[cfg(unix)]
struct MetadataPlan {
    source: Option<UnixMetadataSource>,
}

#[cfg(unix)]
struct UnixMetadataSource {
    path: PathBuf,
    metadata: sys::UnixMetadata,
}

#[cfg(unix)]
impl MetadataPlan {
    /// Probe metadata before temp creation so restrictive destinations never get
    /// a wider temp sibling containing new bytes.
    fn probe(path: &Path, source: MetadataSource<'_>) -> std::io::Result<Self> {
        let (source_path, required) = match source {
            MetadataSource::ExistingDestination => (path, false),
            MetadataSource::Explicit(path) => (path, true),
        };

        match sys::required_metadata(source_path) {
            Ok(metadata) => Ok(Self {
                source: Some(UnixMetadataSource {
                    path: source_path.to_path_buf(),
                    metadata,
                }),
            }),
            Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self { source: None })
            }
            Err(error) => Err(error),
        }
    }

    /// Permission bits used at temp-file creation time.
    #[must_use]
    fn create_mode(&self) -> Option<u32> {
        self.source
            .as_ref()
            .map(|source| source.metadata.mode & 0o777)
    }

    /// Apply metadata that must be present before the final temp-file sync.
    fn apply(&self, temp: &sys::File) -> std::io::Result<()> {
        let Some(source) = &self.source else {
            return Ok(());
        };

        // Mode preservation is required. Best-effort metadata below can fail on
        // ordinary user saves, but silently widening a private file would be a
        // real safety bug.
        sys::apply_mode(temp, source.metadata.mode & 0o7777)?;
        sys::best_effort_chown(temp, source.metadata.uid, source.metadata.gid);
        sys::copy_xattrs_best_effort(&source.path, temp);
        Ok(())
    }
}

#[cfg(not(unix))]
struct MetadataPlan;

#[cfg(not(unix))]
impl MetadataPlan {
    fn probe(path: &Path, source: MetadataSource<'_>) -> std::io::Result<Self> {
        match source {
            MetadataSource::ExistingDestination => Ok(Self),
            MetadataSource::Explicit(source_path) => {
                sys::metadata(source_path)?;
                let _ = path;
                Ok(Self)
            }
        }
    }

    fn create_mode(&self) -> Option<u32> {
        None
    }

    fn apply(&self, _temp: &sys::File) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
fn observe_temp_after_content_for_test(path: &Path) -> std::io::Result<()> {
    if let Some(observer) = TEMP_AFTER_CONTENT_OBSERVER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("temp observer lock poisoned")
        .as_ref()
    {
        observer(path);
    }
    Ok(())
}

#[cfg(not(test))]
fn observe_temp_after_content_for_test(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn sync_temp_after_metadata(file: &sys::File) -> std::io::Result<()> {
    if FAIL_FINAL_TEMP_SYNC_AFTER_METADATA.with(|fail| fail.replace(false)) {
        return Err(std::io::Error::other(
            "injected final temp sync failure after metadata",
        ));
    }
    sys::sync_file(file)
}

#[cfg(not(test))]
fn sync_temp_after_metadata(file: &sys::File) -> std::io::Result<()> {
    sys::sync_file(file)
}

#[cfg(test)]
type TempObserver = Box<dyn Fn(&Path) + Send + Sync + 'static>;

/// Test-only hook that observes a temp file before metadata and final sync.
#[cfg(test)]
static TEMP_AFTER_CONTENT_OBSERVER: OnceLock<Mutex<Option<TempObserver>>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Test hook for failures that happen after metadata is applied but before rename.
    static FAIL_FINAL_TEMP_SYNC_AFTER_METADATA: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Test hook for the post-rename parent-directory sync failure path.
    static FAIL_NEXT_PARENT_SYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
    sys::create_dir_all(path)?;

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
    #[cfg(test)]
    if FAIL_NEXT_PARENT_SYNC.with(|fail| fail.replace(false)) {
        return Err(std::io::Error::other(
            "injected parent directory sync failure",
        ));
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    sync_dir(parent)
}

/// Sync a directory handle on Unix, where LushText's GTK target platforms live.
#[cfg(unix)]
fn sync_dir(path: &Path) -> std::io::Result<()> {
    sys::sync_dir_descriptor(path)
}

/// Keep non-Unix builds compiling even though the shipped target is Linux.
#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Collect ancestors that do not exist yet, starting at `path`.
fn missing_ancestors(path: &Path) -> Vec<PathBuf> {
    path.ancestors()
        .take_while(|ancestor| !sys::path_exists(ancestor))
        .map(Path::to_path_buf)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    #[test]
    fn create_dir_all_durable_creates_nested_tree() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let nested = dir.path().join("a/b/c");

        create_dir_all_durable(&nested).expect("expected operation to succeed");

        assert!(nested.is_dir());
    }

    #[test]
    fn create_dir_durable_creates_single_directory() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let child = dir.path().join("child");

        create_dir_durable(&child).expect("expected operation to succeed");

        assert!(child.is_dir());
    }

    #[test]
    fn sync_parent_dir_accepts_existing_parent() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("data.json");
        fixture::write_text(&path, "{}");

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

        assert_eq!(fixture::read_bytes(&path), b"new");
        assert!(
            fixture::entry_names(dir.path())
                .into_iter()
                .all(|entry| !entry.contains(".test."))
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
    fn target_write_guard_accepts_existing_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("locked.txt");
        fixture::write_text(&path, "content");

        let lock = TargetWriteGuard::acquire(&path).expect("expected operation to succeed");

        drop(lock);
    }

    #[test]
    fn target_write_guard_accepts_missing_file_with_existing_parent() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("missing.txt");

        let lock = TargetWriteGuard::acquire(&path).expect("expected operation to succeed");

        drop(lock);
    }

    #[cfg(unix)]
    #[test]
    fn target_write_guard_reports_missing_parent_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");

        assert!(TargetWriteGuard::acquire(&dir.path().join("missing/target")).is_err());
    }

    #[test]
    fn target_write_guard_releases_on_drop() {
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("locked.txt");
        fixture::write_text(&path, "content");

        let lock = TargetWriteGuard::acquire(&path).expect("lock file");
        let (tx, rx) = mpsc::channel();
        let path_for_thread = path;
        let thread = std::thread::spawn(move || {
            let _lock = TargetWriteGuard::acquire(&path_for_thread).expect("second lock");
            tx.send(()).expect("notify acquired");
        });

        assert!(
            rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "second lock should wait while first guard is held"
        );
        drop(lock);
        rx.recv_timeout(Duration::from_secs(2))
            .expect("second lock should acquire after drop");
        thread.join().expect("lock thread should finish");
    }

    #[cfg(unix)]
    #[test]
    fn target_write_guard_symlink_and_target_share_guard() {
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = TempDir::new().expect("expected operation to succeed");
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link.txt");
        fixture::write_text(&target, "content");
        fixture::symlink(&target, &link);

        let lock = TargetWriteGuard::acquire(&link).expect("lock symlink target");
        let (tx, rx) = mpsc::channel();
        let target_for_thread = target;
        let thread = std::thread::spawn(move || {
            let _lock = TargetWriteGuard::acquire(&target_for_thread).expect("lock target");
            tx.send(()).expect("notify acquired");
        });

        assert!(
            rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "canonical target should wait for symlink-held guard"
        );
        drop(lock);
        rx.recv_timeout(Duration::from_secs(2))
            .expect("target guard should acquire after symlink guard drops");
        thread.join().expect("lock thread should finish");
    }

    #[cfg(unix)]
    #[test]
    fn target_write_guard_accepts_read_only_destination() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("readonly.txt");
        fixture::write_text(&path, "content");
        fixture::set_mode(&path, 0o400);

        let lock =
            TargetWriteGuard::acquire(&path).expect("stable guard should not open read-write");

        drop(lock);
    }

    #[test]
    fn copy_file_durable_writes_destination_before_removing_source() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_text(&from, "snapshot");

        copy_file_durable(&from, &to, "copy").expect("expected operation to succeed");

        assert!(!fixture::exists(&from));
        fixture::assert_text(&to, "snapshot");
    }

    #[cfg(unix)]
    #[test]
    fn copy_file_durable_preserves_source_mode_over_existing_destination() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_text(&from, "source");
        fixture::write_text(&to, "dest");
        fixture::set_mode(&from, 0o644);
        fixture::set_mode(&to, 0o600);

        copy_file_durable(&from, &to, "copy").expect("copy fallback");

        assert!(!fixture::exists(&from));
        fixture::assert_text(&to, "source");
        assert_eq!(fixture::mode(&to) & 0o777, 0o644);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn copy_file_durable_preserves_source_user_xattr_when_supported() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_bytes(&from, b"source");
        fixture::write_bytes(&to, b"dest");

        let name = "user.lushtext_copy_test";
        let value = b"source-xattr";
        if fixture::set_xattr(&from, name, value).is_err() {
            eprintln!("skipping copy xattr test: setxattr unsupported here");
            return;
        }

        copy_file_durable(&from, &to, "copy").expect("copy fallback");

        let read_back =
            fixture::get_xattr(&to, name).expect("source user xattr must survive copy fallback");
        assert_eq!(read_back, value);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn copy_file_durable_preserves_source_posix_acl_when_supported() {
        use std::process::Command;

        let (Ok(setfacl), Ok(getfacl)) = (which("setfacl"), which("getfacl")) else {
            eprintln!("skipping copy ACL test: setfacl/getfacl not installed");
            return;
        };

        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_bytes(&from, b"source");
        fixture::write_bytes(&to, b"dest");

        let applied = Command::new(&setfacl)
            .args(["-m", "u:12345:r--"])
            .arg(&from)
            .status();
        match applied {
            Ok(status) if status.success() => {}
            _ => {
                eprintln!("skipping copy ACL test: setfacl unsupported here");
                return;
            }
        }

        copy_file_durable(&from, &to, "copy").expect("copy fallback");

        let after = Command::new(&getfacl)
            .arg("--omit-header")
            .arg(&to)
            .output()
            .expect("read back ACL");
        let acl_text = String::from_utf8_lossy(&after.stdout);
        assert!(
            acl_text.contains("user:12345:"),
            "the source named-user ACL entry must survive copy fallback, got:\n{acl_text}"
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

        fixture::create_dir_all(&dir.path().join("a"));
        let missing = missing_ancestors(&nested);

        assert_eq!(missing, vec![nested, dir.path().join("a/b")]);
    }

    #[test]
    fn durable_write_error_flattens_and_displays_underlying_io_error() {
        let before = DurableWriteError::BeforeRename(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "nope",
        ));
        assert_eq!(
            before.io_error().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert_eq!(before.to_string(), "nope");
        assert_eq!(
            before.into_io_error().kind(),
            std::io::ErrorKind::PermissionDenied
        );

        let after = DurableWriteError::AfterRename(std::io::Error::other("dir"));
        assert_eq!(after.into_io_error().kind(), std::io::ErrorKind::Other);
    }

    #[test]
    fn atomic_write_bytes_classified_reports_before_rename_when_temp_cannot_be_created() {
        let dir = TempDir::new().expect("expected operation to succeed");
        // The parent directory does not exist, so even creating the temp file
        // fails: a before-rename failure that must leave no destination behind.
        let path = dir.path().join("missing-dir/data.txt");

        let error = atomic_write_bytes_classified(&path, "test", b"bytes")
            .expect_err("missing parent must fail the write");

        assert!(
            matches!(error, DurableWriteError::BeforeRename(_)),
            "a pre-rename failure must classify as BeforeRename"
        );
        assert!(
            !fixture::exists(&path),
            "the destination must not be created"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_bytes_preserves_existing_mode_on_overwrite() {
        let dir = TempDir::new().expect("expected operation to succeed");

        let private = dir.path().join("private.bin");
        fixture::write_bytes(&private, b"old");
        fixture::set_mode(&private, 0o600);
        atomic_write_bytes(&private, "test", b"new").expect("overwrite");
        assert_eq!(
            fixture::mode(&private) & 0o777,
            0o600,
            "overwrite must not widen a 0600 file"
        );

        let exec = dir.path().join("tool");
        fixture::write_bytes(&exec, b"old");
        fixture::set_mode(&exec, 0o755);
        atomic_write_bytes(&exec, "test", b"new").expect("overwrite exec");
        assert_ne!(
            fixture::mode(&exec) & 0o111,
            0,
            "overwrite must keep the executable bit"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_temp_with_new_bytes_is_not_wider_than_private_destination() {
        use std::sync::{Arc, Mutex as StdMutex};

        let dir = TempDir::new().expect("expected operation to succeed");
        let private = dir.path().join("private.bin");
        fixture::write_bytes(&private, b"old");
        fixture::set_mode(&private, 0o600);

        let observed_mode = Arc::new(StdMutex::new(None));
        let observed_mode_for_hook = observed_mode.clone();
        let observed_parent = dir.path().to_path_buf();
        *TEMP_AFTER_CONTENT_OBSERVER
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("observer lock") = Some(Box::new(move |tmp_path| {
            if tmp_path.parent() != Some(observed_parent.as_path()) {
                return;
            }
            let mode = fixture::mode(tmp_path) & 0o777;
            *observed_mode_for_hook.lock().expect("mode lock") = Some(mode);
        }));

        atomic_write_bytes(&private, "test", b"new").expect("overwrite");
        *TEMP_AFTER_CONTENT_OBSERVER
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("observer lock") = None;

        assert_eq!(
            *observed_mode.lock().expect("mode lock"),
            Some(0o600),
            "temp file already containing new bytes must stay private"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_final_sync_after_metadata_failure_is_before_rename() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("data.txt");
        fixture::write_bytes(&path, b"old");
        fixture::set_mode(&path, 0o600);

        FAIL_FINAL_TEMP_SYNC_AFTER_METADATA.with(|fail| fail.set(true));
        let error = atomic_write_bytes_classified(&path, "test", b"new")
            .expect_err("injected final sync failure should fail");

        assert!(
            matches!(error, DurableWriteError::BeforeRename(_)),
            "final temp sync after metadata is still before the rename"
        );
        assert_eq!(fixture::read_bytes(&path), b"old");
        assert_eq!(fixture::mode(&path) & 0o777, 0o600);
    }

    #[test]
    fn atomic_write_parent_sync_failure_is_after_rename() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("data.txt");
        fixture::write_bytes(&path, b"old");

        FAIL_NEXT_PARENT_SYNC.with(|fail| fail.set(true));
        let error = atomic_write_bytes_classified(&path, "test", b"new")
            .expect_err("injected parent sync failure should fail");

        assert!(
            matches!(error, DurableWriteError::AfterRename(_)),
            "parent sync failure happens after the rename has landed"
        );
        assert_eq!(
            fixture::read_bytes(&path),
            b"new",
            "after-rename failure means the new bytes are already visible"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_bytes_new_file_uses_default_permissions() {
        let dir = TempDir::new().expect("expected operation to succeed");

        // A plain new-file helper in the same directory establishes what
        // "default" means under the current umask, so the comparison stays
        // umask-agnostic.
        let reference = dir.path().join("reference");
        fixture::write_bytes(&reference, []);
        let reference_mode = fixture::mode(&reference) & 0o777;

        let fresh = dir.path().join("fresh");
        atomic_write_bytes(&fresh, "test", b"hello").expect("new-file write");
        let fresh_mode = fixture::mode(&fresh) & 0o777;

        assert_eq!(
            fresh_mode, reference_mode,
            "a brand-new atomic write must use default permissions, not inherit anything"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn atomic_write_bytes_preserves_user_xattr_when_supported() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("tagged.bin");
        fixture::write_bytes(&path, b"old");

        let name = "user.lushtext_test";
        let value = b"durable";

        if fixture::set_xattr(&path, name, value).is_err() {
            // tmpfs and some CI filesystems reject user xattrs (ENOTSUP): the
            // behavior is genuinely unavailable here, so skip rather than fail.
            eprintln!("skipping xattr preservation test: setxattr unsupported here");
            return;
        }

        atomic_write_bytes(&path, "test", b"new").expect("overwrite preserves xattr");

        let read_back =
            fixture::get_xattr(&path, name).expect("the user xattr must survive the overwrite");
        assert_eq!(
            read_back, value,
            "the xattr value must be preserved exactly"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn atomic_write_bytes_preserves_posix_acl_when_supported() {
        use std::process::Command;

        // A POSIX access ACL is stored in the `system.posix_acl_access` xattr,
        // so the same xattr copy that preserves `user.*` attributes should carry
        // ACL entries across an overwrite. Prove it end-to-end with the system
        // ACL tools, skipping where they are missing or the filesystem rejects
        // ACLs (the behavior is genuinely unavailable there).
        let (Ok(setfacl), Ok(getfacl)) = (which("setfacl"), which("getfacl")) else {
            eprintln!("skipping ACL preservation test: setfacl/getfacl not installed");
            return;
        };

        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("acl-target.bin");
        fixture::write_bytes(&path, b"old");

        // Grant an extra named-user ACL entry (numeric uid avoids needing a real
        // account). If the filesystem does not support ACLs, skip.
        let applied = Command::new(&setfacl)
            .args(["-m", "u:12345:r--"])
            .arg(&path)
            .status();
        match applied {
            Ok(status) if status.success() => {}
            _ => {
                eprintln!("skipping ACL preservation test: setfacl unsupported here");
                return;
            }
        }

        atomic_write_bytes(&path, "test", b"new").expect("overwrite preserves ACL");

        let after = Command::new(&getfacl)
            .arg("--omit-header")
            .arg(&path)
            .output()
            .expect("read back ACL");
        let acl_text = String::from_utf8_lossy(&after.stdout);
        assert!(
            acl_text.contains("user:12345:"),
            "the named-user ACL entry must survive the overwrite, got:\n{acl_text}"
        );
    }

    /// Resolve a system tool's absolute path for tests, honoring `PATH`.
    #[cfg(target_os = "linux")]
    fn which(tool: &str) -> std::io::Result<PathBuf> {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {tool}"))
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound));
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            return Err(std::io::Error::from(std::io::ErrorKind::NotFound));
        }
        Ok(PathBuf::from(path))
    }
}

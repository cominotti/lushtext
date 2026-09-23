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
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};

use crate::services::filesystem::write_protocol::{
    MoveAction, MoveProtocol, RenameAction, RenameProtocol, StepOutcome, WriteAction, WriteClass,
    WriteProtocol,
};
use crate::services::filesystem::{sys, temp_name};

/// Process-local counter for temp-file names that may be created concurrently.
///
/// Including the process ID and this counter keeps overlapping writes from
/// reusing the same temp path while still leaving recognizable filenames for
/// crash leftovers.
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The current directory, as the parent of a destination with no directory part.
const CURRENT_DIR: &str = ".";

/// Map the empty path, which `Path::ancestors` and `Path::parent` yield for a
/// bare relative name, to the current directory it denotes.
fn current_if_empty(path: &Path) -> &Path {
    if path.as_os_str().is_empty() {
        Path::new(CURRENT_DIR)
    } else {
        path
    }
}

/// The directory that holds `path`'s entry.
///
/// `Path::parent` returns `Some("")` for a bare name such as `notes.md` and
/// `None` for a root or empty path; both mean the current directory here.
/// Opening `""` fails with `ENOENT`, so without this a successful write to a
/// bare relative name would report an after-rename durability failure.
pub(crate) fn parent_or_current(path: &Path) -> &Path {
    path.parent()
        .map_or_else(|| Path::new(CURRENT_DIR), current_if_empty)
}

/// This launch's nonce, carried in the high 32 bits of every temp name's
/// sequence field.
///
/// A process id alone does not identify a launch: inside a Flatpak pid
/// namespace every launch tends to get the same small pid, and host pids are
/// reused. With only the pid, a crash leftover from an earlier launch would
/// look like this process's own in-flight temp file forever (never swept),
/// and a restarted counter could collide with it (`EEXIST`). The pair (pid,
/// nonce) tells this launch's temps from every earlier one while keeping the
/// name shape `.{file}.{tag}.{pid}.{seq}.tmp`.
pub(crate) fn temp_name_launch_nonce() -> u32 {
    static NONCE: OnceLock<u32> = OnceLock::new();
    *NONCE.get_or_init(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        // Fold the whole timestamp so launches a few seconds apart differ;
        // keeping only the low 32 bits of the fold is the point.
        let folded = nanos ^ (nanos >> 32) ^ (nanos >> 64);
        u32::try_from(folded & u128::from(u32::MAX)).map_or(1, |nonce| nonce.max(1))
    })
}

/// Build a unique hidden temp path next to the final destination.
#[must_use]
pub fn unique_temp_path(path: &Path, tmp_tag: &str) -> PathBuf {
    let parent = parent_or_current(path);
    let file_name = path
        .file_name()
        .map_or_else(|| "untitled".into(), OsStr::to_string_lossy);
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    parent.join(temp_name::format(
        &file_name,
        tmp_tag,
        std::process::id(),
        temp_name::sequence(temp_name_launch_nonce(), counter),
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
    // The shell: every decision is `WriteProtocol`'s. Each action is exactly
    // one backend call, and its outcome goes back to the protocol unchanged;
    // the shell only keeps the resources those calls produce and the latest
    // error, which the protocol's classification is reported with.
    let (mut protocol, mut action) = WriteProtocol::start();
    let mut metadata_plan = None;
    let mut temp: Option<(PathBuf, sys::File)> = None;
    let mut write_content = Some(write_content);
    let mut last_error: Option<std::io::Error> = None;
    loop {
        let outcome = match action {
            WriteAction::ProbeMetadata => {
                outcome_of(MetadataPlan::probe(path, metadata_source), &mut last_error)
                    .map(|plan| metadata_plan = Some(plan))
            }
            WriteAction::CreateTemp => {
                let tmp_path = unique_temp_path(path, tmp_tag);
                let mode = metadata_plan.as_ref().and_then(MetadataPlan::create_mode);
                match sys::create_temp_file(&tmp_path, mode) {
                    Ok(file) => {
                        temp = Some((tmp_path, file));
                        Ok(())
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        last_error = Some(error);
                        Err(StepOutcome::AlreadyExists)
                    }
                    Err(error) => {
                        last_error = Some(error);
                        Err(StepOutcome::Failed)
                    }
                }
            }
            WriteAction::WriteContent => {
                let (tmp_path, file) = temp
                    .as_ref()
                    .expect("the protocol writes only after creating");
                let write_content = write_content
                    .take()
                    .expect("the protocol writes content once");
                outcome_of(
                    {
                        let mut writer = std::io::BufWriter::new(file);
                        write_content(&mut writer).and_then(|()| writer.flush())
                    }
                    .and_then(|()| observe_temp_after_content_for_test(tmp_path)),
                    &mut last_error,
                )
            }
            WriteAction::ApplyMetadata => {
                let (_, file) = temp
                    .as_ref()
                    .expect("the protocol applies metadata to its temp");
                let plan = metadata_plan
                    .as_ref()
                    .expect("the protocol probed metadata first");
                outcome_of(plan.apply(file), &mut last_error)
            }
            WriteAction::SyncTemp => {
                let (_, file) = temp.as_ref().expect("the protocol syncs its temp");
                outcome_of(sync_temp_after_metadata(file), &mut last_error)
            }
            WriteAction::Rename => {
                let (tmp_path, _) = temp.as_ref().expect("the protocol renames its temp");
                outcome_of(sys::rename(tmp_path, path), &mut last_error)
            }
            WriteAction::SyncDir => {
                // The rename has landed: the new bytes are now the
                // destination. A failure here means the change is visible but
                // not yet crash-durable.
                crate::services::kill_point::reach(
                    crate::services::kill_point::KillWindow::DurableRenamedBeforeDirSync,
                    tmp_tag,
                );
                outcome_of(sync_parent_dir(path), &mut last_error)
            }
            WriteAction::RemoveTemp => {
                if let Some((tmp_path, file)) = temp.take() {
                    drop(file);
                    let _ = sys::remove_file(&tmp_path);
                }
                Ok(())
            }
            WriteAction::Finish(class) => {
                let mut error = || {
                    last_error
                        .take()
                        .unwrap_or_else(|| std::io::Error::other("durable write failed"))
                };
                return match class {
                    WriteClass::Success => Ok(()),
                    WriteClass::BeforeRename => Err(DurableWriteError::BeforeRename(error())),
                    WriteClass::AfterRename => Err(DurableWriteError::AfterRename(error())),
                };
            }
        };
        (protocol, action) = protocol.step(match outcome {
            Ok(()) => StepOutcome::Done,
            Err(step_outcome) => step_outcome,
        });
    }
}

/// Map one backend result to the protocol's outcome, keeping its error.
fn outcome_of<T>(
    result: std::io::Result<T>,
    last_error: &mut Option<std::io::Error>,
) -> Result<T, StepOutcome> {
    result.map_err(|error| {
        *last_error = Some(error);
        StepOutcome::Failed
    })
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
            let parent = parent_or_current(path);
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
    #[cfg(test)]
    if FAIL_NEXT_RENAME_CROSS_DEVICE.with(|fail| fail.replace(false)) {
        return Err(sys::cross_device_error_for_test());
    }
    run_rename_protocol(from, to, sys::rename)
}

/// Rename durably, refusing atomically when the destination already exists.
///
/// [`rename_durable`] is `rename(2)`, which silently replaces a regular
/// destination and destroys its contents unrecoverably. A caller that must not do
/// that cannot get there with an existence check plus a rename: those are two
/// syscalls, and another process can create the destination in between. This
/// pushes the check into the kernel via `RENAME_NOREPLACE`.
///
/// # Errors
///
/// Returns [`std::io::ErrorKind::AlreadyExists`] when the destination exists, and
/// [`std::io::ErrorKind::Unsupported`] when the kernel or filesystem does not
/// implement the flag — callers fall back to a best-effort check rather than
/// refusing every rename.
pub fn rename_durable_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    run_rename_protocol(from, to, sys::rename_no_replace)
}

/// The shell of [`RenameProtocol`]: rename with `rename`, then sync whichever
/// parent directories the rename mutated, one backend call per action.
fn run_rename_protocol(
    from: &Path,
    to: &Path,
    rename: fn(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let (mut protocol, mut action) =
        RenameProtocol::start(parent_or_current(from) != parent_or_current(to));
    let mut last_error = None;
    loop {
        let result = match action {
            RenameAction::Rename => rename(from, to),
            RenameAction::SyncSourceDir => sync_parent_dir(from),
            RenameAction::SyncDestinationDir => sync_parent_dir(to),
            RenameAction::Finish(true) => return Ok(()),
            RenameAction::Finish(false) => {
                return Err(last_error.unwrap_or_else(|| std::io::Error::other("rename failed")));
            }
        };
        let succeeded = result.is_ok();
        if let Err(error) = result {
            last_error = Some(error);
        }
        (protocol, action) = protocol.step(succeeded);
    }
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

/// Copy a file durably: the destination receives the source's bytes and
/// metadata through the full atomic-write protocol. The source is never
/// touched.
///
/// # Errors
///
/// Returns an error if the source cannot be read or the destination cannot be
/// atomically written.
pub fn copy_durable(from: &Path, to: &Path, tmp_tag: &str) -> std::io::Result<()> {
    let bytes = sys::read(from)?;
    atomic_write_bytes_with_metadata_source_classified(to, tmp_tag, &bytes, from)
        .map_err(DurableWriteError::into_io_error)
}

/// Move a file durably by copying: [`copy_durable`], then remove the source
/// and sync its directory. This is the cross-filesystem fallback for
/// [`rename_durable`].
///
/// The source is removed only after the destination's bytes **and** its
/// directory entry are durable, so any failure before that leaves the source
/// in place.
///
/// # Errors
///
/// Returns an error if the copy fails, the source cannot be removed, or the
/// source directory cannot be synced after removal.
pub fn move_durable(from: &Path, to: &Path, tmp_tag: &str) -> std::io::Result<()> {
    let (mut protocol, mut action) = MoveProtocol::start();
    let mut last_error = None;
    loop {
        let result = match action {
            MoveAction::Copy => copy_durable(from, to, tmp_tag),
            MoveAction::RemoveSource => sys::remove_file(from),
            MoveAction::SyncSourceDir => sync_parent_dir(from),
            MoveAction::Finish(true) => return Ok(()),
            MoveAction::Finish(false) => {
                return Err(last_error.unwrap_or_else(|| std::io::Error::other("move failed")));
            }
        };
        let succeeded = result.is_ok();
        if let Err(error) = result {
            last_error = Some(error);
        }
        (protocol, action) = protocol.step(succeeded);
    }
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

/// Sync the temp file after metadata has been applied but before rename.
///
/// A failure here is still before the destination swap, so callers must see it
/// as recoverable without the previous bytes being replaced.
fn sync_temp_after_metadata(file: &sys::File) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_FINAL_TEMP_SYNC_AFTER_METADATA.with(|fail| fail.replace(false)) {
        return Err(std::io::Error::other(
            "injected final temp sync failure after metadata",
        ));
    }
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
    /// Test hook for a rename refused because source and target are on different filesystems.
    static FAIL_NEXT_RENAME_CROSS_DEVICE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Make the next durable rename on this test thread fail with `EXDEV`, before it runs.
#[cfg(test)]
pub(in crate::services) fn fail_next_rename_cross_device_for_test() {
    FAIL_NEXT_RENAME_CROSS_DEVICE.with(|fail| fail.set(true));
}

/// Make the next parent-directory sync on this test thread fail after rename.
#[cfg(test)]
pub(crate) fn fail_next_parent_sync_for_test() {
    FAIL_NEXT_PARENT_SYNC.with(|fail| fail.set(true));
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

    sync_dir(parent_or_current(path))
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
///
/// A relative path's last ancestor is `""`, the current directory, which
/// always exists; probing it as `""` would report it missing.
fn missing_ancestors(path: &Path) -> Vec<PathBuf> {
    path.ancestors()
        .take_while(|ancestor| !sys::path_exists(current_if_empty(ancestor)))
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
    fn copy_durable_keeps_its_source() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_text(&from, "snapshot");

        copy_durable(&from, &to, "copy").expect("copy");

        fixture::assert_text(&from, "snapshot");
        fixture::assert_text(&to, "snapshot");
    }

    #[test]
    fn move_durable_writes_destination_before_removing_source() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_text(&from, "snapshot");

        move_durable(&from, &to, "copy").expect("expected operation to succeed");

        assert!(!fixture::exists(&from));
        fixture::assert_text(&to, "snapshot");
    }

    #[test]
    fn move_durable_keeps_its_source_when_the_destination_is_not_durable() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_text(&from, "snapshot");

        fail_next_parent_sync_for_test();
        move_durable(&from, &to, "copy").expect_err("the destination sync failed");

        fixture::assert_text(&from, "snapshot");
    }

    #[cfg(unix)]
    #[test]
    fn move_durable_preserves_source_mode_over_existing_destination() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_text(&from, "source");
        fixture::write_text(&to, "dest");
        fixture::set_mode(&from, 0o644);
        fixture::set_mode(&to, 0o600);

        move_durable(&from, &to, "copy").expect("copy fallback");

        assert!(!fixture::exists(&from));
        fixture::assert_text(&to, "source");
        assert_eq!(fixture::mode(&to) & 0o777, 0o644);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn move_durable_preserves_source_user_xattr_when_supported() {
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

        move_durable(&from, &to, "copy").expect("copy fallback");

        let read_back =
            fixture::get_xattr(&to, name).expect("source user xattr must survive copy fallback");
        assert_eq!(read_back, value);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn move_durable_preserves_source_posix_acl_when_supported() {
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

        move_durable(&from, &to, "copy").expect("copy fallback");

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
    fn parent_or_current_maps_bare_and_empty_parents_to_current_dir() {
        assert_eq!(parent_or_current(Path::new("notes.md")), Path::new("."));
        assert_eq!(parent_or_current(Path::new("./notes.md")), Path::new("."));
        assert_eq!(parent_or_current(Path::new("")), Path::new("."));
        assert_eq!(parent_or_current(Path::new("/")), Path::new("."));
        assert_eq!(
            parent_or_current(Path::new("dir/notes.md")),
            Path::new("dir")
        );
        assert_eq!(
            parent_or_current(Path::new("/tmp/notes.md")),
            Path::new("/tmp")
        );
    }

    #[test]
    fn missing_ancestors_of_a_relative_path_stop_before_the_empty_prefix() {
        let relative = Path::new("lushtext-missing-ancestor-probe/child");
        assert!(!sys::path_exists(Path::new(
            "lushtext-missing-ancestor-probe"
        )));

        let missing = missing_ancestors(relative);

        assert_eq!(
            missing,
            vec![
                relative.to_path_buf(),
                PathBuf::from("lushtext-missing-ancestor-probe")
            ],
            "the empty ancestor of a relative path is the current directory, which exists"
        );
    }

    /// Environment marker that turns the bare-name test into its own child.
    const BARE_NAME_CHILD_ENV: &str = "LUSHTEXT_DURABLE_WRITE_BARE_NAME_CHILD";

    /// A durable write to a bare file name must sync `.`, not fail on `""`.
    ///
    /// The working directory is process-global and unit tests share a process
    /// under `cargo test`, so the test re-runs itself as a child process whose
    /// working directory is a fresh temp dir instead of calling
    /// `set_current_dir` under its siblings.
    #[test]
    fn atomic_write_to_bare_relative_name_syncs_current_directory() {
        if std::env::var_os(BARE_NAME_CHILD_ENV).is_some() {
            atomic_write_bytes_classified(Path::new("bare.txt"), "test", b"bare")
                .expect("a bare relative destination must write durably");
            create_dir_all_durable(Path::new("fresh/nested"))
                .expect("a relative directory tree must be created durably");
            return;
        }

        let dir = TempDir::new().expect("expected operation to succeed");
        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "services::durable_write::tests::atomic_write_to_bare_relative_name_syncs_current_directory",
                "--nocapture",
            ])
            .env(BARE_NAME_CHILD_ENV, "1")
            .current_dir(dir.path())
            .output()
            .expect("spawn child test");

        assert!(
            output.status.success(),
            "child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "the child must actually run the test body:\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(fixture::read_bytes(&dir.path().join("bare.txt")), b"bare");
        assert!(dir.path().join("fresh/nested").is_dir());
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

    #[test]
    fn explicit_metadata_source_must_exist_before_writing_destination() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let source = dir.path().join("missing-source.txt");
        let destination = dir.path().join("destination.txt");

        let error = atomic_write_bytes_with_metadata_source_classified(
            &destination,
            "copy",
            b"new",
            &source,
        )
        .expect_err("missing explicit metadata source must abort before writing");

        assert!(
            matches!(error, DurableWriteError::BeforeRename(_)),
            "explicit metadata-source failure happens before destination rename"
        );
        assert!(
            !fixture::exists(&destination),
            "destination must not be created when source metadata is required"
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
    fn atomic_write_bytes_preserves_special_mode_bits_on_overwrite() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("sticky-tool");
        fixture::write_bytes(&path, b"old");
        fixture::set_mode(&path, 0o1755);

        atomic_write_bytes(&path, "test", b"new").expect("overwrite special mode");

        assert_eq!(
            fixture::mode(&path) & 0o7777,
            0o1755,
            "overwrite must restore mode bits that temp-file creation cannot apply"
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

    #[test]
    fn atomic_write_content_failure_is_before_rename_and_removes_the_temp() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("data.txt");
        fixture::write_bytes(&path, b"old");

        let error = atomic_write_stream_classified(&path, "test", |writer| {
            writer.write_all(b"partial")?;
            Err(std::io::Error::other("serializer failed"))
        })
        .expect_err("a failing content stream must fail the write");

        assert!(matches!(error, DurableWriteError::BeforeRename(_)));
        assert_eq!(fixture::read_bytes(&path), b"old");
        assert_eq!(
            fixture::entry_names(dir.path()),
            vec!["data.txt".to_string()]
        );
    }

    #[test]
    fn atomic_write_rename_failure_is_before_rename_and_removes_the_temp() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("occupied");
        fixture::create_dir_all(&path.join("child"));

        let error = atomic_write_bytes_classified(&path, "test", b"new")
            .expect_err("renaming a file over a non-empty directory fails");

        assert!(matches!(error, DurableWriteError::BeforeRename(_)));
        assert!(fixture::exists(&path.join("child")));
        assert_eq!(
            fixture::entry_names(dir.path()),
            vec!["occupied".to_string()]
        );
    }

    #[test]
    fn atomic_write_metadata_probe_failure_is_before_rename_and_creates_nothing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("data.txt");
        let missing_source = dir.path().join("missing-source.txt");

        let error = atomic_write_bytes_with_metadata_source_classified(
            &path,
            "test",
            b"new",
            &missing_source,
        )
        .expect_err("a required metadata source that is missing fails first");

        assert!(matches!(error, DurableWriteError::BeforeRename(_)));
        assert!(fixture::entry_names(dir.path()).is_empty());
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

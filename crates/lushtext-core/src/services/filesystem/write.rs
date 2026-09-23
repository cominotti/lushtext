// SPDX-License-Identifier: GPL-3.0-or-later

//! Durable write operations exposed through the filesystem boundary.
//!
//! This module preserves the existing crash-durable write contract while keeping
//! caller-facing entry points inside `services::filesystem`.

use std::io::Write;
use std::path::Path;

use crate::services::durable_write;

use super::{sys, types::WriteLabel};

pub(crate) use durable_write::parent_or_current;
#[cfg(test)]
pub(crate) use durable_write::unique_temp_path;
pub use durable_write::{DurableWriteError, TargetWriteGuard, WriteTargetIdentity};
pub(crate) use durable_write::{temp_name_launch_nonce, temp_sequence_launch_nonce};

/// Resolve the stable target identity used by coordinated writes.
///
/// # Errors
///
/// Returns an error when neither the target nor its parent can be resolved.
pub fn resolve_target_identity(path: &Path) -> std::io::Result<WriteTargetIdentity> {
    durable_write::resolve_write_target_identity(path)
}

/// Atomically replace a path with bytes and preserve destination identity metadata.
///
/// # Errors
///
/// Returns a classified durable write error if any write, metadata, rename, or
/// parent-directory sync step fails.
pub fn atomic_replace(
    path: &Path,
    label: WriteLabel,
    bytes: &[u8],
) -> Result<(), DurableWriteError> {
    durable_write::atomic_write_bytes_classified(path, label.as_str(), bytes)
}

/// Atomically replace a file in a user workspace, then sweep its own crash leftovers.
///
/// Identical to [`atomic_replace`], plus one bounded, non-recursive pass over
/// `path`'s directory after the write succeeded that removes stale temp files
/// left by an earlier interrupted write of this same file name. Pass the
/// resolved target path the write used. The sweep never reports failure: a
/// leftover it cannot remove today is kept for a later save.
///
/// # Errors
///
/// Returns the same classified durable write error as [`atomic_replace`]; the
/// sweep runs only when the write succeeded.
pub fn atomic_replace_workspace_file(
    path: &Path,
    label: WriteLabel,
    bytes: &[u8],
) -> Result<(), DurableWriteError> {
    atomic_replace(path, label, bytes)?;
    let report = super::leftovers::sweep_target_leftovers(path);
    if report.removed > 0 {
        tracing::debug!(
            "Removed {} stale durable-write leftover(s) beside {}",
            report.removed,
            path.display()
        );
    }
    Ok(())
}

/// Stream a durable atomic replacement into `path`.
///
/// # Errors
///
/// Returns a classified durable write error if serialization or any durability
/// step fails.
pub fn atomic_replace_stream<F>(
    path: &Path,
    label: WriteLabel,
    write_content: F,
) -> Result<(), DurableWriteError>
where
    F: FnOnce(&mut dyn Write) -> std::io::Result<()>,
{
    durable_write::atomic_write_stream_classified(path, label.as_str(), write_content)
}

/// Create a directory tree and sync newly-created directory entries.
///
/// # Errors
///
/// Returns an error when creation or sync fails.
pub fn create_dir_all_durable(path: &Path) -> std::io::Result<()> {
    durable_write::create_dir_all_durable(path)
}

/// Create one directory and sync its parent directory.
///
/// # Errors
///
/// Returns an error when creation or sync fails.
pub fn create_dir_durable(path: &Path) -> std::io::Result<()> {
    durable_write::create_dir_durable(path)
}

/// Rename a path and sync the affected parent directories.
///
/// # Errors
///
/// Returns an error when the rename or directory sync fails.
pub fn rename_durable(from: &Path, to: &Path) -> std::io::Result<()> {
    durable_write::rename_durable(from, to)
}

/// Rename durably, refusing atomically when the destination already exists.
///
/// Use this instead of [`rename_durable`] wherever replacing the destination
/// would destroy user data. See `durable_write::rename_durable_no_replace` for
/// why an existence check plus a rename is not equivalent.
///
/// # Errors
///
/// [`std::io::ErrorKind::AlreadyExists`] when the destination exists;
/// [`std::io::ErrorKind::Unsupported`] when the platform lacks the flag.
pub fn rename_durable_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    durable_write::rename_durable_no_replace(from, to)
}

/// Whether a rename failed only because source and target are on different filesystems.
///
/// This is the one rename failure a copy fallback may answer. Every other
/// error — including a parent-directory sync failure after the rename already
/// took effect, when the source is gone — must propagate so the caller can
/// retry from what is actually on disk.
#[must_use]
pub fn is_cross_device(error: &std::io::Error) -> bool {
    sys::is_cross_device(error)
}

/// Copy a file with durable destination replacement and source cleanup support.
///
/// # Errors
///
/// Returns an error when the source cannot be read, the destination cannot be
/// written durably, or source metadata cannot be preserved.
pub fn copy_file_durable(from: &Path, to: &Path, label: WriteLabel) -> std::io::Result<()> {
    durable_write::copy_file_durable(from, to, label.as_str())
}

/// Create an empty file only when the destination is absent, then sync its parent.
///
/// # Errors
///
/// Returns an error when the file already exists, cannot be created, cannot be
/// synced, or the parent directory cannot be synced.
pub fn create_new_empty_file_durable(path: &Path) -> std::io::Result<()> {
    sys::create_new_empty_file(path)?;
    sync_parent_dir(path)
}

/// Inject one post-rename parent-sync failure through the filesystem boundary.
#[cfg(test)]
pub(crate) fn fail_next_parent_sync_for_test() {
    durable_write::fail_next_parent_sync_for_test();
}

/// Inject one cross-device (`EXDEV`) durable-rename failure through the filesystem boundary.
#[cfg(test)]
pub(in crate::services) fn fail_next_rename_cross_device_for_test() {
    durable_write::fail_next_rename_cross_device_for_test();
}

/// Sync the parent directory of a path after a namespace mutation.
///
/// # Errors
///
/// Returns an error when the parent directory cannot be opened or synced.
pub fn sync_parent_dir(path: &Path) -> std::io::Result<()> {
    durable_write::sync_parent_dir(path)
}

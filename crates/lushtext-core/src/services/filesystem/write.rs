// SPDX-License-Identifier: GPL-3.0-or-later

//! Durable write operations exposed through the filesystem boundary.
//!
//! This module preserves the existing crash-durable write contract while keeping
//! caller-facing entry points inside `services::filesystem`.

use std::io::Write;
use std::path::Path;

use crate::services::durable_write;

use super::{sys, types::WriteLabel};

pub use durable_write::{DurableWriteError, TargetWriteGuard, WriteTargetIdentity};

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

/// Sync the parent directory of a path after a namespace mutation.
///
/// # Errors
///
/// Returns an error when the parent directory cannot be opened or synced.
pub fn sync_parent_dir(path: &Path) -> std::io::Result<()> {
    durable_write::sync_parent_dir(path)
}

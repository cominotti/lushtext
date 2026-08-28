// SPDX-License-Identifier: GPL-3.0-or-later

//! Metadata and identity queries for filesystem callers.
//!
//! These helpers gather common facts in one place so callers do not need to mix
//! direct metadata reads, canonicalization, and mtime conversion by hand.

use std::path::{Path, PathBuf};

use super::sys;
use super::types::{FileFacts, FileKind, PathStatus};

/// Read coarse metadata facts for `path`.
///
/// # Errors
///
/// Returns an error when the target metadata cannot be read.
pub fn file_facts(path: &Path) -> std::io::Result<FileFacts> {
    let metadata = sys::metadata(path)?;
    Ok(FileFacts {
        path: path.to_path_buf(),
        canonical_path: sys::canonicalize(path).ok(),
        kind: kind_from_metadata(&metadata),
        byte_size: sys::descriptor_file_len(path).unwrap_or(metadata.len()),
        modified_at_secs: modified_at_secs(&metadata),
        modified_at_nanos: modified_at_nanos(&metadata),
        identity: sys::file_identity(&metadata),
    })
}

/// Read only existence and coarse kind for `path`.
///
/// Missing paths are reported as [`PathStatus::Missing`]; other metadata
/// failures are returned so callers can decide whether to surface them.
///
/// # Errors
///
/// Returns an error when the platform reports a metadata failure other than
/// the target being absent.
pub fn path_status(path: &Path) -> std::io::Result<PathStatus> {
    match sys::metadata(path) {
        Ok(metadata) => Ok(PathStatus::from(kind_from_metadata(&metadata))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(PathStatus::Missing),
        Err(error) => Err(error),
    }
}

/// Return whether `path` currently exists, treating metadata errors as absent.
///
/// Use [`path_status`] when the caller must distinguish missing paths from
/// permission, parent, or other platform errors.
#[must_use]
pub fn exists(path: &Path) -> bool {
    sys::path_exists(path)
}

/// Canonicalize a path through the filesystem boundary.
///
/// # Errors
///
/// Returns an error when the path cannot be resolved by the platform.
pub fn canonical_path(path: &Path) -> std::io::Result<PathBuf> {
    sys::canonicalize(path)
}

/// Return whether `path` itself is a symbolic link.
///
/// # Errors
///
/// Returns an error when the path cannot be inspected.
pub fn is_symlink(path: &Path) -> std::io::Result<bool> {
    sys::symlink_metadata(path).map(|metadata| metadata.file_type().is_symlink())
}

/// Return a file's Unix mode bits for tests and metadata-preservation checks.
///
/// # Errors
///
/// Returns an error when mode bits are unavailable or the path cannot be read.
pub fn mode(path: &Path) -> std::io::Result<u32> {
    sys::mode(path)
}

/// Return a file's Unix inode for tests that verify identity preservation.
///
/// # Errors
///
/// Returns an error when inode identity is unavailable or the path cannot be read.
pub fn inode(path: &Path) -> std::io::Result<u64> {
    sys::inode(path)
}

/// Identity of the entry itself, not of a symlink's target.
///
/// Use this when the object being identified is the directory entry the user pointed
/// at — a confirmed delete, for example. `inode` follows the link, so a dangling
/// symlink has no identity under it and any identity check would refuse forever.
///
/// # Errors
///
/// Returns the platform error when the entry cannot be inspected — including when it
/// does not exist — or `Unsupported` on non-Unix targets.
pub fn link_inode(path: &Path) -> std::io::Result<u64> {
    sys::link_inode(path)
}

pub(crate) fn modified_at_secs(metadata: &sys::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

pub(crate) fn modified_at_nanos(metadata: &sys::Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
}

pub(crate) fn kind_from_metadata(metadata: &sys::Metadata) -> FileKind {
    let file_type = metadata.file_type();
    if file_type.is_file() {
        FileKind::File
    } else if file_type.is_dir() {
        FileKind::Directory
    } else {
        FileKind::Other
    }
}

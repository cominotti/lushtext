// SPDX-License-Identifier: GPL-3.0-or-later

//! Metadata and identity queries for filesystem callers.
//!
//! These helpers gather common facts in one place so callers do not need to mix
//! direct metadata reads, canonicalization, and mtime conversion by hand.

use std::path::{Path, PathBuf};

use super::sys;
use super::types::{FileFacts, FileKind};

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
    })
}

/// Canonicalize a path through the filesystem boundary.
///
/// # Errors
///
/// Returns an error when the path cannot be resolved by the platform.
pub fn canonical_path(path: &Path) -> std::io::Result<PathBuf> {
    sys::canonicalize(path)
}

/// Read symlink-aware metadata without following links.
///
/// # Errors
///
/// Returns an error when the path cannot be inspected.
pub fn symlink_facts(path: &Path) -> std::io::Result<FileKind> {
    sys::symlink_metadata(path).map(|metadata| kind_from_metadata(&metadata))
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

pub(crate) fn modified_at_secs(metadata: &sys::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
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

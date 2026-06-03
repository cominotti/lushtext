// SPDX-License-Identifier: GPL-3.0-or-later

//! Sidecar-oriented filesystem helpers for notes, bookmarks, and history.
//!
//! Sidecar services have a common shape: create storage directories, list JSON
//! documents, move lineages after renames, and remove stale files. Naming that
//! shape here keeps those services from open-coding filesystem policy.

use std::path::{Path, PathBuf};

use super::{mutate, tree, write};
use crate::services::filesystem::types::DirectoryScanPolicy;

/// Ensure a sidecar directory exists with durable directory-entry sync.
///
/// # Errors
///
/// Returns an error when creation or sync fails.
pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    write::create_dir_all_durable(path)
}

/// List visible sidecar paths under a directory.
///
/// # Errors
///
/// Returns an error when the sidecar directory cannot be read.
pub fn list_paths(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    tree::scan_directory(path, DirectoryScanPolicy::visible_workspace())
        .map(|entries| entries.into_iter().map(|entry| entry.path).collect())
}

/// Move a sidecar path with durable parent-directory sync.
///
/// # Errors
///
/// Returns an error when rename or parent sync fails.
pub fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    mutate::rename_path(from, to)
}

/// Remove a sidecar file if it still exists.
///
/// # Errors
///
/// Returns an error when removal fails for a reason other than not found.
pub fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    mutate::remove_file_if_exists(path).map(|_| ())
}

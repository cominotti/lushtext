// SPDX-License-Identifier: GPL-3.0-or-later

//! Directory traversal helpers for workspace-like scans.
//!
//! This module exposes plain Rust entry values and hides backend traversal
//! details, including descriptor-oriented sync support in the private backend.

use std::path::Path;

use super::sys;
use super::types::{DirectoryEntryInfo, DirectoryScanPolicy};

/// Scan one directory according to a simple boundary policy.
///
/// # Errors
///
/// Returns an error when the directory cannot be read.
pub fn scan_directory(
    path: &Path,
    policy: DirectoryScanPolicy,
) -> std::io::Result<Vec<DirectoryEntryInfo>> {
    let mut entries = Vec::new();
    visit_directory(path, policy, |entry| {
        entries.push(DirectoryEntryInfo {
            path: entry.path,
            file_name: entry.file_name,
            kind: entry.kind,
        });
        true
    })?;
    entries.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(entries)
}

/// Visit directory entries according to a boundary policy.
///
/// Returning `false` from the visitor stops traversal early.
///
/// # Errors
///
/// Returns an error when the directory cannot be read.
pub fn visit_directory<F>(
    path: &Path,
    policy: DirectoryScanPolicy,
    mut visit: F,
) -> std::io::Result<()>
where
    F: FnMut(DirectoryEntryInfo) -> bool,
{
    let mut retained = 0usize;
    sys::visit_directory_entries(path, |entry| {
        if !policy.include_hidden && entry.file_name.as_encoded_bytes().first() == Some(&b'.') {
            return true;
        }
        if retained >= policy.max_entries {
            return false;
        }
        retained += 1;
        visit(DirectoryEntryInfo {
            path: entry.path,
            file_name: entry.file_name.to_string_lossy().into_owned(),
            kind: entry.kind,
        })
    })
}

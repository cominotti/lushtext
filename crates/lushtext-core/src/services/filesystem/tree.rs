// SPDX-License-Identifier: GPL-3.0-or-later

//! Directory traversal helpers for workspace-like scans.
//!
//! This module exposes plain Rust entry values and hides backend traversal
//! details, including descriptor-oriented sync support in the private backend.

use std::collections::BTreeMap;
use std::path::Path;

use super::sys;
use super::types::{DirectoryEntryInfo, DirectoryPage, DirectoryScanPolicy};

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

/// Select the next bounded lexicographic page after an optional filename.
///
/// Traversal may visit the complete directory, but retained memory never exceeds
/// `policy.max_entries`; this makes the string cursor portable across restarts.
///
/// # Errors
///
/// Returns an error when the directory cannot be read.
pub fn scan_directory_page_after(
    path: &Path,
    after_file_name: Option<&str>,
    policy: DirectoryScanPolicy,
) -> std::io::Result<DirectoryPage> {
    let mut retained = BTreeMap::<String, DirectoryEntryInfo>::new();
    let mut matching_entries = 0usize;
    sys::visit_directory_entries(path, |entry| {
        if !policy.include_hidden && entry.file_name.as_encoded_bytes().first() == Some(&b'.') {
            return true;
        }
        let file_name = entry.file_name.to_string_lossy().into_owned();
        if after_file_name.is_some_and(|cursor| file_name.as_str() <= cursor) {
            return true;
        }
        matching_entries = matching_entries.saturating_add(1);
        if policy.max_entries == 0 {
            return true;
        }
        retained.insert(
            file_name.clone(),
            DirectoryEntryInfo {
                path: entry.path,
                file_name,
                kind: entry.kind,
            },
        );
        if retained.len() > policy.max_entries {
            retained.pop_last();
        }
        true
    })?;
    let entries = retained.into_values().collect::<Vec<_>>();
    Ok(DirectoryPage {
        has_more: matching_entries > entries.len(),
        entries,
        wrapped: false,
    })
}

/// Select a bounded page and optionally wrap once when the cursor suffix is empty.
///
/// # Errors
///
/// Returns an error when either directory traversal cannot be completed.
pub fn scan_directory_page(
    path: &Path,
    after_file_name: Option<&str>,
    wrap_if_exhausted: bool,
    policy: DirectoryScanPolicy,
) -> std::io::Result<DirectoryPage> {
    let mut page = scan_directory_page_after(path, after_file_name, policy)?;
    if wrap_if_exhausted && after_file_name.is_some() && page.entries.is_empty() {
        page = scan_directory_page_after(path, None, policy)?;
        page.wrapped = true;
    }
    Ok(page)
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

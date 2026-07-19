// SPDX-License-Identifier: GPL-3.0-or-later

//! Directory traversal helpers for workspace-like scans.
//!
//! This module exposes plain Rust entry values and hides backend traversal
//! details, including descriptor-oriented sync support in the private backend.

use std::collections::BTreeMap;
use std::path::Path;

use super::sys;
use super::types::{
    DirectoryEntryInfo, DirectoryPage, DirectoryPageVisitMetrics, DirectoryScanPolicy,
};

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
    scan_directory_page_after_with_cancel(path, after_file_name, policy, || false)
}

/// Select the next bounded lexicographic page with cooperative cancellation.
///
/// Cancellation is checked while the backend visits entries, so a superseded
/// repair or scan does not need to finish a large raw directory enumeration.
///
/// # Errors
///
/// Returns [`std::io::ErrorKind::Interrupted`] when `cancelled` requests a stop,
/// or the backend traversal error when the directory cannot be read.
pub fn scan_directory_page_after_with_cancel<F>(
    path: &Path,
    after_file_name: Option<&str>,
    policy: DirectoryScanPolicy,
    mut cancelled: F,
) -> std::io::Result<DirectoryPage>
where
    F: FnMut() -> bool,
{
    let mut retained = BTreeMap::<String, DirectoryEntryInfo>::new();
    let mut matching_entries = 0usize;
    let mut interrupted = false;
    sys::visit_directory_entries(path, |entry| {
        if cancelled() {
            interrupted = true;
            return false;
        }
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
    if interrupted {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "directory page scan cancelled",
        ));
    }
    let entries = retained.into_values().collect::<Vec<_>>();
    Ok(DirectoryPage {
        has_more: matching_entries > entries.len(),
        entries,
        wrapped: false,
    })
}

/// Visit bounded pages during one raw directory traversal.
///
/// Both filtered entries and raw backend visits are capped. The raw ceiling is
/// `policy.max_entries + 1`: the extra visit distinguishes an exactly-full
/// terminal directory from an oversized one without rescanning it. Page
/// callbacks run synchronously and retain at most `page_entries` rows.
///
/// # Errors
///
/// Returns [`std::io::ErrorKind::Interrupted`] when `cancelled` requests a stop,
/// or the backend traversal error when the directory cannot be read.
pub fn visit_directory_pages_with_cancel<F, V>(
    path: &Path,
    policy: DirectoryScanPolicy,
    page_entries: usize,
    mut cancelled: F,
    mut visit_page: V,
) -> std::io::Result<DirectoryPageVisitMetrics>
where
    F: FnMut() -> bool,
    V: FnMut(&[DirectoryEntryInfo]) -> bool,
{
    let page_entries = page_entries.max(1);
    let raw_limit = policy.max_entries.saturating_add(1);
    let mut page = Vec::with_capacity(page_entries.min(policy.max_entries));
    let mut metrics = DirectoryPageVisitMetrics::default();
    let mut interrupted = false;

    sys::visit_directory_entries(path, |entry| {
        if cancelled() {
            interrupted = true;
            return false;
        }
        metrics.raw_entries_visited = metrics.raw_entries_visited.saturating_add(1);
        if metrics.raw_entries_visited >= raw_limit {
            metrics.stopped_by_limit = true;
            return false;
        }
        if !policy.include_hidden && entry.file_name.as_encoded_bytes().first() == Some(&b'.') {
            return true;
        }
        if metrics.entries_delivered >= policy.max_entries {
            metrics.stopped_by_limit = true;
            return false;
        }

        page.push(DirectoryEntryInfo {
            path: entry.path,
            file_name: entry.file_name.to_string_lossy().into_owned(),
            kind: entry.kind,
        });
        metrics.entries_delivered = metrics.entries_delivered.saturating_add(1);
        if page.len() < page_entries {
            return true;
        }

        metrics.pages_delivered = metrics.pages_delivered.saturating_add(1);
        let keep_visiting = visit_page(&page);
        page.clear();
        if !keep_visiting {
            metrics.stopped_by_visitor = true;
            return false;
        }
        true
    })?;

    if interrupted {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "directory page visit cancelled",
        ));
    }
    if !page.is_empty() && !metrics.stopped_by_visitor {
        metrics.pages_delivered = metrics.pages_delivered.saturating_add(1);
        if !visit_page(&page) {
            metrics.stopped_by_visitor = true;
            return Ok(metrics);
        }
    }
    if !metrics.stopped_by_limit && !metrics.stopped_by_visitor {
        metrics.reached_terminal = true;
    }
    Ok(metrics)
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! Test and benchmark fixture helpers for filesystem setup and assertions.
//!
//! Tests should use this module instead of direct `std::fs` calls so fixture
//! code remains readable without teaching future production code to bypass the
//! filesystem boundary.

use std::path::Path;

use super::{DirectoryScanPolicy, metadata, mutate, read, sys, tree, write};

/// Write UTF-8 fixture text to a path.
///
/// # Panics
///
/// Panics when the fixture cannot be written.
pub fn write_text(path: &Path, contents: &str) {
    sys::write(path, contents.as_bytes()).expect("write fixture text");
}

/// Write fixture bytes to a path.
///
/// # Panics
///
/// Panics when the fixture cannot be written.
pub fn write_bytes(path: &Path, contents: impl AsRef<[u8]>) {
    sys::write(path, contents).expect("write fixture bytes");
}

/// Write a fixture file by repeating `pattern` until `size` bytes are present.
///
/// # Panics
///
/// Panics when `size` cannot fit in memory for the current platform or when
/// the fixture cannot be written.
pub fn write_repeated_bytes(path: &Path, pattern: &[u8], size: u64) {
    assert!(!pattern.is_empty(), "fixture pattern must not be empty");
    let size = usize::try_from(size).expect("fixture size fits usize");
    let mut contents = Vec::with_capacity(size);
    while contents.len() < size {
        let remaining = size - contents.len();
        let take = remaining.min(pattern.len());
        contents.extend_from_slice(&pattern[..take]);
    }
    write_bytes(path, contents);
}

/// Read UTF-8 fixture text from a path.
///
/// # Panics
///
/// Panics when the fixture cannot be read.
#[must_use]
pub fn read_text(path: &Path) -> String {
    read::text(path).expect("read fixture text")
}

/// Read fixture bytes from a path.
///
/// # Panics
///
/// Panics when the fixture cannot be read.
#[must_use]
pub fn read_bytes(path: &Path) -> Vec<u8> {
    read::bytes(path).expect("read fixture bytes")
}

/// Assert that a fixture file has exactly the expected text.
///
/// # Panics
///
/// Panics when the file cannot be read or the text differs.
pub fn assert_text(path: &Path, expected: &str) {
    assert_eq!(read_text(path), expected);
}

/// Create one fixture directory.
///
/// # Panics
///
/// Panics when the directory cannot be created.
pub fn create_dir(path: &Path) {
    mutate::create_dir(path).expect("create fixture directory");
}

/// Create a fixture directory tree.
///
/// # Panics
///
/// Panics when the directory tree cannot be created.
pub fn create_dir_all(path: &Path) {
    mutate::create_dir_all(path).expect("create fixture directory tree");
}

/// Remove a fixture file if present.
///
/// # Panics
///
/// Panics when removal fails for a reason other than not found.
pub fn remove_file(path: &Path) {
    mutate::remove_file_if_exists(path).expect("remove fixture file");
}

/// Remove a fixture directory tree if present.
///
/// # Panics
///
/// Panics when removal fails for a reason other than not found.
pub fn remove_dir_all(path: &Path) {
    mutate::remove_dir_all_if_exists(path).expect("remove fixture directory tree");
}

/// Rename a fixture path through the same durable namespace helper callers use.
///
/// # Panics
///
/// Panics when the path cannot be renamed.
pub fn rename(from: &Path, to: &Path) {
    write::rename_durable(from, to).expect("rename fixture path");
}

/// Return whether a fixture path exists.
#[must_use]
pub fn exists(path: &Path) -> bool {
    sys::path_exists(path)
}

/// Return all fixture entry names, including hidden temp leftovers.
///
/// # Panics
///
/// Panics when the directory cannot be scanned.
#[must_use]
pub fn entry_names(path: &Path) -> Vec<String> {
    tree::scan_directory(
        path,
        DirectoryScanPolicy {
            max_entries: usize::MAX,
            include_hidden: true,
        },
    )
    .expect("scan fixture directory")
    .into_iter()
    .map(|entry| entry.file_name)
    .collect()
}

/// Create a Unix symlink fixture.
///
/// # Panics
///
/// Panics when the symlink cannot be created.
pub fn symlink(target: &Path, link: &Path) {
    sys::symlink(target, link).expect("create fixture symlink");
}

/// Return whether a fixture path is a symbolic link.
///
/// # Panics
///
/// Panics when the path cannot be inspected.
#[must_use]
pub fn is_symlink(path: &Path) -> bool {
    metadata::is_symlink(path).expect("inspect fixture symlink")
}

/// Set Unix mode bits for a fixture path.
///
/// # Panics
///
/// Panics when permissions cannot be changed.
pub fn set_mode(path: &Path, mode: u32) {
    sys::set_permissions_mode(path, mode).expect("set fixture mode");
}

/// Read Unix mode bits for a fixture path.
///
/// # Panics
///
/// Panics when permissions cannot be inspected.
#[must_use]
pub fn mode(path: &Path) -> u32 {
    metadata::mode(path).expect("read fixture mode")
}

/// Create a sparse fixture file of `len` bytes.
///
/// # Panics
///
/// Panics when the sparse fixture cannot be created.
pub fn create_sparse_file(path: &Path, len: u64) {
    sys::create_sparse_file(path, len).expect("create sparse fixture file");
}

/// Overwrite bytes at the start of an existing fixture file.
///
/// # Panics
///
/// Panics when the fixture cannot be opened or written.
pub fn write_at_start(path: &Path, bytes: &[u8]) {
    sys::write_at_start(path, bytes).expect("write fixture prefix");
}

/// Try to set an extended attribute for tests that skip unsupported filesystems.
///
/// # Errors
///
/// Returns an error when the filesystem does not support extended attributes or
/// the attribute cannot be written, so callers can skip on unsupported setups.
pub fn set_xattr(path: &Path, name: &str, value: &[u8]) -> std::io::Result<()> {
    sys::set_xattr(path, name, value)
}

/// Try to read an extended attribute for tests that skip unsupported filesystems.
///
/// # Errors
///
/// Returns an error when the attribute is absent or the filesystem does not
/// support extended attributes, so callers can skip on unsupported setups.
pub fn get_xattr(path: &Path, name: &str) -> std::io::Result<Vec<u8>> {
    sys::get_xattr(path, name)
}

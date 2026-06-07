// SPDX-License-Identifier: GPL-3.0-or-later

//! File read helpers for callers that need bytes, text, or bounded snapshots.
//!
//! The boundary keeps blocking I/O explicit. UI callers should still run these
//! functions on background workers such as `spawn_blocking_then`.

use std::path::Path;

use super::metadata::file_facts;
use super::sys;
use super::types::FileSnapshot;

/// Read all bytes from a file.
///
/// # Errors
///
/// Returns an error when the file cannot be read.
pub fn bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    sys::read(path)
}

/// Read all text from a file using UTF-8.
///
/// # Errors
///
/// Returns an error when the file cannot be read or decoded as UTF-8.
pub fn text(path: &Path) -> std::io::Result<String> {
    sys::read_to_string(path)
}

/// Read file bytes together with the metadata facts commonly needed by editors.
///
/// # Errors
///
/// Returns an error when metadata or content cannot be read.
pub fn snapshot(path: &Path) -> std::io::Result<FileSnapshot> {
    let facts = file_facts(path)?;
    let bytes = bytes(path)?;
    Ok(FileSnapshot { facts, bytes })
}

/// Read at most `byte_limit` bytes from the start of a file.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or read.
pub fn prefix_bytes(path: &Path, byte_limit: usize) -> std::io::Result<Vec<u8>> {
    sys::read_prefix(path, byte_limit)
}

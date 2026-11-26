// SPDX-License-Identifier: GPL-3.0-or-later

//! File read helpers for callers that need bytes, text, or bounded snapshots.
//!
//! The boundary keeps blocking I/O explicit. UI callers should still run these
//! functions on background workers such as `spawn_blocking_then`.

use std::path::Path;

use super::metadata::file_facts;
use super::sys;
use super::types::FileSnapshot;

/// Failures from a streaming file read with an enforced allocation ceiling.
#[derive(Debug, thiserror::Error)]
pub enum BoundedFileReadError {
    /// The caller cancelled between bounded read chunks.
    #[error("bounded file read cancelled")]
    Cancelled,
    /// The file contained at least one byte beyond the accepted limit.
    #[error("file exceeds the supported {byte_limit}-byte limit")]
    LimitExceeded { byte_limit: u64 },
    /// Opening or reading the file failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Failures from [`stream_bounded`].
#[derive(Debug, thiserror::Error)]
pub enum BoundedStreamError<E> {
    /// The caller cancelled between chunks.
    #[error("bounded file stream cancelled")]
    Cancelled,
    /// The file holds at least one byte past the accepted limit.
    #[error("file exceeds the supported {byte_limit}-byte limit")]
    LimitExceeded { byte_limit: u64 },
    /// Opening or reading the file failed.
    #[error(transparent)]
    Io(std::io::Error),
    /// The consumer refused a chunk.
    #[error("bounded file stream consumer stopped")]
    Consumer(E),
}

/// Stream a file to `consume` in chunks, never past `byte_limit`.
///
/// Memory stays one chunk of `chunk_bytes` whatever the file's size. A file
/// that grows past the limit while it is read fails with `LimitExceeded`
/// after at most the limit was streamed. Returns the bytes streamed.
///
/// # Errors
///
/// Returns a typed cancellation, oversize, I/O or consumer failure.
pub fn stream_bounded<F, C, E>(
    path: &Path,
    byte_limit: u64,
    chunk_bytes: usize,
    cancelled: F,
    consume: C,
) -> Result<u64, BoundedStreamError<E>>
where
    F: FnMut() -> bool,
    C: FnMut(&[u8]) -> Result<(), E>,
{
    sys::stream_bounded(path, byte_limit, chunk_bytes, cancelled, consume)
}

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

/// Read at most `byte_limit` prefix bytes with cooperative cancellation.
///
/// Unlike [`bounded_bytes`], reaching `byte_limit` is a successful truncated
/// prefix rather than an oversize failure, so preview flows can render a
/// bounded excerpt of a larger file. Cancellation is checked between fixed-size
/// read chunks.
///
/// # Errors
///
/// Returns a typed cancellation or I/O failure.
pub fn prefix_bytes_cancellable<F>(
    path: &Path,
    byte_limit: usize,
    cancelled: F,
) -> Result<Vec<u8>, BoundedFileReadError>
where
    F: FnMut() -> bool,
{
    sys::read_prefix_cancellable(path, byte_limit, cancelled)
}

/// Stream bytes with a hard limit plus one-byte oversize sentinel.
///
/// The returned allocation never grows beyond `byte_limit`; the backend reads
/// at most one additional byte into a fixed-size read chunk to distinguish an
/// exact-limit file from a file that grew after metadata planning.
///
/// # Errors
///
/// Returns a typed cancellation, oversize, or I/O failure.
pub fn bounded_bytes<F>(
    path: &Path,
    byte_limit: u64,
    capacity_hint: u64,
    cancelled: F,
) -> Result<Vec<u8>, BoundedFileReadError>
where
    F: FnMut() -> bool,
{
    sys::read_bounded(path, byte_limit, capacity_hint, cancelled)
}

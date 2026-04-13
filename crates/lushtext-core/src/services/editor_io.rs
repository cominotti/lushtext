// SPDX-License-Identifier: GPL-3.0-or-later

//! Blocking file I/O for editor load and save operations.
//!
//! All functions perform synchronous I/O and must be called from a background
//! thread via `spawn_blocking_then`. The load path uses SIMD-accelerated UTF-8
//! validation (simdutf8) to avoid the redundant scalar validation in
//! `read_to_string`.

use crate::services::file_limits::FileSizeCheck;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Successful result from `load_text_file`.
pub struct LoadResult {
    pub content: String,
    pub size: u64,
    pub size_check: FileSizeCheck,
    /// File mtime (epoch seconds), extracted from the metadata already
    /// read for size classification — no extra stat() needed by callers.
    pub mtime: Option<u64>,
}

/// Errors that can occur when loading a file for editing.
/// Each variant carries context (path, size) for user-facing error messages.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("load cancelled")]
    Cancelled,
    #[error("Cannot stat {path}: {source}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid UTF-8")]
    InvalidUtf8 { path: PathBuf },
    #[error("{path} is too large to edit ({size_mb} MB). Consider a pager like `less`.")]
    TooLarge { path: PathBuf, size_mb: u64 },
}

/// Errors that can occur when saving a file.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("No file path set")]
    NoPath,
    #[error("Failed to write {path}: {source}")]
    WriteTemp {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to finalize {to} from {from}: {source}")]
    Finalize {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Read a file from disk, validate UTF-8, and classify its size for feature gating.
///
/// Uses SIMD-accelerated UTF-8 validation (simdutf8) instead of `read_to_string`
/// to avoid redundant scalar validation. Checks cancellation before metadata read,
/// before file read, and after file read for responsive tab close.
///
/// **Threading:** Performs blocking I/O — call from a background thread.
pub fn load_text_file(path: &Path, cancel: &AtomicBool) -> Result<LoadResult, LoadError> {
    if cancel.load(Ordering::Acquire) {
        return Err(LoadError::Cancelled);
    }

    let meta = std::fs::metadata(path).map_err(|source| LoadError::Metadata {
        path: path.to_path_buf(),
        source,
    })?;
    let size = meta.len();
    let check = FileSizeCheck::classify(size);
    let mtime = mtime_from_metadata(&meta);

    if check == FileSizeCheck::TooLarge {
        return Err(LoadError::TooLarge {
            path: path.to_path_buf(),
            size_mb: size / 1_000_000,
        });
    }

    if cancel.load(Ordering::Acquire) {
        return Err(LoadError::Cancelled);
    }

    // Read raw bytes and validate UTF-8 via SIMD (~8x faster than
    // the scalar validation inside read_to_string at any file size).
    let bytes = std::fs::read(path).map_err(|source| LoadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if cancel.load(Ordering::Acquire) {
        return Err(LoadError::Cancelled);
    }

    let content = match simdutf8::basic::from_utf8(&bytes) {
        // SAFETY: simdutf8 confirmed valid UTF-8.
        Ok(_) => unsafe { String::from_utf8_unchecked(bytes) },
        Err(_) => {
            return Err(LoadError::InvalidUtf8 {
                path: path.to_path_buf(),
            });
        }
    };

    Ok(LoadResult {
        content,
        size,
        size_check: check,
        mtime,
    })
}

/// Atomically write text to a file using temp-file-then-rename.
///
/// Creates a `.filename.tmp` sibling, writes content, then renames over the
/// target. `rename(2)` is atomic on POSIX, so readers see either the old or
/// new file, never partial. On rename failure, the temp file is cleaned up.
///
/// **Threading:** Performs blocking I/O — call from a background thread.
/// Returns `(bytes_written, mtime)`. The mtime is read from the freshly
/// written file so callers can update their baseline without a main-thread stat().
pub fn write_snapshot_to_path(path: &Path, text: &str) -> Result<(u64, Option<u64>), SaveError> {
    let tmp_name = format!(
        ".{}.tmp",
        path.file_name().map_or_else(
            || "untitled".to_string(),
            |n| n.to_string_lossy().into_owned()
        )
    );
    let tmp_path = path.with_file_name(&tmp_name);
    let file = std::fs::File::create(&tmp_path).map_err(|source| SaveError::WriteTemp {
        path: tmp_path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(text.as_bytes())
        .map_err(|source| SaveError::WriteTemp {
            path: tmp_path.clone(),
            source,
        })?;
    writer.flush().map_err(|source| SaveError::WriteTemp {
        path: tmp_path.clone(),
        source,
    })?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|source| SaveError::WriteTemp {
            path: tmp_path.clone(),
            source,
        })?;
    std::fs::rename(&tmp_path, path).map_err(|source| {
        let _ = std::fs::remove_file(&tmp_path);
        SaveError::Finalize {
            from: tmp_path.clone(),
            to: path.to_path_buf(),
            source,
        }
    })?;
    let mtime = std::fs::metadata(path)
        .ok()
        .and_then(|m| mtime_from_metadata(&m));
    Ok((text.len() as u64, mtime))
}

/// Extract mtime as epoch seconds from already-fetched metadata.
fn mtime_from_metadata(meta: &std::fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Read a file's mtime as seconds since the UNIX epoch.
/// Returns `None` if the file doesn't exist or metadata can't be read.
///
/// **Threading:** Performs a blocking stat syscall.
#[must_use]
pub fn mtime_secs(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| mtime_from_metadata(&m))
}

/// Current wall-clock time as seconds since the UNIX epoch.
#[must_use]
pub fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use tempfile::NamedTempFile;

    #[test]
    fn load_text_file_reads_utf8_and_classifies_size() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "hello").unwrap();

        let cancel = AtomicBool::new(false);
        let result = load_text_file(file.path(), &cancel).unwrap();

        assert_eq!(result.content, "hello");
        assert_eq!(result.size, 5);
        assert_eq!(result.size_check, FileSizeCheck::Normal);
        assert!(
            result.mtime.is_some(),
            "mtime should be populated from metadata"
        );
    }

    #[test]
    fn load_text_file_honors_cancellation() {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "hello").unwrap();

        let cancel = AtomicBool::new(true);
        let result = load_text_file(file.path(), &cancel);

        assert!(matches!(result, Err(LoadError::Cancelled)));
    }

    #[test]
    fn write_snapshot_to_path_replaces_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");

        let (size, mtime) = write_snapshot_to_path(&path, "saved").unwrap();

        assert_eq!(size, 5);
        assert!(mtime.is_some(), "mtime should be populated after write");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "saved");
    }
}

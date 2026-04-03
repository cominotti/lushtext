// SPDX-License-Identifier: GPL-3.0-or-later

//! Blocking file I/O for editor load and save operations.
//!
//! All functions perform synchronous I/O and must be called from a background
//! thread via `spawn_blocking_then`. The load path uses SIMD-accelerated UTF-8
//! validation (simdutf8) to avoid the redundant scalar validation in
//! `read_to_string`.

use crate::services::file_limits::FileSizeCheck;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

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
pub fn load_text_file(
    path: &Path,
    cancel: &AtomicBool,
) -> Result<(String, u64, FileSizeCheck), LoadError> {
    if cancel.load(Ordering::Acquire) {
        return Err(LoadError::Cancelled);
    }

    let meta = std::fs::metadata(path).map_err(|source| LoadError::Metadata {
        path: path.to_path_buf(),
        source,
    })?;
    let size = meta.len();
    let check = FileSizeCheck::classify(size);

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

    Ok((content, size, check))
}

/// Atomically write text to a file using temp-file-then-rename.
///
/// Creates a `.filename.tmp` sibling, writes content, then renames over the
/// target. `rename(2)` is atomic on POSIX, so readers see either the old or
/// new file, never partial. On rename failure, the temp file is cleaned up.
///
/// **Threading:** Performs blocking I/O — call from a background thread.
pub fn write_snapshot_to_path(path: PathBuf, text: String) -> Result<u64, SaveError> {
    let tmp_name = format!(
        ".{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".to_string())
    );
    let tmp_path = path.with_file_name(&tmp_name);
    std::fs::write(&tmp_path, &text).map_err(|source| SaveError::WriteTemp {
        path: tmp_path.clone(),
        source,
    })?;
    std::fs::rename(&tmp_path, &path).map_err(|source| {
        let _ = std::fs::remove_file(&tmp_path);
        SaveError::Finalize {
            from: tmp_path.clone(),
            to: path.clone(),
            source,
        }
    })?;
    Ok(text.len() as u64)
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
        let (content, size, check) = load_text_file(file.path(), &cancel).unwrap();

        assert_eq!(content, "hello");
        assert_eq!(size, 5);
        assert_eq!(check, FileSizeCheck::Normal);
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

        let size = write_snapshot_to_path(path.clone(), "saved".to_string()).unwrap();

        assert_eq!(size, 5);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "saved");
    }
}

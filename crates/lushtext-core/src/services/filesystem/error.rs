// SPDX-License-Identifier: GPL-3.0-or-later

//! App-facing filesystem error wrapper for boundary operations.
//!
//! Most existing services still consume `std::io::Error`; this type gives new
//! boundary code a place to attach operation and path context without leaking
//! backend-specific `rustix` errno values into ordinary callers.

use std::fmt;
use std::path::{Path, PathBuf};

/// Filesystem error with operation and optional path context.
#[derive(Debug)]
pub struct FilesystemError {
    operation: &'static str,
    path: Option<PathBuf>,
    source: std::io::Error,
}

impl FilesystemError {
    /// Build an error for a filesystem operation that targeted one path.
    #[must_use]
    pub fn for_path(operation: &'static str, path: &Path, source: std::io::Error) -> Self {
        Self {
            operation,
            path: Some(path.to_path_buf()),
            source,
        }
    }

    /// Build an error for an operation whose path is implicit or unavailable.
    #[must_use]
    pub const fn new(operation: &'static str, source: std::io::Error) -> Self {
        Self {
            operation,
            path: None,
            source,
        }
    }

    /// Return the underlying I/O error.
    #[must_use]
    pub const fn source_io(&self) -> &std::io::Error {
        &self.source
    }

    /// Consume the wrapper and return the underlying I/O error.
    #[must_use]
    pub fn into_io_error(self) -> std::io::Error {
        self.source
    }
}

impl fmt::Display for FilesystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(path) => write!(
                f,
                "{} failed for {}: {}",
                self.operation,
                path.display(),
                self.source
            ),
            None => write!(f, "{} failed: {}", self.operation, self.source),
        }
    }
}

impl std::error::Error for FilesystemError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

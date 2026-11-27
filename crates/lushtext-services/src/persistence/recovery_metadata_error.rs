// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed errors for recovery-aware metadata persistence.

use std::io;
use std::path::PathBuf;

/// Typed errors returned when validating or durably writing recovery metadata.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryMetadataError {
    /// Target metadata path did not include a parent directory.
    #[error("recovery metadata path {} has no parent directory", path.display())]
    MissingParent {
        /// Path passed to [`crate::persistence::recovery_metadata::save_json_path`].
        path: PathBuf,
    },
    /// Parent directory for the recovery metadata file could not be created.
    #[error("failed to create {}: {source}", path.display())]
    CreateDir {
        /// Parent directory path that could not be created.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Serialization or durable replacement of the recovery metadata file failed.
    #[error("failed to write {}: {source}", path.display())]
    Write {
        /// Destination recovery metadata file path.
        path: PathBuf,
        /// Underlying I/O or serialization error.
        #[source]
        source: io::Error,
    },
    /// Preserving an existing damaged or unsupported file failed, so overwriting it was refused.
    #[error("{} is not safe to replace with v1 JSON: {detail}", path.display())]
    UnsafeReplacement {
        /// Destination recovery metadata file path.
        path: PathBuf,
        /// Summary of the preservation diagnostics preventing replacement.
        detail: String,
    },
}

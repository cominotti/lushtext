// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed errors for the local-history persistence service.

use crate::filesystem::read::BoundedFileReadError;
use crate::filesystem::{
    DirectoryEntryInfo, DirectoryScanPolicy, metadata as fs_metadata, tree as fs_tree,
};
use crate::notes::note_storage::NoteStorageError;
use std::io;
use std::path::{Path, PathBuf};

/// Typed errors returned by local-history snapshot, index, and lineage operations.
#[derive(Debug, thiserror::Error)]
pub enum LocalHistoryError {
    /// Process-local mutex serializing read-modify-write local-history passes was poisoned.
    #[error("local-history lock poisoned")]
    LockPoisoned,
    /// Resolving canonical sidecar identity for a saved document path failed.
    #[error("failed to resolve local-history identity for {}: {source}", path.display())]
    ResolveIdentity {
        /// Saved document path whose identity could not be resolved.
        path: PathBuf,
        /// Underlying sidecar identity resolution error.
        #[source]
        source: Box<NoteStorageError>,
    },
    /// Inspecting or reading directory or file metadata failed.
    #[error("failed to inspect {}: {source}", path.display())]
    Inspect {
        /// Filesystem path being inspected.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Scanning a local-history directory failed.
    #[error("failed to read {}: {source}", path.display())]
    ScanDir {
        /// Directory path that could not be scanned.
        path: PathBuf,
        /// Underlying directory scan error.
        #[source]
        source: io::Error,
    },
    /// Creating a local-history directory durably failed.
    #[error("failed to create {}: {source}", path.display())]
    CreateDir {
        /// Directory path that could not be created.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Writing a snapshot body via `editor_io` failed.
    #[error("failed to write snapshot body {}: {source}", path.display())]
    WriteSnapshotBody {
        /// Snapshot text path that could not be written.
        path: PathBuf,
        /// Underlying editor save error.
        #[source]
        source: Box<crate::editor_io::EditorSaveError>,
    },
    /// Local-history index path could not be mapped back to an app data directory.
    #[error("local-history index path {} is not under app data directory", path.display())]
    IndexOutsideAppData {
        /// Local-history directory or index path outside the app data root.
        path: PathBuf,
    },
    /// Persisting a local-history JSON index failed.
    #[error("failed to save local-history index {}: {source}", path.display())]
    SaveIndex {
        /// Index file path that could not be saved.
        path: PathBuf,
        /// Underlying sidecar persistence error.
        #[source]
        source: Box<NoteStorageError>,
    },
    /// Moving a local-history lineage directory failed.
    #[error("failed to move {} to {}: {source}", from.display(), to.display())]
    MoveLineage {
        /// Source lineage directory.
        from: PathBuf,
        /// Destination lineage directory.
        to: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Moving an individual snapshot body between lineages failed.
    #[error("failed to move snapshot {} to {}: {source}", from.display(), to.display())]
    MoveSnapshot {
        /// Source snapshot path.
        from: PathBuf,
        /// Destination snapshot path.
        to: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Snapshot path exists but is not a regular file.
    #[error("local-history snapshot {} is not a file", path.display())]
    SnapshotNotAFile {
        /// Snapshot path inspected.
        path: PathBuf,
    },
    /// Snapshot exceeds the local-history browsing size limit.
    #[error("local-history snapshot {} is larger than the {limit} browse limit", path.display())]
    SnapshotTooLarge {
        /// Oversized snapshot path.
        path: PathBuf,
        /// Human-readable size limit string.
        limit: String,
    },
    /// Snapshot body read failed.
    #[error("failed to read {}: {source}", path.display())]
    ReadSnapshot {
        /// Snapshot path whose bounded read failed.
        path: PathBuf,
        /// Underlying bounded read error.
        #[source]
        source: BoundedFileReadError,
    },
    /// Fresh non-cancellable read reported cancellation unexpectedly.
    #[error("fresh local-history read was unexpectedly cancelled")]
    UnexpectedCancellation,
    /// Snapshot bytes were not valid UTF-8.
    #[error("{} is not valid UTF-8: {source}", path.display())]
    InvalidUtf8 {
        /// Snapshot path with invalid UTF-8 bytes.
        path: PathBuf,
        /// UTF-8 validation error.
        #[source]
        source: simdutf8::basic::Utf8Error,
    },
    /// Removing an obsolete or empty local-history lineage directory failed.
    #[error("failed to remove obsolete local-history lineage {}: {source}", path.display())]
    ObsoleteLineageCleanup {
        /// Obsolete lineage directory path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
}

impl LocalHistoryError {
    pub(super) fn inspect(path: &Path, source: io::Error) -> Self {
        Self::Inspect {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(super) fn scan_dir(path: &Path, source: io::Error) -> Self {
        Self::ScanDir {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(super) fn create_dir(path: &Path, source: io::Error) -> Self {
        Self::CreateDir {
            path: path.to_path_buf(),
            source,
        }
    }
}

pub(super) fn path_is_present(path: &Path) -> Result<bool, LocalHistoryError> {
    Ok(fs_metadata::path_status(path)
        .map_err(|e| LocalHistoryError::inspect(path, e))?
        .is_present())
}

pub(super) fn scan_dir_entries(
    path: &Path,
    policy: DirectoryScanPolicy,
) -> Result<Vec<DirectoryEntryInfo>, LocalHistoryError> {
    fs_tree::scan_directory(path, policy).map_err(|e| LocalHistoryError::scan_dir(path, e))
}

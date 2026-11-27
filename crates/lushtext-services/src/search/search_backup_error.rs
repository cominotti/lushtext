// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed errors for persisted Replace All undo journals.

use std::io;
use std::path::PathBuf;

use crate::persistence::recovery_metadata::RecoveryMetadataError;

/// Typed errors returned when inspecting, updating, or cleaning Replace All undo journals.
#[derive(Debug, thiserror::Error)]
pub enum SearchBackupError {
    /// Process-wide journal lock was poisoned by a panicked holder.
    #[error("replace undo backup disk lock poisoned")]
    LockPoisoned,

    /// Journal directory or bodies directory could not be created durably.
    #[error("failed to create {}: {source}", path.display())]
    CreateDir {
        /// Directory path that could not be created.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Cleanup marker could not be removed before activating the journal.
    #[error("failed to clear replace journal cleanup marker in {}: {source}", dir.display())]
    ClearCleanupMarker {
        /// Journal directory holding the cleanup marker.
        dir: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Shrinking was refused because the on-disk journal no longer holds every retained entry.
    #[error("replace undo journal no longer holds every retained entry; it is not rebuilt")]
    MissingRetainedEntry,

    /// Journal path had no expected parent directory.
    #[error("replace undo journal has no parent directory")]
    MissingParent {
        /// Journal path whose parent was expected.
        path: PathBuf,
    },

    /// Entry file path had no journal directory parent.
    #[error("replace undo entry has no journal directory")]
    EntryMissingJournalDir {
        /// Entry path passed to `load_legacy_original`.
        entry_file: PathBuf,
    },

    /// Re-arming was refused because the journal has no active manifest.
    #[error("replace undo journal has no active manifest")]
    MissingActiveManifest {
        /// Journal directory missing an active manifest.
        journal_dir: PathBuf,
    },

    /// Per-file journal entry could not be deleted.
    #[error("failed to delete replace journal entry {}: {source}", path.display())]
    DeleteEntry {
        /// Journal entry path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Per-file undo body could not be deleted.
    #[error("failed to delete replace undo body {}: {source}", path.display())]
    DeleteBody {
        /// Undo body path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Legacy single-file backup could not be deleted.
    #[error("failed to delete replace backup {}: {source}", path.display())]
    DeleteLegacyBackup {
        /// Legacy backup file path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Journal directory tree could not be deleted.
    #[error("failed to delete replace journal {}: {source}", path.display())]
    DeleteJournalDir {
        /// Journal directory path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Legacy entry payload exceeded the configured undo payload cap.
    #[error("legacy replace undo entry exceeds the undo payload limit")]
    LegacyEntryExceedsPayloadLimit {
        /// Legacy entry file path.
        entry_file: PathBuf,
        /// Configured payload byte limit.
        limit: u64,
    },

    /// Entry file was a streamed entry when a legacy inline entry was requested.
    #[error("replace undo entry is not a legacy entry")]
    NotLegacyEntry {
        /// Entry file path.
        entry_file: PathBuf,
    },

    /// Legacy entry file could not be loaded or parsed.
    #[error("legacy replace undo entry could not be loaded")]
    LegacyEntryUnreadable {
        /// Entry file path.
        entry_file: PathBuf,
    },

    /// Journal manifest exceeded the supported entry cap.
    #[error("replace undo journal has {count} entries, above the {cap} entry cap")]
    EntryCapExceeded {
        /// Number of entries requested.
        count: usize,
        /// Maximum supported entry count.
        cap: usize,
    },

    /// Metadata status check failed before marking cleanup in progress.
    #[error("failed to inspect {}: {source}", path.display())]
    ProbeStatus {
        /// Inspected path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Deleting a retired backup was blocked because recovery diagnostics disallowed replacement.
    #[error("retired replace backup is not safe to delete after recovery diagnostics")]
    RetiredBackupUnsafeToDelete,

    /// Writing the cleanup-in-progress marker failed.
    #[error("failed to mark replace journal cleanup in {}: {source}", dir.display())]
    MarkCleanupInProgress {
        /// Journal directory where the cleanup marker is written.
        dir: PathBuf,
        /// Underlying recovery metadata error.
        #[source]
        source: RecoveryMetadataError,
    },

    /// Reading or writing enveloped recovery metadata failed.
    #[error(transparent)]
    Metadata(#[from] RecoveryMetadataError),

    /// Writing a test-fixture undo body failed.
    #[cfg(any(test, feature = "test-utils"))]
    #[error("failed to write replace undo body {}: {source}", path.display())]
    WriteFixtureBody {
        /// Undo body path.
        path: PathBuf,
        /// Underlying durable write error.
        #[source]
        source: crate::filesystem::write::DurableWriteError,
    },

    /// Test-fixture legacy journal bytes were not valid UTF-8.
    #[cfg(any(test, feature = "test-utils"))]
    #[error("legacy journal fixture content is not valid UTF-8: {0}")]
    InvalidFixtureUtf8(#[from] std::string::FromUtf8Error),
}

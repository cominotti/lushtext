// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed evidence and outcomes for conservative draft-orphan cleanup.
//!
//! These plain Rust values separate side-effect-free inspection from durable
//! execution while keeping GTK and filesystem mechanics in the parent service.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::model::draft::{DraftEntry, DraftManifest};
use crate::services::filesystem::PathStatus;

/// Exact manifest-entry generation captured during orphan inspection.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DraftEntryFingerprint {
    /// Stable draft identifier shared by the manifest entry and body filename.
    pub draft_id: String,
    /// Backing document path captured with the inspected manifest entry.
    pub original_path: Option<PathBuf>,
    /// Backing document mtime captured by the inspected autosave generation.
    pub original_mtime_secs: Option<u64>,
    /// Persisted generation timestamp used to distinguish same-ID autosaves.
    pub saved_at_secs: u64,
}

impl DraftEntryFingerprint {
    /// Capture every persisted field that identifies one manifest generation.
    #[must_use]
    pub fn from_entry(entry: &DraftEntry) -> Self {
        Self {
            draft_id: entry.draft_id.clone(),
            original_path: entry.original_path.clone(),
            original_mtime_secs: entry.original_mtime_secs,
            saved_at_secs: entry.saved_at_secs,
        }
    }

    /// Return whether a manifest entry is exactly the generation inspected.
    #[must_use]
    pub fn matches(&self, entry: &DraftEntry) -> bool {
        self.draft_id == entry.draft_id
            && self.original_path == entry.original_path
            && self.original_mtime_secs == entry.original_mtime_secs
            && self.saved_at_secs == entry.saved_at_secs
    }
}

/// One draft body that inspection proved was absent from its manifest snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftOrphanBodyCandidate {
    /// Draft ID parsed from the regular `.draft` filename.
    pub draft_id: String,
    /// Exact body path returned by the bounded directory scan.
    pub path: PathBuf,
    /// Inode identity captured during inspection for stale-body revalidation.
    pub inode: u64,
}

/// Why cleanup deliberately preserved one artifact for a later trusted pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftOrphanCleanupRetentionReason {
    /// A non-manifest file did not use the supported draft-body name.
    UnknownFile,
    /// More than one manifest entry used the same stable draft ID.
    DuplicateManifestId,
    /// Metadata could not prove whether the artifact was present or absent.
    StatusUncertain,
    /// A `.draft` path was not a regular file and therefore was not safe to delete.
    BodyNotRegularFile,
    /// An executable candidate did not match the canonical body path for its draft ID.
    CandidatePathMismatch,
    /// The latest persisted manifest gained an entry for an inspected orphan body.
    ManifestEntryPresent,
    /// The inspected manifest generation was no longer present at execution time.
    FingerprintChanged,
    /// A body appeared after inspection marked its manifest entry as missing.
    BodyReappeared,
    /// The candidate path now names a newer body generation than inspection saw.
    BodyGenerationChanged,
    /// Filesystem deletion failed and the body remains retryable.
    DeleteFailed,
    /// Durable manifest persistence was not confirmed, so evidence remains retryable.
    ManifestCommitFailed,
}

/// Preserved cleanup evidence with a stable reason for diagnostics and tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftOrphanCleanupRetained {
    /// Draft ID when the artifact belongs to a recognized draft generation.
    pub draft_id: Option<String>,
    /// Filesystem path whose evidence was preserved.
    pub path: PathBuf,
    /// Conservative reason cleanup did not mutate this artifact.
    pub reason: DraftOrphanCleanupRetentionReason,
}

/// Metadata failure for one cleanup artifact.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("failed to inspect draft cleanup path {path}: {detail}")]
pub struct DraftOrphanCleanupStatusError {
    /// Path whose coarse presence or kind could not be read.
    pub path: PathBuf,
    /// Platform error detail retained for grouped recovery diagnostics.
    pub detail: String,
}

/// Failure to delete one body that was still a confirmed orphan at execution.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("failed to delete orphan draft body {path}: {detail}")]
pub struct DraftOrphanCleanupDeleteError {
    /// Draft ID parsed from the candidate filename.
    pub draft_id: String,
    /// Body path whose deletion failed.
    pub path: PathBuf,
    /// Platform error detail retained for grouped recovery diagnostics.
    pub detail: String,
}

/// Failure to load or durably commit the serialized draft manifest.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DraftOrphanCleanupManifestError {
    /// Latest persisted state could not be trusted before destructive work.
    #[error("failed to load trusted draft manifest {path}: {detail}")]
    Load {
        /// Draft manifest path used by the serialized update workflow.
        path: PathBuf,
        /// Recovery-load detail explaining why the value was not trusted.
        detail: String,
    },
    /// Confirmed removals could not be accepted as durably committed.
    #[error("failed to commit draft manifest cleanup {path}: {detail}")]
    Write {
        /// Draft manifest path used by the serialized update workflow.
        path: PathBuf,
        /// Durable-write detail retained for diagnostics.
        detail: String,
    },
}

/// Typed failure accumulated without hiding unaffected cleanup outcomes.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DraftOrphanCleanupFailure {
    /// Presence or kind could not be established safely.
    #[error(transparent)]
    Status(#[from] DraftOrphanCleanupStatusError),
    /// A confirmed orphan body could not be deleted.
    #[error(transparent)]
    Delete(#[from] DraftOrphanCleanupDeleteError),
    /// Latest manifest state could not be loaded or committed durably.
    #[error(transparent)]
    Manifest(#[from] DraftOrphanCleanupManifestError),
}

/// Directory-level inspection failure that invalidates the entire cleanup plan.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DraftOrphanCleanupScanError {
    /// The drafts-directory status itself could not be established.
    #[error(transparent)]
    Status(#[from] DraftOrphanCleanupStatusError),
    /// The configured drafts path existed with an unsupported kind.
    #[error("draft cleanup path {path} is not a directory: {status:?}")]
    NotDirectory {
        /// Configured drafts path.
        path: PathBuf,
        /// Coarse kind that made directory traversal unsafe.
        status: PathStatus,
    },
    /// Directory traversal failed before a trusted bounded result was available.
    #[error("failed to scan draft cleanup directory {path}: {detail}")]
    Read {
        /// Drafts directory that could not be traversed.
        path: PathBuf,
        /// Platform traversal error retained for recovery diagnostics.
        detail: String,
    },
}

/// Side-effect-free evidence collected by one bounded orphan inspection pass.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DraftOrphanCleanupPlan {
    /// Manifest generations whose body was confirmed missing during inspection.
    pub missing_body_entries: Vec<DraftEntryFingerprint>,
    /// Regular draft bodies absent from the trusted manifest snapshot.
    pub orphan_bodies: Vec<DraftOrphanBodyCandidate>,
    /// Ambiguous or unsupported artifacts deliberately preserved.
    pub retained: Vec<DraftOrphanCleanupRetained>,
    /// Per-entry status failures that did not invalidate the directory scan.
    pub failures: Vec<DraftOrphanCleanupFailure>,
    /// Whether the directory hit its scan bound or another manifest page remains.
    pub has_more_work: bool,
    /// Manifest offset for the next bounded pass, when entries remain.
    pub next_manifest_offset: Option<usize>,
}

/// Confirmed, skipped, and failed effects from executing one cleanup plan.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DraftOrphanCleanupOutcome {
    /// Body paths whose deletion returned a confirmed changed outcome.
    pub deleted_files: Vec<PathBuf>,
    /// Body paths already absent when deletion was revalidated.
    pub already_absent_files: Vec<PathBuf>,
    /// Exact manifest generations accepted by the durable serialized update.
    pub committed_manifest_removals: HashSet<DraftEntryFingerprint>,
    /// Latest trusted persisted manifest after execution, when it could be loaded.
    pub latest_persisted_manifest: Option<DraftManifest>,
    /// Ambiguous, stale, or failed artifacts preserved for later recovery.
    pub retained: Vec<DraftOrphanCleanupRetained>,
    /// Typed failures grouped by status, deletion, or manifest persistence.
    pub failures: Vec<DraftOrphanCleanupFailure>,
    /// Whether another normal deferred opportunity may still find bounded work.
    pub has_more_work: bool,
    /// Manifest offset the next bounded pass should inspect, when applicable.
    pub next_manifest_offset: Option<usize>,
}

impl DraftOrphanCleanupOutcome {
    /// Count only confirmed file deletions and durably committed manifest removals.
    #[must_use]
    pub fn confirmed_cleaned_count(&self) -> usize {
        saturating_confirmed_cleanup_count(
            self.deleted_files.len(),
            self.committed_manifest_removals.len(),
        )
    }
}

/// Merge exact durable cleanup commits into a current in-memory manifest.
///
/// This pure, allocation-free pass uses borrowed IDs and verifies every
/// persisted generation field, preserving concurrent same-ID autosaves.
pub fn merge_committed_orphan_removals(
    manifest: &mut DraftManifest,
    committed_by_id: &HashMap<String, DraftEntryFingerprint>,
) {
    manifest.drafts.retain(|entry| {
        !committed_by_id
            .get(entry.draft_id.as_str())
            .is_some_and(|fingerprint| fingerprint.matches(entry))
    });
}

/// Combine confirmed actions without allowing a diagnostic count to overflow.
pub(super) const fn saturating_confirmed_cleanup_count(deleted: usize, removed: usize) -> usize {
    deleted.saturating_add(removed)
}

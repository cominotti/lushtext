// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed evidence and outcomes for conservative draft-orphan cleanup.
//!
//! These plain Rust values separate side-effect-free inspection from durable
//! execution while keeping GTK and filesystem mechanics in the parent service.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::model::draft::{DraftCleanupContinuation, DraftEntry, DraftManifest};
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
    /// Persisted continuation could not be treated as a portable filename.
    #[error("draft cleanup continuation is not trusted: {file_name:?}")]
    UntrustedContinuation {
        /// Malformed filename boundary preserved for diagnostics only.
        file_name: String,
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
    /// Durable cursor inspected when this plan was built.
    pub inspected_directory_continuation: Option<DraftCleanupContinuation>,
    /// Cursor to commit only if execution accepts this exact inspected state.
    pub next_directory_continuation: Option<DraftCleanupContinuation>,
    /// Whether this page crossed the directory end and restarted at the beginning.
    pub directory_wrapped: bool,
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
    /// Latest durably accepted directory cursor, when trusted persistence succeeded.
    pub directory_continuation: Option<DraftCleanupContinuation>,
    /// Whether the inspected page crossed the directory end.
    pub directory_wrapped: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a manifest entry whose generation fields all derive from
    /// `saved_at_secs`, so two entries sharing an ID differ in every field a
    /// fingerprint compares.
    fn entry(id: &str, saved_at_secs: u64) -> DraftEntry {
        DraftEntry {
            draft_id: id.to_string(),
            original_path: Some(PathBuf::from(format!("/{id}.rs"))),
            original_mtime_secs: Some(saved_at_secs),
            saved_at_secs,
        }
    }

    fn fingerprints(entries: &[DraftEntry]) -> HashMap<String, DraftEntryFingerprint> {
        entries
            .iter()
            .map(|entry| {
                (
                    entry.draft_id.clone(),
                    DraftEntryFingerprint::from_entry(entry),
                )
            })
            .collect()
    }

    /// A same-ID autosave that landed *after* inspection must survive the merge.
    ///
    /// This is the destructive-cleanup safety property: the worker inspected
    /// generation 1 and committed its deletion, but the live manifest now holds
    /// generation 2 for that same draft ID. Removing by ID alone would discard
    /// the newer entry and orphan the user's most recent recovery body.
    #[test]
    fn cleanup_merge_removes_only_exact_committed_generation() {
        let old = entry("same", 1);
        let newer = entry("same", 2);
        let unrelated = entry("other", 1);
        let mut manifest = DraftManifest {
            drafts: vec![newer.clone(), unrelated.clone()],
            cleanup_continuation: None,
        };

        merge_committed_orphan_removals(&mut manifest, &fingerprints(&[old]));

        assert_eq!(
            manifest.drafts,
            vec![newer, unrelated],
            "an exact-generation mismatch must preserve the entry"
        );
    }

    /// An exact generation match is removed, and unrelated concurrent additions
    /// are left untouched.
    #[test]
    fn cleanup_merge_removes_matching_generation_and_preserves_additions() {
        let removed = entry("removed", 1);
        let concurrent = entry("concurrent", 2);
        let mut manifest = DraftManifest {
            drafts: vec![removed.clone(), concurrent.clone()],
            cleanup_continuation: None,
        };

        merge_committed_orphan_removals(&mut manifest, &fingerprints(&[removed]));

        assert_eq!(manifest.drafts, vec![concurrent]);
    }

    /// Every fingerprint field participates in the match, one dimension at a
    /// time. Without this, a mutant that drops a single comparison from
    /// `DraftEntryFingerprint::matches` still passes the two merge tests above,
    /// because those differ in all three generation fields at once.
    #[test]
    fn cleanup_merge_requires_every_fingerprint_dimension_to_match() {
        let committed = entry("draft", 1);

        for (label, live) in [
            ("mtime", {
                let mut e = committed.clone();
                e.original_mtime_secs = Some(99);
                e
            }),
            ("path", {
                let mut e = committed.clone();
                e.original_path = Some(PathBuf::from("/renamed.rs"));
                e
            }),
            ("saved_at", {
                let mut e = committed.clone();
                e.saved_at_secs = 99;
                e
            }),
        ] {
            let mut manifest = DraftManifest {
                drafts: vec![live.clone()],
                cleanup_continuation: None,
            };

            merge_committed_orphan_removals(
                &mut manifest,
                &fingerprints(std::slice::from_ref(&committed)),
            );

            assert_eq!(
                manifest.drafts,
                vec![live],
                "a differing {label} must keep the live entry"
            );
        }
    }

    /// An empty commit set is a no-op rather than a manifest clear.
    #[test]
    fn cleanup_merge_with_no_commits_removes_nothing() {
        let kept = vec![entry("a", 1), entry("b", 2)];
        let mut manifest = DraftManifest {
            drafts: kept.clone(),
            cleanup_continuation: None,
        };

        merge_committed_orphan_removals(&mut manifest, &HashMap::new());

        assert_eq!(manifest.drafts, kept);
    }

    /// The diagnostic count saturates rather than overflowing.
    #[test]
    fn confirmed_cleanup_count_saturates_instead_of_overflowing() {
        assert_eq!(saturating_confirmed_cleanup_count(2, 3), 5);
        assert_eq!(
            saturating_confirmed_cleanup_count(usize::MAX, 1),
            usize::MAX
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Persisted local-history metadata for one saved document.
//!
//! Local history stores full-text snapshots on disk, but the UI usually needs
//! lightweight metadata first so it can list timestamps and capture origins
//! without reading every snapshot body up front. This module keeps those value
//! types pure and serialization-friendly for the service layer.

use serde::{Deserialize, Serialize};

use super::sidecar_identity::{DocumentSidecarIdentity, next_record_id, now_epoch_millis};

/// Why one local-history snapshot was captured.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LocalHistorySnapshotOrigin {
    /// Snapshot of the saved content that existed before the first unsaved edit.
    Baseline,
    /// Snapshot captured during a long unsaved editing session.
    Periodic,
    /// Snapshot captured after a successful save.
    Save,
    /// Snapshot captured immediately before restoring older history into the buffer.
    RestoreSafety,
}

impl LocalHistorySnapshotOrigin {
    /// Human-friendly label shown in the history browser.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Baseline => "Before edits",
            Self::Periodic => "While editing",
            Self::Save => "Saved",
            Self::RestoreSafety => "Before restore",
        }
    }
}

/// Metadata for one stored local-history snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalHistorySnapshotMeta {
    /// Stable snapshot identifier used as the on-disk filename stem.
    pub snapshot_id: String,
    /// Capture timestamp in epoch milliseconds so newest/oldest ordering stays stable.
    pub captured_at_millis: u64,
    /// Why this snapshot exists.
    pub origin: LocalHistorySnapshotOrigin,
    /// UTF-8 byte length of the normalized stored text.
    pub byte_len: u64,
    /// Deterministic hash of the stored text used for deduplication.
    pub content_hash: String,
}

impl LocalHistorySnapshotMeta {
    /// Create metadata for one freshly captured snapshot.
    #[must_use]
    pub fn new(origin: LocalHistorySnapshotOrigin, byte_len: u64, content_hash: String) -> Self {
        Self {
            snapshot_id: next_record_id("history"),
            captured_at_millis: now_epoch_millis(),
            origin,
            byte_len,
            content_hash,
        }
    }
}

/// Persisted local-history lineage for one saved document identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalHistoryDocument {
    /// Saved-file identity this lineage belongs to.
    pub identity: DocumentSidecarIdentity,
    /// Snapshot metadata stored newest-first after normalization.
    pub snapshots: Vec<LocalHistorySnapshotMeta>,
}

impl LocalHistoryDocument {
    /// Create an empty history lineage for a resolved saved-file identity.
    #[must_use]
    pub fn empty(identity: DocumentSidecarIdentity) -> Self {
        Self {
            identity,
            snapshots: Vec::new(),
        }
    }

    /// Keep snapshots ordered from newest to oldest for browse surfaces and pruning.
    pub fn sort_newest_first(&mut self) {
        self.snapshots.sort_by(|left, right| {
            right
                .captured_at_millis
                .cmp(&left.captured_at_millis)
                .then_with(|| right.snapshot_id.cmp(&left.snapshot_id))
        });
    }
}

/// One loaded local-history snapshot with its full text body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHistorySnapshot {
    /// Metadata used in list rows and action routing.
    pub meta: LocalHistorySnapshotMeta,
    /// Full normalized UTF-8 text stored for restore and copy workflows.
    pub text: String,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::model::sidecar_identity::DocumentSidecarIdentity;

    #[test]
    fn snapshot_origin_labels_match_browser_copy() {
        assert_eq!(LocalHistorySnapshotOrigin::Baseline.label(), "Before edits");
        assert_eq!(
            LocalHistorySnapshotOrigin::Periodic.label(),
            "While editing"
        );
        assert_eq!(LocalHistorySnapshotOrigin::Save.label(), "Saved");
        assert_eq!(
            LocalHistorySnapshotOrigin::RestoreSafety.label(),
            "Before restore"
        );
    }

    #[test]
    fn document_sort_keeps_newest_first() {
        let identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/tmp/file.txt"),
            PathBuf::from("/tmp/file.txt"),
        );
        let mut document = LocalHistoryDocument::empty(identity);
        document.snapshots = vec![
            LocalHistorySnapshotMeta {
                snapshot_id: "history-b".to_string(),
                captured_at_millis: 10,
                origin: LocalHistorySnapshotOrigin::Save,
                byte_len: 10,
                content_hash: "b".to_string(),
            },
            LocalHistorySnapshotMeta {
                snapshot_id: "history-c".to_string(),
                captured_at_millis: 20,
                origin: LocalHistorySnapshotOrigin::Baseline,
                byte_len: 20,
                content_hash: "c".to_string(),
            },
            LocalHistorySnapshotMeta {
                snapshot_id: "history-a".to_string(),
                captured_at_millis: 20,
                origin: LocalHistorySnapshotOrigin::Periodic,
                byte_len: 20,
                content_hash: "a".to_string(),
            },
        ];

        document.sort_newest_first();

        assert_eq!(document.snapshots[0].snapshot_id, "history-c");
        assert_eq!(document.snapshots[1].snapshot_id, "history-a");
        assert_eq!(document.snapshots[2].snapshot_id, "history-b");
    }
}

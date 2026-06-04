// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft persistence model — tracks unsaved buffer content stored on disk.
//!
//! When a user has unsaved changes and the editor exits (or crashes), drafts
//! preserve the buffer content so it can be restored on the next session.
//! The manifest maps draft IDs to original file paths and metadata.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One draft entry in the manifest. Maps a draft file on disk to the
/// original source file and tracks metadata for conflict detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DraftEntry {
    /// Stable identifier for this draft, used as the filename stem.
    /// For path-backed files: hex-encoded hash of the absolute path.
    /// For untitled tabs: a generated unique ID.
    pub draft_id: String,
    /// Original file path. `None` for untitled tabs.
    pub original_path: Option<PathBuf>,
    /// mtime of the original file (seconds since epoch) when the draft was
    /// last written. Used for conflict detection: if the file's mtime has
    /// changed since the draft was saved, both the file and draft diverged.
    pub original_mtime_secs: Option<u64>,
    /// When this draft was last written to disk (seconds since epoch).
    pub saved_at_secs: u64,
}

/// The full draft manifest, stored as JSON alongside the draft files.
/// Loaded at startup and kept in memory; written atomically on each update.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DraftManifest {
    pub drafts: Vec<DraftEntry>,
}

/// Preloaded draft-restore data consumed exactly once during startup restore.
///
/// Untitled drafts and validated file-backed drafts preload their restored
/// content directly. File-backed drafts that were proven stale preload a
/// warning marker instead so the editor can show feedback without applying the
/// old content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreloadedDraftRestore {
    /// Restored content ready to apply once the target editor is available.
    Content(String),
    /// A file-backed draft was discarded because the backing file changed.
    SkipStaleFile,
    /// The draft remains on disk but is too large to apply automatically.
    SkipOversized,
}

/// Result of validating a file-backed draft against its current backing file.
///
/// The service layer computes this on a background thread so the GTK layer can
/// decide whether to apply content, warn once, or quietly skip restore without
/// touching blocking filesystem APIs on the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDraftRestoreResolution {
    /// The draft is still safe to restore and includes ready-to-apply content.
    Restore { content: String },
    /// The backing file's mtime changed, so restoring would overwrite newer data.
    SkipStale,
    /// The draft is too large to read into a GTK buffer automatically.
    SkipOversized,
    /// Automatic restore could not trust current file metadata for this attempt.
    SkipUnavailable,
    /// The manifest entry existed, but the draft file itself was already gone.
    MissingDraft,
}

impl DraftManifest {
    /// Find a draft entry by original file path.
    #[must_use]
    pub fn find_by_path(&self, path: &std::path::Path) -> Option<&DraftEntry> {
        self.drafts
            .iter()
            .find(|d| d.original_path.as_deref() == Some(path))
    }

    /// Find a draft entry by draft ID.
    #[must_use]
    pub fn find_by_id(&self, draft_id: &str) -> Option<&DraftEntry> {
        self.drafts.iter().find(|d| d.draft_id == draft_id)
    }

    /// Remove a draft entry by draft ID. Returns `true` if an entry was removed.
    pub fn remove_by_id(&mut self, draft_id: &str) -> bool {
        let before = self.drafts.len();
        self.drafts.retain(|d| d.draft_id != draft_id);
        self.drafts.len() < before
    }

    /// Remove a draft entry by original path. Returns `true` if an entry was removed.
    pub fn remove_by_path(&mut self, path: &std::path::Path) -> bool {
        let before = self.drafts.len();
        self.drafts
            .retain(|d| d.original_path.as_deref() != Some(path));
        self.drafts.len() < before
    }

    /// Add or update a draft entry. If an entry with the same draft_id exists,
    /// it is replaced.
    pub fn upsert(&mut self, entry: DraftEntry) {
        if let Some(existing) = self
            .drafts
            .iter_mut()
            .find(|d| d.draft_id == entry.draft_id)
        {
            *existing = entry;
        } else {
            self.drafts.push(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, path: Option<&str>) -> DraftEntry {
        DraftEntry {
            draft_id: id.to_string(),
            original_path: path.map(PathBuf::from),
            original_mtime_secs: Some(1000),
            saved_at_secs: 2000,
        }
    }

    #[test]
    fn find_by_path_returns_matching_entry() {
        let manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs")), entry("def", Some("/b.rs"))],
        };
        assert_eq!(
            manifest.find_by_path(std::path::Path::new("/b.rs")),
            Some(&entry("def", Some("/b.rs")))
        );
    }

    #[test]
    fn find_by_path_returns_none_for_missing() {
        let manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs"))],
        };
        assert_eq!(
            manifest.find_by_path(std::path::Path::new("/missing.rs")),
            None
        );
    }

    #[test]
    fn find_by_id_returns_matching_entry() {
        let manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs"))],
        };
        assert_eq!(
            manifest.find_by_id("abc"),
            Some(&entry("abc", Some("/a.rs")))
        );
    }

    #[test]
    fn remove_by_id_removes_and_returns_true() {
        let mut manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs")), entry("def", Some("/b.rs"))],
        };
        assert!(manifest.remove_by_id("abc"));
        assert_eq!(manifest.drafts.len(), 1);
        assert_eq!(manifest.drafts[0].draft_id, "def");
    }

    #[test]
    fn remove_by_id_returns_false_for_missing() {
        let mut manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs"))],
        };
        assert!(!manifest.remove_by_id("missing"));
        assert_eq!(manifest.drafts.len(), 1);
    }

    #[test]
    fn remove_by_path_removes_matching_entry() {
        let mut manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs")), entry("def", Some("/b.rs"))],
        };
        assert!(manifest.remove_by_path(std::path::Path::new("/a.rs")));
        assert_eq!(manifest.drafts.len(), 1);
        assert_eq!(manifest.drafts[0], entry("def", Some("/b.rs")));
    }

    #[test]
    fn remove_by_path_returns_false_and_preserves_entries_when_missing() {
        let mut manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs")), entry("untitled-1", None)],
        };

        assert!(!manifest.remove_by_path(std::path::Path::new("/missing.rs")));
        assert_eq!(
            manifest.drafts,
            vec![entry("abc", Some("/a.rs")), entry("untitled-1", None)]
        );
    }

    #[test]
    fn upsert_adds_new_entry() {
        let mut manifest = DraftManifest::default();
        manifest.upsert(entry("abc", Some("/a.rs")));
        assert_eq!(manifest.drafts.len(), 1);
    }

    #[test]
    fn upsert_replaces_existing_entry() {
        let mut manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs"))],
        };
        let updated = DraftEntry {
            draft_id: "abc".to_string(),
            original_path: Some(PathBuf::from("/a.rs")),
            original_mtime_secs: Some(9999),
            saved_at_secs: 9999,
        };
        manifest.upsert(updated);
        assert_eq!(manifest.drafts.len(), 1);
        assert_eq!(manifest.drafts[0].saved_at_secs, 9999);
    }

    #[test]
    fn default_manifest_is_empty() {
        let manifest = DraftManifest::default();
        assert!(manifest.drafts.is_empty());
    }

    #[test]
    fn serialization_roundtrip() {
        let manifest = DraftManifest {
            drafts: vec![entry("abc", Some("/a.rs")), entry("untitled-1", None)],
        };
        let json = serde_json::to_string(&manifest).expect("expected operation to succeed");
        let deserialized: DraftManifest =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(deserialized, manifest);
    }

    #[test]
    fn find_by_path_skips_untitled_entries() {
        let manifest = DraftManifest {
            drafts: vec![entry("untitled-1", None), entry("abc", Some("/a.rs"))],
        };
        assert_eq!(
            manifest.find_by_path(std::path::Path::new("/a.rs")),
            Some(&entry("abc", Some("/a.rs")))
        );
    }
}

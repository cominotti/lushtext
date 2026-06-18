// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared value objects for metadata format inventory and upgrade plans.
//!
//! These types are plain Rust data so both startup and Preferences can render
//! the same service decisions without learning old payload formats.

use std::path::{Path, PathBuf};

use crate::services::json_format::{
    KIND_BOOKMARK_SIDECAR, KIND_DOCUMENT_NOTE_SIDECAR, KIND_DRAFT_MANIFEST,
    KIND_FOLDER_NOTE_SIDECAR, KIND_LEGACY_WORKSPACE_NOTE_SIDECAR, KIND_LOCAL_HISTORY_INDEX,
    KIND_MIGRATION_LEDGER, KIND_REPLACE_UNDO_CLEANUP_MARKER, KIND_REPLACE_UNDO_ENTRY,
    KIND_REPLACE_UNDO_MANIFEST, KIND_RETIRED_REPLACE_UNDO_BACKUP, KIND_SAVED_SEARCHES,
    KIND_SEARCH_HISTORY, KIND_SESSION, KIND_WORKSPACE_STATE, SUPPORTED_JSON_VERSION,
};

/// Default maximum number of sidecar files scanned per app-owned sidecar directory.
///
/// Ten thousand mirrors existing large-directory UI safety budgets: it is far
/// beyond normal use but prevents a damaged data directory from monopolizing
/// startup preflight.
pub const SIDECAR_SCAN_MAX_ENTRIES: usize = 10_000;
/// Maximum local-history lineage directories inspected during one preflight.
///
/// Preflight reads only each lineage `index.json`; snapshot text bodies are not
/// format metadata and stay outside this scan.
pub const LOCAL_HISTORY_SCAN_MAX_LINEAGES: usize = 10_000;
/// Maximum Replace All undo entry files inspected during one preflight.
///
/// This matches the existing undo-journal recovery budget so upgrade status and
/// runtime cleanup see the same damaged-directory scale.
pub const REPLACE_JOURNAL_SCAN_MAX_ENTRIES: usize = 10_000;
/// Maximum draft bodies included in preservation-only inventory.
///
/// Draft bodies are plain UTF-8 content, not versioned JSON. The limit matches
/// the manifest repair/orphan-cleanup budget to keep startup work bounded.
pub const DRAFT_BODY_SCAN_MAX_ENTRIES: usize = 2_048;

/// Bounded scan limits used by format-upgrade inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormatScanBounds {
    /// Maximum bytes read for one JSON metadata file.
    pub max_metadata_bytes: u64,
    /// Maximum entries per sidecar directory.
    pub max_sidecar_entries: usize,
    /// Maximum local-history lineage directories.
    pub max_local_history_lineages: usize,
    /// Maximum Replace All undo entry files.
    pub max_replace_journal_entries: usize,
    /// Maximum draft body files tracked for preservation-only status.
    pub max_draft_bodies: usize,
}

impl Default for FormatScanBounds {
    fn default() -> Self {
        Self {
            max_metadata_bytes: crate::services::recovery_metadata::DEFAULT_MAX_METADATA_BYTES,
            max_sidecar_entries: SIDECAR_SCAN_MAX_ENTRIES,
            max_local_history_lineages: LOCAL_HISTORY_SCAN_MAX_LINEAGES,
            max_replace_journal_entries: REPLACE_JOURNAL_SCAN_MAX_ENTRIES,
            max_draft_bodies: DRAFT_BODY_SCAN_MAX_ENTRIES,
        }
    }
}

/// App-owned metadata kind understood by the format-upgrade inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum FormatMetadataKind {
    /// Persisted workspace folders and current workspace scope.
    WorkspaceState,
    /// Global tab/session restore state.
    Session,
    /// Draft manifest that maps draft IDs to plain draft body files.
    DraftManifest,
    /// Plain UTF-8 draft body preserved by Start Fresh but not JSON-converted.
    DraftBody,
    /// User-managed saved searches.
    SavedSearches,
    /// Low-stakes recent search history.
    SearchHistory,
    /// Saved-file line bookmark sidecar.
    BookmarkSidecar,
    /// Saved-file rich note sidecar.
    DocumentNoteSidecar,
    /// Workspace-folder rich note sidecar.
    FolderNoteSidecar,
    /// Pre-rename folder-note sidecar accepted by the existing narrow reader.
    LegacyFolderNoteSidecar,
    /// Local-history lineage index.
    LocalHistoryIndex,
    /// Pending post-rename sidecar/history migration ledger.
    MigrationLedger,
    /// Replace All undo journal manifest.
    ReplaceUndoManifest,
    /// Replace All per-file undo entry.
    ReplaceUndoEntry,
    /// Replace All cleanup marker.
    ReplaceUndoCleanupMarker,
    /// Retired pre-public single-file Replace All backup.
    RetiredReplaceUndoBackup,
}

impl FormatMetadataKind {
    /// Return the latest public JSON kind for versioned metadata, if any.
    #[must_use]
    pub const fn json_kind(self) -> Option<&'static str> {
        match self {
            Self::WorkspaceState => Some(KIND_WORKSPACE_STATE),
            Self::Session => Some(KIND_SESSION),
            Self::DraftManifest => Some(KIND_DRAFT_MANIFEST),
            Self::DraftBody => None,
            Self::SavedSearches => Some(KIND_SAVED_SEARCHES),
            Self::SearchHistory => Some(KIND_SEARCH_HISTORY),
            Self::BookmarkSidecar => Some(KIND_BOOKMARK_SIDECAR),
            Self::DocumentNoteSidecar => Some(KIND_DOCUMENT_NOTE_SIDECAR),
            Self::FolderNoteSidecar => Some(KIND_FOLDER_NOTE_SIDECAR),
            Self::LegacyFolderNoteSidecar => Some(KIND_LEGACY_WORKSPACE_NOTE_SIDECAR),
            Self::LocalHistoryIndex => Some(KIND_LOCAL_HISTORY_INDEX),
            Self::MigrationLedger => Some(KIND_MIGRATION_LEDGER),
            Self::ReplaceUndoManifest => Some(KIND_REPLACE_UNDO_MANIFEST),
            Self::ReplaceUndoEntry => Some(KIND_REPLACE_UNDO_ENTRY),
            Self::ReplaceUndoCleanupMarker => Some(KIND_REPLACE_UNDO_CLEANUP_MARKER),
            Self::RetiredReplaceUndoBackup => Some(KIND_RETIRED_REPLACE_UNDO_BACKUP),
        }
    }

    /// Return a stable label for dialogs, tests, and backup manifests.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::WorkspaceState => "workspace state",
            Self::Session => "session",
            Self::DraftManifest => "draft manifest",
            Self::DraftBody => "draft body",
            Self::SavedSearches => "saved searches",
            Self::SearchHistory => "search history",
            Self::BookmarkSidecar => "bookmark sidecar",
            Self::DocumentNoteSidecar => "document note sidecar",
            Self::FolderNoteSidecar => "folder note sidecar",
            Self::LegacyFolderNoteSidecar => "legacy folder note sidecar",
            Self::LocalHistoryIndex => "local history index",
            Self::MigrationLedger => "migration ledger",
            Self::ReplaceUndoManifest => "replace undo manifest",
            Self::ReplaceUndoEntry => "replace undo entry",
            Self::ReplaceUndoCleanupMarker => "replace undo cleanup marker",
            Self::RetiredReplaceUndoBackup => "retired replace undo backup",
        }
    }

    /// Return whether normal startup must pause when this kind is actionable.
    #[must_use]
    pub const fn startup_critical(self) -> bool {
        match self {
            Self::SavedSearches | Self::SearchHistory => false,
            Self::WorkspaceState
            | Self::Session
            | Self::DraftManifest
            | Self::DraftBody
            | Self::BookmarkSidecar
            | Self::DocumentNoteSidecar
            | Self::FolderNoteSidecar
            | Self::LegacyFolderNoteSidecar
            | Self::LocalHistoryIndex
            | Self::MigrationLedger
            | Self::ReplaceUndoManifest
            | Self::ReplaceUndoEntry
            | Self::ReplaceUndoCleanupMarker
            | Self::RetiredReplaceUndoBackup => true,
        }
    }
}

/// App-data-relative item path shown to users and persisted in backup manifests.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct FormatItemPath {
    relative: PathBuf,
}

impl FormatItemPath {
    /// Build an item path from a path already known to be under the data dir.
    #[must_use]
    pub fn from_relative(path: impl Into<PathBuf>) -> Self {
        Self {
            relative: path.into(),
        }
    }

    /// Convert an absolute app-data path into the stable relative form.
    #[must_use]
    pub fn from_data_path(data_dir: &Path, path: &Path) -> Self {
        let relative = path
            .strip_prefix(data_dir)
            .map_or_else(|_| path.to_path_buf(), Path::to_path_buf);
        Self { relative }
    }

    /// Return the relative path used for app-data-local display and manifests.
    #[must_use]
    pub fn relative(&self) -> &Path {
        &self.relative
    }

    /// Resolve this item inside an app data directory.
    #[must_use]
    pub fn absolute(&self, data_dir: &Path) -> PathBuf {
        data_dir.join(&self.relative)
    }

    /// Return a lossy display string suitable for compact UI summaries.
    #[must_use]
    pub fn display(&self) -> String {
        self.relative.display().to_string()
    }
}

/// Format status for one inventory item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatClassification {
    /// The expected path is absent and needs no action.
    Missing,
    /// The path exists in the latest supported format.
    Current {
        /// Version found in the envelope, or `None` for preservation-only files.
        version: Option<u32>,
    },
    /// A tested converter path can update this item to the latest format.
    Upgradeable {
        /// Version found in the older envelope.
        from_version: u32,
        /// Latest version this binary can write.
        to_version: u32,
    },
    /// The item was created by a newer LushText and must not be downgraded.
    FutureVersion {
        /// Version found in the envelope.
        version: u32,
        /// Latest version supported by this binary.
        supported_version: u32,
    },
    /// The item is older or unsupported, but no converter exists.
    UnsupportedOld {
        /// Version found when the bytes still had a recognizable envelope.
        version: Option<u32>,
        /// Short diagnostic for logs and details views.
        detail: String,
    },
    /// The item cannot be parsed or inspected as usable metadata.
    Damaged {
        /// Short diagnostic for logs and details views.
        detail: String,
    },
    /// The item cannot be safely preserved or replaced.
    UnsafeToReplace {
        /// Short diagnostic for logs and details views.
        detail: String,
    },
}

impl FormatClassification {
    /// Return whether this item can ever receive a Convert action.
    #[must_use]
    pub const fn is_upgradeable(&self) -> bool {
        matches!(self, Self::Upgradeable { .. })
    }

    /// Return whether this item was written by a newer LushText.
    #[must_use]
    pub const fn is_future_version(&self) -> bool {
        matches!(self, Self::FutureVersion { .. })
    }

    /// Return whether this item needs preservation for Start Fresh.
    #[must_use]
    pub const fn needs_preservation(&self) -> bool {
        matches!(
            self,
            Self::Upgradeable { .. }
                | Self::FutureVersion { .. }
                | Self::UnsupportedOld { .. }
                | Self::Damaged { .. }
                | Self::UnsafeToReplace { .. }
        )
    }
}

/// Non-item condition observed while inventory stayed within its scan bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatInventoryDiagnostic {
    /// A bounded directory had more entries than preflight will inspect.
    ScanLimitReached {
        /// App-data-relative directory that was truncated.
        directory: FormatItemPath,
        /// Maximum retained entry count.
        limit: usize,
    },
    /// A known directory path existed but was not a directory.
    DirectoryUnavailable {
        /// App-data-relative directory path.
        directory: FormatItemPath,
        /// Diagnostic detail from the filesystem boundary.
        detail: String,
    },
}

/// Return the latest format version used by the public JSON envelope.
#[must_use]
pub const fn latest_format_version() -> u32 {
    SUPPORTED_JSON_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_kind_labels_and_startup_policy_are_stable() {
        let cases = [
            (FormatMetadataKind::WorkspaceState, "workspace state", true),
            (FormatMetadataKind::Session, "session", true),
            (FormatMetadataKind::DraftManifest, "draft manifest", true),
            (FormatMetadataKind::DraftBody, "draft body", true),
            (FormatMetadataKind::SavedSearches, "saved searches", false),
            (FormatMetadataKind::SearchHistory, "search history", false),
            (
                FormatMetadataKind::BookmarkSidecar,
                "bookmark sidecar",
                true,
            ),
            (
                FormatMetadataKind::DocumentNoteSidecar,
                "document note sidecar",
                true,
            ),
            (
                FormatMetadataKind::FolderNoteSidecar,
                "folder note sidecar",
                true,
            ),
            (
                FormatMetadataKind::LegacyFolderNoteSidecar,
                "legacy folder note sidecar",
                true,
            ),
            (
                FormatMetadataKind::LocalHistoryIndex,
                "local history index",
                true,
            ),
            (
                FormatMetadataKind::MigrationLedger,
                "migration ledger",
                true,
            ),
            (
                FormatMetadataKind::ReplaceUndoManifest,
                "replace undo manifest",
                true,
            ),
            (
                FormatMetadataKind::ReplaceUndoEntry,
                "replace undo entry",
                true,
            ),
            (
                FormatMetadataKind::ReplaceUndoCleanupMarker,
                "replace undo cleanup marker",
                true,
            ),
            (
                FormatMetadataKind::RetiredReplaceUndoBackup,
                "retired replace undo backup",
                true,
            ),
        ];

        for (kind, expected_label, startup_critical) in cases {
            assert_eq!(kind.label(), expected_label);
            assert_eq!(kind.startup_critical(), startup_critical);
        }
    }

    #[test]
    fn item_paths_resolve_display_and_absolute_paths() {
        let data_dir = Path::new("/tmp/lushtext-data");
        let path = FormatItemPath::from_relative("bookmarks/file.json");

        assert_eq!(path.relative(), Path::new("bookmarks/file.json"));
        assert_eq!(path.display(), "bookmarks/file.json");
        assert_eq!(
            path.absolute(data_dir),
            PathBuf::from("/tmp/lushtext-data/bookmarks/file.json")
        );
    }

    #[test]
    fn classification_predicates_distinguish_upgrade_future_and_preservation() {
        let cases = [
            (FormatClassification::Missing, false, false, false),
            (
                FormatClassification::Current { version: Some(1) },
                false,
                false,
                false,
            ),
            (
                FormatClassification::Upgradeable {
                    from_version: 0,
                    to_version: 1,
                },
                true,
                false,
                true,
            ),
            (
                FormatClassification::FutureVersion {
                    version: 99,
                    supported_version: 1,
                },
                false,
                true,
                true,
            ),
            (
                FormatClassification::UnsupportedOld {
                    version: None,
                    detail: "old".to_string(),
                },
                false,
                false,
                true,
            ),
            (
                FormatClassification::Damaged {
                    detail: "bad".to_string(),
                },
                false,
                false,
                true,
            ),
            (
                FormatClassification::UnsafeToReplace {
                    detail: "unsafe".to_string(),
                },
                false,
                false,
                true,
            ),
        ];

        for (classification, upgradeable, future_version, preservation) in cases {
            assert_eq!(classification.is_upgradeable(), upgradeable);
            assert_eq!(classification.is_future_version(), future_version);
            assert_eq!(classification.needs_preservation(), preservation);
        }
    }
}

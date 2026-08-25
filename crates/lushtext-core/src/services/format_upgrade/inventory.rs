// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only inventory scan for app-owned metadata format status.
//!
//! The scanner deliberately stops at classification. It does not repair,
//! quarantine, delete, or write any metadata; those side effects belong in the
//! explicit apply command after the user chooses an action.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::services::filesystem::{
    DirectoryScanPolicy, FileKind, PathStatus, metadata as fs_metadata, read as fs_read,
    tree as fs_tree,
};
use crate::services::format_upgrade::diagnostics::{
    FormatClassification, FormatInventoryDiagnostic, FormatItemPath, FormatMetadataKind,
    FormatScanBounds, latest_format_version,
};
use crate::services::format_upgrade::legacy::ConverterRegistry;
use crate::{model::workspace::WorkspacesFile, services::json_format};

const LEGACY_FOLDER_NOTE_SIDECAR_DIR: &str = "workspace-notes";

/// Complete app-owned metadata inventory for one data directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatInventory {
    /// App data directory that all inventory items were resolved from.
    pub data_dir: PathBuf,
    /// Item-level metadata classifications in deterministic path order.
    pub items: Vec<FormatInventoryItem>,
    /// Bounded-scan diagnostics that are not tied to one item.
    pub diagnostics: Vec<FormatInventoryDiagnostic>,
}

impl FormatInventory {
    /// Return whether every discovered item is current or missing.
    #[must_use]
    pub fn is_current_or_empty(&self) -> bool {
        self.items.iter().all(|item| {
            matches!(
                item.classification,
                FormatClassification::Current { .. } | FormatClassification::Missing
            )
        })
    }

    /// Return upgradeable items in stable scan order.
    #[must_use]
    pub fn upgradeable_items(&self) -> Vec<&FormatInventoryItem> {
        self.items
            .iter()
            .filter(|item| item.classification.is_upgradeable())
            .collect()
    }
}

/// Format classification for one app-owned metadata path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatInventoryItem {
    /// Domain category for grouped summaries and backup manifests.
    pub kind: FormatMetadataKind,
    /// App-data-relative item path.
    pub path: FormatItemPath,
    /// Absolute path resolved from the inventory data directory.
    pub absolute_path: PathBuf,
    /// File facts observed during the scan, used by apply commands to reject stale plans.
    pub(crate) file_facts: Option<FormatItemFileFacts>,
    /// Version/classification found by the read-only scan.
    pub classification: FormatClassification,
}

impl FormatInventoryItem {
    fn missing(data_dir: &Path, relative: impl Into<PathBuf>, kind: FormatMetadataKind) -> Self {
        let path = FormatItemPath::from_relative(relative);
        Self {
            absolute_path: path.absolute(data_dir),
            kind,
            path,
            file_facts: None,
            classification: FormatClassification::Missing,
        }
    }
}

/// Stable facts captured for one inventory file during scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FormatItemFileFacts {
    /// File length in bytes.
    pub byte_size: u64,
    /// Last modification time as seconds since the Unix epoch, if available.
    pub modified_at_secs: Option<u64>,
}

/// Accumulates one bounded, read-only metadata scan.
///
/// The builder keeps shared scan limits and converter knowledge together so
/// each discovered path is classified consistently without mutating app data.
struct InventoryBuilder<'a> {
    data_dir: &'a Path,
    bounds: FormatScanBounds,
    registry: &'a ConverterRegistry,
    items: Vec<FormatInventoryItem>,
    diagnostics: Vec<FormatInventoryDiagnostic>,
}

impl<'a> InventoryBuilder<'a> {
    fn new(data_dir: &'a Path, bounds: FormatScanBounds, registry: &'a ConverterRegistry) -> Self {
        Self {
            data_dir,
            bounds,
            registry,
            items: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn into_inventory(mut self) -> FormatInventory {
        self.items.sort_by(|left, right| {
            left.path
                .relative()
                .cmp(right.path.relative())
                .then(left.kind.cmp(&right.kind))
        });
        FormatInventory {
            data_dir: self.data_dir.to_path_buf(),
            items: self.items,
            diagnostics: self.diagnostics,
        }
    }

    fn scan_fixed_json(&mut self, relative: impl Into<PathBuf>, kind: FormatMetadataKind) {
        let relative = relative.into();
        let path = self.data_dir.join(&relative);
        let item_path = FormatItemPath::from_relative(relative);
        let (classification, file_facts) =
            classify_path(&path, kind, self.bounds.max_metadata_bytes, self.registry);
        self.items.push(FormatInventoryItem {
            kind,
            absolute_path: path,
            path: item_path,
            file_facts,
            classification,
        });
    }

    fn scan_json_directory(
        &mut self,
        relative_dir: &'static str,
        kind: FormatMetadataKind,
        max_entries: usize,
    ) {
        let dir = self.data_dir.join(relative_dir);
        match fs_metadata::path_status(&dir) {
            Ok(PathStatus::Missing) => {
                self.items.push(FormatInventoryItem::missing(
                    self.data_dir,
                    relative_dir,
                    kind,
                ));
            }
            Ok(PathStatus::Directory) => {
                let entries = self.scan_bounded_directory(&dir, relative_dir, max_entries);
                for entry in entries {
                    if entry.kind == FileKind::File && has_extension(&entry.path, "json") {
                        let relative = FormatItemPath::from_data_path(self.data_dir, &entry.path);
                        let (classification, file_facts) = classify_path(
                            &entry.path,
                            kind,
                            self.bounds.max_metadata_bytes,
                            self.registry,
                        );
                        self.items.push(FormatInventoryItem {
                            kind,
                            absolute_path: entry.path,
                            path: relative,
                            file_facts,
                            classification,
                        });
                    } else if entry.kind != FileKind::Directory {
                        let relative = FormatItemPath::from_data_path(self.data_dir, &entry.path);
                        self.items.push(FormatInventoryItem {
                            kind,
                            absolute_path: entry.path,
                            path: relative,
                            file_facts: None,
                            classification: FormatClassification::UnsafeToReplace {
                                detail: "sidecar entry is not a JSON file".to_string(),
                            },
                        });
                    }
                }
            }
            Ok(status) => self.push_directory_unavailable(
                relative_dir,
                format!("expected directory, found {status:?}"),
            ),
            Err(error) => self.push_directory_unavailable(relative_dir, error.to_string()),
        }
    }

    fn scan_draft_bodies(&mut self) {
        let relative_dir = "drafts";
        let dir = self.data_dir.join(relative_dir);
        match fs_metadata::path_status(&dir) {
            Ok(PathStatus::Missing) => {}
            Ok(PathStatus::Directory) => {
                for entry in
                    self.scan_bounded_directory(&dir, relative_dir, self.bounds.max_draft_bodies)
                {
                    if entry.kind == FileKind::File && has_extension(&entry.path, "draft") {
                        let relative = FormatItemPath::from_data_path(self.data_dir, &entry.path);
                        let file_facts = item_file_facts(&entry.path).ok();
                        self.items.push(FormatInventoryItem {
                            kind: FormatMetadataKind::DraftBody,
                            absolute_path: entry.path,
                            path: relative,
                            file_facts,
                            classification: FormatClassification::Current { version: None },
                        });
                    }
                }
            }
            Ok(status) => self.push_directory_unavailable(
                relative_dir,
                format!("expected directory, found {status:?}"),
            ),
            Err(error) => self.push_directory_unavailable(relative_dir, error.to_string()),
        }
    }

    fn scan_local_history(&mut self) {
        let relative_dir = "local-history";
        let dir = self.data_dir.join(relative_dir);
        match fs_metadata::path_status(&dir) {
            Ok(PathStatus::Missing) => {
                self.items.push(FormatInventoryItem::missing(
                    self.data_dir,
                    relative_dir,
                    FormatMetadataKind::LocalHistoryIndex,
                ));
            }
            Ok(PathStatus::Directory) => {
                let entries = self.scan_bounded_directory(
                    &dir,
                    relative_dir,
                    self.bounds.max_local_history_lineages,
                );
                for entry in entries {
                    if entry.kind == FileKind::Directory {
                        let index = entry.path.join("index.json");
                        let relative = FormatItemPath::from_data_path(self.data_dir, &index);
                        let (classification, file_facts) = classify_path(
                            &index,
                            FormatMetadataKind::LocalHistoryIndex,
                            self.bounds.max_metadata_bytes,
                            self.registry,
                        );
                        self.items.push(FormatInventoryItem {
                            kind: FormatMetadataKind::LocalHistoryIndex,
                            absolute_path: index,
                            path: relative,
                            file_facts,
                            classification,
                        });
                    }
                }
            }
            Ok(status) => self.push_directory_unavailable(
                relative_dir,
                format!("expected directory, found {status:?}"),
            ),
            Err(error) => self.push_directory_unavailable(relative_dir, error.to_string()),
        }
    }

    fn scan_replace_journal(&mut self) {
        let relative_dir = "replace-backup-journal";
        self.scan_fixed_json(
            format!("{relative_dir}/manifest.json"),
            FormatMetadataKind::ReplaceUndoManifest,
        );
        self.scan_fixed_json(
            format!("{relative_dir}/cleanup-in-progress.json"),
            FormatMetadataKind::ReplaceUndoCleanupMarker,
        );

        let dir = self.data_dir.join(relative_dir);
        if !matches!(fs_metadata::path_status(&dir), Ok(PathStatus::Directory)) {
            return;
        }

        for entry in
            self.scan_bounded_directory(&dir, relative_dir, self.bounds.max_replace_journal_entries)
        {
            let file_name = entry.path.file_name().and_then(OsStr::to_str);
            if file_name == Some("manifest.json") || file_name == Some("cleanup-in-progress.json") {
                continue;
            }
            if entry.kind == FileKind::File && has_extension(&entry.path, "json") {
                let relative = FormatItemPath::from_data_path(self.data_dir, &entry.path);
                let (classification, file_facts) = classify_path(
                    &entry.path,
                    FormatMetadataKind::ReplaceUndoEntry,
                    self.bounds.max_metadata_bytes,
                    self.registry,
                );
                self.items.push(FormatInventoryItem {
                    kind: FormatMetadataKind::ReplaceUndoEntry,
                    absolute_path: entry.path,
                    path: relative,
                    file_facts,
                    classification,
                });
            }
        }
    }

    fn scan_bounded_directory(
        &mut self,
        dir: &Path,
        relative_dir: &'static str,
        max_entries: usize,
    ) -> Vec<crate::services::filesystem::DirectoryEntryInfo> {
        // Ask for one extra entry so truncation can be detected without
        // walking the rest of a damaged or hostile directory.
        let policy = DirectoryScanPolicy {
            max_entries: max_entries.saturating_add(1),
            include_hidden: true,
        };
        match fs_tree::scan_directory(dir, policy) {
            Ok(mut entries) => {
                if entries.len() > max_entries {
                    entries.truncate(max_entries);
                    self.diagnostics
                        .push(FormatInventoryDiagnostic::ScanLimitReached {
                            directory: FormatItemPath::from_relative(relative_dir),
                            limit: max_entries,
                        });
                }
                entries
            }
            Err(error) => {
                self.push_directory_unavailable(relative_dir, error.to_string());
                Vec::new()
            }
        }
    }

    fn push_directory_unavailable(&mut self, relative_dir: &'static str, detail: String) {
        self.diagnostics
            .push(FormatInventoryDiagnostic::DirectoryUnavailable {
                directory: FormatItemPath::from_relative(relative_dir),
                detail,
            });
    }
}

/// Scan app-owned metadata with the production legacy-converter registry.
///
/// # Errors
///
/// This function reports per-item read/parse problems inside the inventory
/// rather than failing the whole scan.
#[must_use]
pub fn scan(data_dir: &Path) -> FormatInventory {
    let registry = ConverterRegistry::production();
    scan_with_registry(data_dir, FormatScanBounds::default(), &registry)
}

/// Scan app-owned metadata with caller-supplied bounds and converter knowledge.
#[must_use]
pub(crate) fn scan_with_registry(
    data_dir: &Path,
    bounds: FormatScanBounds,
    registry: &ConverterRegistry,
) -> FormatInventory {
    let mut builder = InventoryBuilder::new(data_dir, bounds, registry);

    builder.scan_fixed_json("workspaces.json", FormatMetadataKind::WorkspaceState);
    builder.scan_fixed_json("session.json", FormatMetadataKind::Session);
    builder.scan_fixed_json("drafts/manifest.json", FormatMetadataKind::DraftManifest);
    builder.scan_draft_bodies();
    builder.scan_fixed_json("saved-searches.json", FormatMetadataKind::SavedSearches);
    builder.scan_fixed_json("search-history.json", FormatMetadataKind::SearchHistory);
    builder.scan_json_directory(
        "bookmarks",
        FormatMetadataKind::BookmarkSidecar,
        bounds.max_sidecar_entries,
    );
    builder.scan_json_directory(
        "document-notes",
        FormatMetadataKind::DocumentNoteSidecar,
        bounds.max_sidecar_entries,
    );
    builder.scan_json_directory(
        "folder-notes",
        FormatMetadataKind::FolderNoteSidecar,
        bounds.max_sidecar_entries,
    );
    builder.scan_json_directory(
        LEGACY_FOLDER_NOTE_SIDECAR_DIR,
        FormatMetadataKind::LegacyFolderNoteSidecar,
        bounds.max_sidecar_entries,
    );
    builder.scan_local_history();
    builder.scan_fixed_json("migration-ledger.json", FormatMetadataKind::MigrationLedger);
    builder.scan_replace_journal();
    builder.scan_fixed_json(
        "replace-backup.json",
        FormatMetadataKind::RetiredReplaceUndoBackup,
    );

    builder.into_inventory()
}

/// Classify one expected metadata path without mutating it.
fn classify_path(
    path: &Path,
    kind: FormatMetadataKind,
    max_metadata_bytes: u64,
    registry: &ConverterRegistry,
) -> (FormatClassification, Option<FormatItemFileFacts>) {
    match fs_metadata::path_status(path) {
        Ok(PathStatus::Missing) => return (FormatClassification::Missing, None),
        Ok(PathStatus::File) => {}
        Ok(status) => {
            return (
                FormatClassification::UnsafeToReplace {
                    detail: format!("expected file, found {status:?}"),
                },
                None,
            );
        }
        Err(error) => {
            return (
                FormatClassification::Damaged {
                    detail: format!("metadata status failed: {error}"),
                },
                None,
            );
        }
    }

    let file_facts = match item_file_facts(path) {
        Ok(file_facts) => file_facts,
        Err(error) => {
            return (FormatClassification::Damaged { detail: error }, None);
        }
    };

    if kind == FormatMetadataKind::DraftBody {
        return (
            FormatClassification::Current { version: None },
            Some(file_facts),
        );
    }

    if file_facts.byte_size > max_metadata_bytes {
        return (
            FormatClassification::Damaged {
                detail: format!(
                    "metadata file is {} bytes, above {} byte limit",
                    file_facts.byte_size, max_metadata_bytes
                ),
            },
            Some(file_facts),
        );
    }

    let bytes = match fs_read::bytes(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                FormatClassification::Damaged {
                    detail: format!("metadata read failed: {error}"),
                },
                Some(file_facts),
            );
        }
    };
    (
        classify_json_bytes(&bytes, kind, registry),
        Some(file_facts),
    )
}

fn item_file_facts(path: &Path) -> Result<FormatItemFileFacts, String> {
    fs_metadata::file_facts(path)
        .map(|facts| FormatItemFileFacts {
            byte_size: facts.byte_size,
            modified_at_secs: facts.modified_at_secs,
        })
        .map_err(|error| format!("metadata facts could not be read: {error}"))
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension().and_then(OsStr::to_str) == Some(expected)
}

/// Interpret one JSON envelope as current, upgradeable, future, or unsupported.
fn classify_json_bytes(
    bytes: &[u8],
    kind: FormatMetadataKind,
    registry: &ConverterRegistry,
) -> FormatClassification {
    let Some(expected_kind) = kind.json_kind() else {
        return FormatClassification::Current { version: None };
    };
    let value: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(error) => {
            return FormatClassification::Damaged {
                detail: error.to_string(),
            };
        }
    };
    let Some(object) = value.as_object() else {
        return FormatClassification::UnsupportedOld {
            version: None,
            detail: format!("expected envelope object for {expected_kind}"),
        };
    };
    let Some(found_kind) = object.get("kind").and_then(serde_json::Value::as_str) else {
        return FormatClassification::UnsupportedOld {
            version: None,
            detail: format!("missing envelope kind for {expected_kind}"),
        };
    };
    if found_kind != expected_kind {
        return FormatClassification::UnsupportedOld {
            version: None,
            detail: format!("expected kind {expected_kind}, found {found_kind}"),
        };
    }
    let Some(raw_version) = object.get("version").and_then(serde_json::Value::as_u64) else {
        return FormatClassification::UnsupportedOld {
            version: None,
            detail: format!("missing envelope version for {expected_kind}"),
        };
    };
    // Oversized version numbers are treated as future data, because the safe
    // answer is still to avoid rewriting bytes this binary does not understand.
    let version = u32::try_from(raw_version).unwrap_or(u32::MAX);
    if version == latest_format_version() {
        if object.contains_key("data") {
            if let Err(detail) = validate_latest_payload(bytes, kind) {
                return FormatClassification::UnsupportedOld {
                    version: Some(version),
                    detail,
                };
            }
            return FormatClassification::Current {
                version: Some(version),
            };
        }
        return FormatClassification::UnsupportedOld {
            version: Some(version),
            detail: format!("missing data payload for {expected_kind}"),
        };
    }
    if version > latest_format_version() {
        return FormatClassification::FutureVersion {
            version,
            supported_version: latest_format_version(),
        };
    }
    if let Some(to_version) = registry.target_version(expected_kind, version) {
        return FormatClassification::Upgradeable {
            from_version: version,
            to_version,
        };
    }
    FormatClassification::UnsupportedOld {
        version: Some(version),
        detail: format!("no converter registered for {expected_kind} v{version}"),
    }
}

/// Re-run latest readers for kinds whose envelope version is not enough to prove compatibility.
fn validate_latest_payload(bytes: &[u8], kind: FormatMetadataKind) -> Result<(), String> {
    match kind {
        // Workspace state has a known legacy payload shape that can pass
        // envelope checks; other latest-version readers own their detailed
        // recovery paths.
        FormatMetadataKind::WorkspaceState => json_format::parse_v1_payload::<WorkspacesFile>(
            bytes,
            json_format::KIND_WORKSPACE_STATE,
        )
        .map(|_| ())
        .map_err(|error| format!("{error:?}")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use crate::services::json_format::{JsonEnvelopeRef, KIND_SESSION, KIND_WORKSPACE_STATE};
    use serde_json::json;
    use tempfile::TempDir;

    fn write_json(path: &Path, value: &serde_json::Value) {
        fixture::write_text(path, &serde_json::to_string_pretty(&value).expect("json"));
    }

    #[test]
    fn current_v1_session_is_current() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!(JsonEnvelopeRef::new(KIND_SESSION, &json!({"tabs": []}))),
        );

        let inventory = scan(dir.path());

        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("session.json"))
            .expect("session item");
        assert_eq!(
            session.classification,
            FormatClassification::Current { version: Some(1) }
        );
    }

    #[test]
    fn v1_workspace_with_legacy_root_payload_is_not_current() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("workspaces.json"),
            &json!({
                "kind": KIND_WORKSPACE_STATE,
                "version": 1,
                "data": {
                    "current_scope": { "kind": "all" },
                    "workspaces": [{
                        "id": "legacy",
                        "name": "Legacy",
                        "root": "/tmp/legacy"
                    }]
                }
            }),
        );

        let inventory = scan(dir.path());

        let workspace = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("workspaces.json"))
            .expect("workspace item");
        assert!(matches!(
            workspace.classification,
            FormatClassification::UnsupportedOld {
                version: Some(1),
                ..
            }
        ));
    }

    #[test]
    fn missing_metadata_is_noop_missing() {
        let dir = TempDir::new().expect("temp dir");

        let inventory = scan(dir.path());

        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("session.json"))
            .expect("session item");
        assert_eq!(session.classification, FormatClassification::Missing);
    }

    #[test]
    fn malformed_metadata_is_damaged() {
        let dir = TempDir::new().expect("temp dir");
        fixture::write_text(&dir.path().join("session.json"), "{ not json");

        let inventory = scan(dir.path());

        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("session.json"))
            .expect("session item");
        assert!(matches!(
            session.classification,
            FormatClassification::Damaged { .. }
        ));
    }

    #[test]
    fn non_file_metadata_path_is_unsafe_to_replace() {
        let dir = TempDir::new().expect("temp dir");
        fixture::create_dir(&dir.path().join("session.json"));

        let inventory = scan(dir.path());

        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("session.json"))
            .expect("session item");
        assert!(matches!(
            session.classification,
            FormatClassification::UnsafeToReplace { .. }
        ));
    }

    #[test]
    fn future_session_is_not_upgradeable() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 99, "data": {"tabs": []}}),
        );

        let inventory = scan(dir.path());

        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("session.json"))
            .expect("session item");
        assert_eq!(
            session.classification,
            FormatClassification::FutureVersion {
                version: 99,
                supported_version: 1
            }
        );
    }

    #[test]
    fn older_version_without_converter_is_unsupported() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 0, "data": {"tabs": []}}),
        );

        let inventory = scan(dir.path());

        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("session.json"))
            .expect("session item");
        assert!(matches!(
            session.classification,
            FormatClassification::UnsupportedOld {
                version: Some(0),
                ..
            }
        ));
    }

    #[test]
    fn registered_converter_marks_older_version_upgradeable() {
        fn convert(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
            Ok(bytes.to_vec())
        }

        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 0, "data": {"tabs": []}}),
        );
        let registry = ConverterRegistry::production().with_converter(KIND_SESSION, 0, 1, convert);

        let inventory = scan_with_registry(dir.path(), FormatScanBounds::default(), &registry);

        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("session.json"))
            .expect("session item");
        assert_eq!(
            session.classification,
            FormatClassification::Upgradeable {
                from_version: 0,
                to_version: 1
            }
        );
    }

    #[test]
    fn inventory_query_helpers_report_current_and_upgradeable_items() {
        fn convert(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
            Ok(bytes.to_vec())
        }

        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 0, "data": {"tabs": []}}),
        );
        let registry = ConverterRegistry::production().with_converter(KIND_SESSION, 0, 1, convert);

        let inventory = scan_with_registry(dir.path(), FormatScanBounds::default(), &registry);

        assert!(!inventory.is_current_or_empty());
        let upgradeable = inventory.upgradeable_items();
        assert_eq!(upgradeable.len(), 1);
        assert_eq!(upgradeable[0].path.relative(), Path::new("session.json"));

        let empty_inventory = scan(dir.path().join("empty").as_path());
        assert!(empty_inventory.is_current_or_empty());
        assert!(empty_inventory.upgradeable_items().is_empty());
    }

    #[test]
    fn sidecar_directory_scan_keeps_json_files_and_flags_non_json_files_only() {
        let dir = TempDir::new().expect("temp dir");
        let bookmarks = dir.path().join("bookmarks");
        fixture::create_dir_all(&bookmarks);
        fixture::write_text(&bookmarks.join("broken.json"), "{ not json");
        fixture::write_text(&bookmarks.join("readme.txt"), "not metadata");
        fixture::create_dir(&bookmarks.join("nested"));

        let inventory = scan(dir.path());
        let bookmark_items = inventory
            .items
            .iter()
            .filter(|item| item.kind == FormatMetadataKind::BookmarkSidecar)
            .map(|item| {
                (
                    item.path.relative().to_path_buf(),
                    item.classification.clone(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(bookmark_items.len(), 2);
        assert!(bookmark_items.iter().any(|(path, classification)| {
            path == Path::new("bookmarks/broken.json")
                && matches!(classification, FormatClassification::Damaged { .. })
        }));
        assert!(bookmark_items.iter().any(|(path, classification)| {
            path == Path::new("bookmarks/readme.txt")
                && matches!(classification, FormatClassification::UnsafeToReplace { .. })
        }));
        assert!(
            !bookmark_items
                .iter()
                .any(|(path, _)| path == Path::new("bookmarks/nested"))
        );
    }

    #[test]
    fn draft_body_scan_keeps_only_plain_draft_files() {
        let dir = TempDir::new().expect("temp dir");
        let drafts = dir.path().join("drafts");
        fixture::create_dir_all(&drafts);
        fixture::write_text(&drafts.join("keep.draft"), "draft body");
        fixture::write_text(&drafts.join("ignore.txt"), "not a draft");
        fixture::create_dir(&drafts.join("folder.draft"));

        let inventory = scan(dir.path());
        let draft_bodies = inventory
            .items
            .iter()
            .filter(|item| item.kind == FormatMetadataKind::DraftBody)
            .collect::<Vec<_>>();

        assert_eq!(draft_bodies.len(), 1);
        assert_eq!(
            draft_bodies[0].path.relative(),
            Path::new("drafts/keep.draft")
        );
        assert_eq!(
            draft_bodies[0].classification,
            FormatClassification::Current { version: None }
        );
    }

    #[test]
    fn replace_journal_scan_tracks_only_json_entry_files() {
        let dir = TempDir::new().expect("temp dir");
        let journal = dir.path().join("replace-backup-journal");
        fixture::create_dir_all(&journal);
        fixture::write_text(&journal.join("manifest.json"), "{ not manifest");
        fixture::write_text(&journal.join("cleanup-in-progress.json"), "{ not cleanup");
        fixture::write_text(&journal.join("entry.json"), "{ not entry");
        fixture::write_text(&journal.join("readme.txt"), "not metadata");
        fixture::create_dir(&journal.join("nested.json"));

        let inventory = scan(dir.path());
        let entry_items = inventory
            .items
            .iter()
            .filter(|item| item.kind == FormatMetadataKind::ReplaceUndoEntry)
            .collect::<Vec<_>>();

        assert_eq!(entry_items.len(), 1);
        assert_eq!(
            entry_items[0].path.relative(),
            Path::new("replace-backup-journal/entry.json")
        );
        assert!(matches!(
            entry_items[0].classification,
            FormatClassification::Damaged { .. }
        ));
    }

    #[test]
    fn bounded_directory_scan_reports_only_when_entry_count_exceeds_limit() {
        let dir = TempDir::new().expect("temp dir");
        let bookmarks = dir.path().join("bookmarks");
        fixture::create_dir_all(&bookmarks);
        fixture::write_text(&bookmarks.join("a.json"), "{ not json");
        fixture::write_text(&bookmarks.join("b.json"), "{ not json");
        let bounds = FormatScanBounds {
            max_sidecar_entries: 2,
            ..FormatScanBounds::default()
        };

        let exact = scan_with_registry(dir.path(), bounds, &ConverterRegistry::production());

        assert!(!exact.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FormatInventoryDiagnostic::ScanLimitReached { directory, limit }
                if directory.relative() == Path::new("bookmarks") && *limit == 2
        )));

        fixture::write_text(&bookmarks.join("c.json"), "{ not json");
        let over = scan_with_registry(dir.path(), bounds, &ConverterRegistry::production());

        assert!(over.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FormatInventoryDiagnostic::ScanLimitReached { directory, limit }
                if directory.relative() == Path::new("bookmarks") && *limit == 2
        )));
        assert_eq!(
            over.items
                .iter()
                .filter(|item| item.kind == FormatMetadataKind::BookmarkSidecar)
                .count(),
            2
        );
    }

    #[test]
    fn directory_unavailable_diagnostic_is_reported_for_known_directory_files() {
        let dir = TempDir::new().expect("temp dir");
        fixture::write_text(&dir.path().join("bookmarks"), "not a directory");

        let inventory = scan(dir.path());

        assert!(inventory.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic,
            FormatInventoryDiagnostic::DirectoryUnavailable { directory, .. }
                if directory.relative() == Path::new("bookmarks")
        )));
    }

    #[test]
    fn classify_path_allows_metadata_at_exact_byte_limit() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("session.json");
        fixture::write_text(&path, "{");
        let bounds = FormatScanBounds {
            max_metadata_bytes: 1,
            ..FormatScanBounds::default()
        };

        let inventory = scan_with_registry(dir.path(), bounds, &ConverterRegistry::production());
        let session = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new("session.json"))
            .expect("session item");

        assert!(matches!(
            &session.classification,
            FormatClassification::Damaged { detail } if !detail.contains("above")
        ));
    }

    #[test]
    fn scan_and_plan_do_not_write_app_data() {
        let dir = TempDir::new().expect("temp dir");
        write_json(
            &dir.path().join("session.json"),
            &json!({"kind": KIND_SESSION, "version": 2, "data": {"tabs": []}}),
        );
        let before = fixture::read_text(&dir.path().join("session.json"));

        let inventory = scan(dir.path());
        let plan = crate::services::format_upgrade::build_plan(&inventory);

        assert!(plan.has_future_version_blocker());
        assert_eq!(fixture::read_text(&dir.path().join("session.json")), before);
        assert!(
            !crate::services::filesystem::metadata::exists(
                &dir.path()
                    .join(crate::services::format_upgrade::FORMAT_UPGRADE_BACKUP_DIR)
            ),
            "read-only scan/plan must not create backup state"
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared sidecar persistence helpers for note-like workflows.
//!
//! Bookmarks and the richer document/folder note flows all use
//! the same patterns: canonical-path identity resolution, JSON sidecar loading,
//! workspace-folder filtering, and in-app rename migration. Keeping those helpers
//! here avoids subtle drift between the different persistence services.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

use crate::filesystem::metadata as fs_metadata;
#[cfg(test)]
use crate::filesystem::read as fs_read;
use crate::persistence::json_format::{
    KIND_BOOKMARK_SIDECAR, KIND_DOCUMENT_NOTE_SIDECAR, KIND_FOLDER_NOTE_SIDECAR,
    KIND_LEGACY_WORKSPACE_NOTE_SIDECAR, KIND_LOCAL_HISTORY_INDEX,
};
use crate::persistence::recovery_metadata::{
    RecoveryDiagnostic, RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass,
    RecoveryMetadataError, load_enveloped_json_optional_accepting,
    save_enveloped_json_path_accepting,
};
use lushtext_model::persistence::sidecar_identity::DocumentSidecarIdentity;

const NO_LEGACY_SIDECAR_KINDS: &[&str] = &[];
const LEGACY_FOLDER_NOTE_SIDECAR_KINDS: &[&str] = &[KIND_LEGACY_WORKSPACE_NOTE_SIDECAR];

/// Typed errors returned by shared note-sidecar persistence helpers.
#[derive(Debug, thiserror::Error)]
pub enum NoteStorageError {
    /// Resolving the canonical path for a saved document failed.
    #[error("failed to canonicalize {}: {source}", path.display())]
    Canonicalize {
        /// Saved document path that could not be canonicalized.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// One or more sidecars failed during a rename-migration pass.
    #[error("{failures} {kind_label} sidecar(s) could not be migrated; {migrated} succeeded")]
    IncompleteMigration {
        /// Sidecar kind label included in the aggregate summary.
        kind_label: &'static str,
        /// Number of sidecars that failed migration.
        failures: usize,
        /// Number of sidecars that succeeded.
        migrated: usize,
    },
    /// Recovery-aware metadata validation or durable replacement failed.
    #[error(transparent)]
    Recovery(#[from] RecoveryMetadataError),
}

/// Typed errors returned by the legacy strict-load test helper [`load_json_file`].
#[cfg(test)]
#[derive(Debug, thiserror::Error)]
pub enum LegacyJsonLoadError {
    /// Reading the sidecar file from disk failed.
    #[error("failed to read {}: {source}", path.display())]
    Read {
        /// Sidecar file path that could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Parsing the sidecar JSON bytes failed.
    #[error("failed to parse {}: {source}", path.display())]
    Parse {
        /// Sidecar file path whose JSON could not be parsed.
        path: PathBuf,
        /// Underlying JSON parse error.
        #[source]
        source: serde_json::Error,
    },
}

/// Resolve the stable identity for one saved document path.
///
/// # Errors
///
/// Returns an error if the path cannot be canonicalized.
pub fn resolve_document_identity(path: &Path) -> Result<DocumentSidecarIdentity, NoteStorageError> {
    let display_path = path.to_path_buf();
    let canonical_path =
        fs_metadata::canonical_path(path).map_err(|source| NoteStorageError::Canonicalize {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(DocumentSidecarIdentity::from_paths(
        display_path,
        canonical_path,
    ))
}

/// Convert one sidecar identifier into the on-disk JSON filename.
#[must_use]
pub fn sidecar_filename(sidecar_id: &str) -> String {
    format!("{sidecar_id}.json")
}

/// Canonicalize workspace folders before scope matching.
#[must_use]
pub fn canonicalize_folders(workspace_folders: &[PathBuf]) -> Vec<PathBuf> {
    workspace_folders
        .iter()
        .map(|folder| fs_metadata::canonical_path(folder).unwrap_or_else(|_| folder.clone()))
        .collect()
}

/// Return whether a saved-document identity lives under any selected workspace folder.
#[must_use]
pub fn matches_any_folder(
    identity: &DocumentSidecarIdentity,
    workspace_folders: &[PathBuf],
) -> bool {
    workspace_folders.iter().any(|folder| {
        identity.canonical_path.starts_with(folder) || identity.display_path.starts_with(folder)
    })
}

/// Rebase one saved-document identity after an in-app rename of a file or directory.
#[must_use]
pub fn rebase_identity_paths(
    identity: &DocumentSidecarIdentity,
    old_path: &Path,
    new_path: &Path,
) -> Option<(PathBuf, PathBuf)> {
    rebase_display_and_canonical_paths(
        &identity.display_path,
        &identity.canonical_path,
        old_path,
        new_path,
    )
}

/// Rebase paired display/canonical paths after an in-app rename.
#[must_use]
pub fn rebase_display_and_canonical_paths(
    display_path: &Path,
    canonical_path: &Path,
    old_path: &Path,
    new_path: &Path,
) -> Option<(PathBuf, PathBuf)> {
    if display_path == old_path || display_path.starts_with(old_path) {
        let suffix = display_path
            .strip_prefix(old_path)
            .ok()
            .map_or_default(PathBuf::from);
        let display_path = if suffix.as_os_str().is_empty() {
            new_path.to_path_buf()
        } else {
            new_path.join(suffix)
        };
        let canonical_path =
            fs_metadata::canonical_path(&display_path).unwrap_or_else(|_| display_path.clone());
        return Some((display_path, canonical_path));
    }

    if canonical_path == old_path || canonical_path.starts_with(old_path) {
        let suffix = canonical_path
            .strip_prefix(old_path)
            .ok()
            .map_or_default(PathBuf::from);
        let display_path = if suffix.as_os_str().is_empty() {
            new_path.to_path_buf()
        } else {
            new_path.join(suffix)
        };
        let canonical_path =
            fs_metadata::canonical_path(&display_path).unwrap_or_else(|_| display_path.clone());
        return Some((display_path, canonical_path));
    }

    None
}

/// Per-item outcome tally for a bulk sidecar rename-migration loop.
///
/// Bookmark, document-note, and folder-note migration all walk their sidecar
/// directory and rehome one sidecar per iteration. A `?` on any per-item step
/// aborted the whole loop, and `scan_directory` sorts by filename, so every retry
/// stopped at the same poisoned sidecar until `MAX_MIGRATION_ATTEMPTS` gave up —
/// permanently abandoning every *unrelated* sidecar sorted after it. Recording
/// each failure and continuing lets retries make monotonic progress, while
/// [`SidecarMigrationTally::finish`] still reports the kind as incomplete so the
/// migration ledger keeps it pending.
///
/// The three services share this type so the per-item isolation contract, the
/// skip warning, and the aggregate message cannot drift apart.
pub struct SidecarMigrationTally {
    /// Sidecar kind named in the warning and aggregate messages, for example
    /// `"document-note"`.
    kind_label: &'static str,
    /// Sidecars successfully rehomed so far.
    migrated: usize,
    /// Sidecars this pass refused, each already reported as a warning.
    failures: usize,
}

impl SidecarMigrationTally {
    /// Start an empty tally for one sidecar kind.
    #[must_use]
    pub const fn new(kind_label: &'static str) -> Self {
        Self {
            kind_label,
            migrated: 0,
            failures: 0,
        }
    }

    /// Record one sidecar's outcome, logging a skip warning on failure.
    pub fn record<E: std::fmt::Display>(&mut self, sidecar_path: &Path, outcome: Result<(), E>) {
        match outcome {
            Ok(()) => self.migrated += 1,
            Err(error) => {
                tracing::warn!(
                    "Skipping {} sidecar {} during migration: {error}",
                    self.kind_label,
                    sidecar_path.display()
                );
                self.failures += 1;
            }
        }
    }

    /// Return the migrated count, or an aggregate error when any sidecar failed.
    ///
    /// # Errors
    ///
    /// Returns an error naming how many sidecars were refused and how many
    /// succeeded, so the caller's migration kind stays pending for a later retry.
    pub fn finish(self) -> Result<usize, NoteStorageError> {
        if self.failures > 0 {
            return Err(NoteStorageError::IncompleteMigration {
                kind_label: self.kind_label,
                failures: self.failures,
                migrated: self.migrated,
            });
        }
        Ok(self.migrated)
    }
}

/// Load one optional JSON sidecar payload for legacy strict-load tests.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
#[cfg(test)]
pub fn load_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, LegacyJsonLoadError> {
    match fs_read::bytes(path) {
        Ok(bytes) => {
            let value =
                serde_json::from_slice(&bytes).map_err(|source| LegacyJsonLoadError::Parse {
                    path: path.to_path_buf(),
                    source,
                })?;
            Ok(Some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(LegacyJsonLoadError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Load one optional JSON sidecar through recovery-aware metadata handling.
#[must_use]
pub fn load_json_file_recovering<T: DeserializeOwned>(
    data_dir: &Path,
    path: &Path,
    class: RecoveryMetadataClass,
) -> RecoveryLoad<Option<T>> {
    load_json_file_recovering_with_max_bytes(
        data_dir,
        path,
        class,
        crate::persistence::recovery_metadata::DEFAULT_MAX_METADATA_BYTES,
    )
}

/// Load one sidecar under a caller-calibrated pre-admitted input ceiling.
pub(crate) fn load_json_file_recovering_with_max_bytes<T: DeserializeOwned>(
    data_dir: &Path,
    path: &Path,
    class: RecoveryMetadataClass,
    max_bytes: u64,
) -> RecoveryLoad<Option<T>> {
    load_enveloped_json_optional_accepting(
        &RecoveryLoadConfig::new(data_dir, path, class).with_max_bytes(max_bytes),
        sidecar_document_kind(class),
        legacy_sidecar_document_kinds(class),
    )
}

/// Save one sidecar payload as a v1 envelope after preservation checks.
///
/// # Errors
///
/// Returns an error if the current sidecar is unsafe to replace or the durable
/// v1 write fails.
pub fn save_json_file_recovering<T>(
    data_dir: &Path,
    path: &Path,
    class: RecoveryMetadataClass,
    value: &T,
) -> Result<(), NoteStorageError>
where
    T: Serialize + DeserializeOwned,
{
    let config = RecoveryLoadConfig::new(data_dir, path, class);
    let diagnostics = save_enveloped_json_path_accepting(
        &config,
        sidecar_document_kind(class),
        legacy_sidecar_document_kinds(class),
        value,
    )?;
    trace_recovery_diagnostics(&diagnostics);
    Ok(())
}

/// Send recovery diagnostics to tracing without changing a service's public API.
pub fn trace_recovery_diagnostics(diagnostics: &[RecoveryDiagnostic]) {
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
}

fn sidecar_document_kind(class: RecoveryMetadataClass) -> &'static str {
    match class {
        RecoveryMetadataClass::BookmarkSidecar => KIND_BOOKMARK_SIDECAR,
        RecoveryMetadataClass::DocumentNoteSidecar => KIND_DOCUMENT_NOTE_SIDECAR,
        RecoveryMetadataClass::FolderNoteSidecar => KIND_FOLDER_NOTE_SIDECAR,
        RecoveryMetadataClass::LocalHistoryIndex => KIND_LOCAL_HISTORY_INDEX,
        other => panic!(
            "recovery class {} does not map to a note-storage sidecar kind",
            other.slug()
        ),
    }
}

fn legacy_sidecar_document_kinds(class: RecoveryMetadataClass) -> &'static [&'static str] {
    match class {
        RecoveryMetadataClass::FolderNoteSidecar => LEGACY_FOLDER_NOTE_SIDECAR_KINDS,
        RecoveryMetadataClass::BookmarkSidecar
        | RecoveryMetadataClass::DocumentNoteSidecar
        | RecoveryMetadataClass::LocalHistoryIndex => NO_LEGACY_SIDECAR_KINDS,
        other => panic!(
            "recovery class {} does not map to note-storage legacy kinds",
            other.slug()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::fixture;
    use tempfile::TempDir;

    #[test]
    fn canonicalize_folders_resolves_existing_folders_and_keeps_missing_folders() {
        let dir = TempDir::new().expect("tempdir");
        let existing = dir.path().join("workspace");
        let missing = dir.path().join("missing");
        fixture::create_dir_all(&existing);

        let folders = canonicalize_folders(&[existing.clone(), missing.clone()]);

        assert_eq!(
            folders[0],
            fs_metadata::canonical_path(&existing).expect("canonical workspace")
        );
        assert_eq!(folders[1], missing);
    }

    #[test]
    fn matches_any_folder_accepts_display_or_canonical_folders_only() {
        let folder = PathBuf::from("/workspace");
        let canonical_match = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/visible/file.rs"),
            PathBuf::from("/workspace/file.rs"),
        );
        let display_match = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/visible.rs"),
            PathBuf::from("/canonical/file.rs"),
        );
        let outside = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/outside/visible.rs"),
            PathBuf::from("/canonical/file.rs"),
        );

        assert!(matches_any_folder(
            &canonical_match,
            std::slice::from_ref(&folder)
        ));
        assert!(matches_any_folder(
            &display_match,
            std::slice::from_ref(&folder)
        ));
        assert!(!matches_any_folder(&outside, &[folder]));
    }

    #[test]
    fn rebase_identity_paths_handles_display_and_canonical_prefixes() {
        let old_folder = Path::new("/project/old");
        let new_folder = Path::new("/project/new");
        let display_nested = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/project/old/src/file.txt"),
            PathBuf::from("/canonical/elsewhere/file.txt"),
        );
        let canonical_nested = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/visible/elsewhere/file.txt"),
            PathBuf::from("/project/old/src/file.txt"),
        );
        let unrelated = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/project/other/file.txt"),
            PathBuf::from("/canonical/other/file.txt"),
        );

        let (display_path, canonical_path) =
            rebase_identity_paths(&display_nested, old_folder, new_folder)
                .expect("display path should rebase");
        assert_eq!(display_path, PathBuf::from("/project/new/src/file.txt"));
        assert_eq!(canonical_path, PathBuf::from("/project/new/src/file.txt"));

        let (display_path, canonical_path) =
            rebase_identity_paths(&canonical_nested, old_folder, new_folder)
                .expect("canonical path should rebase");
        assert_eq!(display_path, PathBuf::from("/project/new/src/file.txt"));
        assert_eq!(canonical_path, PathBuf::from("/project/new/src/file.txt"));

        assert!(rebase_identity_paths(&unrelated, old_folder, new_folder).is_none());
    }

    #[test]
    fn load_json_file_distinguishes_missing_from_other_read_and_parse_errors() {
        let dir = TempDir::new().expect("tempdir");
        let missing_path = dir.path().join("missing.json");
        let corrupt_path = dir.path().join("corrupt.json");
        fixture::write_text(&corrupt_path, "{not valid json");

        assert!(
            load_json_file::<serde_json::Value>(&missing_path)
                .expect("missing file should be accepted")
                .is_none()
        );
        let read_error =
            load_json_file::<serde_json::Value>(dir.path()).expect_err("directory read");
        assert!(matches!(
            read_error,
            LegacyJsonLoadError::Read { ref path, .. } if path == dir.path()
        ));

        let parse_error =
            load_json_file::<serde_json::Value>(&corrupt_path).expect_err("corrupt json");
        assert!(matches!(
            parse_error,
            LegacyJsonLoadError::Parse { ref path, .. } if path == &corrupt_path
        ));
    }

    #[test]
    fn resolve_document_identity_reports_canonicalize_error_for_missing_path() {
        let dir = TempDir::new().expect("tempdir");
        let missing = dir.path().join("missing.rs");

        let error = resolve_document_identity(&missing).expect_err("missing path should fail");

        assert!(matches!(
            error,
            NoteStorageError::Canonicalize { ref path, .. } if path == &missing
        ));
    }

    #[test]
    fn sidecar_migration_tally_reports_incomplete_migration_counts() {
        let mut tally = SidecarMigrationTally::new("bookmark");
        tally.record::<NoteStorageError>(Path::new("/tmp/ok.json"), Ok(()));
        tally.record(
            Path::new("/tmp/bad.json"),
            Err(NoteStorageError::Canonicalize {
                path: PathBuf::from("/tmp/bad.json"),
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            }),
        );

        let error = tally
            .finish()
            .expect_err("failed item must report incomplete");

        assert!(matches!(
            error,
            NoteStorageError::IncompleteMigration {
                kind_label: "bookmark",
                failures: 1,
                migrated: 1,
            }
        ));
    }
}

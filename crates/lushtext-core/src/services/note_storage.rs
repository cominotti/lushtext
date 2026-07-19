// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared sidecar persistence helpers for note-like workflows.
//!
//! Bookmarks and the richer document/folder note flows all use
//! the same patterns: canonical-path identity resolution, JSON sidecar loading,
//! workspace-folder filtering, and in-app rename migration. Keeping those helpers
//! here avoids subtle drift between the different persistence services.

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

use crate::model::sidecar_identity::DocumentSidecarIdentity;
use crate::services::filesystem::metadata as fs_metadata;
#[cfg(test)]
use crate::services::filesystem::read as fs_read;
use crate::services::json_format::{
    KIND_BOOKMARK_SIDECAR, KIND_DOCUMENT_NOTE_SIDECAR, KIND_FOLDER_NOTE_SIDECAR,
    KIND_LEGACY_WORKSPACE_NOTE_SIDECAR, KIND_LOCAL_HISTORY_INDEX,
};
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass,
    load_enveloped_json_optional_accepting, save_enveloped_json_path_accepting,
};

const NO_LEGACY_SIDECAR_KINDS: &[&str] = &[];
const LEGACY_FOLDER_NOTE_SIDECAR_KINDS: &[&str] = &[KIND_LEGACY_WORKSPACE_NOTE_SIDECAR];

/// Resolve the stable identity for one saved document path.
///
/// # Errors
///
/// Returns an error if the path cannot be canonicalized.
pub fn resolve_document_identity(path: &Path) -> Result<DocumentSidecarIdentity> {
    let display_path = path.to_path_buf();
    let canonical_path = fs_metadata::canonical_path(path)
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
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
            .map(PathBuf::from)
            .unwrap_or_default();
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
            .map(PathBuf::from)
            .unwrap_or_default();
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

/// Load one optional JSON sidecar payload for legacy strict-load tests.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
#[cfg(test)]
pub fn load_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs_read::bytes(path) {
        Ok(bytes) => {
            let value = serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            Ok(Some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(anyhow::anyhow!(
            "failed to read {}: {}",
            path.display(),
            error
        )),
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
        crate::services::recovery_metadata::DEFAULT_MAX_METADATA_BYTES,
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
) -> Result<()>
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
    use crate::services::filesystem::fixture;
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
    fn load_json_file_distinguishes_missing_from_other_read_errors() {
        let dir = TempDir::new().expect("tempdir");
        let missing_path = dir.path().join("missing.json");

        assert!(
            load_json_file::<serde_json::Value>(&missing_path)
                .expect("missing file should be accepted")
                .is_none()
        );
        let error = load_json_file::<serde_json::Value>(dir.path()).expect_err("directory read");
        assert!(
            error.to_string().contains("failed to read"),
            "unexpected error: {error}"
        );
    }
}

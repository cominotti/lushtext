// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared sidecar persistence helpers for note-like workflows.
//!
//! Bookmarks and the richer document/workspace note flows all use
//! the same patterns: canonical-path identity resolution, JSON sidecar loading,
//! workspace-root filtering, and in-app rename migration. Keeping those helpers
//! here avoids subtle drift between the different persistence services.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

use crate::model::sidecar_identity::DocumentSidecarIdentity;
use crate::services::filesystem::metadata as fs_metadata;
#[cfg(test)]
use crate::services::filesystem::read as fs_read;
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass, load_json_optional,
};

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

/// Canonicalize workspace roots before scope matching.
#[must_use]
pub fn canonicalize_roots(workspace_roots: &[PathBuf]) -> Vec<PathBuf> {
    workspace_roots
        .iter()
        .map(|root| fs_metadata::canonical_path(root).unwrap_or_else(|_| root.clone()))
        .collect()
}

/// Return whether a saved-document identity lives under any selected workspace root.
#[must_use]
pub fn matches_any_root(identity: &DocumentSidecarIdentity, workspace_roots: &[PathBuf]) -> bool {
    workspace_roots.iter().any(|root| {
        identity.canonical_path.starts_with(root) || identity.display_path.starts_with(root)
    })
}

/// Rebase one saved-document identity after an in-app rename of a file or directory.
#[must_use]
pub fn rebase_identity_paths(
    identity: &DocumentSidecarIdentity,
    old_path: &Path,
    new_path: &Path,
) -> Option<(PathBuf, PathBuf)> {
    if identity.display_path == old_path || identity.display_path.starts_with(old_path) {
        let suffix = identity
            .display_path
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

    if identity.canonical_path == old_path || identity.canonical_path.starts_with(old_path) {
        let suffix = identity
            .canonical_path
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
    load_json_optional(&RecoveryLoadConfig::new(data_dir, path, class))
}

/// Send recovery diagnostics to tracing without changing a service's public API.
pub fn trace_recovery_diagnostics(diagnostics: &[RecoveryDiagnostic]) {
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    #[test]
    fn canonicalize_roots_resolves_existing_roots_and_keeps_missing_roots() {
        let dir = TempDir::new().expect("tempdir");
        let existing = dir.path().join("workspace");
        let missing = dir.path().join("missing");
        fixture::create_dir_all(&existing);

        let roots = canonicalize_roots(&[existing.clone(), missing.clone()]);

        assert_eq!(
            roots[0],
            fs_metadata::canonical_path(&existing).expect("canonical workspace")
        );
        assert_eq!(roots[1], missing);
    }

    #[test]
    fn matches_any_root_accepts_display_or_canonical_roots_only() {
        let root = PathBuf::from("/workspace");
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

        assert!(matches_any_root(
            &canonical_match,
            std::slice::from_ref(&root)
        ));
        assert!(matches_any_root(
            &display_match,
            std::slice::from_ref(&root)
        ));
        assert!(!matches_any_root(&outside, &[root]));
    }

    #[test]
    fn rebase_identity_paths_handles_display_and_canonical_prefixes() {
        let old_root = Path::new("/project/old");
        let new_root = Path::new("/project/new");
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
            rebase_identity_paths(&display_nested, old_root, new_root)
                .expect("display path should rebase");
        assert_eq!(display_path, PathBuf::from("/project/new/src/file.txt"));
        assert_eq!(canonical_path, PathBuf::from("/project/new/src/file.txt"));

        let (display_path, canonical_path) =
            rebase_identity_paths(&canonical_nested, old_root, new_root)
                .expect("canonical path should rebase");
        assert_eq!(display_path, PathBuf::from("/project/new/src/file.txt"));
        assert_eq!(canonical_path, PathBuf::from("/project/new/src/file.txt"));

        assert!(rebase_identity_paths(&unrelated, old_root, new_root).is_none());
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

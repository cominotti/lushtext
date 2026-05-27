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

/// Resolve the stable identity for one saved document path.
///
/// # Errors
///
/// Returns an error if the path cannot be canonicalized.
pub fn resolve_document_identity(path: &Path) -> Result<DocumentSidecarIdentity> {
    let display_path = path.to_path_buf();
    let canonical_path = path
        .canonicalize()
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
        .map(|root| root.canonicalize().unwrap_or_else(|_| root.clone()))
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
        let canonical_path = display_path
            .canonicalize()
            .unwrap_or_else(|_| display_path.clone());
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
        let canonical_path = display_path
            .canonicalize()
            .unwrap_or_else(|_| display_path.clone());
        return Some((display_path, canonical_path));
    }

    None
}

/// Load one optional JSON sidecar payload.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
pub fn load_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
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

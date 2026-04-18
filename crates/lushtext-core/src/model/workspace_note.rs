// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-root note model types.
//!
//! Workspace notes belong to one workspace root rather than to a single file.
//! They persist under app data so project trees stay untouched, but their
//! identity still follows the canonical workspace root path.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::note::RichNoteBody;
use super::sidecar_identity::stable_path_hash;

/// Stable identity for one workspace-root note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceRootIdentity {
    /// User-visible root path shown back in browse surfaces.
    pub display_root: PathBuf,
    /// Canonical root path used for identity and rename migration.
    pub canonical_root: PathBuf,
    /// Deterministic hash of the canonical root path used for the sidecar file.
    pub sidecar_id: String,
}

impl WorkspaceRootIdentity {
    /// Build a stable identity from a displayed root and its canonical path.
    #[must_use]
    pub fn from_roots(display_root: PathBuf, canonical_root: PathBuf) -> Self {
        Self {
            sidecar_id: stable_path_hash(&canonical_root),
            display_root,
            canonical_root,
        }
    }
}

/// Persisted workspace note for one workspace root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceNoteDocument {
    /// Stable workspace-root identity backing this note.
    pub identity: WorkspaceRootIdentity,
    /// Rich note body stored for the workspace as a whole.
    pub note: RichNoteBody,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_uses_canonical_root_hash() {
        let identity = WorkspaceRootIdentity::from_roots(
            PathBuf::from("/tmp/link"),
            PathBuf::from("/tmp/real"),
        );

        assert_eq!(
            identity.sidecar_id,
            stable_path_hash(&PathBuf::from("/tmp/real"))
        );
    }
}

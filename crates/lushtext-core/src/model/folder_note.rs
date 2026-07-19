// SPDX-License-Identifier: GPL-3.0-or-later

//! Folder-note model types.
//!
//! Folder notes belong to one configured workspace folder rather than to a single file.
//! They persist under app data so project trees stay untouched, but their
//! identity still follows the canonical workspace folder path.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::note::RichNoteBody;
use super::sidecar_identity::stable_path_hash;

/// Stable identity for one folder note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderNoteIdentity {
    /// User-visible folder path shown back in browse surfaces.
    // Legacy sidecars stored this field as `display_root`; keep the alias local
    // to compatibility parsing so the live model speaks in folder terms.
    #[serde(alias = "display_root")]
    pub display_folder: PathBuf,
    /// Canonical folder path used for identity and rename migration.
    // Legacy sidecars stored this field as `canonical_root`; new writes use
    // `canonical_folder` through the normal struct field name.
    #[serde(alias = "canonical_root")]
    pub canonical_folder: PathBuf,
    /// Deterministic hash of the canonical folder path used for the sidecar file.
    pub sidecar_id: String,
}

impl FolderNoteIdentity {
    /// Build a stable identity from a displayed folder and its canonical path.
    #[must_use]
    pub fn from_folders(display_folder: PathBuf, canonical_folder: PathBuf) -> Self {
        Self {
            sidecar_id: stable_path_hash(&canonical_folder),
            display_folder,
            canonical_folder,
        }
    }
}

/// Persisted folder note for one workspace folder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderNoteDocument {
    /// Stable folder identity backing this note.
    pub identity: FolderNoteIdentity,
    /// Rich note body stored for one configured workspace folder.
    pub note: RichNoteBody,
}

impl FolderNoteDocument {
    /// Return the complete retained heap graph used during bounded source construction.
    #[must_use]
    pub fn retained_heap_byte_weight(&self) -> u64 {
        u64::try_from(
            self.identity
                .display_folder
                .capacity()
                .saturating_add(self.identity.canonical_folder.capacity())
                .saturating_add(self.identity.sidecar_id.capacity()),
        )
        .unwrap_or(u64::MAX)
        .saturating_add(self.note.retained_heap_byte_weight())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_uses_canonical_folder_hash() {
        let identity = FolderNoteIdentity::from_folders(
            PathBuf::from("/tmp/link"),
            PathBuf::from("/tmp/real"),
        );

        assert_eq!(
            identity.sidecar_id,
            stable_path_hash(&PathBuf::from("/tmp/real"))
        );
    }
}

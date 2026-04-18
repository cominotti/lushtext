// SPDX-License-Identifier: GPL-3.0-or-later

//! File-backed document-note model types.
//!
//! A document note is the single rich note attached to one saved file as a
//! whole. Persistence stays sidecar-based and keyed by the same stable
//! canonical-path identity used by other saved-file note workflows.

use serde::{Deserialize, Serialize};

use super::note::RichNoteBody;
use super::sidecar_identity::DocumentSidecarIdentity;

/// Persisted document note for one saved file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentNoteDocument {
    /// Stable saved-file identity backing this note.
    pub identity: DocumentSidecarIdentity,
    /// Rich note body stored for the whole file.
    pub note: RichNoteBody,
}

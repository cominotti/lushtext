// SPDX-License-Identifier: GPL-3.0-or-later

//! Application services: business logic and I/O operations.
//!
//! This layer sits between the domain model (`model/`) and the UI (`ui/`).
//! All services are GTK-free and fully unit-testable. Includes workspace
//! management, session persistence, file tree scanning, editor file I/O,
//! file size policy, bounded file peek and bookmark excerpt snapshots, fuzzy
//! search, and the background task concurrency guard.

pub mod async_task;
pub mod bookmark_excerpt;
pub mod bookmark_service;
pub mod content_search;
pub mod document_note_service;
pub mod draft_service;
mod durable_write;
pub mod editor_io;
pub mod editorconfig;
pub mod file_limits;
pub mod file_peek;
pub mod file_tree;
pub mod filesystem;
pub mod folder_note_service;
pub mod json_format;
pub mod json_store;
pub mod local_history_service;
pub mod migration_ledger;
mod note_storage;
pub mod notifications;
pub mod palette;
pub mod recovery_metadata;
pub mod saved_searches;
pub mod search_backup;
pub mod search_history;
pub mod session_service;
pub mod workspace_manager;
pub mod workspace_watch;

/// Feature-gated pure service hooks used only by the property-test target.
#[cfg(feature = "property-tests")]
pub mod property_testing {
    use std::path::{Path, PathBuf};

    use crate::model::sidecar_identity::DocumentSidecarIdentity;

    /// Rebase a saved-document sidecar identity through the shared note helper.
    ///
    /// Property tests use this to prove the same path-prefix policy that
    /// document-note sidecars rely on during rename flows.
    #[must_use]
    pub fn rebase_document_identity_paths(
        identity: &DocumentSidecarIdentity,
        old_path: &Path,
        new_path: &Path,
    ) -> Option<(PathBuf, PathBuf)> {
        super::note_storage::rebase_identity_paths(identity, old_path, new_path)
    }
}

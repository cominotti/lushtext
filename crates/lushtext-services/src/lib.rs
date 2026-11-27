// SPDX-License-Identifier: GPL-3.0-or-later

//! Application services: business logic and driven I/O adapters.
//!
//! This layer sits between the domain model (`lushtext-model`) and the UI (`lushtext-ui`).
//! All services are GTK-free and fully unit-testable, organized into driven
//! adapters (`filesystem/`, `persistence/`, `watch/`) and application services
//! (`notes/`, `search/`, `workspace/`, `editor_io/`, `palette/`,
//! `coordination/`, `instrumentation/`).

// Driven adapters.
pub mod filesystem;
pub mod persistence;
pub mod watch;

// Application services.
pub mod coordination;
pub mod editor_io;
pub mod instrumentation;
pub mod notes;
pub mod palette;
pub mod search;
pub mod workspace;

/// Feature-gated pure service hooks used only by the property-test target.
#[cfg(feature = "property-tests")]
pub mod property_testing {
    use std::path::{Path, PathBuf};

    use lushtext_model::persistence::sidecar_identity::DocumentSidecarIdentity;

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
        super::notes::note_storage::rebase_identity_paths(identity, old_path, new_path)
    }
}

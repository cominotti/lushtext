// SPDX-License-Identifier: GPL-3.0-or-later

//! Document-note persistence and workspace listing helpers.
//!
//! This service owns the filesystem-facing document-note workflow: resolve a
//! stable saved-file identity, load/save one note for that file, migrate notes
//! after in-app renames, and collect workspace-scoped rows for note browsers.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::model::document_note::DocumentNoteDocument;
use crate::model::note::RichNoteBody;
use crate::model::sidecar_identity::DocumentSidecarIdentity;
use crate::services::filesystem::{
    DirectoryScanPolicy, metadata as fs_metadata, mutate as fs_mutate, tree as fs_tree,
};
use crate::services::json_store;

use super::note_storage;

/// Directory name that stores per-file document notes.
const DOCUMENT_NOTES_DIR: &str = "document-notes";

/// Lightweight workspace-facing document-note row for note browsers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDocumentNote {
    /// Path of the saved file that owns the document note.
    pub path: PathBuf,
    /// Stored rich note body.
    pub note: RichNoteBody,
}

/// Resolve the document-note sidecar directory under the app data home.
#[must_use]
pub fn document_notes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DOCUMENT_NOTES_DIR)
}

/// Load the document note for a saved file, returning `None` when no note exists yet.
///
/// # Errors
///
/// Returns an error if the file identity cannot be resolved, the sidecar cannot
/// be read, or the stored JSON cannot be parsed.
pub fn load_for_path(data_dir: &Path, path: &Path) -> Result<Option<DocumentNoteDocument>> {
    let identity = note_storage::resolve_document_identity(path)?;
    load_for_identity(data_dir, &identity)
}

fn load_for_identity(
    data_dir: &Path,
    identity: &DocumentSidecarIdentity,
) -> Result<Option<DocumentNoteDocument>> {
    let path =
        document_notes_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id));
    note_storage::load_json_file::<DocumentNoteDocument>(&path)
}

/// Save the current document note for a saved file.
///
/// Empty note bodies delete the sidecar file instead of persisting an empty payload.
///
/// # Errors
///
/// Returns an error if the file identity cannot be resolved or the sidecar
/// cannot be written or deleted.
pub fn save_for_path(
    data_dir: &Path,
    path: &Path,
    note: &RichNoteBody,
) -> Result<DocumentSidecarIdentity> {
    let identity = note_storage::resolve_document_identity(path)?;
    save_document(
        data_dir,
        &DocumentNoteDocument {
            identity: identity.clone(),
            note: note.clone(),
        },
    )?;
    Ok(identity)
}

/// Save a fully shaped document-note sidecar.
///
/// # Errors
///
/// Returns an error if the sidecar cannot be written or deleted.
pub fn save_document(data_dir: &Path, document: &DocumentNoteDocument) -> Result<()> {
    if document.note.is_empty() {
        return delete_sidecar_file(data_dir, &document.identity);
    }

    json_store::save(
        &document_notes_dir(data_dir),
        &note_storage::sidecar_filename(&document.identity.sidecar_id),
        document,
    )
}

/// Delete the document note for a saved file path if it exists.
///
/// # Errors
///
/// Returns an error if the file identity cannot be resolved or an existing
/// sidecar cannot be deleted.
pub fn delete_for_path(data_dir: &Path, path: &Path) -> Result<()> {
    let identity = note_storage::resolve_document_identity(path)?;
    delete_sidecar_file(data_dir, &identity)
}

fn delete_sidecar_file(data_dir: &Path, identity: &DocumentSidecarIdentity) -> Result<()> {
    let path =
        document_notes_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id));
    match fs_mutate::remove_file_if_exists(&path) {
        Ok(_) => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to delete document note sidecar {}: {}",
            path.display(),
            error
        )),
    }
}

/// Move document-note sidecars after an in-app rename of a file or directory tree.
///
/// Returns the number of sidecars that were rewritten.
///
/// # Errors
///
/// Returns an error if the sidecar directory cannot be scanned or a migrated
/// sidecar cannot be read, rewritten, or cleaned up.
pub fn move_path_tree(data_dir: &Path, old_path: &Path, new_path: &Path) -> Result<usize> {
    let dir = document_notes_dir(data_dir);
    if fs_metadata::file_facts(&dir).is_err() {
        return Ok(0);
    }

    let mut migrated = 0;
    for entry in fs_tree::scan_directory(&dir, DirectoryScanPolicy::visible_workspace())
        .with_context(|| format!("failed to read {}", dir.display()))?
    {
        let sidecar_path = entry.path;
        if sidecar_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let Some(mut document) =
            note_storage::load_json_file::<DocumentNoteDocument>(&sidecar_path)?
        else {
            continue;
        };
        let Some((display_path, canonical_path)) =
            note_storage::rebase_identity_paths(&document.identity, old_path, new_path)
        else {
            continue;
        };

        document.identity = DocumentSidecarIdentity::from_paths(display_path, canonical_path);
        let new_sidecar_path = dir.join(note_storage::sidecar_filename(
            &document.identity.sidecar_id,
        ));
        save_document(data_dir, &document)?;
        if sidecar_path != new_sidecar_path {
            let _ = fs_mutate::remove_file_if_exists(&sidecar_path);
        }
        migrated += 1;
    }

    Ok(migrated)
}

/// Collect document notes under the current workspace roots for note browsers.
///
/// # Errors
///
/// Returns an error if the sidecar directory cannot be scanned or one document
/// note cannot be read or parsed.
pub fn list_workspace_document_notes(
    data_dir: &Path,
    workspace_roots: &[PathBuf],
) -> Result<Vec<WorkspaceDocumentNote>> {
    let canonical_roots = note_storage::canonicalize_roots(workspace_roots);
    let dir = document_notes_dir(data_dir);
    if fs_metadata::file_facts(&dir).is_err() {
        return Ok(Vec::new());
    }

    let mut notes = Vec::new();
    for entry in fs_tree::scan_directory(&dir, DirectoryScanPolicy::visible_workspace())
        .with_context(|| format!("failed to read {}", dir.display()))?
    {
        let Some(document) = note_storage::load_json_file::<DocumentNoteDocument>(&entry.path)?
        else {
            continue;
        };
        if !note_storage::matches_any_root(&document.identity, &canonical_roots) {
            continue;
        }
        notes.push(WorkspaceDocumentNote {
            path: document.identity.display_path,
            note: document.note,
        });
    }

    notes.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fixture::create_dir_all(parent);
        }
        fixture::write_text(path, contents);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");

        let note = RichNoteBody::new("# Summary\n\nKeep this in mind");
        save_for_path(dir.path(), &file_path, &note).expect("expected operation to succeed");

        let loaded = load_for_path(dir.path(), &file_path).expect("expected operation to succeed");
        assert!(loaded.is_some());
        let loaded = loaded.expect("expected document note");
        assert_eq!(loaded.identity.display_path, file_path);
        assert_eq!(loaded.note.text, "# Summary\n\nKeep this in mind");
    }

    #[test]
    fn empty_save_deletes_sidecar() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");

        let identity = save_for_path(dir.path(), &file_path, &RichNoteBody::new("Keep"))
            .expect("expected operation to succeed");
        save_for_path(dir.path(), &file_path, &RichNoteBody::new("   "))
            .expect("expected operation to succeed");

        let sidecar_path = document_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&identity.sidecar_id));
        assert!(fs_metadata::file_facts(&sidecar_path).is_err());
    }

    #[test]
    fn delete_for_path_removes_existing_sidecar_and_ignores_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");

        let identity = save_for_path(dir.path(), &file_path, &RichNoteBody::new("Keep"))
            .expect("expected operation to succeed");
        let sidecar_path = document_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&identity.sidecar_id));

        delete_for_path(dir.path(), &file_path).expect("expected operation to succeed");
        assert!(fs_metadata::file_facts(&sidecar_path).is_err());
        delete_for_path(dir.path(), &file_path).expect("expected missing sidecar to be a no-op");
    }

    #[test]
    fn delete_for_path_reports_non_file_sidecar_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");

        let identity = save_for_path(dir.path(), &file_path, &RichNoteBody::new("Keep"))
            .expect("expected operation to succeed");
        let sidecar_path = document_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&identity.sidecar_id));
        fixture::remove_file(&sidecar_path);
        fixture::create_dir(&sidecar_path);

        let error =
            delete_for_path(dir.path(), &file_path).expect_err("directory sidecar should fail");
        assert!(
            error
                .to_string()
                .contains("failed to delete document note sidecar"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn move_path_tree_rewrites_document_identity() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_file = dir.path().join("workspace/old.rs");
        let new_file = dir.path().join("workspace/new.rs");
        write_file(&old_file, "fn old() {}\n");

        let old_identity =
            save_for_path(dir.path(), &old_file, &RichNoteBody::new("Keep this note"))
                .expect("expected operation to succeed");
        let old_sidecar_path = document_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&old_identity.sidecar_id));

        fixture::rename(&old_file, &new_file);
        let migrated = move_path_tree(dir.path(), &old_file, &new_file)
            .expect("expected operation to succeed");

        assert_eq!(migrated, 1);
        assert!(fs_metadata::file_facts(&old_sidecar_path).is_err());
        let loaded = load_for_path(dir.path(), &new_file).expect("expected operation to succeed");
        let loaded = loaded.expect("expected moved note");
        assert_eq!(loaded.identity.display_path, new_file);
        assert_eq!(loaded.note.text, "Keep this note");
        let json_sidecars = fs_tree::scan_directory(
            &document_notes_dir(dir.path()),
            DirectoryScanPolicy::visible_workspace(),
        )
        .expect("expected operation to succeed")
        .into_iter()
        .filter(|entry| entry.path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .count();
        assert_eq!(json_sidecars, 1);
    }

    #[test]
    fn list_workspace_document_notes_filters_roots_and_sorts_rows() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let workspace = dir.path().join("workspace");
        let alpha = workspace.join("alpha.rs");
        let beta = workspace.join("nested/beta.rs");
        let outside = dir.path().join("outside/gamma.rs");
        write_file(&alpha, "alpha");
        write_file(&beta, "beta");
        write_file(&outside, "gamma");

        let beta_note = RichNoteBody::new("Beta note");
        let outside_note = RichNoteBody::new("Outside note");
        let alpha_note = RichNoteBody::new("Alpha note");

        save_for_path(dir.path(), &beta, &beta_note).expect("expected operation to succeed");
        save_for_path(dir.path(), &outside, &outside_note).expect("expected operation to succeed");
        save_for_path(dir.path(), &alpha, &alpha_note).expect("expected operation to succeed");

        let notes = list_workspace_document_notes(dir.path(), &[workspace])
            .expect("expected operation to succeed");

        assert_eq!(
            notes,
            vec![
                WorkspaceDocumentNote {
                    path: alpha,
                    note: alpha_note,
                },
                WorkspaceDocumentNote {
                    path: beta,
                    note: beta_note,
                },
            ]
        );
    }
}

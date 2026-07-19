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
use crate::model::note::RichNoteMergeDecision;
use crate::model::sidecar_identity::DocumentSidecarIdentity;
use crate::services::filesystem::{
    DirectoryScanPolicy, metadata as fs_metadata, mutate as fs_mutate, tree as fs_tree,
};
use crate::services::recovery_metadata::{RecoveryDiagnostic, RecoveryMetadataClass};

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

/// Workspace document-note rows plus diagnostics for skipped sidecars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDocumentNoteListing {
    /// Document-note rows safe to display in note browsers.
    pub notes: Vec<WorkspaceDocumentNote>,
    /// Recovery diagnostics for malformed or unreadable document-note sidecars.
    pub diagnostics: Vec<RecoveryDiagnostic>,
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
/// Returns an error if the file identity cannot be resolved or the sidecar
/// directory cannot be scanned. Malformed or unreadable sidecars are preserved
/// through recovery diagnostics and treated as absent.
pub fn load_for_path(data_dir: &Path, path: &Path) -> Result<Option<DocumentNoteDocument>> {
    load_for_path_with_max_bytes(
        data_dir,
        path,
        crate::services::recovery_metadata::DEFAULT_MAX_METADATA_BYTES,
    )
}

pub(crate) fn load_for_path_with_max_bytes(
    data_dir: &Path,
    path: &Path,
    max_bytes: u64,
) -> Result<Option<DocumentNoteDocument>> {
    let identity = note_storage::resolve_document_identity(path)?;
    load_for_identity(data_dir, &identity, max_bytes)
}

fn load_for_identity(
    data_dir: &Path,
    identity: &DocumentSidecarIdentity,
    max_bytes: u64,
) -> Result<Option<DocumentNoteDocument>> {
    let path =
        document_notes_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id));
    let load = note_storage::load_json_file_recovering_with_max_bytes::<DocumentNoteDocument>(
        data_dir,
        &path,
        RecoveryMetadataClass::DocumentNoteSidecar,
        max_bytes,
    );
    note_storage::trace_recovery_diagnostics(&load.diagnostics);
    Ok(load.value)
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

    let path = document_notes_dir(data_dir).join(note_storage::sidecar_filename(
        &document.identity.sidecar_id,
    ));
    note_storage::save_json_file_recovering(
        data_dir,
        &path,
        RecoveryMetadataClass::DocumentNoteSidecar,
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
    if !fs_metadata::path_status(&dir)?.is_present() {
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

        let load = note_storage::load_json_file_recovering::<DocumentNoteDocument>(
            data_dir,
            &sidecar_path,
            RecoveryMetadataClass::DocumentNoteSidecar,
        );
        note_storage::trace_recovery_diagnostics(&load.diagnostics);
        let Some(document) = load.value else {
            continue;
        };
        let Some((display_path, canonical_path)) =
            note_storage::rebase_identity_paths(&document.identity, old_path, new_path)
        else {
            continue;
        };

        let new_identity = DocumentSidecarIdentity::from_paths(display_path, canonical_path);
        let new_sidecar_path = dir.join(note_storage::sidecar_filename(&new_identity.sidecar_id));
        let document =
            merge_document_note_target(data_dir, &new_sidecar_path, document, new_identity)?;
        save_document(data_dir, &document)?;
        if sidecar_path != new_sidecar_path {
            remove_obsolete_sidecar(&sidecar_path)?;
        }
        migrated += 1;
    }

    Ok(migrated)
}

fn merge_document_note_target(
    data_dir: &Path,
    target_path: &Path,
    mut source: DocumentNoteDocument,
    target_identity: DocumentSidecarIdentity,
) -> Result<DocumentNoteDocument> {
    let load = note_storage::load_json_file_recovering::<DocumentNoteDocument>(
        data_dir,
        target_path,
        RecoveryMetadataClass::DocumentNoteSidecar,
    );
    note_storage::trace_recovery_diagnostics(&load.diagnostics);
    let Some(target) = load.value else {
        source.identity = target_identity;
        return Ok(source);
    };

    merge_document_note_documents(source, target, target_identity)
}

/// Merge a moved note into an existing target note without guessing on ambiguous conflicts.
fn merge_document_note_documents(
    source: DocumentNoteDocument,
    mut target: DocumentNoteDocument,
    target_identity: DocumentSidecarIdentity,
) -> Result<DocumentNoteDocument> {
    match source.note.merge_decision_against(&target.note) {
        RichNoteMergeDecision::UseSelf => Ok(DocumentNoteDocument {
            identity: target_identity,
            note: source.note,
        }),
        RichNoteMergeDecision::UseOther => {
            target.identity = target_identity;
            Ok(target)
        }
        RichNoteMergeDecision::Conflict => Err(anyhow::anyhow!(
            "ambiguous document note sidecar conflict for {}; both copies were preserved",
            target_identity.display_path.display()
        )),
    }
}

fn remove_obsolete_sidecar(path: &Path) -> Result<()> {
    match fs_mutate::remove_file_if_exists(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to delete obsolete document note sidecar {}: {}",
            path.display(),
            error
        )),
    }
}

/// Collect document notes under the current workspace folders for note browsers.
///
/// # Errors
///
/// Returns an error if the sidecar directory cannot be scanned or one document
/// note cannot be read or parsed.
pub fn list_workspace_document_notes(
    data_dir: &Path,
    workspace_folders: &[PathBuf],
) -> Result<Vec<WorkspaceDocumentNote>> {
    Ok(list_workspace_document_notes_recovering(data_dir, workspace_folders)?.notes)
}

/// Collect document notes and preserve partial-recovery diagnostics.
///
/// # Errors
///
/// Returns an error only when the sidecar directory itself cannot be scanned.
pub fn list_workspace_document_notes_recovering(
    data_dir: &Path,
    workspace_folders: &[PathBuf],
) -> Result<WorkspaceDocumentNoteListing> {
    let canonical_folders = note_storage::canonicalize_folders(workspace_folders);
    let dir = document_notes_dir(data_dir);
    if !fs_metadata::path_status(&dir)?.is_present() {
        return Ok(WorkspaceDocumentNoteListing {
            notes: Vec::new(),
            diagnostics: Vec::new(),
        });
    }

    let mut notes = Vec::new();
    let mut diagnostics = Vec::new();
    for entry in fs_tree::scan_directory(&dir, DirectoryScanPolicy::visible_workspace())
        .with_context(|| format!("failed to read {}", dir.display()))?
    {
        let load = note_storage::load_json_file_recovering::<DocumentNoteDocument>(
            data_dir,
            &entry.path,
            RecoveryMetadataClass::DocumentNoteSidecar,
        );
        note_storage::trace_recovery_diagnostics(&load.diagnostics);
        diagnostics.extend(load.diagnostics);
        let Some(document) = load.value else {
            continue;
        };
        if !note_storage::matches_any_folder(&document.identity, &canonical_folders) {
            continue;
        }
        notes.push(WorkspaceDocumentNote {
            path: document.identity.display_path,
            note: document.note,
        });
    }

    notes.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(WorkspaceDocumentNoteListing { notes, diagnostics })
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
        assert!(!fs_metadata::exists(&sidecar_path));
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
        assert!(!fs_metadata::exists(&sidecar_path));
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
        assert!(!fs_metadata::exists(&old_sidecar_path));
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
    fn move_path_tree_keeps_newest_duplicate_document_note() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_file = dir.path().join("workspace/old.rs");
        let new_file = dir.path().join("workspace/new.rs");
        write_file(&old_file, "fn old() {}\n");
        write_file(&new_file, "fn new() {}\n");
        let old_identity =
            note_storage::resolve_document_identity(&old_file).expect("old identity");
        let new_identity =
            note_storage::resolve_document_identity(&new_file).expect("new identity");
        let old_sidecar_path = document_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&old_identity.sidecar_id));
        save_document(
            dir.path(),
            &DocumentNoteDocument {
                identity: old_identity,
                note: RichNoteBody {
                    text: "newer source note".to_string(),
                    created_at_secs: 1,
                    updated_at_secs: 20,
                },
            },
        )
        .expect("save old duplicate note");
        save_document(
            dir.path(),
            &DocumentNoteDocument {
                identity: new_identity,
                note: RichNoteBody {
                    text: "older target note".to_string(),
                    created_at_secs: 1,
                    updated_at_secs: 10,
                },
            },
        )
        .expect("save target duplicate note");

        let migrated =
            move_path_tree(dir.path(), &old_file, &new_file).expect("newest note should merge");

        assert_eq!(migrated, 1);
        assert!(!fs_metadata::exists(&old_sidecar_path));
        let loaded = load_for_path(dir.path(), &new_file)
            .expect("load merged note")
            .expect("merged note exists");
        assert_eq!(loaded.note.text, "newer source note");
        assert_eq!(loaded.note.updated_at_secs, 20);
    }

    #[test]
    fn move_path_tree_preserves_ambiguous_document_note_conflict() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_file = dir.path().join("workspace/old.rs");
        let new_file = dir.path().join("workspace/new.rs");
        write_file(&old_file, "fn old() {}\n");
        write_file(&new_file, "fn new() {}\n");
        let old_identity =
            note_storage::resolve_document_identity(&old_file).expect("old identity");
        let new_identity =
            note_storage::resolve_document_identity(&new_file).expect("new identity");
        let old_sidecar_path = document_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&old_identity.sidecar_id));
        let new_sidecar_path = document_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&new_identity.sidecar_id));
        save_document(
            dir.path(),
            &DocumentNoteDocument {
                identity: old_identity,
                note: RichNoteBody {
                    text: "source text".to_string(),
                    created_at_secs: 1,
                    updated_at_secs: 10,
                },
            },
        )
        .expect("save old duplicate note");
        save_document(
            dir.path(),
            &DocumentNoteDocument {
                identity: new_identity,
                note: RichNoteBody {
                    text: "target text".to_string(),
                    created_at_secs: 1,
                    updated_at_secs: 10,
                },
            },
        )
        .expect("save target duplicate note");

        let error = move_path_tree(dir.path(), &old_file, &new_file)
            .expect_err("ambiguous equal-timestamp notes should not be guessed");

        assert!(
            error
                .to_string()
                .contains("ambiguous document note sidecar conflict"),
            "unexpected error: {error}"
        );
        assert!(fs_metadata::exists(&old_sidecar_path));
        assert!(fs_metadata::exists(&new_sidecar_path));
    }

    #[test]
    fn merge_document_note_documents_keeps_newer_target_and_rehomes_identity() {
        let source_identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/old.rs"),
            PathBuf::from("/workspace/old.rs"),
        );
        let target_identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/new.rs"),
            PathBuf::from("/workspace/new.rs"),
        );
        let source = DocumentNoteDocument {
            identity: source_identity,
            note: RichNoteBody {
                text: "older source note".to_string(),
                created_at_secs: 1,
                updated_at_secs: 10,
            },
        };
        let target = DocumentNoteDocument {
            identity: target_identity.clone(),
            note: RichNoteBody {
                text: "newer target note".to_string(),
                created_at_secs: 1,
                updated_at_secs: 20,
            },
        };

        let merged = merge_document_note_documents(source, target, target_identity.clone())
            .expect("newer target should win");

        assert_eq!(merged.identity, target_identity);
        assert_eq!(merged.note.text, "newer target note");
        assert_eq!(merged.note.updated_at_secs, 20);
    }

    #[test]
    fn merge_document_note_documents_accepts_same_note_at_same_timestamp() {
        let source_identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/old.rs"),
            PathBuf::from("/workspace/old.rs"),
        );
        let target_identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/new.rs"),
            PathBuf::from("/workspace/new.rs"),
        );
        let source = DocumentNoteDocument {
            identity: source_identity,
            note: RichNoteBody {
                text: "same note".to_string(),
                created_at_secs: 1,
                updated_at_secs: 10,
            },
        };
        let target = DocumentNoteDocument {
            identity: target_identity.clone(),
            note: RichNoteBody {
                text: "same note".to_string(),
                created_at_secs: 1,
                updated_at_secs: 10,
            },
        };

        let merged = merge_document_note_documents(source, target, target_identity.clone())
            .expect("identical equal-timestamp notes should merge without conflict");

        assert_eq!(merged.identity, target_identity);
        assert_eq!(merged.note.text, "same note");
    }

    #[test]
    fn list_workspace_document_notes_filters_folders_and_sorts_rows() {
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

    #[test]
    fn corrupt_document_note_sidecar_is_quarantined_without_hiding_valid_notes() {
        let dir = TempDir::new().expect("tempdir");
        let valid_path = dir.path().join("workspace/valid.rs");
        let corrupt_path = dir.path().join("workspace/corrupt.rs");
        write_file(&valid_path, "valid\n");
        write_file(&corrupt_path, "corrupt\n");
        save_for_path(dir.path(), &valid_path, &RichNoteBody::new("Keep valid"))
            .expect("save valid note");
        let corrupt_identity =
            note_storage::resolve_document_identity(&corrupt_path).expect("corrupt identity");
        let corrupt_sidecar = document_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&corrupt_identity.sidecar_id));
        fixture::write_text(&corrupt_sidecar, "not document note json");

        let listing =
            list_workspace_document_notes_recovering(dir.path(), &[dir.path().join("workspace")])
                .expect("list document notes despite corrupt sidecar");

        assert_eq!(listing.notes.len(), 1);
        assert_eq!(listing.notes[0].path, valid_path);
        assert_eq!(listing.notes[0].note.text, "Keep valid");
        assert_eq!(listing.diagnostics.len(), 1);
        assert_eq!(
            listing.diagnostics[0].class,
            RecoveryMetadataClass::DocumentNoteSidecar
        );
        assert!(
            !fs_metadata::exists(&corrupt_sidecar),
            "corrupt sidecar should be moved out of normal browse path"
        );
    }
}

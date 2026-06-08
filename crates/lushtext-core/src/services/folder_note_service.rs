// SPDX-License-Identifier: GPL-3.0-or-later

//! Folder-note persistence and listing helpers.
//!
//! Folder notes are keyed by the canonical workspace folder rather than by a
//! transient UI slot identifier, so removing and re-adding the same folder can
//! restore the same note.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::model::folder_note::{FolderNoteDocument, FolderNoteIdentity};
use crate::model::note::RichNoteBody;
use crate::model::workspace::{WorkspaceConfig, WorkspaceScope};
use crate::services::filesystem::{
    DirectoryScanPolicy, metadata as fs_metadata, mutate as fs_mutate, tree as fs_tree,
};
use crate::services::recovery_metadata::{RecoveryDiagnostic, RecoveryMetadataClass};

use super::note_storage;

/// Directory name that stores new folder-note sidecars.
const FOLDER_NOTES_DIR: &str = "folder-notes";
/// Directory name used by old releases for folder-note sidecars.
const LEGACY_FOLDER_NOTES_DIR: &str = "workspace-notes";

/// Lightweight workspace-facing note row for unified note browsers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedFolderNote {
    /// User-visible workspace label from the current workspace configuration.
    pub workspace_name: String,
    /// Workspace folder path associated with this note.
    pub folder: PathBuf,
    /// Stored rich note body.
    pub note: RichNoteBody,
}

/// Folder-note rows plus diagnostics for folders with skipped sidecars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderNoteListing {
    /// Folder-note rows safe to display in note browsers.
    pub notes: Vec<ListedFolderNote>,
    /// Recovery diagnostics for malformed or unreadable folder-note sidecars.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

/// Result of loading one concrete folder note with recovery diagnostics.
#[derive(Debug, Default)]
pub struct FolderNoteLoad {
    /// Loaded folder-note document when a supported sidecar could be read.
    pub document: Option<FolderNoteDocument>,
    /// Recovery diagnostics for malformed or unreadable sidecars encountered.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

/// Resolve the folder-note sidecar directory under the app data home.
#[must_use]
pub fn folder_notes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(FOLDER_NOTES_DIR)
}

fn legacy_folder_notes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(LEGACY_FOLDER_NOTES_DIR)
}

fn folder_note_sidecar_path(data_dir: &Path, identity: &FolderNoteIdentity) -> PathBuf {
    folder_notes_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id))
}

fn legacy_folder_note_sidecar_path(data_dir: &Path, identity: &FolderNoteIdentity) -> PathBuf {
    legacy_folder_notes_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id))
}

/// Resolve the stable identity for one workspace folder.
///
/// # Errors
///
/// Returns an error if the folder cannot be canonicalized.
pub fn resolve_folder_note_identity(folder: &Path) -> Result<FolderNoteIdentity> {
    let display_folder = folder.to_path_buf();
    let canonical_folder = fs_metadata::canonical_path(folder)
        .with_context(|| format!("failed to canonicalize {}", folder.display()))?;
    Ok(FolderNoteIdentity::from_folders(
        display_folder,
        canonical_folder,
    ))
}

/// Load the note for one workspace folder, returning `None` when no note exists yet.
///
/// # Errors
///
/// Returns an error if the folder identity cannot be resolved or the sidecar
/// directory cannot be scanned. Malformed or unreadable sidecars are preserved
/// through recovery diagnostics and treated as absent.
pub fn load_for_folder(data_dir: &Path, folder: &Path) -> Result<Option<FolderNoteDocument>> {
    Ok(load_for_folder_recovering(data_dir, folder)?.document)
}

/// Load the note for one workspace folder while preserving recovery diagnostics.
///
/// # Errors
///
/// Returns an error if the folder identity cannot be resolved.
pub fn load_for_folder_recovering(data_dir: &Path, folder: &Path) -> Result<FolderNoteLoad> {
    let identity = resolve_folder_note_identity(folder)?;
    Ok(load_for_identity_recovering(data_dir, &identity))
}

fn load_for_identity_recovering(data_dir: &Path, identity: &FolderNoteIdentity) -> FolderNoteLoad {
    let mut diagnostics = Vec::new();
    let path = folder_note_sidecar_path(data_dir, identity);
    let load = load_folder_note_sidecar(data_dir, &path);
    note_storage::trace_recovery_diagnostics(&load.diagnostics);
    diagnostics.extend(load.diagnostics);
    if load.value.is_some() {
        return FolderNoteLoad {
            document: load.value,
            diagnostics,
        };
    }

    let legacy_path = legacy_folder_note_sidecar_path(data_dir, identity);
    if legacy_path == path {
        return FolderNoteLoad {
            document: None,
            diagnostics,
        };
    }
    let legacy_load = load_folder_note_sidecar(data_dir, &legacy_path);
    note_storage::trace_recovery_diagnostics(&legacy_load.diagnostics);
    diagnostics.extend(legacy_load.diagnostics);
    FolderNoteLoad {
        document: legacy_load.value,
        diagnostics,
    }
}

fn load_folder_note_sidecar(
    data_dir: &Path,
    path: &Path,
) -> crate::services::recovery_metadata::RecoveryLoad<Option<FolderNoteDocument>> {
    note_storage::load_json_file_recovering::<FolderNoteDocument>(
        data_dir,
        path,
        RecoveryMetadataClass::FolderNoteSidecar,
    )
}

/// Save the current note for one workspace folder.
///
/// Empty note bodies delete the sidecar instead of persisting an empty payload.
///
/// # Errors
///
/// Returns an error if the folder identity cannot be resolved or the sidecar
/// cannot be written or deleted.
pub fn save_for_folder(
    data_dir: &Path,
    folder: &Path,
    note: &RichNoteBody,
) -> Result<FolderNoteIdentity> {
    let identity = resolve_folder_note_identity(folder)?;
    save_document(
        data_dir,
        &FolderNoteDocument {
            identity: identity.clone(),
            note: note.clone(),
        },
    )?;
    Ok(identity)
}

/// Save one fully shaped folder-note document.
///
/// # Errors
///
/// Returns an error if the sidecar cannot be written or deleted.
pub fn save_document(data_dir: &Path, document: &FolderNoteDocument) -> Result<()> {
    if document.note.is_empty() {
        return delete_sidecar_file(data_dir, &document.identity);
    }

    let path = folder_note_sidecar_path(data_dir, &document.identity);
    note_storage::save_json_file_recovering(
        data_dir,
        &path,
        RecoveryMetadataClass::FolderNoteSidecar,
        document,
    )
}

/// Delete the note for one workspace folder if it exists.
///
/// # Errors
///
/// Returns an error if the folder identity cannot be resolved or an existing
/// sidecar cannot be deleted.
pub fn delete_for_folder(data_dir: &Path, folder: &Path) -> Result<()> {
    let identity = resolve_folder_note_identity(folder)?;
    delete_sidecar_file(data_dir, &identity)
}

fn delete_sidecar_file(data_dir: &Path, identity: &FolderNoteIdentity) -> Result<()> {
    for path in [
        folder_note_sidecar_path(data_dir, identity),
        legacy_folder_note_sidecar_path(data_dir, identity),
    ] {
        if let Err(error) = fs_mutate::remove_file_if_exists(&path) {
            return Err(anyhow::anyhow!(
                "failed to delete folder note sidecar {}: {}",
                path.display(),
                error
            ));
        }
    }
    Ok(())
}

/// Move folder-note sidecars after an in-app folder-directory rename.
///
/// Returns the number of sidecars that were rewritten.
///
/// # Errors
///
/// Returns an error if the sidecar directory cannot be scanned or a migrated
/// sidecar cannot be read, rewritten, or cleaned up.
pub fn move_folder_tree(data_dir: &Path, old_folder: &Path, new_folder: &Path) -> Result<usize> {
    let mut migrated = 0;
    for dir in [
        folder_notes_dir(data_dir),
        legacy_folder_notes_dir(data_dir),
    ] {
        if !fs_metadata::path_status(&dir)?.is_present() {
            continue;
        }

        for entry in fs_tree::scan_directory(&dir, DirectoryScanPolicy::visible_workspace())
            .with_context(|| format!("failed to read {}", dir.display()))?
        {
            let sidecar_path = entry.path;
            if sidecar_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }

            let load = load_folder_note_sidecar(data_dir, &sidecar_path);
            note_storage::trace_recovery_diagnostics(&load.diagnostics);
            let Some(document) = load.value else {
                continue;
            };
            let Some((display_folder, canonical_folder)) =
                rebase_folder_note_identity(&document.identity, old_folder, new_folder)
            else {
                continue;
            };

            let new_identity = FolderNoteIdentity::from_folders(display_folder, canonical_folder);
            let new_sidecar_path = folder_note_sidecar_path(data_dir, &new_identity);
            let document =
                merge_folder_note_target(data_dir, &new_sidecar_path, document, new_identity)?;
            save_document(data_dir, &document)?;
            if sidecar_path != new_sidecar_path {
                remove_obsolete_sidecar(&sidecar_path)?;
            }
            migrated += 1;
        }
    }

    Ok(migrated)
}

fn merge_folder_note_target(
    data_dir: &Path,
    target_path: &Path,
    mut source: FolderNoteDocument,
    target_identity: FolderNoteIdentity,
) -> Result<FolderNoteDocument> {
    let load = note_storage::load_json_file_recovering::<FolderNoteDocument>(
        data_dir,
        target_path,
        RecoveryMetadataClass::FolderNoteSidecar,
    );
    note_storage::trace_recovery_diagnostics(&load.diagnostics);
    let Some(target) = load.value else {
        source.identity = target_identity;
        return Ok(source);
    };

    merge_folder_note_documents(source, target, target_identity)
}

/// Merge a moved folder note into an existing target without guessing conflicts.
fn merge_folder_note_documents(
    source: FolderNoteDocument,
    mut target: FolderNoteDocument,
    target_identity: FolderNoteIdentity,
) -> Result<FolderNoteDocument> {
    let source_newer = source.note.updated_at_secs > target.note.updated_at_secs;
    let target_newer = target.note.updated_at_secs > source.note.updated_at_secs;
    if source_newer {
        return Ok(FolderNoteDocument {
            identity: target_identity,
            note: source.note,
        });
    }
    if target_newer || source.note == target.note {
        target.identity = target_identity;
        return Ok(target);
    }

    Err(anyhow::anyhow!(
        "ambiguous folder note sidecar conflict for {}; both copies were preserved",
        target_identity.display_folder.display()
    ))
}

fn remove_obsolete_sidecar(path: &Path) -> Result<()> {
    match fs_mutate::remove_file_if_exists(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to delete obsolete folder note sidecar {}: {}",
            path.display(),
            error
        )),
    }
}

/// Collect folder notes covered by the current shared workspace scope.
///
/// # Errors
///
/// Returns an error if one stored note cannot be read or parsed.
pub fn list_folder_notes_for_scope(
    data_dir: &Path,
    workspaces: &[WorkspaceConfig],
    scope: &WorkspaceScope,
) -> Result<Vec<ListedFolderNote>> {
    Ok(list_folder_notes_for_scope_recovering(data_dir, workspaces, scope)?.notes)
}

/// Collect folder notes in workspace/folder order and preserve partial-recovery diagnostics.
///
/// # Errors
///
/// Returns an error if a workspace folder cannot be resolved.
pub fn list_folder_notes_for_scope_recovering(
    data_dir: &Path,
    workspaces: &[WorkspaceConfig],
    scope: &WorkspaceScope,
) -> Result<FolderNoteListing> {
    let visible_workspaces: Vec<&WorkspaceConfig> = match scope {
        WorkspaceScope::All => workspaces.iter().collect(),
        WorkspaceScope::Workspace(workspace_id) => workspaces
            .iter()
            .filter(|workspace| &workspace.id == workspace_id)
            .collect(),
    };

    let mut notes = Vec::new();
    let mut diagnostics = Vec::new();
    for workspace in visible_workspaces {
        for folder_path in workspace.folder_paths() {
            let identity = resolve_folder_note_identity(&folder_path)?;
            let load = load_for_identity_recovering(data_dir, &identity);
            diagnostics.extend(load.diagnostics);
            let Some(document) = load.document else {
                continue;
            };
            notes.push(ListedFolderNote {
                workspace_name: workspace.name.clone(),
                folder: folder_path,
                note: document.note,
            });
        }
    }

    Ok(FolderNoteListing { notes, diagnostics })
}

fn rebase_folder_note_identity(
    identity: &FolderNoteIdentity,
    old_folder: &Path,
    new_folder: &Path,
) -> Option<(PathBuf, PathBuf)> {
    if identity.display_folder == old_folder || identity.display_folder.starts_with(old_folder) {
        let suffix = identity
            .display_folder
            .strip_prefix(old_folder)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_default();
        let display_folder = if suffix.as_os_str().is_empty() {
            new_folder.to_path_buf()
        } else {
            new_folder.join(suffix)
        };
        let canonical_folder =
            fs_metadata::canonical_path(&display_folder).unwrap_or_else(|_| display_folder.clone());
        return Some((display_folder, canonical_folder));
    }

    if identity.canonical_folder == old_folder || identity.canonical_folder.starts_with(old_folder)
    {
        let suffix = identity
            .canonical_folder
            .strip_prefix(old_folder)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_default();
        let display_folder = if suffix.as_os_str().is_empty() {
            new_folder.to_path_buf()
        } else {
            new_folder.join(suffix)
        };
        let canonical_folder =
            fs_metadata::canonical_path(&display_folder).unwrap_or_else(|_| display_folder.clone());
        return Some((display_folder, canonical_folder));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::workspace::{WorkspaceConfig, WorkspaceFolder, WorkspaceId};
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn create_dir(path: &Path) {
        fixture::create_dir_all(path);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let folder = dir.path().join("workspace");
        create_dir(&folder);

        save_for_folder(dir.path(), &folder, &RichNoteBody::new("Project summary"))
            .expect("expected operation to succeed");
        let loaded = load_for_folder(dir.path(), &folder).expect("expected operation to succeed");

        let loaded = loaded.expect("expected folder note");
        assert_eq!(loaded.identity.display_folder, folder);
        assert_eq!(loaded.note.text, "Project summary");
    }

    #[test]
    fn load_for_folder_reads_legacy_sidecar_payload() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let folder = dir.path().join("workspace");
        create_dir(&folder);
        let identity = resolve_folder_note_identity(&folder).expect("folder identity");
        let legacy_path = legacy_folder_note_sidecar_path(dir.path(), &identity);
        fixture::create_dir_all(&legacy_folder_notes_dir(dir.path()));
        fixture::write_text(
            &legacy_path,
            &serde_json::json!({
                "kind": crate::services::json_format::KIND_LEGACY_WORKSPACE_NOTE_SIDECAR,
                "version": 1,
                "data": {
                    "identity": {
                        "display_root": folder,
                        "canonical_root": identity.canonical_folder,
                        "sidecar_id": identity.sidecar_id,
                    },
                    "note": RichNoteBody::new("Legacy project note"),
                }
            })
            .to_string(),
        );

        let loaded = load_for_folder(dir.path(), &folder)
            .expect("legacy sidecar should load")
            .expect("legacy note exists");

        assert_eq!(loaded.identity.display_folder, folder);
        assert_eq!(loaded.note.text, "Legacy project note");
    }

    #[test]
    fn load_for_folder_recovering_reports_corrupt_sidecar() {
        let dir = TempDir::new().expect("tempdir");
        let folder = dir.path().join("workspace");
        create_dir(&folder);
        let identity = resolve_folder_note_identity(&folder).expect("folder identity");
        let corrupt_sidecar =
            folder_notes_dir(dir.path()).join(note_storage::sidecar_filename(&identity.sidecar_id));
        fixture::create_dir_all(&folder_notes_dir(dir.path()));
        fixture::write_text(&corrupt_sidecar, "not folder note json");

        let load = load_for_folder_recovering(dir.path(), &folder)
            .expect("corrupt sidecar is recoverable");

        assert!(load.document.is_none());
        assert_eq!(load.diagnostics.len(), 1);
        assert_eq!(
            load.diagnostics[0].class,
            RecoveryMetadataClass::FolderNoteSidecar
        );
        assert!(
            !fs_metadata::exists(&corrupt_sidecar),
            "corrupt direct-load sidecar should be moved out of the normal load path"
        );
    }

    #[test]
    fn save_document_rewrites_legacy_kind_to_folder_note_kind() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let folder = dir.path().join("workspace");
        create_dir(&folder);
        let identity = resolve_folder_note_identity(&folder).expect("folder identity");
        let sidecar_path = folder_note_sidecar_path(dir.path(), &identity);
        fixture::create_dir_all(&folder_notes_dir(dir.path()));
        fixture::write_text(
            &sidecar_path,
            &serde_json::json!({
                "kind": crate::services::json_format::KIND_LEGACY_WORKSPACE_NOTE_SIDECAR,
                "version": 1,
                "data": {
                    "identity": {
                        "display_root": folder,
                        "canonical_root": identity.canonical_folder,
                        "sidecar_id": identity.sidecar_id,
                    },
                    "note": RichNoteBody::new("Legacy project note"),
                }
            })
            .to_string(),
        );

        save_document(
            dir.path(),
            &FolderNoteDocument {
                identity,
                note: RichNoteBody::new("Renamed folder note"),
            },
        )
        .expect("legacy-compatible replacement should save");

        let bytes = fixture::read_bytes(&sidecar_path);
        let saved: FolderNoteDocument = crate::services::json_format::parse_v1_payload(
            &bytes,
            crate::services::json_format::KIND_FOLDER_NOTE_SIDECAR,
        )
        .expect("saved sidecar should use the folder-note kind");
        assert_eq!(saved.note.text, "Renamed folder note");
    }

    #[test]
    fn empty_save_deletes_sidecar() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let folder = dir.path().join("workspace");
        create_dir(&folder);

        let identity = save_for_folder(dir.path(), &folder, &RichNoteBody::new("Remember this"))
            .expect("expected operation to succeed");
        save_for_folder(dir.path(), &folder, &RichNoteBody::new("   "))
            .expect("expected operation to succeed");

        let sidecar_path =
            folder_notes_dir(dir.path()).join(note_storage::sidecar_filename(&identity.sidecar_id));
        assert!(!fs_metadata::exists(&sidecar_path));
    }

    #[test]
    fn delete_for_folder_removes_existing_sidecar_and_ignores_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let folder = dir.path().join("workspace");
        create_dir(&folder);

        let identity = save_for_folder(dir.path(), &folder, &RichNoteBody::new("Remember this"))
            .expect("expected operation to succeed");
        let sidecar_path =
            folder_notes_dir(dir.path()).join(note_storage::sidecar_filename(&identity.sidecar_id));

        delete_for_folder(dir.path(), &folder).expect("expected operation to succeed");
        assert!(!fs_metadata::exists(&sidecar_path));
        delete_for_folder(dir.path(), &folder).expect("expected missing sidecar to be a no-op");
    }

    #[test]
    fn delete_for_folder_reports_non_file_sidecar_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let folder = dir.path().join("workspace");
        create_dir(&folder);

        let identity = save_for_folder(dir.path(), &folder, &RichNoteBody::new("Remember this"))
            .expect("expected operation to succeed");
        let sidecar_path =
            folder_notes_dir(dir.path()).join(note_storage::sidecar_filename(&identity.sidecar_id));
        fixture::remove_file(&sidecar_path);
        fixture::create_dir(&sidecar_path);

        let error =
            delete_for_folder(dir.path(), &folder).expect_err("directory sidecar should fail");
        assert!(
            error
                .to_string()
                .contains("failed to delete folder note sidecar"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn move_folder_tree_returns_zero_when_sidecar_dir_is_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_folder = dir.path().join("old");
        let new_folder = dir.path().join("new");

        let migrated = move_folder_tree(dir.path(), &old_folder, &new_folder)
            .expect("expected operation to succeed");

        assert_eq!(migrated, 0);
    }

    #[test]
    fn move_folder_tree_rewrites_note_identity_and_removes_old_sidecar() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_folder = dir.path().join("old-workspace");
        let new_folder = dir.path().join("new-workspace");
        create_dir(&old_folder);

        let old_identity =
            save_for_folder(dir.path(), &old_folder, &RichNoteBody::new("Project note"))
                .expect("expected operation to succeed");
        let old_sidecar_path = folder_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&old_identity.sidecar_id));

        fixture::rename(&old_folder, &new_folder);
        let migrated = move_folder_tree(dir.path(), &old_folder, &new_folder)
            .expect("expected operation to succeed");

        assert_eq!(migrated, 1);
        assert!(!fs_metadata::exists(&old_sidecar_path));
        let loaded =
            load_for_folder(dir.path(), &new_folder).expect("expected operation to succeed");
        let loaded = loaded.expect("expected moved folder note");
        assert_eq!(loaded.identity.display_folder, new_folder);
        assert_eq!(loaded.note.text, "Project note");
        let json_sidecars = fs_tree::scan_directory(
            &folder_notes_dir(dir.path()),
            DirectoryScanPolicy::visible_workspace(),
        )
        .expect("expected operation to succeed")
        .into_iter()
        .filter(|entry| entry.path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .count();
        assert_eq!(json_sidecars, 1);
    }

    #[test]
    fn move_folder_tree_keeps_newest_duplicate_folder_note() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_folder = dir.path().join("old-workspace");
        let new_folder = dir.path().join("new-workspace");
        create_dir(&old_folder);
        create_dir(&new_folder);
        let old_identity = resolve_folder_note_identity(&old_folder).expect("old identity");
        let new_identity = resolve_folder_note_identity(&new_folder).expect("new identity");
        let old_sidecar_path = folder_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&old_identity.sidecar_id));
        save_document(
            dir.path(),
            &FolderNoteDocument {
                identity: old_identity,
                note: RichNoteBody {
                    text: "newer source folder note".to_string(),
                    created_at_secs: 1,
                    updated_at_secs: 20,
                },
            },
        )
        .expect("save old duplicate folder note");
        save_document(
            dir.path(),
            &FolderNoteDocument {
                identity: new_identity,
                note: RichNoteBody {
                    text: "older target folder note".to_string(),
                    created_at_secs: 1,
                    updated_at_secs: 10,
                },
            },
        )
        .expect("save target duplicate folder note");

        let migrated = move_folder_tree(dir.path(), &old_folder, &new_folder)
            .expect("newest note should merge");

        assert_eq!(migrated, 1);
        assert!(!fs_metadata::exists(&old_sidecar_path));
        let loaded = load_for_folder(dir.path(), &new_folder)
            .expect("load merged note")
            .expect("merged note exists");
        assert_eq!(loaded.note.text, "newer source folder note");
        assert_eq!(loaded.note.updated_at_secs, 20);
    }

    #[test]
    fn move_folder_tree_preserves_ambiguous_folder_note_conflict() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_folder = dir.path().join("old-workspace");
        let new_folder = dir.path().join("new-workspace");
        create_dir(&old_folder);
        create_dir(&new_folder);
        let old_identity = resolve_folder_note_identity(&old_folder).expect("old identity");
        let new_identity = resolve_folder_note_identity(&new_folder).expect("new identity");
        let old_sidecar_path = folder_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&old_identity.sidecar_id));
        let new_sidecar_path = folder_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&new_identity.sidecar_id));
        save_document(
            dir.path(),
            &FolderNoteDocument {
                identity: old_identity,
                note: RichNoteBody {
                    text: "source folder note".to_string(),
                    created_at_secs: 1,
                    updated_at_secs: 10,
                },
            },
        )
        .expect("save old duplicate folder note");
        save_document(
            dir.path(),
            &FolderNoteDocument {
                identity: new_identity,
                note: RichNoteBody {
                    text: "target folder note".to_string(),
                    created_at_secs: 1,
                    updated_at_secs: 10,
                },
            },
        )
        .expect("save target duplicate folder note");

        let error = move_folder_tree(dir.path(), &old_folder, &new_folder)
            .expect_err("ambiguous equal-timestamp notes should not be guessed");

        assert!(
            error
                .to_string()
                .contains("ambiguous folder note sidecar conflict"),
            "unexpected error: {error}"
        );
        assert!(fs_metadata::exists(&old_sidecar_path));
        assert!(fs_metadata::exists(&new_sidecar_path));
    }

    #[test]
    fn rebase_folder_note_identity_handles_display_and_canonical_prefixes() {
        let old_folder = Path::new("/project/old");
        let new_folder = Path::new("/project/new");
        let display_nested = FolderNoteIdentity::from_folders(
            PathBuf::from("/project/old/nested"),
            PathBuf::from("/canonical/elsewhere"),
        );
        let canonical_nested = FolderNoteIdentity::from_folders(
            PathBuf::from("/visible/elsewhere"),
            PathBuf::from("/project/old/nested"),
        );
        let unrelated = FolderNoteIdentity::from_folders(
            PathBuf::from("/project/other"),
            PathBuf::from("/canonical/other"),
        );

        let (display_folder, canonical_folder) =
            rebase_folder_note_identity(&display_nested, old_folder, new_folder)
                .expect("display folder should rebase");
        assert_eq!(display_folder, PathBuf::from("/project/new/nested"));
        assert_eq!(canonical_folder, PathBuf::from("/project/new/nested"));

        let (display_folder, canonical_folder) =
            rebase_folder_note_identity(&canonical_nested, old_folder, new_folder)
                .expect("canonical folder should rebase");
        assert_eq!(display_folder, PathBuf::from("/project/new/nested"));
        assert_eq!(canonical_folder, PathBuf::from("/project/new/nested"));

        assert!(rebase_folder_note_identity(&unrelated, old_folder, new_folder).is_none());
    }

    #[test]
    fn list_for_scope_uses_folder_identity_not_workspace_slot() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let folder = dir.path().join("workspace");
        create_dir(&folder);

        save_for_folder(dir.path(), &folder, &RichNoteBody::new("Reusable note"))
            .expect("expected operation to succeed");

        let workspaces = vec![WorkspaceConfig::with_one_folder(
            WorkspaceId::new("new-slot"),
            "Workspace",
            folder,
        )];
        let notes = list_folder_notes_for_scope(
            dir.path(),
            &workspaces,
            &WorkspaceScope::Workspace(WorkspaceId::new("new-slot")),
        )
        .expect("expected operation to succeed");

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note.text, "Reusable note");
    }

    #[test]
    fn list_for_scope_preserves_workspace_and_folder_order() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let first_parent = dir.path().join("first-parent");
        let first_child = first_parent.join("nested");
        let second_folder = dir.path().join("second");
        create_dir(&first_child);
        create_dir(&second_folder);

        save_for_folder(
            dir.path(),
            &first_parent,
            &RichNoteBody::new("first parent"),
        )
        .expect("save first parent folder note");
        save_for_folder(dir.path(), &first_child, &RichNoteBody::new("first child"))
            .expect("save first child folder note");
        save_for_folder(dir.path(), &second_folder, &RichNoteBody::new("second"))
            .expect("save second workspace folder note");

        let workspaces = vec![
            WorkspaceConfig::with_one_folder(
                WorkspaceId::new("z-second-but-listed-first"),
                "Zeta",
                second_folder.clone(),
            ),
            WorkspaceConfig::with_folders(
                WorkspaceId::new("a-first-but-listed-second"),
                "Alpha",
                vec![
                    WorkspaceFolder::new(first_parent.clone()),
                    WorkspaceFolder::new(first_child.clone()),
                ],
            ),
        ];

        let notes = list_folder_notes_for_scope(dir.path(), &workspaces, &WorkspaceScope::All)
            .expect("list folder notes");

        let folders = notes
            .iter()
            .map(|note| note.folder.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            folders,
            vec![second_folder, first_parent, first_child],
            "Browse Notes folder-note rows should follow workspace order and then folder order"
        );
    }

    #[test]
    fn corrupt_folder_note_sidecar_is_quarantined_without_blocking_workspace() {
        let dir = TempDir::new().expect("tempdir");
        let folder = dir.path().join("workspace");
        create_dir(&folder);
        let identity = resolve_folder_note_identity(&folder).expect("folder identity");
        let corrupt_sidecar =
            folder_notes_dir(dir.path()).join(note_storage::sidecar_filename(&identity.sidecar_id));
        fixture::create_dir_all(&folder_notes_dir(dir.path()));
        fixture::write_text(&corrupt_sidecar, "not folder note json");

        let workspaces = vec![WorkspaceConfig::with_one_folder(
            WorkspaceId::new("workspace"),
            "Workspace",
            folder,
        )];
        let listing =
            list_folder_notes_for_scope_recovering(dir.path(), &workspaces, &WorkspaceScope::All)
                .expect("corrupt sidecar becomes absent");

        assert!(listing.notes.is_empty());
        assert_eq!(listing.diagnostics.len(), 1);
        assert_eq!(
            listing.diagnostics[0].class,
            RecoveryMetadataClass::FolderNoteSidecar
        );
        assert!(
            !fs_metadata::exists(&corrupt_sidecar),
            "corrupt sidecar should be moved out of normal load path"
        );
    }
}

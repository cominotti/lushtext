// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-note persistence and listing helpers.
//!
//! Workspace notes are keyed by the canonical workspace root rather than by a
//! transient UI slot identifier, so removing and re-adding the same root can
//! restore the same note.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::model::note::RichNoteBody;
use crate::model::workspace::{WorkspaceConfig, WorkspaceScope};
use crate::model::workspace_note::{WorkspaceNoteDocument, WorkspaceRootIdentity};
use crate::services::filesystem::{
    DirectoryScanPolicy, metadata as fs_metadata, mutate as fs_mutate, tree as fs_tree,
};
use crate::services::json_store;

use super::note_storage;

/// Directory name that stores workspace-note sidecars.
const WORKSPACE_NOTES_DIR: &str = "workspace-notes";

/// Lightweight workspace-facing note row for unified note browsers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedWorkspaceNote {
    /// User-visible workspace label from the current workspace configuration.
    pub workspace_name: String,
    /// Workspace root path associated with this note.
    pub root: PathBuf,
    /// Stored rich note body.
    pub note: RichNoteBody,
}

/// Resolve the workspace-note sidecar directory under the app data home.
#[must_use]
pub fn workspace_notes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(WORKSPACE_NOTES_DIR)
}

/// Resolve the stable identity for one workspace root.
///
/// # Errors
///
/// Returns an error if the root cannot be canonicalized.
pub fn resolve_workspace_root_identity(root: &Path) -> Result<WorkspaceRootIdentity> {
    let display_root = root.to_path_buf();
    let canonical_root = fs_metadata::canonical_path(root)
        .with_context(|| format!("failed to canonicalize {}", root.display()))?;
    Ok(WorkspaceRootIdentity::from_roots(
        display_root,
        canonical_root,
    ))
}

/// Load the note for one workspace root, returning `None` when no note exists yet.
///
/// # Errors
///
/// Returns an error if the root identity cannot be resolved, the sidecar cannot
/// be read, or the stored JSON cannot be parsed.
pub fn load_for_root(data_dir: &Path, root: &Path) -> Result<Option<WorkspaceNoteDocument>> {
    let identity = resolve_workspace_root_identity(root)?;
    load_for_identity(data_dir, &identity)
}

fn load_for_identity(
    data_dir: &Path,
    identity: &WorkspaceRootIdentity,
) -> Result<Option<WorkspaceNoteDocument>> {
    let path =
        workspace_notes_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id));
    note_storage::load_json_file::<WorkspaceNoteDocument>(&path)
}

/// Save the current note for one workspace root.
///
/// Empty note bodies delete the sidecar instead of persisting an empty payload.
///
/// # Errors
///
/// Returns an error if the root identity cannot be resolved or the sidecar
/// cannot be written or deleted.
pub fn save_for_root(
    data_dir: &Path,
    root: &Path,
    note: &RichNoteBody,
) -> Result<WorkspaceRootIdentity> {
    let identity = resolve_workspace_root_identity(root)?;
    save_document(
        data_dir,
        &WorkspaceNoteDocument {
            identity: identity.clone(),
            note: note.clone(),
        },
    )?;
    Ok(identity)
}

/// Save one fully shaped workspace-note document.
///
/// # Errors
///
/// Returns an error if the sidecar cannot be written or deleted.
pub fn save_document(data_dir: &Path, document: &WorkspaceNoteDocument) -> Result<()> {
    if document.note.is_empty() {
        return delete_sidecar_file(data_dir, &document.identity);
    }

    json_store::save(
        &workspace_notes_dir(data_dir),
        &note_storage::sidecar_filename(&document.identity.sidecar_id),
        document,
    )
}

/// Delete the note for one workspace root if it exists.
///
/// # Errors
///
/// Returns an error if the root identity cannot be resolved or an existing
/// sidecar cannot be deleted.
pub fn delete_for_root(data_dir: &Path, root: &Path) -> Result<()> {
    let identity = resolve_workspace_root_identity(root)?;
    delete_sidecar_file(data_dir, &identity)
}

fn delete_sidecar_file(data_dir: &Path, identity: &WorkspaceRootIdentity) -> Result<()> {
    let path =
        workspace_notes_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id));
    match fs_mutate::remove_file_if_exists(&path) {
        Ok(_) => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to delete workspace note sidecar {}: {}",
            path.display(),
            error
        )),
    }
}

/// Move workspace-note sidecars after an in-app root-directory rename.
///
/// Returns the number of sidecars that were rewritten.
///
/// # Errors
///
/// Returns an error if the sidecar directory cannot be scanned or a migrated
/// sidecar cannot be read, rewritten, or cleaned up.
pub fn move_root_tree(data_dir: &Path, old_root: &Path, new_root: &Path) -> Result<usize> {
    let dir = workspace_notes_dir(data_dir);
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
            note_storage::load_json_file::<WorkspaceNoteDocument>(&sidecar_path)?
        else {
            continue;
        };
        let Some((display_root, canonical_root)) =
            rebase_workspace_root_identity(&document.identity, old_root, new_root)
        else {
            continue;
        };

        document.identity = WorkspaceRootIdentity::from_roots(display_root, canonical_root);
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

/// Collect workspace notes covered by the current shared workspace scope.
///
/// # Errors
///
/// Returns an error if one stored note cannot be read or parsed.
pub fn list_workspace_notes_for_scope(
    data_dir: &Path,
    workspaces: &[WorkspaceConfig],
    scope: &WorkspaceScope,
) -> Result<Vec<ListedWorkspaceNote>> {
    let visible_workspaces: Vec<&WorkspaceConfig> = match scope {
        WorkspaceScope::All => workspaces.iter().collect(),
        WorkspaceScope::Workspace(workspace_id) => workspaces
            .iter()
            .filter(|workspace| &workspace.id == workspace_id)
            .collect(),
    };

    let mut notes = Vec::new();
    for workspace in visible_workspaces {
        let Some(document) = load_for_root(data_dir, &workspace.root)? else {
            continue;
        };
        notes.push(ListedWorkspaceNote {
            workspace_name: workspace.name.clone(),
            root: workspace.root.clone(),
            note: document.note,
        });
    }

    notes.sort_by(|left, right| left.workspace_name.cmp(&right.workspace_name));
    Ok(notes)
}

fn rebase_workspace_root_identity(
    identity: &WorkspaceRootIdentity,
    old_root: &Path,
    new_root: &Path,
) -> Option<(PathBuf, PathBuf)> {
    if identity.display_root == old_root || identity.display_root.starts_with(old_root) {
        let suffix = identity
            .display_root
            .strip_prefix(old_root)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_default();
        let display_root = if suffix.as_os_str().is_empty() {
            new_root.to_path_buf()
        } else {
            new_root.join(suffix)
        };
        let canonical_root =
            fs_metadata::canonical_path(&display_root).unwrap_or_else(|_| display_root.clone());
        return Some((display_root, canonical_root));
    }

    if identity.canonical_root == old_root || identity.canonical_root.starts_with(old_root) {
        let suffix = identity
            .canonical_root
            .strip_prefix(old_root)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_default();
        let display_root = if suffix.as_os_str().is_empty() {
            new_root.to_path_buf()
        } else {
            new_root.join(suffix)
        };
        let canonical_root =
            fs_metadata::canonical_path(&display_root).unwrap_or_else(|_| display_root.clone());
        return Some((display_root, canonical_root));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::workspace::{WorkspaceConfig, WorkspaceId};
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn create_dir(path: &Path) {
        fixture::create_dir_all(path);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let root = dir.path().join("workspace");
        create_dir(&root);

        save_for_root(dir.path(), &root, &RichNoteBody::new("Project summary"))
            .expect("expected operation to succeed");
        let loaded = load_for_root(dir.path(), &root).expect("expected operation to succeed");

        let loaded = loaded.expect("expected workspace note");
        assert_eq!(loaded.identity.display_root, root);
        assert_eq!(loaded.note.text, "Project summary");
    }

    #[test]
    fn empty_save_deletes_sidecar() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let root = dir.path().join("workspace");
        create_dir(&root);

        let identity = save_for_root(dir.path(), &root, &RichNoteBody::new("Remember this"))
            .expect("expected operation to succeed");
        save_for_root(dir.path(), &root, &RichNoteBody::new("   "))
            .expect("expected operation to succeed");

        let sidecar_path = workspace_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&identity.sidecar_id));
        assert!(fs_metadata::file_facts(&sidecar_path).is_err());
    }

    #[test]
    fn delete_for_root_removes_existing_sidecar_and_ignores_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let root = dir.path().join("workspace");
        create_dir(&root);

        let identity = save_for_root(dir.path(), &root, &RichNoteBody::new("Remember this"))
            .expect("expected operation to succeed");
        let sidecar_path = workspace_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&identity.sidecar_id));

        delete_for_root(dir.path(), &root).expect("expected operation to succeed");
        assert!(fs_metadata::file_facts(&sidecar_path).is_err());
        delete_for_root(dir.path(), &root).expect("expected missing sidecar to be a no-op");
    }

    #[test]
    fn delete_for_root_reports_non_file_sidecar_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let root = dir.path().join("workspace");
        create_dir(&root);

        let identity = save_for_root(dir.path(), &root, &RichNoteBody::new("Remember this"))
            .expect("expected operation to succeed");
        let sidecar_path = workspace_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&identity.sidecar_id));
        fixture::remove_file(&sidecar_path);
        fixture::create_dir(&sidecar_path);

        let error = delete_for_root(dir.path(), &root).expect_err("directory sidecar should fail");
        assert!(
            error
                .to_string()
                .contains("failed to delete workspace note sidecar"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn move_root_tree_returns_zero_when_sidecar_dir_is_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_root = dir.path().join("old");
        let new_root = dir.path().join("new");

        let migrated = move_root_tree(dir.path(), &old_root, &new_root)
            .expect("expected operation to succeed");

        assert_eq!(migrated, 0);
    }

    #[test]
    fn move_root_tree_rewrites_note_identity_and_removes_old_sidecar() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_root = dir.path().join("old-workspace");
        let new_root = dir.path().join("new-workspace");
        create_dir(&old_root);

        let old_identity = save_for_root(dir.path(), &old_root, &RichNoteBody::new("Project note"))
            .expect("expected operation to succeed");
        let old_sidecar_path = workspace_notes_dir(dir.path())
            .join(note_storage::sidecar_filename(&old_identity.sidecar_id));

        fixture::rename(&old_root, &new_root);
        let migrated = move_root_tree(dir.path(), &old_root, &new_root)
            .expect("expected operation to succeed");

        assert_eq!(migrated, 1);
        assert!(fs_metadata::file_facts(&old_sidecar_path).is_err());
        let loaded = load_for_root(dir.path(), &new_root).expect("expected operation to succeed");
        let loaded = loaded.expect("expected moved workspace note");
        assert_eq!(loaded.identity.display_root, new_root);
        assert_eq!(loaded.note.text, "Project note");
        let json_sidecars = fs_tree::scan_directory(
            &workspace_notes_dir(dir.path()),
            DirectoryScanPolicy::visible_workspace(),
        )
        .expect("expected operation to succeed")
        .into_iter()
        .filter(|entry| entry.path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .count();
        assert_eq!(json_sidecars, 1);
    }

    #[test]
    fn rebase_workspace_root_identity_handles_display_and_canonical_prefixes() {
        let old_root = Path::new("/project/old");
        let new_root = Path::new("/project/new");
        let display_nested = WorkspaceRootIdentity::from_roots(
            PathBuf::from("/project/old/nested"),
            PathBuf::from("/canonical/elsewhere"),
        );
        let canonical_nested = WorkspaceRootIdentity::from_roots(
            PathBuf::from("/visible/elsewhere"),
            PathBuf::from("/project/old/nested"),
        );
        let unrelated = WorkspaceRootIdentity::from_roots(
            PathBuf::from("/project/other"),
            PathBuf::from("/canonical/other"),
        );

        let (display_root, canonical_root) =
            rebase_workspace_root_identity(&display_nested, old_root, new_root)
                .expect("display root should rebase");
        assert_eq!(display_root, PathBuf::from("/project/new/nested"));
        assert_eq!(canonical_root, PathBuf::from("/project/new/nested"));

        let (display_root, canonical_root) =
            rebase_workspace_root_identity(&canonical_nested, old_root, new_root)
                .expect("canonical root should rebase");
        assert_eq!(display_root, PathBuf::from("/project/new/nested"));
        assert_eq!(canonical_root, PathBuf::from("/project/new/nested"));

        assert!(rebase_workspace_root_identity(&unrelated, old_root, new_root).is_none());
    }

    #[test]
    fn list_for_scope_uses_root_identity_not_workspace_slot() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let root = dir.path().join("workspace");
        create_dir(&root);

        save_for_root(dir.path(), &root, &RichNoteBody::new("Reusable note"))
            .expect("expected operation to succeed");

        let workspaces = vec![WorkspaceConfig {
            id: WorkspaceId::new("new-slot"),
            name: "Workspace".to_string(),
            root: root.clone(),
        }];
        let notes = list_workspace_notes_for_scope(
            dir.path(),
            &workspaces,
            &WorkspaceScope::Workspace(WorkspaceId::new("new-slot")),
        )
        .expect("expected operation to succeed");

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note.text, "Reusable note");
    }
}

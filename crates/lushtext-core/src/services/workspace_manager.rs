// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace persistence for the public v1 JSON format.
//!
//! This service owns the app-data boundary for `workspaces.json`. The runtime
//! format is a clean break from pre-public bare workspace JSON, so recovery
//! handles preservation before the sidebar consumes a default state.

use crate::model::workspace::{
    WorkspaceFolderId, WorkspaceFolderMoveDirection, WorkspaceId, WorkspacesFile,
};
use crate::services::filesystem::metadata as fs_metadata;
use crate::services::json_format::KIND_WORKSPACE_STATE;
use crate::services::recovery_metadata::{
    RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass, load_enveloped_json_or_default,
    save_enveloped_json_path,
};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Fixed filename for workspace state.
const WORKSPACES_FILE: &str = "workspaces.json";

/// Error returned when a folder cannot be added to a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFolderAddError {
    /// The target workspace id no longer exists in the in-memory state.
    WorkspaceNotFound,
    /// The same canonical or fallback folder identity already exists there.
    DuplicateFolder,
    /// The caller's off-main-thread identity snapshot no longer matches state.
    StaleFolderSnapshot,
}

/// Error returned when a folder membership cannot be removed from a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFolderRemoveError {
    /// The target workspace id no longer exists in the in-memory state.
    WorkspaceNotFound,
    /// The requested folder id no longer belongs to that workspace.
    FolderNotFound,
}

/// Error returned when a folder membership cannot be reordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFolderReorderError {
    /// The target workspace id no longer exists in the in-memory state.
    WorkspaceNotFound,
    /// The requested folder id no longer belongs to that workspace.
    FolderNotFound,
    /// The requested relative move would leave the folder in the same place.
    AlreadyAtBoundary,
}

/// Resolved identity for a configured workspace folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFolderIdentity {
    configured_path: PathBuf,
    canonical_path: Option<PathBuf>,
}

impl WorkspaceFolderIdentity {
    fn candidates(&self) -> impl Iterator<Item = &PathBuf> {
        std::iter::once(&self.configured_path).chain(self.canonical_path.iter())
    }
}

/// Add one folder to a workspace after checking per-workspace folder identity.
///
/// This command mutates only the provided in-memory state. Callers still own
/// debounced persistence so multiple sidebar edits can coalesce safely.
///
/// # Errors
///
/// Returns [`WorkspaceFolderAddError::WorkspaceNotFound`] when the target
/// workspace no longer exists, or [`WorkspaceFolderAddError::DuplicateFolder`]
/// when the target workspace already contains the same canonical or fallback
/// folder identity.
pub fn add_folder_to_workspace(
    file: &mut WorkspacesFile,
    ws_id: &WorkspaceId,
    folder_path: PathBuf,
) -> std::result::Result<WorkspaceFolderId, WorkspaceFolderAddError> {
    let existing_paths = file
        .workspace(ws_id)
        .ok_or(WorkspaceFolderAddError::WorkspaceNotFound)?
        .folder_paths();
    let folder_identity = folder_identity(&folder_path);
    let existing_identities = folder_identities(&existing_paths);
    add_folder_to_workspace_with_identities(
        file,
        ws_id,
        folder_path,
        &existing_paths,
        &folder_identity,
        &existing_identities,
    )
}

/// Add a folder using identities resolved outside the GTK main thread.
///
/// # Errors
///
/// Returns [`WorkspaceFolderAddError::WorkspaceNotFound`] when the workspace is
/// gone, [`WorkspaceFolderAddError::StaleFolderSnapshot`] when the caller's
/// background identity snapshot no longer matches current state, or
/// [`WorkspaceFolderAddError::DuplicateFolder`] for a duplicate folder identity
/// inside the same workspace.
pub fn add_folder_to_workspace_with_identities(
    file: &mut WorkspacesFile,
    ws_id: &WorkspaceId,
    folder_path: PathBuf,
    existing_paths: &[PathBuf],
    folder_identity: &WorkspaceFolderIdentity,
    existing_identities: &[WorkspaceFolderIdentity],
) -> std::result::Result<WorkspaceFolderId, WorkspaceFolderAddError> {
    let workspace = file
        .workspace(ws_id)
        .ok_or(WorkspaceFolderAddError::WorkspaceNotFound)?;
    if workspace.folder_paths() != existing_paths
        || existing_paths.len() != existing_identities.len()
        || folder_identity.configured_path != folder_path
    {
        return Err(WorkspaceFolderAddError::StaleFolderSnapshot);
    }

    if existing_identities
        .iter()
        .any(|existing| folder_identities_match(existing, folder_identity))
    {
        return Err(WorkspaceFolderAddError::DuplicateFolder);
    }

    let id = file
        .workspace_mut(ws_id)
        .ok_or(WorkspaceFolderAddError::WorkspaceNotFound)?
        .add_folder(folder_path);
    Ok(id)
}

/// Remove one folder membership from a workspace without touching filesystem data.
///
/// # Errors
///
/// Returns [`WorkspaceFolderRemoveError::WorkspaceNotFound`] when the workspace
/// is gone, or [`WorkspaceFolderRemoveError::FolderNotFound`] when the stable
/// folder id is no longer present in that workspace.
pub fn remove_folder_from_workspace(
    file: &mut WorkspacesFile,
    ws_id: &WorkspaceId,
    folder_id: &WorkspaceFolderId,
) -> std::result::Result<PathBuf, WorkspaceFolderRemoveError> {
    let workspace = file
        .workspace_mut(ws_id)
        .ok_or(WorkspaceFolderRemoveError::WorkspaceNotFound)?;
    let removed = workspace
        .remove_folder(folder_id)
        .ok_or(WorkspaceFolderRemoveError::FolderNotFound)?;
    Ok(removed.path)
}

/// Reorder one folder membership to an absolute index inside a workspace.
///
/// This command mutates only the provided in-memory state. UI callers should
/// persist through their existing debounced latest-state-wins pipeline.
///
/// # Errors
///
/// Returns [`WorkspaceFolderReorderError::WorkspaceNotFound`] when the
/// workspace is gone, or [`WorkspaceFolderReorderError::FolderNotFound`] when
/// the stable folder id is no longer present in that workspace.
pub fn reorder_folder_in_workspace(
    file: &mut WorkspacesFile,
    ws_id: &WorkspaceId,
    folder_id: &WorkspaceFolderId,
    new_index: usize,
) -> std::result::Result<(), WorkspaceFolderReorderError> {
    let workspace = file
        .workspace_mut(ws_id)
        .ok_or(WorkspaceFolderReorderError::WorkspaceNotFound)?;
    if !workspace
        .folders
        .iter()
        .any(|folder| &folder.id == folder_id)
    {
        return Err(WorkspaceFolderReorderError::FolderNotFound);
    }

    workspace.move_folder(folder_id, new_index);
    Ok(())
}

/// Move one folder membership by one slot inside a workspace.
///
/// # Errors
///
/// Returns [`WorkspaceFolderReorderError::WorkspaceNotFound`] when the
/// workspace is gone, [`WorkspaceFolderReorderError::FolderNotFound`] when the
/// folder id is absent, or [`WorkspaceFolderReorderError::AlreadyAtBoundary`]
/// when the requested move would leave the folder in the same position.
pub fn move_folder_in_workspace(
    file: &mut WorkspacesFile,
    ws_id: &WorkspaceId,
    folder_id: &WorkspaceFolderId,
    direction: WorkspaceFolderMoveDirection,
) -> std::result::Result<(), WorkspaceFolderReorderError> {
    let workspace = file
        .workspace(ws_id)
        .ok_or(WorkspaceFolderReorderError::WorkspaceNotFound)?;
    let index = workspace
        .folders
        .iter()
        .position(|folder| &folder.id == folder_id)
        .ok_or(WorkspaceFolderReorderError::FolderNotFound)?;
    let new_index = match direction {
        WorkspaceFolderMoveDirection::Up if index > 0 => index - 1,
        WorkspaceFolderMoveDirection::Down if index + 1 < workspace.folders.len() => index + 1,
        WorkspaceFolderMoveDirection::Up | WorkspaceFolderMoveDirection::Down => {
            return Err(WorkspaceFolderReorderError::AlreadyAtBoundary);
        }
    };

    reorder_folder_in_workspace(file, ws_id, folder_id, new_index)
}

/// Resolve one configured folder's identity, using canonical path when available.
#[must_use]
pub fn folder_identity(path: &Path) -> WorkspaceFolderIdentity {
    let canonical_path = fs_metadata::canonical_path(path).ok();
    WorkspaceFolderIdentity {
        configured_path: path.to_path_buf(),
        canonical_path,
    }
}

/// Resolve identities for the current folder list as one background-friendly batch.
#[must_use]
pub fn folder_identities(paths: &[PathBuf]) -> Vec<WorkspaceFolderIdentity> {
    paths.iter().map(|path| folder_identity(path)).collect()
}

fn folder_identities_match(
    left: &WorkspaceFolderIdentity,
    right: &WorkspaceFolderIdentity,
) -> bool {
    left.candidates()
        .any(|left| right.candidates().any(|right| left == right))
}

/// Load workspaces from disk, returning default state with diagnostics if needed.
///
/// # Errors
///
/// This compatibility wrapper currently returns recovered state. Use
/// [`load_recovering`] when diagnostics matter to the caller.
pub fn load(data_dir: &Path) -> Result<WorkspacesFile> {
    Ok(load_recovering(data_dir).value)
}

/// Load workspaces through recovery-aware v1 envelope handling.
#[must_use]
pub fn load_recovering(data_dir: &Path) -> RecoveryLoad<WorkspacesFile> {
    let path = data_dir.join(WORKSPACES_FILE);
    let mut load: RecoveryLoad<WorkspacesFile> = load_enveloped_json_or_default(
        &RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::WorkspaceState),
        KIND_WORKSPACE_STATE,
    );
    load.value.normalize_scope();
    load
}

#[cfg(test)]
fn trace_recovery_diagnostics(load: &RecoveryLoad<WorkspacesFile>) {
    for diagnostic in &load.diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
}

/// Save workspaces to disk.
///
/// # Errors
///
/// Returns an error if the workspace file cannot be serialized or written.
pub fn save(data_dir: &Path, file: &WorkspacesFile) -> Result<()> {
    let path = data_dir.join(WORKSPACES_FILE);
    let config = RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::WorkspaceState);
    let diagnostics = save_enveloped_json_path(&config, KIND_WORKSPACE_STATE, file)?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::workspace::{
        WorkspaceConfig, WorkspaceFolder, WorkspaceFolderId, WorkspaceFolderMoveDirection,
        WorkspaceId, WorkspaceScope,
    };
    use crate::services::filesystem::fixture;
    use crate::services::json_format::{JsonEnvelopeRef, KIND_SESSION};
    use crate::services::recovery_metadata::{
        RecoveryLoadOutcome, RecoveryPreservation, RecoveryProblem,
    };
    use tempfile::TempDir;

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let result = load(dir.path()).expect("expected operation to succeed");
        assert!(result.workspaces.is_empty());
        assert_eq!(result.current_scope, WorkspaceScope::All);
    }

    #[test]
    fn test_load_non_file_workspace_path_returns_error() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&dir.path().join("workspaces.json"));

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.workspaces.is_empty());
        assert!(!loaded.replacement_allowed());
        assert_eq!(loaded.outcome, RecoveryLoadOutcome::PreservedDefault);
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::UnsupportedFileKind { .. }
        ));
        assert!(matches!(
            loaded.diagnostics[0].preservation,
            RecoveryPreservation::PreservedInPlace
        ));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("my workspace", "/home/user/project".into());
        file.set_current_scope(WorkspaceScope::workspace(workspace_id.clone()));

        save(dir.path(), &file).expect("expected operation to succeed");
        let saved_json = fixture::read_text(&dir.path().join(WORKSPACES_FILE));
        assert!(
            saved_json.contains("\"folders\""),
            "workspace state should save the new folder-set payload"
        );
        assert!(
            !saved_json.contains("\"root\""),
            "workspace state should not write the legacy `root` field"
        );

        let loaded = load_recovering(dir.path());
        trace_recovery_diagnostics(&loaded);
        let loaded = loaded.value;

        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].name, "my workspace");
        assert_eq!(
            loaded.workspaces[0].folder_paths(),
            vec![PathBuf::from("/home/user/project")]
        );
        assert_eq!(
            loaded.current_scope(),
            WorkspaceScope::workspace(workspace_id)
        );
    }

    #[test]
    fn test_load_new_folder_set_payload_restores_folder_order_and_ids() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &dir.path().join("workspaces.json"),
            &serde_json::to_string_pretty(&JsonEnvelopeRef::new(
                KIND_WORKSPACE_STATE,
                &serde_json::json!({
                    "current_scope": { "kind": "workspace", "workspace_id": "existing" },
                    "workspaces": [{
                        "id": "existing",
                        "name": "Existing",
                        "folders": [
                            { "id": "folder-a", "path": "/tmp/one" },
                            { "id": "folder-b", "path": "/tmp/two" }
                        ]
                    }]
                }),
            ))
            .expect("workspace fixture"),
        );

        let loaded = load(dir.path()).expect("expected operation to succeed");

        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(
            loaded.workspaces[0].folder_paths(),
            vec![
                Path::new("/tmp/one").to_path_buf(),
                Path::new("/tmp/two").to_path_buf(),
            ]
        );
        assert_eq!(loaded.workspaces[0].folders[0].id.as_str(), "folder-a");
        assert_eq!(loaded.workspaces[0].folders[1].id.as_str(), "folder-b");
        assert_eq!(
            loaded.current_scope(),
            WorkspaceScope::workspace(WorkspaceId::new("existing"))
        );
    }

    #[test]
    fn test_load_v1_legacy_single_folder_payload_is_preserved_and_reset() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &dir.path().join(WORKSPACES_FILE),
            &serde_json::to_string_pretty(&JsonEnvelopeRef::new(
                KIND_WORKSPACE_STATE,
                &serde_json::json!({
                    "current_scope": { "kind": "workspace", "workspace_id": "existing" },
                    "workspaces": [{
                        "id": "existing",
                        "name": "Existing",
                        "root": "/tmp/existing"
                    }]
                }),
            ))
            .expect("workspace fixture"),
        );

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.workspaces.is_empty());
        assert_eq!(loaded.value.current_scope(), WorkspaceScope::All);
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::UnsupportedFormat { .. }
        ));
        assert!(
            loaded.diagnostics[0]
                .preservation
                .quarantine_path()
                .is_some(),
            "legacy single-folder payload should be preserved before defaults are used"
        );
    }

    #[test]
    fn test_save_and_load_empty_workspace_folder_set() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_empty_workspace("empty");

        save(dir.path(), &file).expect("save empty workspace");
        let saved_json = fixture::read_text(&dir.path().join(WORKSPACES_FILE));
        assert!(saved_json.contains("\"folders\": []"));
        assert!(!saved_json.contains("\"root\""));

        let loaded = load(dir.path()).expect("load empty workspace");

        assert_eq!(loaded.workspaces.len(), 1);
        assert!(loaded.workspaces[0].folders.is_empty());
        assert_eq!(
            loaded.current_scope(),
            WorkspaceScope::workspace(workspace_id)
        );
    }

    #[test]
    fn test_save_preserves_explicit_folder_ids_and_order() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("existing")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("existing"),
                "Existing",
                vec![
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("folder-a"), "/tmp/a".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("folder-b"), "/tmp/b".into()),
                ],
            )],
        };

        save(dir.path(), &file).expect("save folder-set workspace");
        let loaded = load(dir.path()).expect("load folder-set workspace");

        assert_eq!(loaded.workspaces[0].folders[0].id.as_str(), "folder-a");
        assert_eq!(loaded.workspaces[0].folders[1].id.as_str(), "folder-b");
        assert_eq!(
            loaded.workspaces[0].folder_paths(),
            vec![
                Path::new("/tmp/a").to_path_buf(),
                Path::new("/tmp/b").to_path_buf()
            ]
        );
    }

    #[test]
    fn add_folder_rejects_duplicate_canonical_path_inside_one_workspace() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let project = dir.path().join("project");
        let alias = dir.path().join("alias");
        fixture::create_dir(&project);
        fixture::symlink(&project, &alias);

        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("project", project.clone());

        let result = add_folder_to_workspace(&mut file, &workspace_id, alias);

        assert_eq!(result, Err(WorkspaceFolderAddError::DuplicateFolder));
        assert_eq!(file.workspaces[0].folder_paths(), vec![project]);
    }

    #[test]
    fn add_folder_allows_same_canonical_path_in_another_workspace() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let project = dir.path().join("project");
        fixture::create_dir(&project);

        let mut file = WorkspacesFile::default();
        let first = file.add_workspace("first", project.clone());
        let second = file.add_empty_workspace("second");

        let result = add_folder_to_workspace(&mut file, &second, project.clone());

        assert!(result.is_ok());
        assert_eq!(
            file.workspace(&first).expect("first").folder_paths(),
            vec![project.clone()]
        );
        assert_eq!(
            file.workspace(&second).expect("second").folder_paths(),
            vec![project]
        );
    }

    #[test]
    fn add_folder_allows_parent_and_child_overlap() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let project = dir.path().join("project");
        let child = project.join("src");
        fixture::create_dir_all(&child);

        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("project", project.clone());

        let result = add_folder_to_workspace(&mut file, &workspace_id, child.clone());

        assert!(result.is_ok());
        assert_eq!(
            file.workspace(&workspace_id)
                .expect("workspace")
                .folder_paths(),
            vec![project, child]
        );
    }

    #[test]
    fn add_folder_rejects_literal_duplicate_when_canonicalization_fails() {
        let missing = PathBuf::from("/tmp/lushtext-missing-workspace-folder");
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("missing", missing.clone());

        let result = add_folder_to_workspace(&mut file, &workspace_id, missing.clone());

        assert_eq!(result, Err(WorkspaceFolderAddError::DuplicateFolder));
        assert_eq!(
            file.workspace(&workspace_id)
                .expect("workspace")
                .folder_paths(),
            vec![missing]
        );
    }

    #[test]
    fn add_folder_rechecks_canonical_duplicate_after_missing_folder_appears() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let project = dir.path().join("project");
        let alias = dir.path().join("alias");
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("project", project.clone());

        fixture::create_dir(&project);
        fixture::symlink(&project, &alias);

        let result = add_folder_to_workspace(&mut file, &workspace_id, alias);

        assert_eq!(result, Err(WorkspaceFolderAddError::DuplicateFolder));
        assert_eq!(
            file.workspace(&workspace_id)
                .expect("workspace")
                .folder_paths(),
            vec![project]
        );
    }

    #[test]
    fn add_folder_reports_missing_workspace_without_mutating_state() {
        let mut file = WorkspacesFile::default();

        let result = add_folder_to_workspace(
            &mut file,
            &WorkspaceId::new("missing"),
            "/tmp/project".into(),
        );

        assert_eq!(result, Err(WorkspaceFolderAddError::WorkspaceNotFound));
        assert!(file.workspaces.is_empty());
    }

    #[test]
    fn add_folder_with_stale_identity_snapshot_refuses_to_mutate() {
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("project", "/tmp/project".into());
        let existing_paths = vec![PathBuf::from("/tmp/old")];
        let existing_identities = folder_identities(&existing_paths);
        let folder_path = PathBuf::from("/tmp/other");
        let folder_identity = folder_identity(&folder_path);

        let result = add_folder_to_workspace_with_identities(
            &mut file,
            &workspace_id,
            folder_path,
            &existing_paths,
            &folder_identity,
            &existing_identities,
        );

        assert_eq!(result, Err(WorkspaceFolderAddError::StaleFolderSnapshot));
        assert_eq!(
            file.workspace(&workspace_id)
                .expect("workspace")
                .folder_paths(),
            vec![PathBuf::from("/tmp/project")]
        );
    }

    #[test]
    fn add_folder_with_stale_identity_count_or_target_refuses_to_mutate() {
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("project", "/tmp/project".into());
        let existing_paths = vec![PathBuf::from("/tmp/project")];
        let folder_path = PathBuf::from("/tmp/new");
        let new_folder_identity = folder_identity(&folder_path);

        let stale_count = add_folder_to_workspace_with_identities(
            &mut file,
            &workspace_id,
            folder_path.clone(),
            &existing_paths,
            &new_folder_identity,
            &[],
        );
        assert_eq!(
            stale_count,
            Err(WorkspaceFolderAddError::StaleFolderSnapshot)
        );

        let mismatched_identity = folder_identity(Path::new("/tmp/other"));
        let existing_identities = folder_identities(&existing_paths);
        let stale_target = add_folder_to_workspace_with_identities(
            &mut file,
            &workspace_id,
            folder_path,
            &existing_paths,
            &mismatched_identity,
            &existing_identities,
        );
        assert_eq!(
            stale_target,
            Err(WorkspaceFolderAddError::StaleFolderSnapshot)
        );
        assert_eq!(
            file.workspace(&workspace_id)
                .expect("workspace")
                .folder_paths(),
            vec![PathBuf::from("/tmp/project")]
        );
    }

    #[test]
    fn remove_folder_removes_membership_without_removing_workspace() {
        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("ws"),
                "Project",
                vec![
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("one"), "/tmp/one".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("two"), "/tmp/two".into()),
                ],
            )],
        };

        let removed = remove_folder_from_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("one"),
        )
        .expect("remove folder");

        assert_eq!(removed, PathBuf::from("/tmp/one"));
        assert_eq!(file.workspaces.len(), 1);
        assert_eq!(
            file.workspace(&WorkspaceId::new("ws"))
                .expect("workspace")
                .folder_paths(),
            vec![PathBuf::from("/tmp/two")]
        );
        assert_eq!(
            file.current_scope(),
            WorkspaceScope::workspace(WorkspaceId::new("ws"))
        );
    }

    #[test]
    fn remove_last_folder_keeps_empty_workspace_selected() {
        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("ws"),
                "Project",
                vec![WorkspaceFolder::with_id(
                    WorkspaceFolderId::new("only"),
                    "/tmp/only".into(),
                )],
            )],
        };

        remove_folder_from_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("only"),
        )
        .expect("remove only folder");

        let workspace = file.workspace(&WorkspaceId::new("ws")).expect("workspace");
        assert!(workspace.folders.is_empty());
        assert_eq!(
            file.current_scope(),
            WorkspaceScope::workspace(WorkspaceId::new("ws"))
        );
    }

    #[test]
    fn remove_folder_reports_missing_targets_without_mutating_state() {
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("project", "/tmp/project".into());
        let original = file.clone();

        let missing_folder = remove_folder_from_workspace(
            &mut file,
            &workspace_id,
            &WorkspaceFolderId::new("missing"),
        );
        assert_eq!(
            missing_folder,
            Err(WorkspaceFolderRemoveError::FolderNotFound)
        );
        assert_eq!(file, original);

        let missing_workspace = remove_folder_from_workspace(
            &mut file,
            &WorkspaceId::new("missing"),
            &WorkspaceFolderId::new("missing"),
        );
        assert_eq!(
            missing_workspace,
            Err(WorkspaceFolderRemoveError::WorkspaceNotFound)
        );
        assert_eq!(file, original);
    }

    #[test]
    fn reorder_folder_moves_membership_by_stable_id() {
        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("ws"),
                "Project",
                vec![
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("one"), "/tmp/one".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("two"), "/tmp/two".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("three"), "/tmp/three".into()),
                ],
            )],
        };

        reorder_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("three"),
            0,
        )
        .expect("reorder folder");

        assert_eq!(
            file.workspace(&WorkspaceId::new("ws"))
                .expect("workspace")
                .folder_paths(),
            vec![
                PathBuf::from("/tmp/three"),
                PathBuf::from("/tmp/one"),
                PathBuf::from("/tmp/two")
            ]
        );
    }

    #[test]
    fn reorder_folder_reports_missing_folder_without_mutating_state() {
        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("ws"),
                "Project",
                vec![WorkspaceFolder::with_id(
                    WorkspaceFolderId::new("one"),
                    "/tmp/one".into(),
                )],
            )],
        };
        let original = file.clone();

        let result = reorder_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("missing"),
            0,
        );

        assert_eq!(result, Err(WorkspaceFolderReorderError::FolderNotFound));
        assert_eq!(file, original);
    }

    #[test]
    fn move_folder_in_workspace_uses_relative_direction() {
        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("ws"),
                "Project",
                vec![
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("one"), "/tmp/one".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("two"), "/tmp/two".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("three"), "/tmp/three".into()),
                ],
            )],
        };

        move_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("three"),
            WorkspaceFolderMoveDirection::Up,
        )
        .expect("move folder up");

        assert_eq!(
            file.workspace(&WorkspaceId::new("ws"))
                .expect("workspace")
                .folder_paths(),
            vec![
                PathBuf::from("/tmp/one"),
                PathBuf::from("/tmp/three"),
                PathBuf::from("/tmp/two")
            ]
        );
    }

    #[test]
    fn reorder_folder_paths_does_not_mutate_filesystem_content() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let first_folder = dir.path().join("first");
        let second_folder = dir.path().join("second");
        fixture::create_dir_all(&first_folder);
        fixture::create_dir_all(&second_folder);
        let first_sentinel = first_folder.join("sentinel.txt");
        let second_sentinel = second_folder.join("sentinel.txt");
        fixture::write_text(&first_sentinel, "first stays put\n");
        fixture::write_text(&second_sentinel, "second stays put\n");

        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("ws"),
                "Project",
                vec![
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("first"), first_folder.clone()),
                    WorkspaceFolder::with_id(
                        WorkspaceFolderId::new("second"),
                        second_folder.clone(),
                    ),
                ],
            )],
        };

        reorder_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("second"),
            0,
        )
        .expect("absolute reorder");
        move_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("second"),
            WorkspaceFolderMoveDirection::Down,
        )
        .expect("relative reorder");

        assert_eq!(fixture::read_text(&first_sentinel), "first stays put\n");
        assert_eq!(fixture::read_text(&second_sentinel), "second stays put\n");
        assert!(fs_metadata::exists(&first_folder));
        assert!(fs_metadata::exists(&second_folder));
        assert_eq!(
            file.workspace(&WorkspaceId::new("ws"))
                .expect("workspace")
                .folder_paths(),
            vec![first_folder, second_folder]
        );
    }

    #[test]
    fn move_folder_reports_boundaries_and_missing_targets_without_mutating_state() {
        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("ws"),
                "Project",
                vec![
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("one"), "/tmp/one".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("two"), "/tmp/two".into()),
                ],
            )],
        };
        let original = file.clone();

        let first_up = move_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("one"),
            WorkspaceFolderMoveDirection::Up,
        );
        assert_eq!(
            first_up,
            Err(WorkspaceFolderReorderError::AlreadyAtBoundary)
        );
        assert_eq!(file, original);

        let last_down = move_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("two"),
            WorkspaceFolderMoveDirection::Down,
        );
        assert_eq!(
            last_down,
            Err(WorkspaceFolderReorderError::AlreadyAtBoundary)
        );
        assert_eq!(file, original);

        let missing_folder = move_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("missing"),
            WorkspaceFolderMoveDirection::Down,
        );
        assert_eq!(
            missing_folder,
            Err(WorkspaceFolderReorderError::FolderNotFound)
        );
        assert_eq!(file, original);

        let missing_workspace = move_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("missing"),
            &WorkspaceFolderId::new("one"),
            WorkspaceFolderMoveDirection::Down,
        );
        assert_eq!(
            missing_workspace,
            Err(WorkspaceFolderReorderError::WorkspaceNotFound)
        );
        assert_eq!(file, original);
    }

    #[test]
    fn save_after_folder_reorder_restores_newest_order() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(WorkspaceId::new("ws")),
            workspaces: vec![WorkspaceConfig::with_folders(
                WorkspaceId::new("ws"),
                "Project",
                vec![
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("one"), "/tmp/one".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("two"), "/tmp/two".into()),
                    WorkspaceFolder::with_id(WorkspaceFolderId::new("three"), "/tmp/three".into()),
                ],
            )],
        };

        move_folder_in_workspace(
            &mut file,
            &WorkspaceId::new("ws"),
            &WorkspaceFolderId::new("three"),
            WorkspaceFolderMoveDirection::Up,
        )
        .expect("move folder");
        save(dir.path(), &file).expect("save reordered folders");

        let loaded = load(dir.path()).expect("load reordered folders");

        assert_eq!(
            loaded
                .workspace(&WorkspaceId::new("ws"))
                .expect("workspace")
                .folder_paths(),
            vec![
                PathBuf::from("/tmp/one"),
                PathBuf::from("/tmp/three"),
                PathBuf::from("/tmp/two")
            ]
        );
    }

    #[test]
    fn save_after_rapid_workspace_mutation_sequence_restores_latest_state() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let folders_dir = TempDir::new().expect("expected operation to succeed");
        let one = folders_dir.path().join("one");
        let two = folders_dir.path().join("two");
        let three = folders_dir.path().join("three");
        let four = folders_dir.path().join("four");
        let beta_folder = folders_dir.path().join("beta");
        for path in [&one, &two, &three, &four, &beta_folder] {
            fixture::create_dir(path);
        }

        let alpha = WorkspaceId::new("alpha");
        let beta = WorkspaceId::new("beta");
        let mut file = WorkspacesFile {
            current_scope: WorkspaceScope::workspace(alpha.clone()),
            workspaces: vec![
                WorkspaceConfig::with_folders(
                    alpha.clone(),
                    "Alpha",
                    vec![
                        WorkspaceFolder::with_id(WorkspaceFolderId::new("one"), one),
                        WorkspaceFolder::with_id(WorkspaceFolderId::new("two"), two.clone()),
                        WorkspaceFolder::with_id(WorkspaceFolderId::new("three"), three.clone()),
                    ],
                ),
                WorkspaceConfig::with_folders(
                    beta.clone(),
                    "Beta",
                    vec![WorkspaceFolder::with_id(
                        WorkspaceFolderId::new("beta-folder"),
                        beta_folder,
                    )],
                ),
            ],
        };

        let added_id =
            add_folder_to_workspace(&mut file, &alpha, four.clone()).expect("add folder");
        remove_folder_from_workspace(&mut file, &alpha, &WorkspaceFolderId::new("one"))
            .expect("remove folder");
        reorder_folder_in_workspace(&mut file, &alpha, &added_id, 0).expect("reorder folder");
        file.rename_workspace(&alpha, "Latest Alpha");
        file.set_current_scope(WorkspaceScope::workspace(beta.clone()));
        file.remove_workspace(&beta);

        save(dir.path(), &file).expect("save rapidly mutated workspace state");
        let loaded = load(dir.path()).expect("load latest rapidly mutated workspace state");

        assert_eq!(loaded.workspaces.len(), 1);
        let workspace = loaded.workspace(&alpha).expect("alpha workspace");
        assert_eq!(workspace.name, "Latest Alpha");
        assert_eq!(loaded.current_scope(), WorkspaceScope::All);
        assert_eq!(
            workspace
                .folders
                .iter()
                .map(|folder| folder.id.as_str())
                .collect::<Vec<_>>(),
            vec![added_id.as_str(), "two", "three"]
        );
        assert_eq!(
            workspace.folder_paths(),
            vec![four, two, three],
            "reload should reflect the final mutation order, not an earlier snapshot"
        );
    }

    #[test]
    fn test_load_rejects_legacy_entries_workspace_payload() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &dir.path().join("workspaces.json"),
            &serde_json::json!({
                "active_workspace": "legacy",
                "workspaces": [{
                    "id": "legacy",
                    "name": "Legacy",
                    "entries": [
                        { "kind": "directory", "path": "/tmp/one" },
                        { "kind": "directory", "path": "/tmp/two" }
                    ]
                }]
            })
            .to_string(),
        );

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.workspaces.is_empty());
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::UnsupportedFormat { .. }
        ));
        let quarantine_path = loaded.diagnostics[0]
            .preservation
            .quarantine_path()
            .expect("quarantined unsupported workspace");
        assert!(fixture::read_text(quarantine_path).contains("active_workspace"));
    }

    #[test]
    fn test_load_preserves_wrong_kind_workspace_envelope() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &dir.path().join(WORKSPACES_FILE),
            &serde_json::to_string_pretty(&JsonEnvelopeRef::new(
                KIND_SESSION,
                &serde_json::json!({ "tabs": [] }),
            ))
            .expect("wrong-kind workspace fixture"),
        );

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.workspaces.is_empty());
        assert_eq!(loaded.outcome, RecoveryLoadOutcome::QuarantinedDefault);
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::UnsupportedFormat { .. }
        ));
        let quarantine_path = loaded.diagnostics[0]
            .preservation
            .quarantine_path()
            .expect("wrong-kind workspace evidence should be quarantined");
        assert!(fixture::read_text(quarantine_path).contains(KIND_SESSION));
    }

    #[test]
    fn test_load_preserves_future_workspace_envelope_version() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &dir.path().join(WORKSPACES_FILE),
            &serde_json::json!({
                "kind": KIND_WORKSPACE_STATE,
                "version": 2,
                "data": {
                    "current_scope": { "kind": "all" },
                    "workspaces": []
                }
            })
            .to_string(),
        );

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.workspaces.is_empty());
        assert_eq!(loaded.outcome, RecoveryLoadOutcome::QuarantinedDefault);
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::UnsupportedVersion { version: 2, .. }
        ));
        let quarantine_path = loaded.diagnostics[0]
            .preservation
            .quarantine_path()
            .expect("future workspace evidence should be quarantined");
        assert!(fixture::read_text(quarantine_path).contains("\"version\":2"));
    }

    #[test]
    fn test_load_preserves_malformed_workspace_json() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join(WORKSPACES_FILE), "{ not valid json");

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.workspaces.is_empty());
        assert_eq!(loaded.outcome, RecoveryLoadOutcome::QuarantinedDefault);
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::Malformed { .. }
        ));
        let quarantine_path = loaded.diagnostics[0]
            .preservation
            .quarantine_path()
            .expect("malformed workspace evidence should be quarantined");
        assert_eq!(fixture::read_text(quarantine_path), "{ not valid json");
    }

    #[test]
    fn test_load_falls_back_to_all_scope_when_target_is_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &dir.path().join("workspaces.json"),
            &serde_json::to_string_pretty(&JsonEnvelopeRef::new(
                KIND_WORKSPACE_STATE,
                &serde_json::json!({
                    "current_scope": { "kind": "workspace", "workspace_id": "missing" },
                    "workspaces": [{
                        "id": "existing",
                        "name": "Existing",
                        "folders": []
                    }]
                }),
            ))
            .expect("workspace fixture"),
        );

        let loaded = load(dir.path()).expect("expected operation to succeed");
        assert_eq!(loaded.current_scope(), WorkspaceScope::All);
    }

    #[test]
    fn save_quarantines_unsupported_workspace_before_replacement() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join(WORKSPACES_FILE), r#"{"workspaces":[]}"#);

        save(dir.path(), &WorkspacesFile::default()).expect("save v1 workspace");

        let quarantine_dir = dir
            .path()
            .join(crate::services::recovery_metadata::QUARANTINE_DIR);
        let quarantine_entries = crate::services::filesystem::tree::scan_directory(
            &quarantine_dir,
            crate::services::filesystem::DirectoryScanPolicy::visible_workspace(),
        )
        .expect("quarantine entries");
        assert_eq!(quarantine_entries.len(), 1);
        let loaded = load_recovering(dir.path());
        assert!(loaded.diagnostics.is_empty());
    }
}

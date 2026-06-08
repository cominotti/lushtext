// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace model — persisted folder-set workspaces plus the current scope.
//!
//! This module stays framework-free and defines the durable workspace contract
//! that the sidebar shell, search, palette, and note workflows all share.

use serde::{Deserialize, Deserializer, Serialize};
use std::path::{Path, PathBuf};

use crate::model::sidecar_identity::stable_bytes_hash;

/// Stable identifier for a workspace.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    /// Build a workspace identifier from stored or generated text.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the underlying identifier string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return whether this identifier is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Stable identifier for one configured folder inside a workspace.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceFolderId(String);

impl WorkspaceFolderId {
    /// Build a workspace-folder identifier from stored or generated text.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the underlying identifier string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return whether this identifier is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Derive a repeatable folder id when loading the old single-folder payload.
    #[must_use]
    fn from_legacy_payload(workspace_id: &WorkspaceId, folder_path: &Path) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(workspace_id.as_str().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(folder_path.to_string_lossy().as_bytes());
        Self(format!("migrated-folder-{}", stable_bytes_hash(&bytes)))
    }
}

/// App-wide workspace scope shared by the sidebar and workspace-aware features.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "workspace_id", rename_all = "snake_case")]
pub enum WorkspaceScope {
    /// Aggregate scope spanning every restored workspace.
    #[default]
    All,
    /// Scope narrowed to one specific workspace.
    Workspace(WorkspaceId),
}

impl WorkspaceScope {
    /// Build a scope targeting one concrete workspace.
    #[must_use]
    pub fn workspace(workspace_id: WorkspaceId) -> Self {
        Self::Workspace(workspace_id)
    }

    /// Return whether this scope is the explicit aggregate scope.
    #[must_use]
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    /// Borrow the concrete workspace id when this scope targets one workspace.
    #[must_use]
    pub fn workspace_id(&self) -> Option<&WorkspaceId> {
        match self {
            Self::All => None,
            Self::Workspace(workspace_id) => Some(workspace_id),
        }
    }

    /// Return whether this scope includes the given workspace id.
    #[must_use]
    pub fn includes_workspace(&self, workspace_id: &WorkspaceId) -> bool {
        self.is_all() || self.workspace_id() == Some(workspace_id)
    }
}

/// One configured folder that belongs to a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceFolder {
    /// Stable folder id used by UI reorder/remove operations.
    pub id: WorkspaceFolderId,
    /// Configured folder path persisted for this workspace membership.
    pub path: PathBuf,
}

impl WorkspaceFolder {
    /// Create a folder membership with a freshly generated id.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self::with_id(WorkspaceFolderId::new(generate_id()), path)
    }

    /// Create a folder membership with a caller-provided stable id.
    #[must_use]
    pub fn with_id(id: WorkspaceFolderId, path: PathBuf) -> Self {
        Self { id, path }
    }

    /// Borrow the configured folder path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Relative direction for moving one workspace folder in its ordered set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFolderMoveDirection {
    /// Move the folder one position earlier.
    Up,
    /// Move the folder one position later.
    Down,
}

/// Section-local row seed used while building workspace folder trees.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FolderTreeEntry {
    /// Directory row seed used by configured folders and drill-down views.
    Directory { path: PathBuf },
    /// Standalone file row seed used by older sidebar fixtures and tree setup.
    File { path: PathBuf },
}

impl FolderTreeEntry {
    /// Borrow the filesystem path behind this tree row seed.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            FolderTreeEntry::Directory { path } | FolderTreeEntry::File { path } => path,
        }
    }

    /// Return whether this row seed points at a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        matches!(self, FolderTreeEntry::Directory { .. })
    }
}

/// One persisted workspace containing an ordered set of folders.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    /// Stable identifier used by persisted scope selection and callbacks.
    pub id: WorkspaceId,
    /// User-visible workspace label shown in the selector and section header.
    pub name: String,
    /// Ordered folder memberships that define this workspace.
    #[serde(default)]
    pub folders: Vec<WorkspaceFolder>,
}

impl WorkspaceConfig {
    /// Build a workspace with one initial folder.
    #[must_use]
    pub fn with_one_folder(id: WorkspaceId, name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            id,
            name: name.into(),
            folders: vec![WorkspaceFolder::new(path)],
        }
    }

    /// Build a workspace with an explicit folder list.
    #[must_use]
    pub fn with_folders(
        id: WorkspaceId,
        name: impl Into<String>,
        folders: Vec<WorkspaceFolder>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            folders,
        }
    }

    /// Collect this workspace's folder paths in stored order.
    #[must_use]
    pub fn folder_paths(&self) -> Vec<PathBuf> {
        self.folders
            .iter()
            .map(|folder| folder.path.clone())
            .collect()
    }

    /// Append one folder membership and return its stable folder id.
    pub fn add_folder(&mut self, path: PathBuf) -> WorkspaceFolderId {
        let folder = WorkspaceFolder::new(path);
        let id = folder.id.clone();
        self.folders.push(folder);
        id
    }

    /// Remove one folder membership by id.
    pub fn remove_folder(&mut self, folder_id: &WorkspaceFolderId) -> Option<WorkspaceFolder> {
        let index = self
            .folders
            .iter()
            .position(|folder| &folder.id == folder_id)?;
        Some(self.folders.remove(index))
    }

    /// Move one folder membership to a new index within this workspace.
    pub fn move_folder(&mut self, folder_id: &WorkspaceFolderId, new_index: usize) -> bool {
        let Some(old_index) = self
            .folders
            .iter()
            .position(|folder| &folder.id == folder_id)
        else {
            return false;
        };
        let folder = self.folders.remove(old_index);
        let insert_at = new_index.min(self.folders.len());
        self.folders.insert(insert_at, folder);
        old_index != insert_at
    }
}

impl<'de> Deserialize<'de> for WorkspaceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawWorkspaceConfig {
            id: WorkspaceId,
            name: String,
            folders: Option<Vec<WorkspaceFolder>>,
            #[serde(rename = "root")]
            legacy_folder: Option<PathBuf>,
        }

        let raw = RawWorkspaceConfig::deserialize(deserializer)?;
        let folders = match raw.folders {
            Some(folders) => folders,
            None => raw.legacy_folder.map_or_else(Vec::new, |folder_path| {
                vec![WorkspaceFolder::with_id(
                    WorkspaceFolderId::from_legacy_payload(&raw.id, &folder_path),
                    folder_path,
                )]
            }),
        };

        Ok(Self {
            id: raw.id,
            name: raw.name,
            folders,
        })
    }
}

/// Top-level persisted state for all workspaces plus the current scope.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspacesFile {
    /// The user's last explicit workspace scope selection.
    #[serde(default)]
    pub current_scope: WorkspaceScope,
    /// All restored workspaces, each with an ordered folder set.
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,
}

impl WorkspacesFile {
    /// Add a new workspace with one initial folder and select it immediately.
    pub fn add_workspace(&mut self, name: &str, folder_path: PathBuf) -> WorkspaceId {
        let id = WorkspaceId::new(generate_id());
        self.workspaces.push(WorkspaceConfig::with_one_folder(
            id.clone(),
            name,
            folder_path,
        ));
        self.current_scope = WorkspaceScope::workspace(id.clone());
        id
    }

    /// Add a new empty workspace and select it immediately.
    pub fn add_empty_workspace(&mut self, name: &str) -> WorkspaceId {
        let id = WorkspaceId::new(generate_id());
        self.workspaces
            .push(WorkspaceConfig::with_folders(id.clone(), name, Vec::new()));
        self.current_scope = WorkspaceScope::workspace(id.clone());
        id
    }

    /// Remove one workspace by id and fall back to the aggregate scope when needed.
    pub fn remove_workspace(&mut self, ws_id: &WorkspaceId) {
        self.workspaces.retain(|workspace| &workspace.id != ws_id);
        if self.current_scope.workspace_id() == Some(ws_id) {
            self.current_scope = WorkspaceScope::All;
        } else {
            self.normalize_scope();
        }
    }

    /// Rename one workspace and report whether the id was found.
    pub fn rename_workspace(&mut self, ws_id: &WorkspaceId, new_name: &str) -> bool {
        if let Some(workspace) = self
            .workspaces
            .iter_mut()
            .find(|workspace| &workspace.id == ws_id)
        {
            workspace.name = new_name.to_string();
            true
        } else {
            false
        }
    }

    /// Borrow one workspace by id.
    #[must_use]
    pub fn workspace(&self, ws_id: &WorkspaceId) -> Option<&WorkspaceConfig> {
        self.workspaces
            .iter()
            .find(|workspace| &workspace.id == ws_id)
    }

    /// Mutably borrow one workspace by id.
    pub fn workspace_mut(&mut self, ws_id: &WorkspaceId) -> Option<&mut WorkspaceConfig> {
        self.workspaces
            .iter_mut()
            .find(|workspace| &workspace.id == ws_id)
    }

    /// Persist a new current scope, falling back to `All` if the target is gone.
    pub fn set_current_scope(&mut self, scope: WorkspaceScope) {
        self.current_scope = self.normalized_scope(scope);
    }

    /// Return the current scope after applying missing-workspace fallback rules.
    #[must_use]
    pub fn current_scope(&self) -> WorkspaceScope {
        self.normalized_scope(self.current_scope.clone())
    }

    /// Collect every persisted workspace folder in workspace and folder order.
    #[must_use]
    pub fn all_workspace_folder_paths(&self) -> Vec<PathBuf> {
        self.workspaces
            .iter()
            .flat_map(WorkspaceConfig::folder_paths)
            .collect()
    }

    /// Collect the workspace folders covered by the given scope.
    #[must_use]
    pub fn folder_paths_for_scope(&self, scope: &WorkspaceScope) -> Vec<PathBuf> {
        match self.normalized_scope(scope.clone()) {
            WorkspaceScope::All => self.all_workspace_folder_paths(),
            WorkspaceScope::Workspace(workspace_id) => self
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .map_or_else(Vec::new, WorkspaceConfig::folder_paths),
        }
    }

    /// Re-normalize the stored scope after structural mutations.
    pub fn normalize_scope(&mut self) {
        self.current_scope = self.normalized_scope(self.current_scope.clone());
    }

    fn normalized_scope(&self, scope: WorkspaceScope) -> WorkspaceScope {
        match scope {
            WorkspaceScope::Workspace(workspace_id)
                if self
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == workspace_id) =>
            {
                WorkspaceScope::Workspace(workspace_id)
            }
            WorkspaceScope::All | WorkspaceScope::Workspace(_) => WorkspaceScope::All,
        }
    }
}

/// Generate a unique-enough identifier for workspace ids.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should not be earlier than the UNIX epoch")
        .as_nanos();
    format!("{:016x}-{:04x}", nanos, std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_scope_defaults_to_all() {
        assert_eq!(WorkspaceScope::default(), WorkspaceScope::All);
    }

    #[test]
    fn workspace_id_accessors_report_underlying_identifier() {
        let id = WorkspaceId::new("project-1");
        let empty = WorkspaceId::new("");

        assert_eq!(id.as_str(), "project-1");
        assert!(!id.is_empty());
        assert_eq!(empty.as_str(), "");
        assert!(empty.is_empty());
    }

    #[test]
    fn workspace_folder_id_accessors_report_underlying_identifier() {
        let id = WorkspaceFolderId::new("folder-1");
        let empty = WorkspaceFolderId::new("");

        assert_eq!(id.as_str(), "folder-1");
        assert!(!id.is_empty());
        assert_eq!(empty.as_str(), "");
        assert!(empty.is_empty());
    }

    #[test]
    fn workspace_scope_queries_distinguish_all_from_specific_workspace() {
        let first = WorkspaceId::new("first");
        let second = WorkspaceId::new("second");
        let all = WorkspaceScope::All;
        let scoped = WorkspaceScope::workspace(first.clone());

        assert!(all.is_all());
        assert_eq!(all.workspace_id(), None);
        assert!(all.includes_workspace(&first));
        assert!(all.includes_workspace(&second));

        assert!(!scoped.is_all());
        assert_eq!(scoped.workspace_id(), Some(&first));
        assert!(scoped.includes_workspace(&first));
        assert!(!scoped.includes_workspace(&second));
    }

    #[test]
    fn add_workspace_selects_new_workspace() {
        let mut file = WorkspacesFile::default();
        let id = file.add_workspace("project", "/tmp/project".into());

        assert_eq!(file.workspaces.len(), 1);
        assert_eq!(file.current_scope(), WorkspaceScope::workspace(id));
        assert_eq!(
            file.workspaces[0].folder_paths(),
            vec![PathBuf::from("/tmp/project")]
        );
    }

    #[test]
    fn add_empty_workspace_selects_new_workspace_without_fake_folder() {
        let mut file = WorkspacesFile::default();
        let id = file.add_empty_workspace("empty");

        assert_eq!(file.workspaces.len(), 1);
        assert_eq!(file.current_scope(), WorkspaceScope::workspace(id));
        assert!(file.workspaces[0].folders.is_empty());
    }

    #[test]
    fn remove_selected_workspace_falls_back_to_all() {
        let mut file = WorkspacesFile::default();
        let first = file.add_workspace("first", "/tmp/first".into());
        let _second = file.add_workspace("second", "/tmp/second".into());
        file.set_current_scope(WorkspaceScope::workspace(first.clone()));

        file.remove_workspace(&first);

        assert_eq!(file.current_scope(), WorkspaceScope::All);
        assert_eq!(file.workspaces.len(), 1);
    }

    #[test]
    fn set_current_scope_falls_back_to_all_for_missing_workspace() {
        let mut file = WorkspacesFile::default();
        let existing = file.add_workspace("project", "/tmp/project".into());

        file.set_current_scope(WorkspaceScope::workspace(WorkspaceId::new("missing")));
        assert_eq!(file.current_scope(), WorkspaceScope::All);

        file.set_current_scope(WorkspaceScope::workspace(existing.clone()));
        assert_eq!(file.current_scope(), WorkspaceScope::workspace(existing));
    }

    #[test]
    fn remove_and_add_different_folder_creates_distinct_workspace_identity() {
        let mut file = WorkspacesFile::default();
        let old_id = file.add_workspace("old", "/tmp/old".into());

        file.remove_workspace(&old_id);
        let new_id = file.add_workspace("new", "/tmp/new".into());

        assert_ne!(old_id, new_id);
        assert_eq!(file.workspaces.len(), 1);
        assert_eq!(file.workspaces[0].id, new_id);
        assert_eq!(
            file.workspaces[0].folder_paths(),
            vec![PathBuf::from("/tmp/new")]
        );
        assert_eq!(file.workspaces[0].name, "new");
    }

    #[test]
    fn folder_paths_for_scope_returns_selected_workspace_only() {
        let mut file = WorkspacesFile::default();
        let first = file.add_workspace("first", "/tmp/first".into());
        let _second = file.add_workspace("second", "/tmp/second".into());

        let folders = file.folder_paths_for_scope(&WorkspaceScope::workspace(first));

        assert_eq!(folders, vec![PathBuf::from("/tmp/first")]);
    }

    #[test]
    fn folder_paths_for_all_scope_returns_every_folder() {
        let mut file = WorkspacesFile::default();
        let _ = file.add_workspace("first", "/tmp/first".into());
        let _ = file.add_workspace("second", "/tmp/second".into());

        let folders = file.folder_paths_for_scope(&WorkspaceScope::All);

        assert_eq!(
            folders,
            vec![PathBuf::from("/tmp/first"), PathBuf::from("/tmp/second")]
        );
    }

    #[test]
    fn workspace_folder_commands_add_remove_and_move_entries() {
        let mut workspace = WorkspaceConfig::with_folders(
            WorkspaceId::new("workspace"),
            "workspace",
            vec![
                WorkspaceFolder::with_id(WorkspaceFolderId::new("a"), "/tmp/a".into()),
                WorkspaceFolder::with_id(WorkspaceFolderId::new("b"), "/tmp/b".into()),
            ],
        );

        let added = workspace.add_folder("/tmp/c".into());
        assert_eq!(
            workspace.folder_paths(),
            vec![
                PathBuf::from("/tmp/a"),
                PathBuf::from("/tmp/b"),
                PathBuf::from("/tmp/c")
            ]
        );

        assert!(workspace.move_folder(&added, 0));
        assert_eq!(
            workspace.folder_paths(),
            vec![
                PathBuf::from("/tmp/c"),
                PathBuf::from("/tmp/a"),
                PathBuf::from("/tmp/b")
            ]
        );

        let removed = workspace
            .remove_folder(&WorkspaceFolderId::new("a"))
            .expect("remove folder");
        assert_eq!(removed.path, PathBuf::from("/tmp/a"));
        assert_eq!(
            workspace.folder_paths(),
            vec![PathBuf::from("/tmp/c"), PathBuf::from("/tmp/b")]
        );
    }

    #[test]
    fn workspaces_file_borrows_workspace_by_id() {
        let mut file = WorkspacesFile::default();
        let id = file.add_workspace("workspace", "/tmp/workspace".into());

        assert_eq!(
            file.workspace(&id).map(|workspace| &workspace.name),
            Some(&"workspace".to_string())
        );
        file.workspace_mut(&id).expect("workspace").name = "renamed".to_string();
        assert_eq!(
            file.workspace(&id).map(|workspace| &workspace.name),
            Some(&"renamed".to_string())
        );
        assert!(file.workspace(&WorkspaceId::new("missing")).is_none());
    }

    #[test]
    fn workspace_scope_serialization_roundtrip() {
        let scope = WorkspaceScope::workspace(WorkspaceId::new("demo"));
        let json = serde_json::to_string(&scope).expect("expected operation to succeed");
        let restored: WorkspaceScope =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(restored, scope);
    }

    #[test]
    fn folder_tree_entry_serialization_directory() {
        let entry = FolderTreeEntry::Directory {
            path: "/tmp/project".into(),
        };
        let json = serde_json::to_string(&entry).expect("expected operation to succeed");
        let restored: FolderTreeEntry =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(restored, entry);
    }

    #[test]
    fn folder_tree_entry_queries_expose_path_and_kind() {
        let directory = FolderTreeEntry::Directory {
            path: "/tmp/project".into(),
        };
        let file = FolderTreeEntry::File {
            path: "/tmp/project/file.txt".into(),
        };

        assert_eq!(directory.path(), Path::new("/tmp/project"));
        assert!(directory.is_dir());
        assert_eq!(file.path(), Path::new("/tmp/project/file.txt"));
        assert!(!file.is_dir());
    }

    #[test]
    fn remove_unselected_workspace_preserves_valid_current_scope() {
        let mut file = WorkspacesFile::default();
        let first = file.add_workspace("first", "/tmp/first".into());
        let second = file.add_workspace("second", "/tmp/second".into());
        file.set_current_scope(WorkspaceScope::workspace(first.clone()));

        file.remove_workspace(&second);

        assert_eq!(file.current_scope(), WorkspaceScope::workspace(first));
        assert_eq!(file.workspaces.len(), 1);
        assert_eq!(file.workspaces[0].name, "first");
    }

    #[test]
    fn rename_workspace_updates_only_matching_entry() {
        let mut file = WorkspacesFile::default();
        let first = file.add_workspace("first", "/tmp/first".into());
        let second = file.add_workspace("second", "/tmp/second".into());

        file.rename_workspace(&first, "renamed");
        file.rename_workspace(&WorkspaceId::new("missing"), "ignored");

        assert_eq!(file.workspaces[0].name, "renamed");
        assert_eq!(file.workspaces[1].name, "second");
        assert_eq!(file.current_scope(), WorkspaceScope::workspace(second));
    }

    #[test]
    fn normalize_scope_keeps_existing_workspace_and_clears_missing_one() {
        let mut file = WorkspacesFile::default();
        let existing = file.add_workspace("project", "/tmp/project".into());
        file.current_scope = WorkspaceScope::workspace(existing.clone());

        file.normalize_scope();
        assert_eq!(
            file.current_scope,
            WorkspaceScope::workspace(existing.clone())
        );
        assert_eq!(file.current_scope(), WorkspaceScope::workspace(existing));

        file.current_scope = WorkspaceScope::workspace(WorkspaceId::new("missing"));
        file.normalize_scope();
        assert_eq!(file.current_scope, WorkspaceScope::All);
        assert_eq!(file.current_scope(), WorkspaceScope::All);
    }

    #[test]
    fn folder_tree_entry_serialization_file() {
        let entry = FolderTreeEntry::File {
            path: "/tmp/file.txt".into(),
        };
        let json = serde_json::to_string(&entry).expect("expected operation to succeed");
        let restored: FolderTreeEntry =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(restored, entry);
    }

    #[test]
    fn workspace_config_serialization_roundtrip() {
        let config = WorkspaceConfig::with_folders(
            WorkspaceId::new("ws-123"),
            "project",
            vec![WorkspaceFolder::with_id(
                WorkspaceFolderId::new("folder-1"),
                "/tmp/project".into(),
            )],
        );
        let json = serde_json::to_string(&config).expect("expected operation to succeed");
        let restored: WorkspaceConfig =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(restored, config);
        assert!(!json.contains("\"root\""));
    }

    #[test]
    fn workspace_config_deserializes_legacy_single_folder_payload() {
        let restored: WorkspaceConfig = serde_json::from_value(serde_json::json!({
            "id": "legacy",
            "name": "Legacy",
            "root": "/tmp/legacy"
        }))
        .expect("legacy workspace config");

        assert_eq!(restored.id, WorkspaceId::new("legacy"));
        assert_eq!(restored.name, "Legacy");
        assert_eq!(restored.folder_paths(), vec![PathBuf::from("/tmp/legacy")]);
        assert_eq!(
            restored.folders[0].id,
            WorkspaceFolderId::new("migrated-folder-75ab1baeef54db5a")
        );
    }

    #[test]
    fn workspace_config_deserializes_empty_folder_set() {
        let restored: WorkspaceConfig = serde_json::from_value(serde_json::json!({
            "id": "empty",
            "name": "Empty",
            "folders": []
        }))
        .expect("empty workspace config");

        assert_eq!(restored.id, WorkspaceId::new("empty"));
        assert_eq!(restored.name, "Empty");
        assert!(restored.folders.is_empty());
    }

    #[test]
    fn explicit_empty_folder_set_does_not_fall_back_to_legacy_root() {
        let restored: WorkspaceConfig = serde_json::from_value(serde_json::json!({
            "id": "empty",
            "name": "Empty",
            "root": "/tmp/legacy",
            "folders": []
        }))
        .expect("empty workspace config");

        assert!(restored.folders.is_empty());
    }
}

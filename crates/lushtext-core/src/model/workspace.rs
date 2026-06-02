// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace model — persisted single-root workspaces plus the current scope.
//!
//! This module stays framework-free and defines the durable workspace contract
//! that the sidebar shell, search, palette, and note workflows all share.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

/// Legacy or section-local root entry shape used during migration and tree setup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceEntry {
    /// Directory root used by legacy persisted workspaces and drill-down views.
    Directory { path: PathBuf },
    /// Standalone file root used only by legacy persisted workspaces.
    File { path: PathBuf },
}

impl WorkspaceEntry {
    /// Borrow the filesystem path behind this legacy entry.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            WorkspaceEntry::Directory { path } | WorkspaceEntry::File { path } => path,
        }
    }

    /// Return whether this entry points at a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        matches!(self, WorkspaceEntry::Directory { .. })
    }
}

/// One persisted workspace with exactly one root directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    /// Stable identifier used by persisted scope selection and callbacks.
    pub id: WorkspaceId,
    /// User-visible workspace label shown in the selector and section header.
    pub name: String,
    /// Canonical root directory for this workspace.
    pub root: PathBuf,
}

/// Top-level persisted state for all workspaces plus the current scope.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspacesFile {
    /// The user's last explicit workspace scope selection.
    #[serde(default)]
    pub current_scope: WorkspaceScope,
    /// All restored workspaces, each with exactly one root directory.
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,
}

impl WorkspacesFile {
    /// Add a new single-root workspace and select it immediately.
    pub fn add_workspace(&mut self, name: &str, root: PathBuf) -> WorkspaceId {
        let id = WorkspaceId::new(generate_id());
        self.workspaces.push(WorkspaceConfig {
            id: id.clone(),
            name: name.to_string(),
            root,
        });
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

    /// Rename one workspace. No-op if the workspace id is not found.
    pub fn rename_workspace(&mut self, ws_id: &WorkspaceId, new_name: &str) {
        if let Some(workspace) = self
            .workspaces
            .iter_mut()
            .find(|workspace| &workspace.id == ws_id)
        {
            workspace.name = new_name.to_string();
        }
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

    /// Collect every persisted workspace root directory.
    #[must_use]
    pub fn all_workspace_root_paths(&self) -> Vec<PathBuf> {
        self.workspaces
            .iter()
            .map(|workspace| workspace.root.clone())
            .collect()
    }

    /// Collect the workspace roots covered by the given scope.
    #[must_use]
    pub fn root_paths_for_scope(&self, scope: &WorkspaceScope) -> Vec<PathBuf> {
        match self.normalized_scope(scope.clone()) {
            WorkspaceScope::All => self.all_workspace_root_paths(),
            WorkspaceScope::Workspace(workspace_id) => self
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .map_or_else(Vec::new, |workspace| vec![workspace.root.clone()]),
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
        assert_eq!(file.current_scope(), WorkspaceScope::workspace(id.clone()));
        assert_eq!(file.workspaces[0].root, Path::new("/tmp/project"));
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
    fn remove_and_add_different_root_creates_distinct_workspace_identity() {
        let mut file = WorkspacesFile::default();
        let old_id = file.add_workspace("old", "/tmp/old".into());

        file.remove_workspace(&old_id);
        let new_id = file.add_workspace("new", "/tmp/new".into());

        assert_ne!(old_id, new_id);
        assert_eq!(file.workspaces.len(), 1);
        assert_eq!(file.workspaces[0].id, new_id);
        assert_eq!(file.workspaces[0].root, Path::new("/tmp/new"));
        assert_eq!(file.workspaces[0].name, "new");
    }

    #[test]
    fn root_paths_for_scope_returns_selected_workspace_only() {
        let mut file = WorkspacesFile::default();
        let first = file.add_workspace("first", "/tmp/first".into());
        let _second = file.add_workspace("second", "/tmp/second".into());

        let roots = file.root_paths_for_scope(&WorkspaceScope::workspace(first));

        assert_eq!(roots, vec![PathBuf::from("/tmp/first")]);
    }

    #[test]
    fn root_paths_for_all_scope_returns_every_root() {
        let mut file = WorkspacesFile::default();
        let _ = file.add_workspace("first", "/tmp/first".into());
        let _ = file.add_workspace("second", "/tmp/second".into());

        let roots = file.root_paths_for_scope(&WorkspaceScope::All);

        assert_eq!(
            roots,
            vec![PathBuf::from("/tmp/first"), PathBuf::from("/tmp/second")]
        );
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
    fn workspace_entry_serialization_directory() {
        let entry = WorkspaceEntry::Directory {
            path: "/tmp/project".into(),
        };
        let json = serde_json::to_string(&entry).expect("expected operation to succeed");
        let restored: WorkspaceEntry =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(restored, entry);
    }

    #[test]
    fn workspace_entry_queries_expose_path_and_kind() {
        let directory = WorkspaceEntry::Directory {
            path: "/tmp/project".into(),
        };
        let file = WorkspaceEntry::File {
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
    fn workspace_entry_serialization_file() {
        let entry = WorkspaceEntry::File {
            path: "/tmp/file.txt".into(),
        };
        let json = serde_json::to_string(&entry).expect("expected operation to succeed");
        let restored: WorkspaceEntry =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(restored, entry);
    }

    #[test]
    fn workspace_config_serialization_roundtrip() {
        let config = WorkspaceConfig {
            id: WorkspaceId::new("ws-123"),
            name: "project".into(),
            root: "/tmp/project".into(),
        };
        let json = serde_json::to_string(&config).expect("expected operation to succeed");
        let restored: WorkspaceConfig =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(restored, config);
    }
}

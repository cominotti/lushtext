// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace persistence and legacy normalization.
//!
//! This service owns the migration from older mixed-root workspace files into
//! the current single-root contract before the sidebar or window consumes them.

use crate::model::workspace::{
    WorkspaceConfig, WorkspaceEntry, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use crate::services::json_store;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Load workspaces from disk, normalizing any legacy persisted shapes.
///
/// # Errors
///
/// Returns an error if the workspace file exists but cannot be read or parsed.
pub fn load(data_dir: &Path) -> Result<WorkspacesFile> {
    let path = data_dir.join("workspaces.json");
    match std::fs::read(&path) {
        Ok(bytes) => {
            let stored: StoredWorkspacesFile = serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            Ok(stored.into_normalized())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(WorkspacesFile::default()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to read {}: {}",
            path.display(),
            error
        )),
    }
}

/// Save workspaces to disk.
///
/// # Errors
///
/// Returns an error if the workspace file cannot be serialized or written.
pub fn save(data_dir: &Path, file: &WorkspacesFile) -> Result<()> {
    json_store::save(data_dir, "workspaces.json", file)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredWorkspacesFile {
    Current(WorkspacesFile),
    Legacy(LegacyWorkspacesFile),
}

impl StoredWorkspacesFile {
    fn into_normalized(self) -> WorkspacesFile {
        match self {
            StoredWorkspacesFile::Current(mut file) => {
                file.normalize_scope();
                file
            }
            StoredWorkspacesFile::Legacy(file) => normalize_legacy(file),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct LegacyWorkspacesFile {
    #[serde(default)]
    active_workspace: Option<WorkspaceId>,
    #[serde(default)]
    workspaces: Vec<LegacyWorkspaceConfig>,
}

#[derive(Debug, Deserialize)]
struct LegacyWorkspaceConfig {
    id: WorkspaceId,
    name: String,
    #[serde(default)]
    entries: Vec<WorkspaceEntry>,
}

fn normalize_legacy(file: LegacyWorkspacesFile) -> WorkspacesFile {
    let mut workspaces = Vec::new();
    let mut selected_scope = WorkspaceScope::All;

    for workspace in file.workspaces {
        let roots = normalize_legacy_entries(&workspace.entries);
        let mut roots = roots.into_iter();

        let Some(first_root) = roots.next() else {
            tracing::warn!(
                "Dropping legacy workspace '{}' because no directory root could be normalized",
                workspace.name
            );
            continue;
        };

        workspaces.push(WorkspaceConfig {
            id: workspace.id.clone(),
            name: workspace.name.clone(),
            root: first_root,
        });

        if file.active_workspace.as_ref() == Some(&workspace.id) {
            selected_scope = WorkspaceScope::workspace(workspace.id.clone());
        }

        for root in roots {
            workspaces.push(WorkspaceConfig {
                id: generated_workspace_id(),
                name: display_name_for_root(&root),
                root,
            });
        }
    }

    let mut normalized = WorkspacesFile {
        current_scope: selected_scope,
        workspaces,
    };
    normalized.normalize_scope();
    normalized
}

fn normalize_legacy_entries(entries: &[WorkspaceEntry]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for entry in entries {
        let candidate = match entry {
            WorkspaceEntry::Directory { path } => Some(path.clone()),
            WorkspaceEntry::File { path } => path.parent().map(Path::to_path_buf),
        };

        if let Some(root) = candidate
            && !roots.iter().any(|existing| existing == &root)
        {
            roots.push(root);
        }
    }
    roots
}

fn display_name_for_root(path: &Path) -> String {
    path.file_name().map_or_else(
        || "Workspace".to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn generated_workspace_id() -> WorkspaceId {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should not be earlier than the UNIX epoch")
        .as_nanos();
    WorkspaceId::new(format!("{:016x}-{:04x}", nanos, std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        std::fs::create_dir(dir.path().join("workspaces.json"))
            .expect("expected operation to succeed");

        let error = load(dir.path()).expect_err("directory workspace file should fail");

        assert!(
            error.to_string().contains("failed to read"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("my workspace", "/home/user/project".into());
        file.set_current_scope(WorkspaceScope::workspace(workspace_id.clone()));

        save(dir.path(), &file).expect("expected operation to succeed");
        let loaded = load(dir.path()).expect("expected operation to succeed");

        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].name, "my workspace");
        assert_eq!(loaded.workspaces[0].root, Path::new("/home/user/project"));
        assert_eq!(
            loaded.current_scope(),
            WorkspaceScope::workspace(workspace_id)
        );
    }

    #[test]
    fn test_load_normalizes_legacy_multi_root_workspace() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(
            dir.path().join("workspaces.json"),
            serde_json::json!({
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
        )
        .expect("expected operation to succeed");

        let loaded = load(dir.path()).expect("expected operation to succeed");

        assert_eq!(loaded.workspaces.len(), 2);
        assert_eq!(loaded.workspaces[0].id, WorkspaceId::new("legacy"));
        assert_eq!(loaded.workspaces[0].root, Path::new("/tmp/one"));
        assert_eq!(loaded.workspaces[1].name, "two");
        assert_eq!(loaded.workspaces[1].root, Path::new("/tmp/two"));
        assert_ne!(loaded.workspaces[1].id, WorkspaceId::default());
        assert_ne!(loaded.workspaces[1].id, WorkspaceId::new("legacy"));
        assert_eq!(
            loaded.current_scope(),
            WorkspaceScope::workspace(WorkspaceId::new("legacy"))
        );
    }

    #[test]
    fn test_load_normalizes_legacy_file_root_to_parent_directory() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(
            dir.path().join("workspaces.json"),
            serde_json::json!({
                "active_workspace": "legacy",
                "workspaces": [{
                    "id": "legacy",
                    "name": "Legacy",
                    "entries": [
                        { "kind": "file", "path": "/tmp/project/src/lib.rs" }
                    ]
                }]
            })
            .to_string(),
        )
        .expect("expected operation to succeed");

        let loaded = load(dir.path()).expect("expected operation to succeed");

        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].root, Path::new("/tmp/project/src"));
    }

    #[test]
    fn test_load_falls_back_to_all_scope_when_target_is_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(
            dir.path().join("workspaces.json"),
            serde_json::json!({
                "current_scope": { "kind": "workspace", "workspace_id": "missing" },
                "workspaces": [{
                    "id": "existing",
                    "name": "Existing",
                    "root": "/tmp/existing"
                }]
            })
            .to_string(),
        )
        .expect("expected operation to succeed");

        let loaded = load(dir.path()).expect("expected operation to succeed");
        assert_eq!(loaded.current_scope(), WorkspaceScope::All);
    }
}

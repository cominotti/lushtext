// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace persistence: load and save workspace configurations.

use crate::model::workspace::WorkspacesFile;
use crate::services::json_store;
use anyhow::Result;
use std::path::Path;

/// Load workspaces from disk. Returns default (empty) if file doesn't exist.
///
/// # Errors
///
/// Returns an error if the workspace file exists but cannot be read or parsed.
pub fn load(data_dir: &Path) -> Result<WorkspacesFile> {
    json_store::load(data_dir, "workspaces.json")
}

/// Save workspaces to disk.
///
/// # Errors
///
/// Returns an error if the workspace file cannot be serialized or written.
pub fn save(data_dir: &Path, file: &WorkspacesFile) -> Result<()> {
    json_store::save(data_dir, "workspaces.json", file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::workspace::{WorkspaceConfig, WorkspaceEntry, WorkspaceId};
    use tempfile::TempDir;

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let result = load(dir.path()).expect("expected operation to succeed");
        assert!(result.workspaces.is_empty());
        assert!(result.active_workspace.is_none());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut file = WorkspacesFile::default();
        let ws = WorkspaceConfig {
            id: WorkspaceId::new("test-id"),
            name: "my workspace".into(),
            entries: vec![
                WorkspaceEntry::Directory {
                    path: "/home/user/project".into(),
                },
                WorkspaceEntry::File {
                    path: "/home/user/notes.md".into(),
                },
            ],
        };
        file.workspaces.push(ws);
        file.active_workspace = Some(WorkspaceId::new("test-id"));

        save(dir.path(), &file).expect("expected operation to succeed");
        let loaded = load(dir.path()).expect("expected operation to succeed");

        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].name, "my workspace");
        assert_eq!(loaded.workspaces[0].entries.len(), 2);
    }
}

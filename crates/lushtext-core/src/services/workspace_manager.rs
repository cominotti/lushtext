// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace management: load, save, and modify workspace configurations.

use crate::model::workspace::{WorkspaceConfig, WorkspaceEntry, WorkspaceId, WorkspacesFile};
use crate::services::json_store;
use anyhow::Result;
use std::path::Path;

/// Load workspaces from disk. Returns default (empty) if file doesn't exist.
pub fn load(data_dir: &Path) -> Result<WorkspacesFile> {
    json_store::load(data_dir, "workspaces.json")
}

/// Save workspaces to disk.
pub fn save(data_dir: &Path, file: &WorkspacesFile) -> Result<()> {
    json_store::save(data_dir, "workspaces.json", file)
}

/// Get the active workspace, or create a default one if none exists.
pub fn active_workspace(file: &mut WorkspacesFile) -> &WorkspaceConfig {
    if file.workspaces.is_empty() {
        let default_ws = WorkspaceConfig {
            id: WorkspaceId(generate_id()),
            name: "workspace".to_string(),
            entries: Vec::new(),
        };
        file.workspaces.push(default_ws);
        file.active_workspace = Some(file.workspaces[0].id.clone());
    }

    let active_id = file.active_workspace.as_ref().unwrap();
    let idx = file
        .workspaces
        .iter()
        .position(|w| &w.id == active_id)
        .unwrap_or(0);
    &file.workspaces[idx]
}

/// Add an entry to a workspace.
pub fn add_entry(file: &mut WorkspacesFile, ws_id: &WorkspaceId, entry: WorkspaceEntry) {
    if let Some(ws) = file.workspaces.iter_mut().find(|w| &w.id == ws_id) {
        if !ws.entries.iter().any(|e| e.path() == entry.path()) {
            ws.entries.push(entry);
        }
    }
}

/// Remove an entry from a workspace by path.
pub fn remove_entry(file: &mut WorkspacesFile, ws_id: &WorkspaceId, path: &Path) {
    if let Some(ws) = file.workspaces.iter_mut().find(|w| &w.id == ws_id) {
        ws.entries.retain(|e| e.path() != path);
    }
}

/// Generate a unique-enough identifier for workspace IDs.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:016x}-{:04x}", nanos, std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = TempDir::new().unwrap();
        let result = load(dir.path()).unwrap();
        assert!(result.workspaces.is_empty());
        assert!(result.active_workspace.is_none());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let mut file = WorkspacesFile::default();
        let ws = WorkspaceConfig {
            id: WorkspaceId("test-id".into()),
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
        file.active_workspace = Some(WorkspaceId("test-id".into()));

        save(dir.path(), &file).unwrap();
        let loaded = load(dir.path()).unwrap();

        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].name, "my workspace");
        assert_eq!(loaded.workspaces[0].entries.len(), 2);
    }

    #[test]
    fn test_active_workspace_creates_default() {
        let mut file = WorkspacesFile::default();
        let ws = active_workspace(&mut file);
        assert_eq!(ws.name, "workspace");
        assert!(file.active_workspace.is_some());
    }

    #[test]
    fn test_add_entry_deduplicates() {
        let mut file = WorkspacesFile::default();
        let _ = active_workspace(&mut file);
        let ws_id = file.workspaces[0].id.clone();

        add_entry(
            &mut file,
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/tmp/test".into(),
            },
        );
        add_entry(
            &mut file,
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/tmp/test".into(),
            },
        );

        assert_eq!(file.workspaces[0].entries.len(), 1);
    }

    #[test]
    fn test_remove_entry() {
        let mut file = WorkspacesFile::default();
        let _ = active_workspace(&mut file);
        let ws_id = file.workspaces[0].id.clone();

        add_entry(
            &mut file,
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/tmp/test".into(),
            },
        );
        assert_eq!(file.workspaces[0].entries.len(), 1);

        remove_entry(&mut file, &ws_id, Path::new("/tmp/test"));
        assert_eq!(file.workspaces[0].entries.len(), 0);
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace model — a named collection of root directories and files.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Stable identifier for a workspace (not user-visible name).
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A single entry in a workspace: either a directory root or a standalone file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceEntry {
    Directory { path: PathBuf },
    File { path: PathBuf },
}

impl WorkspaceEntry {
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            WorkspaceEntry::Directory { path } | WorkspaceEntry::File { path } => path,
        }
    }

    #[must_use]
    pub fn is_dir(&self) -> bool {
        matches!(self, WorkspaceEntry::Directory { .. })
    }
}

/// A named workspace persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub id: WorkspaceId,
    pub name: String,
    pub entries: Vec<WorkspaceEntry>,
}

/// Top-level persisted state: all workspaces + which one is active.
/// Stored at `$XDG_DATA_HOME/lushtext/workspaces.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WorkspacesFile {
    pub active_workspace: Option<WorkspaceId>,
    pub workspaces: Vec<WorkspaceConfig>,
}

impl WorkspacesFile {
    /// Get the active workspace, or create a default one if none exists.
    pub fn active_workspace(&mut self) -> &WorkspaceConfig {
        if self.workspaces.is_empty() {
            let default_ws = WorkspaceConfig {
                id: WorkspaceId::new(generate_id()),
                name: "New Workspace".to_string(),
                entries: Vec::new(),
            };
            self.workspaces.push(default_ws);
            self.active_workspace = Some(self.workspaces[0].id.clone());
        }

        let idx = self
            .active_workspace
            .as_ref()
            .and_then(|id| self.workspaces.iter().position(|w| &w.id == id))
            .unwrap_or(0);
        &self.workspaces[idx]
    }

    /// Add an entry to a workspace. Deduplicates by path.
    pub fn add_entry(&mut self, ws_id: &WorkspaceId, entry: WorkspaceEntry) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| &w.id == ws_id)
            && !ws.entries.iter().any(|e| e.path() == entry.path())
        {
            ws.entries.push(entry);
        }
    }

    /// Remove an entry from a workspace by path.
    pub fn remove_entry(&mut self, ws_id: &WorkspaceId, path: &Path) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| &w.id == ws_id) {
            ws.entries.retain(|e| e.path() != path);
        }
    }

    /// Add a new workspace with the given name. Returns the generated ID.
    pub fn add_workspace(&mut self, name: &str) -> WorkspaceId {
        let id = WorkspaceId::new(generate_id());
        self.workspaces.push(WorkspaceConfig {
            id: id.clone(),
            name: name.to_string(),
            entries: Vec::new(),
        });
        id
    }

    /// Remove a workspace by ID. If the removed workspace was active,
    /// switches active to the first remaining workspace.
    pub fn remove_workspace(&mut self, ws_id: &WorkspaceId) {
        self.workspaces.retain(|w| &w.id != ws_id);
        if self.active_workspace.as_ref() == Some(ws_id) {
            self.active_workspace = self.workspaces.first().map(|w| w.id.clone());
        }
    }

    /// Rename a workspace. No-op if the workspace ID is not found.
    pub fn rename_workspace(&mut self, ws_id: &WorkspaceId, new_name: &str) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| &w.id == ws_id) {
            ws.name = new_name.to_string();
        }
    }

    /// Replace all entries in a workspace with a single new root, updating the name.
    /// No-op if the workspace ID is not found.
    pub fn replace_root(&mut self, ws_id: &WorkspaceId, entry: WorkspaceEntry, name: &str) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| &w.id == ws_id) {
            ws.entries.clear();
            ws.entries.push(entry);
            ws.name = name.to_string();
        }
    }
}

/// Generate a unique-enough identifier for workspace IDs.
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

    /// Create a `WorkspacesFile` with a default workspace already initialized.
    /// Returns the file and the active workspace's id.
    fn file_with_default_workspace() -> (WorkspacesFile, WorkspaceId) {
        let mut file = WorkspacesFile::default();
        let _ = file.active_workspace();
        let ws_id = file.workspaces[0].id.clone();
        (file, ws_id)
    }

    #[test]
    fn test_active_workspace_creates_default() {
        let mut file = WorkspacesFile::default();
        let ws = file.active_workspace();
        assert_eq!(ws.name, "New Workspace");
        assert!(file.active_workspace.is_some());
    }

    #[test]
    fn test_add_entry_deduplicates() {
        let (mut file, ws_id) = file_with_default_workspace();

        file.add_entry(
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/tmp/test".into(),
            },
        );
        file.add_entry(
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/tmp/test".into(),
            },
        );

        assert_eq!(file.workspaces[0].entries.len(), 1);
    }

    #[test]
    fn test_remove_entry() {
        let (mut file, ws_id) = file_with_default_workspace();

        file.add_entry(
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/tmp/test".into(),
            },
        );
        assert_eq!(file.workspaces[0].entries.len(), 1);

        file.remove_entry(&ws_id, Path::new("/tmp/test"));
        assert!(file.workspaces[0].entries.is_empty());
    }

    #[test]
    fn test_workspace_entry_path_directory() {
        let entry = WorkspaceEntry::Directory {
            path: "/tmp/project".into(),
        };
        assert_eq!(entry.path(), Path::new("/tmp/project"));
    }

    #[test]
    fn test_workspace_entry_path_file() {
        let entry = WorkspaceEntry::File {
            path: "/tmp/notes.md".into(),
        };
        assert_eq!(entry.path(), Path::new("/tmp/notes.md"));
    }

    #[test]
    fn test_active_workspace_fallback_when_id_not_found() {
        let mut file = WorkspacesFile {
            active_workspace: Some(WorkspaceId::new("nonexistent")),
            workspaces: vec![WorkspaceConfig {
                id: WorkspaceId::new("real"),
                name: "real-workspace".into(),
                entries: vec![],
            }],
        };
        let ws = file.active_workspace();
        assert_eq!(ws.id, WorkspaceId::new("real"));
        assert_eq!(ws.name, "real-workspace");
    }

    #[test]
    fn test_add_entry_noop_for_unknown_workspace() {
        let (mut file, _) = file_with_default_workspace();

        file.add_entry(
            &WorkspaceId::new("nonexistent"),
            WorkspaceEntry::File {
                path: "/tmp/file".into(),
            },
        );
        assert!(file.workspaces[0].entries.is_empty());
    }

    #[test]
    fn test_remove_entry_noop_for_unknown_workspace() {
        let (mut file, ws_id) = file_with_default_workspace();

        file.add_entry(
            &ws_id,
            WorkspaceEntry::File {
                path: "/tmp/file".into(),
            },
        );

        file.remove_entry(&WorkspaceId::new("nonexistent"), Path::new("/tmp/file"));
        assert_eq!(file.workspaces[0].entries.len(), 1);
    }

    #[test]
    fn test_remove_entry_noop_for_nonexistent_path() {
        let (mut file, ws_id) = file_with_default_workspace();

        file.add_entry(
            &ws_id,
            WorkspaceEntry::File {
                path: "/tmp/file".into(),
            },
        );

        file.remove_entry(&ws_id, Path::new("/tmp/other"));
        assert_eq!(file.workspaces[0].entries.len(), 1);
    }

    #[test]
    fn test_add_entry_deduplicates_across_kinds() {
        let (mut file, ws_id) = file_with_default_workspace();

        file.add_entry(
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/tmp/target".into(),
            },
        );
        file.add_entry(
            &ws_id,
            WorkspaceEntry::File {
                path: "/tmp/target".into(),
            },
        );

        assert_eq!(file.workspaces[0].entries.len(), 1);
    }

    #[test]
    fn test_workspaces_file_default_is_empty() {
        let file = WorkspacesFile::default();
        assert!(file.workspaces.is_empty());
        assert!(file.active_workspace.is_none());
    }

    #[test]
    fn test_workspace_entry_serialization_directory() {
        let entry = WorkspaceEntry::Directory {
            path: "/tmp/project".into(),
        };
        let json = serde_json::to_string(&entry).expect("expected operation to succeed");
        let deserialized: WorkspaceEntry =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(deserialized.path(), entry.path());
        assert!(matches!(deserialized, WorkspaceEntry::Directory { .. }));
    }

    #[test]
    fn test_workspace_entry_serialization_file() {
        let entry = WorkspaceEntry::File {
            path: "/tmp/notes.md".into(),
        };
        let json = serde_json::to_string(&entry).expect("expected operation to succeed");
        let deserialized: WorkspaceEntry =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(deserialized.path(), entry.path());
        assert!(matches!(deserialized, WorkspaceEntry::File { .. }));
    }

    #[test]
    fn test_workspace_config_serialization_roundtrip() {
        let config = WorkspaceConfig {
            id: WorkspaceId::new("ws-123"),
            name: "my project".into(),
            entries: vec![
                WorkspaceEntry::Directory {
                    path: "/home/user/src".into(),
                },
                WorkspaceEntry::File {
                    path: "/home/user/notes.md".into(),
                },
            ],
        };
        let json = serde_json::to_string(&config).expect("expected operation to succeed");
        let deserialized: WorkspaceConfig =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(deserialized.id, config.id);
        assert_eq!(deserialized.name, config.name);
        assert_eq!(deserialized.entries.len(), 2);
        assert!(matches!(
            deserialized.entries[0],
            WorkspaceEntry::Directory { .. }
        ));
        assert!(matches!(
            deserialized.entries[1],
            WorkspaceEntry::File { .. }
        ));
    }

    #[test]
    fn test_generated_ids_are_nonempty() {
        let (file, _) = file_with_default_workspace();
        assert!(!file.workspaces[0].id.is_empty());
    }

    #[test]
    fn test_add_workspace_creates_with_id() {
        let mut file = WorkspacesFile::default();
        let id = file.add_workspace("my project");
        assert_eq!(file.workspaces.len(), 1);
        assert_eq!(file.workspaces[0].name, "my project");
        assert_eq!(file.workspaces[0].id, id);
        assert!(file.workspaces[0].entries.is_empty());
    }

    #[test]
    fn test_add_workspace_appends_to_existing() {
        let (mut file, _) = file_with_default_workspace();
        let id = file.add_workspace("second");
        assert_eq!(file.workspaces.len(), 2);
        assert_eq!(file.workspaces[1].id, id);
        assert_eq!(file.workspaces[1].name, "second");
    }

    #[test]
    fn test_remove_workspace_basic() {
        let mut file = WorkspacesFile::default();
        let id1 = file.add_workspace("first");
        let _id2 = file.add_workspace("second");
        assert_eq!(file.workspaces.len(), 2);

        file.remove_workspace(&id1);
        assert_eq!(file.workspaces.len(), 1);
        assert_eq!(file.workspaces[0].name, "second");
    }

    #[test]
    fn test_remove_workspace_updates_active() {
        let mut file = WorkspacesFile::default();
        let id1 = file.add_workspace("first");
        let id2 = file.add_workspace("second");
        file.active_workspace = Some(id1.clone());

        file.remove_workspace(&id1);
        assert_eq!(file.active_workspace, Some(id2));
    }

    #[test]
    fn test_remove_workspace_noop_for_unknown() {
        let (mut file, _) = file_with_default_workspace();
        let count_before = file.workspaces.len();
        file.remove_workspace(&WorkspaceId::new("nonexistent"));
        assert_eq!(file.workspaces.len(), count_before);
    }

    #[test]
    fn test_rename_workspace_basic() {
        let mut file = WorkspacesFile::default();
        let id = file.add_workspace("old name");
        file.rename_workspace(&id, "new name");
        assert_eq!(file.workspaces[0].name, "new name");
    }

    #[test]
    fn test_rename_workspace_noop_for_unknown() {
        let (mut file, _) = file_with_default_workspace();
        let original_name = file.workspaces[0].name.clone();
        file.rename_workspace(&WorkspaceId::new("nonexistent"), "changed");
        assert_eq!(file.workspaces[0].name, original_name);
    }

    #[test]
    fn test_replace_root_clears_and_replaces() {
        let (mut file, ws_id) = file_with_default_workspace();
        file.add_entry(
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/old/dir".into(),
            },
        );
        file.add_entry(
            &ws_id,
            WorkspaceEntry::File {
                path: "/old/file.txt".into(),
            },
        );
        assert_eq!(file.workspaces[0].entries.len(), 2);

        file.replace_root(
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/new/root".into(),
            },
            "new-name",
        );

        assert_eq!(file.workspaces[0].entries.len(), 1);
        assert_eq!(file.workspaces[0].entries[0].path(), Path::new("/new/root"));
        assert_eq!(file.workspaces[0].name, "new-name");
    }

    #[test]
    fn test_replace_root_noop_for_unknown() {
        let (mut file, ws_id) = file_with_default_workspace();
        file.add_entry(
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/keep".into(),
            },
        );

        file.replace_root(
            &WorkspaceId::new("nonexistent"),
            WorkspaceEntry::Directory {
                path: "/new".into(),
            },
            "ignored",
        );

        assert_eq!(file.workspaces[0].entries.len(), 1);
        assert_eq!(file.workspaces[0].entries[0].path(), Path::new("/keep"));
    }
}

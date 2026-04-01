// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace model — a named collection of root directories and files.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Stable identifier for a workspace (not user-visible name).
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceId(pub String);

/// A single entry in a workspace: either a directory root or a standalone file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceEntry {
    Directory { path: PathBuf },
    File { path: PathBuf },
}

impl WorkspaceEntry {
    pub fn path(&self) -> &Path {
        match self {
            WorkspaceEntry::Directory { path } | WorkspaceEntry::File { path } => path,
        }
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
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WorkspacesFile {
    pub active_workspace: Option<WorkspaceId>,
    pub workspaces: Vec<WorkspaceConfig>,
}

impl WorkspacesFile {
    /// Get the active workspace, or create a default one if none exists.
    pub fn active_workspace(&mut self) -> &WorkspaceConfig {
        if self.workspaces.is_empty() {
            let default_ws = WorkspaceConfig {
                id: WorkspaceId(generate_id()),
                name: "workspace".to_string(),
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
        if let Some(ws) = self.workspaces.iter_mut().find(|w| &w.id == ws_id) {
            if !ws.entries.iter().any(|e| e.path() == entry.path()) {
                ws.entries.push(entry);
            }
        }
    }

    /// Remove an entry from a workspace by path.
    pub fn remove_entry(&mut self, ws_id: &WorkspaceId, path: &Path) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| &w.id == ws_id) {
            ws.entries.retain(|e| e.path() != path);
        }
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

    #[test]
    fn test_active_workspace_creates_default() {
        let mut file = WorkspacesFile::default();
        let ws = file.active_workspace();
        assert_eq!(ws.name, "workspace");
        assert!(file.active_workspace.is_some());
    }

    #[test]
    fn test_add_entry_deduplicates() {
        let mut file = WorkspacesFile::default();
        let _ = file.active_workspace();
        let ws_id = file.workspaces[0].id.clone();

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
        let mut file = WorkspacesFile::default();
        let _ = file.active_workspace();
        let ws_id = file.workspaces[0].id.clone();

        file.add_entry(
            &ws_id,
            WorkspaceEntry::Directory {
                path: "/tmp/test".into(),
            },
        );
        assert_eq!(file.workspaces[0].entries.len(), 1);

        file.remove_entry(&ws_id, Path::new("/tmp/test"));
        assert_eq!(file.workspaces[0].entries.len(), 0);
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace model — a named collection of root directories and files.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Stable identifier for a workspace (not user-visible name).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

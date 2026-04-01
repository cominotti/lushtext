// SPDX-License-Identifier: GPL-3.0-or-later

//! Session model — persisted open-tab state for restoring on startup.

use super::workspace::WorkspaceId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One open tab in the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTab {
    pub path: PathBuf,
    pub cursor_line: u32,
    pub cursor_col: u32,
    pub scroll_line: u32,
}

/// Full session for one workspace.
/// Stored at `$XDG_DATA_HOME/lushtext/session-{workspace_id}.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SessionData {
    pub workspace_id: WorkspaceId,
    pub tabs: Vec<SessionTab>,
    pub active_tab: Option<PathBuf>,
}

impl Default for WorkspaceId {
    fn default() -> Self {
        WorkspaceId(String::new())
    }
}

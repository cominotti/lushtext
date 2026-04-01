// SPDX-License-Identifier: GPL-3.0-or-later

//! Session model — persisted open-tab state for restoring on startup.

use super::workspace::WorkspaceId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

impl SessionData {
    /// Retain only tabs whose paths are in the given set.
    /// Clears `active_tab` if the active file was removed.
    pub fn retain_tabs_by_path(&mut self, existing_paths: &HashSet<PathBuf>) {
        self.tabs.retain(|tab| existing_paths.contains(&tab.path));

        if let Some(ref active) = self.active_tab {
            if !existing_paths.contains(active) {
                self.active_tab = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retain_tabs_keeps_matching_clears_active() {
        let mut session = SessionData {
            workspace_id: WorkspaceId("test".into()),
            tabs: vec![
                SessionTab {
                    path: "/a/exists.rs".into(),
                    cursor_line: 1,
                    cursor_col: 0,
                    scroll_line: 0,
                },
                SessionTab {
                    path: "/b/gone.rs".into(),
                    cursor_line: 5,
                    cursor_col: 3,
                    scroll_line: 2,
                },
            ],
            active_tab: Some("/b/gone.rs".into()),
        };

        let existing: HashSet<PathBuf> = [PathBuf::from("/a/exists.rs")].into_iter().collect();
        session.retain_tabs_by_path(&existing);

        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.tabs[0].path, PathBuf::from("/a/exists.rs"));
        assert_eq!(session.active_tab, None);
    }
}

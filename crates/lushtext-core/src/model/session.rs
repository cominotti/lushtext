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

        if let Some(ref active) = self.active_tab
            && !existing_paths.contains(active)
        {
            self.active_tab = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a `SessionTab` at the given path and line, with zeroed cursor_col/scroll_line.
    fn tab(path: &str, cursor_line: u32) -> SessionTab {
        SessionTab {
            path: path.into(),
            cursor_line,
            cursor_col: 0,
            scroll_line: 0,
        }
    }

    #[test]
    fn test_retain_tabs_keeps_matching_clears_active() {
        let mut session = SessionData {
            workspace_id: WorkspaceId::new("test"),
            tabs: vec![
                tab("/a/exists.rs", 1),
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

    #[test]
    fn test_retain_tabs_empty_set_removes_all() {
        let mut session = SessionData {
            workspace_id: WorkspaceId::new("test"),
            tabs: vec![tab("/a.rs", 1), tab("/b.rs", 2)],
            active_tab: Some("/a.rs".into()),
        };

        session.retain_tabs_by_path(&HashSet::new());
        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab, None);
    }

    #[test]
    fn test_retain_tabs_all_matching_keeps_all() {
        let mut session = SessionData {
            workspace_id: WorkspaceId::new("test"),
            tabs: vec![tab("/a.rs", 1), tab("/b.rs", 2)],
            active_tab: Some("/a.rs".into()),
        };

        let all: HashSet<PathBuf> = [PathBuf::from("/a.rs"), PathBuf::from("/b.rs")]
            .into_iter()
            .collect();
        session.retain_tabs_by_path(&all);
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.active_tab, Some("/a.rs".into()));
    }

    #[test]
    fn test_retain_tabs_none_active_stays_none() {
        let mut session = SessionData {
            workspace_id: WorkspaceId::new("test"),
            tabs: vec![tab("/a.rs", 1)],
            active_tab: None,
        };

        let existing: HashSet<PathBuf> = [PathBuf::from("/a.rs")].into_iter().collect();
        session.retain_tabs_by_path(&existing);
        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.active_tab, None);
    }

    #[test]
    fn test_session_data_default_is_empty() {
        let session = SessionData::default();
        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab, None);
        assert_eq!(session.workspace_id, WorkspaceId::default());
    }

    #[test]
    fn test_session_tab_serialization_roundtrip() {
        let tab = SessionTab {
            path: "/home/user/project/main.rs".into(),
            cursor_line: 42,
            cursor_col: 15,
            scroll_line: 30,
        };
        let json = serde_json::to_string(&tab).unwrap();
        let deserialized: SessionTab = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.path, tab.path);
        assert_eq!(deserialized.cursor_line, tab.cursor_line);
        assert_eq!(deserialized.cursor_col, tab.cursor_col);
        assert_eq!(deserialized.scroll_line, tab.scroll_line);
    }

    #[test]
    fn test_session_data_serialization_roundtrip() {
        let session = SessionData {
            workspace_id: WorkspaceId::new("test-ws"),
            tabs: vec![SessionTab {
                path: "/a.rs".into(),
                cursor_line: 10,
                cursor_col: 5,
                scroll_line: 8,
            }],
            active_tab: Some("/a.rs".into()),
        };
        let json = serde_json::to_string(&session).unwrap();
        let deserialized: SessionData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.workspace_id, session.workspace_id);
        assert_eq!(deserialized.tabs.len(), 1);
        assert_eq!(deserialized.tabs[0].path, PathBuf::from("/a.rs"));
        assert_eq!(deserialized.active_tab, Some(PathBuf::from("/a.rs")));
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Session model — persisted open-tab state for restoring on startup.
//!
//! A single global session file (`session.json`) captures all open tabs
//! regardless of workspace. Tabs are not workspace-scoped in the UI —
//! they all share one `AdwTabView`.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

/// One open tab in the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTab {
    /// File path, or `None` for untitled tabs.
    pub path: Option<PathBuf>,
    /// Draft ID for untitled tab draft recovery. For file-backed tabs
    /// the draft system can derive the ID from the path, but untitled
    /// tabs need this stored explicitly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    pub cursor_line: u32,
    pub cursor_col: u32,
    pub scroll_line: u32,
}

/// Full session state for the application.
/// Stored at `$XDG_DATA_HOME/lushtext/session.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SessionData {
    pub tabs: Vec<SessionTab>,
    /// Index of the active (selected) tab in the `tabs` vec.
    pub active_tab_index: Option<usize>,
}

impl SessionData {
    /// Retain only untitled tabs and file-backed tabs whose paths satisfy `keep`.
    ///
    /// This keeps the "rebase the active index onto the surviving tab list"
    /// rule in the domain model so service code can provide only the filesystem
    /// predicate (`Path::exists`, precomputed path sets, and so on).
    pub fn retain_tabs_where<F>(&mut self, mut keep: F)
    where
        F: FnMut(&Path) -> bool,
    {
        let active_original_idx = self.active_tab_index;

        // Rebuild the tab list, tracking how the active tab index shifts.
        // `Vec::retain` doesn't expose indices, so drain + filter is clearer.
        let old_tabs = std::mem::take(&mut self.tabs);
        let mut new_active_index: Option<usize> = None;

        for (old_idx, tab) in old_tabs.into_iter().enumerate() {
            let keep_tab = match &tab.path {
                None => true, // untitled tabs always survive
                Some(path) => keep(path),
            };
            if keep_tab {
                if active_original_idx == Some(old_idx) {
                    new_active_index = Some(self.tabs.len());
                }
                self.tabs.push(tab);
            }
        }

        self.active_tab_index = new_active_index;
    }

    /// Retain only file-backed tabs whose paths are in the given set.
    /// Untitled tabs (path = None) are always kept.
    /// Adjusts `active_tab_index` to track the same tab after removal,
    /// or clears it if the active tab was removed.
    pub fn retain_tabs_by_path(&mut self, existing_paths: &HashSet<PathBuf>) {
        self.retain_tabs_where(|path| existing_paths.contains(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a file-backed `SessionTab` at the given path.
    fn tab(path: &str, cursor_line: u32) -> SessionTab {
        SessionTab {
            path: Some(path.into()),
            draft_id: None,
            cursor_line,
            cursor_col: 0,
            scroll_line: 0,
        }
    }

    /// Create an untitled `SessionTab`.
    fn untitled_tab(draft_id: &str) -> SessionTab {
        SessionTab {
            path: None,
            draft_id: Some(draft_id.to_string()),
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
        }
    }

    #[test]
    fn test_retain_tabs_keeps_matching_clears_active() {
        let mut session = SessionData {
            tabs: vec![tab("/a/exists.rs", 1), tab("/b/gone.rs", 5)],
            active_tab_index: Some(1), // points to gone.rs
        };

        let existing: HashSet<PathBuf> = [PathBuf::from("/a/exists.rs")].into_iter().collect();
        session.retain_tabs_by_path(&existing);

        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.tabs[0].path, Some(PathBuf::from("/a/exists.rs")));
        assert_eq!(session.active_tab_index, None); // gone.rs was removed
    }

    #[test]
    fn test_retain_tabs_empty_set_removes_all_file_backed() {
        let mut session = SessionData {
            tabs: vec![tab("/a.rs", 1), tab("/b.rs", 2)],
            active_tab_index: Some(0),
        };

        session.retain_tabs_by_path(&HashSet::new());
        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab_index, None);
    }

    #[test]
    fn test_retain_tabs_all_matching_keeps_all() {
        let mut session = SessionData {
            tabs: vec![tab("/a.rs", 1), tab("/b.rs", 2)],
            active_tab_index: Some(0),
        };

        let all: HashSet<PathBuf> = [PathBuf::from("/a.rs"), PathBuf::from("/b.rs")]
            .into_iter()
            .collect();
        session.retain_tabs_by_path(&all);
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.active_tab_index, Some(0));
    }

    #[test]
    fn test_retain_tabs_none_active_stays_none() {
        let mut session = SessionData {
            tabs: vec![tab("/a.rs", 1)],
            active_tab_index: None,
        };

        let existing: HashSet<PathBuf> = [PathBuf::from("/a.rs")].into_iter().collect();
        session.retain_tabs_by_path(&existing);
        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.active_tab_index, None);
    }

    #[test]
    fn test_session_data_default_is_empty() {
        let session = SessionData::default();
        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab_index, None);
    }

    #[test]
    fn test_session_tab_serialization_roundtrip() {
        let tab = SessionTab {
            path: Some("/home/user/project/main.rs".into()),
            draft_id: None,
            cursor_line: 42,
            cursor_col: 15,
            scroll_line: 30,
        };
        let json = serde_json::to_string(&tab).expect("expected operation to succeed");
        let deserialized: SessionTab =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(deserialized.path, tab.path);
        assert_eq!(deserialized.cursor_line, tab.cursor_line);
        assert_eq!(deserialized.cursor_col, tab.cursor_col);
        assert_eq!(deserialized.scroll_line, tab.scroll_line);
    }

    #[test]
    fn test_session_data_serialization_roundtrip() {
        let session = SessionData {
            tabs: vec![SessionTab {
                path: Some("/a.rs".into()),
                draft_id: None,
                cursor_line: 10,
                cursor_col: 5,
                scroll_line: 8,
            }],
            active_tab_index: Some(0),
        };
        let json = serde_json::to_string(&session).expect("expected operation to succeed");
        let deserialized: SessionData =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(deserialized.tabs.len(), 1);
        assert_eq!(deserialized.tabs[0].path, Some(PathBuf::from("/a.rs")));
        assert_eq!(deserialized.active_tab_index, Some(0));
    }

    #[test]
    fn test_retain_tabs_preserves_untitled() {
        let mut session = SessionData {
            tabs: vec![
                tab("/a.rs", 1),
                untitled_tab("untitled-0"),
                tab("/gone.rs", 3),
            ],
            active_tab_index: Some(1), // untitled tab
        };

        let existing: HashSet<PathBuf> = [PathBuf::from("/a.rs")].into_iter().collect();
        session.retain_tabs_by_path(&existing);

        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[0].path, Some(PathBuf::from("/a.rs")));
        assert_eq!(session.tabs[1].path, None); // untitled preserved
        assert_eq!(session.active_tab_index, Some(1)); // index adjusted
    }

    #[test]
    fn test_retain_tabs_active_index_adjusts_on_removal_before() {
        let mut session = SessionData {
            tabs: vec![tab("/gone.rs", 1), tab("/keep.rs", 2), tab("/also.rs", 3)],
            active_tab_index: Some(2), // /also.rs
        };

        let existing: HashSet<PathBuf> = [PathBuf::from("/keep.rs"), PathBuf::from("/also.rs")]
            .into_iter()
            .collect();
        session.retain_tabs_by_path(&existing);

        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.active_tab_index, Some(1)); // shifted from 2→1
    }

    #[test]
    fn test_untitled_tab_draft_id_serialization() {
        let tab = untitled_tab("untitled-42");
        let json = serde_json::to_string(&tab).expect("expected operation to succeed");
        assert!(json.contains("untitled-42"));

        let deserialized: SessionTab =
            serde_json::from_str(&json).expect("expected operation to succeed");
        assert_eq!(deserialized.path, None);
        assert_eq!(deserialized.draft_id, Some("untitled-42".to_string()));
    }

    #[test]
    fn test_draft_id_skipped_when_none() {
        let tab = tab("/a.rs", 1);
        let json = serde_json::to_string(&tab).expect("expected operation to succeed");
        // draft_id should not appear in serialized form when None
        assert!(!json.contains("draft_id"));
    }

    #[test]
    fn test_retain_tabs_only_untitled_survive_empty_set() {
        let mut session = SessionData {
            tabs: vec![tab("/gone.rs", 1), untitled_tab("u-0"), untitled_tab("u-1")],
            active_tab_index: Some(0), // /gone.rs — will be removed
        };

        session.retain_tabs_by_path(&HashSet::new());
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[0].draft_id, Some("u-0".into()));
        assert_eq!(session.tabs[1].draft_id, Some("u-1".into()));
        assert_eq!(session.active_tab_index, None); // removed tab was active
    }
}

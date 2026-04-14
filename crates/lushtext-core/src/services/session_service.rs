// SPDX-License-Identifier: GPL-3.0-or-later

//! Session persistence: save/restore open tabs across restarts.
//!
//! Uses a single global session file (`session.json`) because tabs are
//! not workspace-scoped in the UI — all tabs share one `AdwTabView`.

use crate::model::session::SessionData;
use crate::services::json_store;
use anyhow::Result;
use std::path::Path;

/// Fixed filename for the global session file.
const SESSION_FILENAME: &str = "session.json";

/// Load the global session. Returns default (no tabs) if file doesn't exist.
///
/// # Errors
///
/// Returns an error if the session file exists but cannot be read or parsed.
pub fn load(data_dir: &Path) -> Result<SessionData> {
    json_store::load(data_dir, SESSION_FILENAME)
}

/// Save the global session to disk.
///
/// # Errors
///
/// Returns an error if the session file cannot be serialized or written.
pub fn save(data_dir: &Path, session: &SessionData) -> Result<()> {
    json_store::save(data_dir, SESSION_FILENAME, session)
}

/// Filter session tabs to only those whose files still exist on disk.
/// Untitled tabs (path = None) are always preserved.
///
/// **Threading:** This function calls `Path::exists()` (stat syscall) per
/// file-backed tab. On NFS/FUSE mounts this can block for 10-100ms per
/// path. Always call from a background thread via `spawn_blocking_then`,
/// never on the GTK main thread.
pub fn filter_existing_tabs(session: &mut SessionData) {
    session.retain_tabs_where(Path::exists);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::session::SessionTab;
    use tempfile::TempDir;

    /// Create a file-backed session tab.
    fn tab(path: impl Into<std::path::PathBuf>, cursor_line: u32) -> SessionTab {
        SessionTab {
            path: Some(path.into()),
            draft_id: None,
            cursor_line,
            cursor_col: 0,
            scroll_line: 0,
        }
    }

    /// Create an untitled session tab.
    fn untitled(draft_id: &str) -> SessionTab {
        SessionTab {
            path: None,
            draft_id: Some(draft_id.to_string()),
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
        }
    }

    #[test]
    fn test_load_missing_returns_default() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let session = load(dir.path()).expect("expected operation to succeed");
        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab_index, None);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let session = SessionData {
            tabs: vec![SessionTab {
                path: Some("/tmp/file.rs".into()),
                draft_id: None,
                cursor_line: 10,
                cursor_col: 5,
                scroll_line: 8,
            }],
            active_tab_index: Some(0),
        };

        save(dir.path(), &session).expect("expected operation to succeed");
        let loaded = load(dir.path()).expect("expected operation to succeed");

        assert_eq!(loaded.tabs.len(), 1);
        assert_eq!(loaded.tabs[0].cursor_line, 10);
        assert_eq!(loaded.active_tab_index, Some(0));
    }

    #[test]
    fn test_save_and_load_with_untitled_tabs() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let session = SessionData {
            tabs: vec![
                tab("/tmp/file.rs", 1),
                untitled("untitled-0"),
                untitled("untitled-1"),
            ],
            active_tab_index: Some(1),
        };

        save(dir.path(), &session).expect("expected operation to succeed");
        let loaded = load(dir.path()).expect("expected operation to succeed");

        assert_eq!(loaded.tabs.len(), 3);
        assert_eq!(loaded.tabs[0].path, Some("/tmp/file.rs".into()));
        assert_eq!(loaded.tabs[1].path, None);
        assert_eq!(loaded.tabs[1].draft_id, Some("untitled-0".into()));
        assert_eq!(loaded.tabs[2].draft_id, Some("untitled-1".into()));
        assert_eq!(loaded.active_tab_index, Some(1));
    }

    #[test]
    fn test_filter_existing_tabs_removes_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");

        let real_file = dir.path().join("exists.txt");
        std::fs::write(&real_file, "hello").expect("expected operation to succeed");

        let mut session = SessionData {
            tabs: vec![tab(real_file.clone(), 1), tab("/nonexistent/file.txt", 5)],
            active_tab_index: Some(1), // points to nonexistent
        };

        filter_existing_tabs(&mut session);

        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.tabs[0].path, Some(real_file));
        assert_eq!(session.active_tab_index, None);
    }

    #[test]
    fn test_filter_existing_tabs_preserves_untitled() {
        let dir = TempDir::new().expect("expected operation to succeed");

        let real_file = dir.path().join("exists.txt");
        std::fs::write(&real_file, "content").expect("expected operation to succeed");

        let mut session = SessionData {
            tabs: vec![
                tab(real_file.clone(), 1),
                untitled("u-0"),
                tab("/gone.txt", 3),
            ],
            active_tab_index: Some(1), // untitled tab
        };

        filter_existing_tabs(&mut session);

        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[0].path, Some(real_file));
        assert_eq!(session.tabs[1].path, None);
        assert_eq!(session.active_tab_index, Some(1)); // untitled survived at new index
    }

    #[test]
    fn test_filter_existing_tabs_adjusts_active_index() {
        let dir = TempDir::new().expect("expected operation to succeed");

        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        std::fs::write(&file_a, "a").expect("expected operation to succeed");
        std::fs::write(&file_b, "b").expect("expected operation to succeed");

        let mut session = SessionData {
            tabs: vec![
                tab("/gone.txt", 1),
                tab(file_a.clone(), 2),
                tab(file_b.clone(), 3),
            ],
            active_tab_index: Some(2), // file_b
        };

        filter_existing_tabs(&mut session);

        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.active_tab_index, Some(1)); // shifted from 2→1
    }

    #[test]
    fn test_filter_all_missing_clears_everything() {
        let mut session = SessionData {
            tabs: vec![tab("/gone1.txt", 1), tab("/gone2.txt", 2)],
            active_tab_index: Some(0),
        };

        filter_existing_tabs(&mut session);

        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab_index, None);
    }

    #[test]
    fn test_filter_empty_session_is_noop() {
        let mut session = SessionData::default();
        filter_existing_tabs(&mut session);
        assert!(session.tabs.is_empty());
        assert_eq!(session.active_tab_index, None);
    }

    #[test]
    fn test_save_overwrites_previous_session() {
        let dir = TempDir::new().expect("expected operation to succeed");

        let session1 = SessionData {
            tabs: vec![tab("/old.rs", 1)],
            active_tab_index: Some(0),
        };
        save(dir.path(), &session1).expect("expected operation to succeed");

        let session2 = SessionData {
            tabs: vec![tab("/new.rs", 5), tab("/also.rs", 10)],
            active_tab_index: Some(1),
        };
        save(dir.path(), &session2).expect("expected operation to succeed");

        let loaded = load(dir.path()).expect("expected operation to succeed");
        assert_eq!(loaded.tabs.len(), 2);
        assert_eq!(loaded.tabs[0].path, Some("/new.rs".into()));
        assert_eq!(loaded.active_tab_index, Some(1));
    }
}

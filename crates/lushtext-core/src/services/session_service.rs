// SPDX-License-Identifier: GPL-3.0-or-later

//! Session persistence: save/restore open tabs across restarts.

use crate::model::session::SessionData;
use crate::model::workspace::WorkspaceId;
use crate::services::json_store;
use anyhow::Result;
use std::path::Path;

fn session_filename(ws_id: &WorkspaceId) -> String {
    format!("session-{}.json", ws_id.as_str())
}

/// Load session for a workspace. Returns default (no tabs) if file doesn't exist.
pub fn load(data_dir: &Path, ws_id: &WorkspaceId) -> Result<SessionData> {
    let mut session: SessionData = json_store::load(data_dir, &session_filename(ws_id))?;
    if session.workspace_id.is_empty() {
        session.workspace_id = ws_id.clone();
    }
    Ok(session)
}

/// Save session to disk.
pub fn save(data_dir: &Path, session: &SessionData) -> Result<()> {
    json_store::save(data_dir, &session_filename(&session.workspace_id), session)
}

/// Filter session tabs to only those whose files still exist on disk.
/// Performs I/O to check file existence and prunes the in-memory session
/// without cloning every surviving path into a temporary collection.
///
/// **Threading:** This function calls `Path::exists()` (stat syscall) per tab.
/// On NFS/FUSE mounts this can block for 10-100ms per path. Always call from
/// a background thread via `spawn_blocking_then`, never on the GTK main thread.
pub fn filter_existing_tabs(session: &mut SessionData) {
    session.tabs.retain(|tab| tab.path.exists());
    if session
        .active_tab
        .as_ref()
        .is_some_and(|path| !path.exists())
    {
        session.active_tab = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::session::SessionTab;
    use tempfile::TempDir;

    #[test]
    fn test_load_missing_returns_default() {
        let dir = TempDir::new().unwrap();
        let ws_id = WorkspaceId::new("test");
        let session = load(dir.path(), &ws_id).unwrap();
        assert!(session.tabs.is_empty());
        assert_eq!(session.workspace_id, ws_id);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let ws_id = WorkspaceId::new("test");
        let session = SessionData {
            workspace_id: ws_id.clone(),
            tabs: vec![SessionTab {
                path: "/tmp/file.rs".into(),
                cursor_line: 10,
                cursor_col: 5,
                scroll_line: 8,
            }],
            active_tab: Some("/tmp/file.rs".into()),
        };

        save(dir.path(), &session).unwrap();
        let loaded = load(dir.path(), &ws_id).unwrap();

        assert_eq!(loaded.tabs.len(), 1);
        assert_eq!(loaded.tabs[0].cursor_line, 10);
        assert_eq!(loaded.active_tab, Some("/tmp/file.rs".into()));
    }

    #[test]
    fn test_load_backfills_empty_workspace_id() {
        let dir = TempDir::new().unwrap();
        let ws_id = WorkspaceId::new("my-ws");

        // Write a session file with empty workspace_id (legacy format)
        let json = r#"{"workspace_id": "", "tabs": [], "active_tab": null}"#;
        let filename = format!("session-{}.json", ws_id.as_str());
        std::fs::write(dir.path().join(filename), json).unwrap();

        let session = load(dir.path(), &ws_id).unwrap();
        assert_eq!(session.workspace_id, ws_id);
    }

    #[test]
    fn test_filter_existing_tabs_removes_missing() {
        let dir = TempDir::new().unwrap();

        let real_file = dir.path().join("exists.txt");
        std::fs::write(&real_file, "hello").unwrap();

        let ws_id = WorkspaceId::new("test");
        let mut session = SessionData {
            workspace_id: ws_id,
            tabs: vec![
                SessionTab {
                    path: real_file.clone(),
                    cursor_line: 1,
                    cursor_col: 0,
                    scroll_line: 0,
                },
                SessionTab {
                    path: "/nonexistent/file.txt".into(),
                    cursor_line: 5,
                    cursor_col: 3,
                    scroll_line: 2,
                },
            ],
            active_tab: Some("/nonexistent/file.txt".into()),
        };

        filter_existing_tabs(&mut session);

        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.tabs[0].path, real_file);
        assert_eq!(session.active_tab, None);
    }
}

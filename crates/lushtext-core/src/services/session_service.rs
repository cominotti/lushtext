// SPDX-License-Identifier: GPL-3.0-or-later

//! Session persistence: save/restore open tabs across restarts.
//!
//! Uses a single global session file (`session.json`) because tabs are
//! not workspace-scoped in the UI — all tabs share one `AdwTabView`.

use crate::model::session::SessionData;
use crate::services::json_store;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Fixed filename for the global session file.
const SESSION_FILENAME: &str = "session.json";

fn ordered_session_saves() -> &'static Mutex<HashMap<std::path::PathBuf, u64>> {
    static SAVES: OnceLock<Mutex<HashMap<std::path::PathBuf, u64>>> = OnceLock::new();
    SAVES.get_or_init(|| Mutex::new(HashMap::new()))
}

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

/// Save the global session unless a newer snapshot has already been persisted.
///
/// Window close uses this to outrank older debounced background saves that may
/// still be queued behind filesystem I/O. The lock is scoped per process and
/// per data directory so widget tests using isolated data homes do not interfere
/// with each other.
///
/// # Errors
///
/// Returns an error if the session file cannot be serialized or written.
///
/// # Panics
///
/// Panics if an earlier panic poisoned the process-local session ordering lock.
pub fn save_ordered(data_dir: &Path, session: &SessionData, generation: u64) -> Result<bool> {
    let mut generations = ordered_session_saves()
        .lock()
        .expect("session save ordering lock poisoned");
    let accepted_generation = generations.get(data_dir).copied().unwrap_or(0);
    if generation < accepted_generation {
        return Ok(false);
    }

    save(data_dir, session)?;
    generations.insert(data_dir.to_path_buf(), generation);
    Ok(true)
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
            pinned: false,
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
            pinned: false,
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
                pinned: true,
            }],
            active_tab_index: Some(0),
        };

        save(dir.path(), &session).expect("expected operation to succeed");
        let loaded = load(dir.path()).expect("expected operation to succeed");

        assert_eq!(loaded.tabs.len(), 1);
        assert_eq!(loaded.tabs[0].cursor_line, 10);
        assert_eq!(loaded.active_tab_index, Some(0));
        assert!(loaded.tabs[0].pinned);
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

    #[test]
    fn ordered_save_ignores_older_generation_after_newer_snapshot() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let older = SessionData {
            tabs: vec![tab("/tmp/older.rs", 1)],
            active_tab_index: Some(0),
        };
        let newer = SessionData {
            tabs: vec![tab("/tmp/newer.rs", 2)],
            active_tab_index: Some(0),
        };

        assert!(
            save_ordered(dir.path(), &newer, 2).expect("expected operation to succeed"),
            "newer save should be accepted"
        );
        assert!(
            !save_ordered(dir.path(), &older, 1).expect("expected operation to succeed"),
            "older save should be ignored"
        );

        let loaded = load(dir.path()).expect("expected operation to succeed");
        assert_eq!(loaded.tabs[0].path, Some("/tmp/newer.rs".into()));
    }
}

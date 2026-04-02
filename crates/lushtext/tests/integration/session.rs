// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for session persistence.

use crate::common::TestContext;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::model::workspace::WorkspaceId;
use lushtext_core::services::session_service;

#[test]
fn test_session_save_restore_roundtrip() {
    let ctx = TestContext::new();
    let ws_id = WorkspaceId::new("test-ws");

    // Create some real files to reference
    let file1 = ctx.write_file("src/main.rs", "fn main() {}");
    let file2 = ctx.write_file("src/lib.rs", "pub mod app;");

    let session = SessionData {
        workspace_id: ws_id.clone(),
        tabs: vec![
            SessionTab {
                path: file1.clone(),
                cursor_line: 1,
                cursor_col: 0,
                scroll_line: 0,
            },
            SessionTab {
                path: file2.clone(),
                cursor_line: 15,
                cursor_col: 8,
                scroll_line: 10,
            },
        ],
        active_tab: Some(file2.clone()),
    };

    session_service::save(ctx.data_dir(), &session).unwrap();
    let mut loaded = session_service::load(ctx.data_dir(), &ws_id).unwrap();

    assert_eq!(loaded.tabs.len(), 2);
    assert_eq!(loaded.tabs[1].cursor_line, 15);
    assert_eq!(loaded.active_tab, Some(file2));

    // All files exist, so filter should keep everything
    session_service::filter_existing_tabs(&mut loaded);
    assert_eq!(loaded.tabs.len(), 2);
}

#[test]
fn test_session_filter_removes_deleted_files() {
    let ctx = TestContext::new();
    let ws_id = WorkspaceId::new("test-ws");

    let real_file = ctx.write_file("still-here.txt", "content");

    let session = SessionData {
        workspace_id: ws_id.clone(),
        tabs: vec![
            SessionTab {
                path: real_file.clone(),
                cursor_line: 1,
                cursor_col: 0,
                scroll_line: 0,
            },
            SessionTab {
                path: ctx.path().join("deleted.txt"),
                cursor_line: 5,
                cursor_col: 3,
                scroll_line: 2,
            },
        ],
        active_tab: Some(ctx.path().join("deleted.txt")),
    };

    session_service::save(ctx.data_dir(), &session).unwrap();
    let mut loaded = session_service::load(ctx.data_dir(), &ws_id).unwrap();

    session_service::filter_existing_tabs(&mut loaded);
    assert_eq!(loaded.tabs.len(), 1);
    assert_eq!(loaded.tabs[0].path, real_file);
    assert_eq!(loaded.active_tab, None);
}

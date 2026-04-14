// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for session persistence.

use crate::common::TestContext;
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::services::draft_service;
use lushtext_core::services::session_service;

/// Create a file-backed session tab with cursor/scroll state.
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

/// Create a file-backed tab with full position state.
fn tab_with_position(
    path: impl Into<std::path::PathBuf>,
    cursor_line: u32,
    cursor_col: u32,
    scroll_line: u32,
) -> SessionTab {
    SessionTab {
        path: Some(path.into()),
        draft_id: None,
        cursor_line,
        cursor_col,
        scroll_line,
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

// --- Basic roundtrip tests ---

#[test]
fn test_session_save_restore_roundtrip() {
    let ctx = TestContext::new();

    let file1 = ctx.write_file("src/main.rs", "fn main() {}");
    let file2 = ctx.write_file("src/lib.rs", "pub mod app;");

    let session = SessionData {
        tabs: vec![
            tab_with_position(file1.clone(), 1, 0, 0),
            tab_with_position(file2.clone(), 15, 8, 10),
        ],
        active_tab_index: Some(1),
    };

    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let mut loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(loaded.tabs.len(), 2);
    assert_eq!(loaded.tabs[1].cursor_line, 15);
    assert_eq!(loaded.tabs[1].cursor_col, 8);
    assert_eq!(loaded.tabs[1].scroll_line, 10);
    assert_eq!(loaded.active_tab_index, Some(1));

    // All files exist, so filter should keep everything
    session_service::filter_existing_tabs(&mut loaded);
    assert_eq!(loaded.tabs.len(), 2);
}

#[test]
fn test_session_filter_removes_deleted_files() {
    let ctx = TestContext::new();

    let real_file = ctx.write_file("still-here.txt", "content");

    let session = SessionData {
        tabs: vec![
            tab(real_file.clone(), 1),
            tab(ctx.path().join("deleted.txt"), 5),
        ],
        active_tab_index: Some(1), // deleted file was active
    };

    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let mut loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");

    session_service::filter_existing_tabs(&mut loaded);
    assert_eq!(loaded.tabs.len(), 1);
    assert_eq!(loaded.tabs[0].path, Some(real_file));
    assert_eq!(loaded.active_tab_index, None); // cleared — active tab was removed
}

// --- Untitled tab persistence ---

#[test]
fn test_session_with_untitled_tabs_roundtrip() {
    let ctx = TestContext::new();

    let file1 = ctx.write_file("main.rs", "fn main() {}");

    let session = SessionData {
        tabs: vec![
            tab(file1.clone(), 1),
            untitled("untitled-0"),
            untitled("untitled-1"),
        ],
        active_tab_index: Some(1),
    };

    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(loaded.tabs.len(), 3);
    assert_eq!(loaded.tabs[0].path, Some(file1));
    assert_eq!(loaded.tabs[1].path, None);
    assert_eq!(loaded.tabs[1].draft_id, Some("untitled-0".into()));
    assert_eq!(loaded.tabs[2].path, None);
    assert_eq!(loaded.tabs[2].draft_id, Some("untitled-1".into()));
    assert_eq!(loaded.active_tab_index, Some(1));
}

#[test]
fn test_filter_preserves_untitled_tabs() {
    let ctx = TestContext::new();

    let real_file = ctx.write_file("exists.txt", "content");

    let mut session = SessionData {
        tabs: vec![
            tab(real_file.clone(), 1),
            untitled("u-0"),
            tab(ctx.path().join("gone.txt"), 3),
            untitled("u-1"),
        ],
        active_tab_index: Some(1), // untitled tab
    };

    session_service::filter_existing_tabs(&mut session);

    assert_eq!(session.tabs.len(), 3); // real_file + 2 untitled
    assert_eq!(session.tabs[0].path, Some(real_file));
    assert_eq!(session.tabs[1].path, None); // u-0
    assert_eq!(session.tabs[1].draft_id, Some("u-0".into()));
    assert_eq!(session.tabs[2].path, None); // u-1
    assert_eq!(session.tabs[2].draft_id, Some("u-1".into()));
    assert_eq!(session.active_tab_index, Some(1)); // untitled survived
}

#[test]
fn test_filter_untitled_only_session_survives_intact() {
    let mut session = SessionData {
        tabs: vec![untitled("u-0"), untitled("u-1"), untitled("u-2")],
        active_tab_index: Some(2),
    };

    session_service::filter_existing_tabs(&mut session);

    assert_eq!(session.tabs.len(), 3);
    assert_eq!(session.active_tab_index, Some(2));
}

// --- Active tab index tracking ---

#[test]
fn test_filter_adjusts_active_index_when_preceding_tab_removed() {
    let ctx = TestContext::new();

    let file_b = ctx.write_file("b.txt", "b");
    let file_c = ctx.write_file("c.txt", "c");

    let mut session = SessionData {
        tabs: vec![
            tab(ctx.path().join("gone.txt"), 1), // index 0, will be removed
            tab(file_b.clone(), 2),              // index 1 → becomes 0
            tab(file_c.clone(), 3),              // index 2 → becomes 1 (active)
        ],
        active_tab_index: Some(2), // c.txt
    };

    session_service::filter_existing_tabs(&mut session);

    assert_eq!(session.tabs.len(), 2);
    assert_eq!(session.active_tab_index, Some(1)); // shifted from 2→1
    assert_eq!(session.tabs[1].path, Some(file_c));
}

#[test]
fn test_filter_active_index_cleared_when_active_tab_removed() {
    let ctx = TestContext::new();

    let file_a = ctx.write_file("a.txt", "a");

    let mut session = SessionData {
        tabs: vec![tab(file_a.clone(), 1), tab(ctx.path().join("gone.txt"), 2)],
        active_tab_index: Some(1), // gone.txt
    };

    session_service::filter_existing_tabs(&mut session);

    assert_eq!(session.tabs.len(), 1);
    assert_eq!(session.active_tab_index, None);
}

#[test]
fn test_filter_no_active_tab_remains_none() {
    let ctx = TestContext::new();

    let file_a = ctx.write_file("a.txt", "a");

    let mut session = SessionData {
        tabs: vec![tab(file_a, 1)],
        active_tab_index: None,
    };

    session_service::filter_existing_tabs(&mut session);
    assert_eq!(session.active_tab_index, None);
}

// --- Edge cases ---

#[test]
fn test_empty_session_roundtrip() {
    let ctx = TestContext::new();

    let session = SessionData::default();
    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");

    assert!(loaded.tabs.is_empty());
    assert_eq!(loaded.active_tab_index, None);
}

#[test]
fn test_load_nonexistent_returns_default() {
    let ctx = TestContext::new();
    let session = session_service::load(ctx.data_dir()).expect("expected operation to succeed");
    assert!(session.tabs.is_empty());
    assert_eq!(session.active_tab_index, None);
}

#[test]
fn test_filter_empty_session_is_noop() {
    let mut session = SessionData::default();
    session_service::filter_existing_tabs(&mut session);
    assert!(session.tabs.is_empty());
    assert_eq!(session.active_tab_index, None);
}

#[test]
fn test_filter_all_files_deleted_clears_everything() {
    let ctx = TestContext::new();
    let mut session = SessionData {
        tabs: vec![
            tab(ctx.path().join("gone1.txt"), 1),
            tab(ctx.path().join("gone2.txt"), 2),
        ],
        active_tab_index: Some(0),
    };

    session_service::filter_existing_tabs(&mut session);
    assert!(session.tabs.is_empty());
    assert_eq!(session.active_tab_index, None);
}

#[test]
fn test_save_overwrites_previous_session() {
    let ctx = TestContext::new();

    let session1 = SessionData {
        tabs: vec![tab("/old.rs", 1)],
        active_tab_index: Some(0),
    };
    session_service::save(ctx.data_dir(), &session1).expect("expected operation to succeed");

    let session2 = SessionData {
        tabs: vec![tab("/new.rs", 5), tab("/also.rs", 10)],
        active_tab_index: Some(1),
    };
    session_service::save(ctx.data_dir(), &session2).expect("expected operation to succeed");

    let loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");
    assert_eq!(loaded.tabs.len(), 2);
    assert_eq!(loaded.tabs[0].path, Some("/new.rs".into()));
    assert_eq!(loaded.active_tab_index, Some(1));
}

// --- Position persistence ---

#[test]
fn test_cursor_and_scroll_positions_persist() {
    let ctx = TestContext::new();

    let session = SessionData {
        tabs: vec![
            tab_with_position("/a.rs", 42, 15, 30),
            tab_with_position("/b.rs", 100, 0, 90),
        ],
        active_tab_index: Some(0),
    };

    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(loaded.tabs[0].cursor_line, 42);
    assert_eq!(loaded.tabs[0].cursor_col, 15);
    assert_eq!(loaded.tabs[0].scroll_line, 30);
    assert_eq!(loaded.tabs[1].cursor_line, 100);
    assert_eq!(loaded.tabs[1].cursor_col, 0);
    assert_eq!(loaded.tabs[1].scroll_line, 90);
}

// --- Mixed scenarios ---

#[test]
fn test_complex_mixed_session_filter() {
    let ctx = TestContext::new();

    let file_a = ctx.write_file("a.txt", "a");
    let file_c = ctx.write_file("c.txt", "c");

    let mut session = SessionData {
        tabs: vec![
            tab(file_a.clone(), 1),               // 0: survives → 0
            tab(ctx.path().join("gone1.txt"), 2), // 1: removed
            untitled("u-0"),                      // 2: survives → 1
            tab(file_c.clone(), 4),               // 3: survives → 2
            tab(ctx.path().join("gone2.txt"), 5), // 4: removed
            untitled("u-1"),                      // 5: survives → 3
        ],
        active_tab_index: Some(3), // c.txt
    };

    session_service::filter_existing_tabs(&mut session);

    assert_eq!(session.tabs.len(), 4);
    assert_eq!(session.tabs[0].path, Some(file_a));
    assert_eq!(session.tabs[1].path, None); // u-0
    assert_eq!(session.tabs[2].path, Some(file_c));
    assert_eq!(session.tabs[3].path, None); // u-1
    assert_eq!(session.active_tab_index, Some(2)); // c.txt shifted from 3→2
}

#[test]
fn test_many_tabs_roundtrip() {
    let ctx = TestContext::new();

    let tabs: Vec<SessionTab> = (0..50)
        .map(|i| {
            let file = ctx.write_file(&format!("file_{i}.txt"), &format!("content {i}"));
            tab_with_position(file, i, i * 2, i * 3)
        })
        .collect();

    let session = SessionData {
        tabs,
        active_tab_index: Some(25),
    };

    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(loaded.tabs.len(), 50);
    assert_eq!(loaded.active_tab_index, Some(25));
    assert_eq!(loaded.tabs[25].cursor_line, 25);
    assert_eq!(loaded.tabs[25].cursor_col, 50);
    assert_eq!(loaded.tabs[25].scroll_line, 75);
}

#[test]
fn test_session_roundtrip_preserves_pinned_segment_state() {
    let ctx = TestContext::new();

    let pinned_file = ctx.write_file("pinned.rs", "fn pinned() {}");
    let regular_file = ctx.write_file("regular.rs", "fn regular() {}");

    let session = SessionData {
        tabs: vec![
            SessionTab {
                path: Some(pinned_file.clone()),
                draft_id: None,
                cursor_line: 1,
                cursor_col: 0,
                scroll_line: 0,
                pinned: true,
            },
            SessionTab {
                path: Some(regular_file.clone()),
                draft_id: None,
                cursor_line: 2,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            },
        ],
        active_tab_index: Some(1),
    };

    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(loaded.tabs.len(), 2);
    assert_eq!(loaded.tabs[0].path, Some(pinned_file));
    assert!(loaded.tabs[0].pinned);
    assert_eq!(loaded.tabs[1].path, Some(regular_file));
    assert!(!loaded.tabs[1].pinned);
    assert_eq!(loaded.active_tab_index, Some(1));
}

#[test]
fn test_active_index_out_of_bounds_preserved_in_serialization() {
    // The model doesn't enforce bounds — that's the UI layer's job.
    let ctx = TestContext::new();

    let session = SessionData {
        tabs: vec![tab("/a.rs", 1)],
        active_tab_index: Some(99), // out of bounds
    };

    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");
    assert_eq!(loaded.active_tab_index, Some(99));
}

#[test]
fn test_startup_restore_load_preserves_temporarily_unavailable_file_tabs() {
    let ctx = TestContext::new();

    let missing_path = ctx.path().join("offline-share/notes.md");
    let real_file = ctx.write_file("local.txt", "local");
    let real_draft_id = draft_service::draft_id_for_path(&real_file);
    draft_service::write_draft(ctx.data_dir(), &real_draft_id, "drafted local")
        .expect("expected operation to succeed");
    draft_service::save_manifest(
        ctx.data_dir(),
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: real_draft_id.clone(),
                original_path: Some(real_file.clone()),
                original_mtime_secs: None,
                saved_at_secs: 1,
            }],
        },
    )
    .expect("expected operation to succeed");

    let session = SessionData {
        tabs: vec![tab(missing_path.clone(), 5), tab(real_file.clone(), 1)],
        active_tab_index: Some(0),
    };
    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");

    let (_manifest, restored_session, preloaded) =
        draft_service::load_restore_state(ctx.data_dir());

    assert_eq!(restored_session.tabs.len(), 2);
    assert_eq!(restored_session.tabs[0].path, Some(missing_path));
    assert_eq!(restored_session.tabs[1].path, Some(real_file));
    assert_eq!(restored_session.active_tab_index, Some(0));
    assert_eq!(
        preloaded.get(&real_draft_id).map(String::as_str),
        Some("drafted local")
    );
}

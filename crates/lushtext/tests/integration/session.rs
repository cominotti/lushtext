// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for session persistence.

use crate::common::TestContext;
use lushtext_core::model::draft::{
    DraftEntry, DraftManifest, PreloadedDraftRestore, PreloadedDraftSkip,
};
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::services::filesystem::fixture;
use lushtext_core::services::recovery_metadata::{RecoveryMetadataClass, RecoveryProblem};
use lushtext_core::services::session_service;
use lushtext_core::services::{draft_service, editor_io};

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
            tab_with_position(file1, 1, 0, 0),
            tab_with_position(file2, 15, 8, 10),
        ],
        active_tab_index: Some(1),
    };

    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");
    let loaded = session_service::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(loaded.tabs.len(), 2);
    assert_eq!(loaded.tabs[1].cursor_line, 15);
    assert_eq!(loaded.tabs[1].cursor_col, 8);
    assert_eq!(loaded.tabs[1].scroll_line, 10);
    assert_eq!(loaded.active_tab_index, Some(1));
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
            cleanup_continuation: None,
        },
    )
    .expect("expected operation to succeed");

    let session = SessionData {
        tabs: vec![tab(missing_path.clone(), 5), tab(real_file.clone(), 1)],
        active_tab_index: Some(0),
    };
    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");

    let restore = draft_service::load_restore_state(ctx.data_dir());

    assert_eq!(restore.session.tabs.len(), 2);
    assert_eq!(restore.session.tabs[0].path, Some(missing_path));
    assert_eq!(restore.session.tabs[1].path, Some(real_file));
    assert_eq!(restore.session.active_tab_index, Some(0));
    assert_eq!(
        restore.preloaded_drafts.get(&real_draft_id),
        Some(&PreloadedDraftRestore::Content("drafted local".to_string()))
    );
}

#[test]
fn test_startup_restore_load_marks_stale_file_backed_drafts_and_removes_them() {
    let ctx = TestContext::new();

    let file_path = ctx.write_file("stale.txt", "current disk content");
    let draft_id = draft_service::draft_id_for_path(&file_path);
    draft_service::write_draft(ctx.data_dir(), &draft_id, "stale draft content")
        .expect("expected operation to succeed");
    let current_mtime = editor_io::mtime_secs(&file_path).expect("expected file mtime");
    let stale_mtime = current_mtime
        .checked_add(1)
        .unwrap_or_else(|| current_mtime.saturating_sub(1));

    draft_service::save_manifest(
        ctx.data_dir(),
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.clone(),
                original_path: Some(file_path.clone()),
                original_mtime_secs: Some(stale_mtime),
                saved_at_secs: 1,
            }],
            cleanup_continuation: None,
        },
    )
    .expect("expected operation to succeed");

    let session = SessionData {
        tabs: vec![tab(file_path.clone(), 1)],
        active_tab_index: Some(0),
    };
    session_service::save(ctx.data_dir(), &session).expect("expected operation to succeed");

    let restore = draft_service::load_restore_state(ctx.data_dir());

    assert_eq!(restore.session.tabs.len(), 1);
    assert_eq!(restore.session.tabs[0].path, Some(file_path));
    assert_eq!(
        restore.preloaded_drafts.get(&draft_id),
        Some(&PreloadedDraftRestore::Skip(PreloadedDraftSkip::StaleFile))
    );
    assert!(restore.manifest.find_by_id(&draft_id).is_none());
    assert_eq!(
        draft_service::read_draft(ctx.data_dir(), &draft_id).expect("read stale draft"),
        None,
        "confirmed-stale draft files should be deleted during preload cleanup",
    );
}

#[test]
fn test_startup_restore_reports_corrupt_session_json_without_deleting_drafts() {
    let ctx = TestContext::new();
    let draft_id = "untitled-0000000000000099";
    draft_service::write_draft(ctx.data_dir(), draft_id, "valid draft").expect("write draft");
    draft_service::save_manifest(
        ctx.data_dir(),
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.to_string(),
                original_path: None,
                original_mtime_secs: None,
                saved_at_secs: 1,
            }],
            cleanup_continuation: None,
        },
    )
    .expect("save manifest");
    fixture::write_text(&ctx.data_dir().join("session.json"), "not json");

    let restore = draft_service::load_restore_state(ctx.data_dir());

    assert!(restore.session.tabs.is_empty());
    assert!(
        restore.manifest.find_by_id(draft_id).is_some(),
        "valid manifest state should survive corrupt session metadata"
    );
    assert_eq!(
        draft_service::read_draft(ctx.data_dir(), draft_id).expect("read draft"),
        Some("valid draft".to_string())
    );
    assert!(restore.diagnostics.iter().any(|diagnostic| {
        diagnostic.class == RecoveryMetadataClass::Session
            && matches!(diagnostic.problem, RecoveryProblem::Malformed { .. })
    }));
}

#[test]
fn test_startup_restore_repairs_corrupt_manifest_for_untitled_draft() {
    let ctx = TestContext::new();
    let draft_id = "untitled-0000000000000100";
    draft_service::write_draft(ctx.data_dir(), draft_id, "restored text").expect("write draft");
    fixture::write_text(
        &draft_service::drafts_dir(ctx.data_dir()).join("manifest.json"),
        "not json",
    );
    session_service::save(
        ctx.data_dir(),
        &SessionData {
            tabs: vec![untitled(draft_id)],
            active_tab_index: Some(0),
        },
    )
    .expect("save session");

    let restore = draft_service::load_restore_state(ctx.data_dir());

    assert_eq!(restore.session.tabs.len(), 1);
    assert_eq!(
        restore.preloaded_drafts.get(draft_id),
        Some(&PreloadedDraftRestore::Content("restored text".to_string()))
    );
    assert!(
        restore.manifest.find_by_id(draft_id).is_some(),
        "safe untitled draft should be rebuilt into the manifest"
    );
    assert!(restore.diagnostics.iter().any(|diagnostic| {
        diagnostic.class == RecoveryMetadataClass::DraftManifest
            && matches!(diagnostic.problem, RecoveryProblem::Malformed { .. })
    }));
    assert!(restore.diagnostics.iter().any(|diagnostic| {
        diagnostic.class == RecoveryMetadataClass::DraftManifest
            && matches!(diagnostic.problem, RecoveryProblem::Repaired { .. })
    }));
    assert!(
        draft_service::load_manifest(ctx.data_dir())
            .expect("load repaired manifest")
            .find_by_id(draft_id)
            .is_some()
    );
}

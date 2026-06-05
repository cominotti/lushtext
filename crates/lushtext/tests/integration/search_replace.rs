// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for search/replace persistence safety.
//!
//! These tests stay GTK-free while exercising the same app-data journal files
//! the search panel uses to keep Replace All undo state crash-aware.

use std::path::PathBuf;

use lushtext_core::services::{
    content_search::{ReplaceUndoBackup, ReplaceUndoEntry},
    filesystem::{fixture, metadata as fs_metadata},
    search_backup,
};

use crate::common::TestContext;

const JOURNAL_DIR: &str = "replace-backup-journal";
const CLEANUP_MARKER_FILE: &str = "cleanup-in-progress.json";

#[test]
fn interrupted_startup_cleanup_never_reactivates_replace_undo() {
    let ctx = TestContext::new();
    let backup = sample_backup();
    search_backup::save(ctx.data_dir(), &backup).expect("save active replace journal");
    fixture::write_text(
        &ctx.data_dir().join(JOURNAL_DIR).join(CLEANUP_MARKER_FILE),
        r#"{"reason":"simulated interrupted startup cleanup"}"#,
    );

    let after_restart = search_backup::load_recovering(ctx.data_dir());

    assert!(!after_restart.active);
    assert!(after_restart.backup.is_empty());
    assert!(!after_restart.diagnostics.is_empty());
    assert!(
        search_backup::load(ctx.data_dir())
            .expect("interrupted cleanup should load as inactive")
            .is_empty()
    );

    let cleanup = search_backup::cleanup_stale(ctx.data_dir());
    assert!(cleanup.diagnostics.is_empty());
    assert!(!fs_metadata::exists(&ctx.data_dir().join(JOURNAL_DIR)));
}

#[test]
fn undo_completion_cleanup_remains_empty_across_restart() {
    let ctx = TestContext::new();
    let backup = sample_backup();
    search_backup::save(ctx.data_dir(), &backup).expect("save active replace journal");
    assert_eq!(
        search_backup::load(ctx.data_dir()).expect("active journal loads"),
        backup
    );

    search_backup::delete(ctx.data_dir()).expect("undo completion cleanup");

    let after_restart = search_backup::load_recovering(ctx.data_dir());
    assert!(!after_restart.active);
    assert!(after_restart.backup.is_empty());
    assert!(after_restart.diagnostics.is_empty());
}

fn sample_backup() -> ReplaceUndoBackup {
    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        PathBuf::from("/tmp/lushtext-replace-a.txt"),
        ReplaceUndoEntry::new(b"before-a".to_vec(), b"after-a".to_vec()),
    );
    backup.insert(
        PathBuf::from("/tmp/lushtext-replace-b.txt"),
        ReplaceUndoEntry::new(b"before-b".to_vec(), b"after-b".to_vec()),
    );
    backup
}

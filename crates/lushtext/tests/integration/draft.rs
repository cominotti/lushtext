// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for draft persistence.
//!
//! Tests exercise the full draft lifecycle: create, detect, restore, delete,
//! and orphan cleanup through `draft_service` and `DraftManifest`.

use super::common::TestContext;
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::services::draft_service;
use std::path::PathBuf;

// --- Draft ID generation ---

#[test]
fn draft_id_for_same_path_is_stable() {
    let id1 = draft_service::draft_id_for_path(&PathBuf::from("/home/user/main.rs"));
    let id2 = draft_service::draft_id_for_path(&PathBuf::from("/home/user/main.rs"));
    assert_eq!(id1, id2);
}

#[test]
fn draft_id_differs_across_paths() {
    let id1 = draft_service::draft_id_for_path(&PathBuf::from("/a.rs"));
    let id2 = draft_service::draft_id_for_path(&PathBuf::from("/b.rs"));
    assert_ne!(id1, id2);
}

#[test]
fn untitled_draft_ids_are_unique() {
    let id1 = draft_service::draft_id_for_untitled(0);
    let id2 = draft_service::draft_id_for_untitled(1);
    assert_ne!(id1, id2);
    assert!(id1.starts_with("untitled-"));
}

// --- Full draft lifecycle ---

#[test]
fn draft_lifecycle_create_detect_delete() {
    let ctx = TestContext::new();
    let file_path = ctx.write_file("project/main.rs", "fn main() {}");

    let draft_id = draft_service::draft_id_for_path(&file_path);
    let draft_content = "fn main() { println!(\"modified\"); }";

    // Create draft
    draft_service::write_draft(ctx.data_dir(), &draft_id, draft_content).unwrap();

    // Detect draft
    let read_back = draft_service::read_draft(ctx.data_dir(), &draft_id).unwrap();
    assert_eq!(read_back, Some(draft_content.to_string()));

    // Delete draft
    draft_service::delete_draft_file(ctx.data_dir(), &draft_id).unwrap();

    // Verify deleted
    let after_delete = draft_service::read_draft(ctx.data_dir(), &draft_id).unwrap();
    assert_eq!(after_delete, None);
}

// --- Manifest lifecycle ---

#[test]
fn manifest_persist_and_restore() {
    let ctx = TestContext::new();

    let entry = DraftEntry {
        draft_id: "abc123".into(),
        original_path: Some(PathBuf::from("/project/main.rs")),
        original_mtime_secs: Some(1700000000),
        saved_at_secs: 1700000030,
    };

    let mut manifest = DraftManifest::default();
    manifest.upsert(entry.clone());

    draft_service::save_manifest(ctx.data_dir(), &manifest).unwrap();
    let loaded = draft_service::load_manifest(ctx.data_dir()).unwrap();

    assert_eq!(loaded.drafts.len(), 1);
    assert_eq!(loaded.drafts[0], entry);
}

#[test]
fn manifest_upsert_updates_existing() {
    let ctx = TestContext::new();

    let entry1 = DraftEntry {
        draft_id: "abc".into(),
        original_path: Some(PathBuf::from("/a.rs")),
        original_mtime_secs: Some(1000),
        saved_at_secs: 2000,
    };

    let mut manifest = DraftManifest::default();
    manifest.upsert(entry1);

    let entry2 = DraftEntry {
        draft_id: "abc".into(),
        original_path: Some(PathBuf::from("/a.rs")),
        original_mtime_secs: Some(3000),
        saved_at_secs: 4000,
    };
    manifest.upsert(entry2.clone());

    draft_service::save_manifest(ctx.data_dir(), &manifest).unwrap();
    let loaded = draft_service::load_manifest(ctx.data_dir()).unwrap();

    assert_eq!(loaded.drafts.len(), 1);
    assert_eq!(loaded.drafts[0].saved_at_secs, 4000);
}

// --- Orphan cleanup ---

#[test]
fn cleanup_removes_manifest_entries_without_draft_files() {
    let ctx = TestContext::new();

    let mut manifest = DraftManifest {
        drafts: vec![DraftEntry {
            draft_id: "ghost".into(),
            original_path: Some(PathBuf::from("/gone.rs")),
            original_mtime_secs: None,
            saved_at_secs: 1000,
        }],
    };

    // Create the drafts directory but NOT the draft file
    std::fs::create_dir_all(draft_service::drafts_dir(ctx.data_dir())).unwrap();

    let cleaned = draft_service::cleanup_orphans(ctx.data_dir(), &mut manifest).unwrap();
    assert_eq!(cleaned, 1);
    assert!(manifest.drafts.is_empty());
}

#[test]
fn cleanup_removes_draft_files_without_manifest_entries() {
    let ctx = TestContext::new();

    // Write a draft file with no manifest entry
    draft_service::write_draft(ctx.data_dir(), "orphan", "stale content").unwrap();
    let mut manifest = DraftManifest::default();

    let cleaned = draft_service::cleanup_orphans(ctx.data_dir(), &mut manifest).unwrap();
    assert_eq!(cleaned, 1);

    // File should be gone
    assert_eq!(
        draft_service::read_draft(ctx.data_dir(), "orphan").unwrap(),
        None
    );
}

#[test]
fn cleanup_preserves_valid_drafts() {
    let ctx = TestContext::new();

    // Write draft file AND manifest entry
    draft_service::write_draft(ctx.data_dir(), "valid", "content").unwrap();
    let mut manifest = DraftManifest {
        drafts: vec![DraftEntry {
            draft_id: "valid".into(),
            original_path: Some(PathBuf::from("/a.rs")),
            original_mtime_secs: None,
            saved_at_secs: 1000,
        }],
    };

    let cleaned = draft_service::cleanup_orphans(ctx.data_dir(), &mut manifest).unwrap();
    assert_eq!(cleaned, 0);
    assert_eq!(manifest.drafts.len(), 1);
    assert_eq!(
        draft_service::read_draft(ctx.data_dir(), "valid").unwrap(),
        Some("content".into())
    );
}

// --- Edge cases ---

#[test]
fn write_draft_with_large_content() {
    let ctx = TestContext::new();
    let large_content = "a".repeat(1_000_000); // 1MB
    draft_service::write_draft(ctx.data_dir(), "large", &large_content).unwrap();
    let read_back = draft_service::read_draft(ctx.data_dir(), "large").unwrap();
    assert_eq!(read_back.as_deref(), Some(large_content.as_str()));
}

#[test]
fn write_draft_with_unicode_content() {
    let ctx = TestContext::new();
    let content = "fn main() { println!(\"日本語\"); } // 🦀\n";
    draft_service::write_draft(ctx.data_dir(), "unicode", content).unwrap();
    let read_back = draft_service::read_draft(ctx.data_dir(), "unicode").unwrap();
    assert_eq!(read_back, Some(content.to_string()));
}

#[test]
fn manifest_find_by_path_with_multiple_entries() {
    let manifest = DraftManifest {
        drafts: vec![
            DraftEntry {
                draft_id: "id1".into(),
                original_path: Some(PathBuf::from("/a.rs")),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            },
            DraftEntry {
                draft_id: "id2".into(),
                original_path: Some(PathBuf::from("/b.rs")),
                original_mtime_secs: None,
                saved_at_secs: 2000,
            },
            DraftEntry {
                draft_id: "id3".into(),
                original_path: None, // untitled
                original_mtime_secs: None,
                saved_at_secs: 3000,
            },
        ],
    };

    assert_eq!(
        manifest
            .find_by_path(std::path::Path::new("/b.rs"))
            .unwrap()
            .draft_id,
        "id2"
    );
    assert!(
        manifest
            .find_by_path(std::path::Path::new("/c.rs"))
            .is_none()
    );
}

#[test]
fn manifest_remove_by_path_leaves_others_intact() {
    let mut manifest = DraftManifest {
        drafts: vec![
            DraftEntry {
                draft_id: "id1".into(),
                original_path: Some(PathBuf::from("/a.rs")),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            },
            DraftEntry {
                draft_id: "id2".into(),
                original_path: Some(PathBuf::from("/b.rs")),
                original_mtime_secs: None,
                saved_at_secs: 2000,
            },
        ],
    };

    assert!(manifest.remove_by_path(std::path::Path::new("/a.rs")));
    assert_eq!(manifest.drafts.len(), 1);
    assert_eq!(manifest.drafts[0].draft_id, "id2");
}

#[test]
fn delete_nonexistent_draft_is_ok() {
    let ctx = TestContext::new();
    // Should not error even without the drafts directory existing
    draft_service::delete_draft_file(ctx.data_dir(), "does_not_exist").unwrap();
}

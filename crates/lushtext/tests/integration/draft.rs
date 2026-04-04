// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for draft persistence.
//!
//! Tests exercise the full draft lifecycle: create, detect, restore, delete,
//! and orphan cleanup through `draft_service` and `DraftManifest`.

use super::common::TestContext;
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::services::draft_service;
use std::collections::HashSet;
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

// --- Merge-back pattern for deferred orphan cleanup ---
// These tests validate the logic used by schedule_orphan_cleanup:
// run cleanup_orphans on a snapshot, compute removed IDs, apply removals
// to the live manifest. Entries added concurrently must survive.

#[test]
fn cleanup_merge_back_preserves_concurrent_additions() {
    let ctx = TestContext::new();

    // Initial manifest: one valid entry + one orphan (no draft file).
    let valid_entry = DraftEntry {
        draft_id: "valid".into(),
        original_path: Some(PathBuf::from("/a.rs")),
        original_mtime_secs: None,
        saved_at_secs: 1000,
    };
    let orphan_entry = DraftEntry {
        draft_id: "orphan".into(),
        original_path: Some(PathBuf::from("/gone.rs")),
        original_mtime_secs: None,
        saved_at_secs: 1000,
    };
    draft_service::write_draft(ctx.data_dir(), "valid", "content").unwrap();
    // Don't create file for "orphan" — it will be cleaned up.

    // Snapshot (simulating the clone before background work).
    let mut snapshot = DraftManifest {
        drafts: vec![valid_entry.clone(), orphan_entry.clone()],
    };
    let ids_before: Vec<String> = snapshot.drafts.iter().map(|e| e.draft_id.clone()).collect();

    // Run cleanup on the snapshot (simulating background thread).
    draft_service::cleanup_orphans(ctx.data_dir(), &mut snapshot).unwrap();

    // Compute removed IDs.
    let ids_after: HashSet<&str> = snapshot
        .drafts
        .iter()
        .map(|e| e.draft_id.as_str())
        .collect();
    let removed: Vec<String> = ids_before
        .into_iter()
        .filter(|id| !ids_after.contains(id.as_str()))
        .collect();

    assert_eq!(removed, vec!["orphan".to_string()]);

    // Simulate a concurrent addition to the live manifest (e.g., autosave
    // added a new entry while cleanup was running in the background).
    let concurrent_entry = DraftEntry {
        draft_id: "new_during_cleanup".into(),
        original_path: Some(PathBuf::from("/new.rs")),
        original_mtime_secs: None,
        saved_at_secs: 2000,
    };
    let mut live_manifest = DraftManifest {
        drafts: vec![valid_entry, orphan_entry, concurrent_entry.clone()],
    };

    // Apply removals to the live manifest (simulating the main-thread callback).
    live_manifest
        .drafts
        .retain(|e| !removed.contains(&e.draft_id));

    // The concurrent addition must survive.
    assert_eq!(live_manifest.drafts.len(), 2);
    assert!(live_manifest.find_by_id("valid").is_some());
    assert!(live_manifest.find_by_id("new_during_cleanup").is_some());
    assert!(live_manifest.find_by_id("orphan").is_none());
}

#[test]
fn cleanup_merge_back_empty_removal_is_noop() {
    // When cleanup removes nothing, the live manifest is untouched.
    let ctx = TestContext::new();

    let entry = DraftEntry {
        draft_id: "good".into(),
        original_path: Some(PathBuf::from("/a.rs")),
        original_mtime_secs: None,
        saved_at_secs: 1000,
    };
    draft_service::write_draft(ctx.data_dir(), "good", "content").unwrap();

    let mut snapshot = DraftManifest {
        drafts: vec![entry.clone()],
    };
    let ids_before: Vec<String> = snapshot.drafts.iter().map(|e| e.draft_id.clone()).collect();

    draft_service::cleanup_orphans(ctx.data_dir(), &mut snapshot).unwrap();

    let ids_after: HashSet<&str> = snapshot
        .drafts
        .iter()
        .map(|e| e.draft_id.as_str())
        .collect();
    let removed: Vec<String> = ids_before
        .into_iter()
        .filter(|id| !ids_after.contains(id.as_str()))
        .collect();

    assert!(removed.is_empty());
    assert_eq!(snapshot.drafts.len(), 1);
}

// --- Batch draft preload (used by load_session_and_drafts) ---

#[test]
fn batch_preload_reads_matching_drafts() {
    let ctx = TestContext::new();

    // Create drafts for two paths.
    let path_a = PathBuf::from("/home/user/a.rs");
    let path_b = PathBuf::from("/home/user/b.rs");
    let id_a = draft_service::draft_id_for_path(&path_a);
    let id_b = draft_service::draft_id_for_path(&path_b);

    draft_service::write_draft(ctx.data_dir(), &id_a, "content_a").unwrap();
    draft_service::write_draft(ctx.data_dir(), &id_b, "content_b").unwrap();

    let manifest = DraftManifest {
        drafts: vec![
            DraftEntry {
                draft_id: id_a.clone(),
                original_path: Some(path_a),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            },
            DraftEntry {
                draft_id: id_b.clone(),
                original_path: Some(path_b),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            },
        ],
    };

    // Simulate the batch preload loop from load_session_and_drafts.
    let mut preloaded = std::collections::HashMap::new();
    for entry in &manifest.drafts {
        if let Ok(Some(content)) = draft_service::read_draft(ctx.data_dir(), &entry.draft_id) {
            preloaded.insert(entry.draft_id.clone(), content);
        }
    }

    assert_eq!(preloaded.len(), 2);
    assert_eq!(preloaded[&id_a], "content_a");
    assert_eq!(preloaded[&id_b], "content_b");
}

#[test]
fn batch_preload_skips_missing_draft_files() {
    let ctx = TestContext::new();

    // Manifest entry exists but draft file does not.
    let path = PathBuf::from("/home/user/gone.rs");
    let id = draft_service::draft_id_for_path(&path);

    let manifest = DraftManifest {
        drafts: vec![DraftEntry {
            draft_id: id.clone(),
            original_path: Some(path),
            original_mtime_secs: None,
            saved_at_secs: 1000,
        }],
    };

    let mut preloaded = std::collections::HashMap::new();
    for entry in &manifest.drafts {
        if let Ok(Some(content)) = draft_service::read_draft(ctx.data_dir(), &entry.draft_id) {
            preloaded.insert(entry.draft_id.clone(), content);
        }
    }

    // Missing file should be silently skipped.
    assert!(preloaded.is_empty());
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for draft persistence.
//!
//! Tests exercise the full draft lifecycle: create, detect, restore, delete,
//! and orphan cleanup through `draft_service` and `DraftManifest`.

use super::common::TestContext;
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::services::draft_service;
use lushtext_core::services::filesystem::fixture;
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
    draft_service::write_draft(ctx.data_dir(), &draft_id, draft_content)
        .expect("expected operation to succeed");

    // Detect draft
    let read_back = draft_service::read_draft(ctx.data_dir(), &draft_id)
        .expect("expected operation to succeed");
    assert_eq!(read_back, Some(draft_content.to_string()));

    // Delete draft
    draft_service::delete_draft_file(ctx.data_dir(), &draft_id)
        .expect("expected operation to succeed");

    // Verify deleted
    let after_delete = draft_service::read_draft(ctx.data_dir(), &draft_id)
        .expect("expected operation to succeed");
    assert_eq!(after_delete, None);
}

// --- Manifest lifecycle ---

#[test]
fn manifest_persist_and_restore() {
    let ctx = TestContext::new();

    let entry = DraftEntry {
        draft_id: "abc123".into(),
        original_path: Some(PathBuf::from("/project/main.rs")),
        original_mtime_secs: Some(1_700_000_000),
        saved_at_secs: 1_700_000_030,
    };

    let mut manifest = DraftManifest::default();
    manifest.upsert(entry.clone());

    draft_service::save_manifest(ctx.data_dir(), &manifest).expect("expected operation to succeed");
    let loaded =
        draft_service::load_manifest(ctx.data_dir()).expect("expected operation to succeed");

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
    manifest.upsert(entry2);

    draft_service::save_manifest(ctx.data_dir(), &manifest).expect("expected operation to succeed");
    let loaded =
        draft_service::load_manifest(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(loaded.drafts.len(), 1);
    assert_eq!(loaded.drafts[0].saved_at_secs, 4000);
}

// --- Orphan cleanup ---

#[test]
fn cleanup_removes_manifest_entries_without_draft_files() {
    let ctx = TestContext::new();

    let manifest = DraftManifest {
        drafts: vec![DraftEntry {
            draft_id: "ghost".into(),
            original_path: Some(PathBuf::from("/gone.rs")),
            original_mtime_secs: None,
            saved_at_secs: 1000,
        }],
    };

    draft_service::save_manifest(ctx.data_dir(), &manifest).expect("seed manifest");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &manifest)
        .expect("inspection should succeed");

    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert_eq!(outcome.confirmed_cleaned_count(), 1);
    assert_eq!(outcome.committed_manifest_removals.len(), 1);
    assert!(
        draft_service::load_manifest(ctx.data_dir())
            .expect("load cleaned manifest")
            .drafts
            .is_empty()
    );
}

#[test]
fn cleanup_removes_draft_files_without_manifest_entries() {
    let ctx = TestContext::new();

    draft_service::write_draft(ctx.data_dir(), "orphan", "stale content")
        .expect("expected operation to succeed");
    let manifest = DraftManifest::default();
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &manifest)
        .expect("inspection should succeed");

    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert_eq!(outcome.confirmed_cleaned_count(), 1);
    assert_eq!(outcome.deleted_files.len(), 1);

    assert_eq!(
        draft_service::read_draft(ctx.data_dir(), "orphan").expect("expected operation to succeed"),
        None
    );
}

#[test]
fn cleanup_preserves_valid_drafts() {
    let ctx = TestContext::new();

    draft_service::write_draft(ctx.data_dir(), "valid", "content")
        .expect("expected operation to succeed");
    let manifest = DraftManifest {
        drafts: vec![DraftEntry {
            draft_id: "valid".into(),
            original_path: Some(PathBuf::from("/a.rs")),
            original_mtime_secs: None,
            saved_at_secs: 1000,
        }],
    };

    draft_service::save_manifest(ctx.data_dir(), &manifest).expect("seed manifest");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &manifest)
        .expect("inspection should succeed");
    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert_eq!(outcome.confirmed_cleaned_count(), 0);
    assert_eq!(
        outcome
            .latest_persisted_manifest
            .expect("trusted latest manifest")
            .drafts
            .len(),
        1
    );
    assert_eq!(
        draft_service::read_draft(ctx.data_dir(), "valid").expect("expected operation to succeed"),
        Some("content".into())
    );
}

// --- Edge cases ---

#[test]
fn write_draft_with_large_content() {
    let ctx = TestContext::new();
    let large_content = "a".repeat(1_000_000); // 1MB
    draft_service::write_draft(ctx.data_dir(), "large", &large_content)
        .expect("expected operation to succeed");
    let read_back =
        draft_service::read_draft(ctx.data_dir(), "large").expect("expected operation to succeed");
    assert_eq!(read_back.as_deref(), Some(large_content.as_str()));
}

#[test]
fn write_draft_with_unicode_content() {
    let ctx = TestContext::new();
    let content = "fn main() { println!(\"日本語\"); } // 🦀\n";
    draft_service::write_draft(ctx.data_dir(), "unicode", content)
        .expect("expected operation to succeed");
    let read_back = draft_service::read_draft(ctx.data_dir(), "unicode")
        .expect("expected operation to succeed");
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
            .expect("expected operation to succeed")
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
    draft_service::delete_draft_file(ctx.data_dir(), "does_not_exist")
        .expect("expected operation to succeed");
}

// --- Revalidation and partial outcomes ---

#[test]
fn cleanup_preserves_body_when_manifest_entry_appears_after_inspection() {
    let ctx = TestContext::new();
    draft_service::write_draft(ctx.data_dir(), "orphan", "new recovery body")
        .expect("write orphan candidate");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &DraftManifest::default())
        .expect("inspection should succeed");
    let new_entry = DraftEntry {
        draft_id: "orphan".into(),
        original_path: Some(PathBuf::from("/new.rs")),
        original_mtime_secs: None,
        saved_at_secs: 2000,
    };
    draft_service::save_manifest(
        ctx.data_dir(),
        &DraftManifest {
            drafts: vec![new_entry.clone()],
        },
    )
    .expect("commit concurrent manifest entry");

    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert!(outcome.deleted_files.is_empty());
    assert_eq!(
        draft_service::read_draft(ctx.data_dir(), "orphan").expect("read retained body"),
        Some("new recovery body".to_string())
    );
    assert_eq!(
        draft_service::load_manifest(ctx.data_dir())
            .expect("load latest manifest")
            .drafts,
        vec![new_entry]
    );
}

#[test]
fn cleanup_preserves_new_orphan_body_generation_written_after_inspection() {
    let ctx = TestContext::new();
    draft_service::write_draft(ctx.data_dir(), "orphan", "old body").expect("write inspected body");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &DraftManifest::default())
        .expect("inspect old body generation");
    draft_service::write_draft(ctx.data_dir(), "orphan", "new body")
        .expect("replace body atomically");

    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert!(outcome.deleted_files.is_empty());
    assert!(outcome.retained.iter().any(|retained| {
        retained.draft_id.as_deref() == Some("orphan")
            && retained.reason
                == draft_service::DraftOrphanCleanupRetentionReason::BodyGenerationChanged
    }));
    assert_eq!(
        draft_service::read_draft(ctx.data_dir(), "orphan").expect("read newer body"),
        Some("new body".to_string())
    );
}

#[test]
fn cleanup_preserves_manifest_when_body_reappears_after_inspection() {
    let ctx = TestContext::new();
    let entry = DraftEntry {
        draft_id: "reappeared".into(),
        original_path: None,
        original_mtime_secs: None,
        saved_at_secs: 1000,
    };
    let manifest = DraftManifest {
        drafts: vec![entry.clone()],
    };
    draft_service::save_manifest(ctx.data_dir(), &manifest).expect("seed manifest");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &manifest)
        .expect("inspection should find missing body");
    draft_service::write_draft(ctx.data_dir(), "reappeared", "new body")
        .expect("write concurrent body");

    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert!(outcome.committed_manifest_removals.is_empty());
    assert_eq!(
        draft_service::load_manifest(ctx.data_dir())
            .expect("load retained manifest")
            .drafts,
        vec![entry]
    );
}

#[test]
fn cleanup_preserves_newer_same_id_generation() {
    let ctx = TestContext::new();
    let old = DraftEntry {
        draft_id: "same".into(),
        original_path: Some(PathBuf::from("/old.rs")),
        original_mtime_secs: Some(1),
        saved_at_secs: 1,
    };
    let newer = DraftEntry {
        draft_id: "same".into(),
        original_path: Some(PathBuf::from("/new.rs")),
        original_mtime_secs: Some(2),
        saved_at_secs: 2,
    };
    let old_manifest = DraftManifest { drafts: vec![old] };
    draft_service::save_manifest(ctx.data_dir(), &old_manifest).expect("seed old manifest");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &old_manifest)
        .expect("inspect old generation");
    draft_service::save_manifest(
        ctx.data_dir(),
        &DraftManifest {
            drafts: vec![newer.clone()],
        },
    )
    .expect("commit newer generation");

    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert!(outcome.committed_manifest_removals.is_empty());
    assert_eq!(
        draft_service::load_manifest(ctx.data_dir())
            .expect("load newer manifest")
            .drafts,
        vec![newer]
    );
}

#[test]
fn cleanup_reports_already_absent_body_without_counting_deletion() {
    let ctx = TestContext::new();
    draft_service::write_draft(ctx.data_dir(), "vanished", "body").expect("write body");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &DraftManifest::default())
        .expect("inspect body");
    fixture::remove_file(&draft_service::drafts_dir(ctx.data_dir()).join("vanished.draft"));

    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert_eq!(outcome.already_absent_files.len(), 1);
    assert_eq!(outcome.confirmed_cleaned_count(), 0);
}

#[test]
fn cleanup_reports_partial_success_without_counting_retained_body() {
    let ctx = TestContext::new();
    draft_service::write_draft(ctx.data_dir(), "deleted", "body").expect("write first body");
    draft_service::write_draft(ctx.data_dir(), "changed", "body").expect("write second body");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &DraftManifest::default())
        .expect("inspect both bodies");
    let changed_path = draft_service::drafts_dir(ctx.data_dir()).join("changed.draft");
    fixture::remove_file(&changed_path);
    fixture::create_dir_all(&changed_path);

    let outcome = draft_service::execute_orphan_cleanup(ctx.data_dir(), plan);

    assert_eq!(outcome.deleted_files.len(), 1);
    assert_eq!(outcome.confirmed_cleaned_count(), 1);
    assert!(outcome.retained.iter().any(|retained| {
        retained.draft_id.as_deref() == Some("changed")
            && retained.reason
                == draft_service::DraftOrphanCleanupRetentionReason::BodyNotRegularFile
    }));
}

#[cfg(unix)]
#[test]
fn cleanup_reports_delete_failure_and_preserves_body() {
    let ctx = TestContext::new();
    draft_service::write_draft(ctx.data_dir(), "blocked", "body").expect("write body");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &DraftManifest::default())
        .expect("inspect body");
    let outcome = draft_service::execute_orphan_cleanup_with_fault_for_test(
        ctx.data_dir(),
        plan,
        draft_service::DraftOrphanCleanupFault::Delete,
    );
    assert!(outcome.deleted_files.is_empty());
    assert!(
        outcome
            .failures
            .iter()
            .any(|failure| matches!(failure, draft_service::DraftOrphanCleanupFailure::Delete(_)))
    );
    assert!(
        draft_service::read_draft(ctx.data_dir(), "blocked")
            .expect("read retained body")
            .is_some()
    );
}

#[cfg(unix)]
#[test]
fn cleanup_reports_manifest_write_failure_without_committing_removal() {
    let ctx = TestContext::new();
    let entry = DraftEntry {
        draft_id: "ghost".into(),
        original_path: None,
        original_mtime_secs: None,
        saved_at_secs: 1,
    };
    let manifest = DraftManifest {
        drafts: vec![entry.clone()],
    };
    draft_service::save_manifest(ctx.data_dir(), &manifest).expect("seed manifest");
    let plan = draft_service::inspect_orphan_cleanup(ctx.data_dir(), &manifest)
        .expect("inspect missing body");
    let outcome = draft_service::execute_orphan_cleanup_with_fault_for_test(
        ctx.data_dir(),
        plan,
        draft_service::DraftOrphanCleanupFault::Manifest,
    );
    assert!(outcome.committed_manifest_removals.is_empty());
    assert!(outcome.failures.iter().any(|failure| matches!(
        failure,
        draft_service::DraftOrphanCleanupFailure::Manifest(
            draft_service::DraftOrphanCleanupManifestError::Write { .. }
        )
    )));
    assert_eq!(
        draft_service::load_manifest(ctx.data_dir())
            .expect("load unmodified manifest")
            .drafts,
        vec![entry]
    );
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

    draft_service::write_draft(ctx.data_dir(), &id_a, "content_a")
        .expect("expected operation to succeed");
    draft_service::write_draft(ctx.data_dir(), &id_b, "content_b")
        .expect("expected operation to succeed");

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
            draft_id: id,
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

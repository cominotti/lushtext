// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated duplicate-state coverage for bookmark and note sidecar reconciliation.
//!
//! Each case creates a tiny source/target rename state and then runs the real
//! migration services. Successful reconciliations must leave a durable target;
//! ambiguous note conflicts must leave both copies preserved for retry.

use std::path::{Path, PathBuf};
use std::time::Duration;

use lushtext_core::model::bookmark::{BookmarkDocument, BookmarkId, BookmarkRecord};
use lushtext_core::model::document_note::DocumentNoteDocument;
use lushtext_core::model::folder_note::FolderNoteDocument;
use lushtext_core::model::local_history::{
    LocalHistoryDocument, LocalHistorySnapshotMeta, LocalHistorySnapshotOrigin,
};
use lushtext_core::model::note::RichNoteBody;
use lushtext_core::model::sidecar_identity::{
    DocumentSidecarIdentity, next_record_id, now_epoch_millis, stable_bytes_hash,
};
use lushtext_core::services::{
    bookmark_service, document_note_service,
    filesystem::{fixture, metadata as fs_metadata},
    folder_note_service,
    json_format::{JsonEnvelopeRef, KIND_LOCAL_HISTORY_INDEX},
    local_history_service,
};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use tempfile::TempDir;

use crate::support;

#[derive(Debug, Clone, Copy)]
enum SidecarReconcileCase {
    BookmarkSourceOnly,
    BookmarkDuplicate,
    DocumentSourceOnly,
    DocumentAmbiguous,
    FolderSourceOnly,
    FolderAmbiguous,
    LocalHistoryDuplicate,
    LocalHistoryOrphan,
}

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn duplicate_sidecar_reconciliation_preserves_last_copy(
        case in sidecar_reconcile_case(),
        source_text in support::text_fragment(),
        target_text in support::text_fragment(),
    ) {
        let dir = TempDir::new()
            .map_err(|error| TestCaseError::fail(format!("tempdir creation failed: {error}")))?;

        run_case(dir.path(), case, &source_text, &target_text)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}

fn sidecar_reconcile_case() -> impl Strategy<Value = SidecarReconcileCase> {
    prop_oneof![
        Just(SidecarReconcileCase::BookmarkSourceOnly),
        Just(SidecarReconcileCase::BookmarkDuplicate),
        Just(SidecarReconcileCase::DocumentSourceOnly),
        Just(SidecarReconcileCase::DocumentAmbiguous),
        Just(SidecarReconcileCase::FolderSourceOnly),
        Just(SidecarReconcileCase::FolderAmbiguous),
        Just(SidecarReconcileCase::LocalHistoryDuplicate),
        Just(SidecarReconcileCase::LocalHistoryOrphan),
    ]
}

fn run_case(
    data_dir: &Path,
    case: SidecarReconcileCase,
    source_text: &str,
    target_text: &str,
) -> anyhow::Result<()> {
    match case {
        SidecarReconcileCase::BookmarkSourceOnly => bookmark_source_only(data_dir),
        SidecarReconcileCase::BookmarkDuplicate => bookmark_duplicate(data_dir),
        SidecarReconcileCase::DocumentSourceOnly => document_source_only(data_dir, source_text),
        SidecarReconcileCase::DocumentAmbiguous => {
            document_ambiguous(data_dir, source_text, target_text)
        }
        SidecarReconcileCase::FolderSourceOnly => folder_source_only(data_dir, source_text),
        SidecarReconcileCase::FolderAmbiguous => {
            folder_ambiguous(data_dir, source_text, target_text)
        }
        SidecarReconcileCase::LocalHistoryDuplicate => {
            local_history_duplicate(data_dir, source_text, target_text)
        }
        SidecarReconcileCase::LocalHistoryOrphan => local_history_orphan(data_dir, source_text),
    }
}

fn bookmark_source_only(data_dir: &Path) -> anyhow::Result<()> {
    let old_file = seed_file(data_dir, "workspace/old-bookmark.rs");
    let new_file = seed_file(data_dir, "workspace/new-bookmark.rs");
    let old_identity = bookmark_service::save_for_path(
        data_dir,
        &old_file,
        &[BookmarkRecord::new(1, Some("source".to_string()))],
    )?;
    let old_sidecar = bookmark_sidecar_path(data_dir, &old_identity.sidecar_id);

    bookmark_service::move_path_tree(data_dir, &old_file, &new_file)?;

    assert!(!fs_metadata::exists(&old_sidecar));
    assert!(
        !bookmark_service::load_for_path(data_dir, &new_file)?
            .bookmarks
            .is_empty()
    );
    Ok(())
}

fn bookmark_duplicate(data_dir: &Path) -> anyhow::Result<()> {
    let old_file = seed_file(data_dir, "workspace/old-bookmark-duplicate.rs");
    let new_file = seed_file(data_dir, "workspace/new-bookmark-duplicate.rs");
    let old_identity = bookmark_service::resolve_document_identity(&old_file)?;
    let new_identity = bookmark_service::resolve_document_identity(&new_file)?;
    let old_sidecar = bookmark_sidecar_path(data_dir, &old_identity.sidecar_id);
    let mut shared_source = BookmarkRecord::new(4, Some("source".to_string()));
    shared_source.id = BookmarkId("generated-shared-bookmark".to_string());
    shared_source.updated_at_secs = 20;
    let mut shared_target = BookmarkRecord::new(2, Some("target".to_string()));
    shared_target.id = BookmarkId("generated-shared-bookmark".to_string());
    shared_target.updated_at_secs = 10;
    bookmark_service::save_document(
        data_dir,
        BookmarkDocument {
            identity: old_identity,
            bookmarks: vec![shared_source],
        },
    )?;
    bookmark_service::save_document(
        data_dir,
        BookmarkDocument {
            identity: new_identity,
            bookmarks: vec![shared_target],
        },
    )?;

    bookmark_service::move_path_tree(data_dir, &old_file, &new_file)?;

    assert!(!fs_metadata::exists(&old_sidecar));
    let loaded = bookmark_service::load_for_path(data_dir, &new_file)?;
    assert_eq!(loaded.bookmarks.len(), 1);
    assert_eq!(loaded.bookmarks[0].line, 4);
    Ok(())
}

fn document_source_only(data_dir: &Path, text: &str) -> anyhow::Result<()> {
    let old_file = seed_file(data_dir, "workspace/old-document-note.rs");
    let new_file = seed_file(data_dir, "workspace/new-document-note.rs");
    let old_identity =
        document_note_service::save_for_path(data_dir, &old_file, &RichNoteBody::new(text))?;
    let old_sidecar = document_sidecar_path(data_dir, &old_identity.sidecar_id);

    document_note_service::move_path_tree(data_dir, &old_file, &new_file)?;

    assert!(!fs_metadata::exists(&old_sidecar));
    assert!(document_note_service::load_for_path(data_dir, &new_file)?.is_some());
    Ok(())
}

fn document_ambiguous(data_dir: &Path, source_text: &str, target_text: &str) -> anyhow::Result<()> {
    let old_file = seed_file(data_dir, "workspace/old-document-conflict.rs");
    let new_file = seed_file(data_dir, "workspace/new-document-conflict.rs");
    let old_identity = bookmark_service::resolve_document_identity(&old_file)?;
    let new_identity = bookmark_service::resolve_document_identity(&new_file)?;
    let old_sidecar = document_sidecar_path(data_dir, &old_identity.sidecar_id);
    let new_sidecar = document_sidecar_path(data_dir, &new_identity.sidecar_id);
    let target_text = distinct_target_text(source_text, target_text);
    document_note_service::save_document(
        data_dir,
        &DocumentNoteDocument {
            identity: old_identity,
            note: note_with_timestamp(source_text, 10),
        },
    )?;
    document_note_service::save_document(
        data_dir,
        &DocumentNoteDocument {
            identity: new_identity,
            note: note_with_timestamp(&target_text, 10),
        },
    )?;

    let result = document_note_service::move_path_tree(data_dir, &old_file, &new_file);

    assert!(result.is_err());
    assert!(fs_metadata::exists(&old_sidecar));
    assert!(fs_metadata::exists(&new_sidecar));
    Ok(())
}

fn folder_source_only(data_dir: &Path, text: &str) -> anyhow::Result<()> {
    let old_folder = seed_dir(data_dir, "old-folder-note");
    let new_folder = seed_dir(data_dir, "new-folder-note");
    let old_identity =
        folder_note_service::save_for_folder(data_dir, &old_folder, &RichNoteBody::new(text))?;
    let old_sidecar = folder_sidecar_path(data_dir, &old_identity.sidecar_id);

    folder_note_service::move_folder_tree(data_dir, &old_folder, &new_folder)?;

    assert!(!fs_metadata::exists(&old_sidecar));
    assert!(folder_note_service::load_for_folder(data_dir, &new_folder)?.is_some());
    Ok(())
}

fn folder_ambiguous(data_dir: &Path, source_text: &str, target_text: &str) -> anyhow::Result<()> {
    let old_folder = seed_dir(data_dir, "old-folder-conflict");
    let new_folder = seed_dir(data_dir, "new-folder-conflict");
    let old_identity = folder_note_service::resolve_folder_note_identity(&old_folder)?;
    let new_identity = folder_note_service::resolve_folder_note_identity(&new_folder)?;
    let old_sidecar = folder_sidecar_path(data_dir, &old_identity.sidecar_id);
    let new_sidecar = folder_sidecar_path(data_dir, &new_identity.sidecar_id);
    let target_text = distinct_target_text(source_text, target_text);
    folder_note_service::save_document(
        data_dir,
        &FolderNoteDocument {
            identity: old_identity,
            note: note_with_timestamp(source_text, 10),
        },
    )?;
    folder_note_service::save_document(
        data_dir,
        &FolderNoteDocument {
            identity: new_identity,
            note: note_with_timestamp(&target_text, 10),
        },
    )?;

    let result = folder_note_service::move_folder_tree(data_dir, &old_folder, &new_folder);

    assert!(result.is_err());
    assert!(fs_metadata::exists(&old_sidecar));
    assert!(fs_metadata::exists(&new_sidecar));
    Ok(())
}

fn local_history_duplicate(
    data_dir: &Path,
    source_text: &str,
    target_text: &str,
) -> anyhow::Result<()> {
    let path = seed_file(data_dir, "workspace/local-history-target.rs");
    let target_text = distinct_target_text(source_text, target_text);
    local_history_service::capture_snapshot_for_path(
        data_dir,
        &path,
        &target_text,
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
    )?;
    let identity = local_history_service::resolve_document_identity(&path)?;
    let duplicate_dir = local_history_service::local_history_dir(data_dir)
        .join(format!("duplicate-{}", identity.sidecar_id));
    seed_local_history_lineage(&duplicate_dir, identity, source_text)?;

    let report = local_history_service::reconcile_lineages_with_budget(
        data_dir,
        local_history_service::LocalHistoryReconcileBudget::new(16, Duration::from_secs(60)),
    )?;

    assert_eq!(report.reconciled_lineages, 1);
    assert!(!fs_metadata::exists(&duplicate_dir));
    let snapshots = local_history_service::list_snapshots_for_path(data_dir, &path)?;
    assert_eq!(snapshots.len(), 2);
    let bodies = snapshots
        .iter()
        .map(|meta| {
            Ok(
                local_history_service::load_snapshot_for_path(data_dir, &path, &meta.snapshot_id)?
                    .expect("snapshot should exist")
                    .text,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    assert!(bodies.iter().any(|body| body == source_text));
    assert!(bodies.iter().any(|body| body == &target_text));
    Ok(())
}

fn local_history_orphan(data_dir: &Path, text: &str) -> anyhow::Result<()> {
    let orphan_dir = local_history_service::local_history_dir(data_dir).join("orphan-generated");
    fixture::create_dir_all(&orphan_dir);
    fixture::write_text(&orphan_dir.join("history-without-index.txt"), text);

    let report = local_history_service::reconcile_lineages_with_budget(
        data_dir,
        local_history_service::LocalHistoryReconcileBudget::new(16, Duration::from_secs(60)),
    )?;

    assert_eq!(report.orphaned_lineages, 1);
    assert!(fs_metadata::exists(&orphan_dir));
    Ok(())
}

fn seed_file(data_dir: &Path, relative: &str) -> PathBuf {
    let path = data_dir.join(relative);
    if let Some(parent) = path.parent() {
        fixture::create_dir_all(parent);
    }
    fixture::write_text(&path, "contents\n");
    path
}

fn seed_dir(data_dir: &Path, relative: &str) -> PathBuf {
    let path = data_dir.join(relative);
    fixture::create_dir_all(&path);
    path
}

fn note_with_timestamp(text: &str, updated_at_secs: u64) -> RichNoteBody {
    RichNoteBody {
        text: text.to_string(),
        created_at_secs: 1,
        updated_at_secs,
    }
}

fn seed_local_history_lineage(
    lineage_dir: &Path,
    identity: DocumentSidecarIdentity,
    text: &str,
) -> anyhow::Result<()> {
    fixture::create_dir_all(lineage_dir);
    let meta = LocalHistorySnapshotMeta {
        snapshot_id: next_record_id("history"),
        captured_at_millis: now_epoch_millis(),
        origin: LocalHistorySnapshotOrigin::Save,
        byte_len: text.len() as u64,
        content_hash: stable_bytes_hash(text.as_bytes()),
    };
    fixture::write_text(&lineage_dir.join(format!("{}.txt", meta.snapshot_id)), text);
    let document = LocalHistoryDocument {
        identity,
        snapshots: vec![meta],
    };
    let envelope = JsonEnvelopeRef::new(KIND_LOCAL_HISTORY_INDEX, &document);
    let json = serde_json::to_string_pretty(&envelope)?;
    fixture::write_text(&lineage_dir.join("index.json"), &json);
    Ok(())
}

fn distinct_target_text(source_text: &str, target_text: &str) -> String {
    if source_text == target_text {
        format!("{target_text}-target")
    } else {
        target_text.to_string()
    }
}

fn bookmark_sidecar_path(data_dir: &Path, sidecar_id: &str) -> PathBuf {
    bookmark_service::bookmarks_dir(data_dir).join(format!("{sidecar_id}.json"))
}

fn document_sidecar_path(data_dir: &Path, sidecar_id: &str) -> PathBuf {
    document_note_service::document_notes_dir(data_dir).join(format!("{sidecar_id}.json"))
}

fn folder_sidecar_path(data_dir: &Path, sidecar_id: &str) -> PathBuf {
    folder_note_service::folder_notes_dir(data_dir).join(format!("{sidecar_id}.json"))
}

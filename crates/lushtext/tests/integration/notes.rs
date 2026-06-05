// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for bookmark and note sidecar services.

use lushtext_core::model::bookmark::BookmarkRecord;
use lushtext_core::model::document_note::DocumentNoteDocument;
use lushtext_core::model::migration_ledger::MigrationKind;
use lushtext_core::model::note::RichNoteBody;
use lushtext_core::model::workspace::{WorkspaceConfig, WorkspaceId, WorkspaceScope};
use lushtext_core::services::{
    bookmark_service, document_note_service,
    filesystem::{fixture, metadata as fs_metadata},
    migration_ledger, workspace_note_service,
};

use crate::common::TestContext;

#[test]
fn bookmark_sidecar_roundtrip_uses_saved_file_identity() {
    let ctx = TestContext::new();
    let file_path = ctx.write_file("workspace/src/main.rs", "fn main() {}\n");

    let bookmarks = vec![
        BookmarkRecord::new(2, Some("important".to_string())),
        BookmarkRecord::new(0, None),
    ];

    let identity = bookmark_service::save_for_path(ctx.data_dir(), &file_path, &bookmarks)
        .expect("expected operation to succeed");
    let loaded = bookmark_service::load_for_path(ctx.data_dir(), &file_path)
        .expect("expected operation to succeed");

    let sidecar_path = bookmark_service::bookmarks_dir(ctx.data_dir())
        .join(format!("{}.json", identity.sidecar_id));
    assert!(
        fs_metadata::exists(&sidecar_path),
        "bookmark sidecar should be written"
    );
    assert_eq!(loaded.identity.display_path, file_path);
    assert_eq!(loaded.bookmarks.len(), 2);
    assert_eq!(loaded.bookmarks[0].line, 0);
    assert_eq!(loaded.bookmarks[1].label.as_deref(), Some("important"));
}

#[test]
fn note_sidecars_follow_in_app_rename_migration() {
    let ctx = TestContext::new();
    let old_file = ctx.write_file("workspace/src/old.rs", "fn old_name() {}\n");
    let new_file = ctx.path().join("workspace/src/new.rs");

    bookmark_service::save_for_path(
        ctx.data_dir(),
        &old_file,
        &[BookmarkRecord::new(1, Some("bookmark".to_string()))],
    )
    .expect("expected operation to succeed");
    document_note_service::save_for_path(ctx.data_dir(), &old_file, &RichNoteBody::new("doc note"))
        .expect("expected operation to succeed");

    fixture::rename(&old_file, &new_file);
    bookmark_service::move_path_tree(ctx.data_dir(), &old_file, &new_file)
        .expect("expected operation to succeed");
    document_note_service::move_path_tree(ctx.data_dir(), &old_file, &new_file)
        .expect("expected operation to succeed");

    let loaded_bookmarks = bookmark_service::load_for_path(ctx.data_dir(), &new_file)
        .expect("expected operation to succeed");
    let loaded_document_note = document_note_service::load_for_path(ctx.data_dir(), &new_file)
        .expect("expected operation to succeed")
        .expect("expected document note after rename");

    assert_eq!(loaded_bookmarks.bookmarks.len(), 1);
    assert_eq!(
        loaded_bookmarks.bookmarks[0].label.as_deref(),
        Some("bookmark")
    );
    assert_eq!(loaded_document_note.note.text, "doc note");
}

#[test]
fn document_note_roundtrip_uses_saved_file_identity() {
    let ctx = TestContext::new();
    let file_path = ctx.write_file("workspace/src/main.rs", "fn main() {}\n");

    let identity =
        document_note_service::save_for_path(ctx.data_dir(), &file_path, &RichNoteBody::new("Doc"))
            .expect("expected operation to succeed");
    let loaded = document_note_service::load_for_path(ctx.data_dir(), &file_path)
        .expect("expected operation to succeed")
        .expect("expected document note");

    let sidecar_path = document_note_service::document_notes_dir(ctx.data_dir())
        .join(format!("{}.json", identity.sidecar_id));
    assert!(
        fs_metadata::exists(&sidecar_path),
        "document note sidecar should be written"
    );
    assert_eq!(loaded.identity.display_path, file_path);
    assert_eq!(loaded.note.text, "Doc");
}

#[test]
fn workspace_note_roundtrip_uses_root_identity_and_scope_listing() {
    let ctx = TestContext::new();
    let root = ctx.mkdir("workspace");

    let identity = workspace_note_service::save_for_root(
        ctx.data_dir(),
        &root,
        &RichNoteBody::new("Root note"),
    )
    .expect("expected operation to succeed");
    let loaded = workspace_note_service::load_for_root(ctx.data_dir(), &root)
        .expect("expected operation to succeed")
        .expect("expected workspace note");

    let sidecar_path = workspace_note_service::workspace_notes_dir(ctx.data_dir())
        .join(format!("{}.json", identity.sidecar_id));
    assert!(
        fs_metadata::exists(&sidecar_path),
        "workspace note sidecar should be written"
    );
    assert_eq!(loaded.identity.display_root, root);
    assert_eq!(loaded.note.text, "Root note");

    let listed = workspace_note_service::list_workspace_notes_for_scope(
        ctx.data_dir(),
        &[WorkspaceConfig {
            id: WorkspaceId::new("new-slot"),
            name: "Workspace".to_string(),
            root,
        }],
        &WorkspaceScope::Workspace(WorkspaceId::new("new-slot")),
    )
    .expect("expected operation to succeed");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].note.text, "Root note");
}

#[test]
fn document_note_migration_failure_survives_restart_and_retry_cleans_obsolete_sidecar() {
    let ctx = TestContext::new();
    let old_file = ctx.write_file("workspace/src/old.rs", "fn old_name() {}\n");
    let new_file = ctx.write_file("workspace/src/new.rs", "fn new_name() {}\n");
    let old_identity = bookmark_service::resolve_document_identity(&old_file)
        .expect("expected operation to succeed");
    let new_identity = bookmark_service::resolve_document_identity(&new_file)
        .expect("expected operation to succeed");
    let old_sidecar = document_note_service::document_notes_dir(ctx.data_dir())
        .join(format!("{}.json", old_identity.sidecar_id));
    let new_sidecar = document_note_service::document_notes_dir(ctx.data_dir())
        .join(format!("{}.json", new_identity.sidecar_id));
    document_note_service::save_document(
        ctx.data_dir(),
        &DocumentNoteDocument {
            identity: old_identity.clone(),
            note: RichNoteBody {
                text: "source conflict".to_string(),
                created_at_secs: 1,
                updated_at_secs: 10,
            },
        },
    )
    .expect("save old note");
    document_note_service::save_document(
        ctx.data_dir(),
        &DocumentNoteDocument {
            identity: new_identity,
            note: RichNoteBody {
                text: "target conflict".to_string(),
                created_at_secs: 1,
                updated_at_secs: 10,
            },
        },
    )
    .expect("save target note");
    migration_ledger::record_pending(
        ctx.data_dir(),
        &old_file,
        &new_file,
        &[MigrationKind::DocumentNotes],
    )
    .expect("record pending migration");

    let failed_retry = migration_ledger::reconcile_pending(ctx.data_dir())
        .expect("ambiguous migration should stay diagnostic");

    assert_eq!(failed_retry.attempted, 1);
    assert_eq!(failed_retry.completed, 0);
    assert_eq!(failed_retry.diagnostics.len(), 1);
    assert!(fs_metadata::exists(&old_sidecar));
    assert!(fs_metadata::exists(&new_sidecar));
    assert_eq!(
        migration_ledger::load_recovering(ctx.data_dir())
            .value
            .entries
            .len(),
        1
    );

    document_note_service::save_document(
        ctx.data_dir(),
        &DocumentNoteDocument {
            identity: old_identity,
            note: RichNoteBody {
                text: "source wins on retry".to_string(),
                created_at_secs: 1,
                updated_at_secs: 20,
            },
        },
    )
    .expect("make retry deterministic");

    let successful_retry =
        migration_ledger::reconcile_pending(ctx.data_dir()).expect("retry migration");

    assert_eq!(successful_retry.attempted, 1);
    assert_eq!(successful_retry.completed, 1);
    assert!(successful_retry.diagnostics.is_empty());
    assert!(!fs_metadata::exists(&old_sidecar));
    assert!(fs_metadata::exists(&new_sidecar));
    let loaded = document_note_service::load_for_path(ctx.data_dir(), &new_file)
        .expect("load merged note")
        .expect("merged note exists");
    assert_eq!(loaded.note.text, "source wins on retry");
    assert!(
        migration_ledger::load_recovering(ctx.data_dir())
            .value
            .entries
            .is_empty()
    );
}

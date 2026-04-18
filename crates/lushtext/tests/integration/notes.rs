// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for bookmark and annotation sidecar services.

use lushtext_core::model::annotation::{AnnotationRecord, AnnotationStyle};
use lushtext_core::model::bookmark::BookmarkRecord;
use lushtext_core::model::note::RichNoteBody;
use lushtext_core::model::workspace::{WorkspaceConfig, WorkspaceId, WorkspaceScope};
use lushtext_core::services::{
    annotation_service, bookmark_service, document_note_service, workspace_note_service,
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
    assert!(sidecar_path.exists(), "bookmark sidecar should be written");
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
    annotation_service::save_for_path(
        ctx.data_dir(),
        &old_file,
        &[AnnotationRecord::new(
            0,
            0,
            "carry this annotation",
            AnnotationStyle::Warning,
        )],
    )
    .expect("expected operation to succeed");
    document_note_service::save_for_path(ctx.data_dir(), &old_file, &RichNoteBody::new("doc note"))
        .expect("expected operation to succeed");

    std::fs::rename(&old_file, &new_file).expect("expected operation to succeed");
    bookmark_service::move_path_tree(ctx.data_dir(), &old_file, &new_file)
        .expect("expected operation to succeed");
    annotation_service::move_path_tree(ctx.data_dir(), &old_file, &new_file)
        .expect("expected operation to succeed");
    document_note_service::move_path_tree(ctx.data_dir(), &old_file, &new_file)
        .expect("expected operation to succeed");

    let loaded_bookmarks = bookmark_service::load_for_path(ctx.data_dir(), &new_file)
        .expect("expected operation to succeed");
    let loaded_annotations = annotation_service::load_for_path(ctx.data_dir(), &new_file)
        .expect("expected operation to succeed");
    let loaded_document_note = document_note_service::load_for_path(ctx.data_dir(), &new_file)
        .expect("expected operation to succeed")
        .expect("expected document note after rename");

    assert_eq!(loaded_bookmarks.bookmarks.len(), 1);
    assert_eq!(
        loaded_bookmarks.bookmarks[0].label.as_deref(),
        Some("bookmark")
    );
    assert_eq!(loaded_annotations.annotations.len(), 1);
    assert_eq!(
        loaded_annotations.annotations[0].note_text,
        "carry this annotation"
    );
    assert_eq!(loaded_document_note.note.text, "doc note");
}

#[test]
fn annotation_export_groups_by_file_and_includes_excerpt() {
    let ctx = TestContext::new();
    let file_path = ctx.write_file(
        "workspace/src/lib.rs",
        "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\n",
    );

    annotation_service::save_for_path(
        ctx.data_dir(),
        &file_path,
        &[AnnotationRecord::new(
            1,
            4,
            "Explain this block",
            AnnotationStyle::Todo,
        )],
    )
    .expect("expected operation to succeed");

    let markdown = annotation_service::export_workspace_markdown(
        ctx.data_dir(),
        &[ctx.path().join("workspace")],
    )
    .expect("expected operation to succeed");

    assert!(markdown.contains("# Workspace Range Notes"));
    assert!(markdown.contains("## "));
    assert!(markdown.contains("Lines 2-5 · Todo"));
    assert!(markdown.contains("Explain this block"));
    assert!(markdown.contains("line 2\nline 3\nline 4\nline 5"));
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
        sidecar_path.exists(),
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
        sidecar_path.exists(),
        "workspace note sidecar should be written"
    );
    assert_eq!(loaded.identity.display_root, root);
    assert_eq!(loaded.note.text, "Root note");

    let listed = workspace_note_service::list_workspace_notes_for_scope(
        ctx.data_dir(),
        &[WorkspaceConfig {
            id: WorkspaceId::new("new-slot"),
            name: "Workspace".to_string(),
            root: root.clone(),
        }],
        &WorkspaceScope::Workspace(WorkspaceId::new("new-slot")),
    )
    .expect("expected operation to succeed");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].note.text, "Root note");
}

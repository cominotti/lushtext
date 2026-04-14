// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for bookmark and annotation sidecar services.

use lushtext_core::model::annotation::{AnnotationRecord, AnnotationStyle};
use lushtext_core::model::bookmark::BookmarkRecord;
use lushtext_core::services::{annotation_service, bookmark_service};

use crate::common::TestContext;

#[test]
fn bookmark_sidecar_roundtrip_uses_saved_file_identity() {
    let ctx = TestContext::new();
    let file_path = ctx.write_file("workspace/src/main.rs", "fn main() {}\n");

    let bookmarks = vec![
        BookmarkRecord::new(2, Some("important".to_string())),
        BookmarkRecord::new(0, None),
    ];

    let identity = bookmark_service::save_for_path(ctx.data_dir(), &file_path, &bookmarks).unwrap();
    let loaded = bookmark_service::load_for_path(ctx.data_dir(), &file_path).unwrap();

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
    .unwrap();
    annotation_service::save_for_path(
        ctx.data_dir(),
        &old_file,
        &[AnnotationRecord::new(
            0,
            0,
            "carry this annotation".to_string(),
            AnnotationStyle::Warning,
        )],
    )
    .unwrap();

    std::fs::rename(&old_file, &new_file).unwrap();
    bookmark_service::move_path_tree(ctx.data_dir(), &old_file, &new_file).unwrap();
    annotation_service::move_path_tree(ctx.data_dir(), &old_file, &new_file).unwrap();

    let loaded_bookmarks = bookmark_service::load_for_path(ctx.data_dir(), &new_file).unwrap();
    let loaded_annotations = annotation_service::load_for_path(ctx.data_dir(), &new_file).unwrap();

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
            "Explain this block".to_string(),
            AnnotationStyle::Todo,
        )],
    )
    .unwrap();

    let markdown = annotation_service::export_workspace_markdown(
        ctx.data_dir(),
        &[ctx.path().join("workspace")],
    )
    .unwrap();

    assert!(markdown.contains("# Workspace Annotations"));
    assert!(markdown.contains("## "));
    assert!(markdown.contains("Lines 2-5 · Todo"));
    assert!(markdown.contains("Explain this block"));
    assert!(markdown.contains("line 2\nline 3\nline 4\nline 5"));
}

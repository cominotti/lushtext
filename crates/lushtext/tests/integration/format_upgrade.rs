// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration coverage for the app-owned metadata format-upgrade inventory.
//!
//! The fixtures use one mixed app-data directory so path additions or renames
//! in individual persistence services are caught by the shared preflight map.

use std::path::Path;

use lushtext_core::services::filesystem::fixture;
use lushtext_core::services::format_upgrade::{
    FormatClassification, FormatMetadataKind, build_plan, scan,
};
use lushtext_core::services::json_format::{
    JsonEnvelopeRef, KIND_BOOKMARK_SIDECAR, KIND_DOCUMENT_NOTE_SIDECAR, KIND_DRAFT_MANIFEST,
    KIND_FOLDER_NOTE_SIDECAR, KIND_LOCAL_HISTORY_INDEX, KIND_MIGRATION_LEDGER,
    KIND_REPLACE_UNDO_CLEANUP_MARKER, KIND_REPLACE_UNDO_ENTRY, KIND_REPLACE_UNDO_MANIFEST,
    KIND_SAVED_SEARCHES, KIND_SEARCH_HISTORY, KIND_SESSION, KIND_WORKSPACE_STATE,
};
use serde_json::json;

use super::common::TestContext;

#[test]
fn mixed_app_data_inventory_reports_current_v1_without_actions() {
    let ctx = TestContext::new();
    seed_v1_metadata(&ctx);

    let inventory = scan(ctx.data_dir());
    let plan = build_plan(&inventory);

    for (relative, kind) in [
        ("workspaces.json", FormatMetadataKind::WorkspaceState),
        ("session.json", FormatMetadataKind::Session),
        ("drafts/manifest.json", FormatMetadataKind::DraftManifest),
        ("drafts/file-1.draft", FormatMetadataKind::DraftBody),
        ("saved-searches.json", FormatMetadataKind::SavedSearches),
        ("search-history.json", FormatMetadataKind::SearchHistory),
        ("bookmarks/a.json", FormatMetadataKind::BookmarkSidecar),
        (
            "document-notes/a.json",
            FormatMetadataKind::DocumentNoteSidecar,
        ),
        ("folder-notes/a.json", FormatMetadataKind::FolderNoteSidecar),
        (
            "local-history/lineage-1/index.json",
            FormatMetadataKind::LocalHistoryIndex,
        ),
        ("migration-ledger.json", FormatMetadataKind::MigrationLedger),
        (
            "replace-backup-journal/manifest.json",
            FormatMetadataKind::ReplaceUndoManifest,
        ),
        (
            "replace-backup-journal/entry.json",
            FormatMetadataKind::ReplaceUndoEntry,
        ),
        (
            "replace-backup-journal/cleanup-in-progress.json",
            FormatMetadataKind::ReplaceUndoCleanupMarker,
        ),
    ] {
        let item = inventory
            .items
            .iter()
            .find(|item| item.path.relative() == Path::new(relative) && item.kind == kind)
            .unwrap_or_else(|| panic!("missing inventory item {relative}"));
        assert!(
            matches!(item.classification, FormatClassification::Current { .. }),
            "expected {relative} to be current, got {:?}",
            item.classification
        );
    }

    assert!(plan.has_no_action());
}

fn seed_v1_metadata(ctx: &TestContext) {
    write_envelope(ctx, "workspaces.json", KIND_WORKSPACE_STATE, &json!({}));
    write_envelope(ctx, "session.json", KIND_SESSION, &json!({}));
    write_envelope(ctx, "drafts/manifest.json", KIND_DRAFT_MANIFEST, &json!({}));
    fixture::write_text(&ctx.data_dir().join("drafts/file-1.draft"), "unsaved text");
    write_envelope(ctx, "saved-searches.json", KIND_SAVED_SEARCHES, &json!([]));
    write_envelope(ctx, "search-history.json", KIND_SEARCH_HISTORY, &json!([]));
    write_envelope(ctx, "bookmarks/a.json", KIND_BOOKMARK_SIDECAR, &json!({}));
    write_envelope(
        ctx,
        "document-notes/a.json",
        KIND_DOCUMENT_NOTE_SIDECAR,
        &json!({}),
    );
    write_envelope(
        ctx,
        "folder-notes/a.json",
        KIND_FOLDER_NOTE_SIDECAR,
        &json!({}),
    );
    write_envelope(
        ctx,
        "local-history/lineage-1/index.json",
        KIND_LOCAL_HISTORY_INDEX,
        &json!({}),
    );
    write_envelope(
        ctx,
        "migration-ledger.json",
        KIND_MIGRATION_LEDGER,
        &json!({}),
    );
    write_envelope(
        ctx,
        "replace-backup-journal/manifest.json",
        KIND_REPLACE_UNDO_MANIFEST,
        &json!({}),
    );
    write_envelope(
        ctx,
        "replace-backup-journal/entry.json",
        KIND_REPLACE_UNDO_ENTRY,
        &json!({}),
    );
    write_envelope(
        ctx,
        "replace-backup-journal/cleanup-in-progress.json",
        KIND_REPLACE_UNDO_CLEANUP_MARKER,
        &json!({}),
    );
}

fn write_envelope(ctx: &TestContext, relative: &str, kind: &'static str, data: &serde_json::Value) {
    let path = ctx.data_dir().join(relative);
    if let Some(parent) = path.parent() {
        fixture::create_dir_all(parent);
    }
    let envelope = JsonEnvelopeRef::new(kind, &data);
    fixture::write_text(
        &path,
        &serde_json::to_string_pretty(&envelope).expect("serialize envelope"),
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Golden-fixture coverage for LushText's public v1 JSON persistence envelope.

use std::path::Path;

use lushtext_core::model::bookmark::{BookmarkDocument, BookmarkId, BookmarkRecord};
use lushtext_core::model::content_search::{
    ContentSearchOptions, SavedSearch, SearchHistoryEntry, SearchQuerySpec,
};
use lushtext_core::model::document_note::DocumentNoteDocument;
use lushtext_core::model::draft::{DraftEntry, DraftManifest};
use lushtext_core::model::folder_note::{FolderNoteDocument, FolderNoteIdentity};
use lushtext_core::model::local_history::{LocalHistoryDocument, LocalHistorySnapshotOrigin};
use lushtext_core::model::migration_ledger::{MigrationKind, MigrationLedgerDocument};
use lushtext_core::model::note::RichNoteBody;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::model::sidecar_identity::{DocumentSidecarIdentity, stable_path_hash};
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceFolder, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::content_search::{ReplaceUndoBackup, ReplaceUndoEntry};
use lushtext_core::services::filesystem::fixture;
use lushtext_core::services::json_format::{
    self, JsonFormatError, KIND_BOOKMARK_SIDECAR, KIND_DOCUMENT_NOTE_SIDECAR, KIND_DRAFT_MANIFEST,
    KIND_FOLDER_NOTE_SIDECAR, KIND_LEGACY_WORKSPACE_NOTE_SIDECAR, KIND_LOCAL_HISTORY_INDEX,
    KIND_MIGRATION_LEDGER, KIND_REPLACE_UNDO_CLEANUP_MARKER, KIND_REPLACE_UNDO_ENTRY,
    KIND_REPLACE_UNDO_MANIFEST, KIND_SAVED_SEARCHES, KIND_SEARCH_HISTORY, KIND_SESSION,
    KIND_WORKSPACE_STATE,
};
use lushtext_core::services::recovery_metadata::{
    RecoveryLoadConfig, RecoveryLoadOutcome, RecoveryMetadataClass, RecoveryProblem,
    load_enveloped_json_or_default,
};
use lushtext_core::services::{
    bookmark_service, document_note_service, draft_service, folder_note_service,
    local_history_service, migration_ledger, saved_searches, search_backup, search_history,
    session_service, workspace_manager,
};
use serde::de::DeserializeOwned;
use tempfile::TempDir;

macro_rules! fixture_bytes {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/persistent_json/",
            $name
        ))
    };
}

macro_rules! fixture_str {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/persistent_json/",
            $name
        ))
    };
}

fn parse_fixture<T>(bytes: &[u8], expected_kind: &'static str) -> T
where
    T: DeserializeOwned,
{
    json_format::parse_v1_payload(bytes, expected_kind).expect("fixture should parse")
}

#[test]
fn golden_v1_fixtures_parse_supported_payloads() {
    let workspace: WorkspacesFile = parse_fixture(
        fixture_bytes!("workspace-state-v1.json"),
        KIND_WORKSPACE_STATE,
    );
    assert_eq!(workspace.workspaces[0].id.as_str(), "workspace-a");
    assert_eq!(
        workspace.workspaces[0].folder_paths(),
        vec![
            Path::new("/tmp/project").to_path_buf(),
            Path::new("/tmp/project/docs").to_path_buf(),
        ]
    );
    assert_eq!(
        workspace.workspaces[0].folders[0].id.as_str(),
        "folder-project"
    );
    assert_eq!(
        workspace.workspaces[0].folders[1].id.as_str(),
        "folder-docs"
    );
    assert_eq!(
        workspace.current_scope.workspace_id(),
        Some(&workspace.workspaces[0].id)
    );

    let saved_searches: Vec<SavedSearch> = parse_fixture(
        fixture_bytes!("saved-searches-v1.json"),
        KIND_SAVED_SEARCHES,
    );
    assert_eq!(saved_searches[0].name, "Rust functions");
    assert_eq!(saved_searches[0].spec.query, "fn ");

    let history: Vec<SearchHistoryEntry> = parse_fixture(
        fixture_bytes!("search-history-v1.json"),
        KIND_SEARCH_HISTORY,
    );
    assert_eq!(history[0].spec.query, "TODO");
    assert!(history[0].spec.options.whole_word);

    let session: SessionData = parse_fixture(fixture_bytes!("session-v1.json"), KIND_SESSION);
    assert_eq!(session.tabs.len(), 2);
    assert!(session.tabs[0].pinned);
    assert_eq!(
        session.tabs[1].draft_id.as_deref(),
        Some("untitled-0000000000000001")
    );

    let manifest: DraftManifest = parse_fixture(
        fixture_bytes!("draft-manifest-v1.json"),
        KIND_DRAFT_MANIFEST,
    );
    assert_eq!(manifest.drafts[0].draft_id, "deb09d05810ef629");

    let bookmark: BookmarkDocument = parse_fixture(
        fixture_bytes!("bookmark-sidecar-v1.json"),
        KIND_BOOKMARK_SIDECAR,
    );
    assert_eq!(
        bookmark.identity.sidecar_id,
        stable_path_hash(Path::new("/tmp/project/src/main.rs"))
    );
    assert_eq!(bookmark.bookmarks[0].line, 2);

    let document_note: DocumentNoteDocument = parse_fixture(
        fixture_bytes!("document-note-sidecar-v1.json"),
        KIND_DOCUMENT_NOTE_SIDECAR,
    );
    assert_eq!(document_note.note.text, "Remember this file");

    let folder_note: FolderNoteDocument = parse_fixture(
        fixture_bytes!("folder-note-sidecar-v1.json"),
        KIND_FOLDER_NOTE_SIDECAR,
    );
    assert_eq!(folder_note.note.text, "Folder note");

    let legacy_folder_note: FolderNoteDocument = parse_fixture(
        fixture_bytes!("legacy-folder-note-sidecar-v1.json"),
        KIND_LEGACY_WORKSPACE_NOTE_SIDECAR,
    );
    assert_eq!(legacy_folder_note.note.text, "Legacy folder note");

    let history_index: LocalHistoryDocument = parse_fixture(
        fixture_bytes!("local-history-index-v1.json"),
        KIND_LOCAL_HISTORY_INDEX,
    );
    assert_eq!(
        history_index.snapshots[0].origin,
        LocalHistorySnapshotOrigin::Save
    );

    let ledger: MigrationLedgerDocument = parse_fixture(
        fixture_bytes!("migration-ledger-v1.json"),
        KIND_MIGRATION_LEDGER,
    );
    assert_eq!(ledger.next_generation, 2);
    assert_eq!(ledger.entries[0].kinds[0].kind, MigrationKind::Bookmarks);

    assert_json_value_field(
        fixture_bytes!("replace-undo-manifest-v1.json"),
        KIND_REPLACE_UNDO_MANIFEST,
        &["entries", "0", "entry_file"],
        "deb09d05810ef629.json",
    );
    assert_json_value_field(
        fixture_bytes!("replace-undo-entry-v1.json"),
        KIND_REPLACE_UNDO_ENTRY,
        &["path"],
        "/tmp/project/src/main.rs",
    );
    assert_json_value_field(
        fixture_bytes!("replace-undo-cleanup-marker-v1.json"),
        KIND_REPLACE_UNDO_CLEANUP_MARKER,
        &["reason"],
        "fixture cleanup",
    );
}

#[test]
fn fixture_parser_rejects_unsupported_shapes_and_accepts_extensible_session_data() {
    let old_workspace = json_format::parse_v1_payload::<WorkspacesFile>(
        fixture_bytes!("workspace-old-shape.json"),
        KIND_WORKSPACE_STATE,
    )
    .expect_err("old workspace shape should be unsupported");
    assert!(matches!(
        old_workspace,
        JsonFormatError::UnsupportedFormat { .. }
    ));

    let wrong_kind = json_format::parse_v1_payload::<SessionData>(
        fixture_bytes!("session-wrong-kind.json"),
        KIND_SESSION,
    )
    .expect_err("wrong kind should be unsupported");
    assert!(matches!(
        wrong_kind,
        JsonFormatError::UnsupportedFormat { .. }
    ));

    let future_version = json_format::parse_v1_payload::<SessionData>(
        fixture_bytes!("session-unsupported-version.json"),
        KIND_SESSION,
    )
    .expect_err("future version should be unsupported");
    assert!(matches!(
        future_version,
        JsonFormatError::UnsupportedVersion { version: 2, .. }
    ));

    let malformed = json_format::parse_v1_payload::<SessionData>(
        fixture_bytes!("malformed.json"),
        KIND_SESSION,
    )
    .expect_err("malformed JSON should not parse");
    assert!(matches!(malformed, JsonFormatError::Malformed { .. }));

    let missing_optional: SessionData = parse_fixture(
        fixture_bytes!("session-missing-optional-v1.json"),
        KIND_SESSION,
    );
    assert_eq!(missing_optional.tabs[0].draft_id, None);
    assert!(!missing_optional.tabs[0].pinned);

    let unknown_fields: SessionData = parse_fixture(
        fixture_bytes!("session-unknown-fields-v1.json"),
        KIND_SESSION,
    );
    assert!(unknown_fields.tabs.is_empty());
}

#[test]
fn recovery_loader_preserves_old_shape_fixture_before_defaulting() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("workspaces.json");
    fixture::write_text(&path, fixture_str!("workspace-old-shape.json"));

    let loaded = load_enveloped_json_or_default::<WorkspacesFile>(
        &RecoveryLoadConfig::new(dir.path(), &path, RecoveryMetadataClass::WorkspaceState),
        KIND_WORKSPACE_STATE,
    );

    assert!(loaded.value.workspaces.is_empty());
    assert_eq!(loaded.outcome, RecoveryLoadOutcome::QuarantinedDefault);
    assert!(matches!(
        loaded.diagnostics[0].problem,
        RecoveryProblem::UnsupportedFormat { .. }
    ));
    let quarantine_path = loaded.diagnostics[0]
        .preservation
        .quarantine_path()
        .expect("old workspace should be quarantined");
    assert_eq!(
        fixture::read_text(quarantine_path),
        fixture_str!("workspace-old-shape.json")
    );
}

#[test]
fn recovery_loader_reports_oversized_v1_metadata_without_panicking() {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("session.json");
    fixture::write_text(&path, fixture_str!("session-v1.json"));

    let config = RecoveryLoadConfig::new(dir.path(), &path, RecoveryMetadataClass::Session)
        .with_max_bytes(4);
    let loaded = load_enveloped_json_or_default::<SessionData>(&config, KIND_SESSION);

    assert!(loaded.value.tabs.is_empty());
    assert_eq!(loaded.outcome, RecoveryLoadOutcome::QuarantinedDefault);
    assert!(matches!(
        loaded.diagnostics[0].problem,
        RecoveryProblem::Oversized { max_bytes: 4, .. }
    ));
}

#[test]
fn generated_damaged_envelope_inputs_return_diagnostics_without_panics() {
    let cases: &[(&str, &[u8])] = &[
        ("empty.json", b""),
        ("null.json", b"null"),
        ("array.json", b"[]"),
        ("missing-kind.json", br#"{"version":1,"data":{}}"#),
        (
            "missing-version.json",
            br#"{"kind":"dev.cominotti.lushtext.session","data":{}}"#,
        ),
        (
            "missing-data.json",
            br#"{"kind":"dev.cominotti.lushtext.session","version":1}"#,
        ),
        (
            "bad-payload.json",
            br#"{"kind":"dev.cominotti.lushtext.session","version":1,"data":{"tabs":"bad"}}"#,
        ),
    ];

    for (name, bytes) in cases {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join(name);
        fixture::write_bytes(&path, bytes);

        let loaded = load_enveloped_json_or_default::<SessionData>(
            &RecoveryLoadConfig::new(dir.path(), &path, RecoveryMetadataClass::Session),
            KIND_SESSION,
        );

        assert!(loaded.value.tabs.is_empty());
        assert!(
            !loaded.diagnostics.is_empty(),
            "{name} should emit a diagnostic"
        );
    }
}

#[test]
fn public_service_saves_write_v1_envelopes() {
    let dir = TempDir::new().expect("temp dir");
    let data_dir = dir.path();
    let source_path = data_dir.join("project").join("src").join("main.rs");
    fixture::create_dir_all(
        source_path
            .parent()
            .expect("source path should have parent"),
    );
    fixture::write_text(&source_path, "fn main() {}\n");
    let canonical_source =
        lushtext_core::services::filesystem::metadata::canonical_path(&source_path)
            .expect("canonical source path");
    let document_identity =
        DocumentSidecarIdentity::from_paths(source_path.clone(), canonical_source.clone());

    workspace_manager::save(data_dir, &sample_workspaces_file(&source_path))
        .expect("save workspace state");
    assert_file_kind(&data_dir.join("workspaces.json"), KIND_WORKSPACE_STATE);

    saved_searches::save(data_dir, &[sample_saved_search()]).expect("save saved searches");
    assert_file_kind(&data_dir.join("saved-searches.json"), KIND_SAVED_SEARCHES);

    search_history::save(data_dir, &[sample_history_entry()]).expect("save search history");
    assert_file_kind(&data_dir.join("search-history.json"), KIND_SEARCH_HISTORY);

    session_service::save(data_dir, &sample_session(&source_path)).expect("save session");
    assert_file_kind(&data_dir.join("session.json"), KIND_SESSION);

    draft_service::save_manifest(data_dir, &sample_draft_manifest(&source_path))
        .expect("save draft manifest");
    assert_file_kind(
        &data_dir.join("drafts").join("manifest.json"),
        KIND_DRAFT_MANIFEST,
    );

    bookmark_service::save_document(
        data_dir,
        sample_bookmark_document(document_identity.clone()),
    )
    .expect("save bookmark sidecar");
    assert_file_kind(
        &data_dir
            .join("bookmarks")
            .join(format!("{}.json", document_identity.sidecar_id)),
        KIND_BOOKMARK_SIDECAR,
    );

    document_note_service::save_document(
        data_dir,
        &DocumentNoteDocument {
            identity: document_identity.clone(),
            note: RichNoteBody::new("Document note"),
        },
    )
    .expect("save document note");
    assert_file_kind(
        &data_dir
            .join("document-notes")
            .join(format!("{}.json", document_identity.sidecar_id)),
        KIND_DOCUMENT_NOTE_SIDECAR,
    );

    let folder_identity = FolderNoteIdentity::from_folders(data_dir.into(), data_dir.into());
    folder_note_service::save_document(
        data_dir,
        &FolderNoteDocument {
            identity: folder_identity.clone(),
            note: RichNoteBody::new("Folder note"),
        },
    )
    .expect("save folder note");
    assert_file_kind(
        &data_dir
            .join("folder-notes")
            .join(format!("{}.json", folder_identity.sidecar_id)),
        KIND_FOLDER_NOTE_SIDECAR,
    );

    local_history_service::capture_snapshot_for_path(
        data_dir,
        &source_path,
        "fn main() {}\n",
        LocalHistorySnapshotOrigin::Save,
        local_history_service::LocalHistoryCapturePolicy::PreserveDuplicate,
    )
    .expect("capture local history");
    assert_file_kind(
        &data_dir
            .join("local-history")
            .join(stable_path_hash(&canonical_source))
            .join("index.json"),
        KIND_LOCAL_HISTORY_INDEX,
    );

    migration_ledger::record_pending(
        data_dir,
        Path::new("/tmp/project/old.rs"),
        Path::new("/tmp/project/new.rs"),
        &[MigrationKind::Bookmarks],
    )
    .expect("record migration ledger");
    assert_file_kind(
        &data_dir.join("migration-ledger.json"),
        KIND_MIGRATION_LEDGER,
    );

    let mut backup = ReplaceUndoBackup::new();
    backup.insert(
        source_path.clone(),
        ReplaceUndoEntry::new(b"before".to_vec(), b"after".to_vec()),
    );
    search_backup::save(data_dir, &backup).expect("save replace undo journal");
    let journal_dir = data_dir.join("replace-backup-journal");
    assert_file_kind(
        &journal_dir.join("manifest.json"),
        KIND_REPLACE_UNDO_MANIFEST,
    );
    assert_file_kind(
        &journal_dir.join(format!("{}.json", stable_path_hash(&source_path))),
        KIND_REPLACE_UNDO_ENTRY,
    );
}

fn assert_file_kind(path: &Path, expected_kind: &'static str) {
    let bytes = fixture::read_bytes(path);
    let value: serde_json::Value =
        json_format::parse_v1_payload(&bytes, expected_kind).expect("saved envelope should parse");
    assert!(!value.is_null());
}

fn assert_json_value_field(
    bytes: &[u8],
    expected_kind: &'static str,
    path: &[&str],
    expected: &str,
) {
    let value: serde_json::Value = parse_fixture(bytes, expected_kind);
    let mut current = &value;
    for segment in path {
        current = if let Ok(index) = segment.parse::<usize>() {
            &current[index]
        } else {
            &current[*segment]
        };
    }
    assert_eq!(current.as_str(), Some(expected));
}

fn sample_workspaces_file(folder: &Path) -> WorkspacesFile {
    let id = WorkspaceId::new("workspace-a");
    WorkspacesFile {
        current_scope: WorkspaceScope::workspace(id.clone()),
        workspaces: vec![WorkspaceConfig {
            id,
            name: "Project".to_string(),
            folders: vec![WorkspaceFolder::new(folder.to_path_buf())],
        }],
    }
}

fn sample_saved_search() -> SavedSearch {
    SavedSearch::from_spec(
        "Rust functions",
        SearchQuerySpec::new("fn ", ContentSearchOptions::default()),
    )
}

fn sample_history_entry() -> SearchHistoryEntry {
    SearchHistoryEntry::from_spec(SearchQuerySpec::new(
        "TODO",
        ContentSearchOptions::default(),
    ))
}

fn sample_session(path: &Path) -> SessionData {
    SessionData {
        tabs: vec![SessionTab {
            path: Some(path.to_path_buf()),
            draft_id: None,
            cursor_line: 12,
            cursor_col: 2,
            scroll_line: 9,
            pinned: true,
        }],
        active_tab_index: Some(0),
    }
}

fn sample_draft_manifest(path: &Path) -> DraftManifest {
    DraftManifest {
        drafts: vec![DraftEntry {
            draft_id: stable_path_hash(path),
            original_path: Some(path.to_path_buf()),
            original_mtime_secs: Some(1),
            saved_at_secs: 2,
        }],
    }
}

fn sample_bookmark_document(identity: DocumentSidecarIdentity) -> BookmarkDocument {
    BookmarkDocument {
        identity,
        bookmarks: vec![BookmarkRecord {
            id: BookmarkId("bookmark-fixture".to_string()),
            line: 2,
            label: Some("Entry".to_string()),
            created_at_secs: 1,
            updated_at_secs: 2,
        }],
    }
}

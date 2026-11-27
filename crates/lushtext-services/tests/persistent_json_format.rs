// SPDX-License-Identifier: GPL-3.0-or-later

//! Golden-fixture coverage for LushText's public v1 JSON persistence envelope.

use std::path::Path;

use lushtext_model::persistence::bookmark::{BookmarkDocument, BookmarkId, BookmarkRecord};
use lushtext_model::persistence::document_note::DocumentNoteDocument;
use lushtext_model::persistence::draft::{DraftEntry, DraftManifest};
use lushtext_model::persistence::folder_note::{FolderNoteDocument, FolderNoteIdentity};
use lushtext_model::persistence::local_history::{
    LocalHistoryDocument, LocalHistorySnapshotOrigin,
};
use lushtext_model::persistence::migration_ledger::{MigrationKind, MigrationLedgerDocument};
use lushtext_model::persistence::note::RichNoteBody;
use lushtext_model::persistence::session::{SessionData, SessionTab};
use lushtext_model::persistence::sidecar_identity::{DocumentSidecarIdentity, stable_path_hash};
use lushtext_model::workspace::content_search::{
    ContentSearchOptions, SavedSearch, SearchHistoryEntry, SearchQuerySpec,
};
use lushtext_model::workspace::{
    WorkspaceConfig, WorkspaceFolder, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_services::filesystem::fixture;
use lushtext_services::persistence::json_format::{
    self, JsonFormatError, KIND_BOOKMARK_SIDECAR, KIND_DOCUMENT_NOTE_SIDECAR, KIND_DRAFT_MANIFEST,
    KIND_FOLDER_NOTE_SIDECAR, KIND_LEGACY_WORKSPACE_NOTE_SIDECAR, KIND_LOCAL_HISTORY_INDEX,
    KIND_MIGRATION_LEDGER, KIND_REPLACE_UNDO_CLEANUP_MARKER, KIND_REPLACE_UNDO_ENTRY,
    KIND_REPLACE_UNDO_MANIFEST, KIND_REPLACE_UNDO_STREAM_ENTRY, KIND_SAVED_SEARCHES,
    KIND_SEARCH_HISTORY, KIND_SESSION, KIND_WORKSPACE_STATE,
};
use lushtext_services::persistence::recovery_metadata::{
    RecoveryLoadConfig, RecoveryLoadOutcome, RecoveryMetadataClass, RecoveryProblem,
    load_enveloped_json_or_default,
};
use lushtext_services::{
    notes::bookmark_service, notes::document_note_service, notes::folder_note_service,
    persistence::draft_service, persistence::local_history_service, persistence::migration_ledger,
    persistence::session_service, search::saved_searches, search::search_backup,
    search::search_history, workspace::workspace_manager,
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
    assert_eq!(
        ledger.next_generation,
        lushtext_model::values::MigrationGeneration::new(2)
    );
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
    let canonical_source = lushtext_services::filesystem::metadata::canonical_path(&source_path)
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

    search_backup::fixture::write_stream_journal(data_dir, &[(&source_path, b"before", b"after")])
        .expect("save replace undo journal");
    let journal_dir = data_dir.join("replace-backup-journal");
    assert_file_kind(
        &journal_dir.join("manifest.json"),
        KIND_REPLACE_UNDO_MANIFEST,
    );
    assert_file_kind(
        &journal_dir.join(format!("{}.json", stable_path_hash(&source_path))),
        KIND_REPLACE_UNDO_STREAM_ENTRY,
    );
    // The body is raw bytes, not JSON.
    assert_eq!(
        fixture::read_bytes(
            &journal_dir
                .join("bodies")
                .join(format!("{}.body", stable_path_hash(&source_path)))
        ),
        b"before"
    );
}

/// Write a journal directory from the stream-entry fixtures: a manifest, one
/// entry file and its body (design D3).
fn stream_journal_from_fixtures(data_dir: &Path, entry: &[u8], body: &[u8]) {
    let journal = data_dir.join("replace-backup-journal");
    fixture::create_dir_all(&journal.join("bodies"));
    fixture::write_bytes(
        &journal.join("manifest.json"),
        fixture_bytes!("replace-undo-manifest-launch-v1.json"),
    );
    fixture::write_bytes(&journal.join("deb09d05810ef629.json"), entry);
    fixture::write_bytes(&journal.join("bodies").join("deb09d05810ef629.body"), body);
}

#[test]
fn stream_entry_fixtures_load_as_descriptors_and_reject_a_wrong_body() {
    let entry = fixture_bytes!("replace-undo-stream-entry-v1.json");
    assert_json_value_field(
        entry,
        KIND_REPLACE_UNDO_STREAM_ENTRY,
        &["body_file"],
        "deb09d05810ef629.body",
    );

    // Valid, and with an unknown field: active, one descriptor.
    for entry in [
        entry.as_slice(),
        fixture_bytes!("replace-undo-stream-entry-unknown-field-v1.json").as_slice(),
    ] {
        let dir = TempDir::new().expect("temp dir");
        stream_journal_from_fixtures(dir.path(), entry, b"before");
        let load = search_backup::load_recovering(dir.path());
        assert!(load.active, "{:?}", load.diagnostics);
        assert_eq!(load.backup.len(), 1);
        let descriptor = &load.backup[Path::new("/tmp/project/src/main.rs")];
        assert_eq!(descriptor.original.len, 6);
        assert_eq!(descriptor.replaced.len, 5);
        assert_eq!(descriptor.launch.pid, 4242);
        assert_eq!(descriptor.launch.nonce, 7);
    }

    // A body whose size differs from the recorded original: malformed.
    let dir = TempDir::new().expect("temp dir");
    stream_journal_from_fixtures(dir.path(), entry, b"before!");
    let load = search_backup::load_recovering(dir.path());
    assert!(!load.active);
    assert!(load.backup.is_empty());

    // A wrong kind: malformed.
    let wrong_kind = String::from_utf8(entry.to_vec())
        .expect("utf-8 fixture")
        .replace(KIND_REPLACE_UNDO_STREAM_ENTRY, KIND_REPLACE_UNDO_MANIFEST);
    let dir = TempDir::new().expect("temp dir");
    stream_journal_from_fixtures(dir.path(), wrong_kind.as_bytes(), b"before");
    assert!(!search_backup::load_recovering(dir.path()).active);

    // A missing optional manifest field (a journal from before streamed
    // entries records no launch): still active.
    let dir = TempDir::new().expect("temp dir");
    stream_journal_from_fixtures(dir.path(), entry, b"before");
    fixture::write_text(
        &dir.path()
            .join("replace-backup-journal")
            .join("manifest.json"),
        r#"{"kind":"dev.cominotti.lushtext.replace-all-undo-manifest","version":1,"data":{"incremental":true,"entries":[]}}"#,
    );
    assert!(search_backup::load_recovering(dir.path()).active);

    // A legacy entry beside a streamed one: mixed kinds, malformed.
    let dir = TempDir::new().expect("temp dir");
    stream_journal_from_fixtures(dir.path(), entry, b"before");
    fixture::write_bytes(
        &dir.path()
            .join("replace-backup-journal")
            .join("0000000000000000.json"),
        fixture_bytes!("replace-undo-entry-v1.json"),
    );
    assert!(!search_backup::load_recovering(dir.path()).active);
}

#[test]
fn draft_manifest_v1_without_cleanup_continuation_remains_readable() {
    let bytes = br#"{
        "kind":"dev.cominotti.lushtext.draft-manifest",
        "version":1,
        "data":{"drafts":[]}
    }"#;

    let manifest = json_format::parse_v1_payload::<DraftManifest>(bytes, KIND_DRAFT_MANIFEST)
        .expect("legacy v1 draft manifest payload should parse");

    assert!(manifest.drafts.is_empty());
    assert!(manifest.cleanup_continuation.is_none());
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
        cleanup_continuation: None,
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

/// `main`'s journal shapes, vendored from `services/search_backup.rs` at the
/// commit this change branched from (stream-replace-all task 7.6). `main`
/// reads an entry with `parse_v1_payload::<ReplaceBackupDiskEntry>(..,
/// KIND_REPLACE_UNDO_ENTRY)` and a manifest into `ReplaceJournalManifest`;
/// serde ignores unknown fields on both.
mod main_journal_shape {
    use std::path::PathBuf;

    #[derive(Debug, serde::Deserialize)]
    #[expect(dead_code, reason = "vendored shape: only its parse result matters")]
    pub struct ReplaceBackupDiskEntry {
        pub path: PathBuf,
        pub original_content: String,
        pub replaced_content: String,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct ReplaceJournalManifest {
        #[serde(default)]
        pub incremental: bool,
        pub entries: Vec<ReplaceJournalManifestEntry>,
    }

    #[derive(Debug, serde::Deserialize)]
    #[expect(dead_code, reason = "vendored shape: only its parse result matters")]
    pub struct ReplaceJournalManifestEntry {
        pub path: PathBuf,
        pub entry_file: String,
    }
}

/// Downgrade (design D6): a journal this branch writes, read by `main`'s
/// loader. `main` parses the manifest, rejects every streamed entry as the
/// wrong kind (so the journal is inactive: no Undo, no target written), and
/// never reads `bodies/`, whose files are not `.json` and sit in a
/// subdirectory its non-recursive scan does not enter.
#[test]
fn main_loader_exposes_no_undo_for_a_streamed_journal() {
    let dir = TempDir::new().expect("tempdir");
    let data_dir = dir.path();
    let sources = [
        Path::new("/tmp/project/a.rs"),
        Path::new("/tmp/project/b.rs"),
    ];
    search_backup::fixture::write_stream_journal(
        data_dir,
        &[
            (sources[0], b"before a", b"after a"),
            (sources[1], b"before b", b"after b"),
        ],
    )
    .expect("write streamed journal");
    let journal_dir = data_dir.join("replace-backup-journal");

    let manifest: main_journal_shape::ReplaceJournalManifest = json_format::parse_v1_payload(
        &fixture::read_bytes(&journal_dir.join("manifest.json")),
        KIND_REPLACE_UNDO_MANIFEST,
    )
    .expect("main parses the streamed journal's manifest");
    // An incremental manifest lists nothing: main scans the entry files.
    assert!(manifest.incremental || manifest.entries.len() == sources.len());

    for source in sources {
        let entry_file = journal_dir.join(format!("{}.json", stable_path_hash(source)));
        let parsed = json_format::parse_v1_payload::<main_journal_shape::ReplaceBackupDiskEntry>(
            &fixture::read_bytes(&entry_file),
            KIND_REPLACE_UNDO_ENTRY,
        );
        assert!(
            matches!(parsed, Err(JsonFormatError::UnsupportedFormat { .. })),
            "main rejects a streamed entry as another kind: {parsed:?}",
        );
        let body = journal_dir
            .join("bodies")
            .join(format!("{}.body", stable_path_hash(source)));
        assert!(fixture::read_bytes(&body).starts_with(b"before"));
        assert_ne!(
            body.extension().and_then(std::ffi::OsStr::to_str),
            Some("json")
        );
    }
}

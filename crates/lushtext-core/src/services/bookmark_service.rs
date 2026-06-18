// SPDX-License-Identifier: GPL-3.0-or-later

//! Bookmark sidecar persistence and workspace listing helpers.
//!
//! This service owns the filesystem-facing bookmark workflow: resolve a stable
//! canonical-path identity, load/save sidecar JSON, migrate records after
//! in-app renames, and collect bookmark rows for browse surfaces.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::model::bookmark::{BookmarkDocument, BookmarkId, BookmarkRecord};
use crate::model::sidecar_identity::DocumentSidecarIdentity;
use crate::services::filesystem::{
    DirectoryScanPolicy, metadata as fs_metadata, mutate as fs_mutate, tree as fs_tree,
};
use crate::services::note_storage;
use crate::services::recovery_metadata::{RecoveryDiagnostic, RecoveryMetadataClass};

/// Directory name that stores per-file bookmark sidecars.
const BOOKMARKS_DIR: &str = "bookmarks";

/// Lightweight workspace-facing bookmark row for browse dialogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBookmark {
    /// Path of the bookmarked file.
    pub path: PathBuf,
    /// Bookmark identity for label editing and row activation.
    pub bookmark_id: BookmarkId,
    /// Zero-based line number used for editor navigation.
    pub line: u32,
    /// Optional label shown in the bookmark list.
    pub label: Option<String>,
}

/// Workspace bookmark rows plus diagnostics for sidecars skipped during listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBookmarkListing {
    /// Bookmark rows safe to display in browse surfaces.
    pub bookmarks: Vec<WorkspaceBookmark>,
    /// Recovery diagnostics for malformed or unreadable bookmark sidecars.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

impl WorkspaceBookmark {
    /// Human-friendly title used in bookmark rows.
    #[must_use]
    pub fn display_label(&self) -> String {
        self.label
            .clone()
            .unwrap_or_else(|| format!("Line {}", self.line.saturating_add(1)))
    }
}

/// Resolve the bookmark sidecar directory under the app data home.
#[must_use]
pub fn bookmarks_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BOOKMARKS_DIR)
}

/// Resolve the stable identity for a saved document path.
///
/// # Errors
///
/// Returns an error if the path cannot be canonicalized.
pub fn resolve_document_identity(path: &Path) -> Result<DocumentSidecarIdentity> {
    note_storage::resolve_document_identity(path)
}

/// Load bookmarks for a saved file, returning an empty document if no sidecar exists yet.
///
/// # Errors
///
/// Returns an error if the document identity cannot be resolved or the sidecar
/// directory cannot be scanned. Malformed or unreadable sidecars are preserved
/// through recovery diagnostics and treated as absent.
pub fn load_for_path(data_dir: &Path, path: &Path) -> Result<BookmarkDocument> {
    let identity = resolve_document_identity(path)?;
    load_for_identity(data_dir, identity)
}

fn load_for_identity(
    data_dir: &Path,
    identity: DocumentSidecarIdentity,
) -> Result<BookmarkDocument> {
    let filename = note_storage::sidecar_filename(&identity.sidecar_id);
    let path = bookmarks_dir(data_dir).join(&filename);
    let load = note_storage::load_json_file_recovering::<BookmarkDocument>(
        data_dir,
        &path,
        RecoveryMetadataClass::BookmarkSidecar,
    );
    note_storage::trace_recovery_diagnostics(&load.diagnostics);
    match load.value {
        Some(mut document) => {
            document.sort_stable();
            Ok(document)
        }
        None => Ok(BookmarkDocument::empty(identity)),
    }
}

/// Save bookmarks for a document path. Empty bookmark sets delete the sidecar file.
///
/// # Errors
///
/// Returns an error if the document identity cannot be resolved or the sidecar
/// cannot be written or deleted.
pub fn save_for_path(
    data_dir: &Path,
    path: &Path,
    bookmarks: &[BookmarkRecord],
) -> Result<DocumentSidecarIdentity> {
    let identity = resolve_document_identity(path)?;
    save_document(
        data_dir,
        BookmarkDocument {
            identity: identity.clone(),
            bookmarks: bookmarks.to_vec(),
        },
    )?;
    Ok(identity)
}

/// Save a fully shaped bookmark document.
///
/// # Errors
///
/// Returns an error if the sidecar cannot be written or deleted.
pub fn save_document(data_dir: &Path, mut document: BookmarkDocument) -> Result<()> {
    document.sort_stable();

    if document.bookmarks.is_empty() {
        return delete_sidecar_file(data_dir, &document.identity);
    }

    let path = bookmarks_dir(data_dir).join(note_storage::sidecar_filename(
        &document.identity.sidecar_id,
    ));
    note_storage::save_json_file_recovering(
        data_dir,
        &path,
        RecoveryMetadataClass::BookmarkSidecar,
        &document,
    )
}

/// Delete the bookmark sidecar for a saved file path if it exists.
///
/// # Errors
///
/// Returns an error if the document identity cannot be resolved or an existing
/// sidecar cannot be deleted.
pub fn delete_for_path(data_dir: &Path, path: &Path) -> Result<()> {
    let identity = resolve_document_identity(path)?;
    delete_sidecar_file(data_dir, &identity)
}

fn delete_sidecar_file(data_dir: &Path, identity: &DocumentSidecarIdentity) -> Result<()> {
    let path = bookmarks_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id));
    match fs_mutate::remove_file_if_exists(&path) {
        Ok(_) => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to delete bookmark sidecar {}: {}",
            path.display(),
            error
        )),
    }
}

/// Move bookmark sidecars after an in-app rename of a file or directory tree.
///
/// Returns the number of bookmark documents that were rewritten.
///
/// # Errors
///
/// Returns an error if the sidecar directory cannot be scanned or a migrated
/// document cannot be read, rewritten, or cleaned up.
pub fn move_path_tree(data_dir: &Path, old_path: &Path, new_path: &Path) -> Result<usize> {
    let dir = bookmarks_dir(data_dir);
    if !fs_metadata::path_status(&dir)?.is_present() {
        return Ok(0);
    }

    let mut migrated = 0;
    for entry in fs_tree::scan_directory(&dir, DirectoryScanPolicy::visible_workspace())
        .with_context(|| format!("failed to read {}", dir.display()))?
    {
        let sidecar_path = entry.path;
        if sidecar_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let load = note_storage::load_json_file_recovering::<BookmarkDocument>(
            data_dir,
            &sidecar_path,
            RecoveryMetadataClass::BookmarkSidecar,
        );
        note_storage::trace_recovery_diagnostics(&load.diagnostics);
        let Some(document) = load.value else {
            continue;
        };
        let Some((display_path, canonical_path)) =
            note_storage::rebase_identity_paths(&document.identity, old_path, new_path)
        else {
            continue;
        };

        let new_identity = DocumentSidecarIdentity::from_paths(display_path, canonical_path);
        let new_sidecar_path = dir.join(note_storage::sidecar_filename(&new_identity.sidecar_id));
        let document = merge_bookmark_target(data_dir, &new_sidecar_path, document, new_identity)?;
        save_document(data_dir, document)?;
        if sidecar_path != new_sidecar_path {
            remove_obsolete_sidecar(&sidecar_path)?;
        }
        migrated += 1;
    }

    Ok(migrated)
}

fn merge_bookmark_target(
    data_dir: &Path,
    target_path: &Path,
    mut source: BookmarkDocument,
    target_identity: DocumentSidecarIdentity,
) -> Result<BookmarkDocument> {
    let load = note_storage::load_json_file_recovering::<BookmarkDocument>(
        data_dir,
        target_path,
        RecoveryMetadataClass::BookmarkSidecar,
    );
    note_storage::trace_recovery_diagnostics(&load.diagnostics);
    let Some(target) = load.value else {
        source.identity = target_identity;
        return Ok(source);
    };

    Ok(merge_bookmark_documents(source, target, target_identity))
}

/// Merge a moved sidecar into an existing target sidecar without guessing on conflicts.
fn merge_bookmark_documents(
    source: BookmarkDocument,
    mut target: BookmarkDocument,
    target_identity: DocumentSidecarIdentity,
) -> BookmarkDocument {
    for source_bookmark in source.bookmarks {
        match target
            .bookmarks
            .iter_mut()
            .find(|bookmark| bookmark.id == source_bookmark.id)
        {
            Some(existing) if source_bookmark.updated_at_secs > existing.updated_at_secs => {
                *existing = source_bookmark;
            }
            Some(_) => {}
            None => target.bookmarks.push(source_bookmark),
        }
    }
    target.identity = target_identity;
    target.sort_stable();
    target
}

fn remove_obsolete_sidecar(path: &Path) -> Result<()> {
    match fs_mutate::remove_file_if_exists(path) {
        Ok(_) => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to delete obsolete bookmark sidecar {}: {}",
            path.display(),
            error
        )),
    }
}

/// Collect all bookmarks under the current workspace folders for browse dialogs.
///
/// # Errors
///
/// Returns an error if the sidecar directory cannot be scanned or a bookmark
/// document cannot be read or parsed.
pub fn list_workspace_bookmarks(
    data_dir: &Path,
    workspace_folders: &[PathBuf],
) -> Result<Vec<WorkspaceBookmark>> {
    Ok(list_workspace_bookmarks_recovering(data_dir, workspace_folders)?.bookmarks)
}

/// Collect workspace bookmarks and preserve partial-recovery diagnostics.
///
/// # Errors
///
/// Returns an error only when the sidecar directory itself cannot be scanned.
pub fn list_workspace_bookmarks_recovering(
    data_dir: &Path,
    workspace_folders: &[PathBuf],
) -> Result<WorkspaceBookmarkListing> {
    let canonical_folders = note_storage::canonicalize_folders(workspace_folders);
    let dir = bookmarks_dir(data_dir);
    if !fs_metadata::path_status(&dir)?.is_present() {
        return Ok(WorkspaceBookmarkListing {
            bookmarks: Vec::new(),
            diagnostics: Vec::new(),
        });
    }

    let mut bookmarks = Vec::new();
    let mut diagnostics = Vec::new();
    for entry in fs_tree::scan_directory(&dir, DirectoryScanPolicy::visible_workspace())
        .with_context(|| format!("failed to read {}", dir.display()))?
    {
        let load = note_storage::load_json_file_recovering::<BookmarkDocument>(
            data_dir,
            &entry.path,
            RecoveryMetadataClass::BookmarkSidecar,
        );
        note_storage::trace_recovery_diagnostics(&load.diagnostics);
        diagnostics.extend(load.diagnostics);
        let Some(document) = load.value else {
            continue;
        };
        if !note_storage::matches_any_folder(&document.identity, &canonical_folders) {
            continue;
        }
        let display_path = document.identity.display_path.clone();

        bookmarks.extend(
            document
                .bookmarks
                .into_iter()
                .map(|bookmark| WorkspaceBookmark {
                    path: display_path.clone(),
                    bookmark_id: bookmark.id,
                    line: bookmark.line,
                    label: bookmark.label,
                }),
        );
    }

    bookmarks.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.bookmark_id.0.cmp(&right.bookmark_id.0))
    });
    Ok(WorkspaceBookmarkListing {
        bookmarks,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::bookmark::BookmarkRecord;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fixture::create_dir_all(parent);
        }
        fixture::write_text(path, contents);
    }

    fn sidecar_path_for_identity(data_dir: &Path, identity: &DocumentSidecarIdentity) -> PathBuf {
        bookmarks_dir(data_dir).join(note_storage::sidecar_filename(&identity.sidecar_id))
    }

    #[test]
    fn workspace_bookmark_display_label_prefers_label_and_falls_back_to_line() {
        let labeled = WorkspaceBookmark {
            path: PathBuf::from("/workspace/file.rs"),
            bookmark_id: BookmarkId("bookmark-labeled".to_string()),
            line: 41,
            label: Some("Review this".to_string()),
        };
        let unlabeled = WorkspaceBookmark {
            path: PathBuf::from("/workspace/file.rs"),
            bookmark_id: BookmarkId("bookmark-unlabeled".to_string()),
            line: 0,
            label: None,
        };

        assert_eq!(labeled.display_label(), "Review this");
        assert_eq!(unlabeled.display_label(), "Line 1");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");

        let bookmarks = vec![
            BookmarkRecord::new(4, Some("Important".to_string())),
            BookmarkRecord::new(1, None),
        ];

        save_for_path(dir.path(), &file_path, &bookmarks).expect("expected operation to succeed");
        let loaded = load_for_path(dir.path(), &file_path).expect("expected operation to succeed");

        assert_eq!(loaded.bookmarks.len(), 2);
        assert_eq!(loaded.bookmarks[0].line, 1);
        assert_eq!(loaded.bookmarks[1].label.as_deref(), Some("Important"));
    }

    #[test]
    fn empty_save_deletes_sidecar() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");

        let identity = save_for_path(dir.path(), &file_path, &[BookmarkRecord::new(0, None)])
            .expect("expected operation to succeed");
        save_for_path(dir.path(), &file_path, &[]).expect("expected operation to succeed");

        let sidecar_path = sidecar_path_for_identity(dir.path(), &identity);
        assert!(!fs_metadata::exists(&sidecar_path));
    }

    #[test]
    fn delete_for_path_removes_existing_sidecar_and_ignores_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");
        let identity = save_for_path(dir.path(), &file_path, &[BookmarkRecord::new(0, None)])
            .expect("expected operation to succeed");
        let sidecar_path = sidecar_path_for_identity(dir.path(), &identity);

        delete_for_path(dir.path(), &file_path).expect("delete sidecar");

        assert!(!fs_metadata::exists(&sidecar_path));
        delete_for_path(dir.path(), &file_path).expect("missing sidecar should be ignored");
    }

    #[test]
    fn delete_for_path_reports_non_file_sidecar_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");
        let identity = resolve_document_identity(&file_path).expect("resolve identity");
        let sidecar_path = sidecar_path_for_identity(dir.path(), &identity);
        fixture::create_dir_all(&sidecar_path);

        let error = delete_for_path(dir.path(), &file_path).expect_err("delete directory sidecar");

        assert!(
            error
                .to_string()
                .contains("failed to delete bookmark sidecar"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn move_path_tree_rewrites_document_identity() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_file = dir.path().join("workspace/old.rs");
        let new_file = dir.path().join("workspace/new.rs");
        write_file(&old_file, "fn old() {}\n");

        let old_identity = save_for_path(
            dir.path(),
            &old_file,
            &[BookmarkRecord::new(2, Some("keep".to_string()))],
        )
        .expect("expected operation to succeed");
        let old_sidecar_path = sidecar_path_for_identity(dir.path(), &old_identity);

        fixture::rename(&old_file, &new_file);
        let migrated = move_path_tree(dir.path(), &old_file, &new_file)
            .expect("expected operation to succeed");

        assert_eq!(migrated, 1);
        assert!(
            !fs_metadata::exists(&old_sidecar_path),
            "old sidecar should be removed"
        );
        let loaded = load_for_path(dir.path(), &new_file).expect("expected operation to succeed");
        assert_eq!(loaded.identity.display_path, new_file);
        assert_eq!(loaded.bookmarks.len(), 1);
        assert_eq!(loaded.bookmarks[0].label.as_deref(), Some("keep"));
    }

    #[test]
    fn move_path_tree_merges_duplicate_bookmark_sidecars_before_cleanup() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let old_file = dir.path().join("workspace/old.rs");
        let new_file = dir.path().join("workspace/new.rs");
        write_file(&old_file, "fn old() {}\n");
        write_file(&new_file, "fn new() {}\n");
        let old_identity = resolve_document_identity(&old_file).expect("old identity");
        let new_identity = resolve_document_identity(&new_file).expect("new identity");
        let mut older_target = BookmarkRecord::new(2, Some("target-old-label".to_string()));
        older_target.id = BookmarkId("bookmark-shared".to_string());
        older_target.updated_at_secs = 10;
        let mut newer_source = BookmarkRecord::new(8, Some("source-new-label".to_string()));
        newer_source.id = BookmarkId("bookmark-shared".to_string());
        newer_source.updated_at_secs = 20;
        let extra_target = BookmarkRecord::new(1, Some("target-extra".to_string()));

        save_document(
            dir.path(),
            BookmarkDocument {
                identity: old_identity.clone(),
                bookmarks: vec![newer_source],
            },
        )
        .expect("save old bookmark sidecar");
        save_document(
            dir.path(),
            BookmarkDocument {
                identity: new_identity,
                bookmarks: vec![older_target, extra_target],
            },
        )
        .expect("save target bookmark sidecar");
        let old_sidecar_path = sidecar_path_for_identity(dir.path(), &old_identity);

        let migrated = move_path_tree(dir.path(), &old_file, &new_file)
            .expect("duplicate bookmark sidecars should merge");

        assert_eq!(migrated, 1);
        assert!(!fs_metadata::exists(&old_sidecar_path));
        let loaded = load_for_path(dir.path(), &new_file).expect("load merged bookmarks");
        assert_eq!(loaded.bookmarks.len(), 2);
        let shared = loaded
            .bookmarks
            .iter()
            .find(|bookmark| bookmark.id.0 == "bookmark-shared")
            .expect("shared bookmark should survive");
        assert_eq!(shared.line, 8);
        assert_eq!(shared.label.as_deref(), Some("source-new-label"));
    }

    #[test]
    fn merge_bookmark_documents_keeps_newer_target_bookmark_on_id_conflict() {
        let source_identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/old.rs"),
            PathBuf::from("/workspace/old.rs"),
        );
        let target_identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/new.rs"),
            PathBuf::from("/workspace/new.rs"),
        );
        let mut older_source = BookmarkRecord::new(8, Some("source-old-label".to_string()));
        older_source.id = BookmarkId("bookmark-shared".to_string());
        older_source.updated_at_secs = 10;
        let mut newer_target = BookmarkRecord::new(2, Some("target-new-label".to_string()));
        newer_target.id = BookmarkId("bookmark-shared".to_string());
        newer_target.updated_at_secs = 20;

        let merged = merge_bookmark_documents(
            BookmarkDocument {
                identity: source_identity,
                bookmarks: vec![older_source],
            },
            BookmarkDocument {
                identity: target_identity.clone(),
                bookmarks: vec![newer_target],
            },
            target_identity.clone(),
        );

        assert_eq!(merged.identity, target_identity);
        assert_eq!(merged.bookmarks.len(), 1);
        assert_eq!(merged.bookmarks[0].line, 2);
        assert_eq!(
            merged.bookmarks[0].label.as_deref(),
            Some("target-new-label")
        );
        assert_eq!(merged.bookmarks[0].updated_at_secs, 20);
    }

    #[test]
    fn merge_bookmark_documents_keeps_target_bookmark_on_equal_timestamp_conflict() {
        let source_identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/old.rs"),
            PathBuf::from("/workspace/old.rs"),
        );
        let target_identity = DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/new.rs"),
            PathBuf::from("/workspace/new.rs"),
        );
        let mut source = BookmarkRecord::new(8, Some("source-label".to_string()));
        source.id = BookmarkId("bookmark-shared".to_string());
        source.updated_at_secs = 20;
        let mut target = BookmarkRecord::new(2, Some("target-label".to_string()));
        target.id = BookmarkId("bookmark-shared".to_string());
        target.updated_at_secs = 20;

        let merged = merge_bookmark_documents(
            BookmarkDocument {
                identity: source_identity,
                bookmarks: vec![source],
            },
            BookmarkDocument {
                identity: target_identity.clone(),
                bookmarks: vec![target],
            },
            target_identity,
        );

        assert_eq!(merged.bookmarks.len(), 1);
        assert_eq!(merged.bookmarks[0].line, 2);
        assert_eq!(merged.bookmarks[0].label.as_deref(), Some("target-label"));
    }

    #[test]
    fn list_workspace_bookmarks_filters_folders_and_sorts_rows() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let inside_a = dir.path().join("workspace/a.rs");
        let inside_b = dir.path().join("workspace/b.rs");
        let outside = dir.path().join("other/outside.rs");
        write_file(&inside_a, "fn a() {}\n");
        write_file(&inside_b, "fn b() {}\n");
        write_file(&outside, "fn outside() {}\n");

        save_for_path(
            dir.path(),
            &inside_b,
            &[BookmarkRecord::new(4, Some("B".to_string()))],
        )
        .expect("save b");
        save_for_path(
            dir.path(),
            &inside_a,
            &[
                BookmarkRecord::new(3, Some("A3".to_string())),
                BookmarkRecord::new(1, None),
            ],
        )
        .expect("save a");
        save_for_path(
            dir.path(),
            &outside,
            &[BookmarkRecord::new(0, Some("Outside".to_string()))],
        )
        .expect("save outside");

        let bookmarks = list_workspace_bookmarks(dir.path(), &[dir.path().join("workspace")])
            .expect("list bookmarks");

        assert_eq!(bookmarks.len(), 3);
        assert_eq!(bookmarks[0].path, inside_a);
        assert_eq!(bookmarks[0].line, 1);
        assert_eq!(bookmarks[1].path, dir.path().join("workspace/a.rs"));
        assert_eq!(bookmarks[1].line, 3);
        assert_eq!(bookmarks[2].path, inside_b);
        assert!(
            bookmarks.iter().all(|bookmark| bookmark.path != outside),
            "outside workspace bookmarks should be filtered out"
        );
    }

    #[test]
    fn corrupt_bookmark_sidecar_is_quarantined_without_hiding_valid_bookmarks() {
        let dir = TempDir::new().expect("tempdir");
        let valid_path = dir.path().join("workspace/valid.rs");
        let corrupt_path = dir.path().join("workspace/corrupt.rs");
        write_file(&valid_path, "valid\n");
        write_file(&corrupt_path, "corrupt\n");
        save_for_path(
            dir.path(),
            &valid_path,
            &[BookmarkRecord::new(4, Some("valid".to_string()))],
        )
        .expect("save valid bookmark");
        let corrupt_identity = resolve_document_identity(&corrupt_path).expect("corrupt identity");
        let corrupt_sidecar = sidecar_path_for_identity(dir.path(), &corrupt_identity);
        fixture::write_text(&corrupt_sidecar, "not bookmark json");

        let listing =
            list_workspace_bookmarks_recovering(dir.path(), &[dir.path().join("workspace")])
                .expect("list bookmarks despite corrupt sidecar");

        assert_eq!(listing.bookmarks.len(), 1);
        assert_eq!(listing.bookmarks[0].path, valid_path);
        assert_eq!(listing.diagnostics.len(), 1);
        assert_eq!(
            listing.diagnostics[0].class,
            RecoveryMetadataClass::BookmarkSidecar
        );
        assert!(
            !fs_metadata::exists(&corrupt_sidecar),
            "corrupt sidecar should be moved out of normal browse path"
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft persistence service — save and restore unsaved buffer content.
//!
//! All functions perform blocking I/O and must be called from a background
//! thread via `spawn_blocking_then`. Drafts are stored as plain UTF-8 text
//! files in `$XDG_DATA_HOME/lushtext/drafts/`, with a JSON manifest mapping
//! draft IDs to original file paths and metadata.

use crate::model::draft::{
    DraftEntry, DraftManifest, FileDraftRestoreResolution, PreloadedDraftRestore,
};
use crate::model::session::SessionData;
use crate::services::{durable_write, editor_io, json_store, session_service};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const DRAFTS_DIR: &str = "drafts";
const MANIFEST_FILE: &str = "manifest.json";

fn manifest_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Returns the drafts directory: `{data_dir}/drafts/`.
#[must_use]
pub fn drafts_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DRAFTS_DIR)
}

/// Generate a stable draft ID from an absolute file path using the
/// standard library's `DefaultHasher` (SipHash). Produces a 16-char
/// hex string that is deterministic for the same path.
#[must_use]
pub fn draft_id_for_path(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Generate a unique draft ID for an untitled tab using a monotonic
/// counter value. Each untitled tab gets a different ID.
#[must_use]
pub fn draft_id_for_untitled(counter: u64) -> String {
    format!("untitled-{counter:016x}")
}

/// Load the draft manifest from disk. Returns an empty manifest if
/// the file doesn't exist.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if the manifest file exists but cannot be read or parsed.
pub fn load_manifest(data_dir: &Path) -> Result<DraftManifest> {
    let dir = drafts_dir(data_dir);
    json_store::load(&dir, MANIFEST_FILE)
}

/// Save the draft manifest atomically (temp file + rename).
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if the manifest cannot be serialized or written.
///
/// # Panics
///
/// Panics if the process-wide manifest write lock is poisoned by an earlier
/// panic while the lock was held.
pub fn save_manifest(data_dir: &Path, manifest: &DraftManifest) -> Result<()> {
    let _guard = manifest_write_lock()
        .lock()
        .expect("draft manifest write lock poisoned");
    save_manifest_locked(data_dir, manifest)
}

fn save_manifest_locked(data_dir: &Path, manifest: &DraftManifest) -> Result<()> {
    let dir = drafts_dir(data_dir);
    json_store::save(&dir, MANIFEST_FILE, manifest)
}

/// Load, mutate, and save the draft manifest under a single lock.
/// Returns the final manifest snapshot written to disk.
///
/// # Errors
///
/// Returns an error if the manifest cannot be read or the updated manifest
/// cannot be written back to disk.
///
/// # Panics
///
/// Panics if the process-wide manifest write lock is poisoned by an earlier
/// panic while the lock was held.
pub fn update_manifest<F>(data_dir: &Path, update: F) -> Result<DraftManifest>
where
    F: FnOnce(&mut DraftManifest),
{
    let _guard = manifest_write_lock()
        .lock()
        .expect("draft manifest write lock poisoned");
    let mut manifest = load_manifest(data_dir)?;
    update(&mut manifest);
    save_manifest_locked(data_dir, &manifest)?;
    Ok(manifest)
}

/// Load the manifest, session, and any draft content needed for startup restore.
///
/// This intentionally preserves file-backed session tabs even when their paths
/// are temporarily unavailable, so startup does not turn a transient mount
/// outage into permanent session loss on the next save.
pub fn load_restore_state(
    data_dir: &Path,
) -> (
    DraftManifest,
    SessionData,
    HashMap<String, PreloadedDraftRestore>,
) {
    let mut manifest = load_manifest(data_dir).unwrap_or_default();
    let session = session_service::load(data_dir).unwrap_or_default();

    let mut preloaded = HashMap::new();
    let mut stale_draft_ids = Vec::new();
    for tab in &session.tabs {
        let draft_id = match &tab.path {
            Some(path) => draft_id_for_path(path),
            None => match &tab.draft_id {
                Some(id) => id.clone(),
                None => continue,
            },
        };
        let Some(entry) = manifest.find_by_id(&draft_id).cloned() else {
            continue;
        };
        if entry.original_path.is_some() {
            match resolve_file_draft_restore(data_dir, &entry) {
                Ok(FileDraftRestoreResolution::Restore { content }) => {
                    preloaded.insert(draft_id, PreloadedDraftRestore::Content(content));
                }
                Ok(FileDraftRestoreResolution::SkipStale) => {
                    preloaded.insert(draft_id.clone(), PreloadedDraftRestore::SkipStaleFile);
                    if !stale_draft_ids.contains(&draft_id) {
                        stale_draft_ids.push(draft_id);
                    }
                }
                Ok(
                    FileDraftRestoreResolution::SkipUnavailable
                    | FileDraftRestoreResolution::MissingDraft,
                ) => {}
                Err(e) => {
                    tracing::warn!("Failed to pre-resolve draft {draft_id}: {e}");
                }
            }
            continue;
        }

        match read_draft(data_dir, &draft_id) {
            Ok(Some(content)) => {
                preloaded.insert(draft_id, PreloadedDraftRestore::Content(content));
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("Failed to pre-read draft {draft_id}: {e}");
            }
        }
    }

    cleanup_stale_restore_entries(data_dir, &mut manifest, &stale_draft_ids);
    (manifest, session, preloaded)
}

/// Resolve whether a file-backed draft is still safe to restore.
///
/// This helper keeps blocking metadata checks and draft-file reads inside the
/// service layer so both startup preload and later `check_draft_on_open()`
/// calls can share one decision path.
///
/// # Errors
///
/// Returns an error if the draft file exists but cannot be read as UTF-8 text.
pub fn resolve_file_draft_restore(
    data_dir: &Path,
    entry: &DraftEntry,
) -> Result<FileDraftRestoreResolution> {
    let Some(path) = entry.original_path.as_deref() else {
        return Ok(FileDraftRestoreResolution::SkipUnavailable);
    };

    if let Some(saved_mtime) = entry.original_mtime_secs {
        let Some(current_mtime) = editor_io::mtime_secs(path) else {
            return Ok(FileDraftRestoreResolution::SkipUnavailable);
        };
        if current_mtime != saved_mtime {
            return Ok(FileDraftRestoreResolution::SkipStale);
        }
    }

    match read_draft(data_dir, &entry.draft_id)? {
        Some(content) => Ok(FileDraftRestoreResolution::Restore { content }),
        None => Ok(FileDraftRestoreResolution::MissingDraft),
    }
}

/// Delete stale draft files and remove their manifest entries after a confirmed
/// backing-file mismatch.
fn cleanup_stale_restore_entries(
    data_dir: &Path,
    manifest: &mut DraftManifest,
    stale_draft_ids: &[String],
) {
    if stale_draft_ids.is_empty() {
        return;
    }

    for draft_id in stale_draft_ids {
        if let Err(e) = delete_draft_file(data_dir, draft_id) {
            tracing::warn!("Failed to delete stale draft {draft_id}: {e}");
        }
    }

    match update_manifest(data_dir, |manifest| {
        for draft_id in stale_draft_ids {
            manifest.remove_by_id(draft_id);
        }
    }) {
        Ok(updated_manifest) => {
            *manifest = updated_manifest;
        }
        Err(e) => {
            tracing::warn!("Failed to persist stale draft cleanup: {e}");
            manifest
                .drafts
                .retain(|entry| !stale_draft_ids.iter().any(|id| id == &entry.draft_id));
        }
    }
}

/// Write a single draft file atomically (temp + rename). The draft
/// content is plain UTF-8 text, not wrapped in JSON.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if the drafts directory cannot be created or the draft file
/// cannot be written, flushed, synced, or renamed into place.
pub fn write_draft(data_dir: &Path, draft_id: &str, content: &str) -> Result<()> {
    let dir = drafts_dir(data_dir);
    durable_write::create_dir_all_durable(&dir)
        .with_context(|| format!("failed to create drafts dir: {}", dir.display()))?;

    let path = dir.join(format!("{draft_id}.draft"));
    let tmp_path = durable_write::unique_temp_path(&path, "draft");

    let mut file = std::fs::File::create(&tmp_path)
        .with_context(|| format!("failed to create temp draft: {}", tmp_path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("failed to write temp draft: {}", tmp_path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush temp draft: {}", tmp_path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync temp draft: {}", tmp_path.display()))?;

    std::fs::rename(&tmp_path, &path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;
    durable_write::sync_parent_dir(&path)
        .with_context(|| format!("failed to sync drafts dir for {}", path.display()))
}

/// Read a draft file's content. Returns `None` if the draft file
/// doesn't exist.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if an existing draft file cannot be read as UTF-8 text.
pub fn read_draft(data_dir: &Path, draft_id: &str) -> Result<Option<String>> {
    let path = drafts_dir(data_dir).join(format!("{draft_id}.draft"));
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!(
            "failed to read draft {}: {}",
            path.display(),
            e
        )),
    }
}

/// Delete a single draft file from disk. No-op if the file doesn't exist.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if an existing draft file cannot be deleted.
pub fn delete_draft_file(data_dir: &Path, draft_id: &str) -> Result<()> {
    let path = drafts_dir(data_dir).join(format!("{draft_id}.draft"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "failed to delete draft {}: {}",
            path.display(),
            e
        )),
    }
}

/// Remove orphaned drafts: draft files with no manifest entry, and
/// manifest entries whose draft file no longer exists. Returns the
/// count of items cleaned up.
///
/// **Threading:** blocking I/O, call from background thread.
///
/// # Errors
///
/// Returns an error if the drafts directory exists but its contents cannot be
/// inspected consistently enough to finish cleanup.
pub fn cleanup_orphans(data_dir: &Path, manifest: &mut DraftManifest) -> Result<usize> {
    let dir = drafts_dir(data_dir);
    let mut cleaned = 0;

    // Remove manifest entries whose draft file is missing.
    let before = manifest.drafts.len();
    manifest.drafts.retain(|entry| {
        let path = dir.join(format!("{}.draft", entry.draft_id));
        path.exists()
    });
    cleaned += before - manifest.drafts.len();

    // Remove draft files with no manifest entry.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if !name.ends_with(".draft") || name.starts_with('.') {
                continue;
            }
            let draft_id = name.trim_end_matches(".draft");
            if manifest.find_by_id(draft_id).is_none() {
                let _ = std::fs::remove_file(entry.path());
                cleaned += 1;
            }
        }
    }

    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::draft::{DraftEntry, FileDraftRestoreResolution};
    use crate::model::session::{SessionData, SessionTab};
    use tempfile::TempDir;

    fn file_entry(id: &str, path: &Path, original_mtime_secs: Option<u64>) -> DraftEntry {
        DraftEntry {
            draft_id: id.into(),
            original_path: Some(path.to_path_buf()),
            original_mtime_secs,
            saved_at_secs: 1,
        }
    }

    fn session_tab(path: &Path) -> SessionTab {
        SessionTab {
            path: Some(path.to_path_buf()),
            draft_id: None,
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
            pinned: false,
        }
    }

    #[test]
    fn draft_id_for_path_is_deterministic() {
        let path = Path::new("/home/user/project/src/main.rs");
        let id1 = draft_id_for_path(path);
        let id2 = draft_id_for_path(path);
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 16);
    }

    #[test]
    fn draft_id_differs_for_different_paths() {
        let id1 = draft_id_for_path(Path::new("/a.rs"));
        let id2 = draft_id_for_path(Path::new("/b.rs"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn draft_id_for_untitled_format() {
        let id = draft_id_for_untitled(42);
        assert!(id.starts_with("untitled-"));
        assert_eq!(id.len(), "untitled-".len() + 16);
    }

    #[test]
    fn write_and_read_draft_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        write_draft(dir.path(), "abc123", content).expect("expected operation to succeed");
        let read = read_draft(dir.path(), "abc123").expect("expected operation to succeed");
        assert_eq!(read, Some(content.to_string()));
    }

    #[test]
    fn read_draft_missing_returns_none() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let result = read_draft(dir.path(), "nonexistent").expect("expected operation to succeed");
        assert_eq!(result, None);
    }

    #[test]
    fn delete_draft_file_removes_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "abc123", "content").expect("expected operation to succeed");
        delete_draft_file(dir.path(), "abc123").expect("expected operation to succeed");
        let result = read_draft(dir.path(), "abc123").expect("expected operation to succeed");
        assert_eq!(result, None);
    }

    #[test]
    fn delete_draft_file_missing_is_noop() {
        let dir = TempDir::new().expect("expected operation to succeed");
        // Should not error
        delete_draft_file(dir.path(), "nonexistent").expect("expected operation to succeed");
    }

    #[test]
    fn read_draft_reports_non_file_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = drafts_dir(dir.path()).join("blocked.draft");
        std::fs::create_dir_all(&path).expect("expected operation to succeed");

        let error = read_draft(dir.path(), "blocked").expect_err("directory draft should fail");
        assert!(
            error.to_string().contains("failed to read draft"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn delete_draft_file_reports_non_file_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = drafts_dir(dir.path()).join("blocked.draft");
        std::fs::create_dir_all(&path).expect("expected operation to succeed");

        let error =
            delete_draft_file(dir.path(), "blocked").expect_err("directory draft should fail");
        assert!(
            error.to_string().contains("failed to delete draft"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn manifest_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "abc".into(),
                original_path: Some(PathBuf::from("/a.rs")),
                original_mtime_secs: Some(1000),
                saved_at_secs: 2000,
            }],
        };
        save_manifest(dir.path(), &manifest).expect("expected operation to succeed");
        let loaded = load_manifest(dir.path()).expect("expected operation to succeed");
        assert_eq!(loaded, manifest);
    }

    #[test]
    fn load_manifest_missing_returns_default() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let manifest = load_manifest(dir.path()).expect("expected operation to succeed");
        assert_eq!(manifest, DraftManifest::default());
    }

    #[test]
    fn update_manifest_serializes_concurrent_read_modify_write() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let data_dir = dir.path().to_path_buf();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let first_data_dir = data_dir.clone();

        let first = std::thread::spawn(move || {
            update_manifest(&first_data_dir, |manifest| {
                manifest.upsert(DraftEntry {
                    draft_id: "first".into(),
                    original_path: Some(PathBuf::from("/first.rs")),
                    original_mtime_secs: None,
                    saved_at_secs: 1,
                });
                entered_tx.send(()).expect("expected operation to succeed");
                // Hold the read-modify-write lock long enough for the second
                // thread to prove it waits for the first saved snapshot.
                std::thread::sleep(std::time::Duration::from_millis(150));
            })
            .expect("expected operation to succeed");
        });

        entered_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("expected first update to enter critical section");
        let second_data_dir = data_dir.clone();
        let second = std::thread::spawn(move || {
            update_manifest(&second_data_dir, |manifest| {
                manifest.upsert(DraftEntry {
                    draft_id: "second".into(),
                    original_path: Some(PathBuf::from("/second.rs")),
                    original_mtime_secs: None,
                    saved_at_secs: 2,
                });
            })
            .expect("expected operation to succeed");
        });

        first.join().expect("first update should not panic");
        second.join().expect("second update should not panic");

        let manifest = load_manifest(&data_dir).expect("expected operation to succeed");
        assert_eq!(manifest.drafts.len(), 2);
        assert!(manifest.find_by_id("first").is_some());
        assert!(manifest.find_by_id("second").is_some());
    }

    #[test]
    fn resolve_file_draft_restore_returns_content_when_mtime_matches() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "disk content").expect("expected operation to succeed");
        write_draft(dir.path(), "draft", "restored content")
            .expect("expected operation to succeed");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");

        let resolution = resolve_file_draft_restore(
            dir.path(),
            &file_entry("draft", &path, Some(current_mtime)),
        )
        .expect("expected operation to succeed");

        assert_eq!(
            resolution,
            FileDraftRestoreResolution::Restore {
                content: "restored content".to_string()
            }
        );
    }

    #[test]
    fn resolve_file_draft_restore_skips_changed_file_mtime() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "disk content").expect("expected operation to succeed");
        write_draft(dir.path(), "draft", "stale content").expect("expected operation to succeed");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");
        let stale_mtime = current_mtime
            .checked_add(1)
            .unwrap_or_else(|| current_mtime.saturating_sub(1));

        let resolution =
            resolve_file_draft_restore(dir.path(), &file_entry("draft", &path, Some(stale_mtime)))
                .expect("expected operation to succeed");

        assert_eq!(resolution, FileDraftRestoreResolution::SkipStale);
    }

    #[test]
    fn resolve_file_draft_restore_allows_legacy_entries_without_stored_mtime() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "disk content").expect("expected operation to succeed");
        write_draft(dir.path(), "draft", "legacy content").expect("expected operation to succeed");

        let resolution = resolve_file_draft_restore(dir.path(), &file_entry("draft", &path, None))
            .expect("expected operation to succeed");

        assert_eq!(
            resolution,
            FileDraftRestoreResolution::Restore {
                content: "legacy content".to_string()
            }
        );
    }

    #[test]
    fn resolve_file_draft_restore_skips_when_metadata_cannot_be_read() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let missing_path = dir.path().join("missing.txt");
        write_draft(dir.path(), "draft", "content").expect("expected operation to succeed");

        let resolution =
            resolve_file_draft_restore(dir.path(), &file_entry("draft", &missing_path, Some(123)))
                .expect("expected operation to succeed");

        assert_eq!(resolution, FileDraftRestoreResolution::SkipUnavailable);
    }

    #[test]
    fn resolve_file_draft_restore_handles_missing_draft_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "disk content").expect("expected operation to succeed");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");

        let resolution = resolve_file_draft_restore(
            dir.path(),
            &file_entry("draft", &path, Some(current_mtime)),
        )
        .expect("expected operation to succeed");

        assert_eq!(resolution, FileDraftRestoreResolution::MissingDraft);
    }

    #[test]
    fn load_restore_state_removes_stale_file_draft_from_manifest_and_disk() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "disk content").expect("expected operation to succeed");
        let current_mtime = crate::services::editor_io::mtime_secs(&path).expect("expected mtime");
        let stale_mtime = current_mtime
            .checked_add(1)
            .unwrap_or_else(|| current_mtime.saturating_sub(1));
        let draft_id = draft_id_for_path(&path);

        write_draft(dir.path(), &draft_id, "stale content").expect("expected operation to succeed");
        save_manifest(
            dir.path(),
            &DraftManifest {
                drafts: vec![file_entry(&draft_id, &path, Some(stale_mtime))],
            },
        )
        .expect("expected operation to succeed");
        session_service::save(
            dir.path(),
            &SessionData {
                tabs: vec![session_tab(&path)],
                active_tab_index: Some(0),
            },
        )
        .expect("expected operation to succeed");

        let (manifest, _session, preloaded) = load_restore_state(dir.path());

        assert_eq!(
            preloaded.get(&draft_id),
            Some(&PreloadedDraftRestore::SkipStaleFile)
        );
        assert!(manifest.find_by_id(&draft_id).is_none());
        assert!(
            load_manifest(dir.path())
                .expect("expected operation to succeed")
                .find_by_id(&draft_id)
                .is_none()
        );
        assert_eq!(
            read_draft(dir.path(), &draft_id).expect("expected operation to succeed"),
            None
        );
    }

    #[test]
    fn stale_restore_cleanup_fallback_retains_only_non_stale_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "stale", "stale content").expect("expected operation to succeed");
        write_draft(dir.path(), "keep", "keep content").expect("expected operation to succeed");
        let keep = DraftEntry {
            draft_id: "keep".into(),
            original_path: Some(PathBuf::from("/keep.rs")),
            original_mtime_secs: None,
            saved_at_secs: 2,
        };
        let mut manifest = DraftManifest {
            drafts: vec![
                DraftEntry {
                    draft_id: "stale".into(),
                    original_path: Some(PathBuf::from("/stale.rs")),
                    original_mtime_secs: None,
                    saved_at_secs: 1,
                },
                keep.clone(),
            ],
        };
        std::fs::create_dir_all(drafts_dir(dir.path()).join(MANIFEST_FILE))
            .expect("expected operation to succeed");

        cleanup_stale_restore_entries(dir.path(), &mut manifest, &[String::from("stale")]);

        assert_eq!(manifest.drafts, vec![keep]);
        assert_eq!(
            read_draft(dir.path(), "stale").expect("expected operation to succeed"),
            None
        );
        assert_eq!(
            read_draft(dir.path(), "keep").expect("expected operation to succeed"),
            Some("keep content".to_string())
        );
    }

    #[test]
    fn cleanup_orphans_removes_entries_without_files() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "ghost".into(),
                original_path: Some(PathBuf::from("/gone.rs")),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            }],
        };
        // Don't create the draft file — the manifest entry is orphaned
        std::fs::create_dir_all(drafts_dir(dir.path())).expect("expected operation to succeed");
        let cleaned =
            cleanup_orphans(dir.path(), &mut manifest).expect("expected operation to succeed");
        assert_eq!(cleaned, 1);
        assert!(manifest.drafts.is_empty());
    }

    #[test]
    fn cleanup_orphans_removes_files_without_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut manifest = DraftManifest::default();
        // Create a draft file with no manifest entry
        write_draft(dir.path(), "orphan", "stale content").expect("expected operation to succeed");
        let cleaned =
            cleanup_orphans(dir.path(), &mut manifest).expect("expected operation to succeed");
        assert_eq!(cleaned, 1);
        assert_eq!(
            read_draft(dir.path(), "orphan").expect("expected operation to succeed"),
            None
        );
    }

    #[test]
    fn cleanup_orphans_keeps_valid_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "valid", "content").expect("expected operation to succeed");
        let mut manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "valid".into(),
                original_path: Some(PathBuf::from("/a.rs")),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            }],
        };
        let cleaned =
            cleanup_orphans(dir.path(), &mut manifest).expect("expected operation to succeed");
        assert_eq!(cleaned, 0);
        assert_eq!(manifest.drafts.len(), 1);
    }

    #[test]
    fn cleanup_orphans_ignores_hidden_draft_files() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let hidden_path = drafts_dir(dir.path()).join(".hidden.draft");
        std::fs::create_dir_all(hidden_path.parent().expect("expected drafts dir"))
            .expect("expected operation to succeed");
        std::fs::write(&hidden_path, "editor swap").expect("expected operation to succeed");
        let mut manifest = DraftManifest::default();

        let cleaned =
            cleanup_orphans(dir.path(), &mut manifest).expect("expected operation to succeed");

        assert_eq!(cleaned, 0);
        assert!(hidden_path.exists());
    }

    #[test]
    fn write_draft_creates_directory() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let nested = dir.path().join("deep/path");
        write_draft(&nested, "abc", "content").expect("expected operation to succeed");
        let result = read_draft(&nested, "abc").expect("expected operation to succeed");
        assert_eq!(result, Some("content".to_string()));
    }

    #[test]
    fn write_draft_overwrites_existing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        write_draft(dir.path(), "abc", "first").expect("expected operation to succeed");
        write_draft(dir.path(), "abc", "second").expect("expected operation to succeed");
        let result = read_draft(dir.path(), "abc").expect("expected operation to succeed");
        assert_eq!(result, Some("second".to_string()));
    }
}

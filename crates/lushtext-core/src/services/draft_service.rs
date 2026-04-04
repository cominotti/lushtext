// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft persistence service — save and restore unsaved buffer content.
//!
//! All functions perform blocking I/O and must be called from a background
//! thread via `spawn_blocking_then`. Drafts are stored as plain UTF-8 text
//! files in `$XDG_DATA_HOME/lushtext/drafts/`, with a JSON manifest mapping
//! draft IDs to original file paths and metadata.

use crate::model::draft::DraftManifest;
use crate::services::json_store;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

const DRAFTS_DIR: &str = "drafts";
const MANIFEST_FILE: &str = "manifest.json";

/// Returns the drafts directory: `{data_dir}/drafts/`.
pub fn drafts_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DRAFTS_DIR)
}

/// Generate a stable draft ID from an absolute file path using the
/// standard library's `DefaultHasher` (SipHash). Produces a 16-char
/// hex string that is deterministic for the same path.
pub fn draft_id_for_path(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Generate a unique draft ID for an untitled tab using a monotonic
/// counter value. Each untitled tab gets a different ID.
pub fn draft_id_for_untitled(counter: u64) -> String {
    format!("untitled-{counter:016x}")
}

/// Load the draft manifest from disk. Returns an empty manifest if
/// the file doesn't exist.
///
/// **Threading:** blocking I/O, call from background thread.
pub fn load_manifest(data_dir: &Path) -> Result<DraftManifest> {
    let dir = drafts_dir(data_dir);
    json_store::load(&dir, MANIFEST_FILE)
}

/// Save the draft manifest atomically (temp file + rename).
///
/// **Threading:** blocking I/O, call from background thread.
pub fn save_manifest(data_dir: &Path, manifest: &DraftManifest) -> Result<()> {
    let dir = drafts_dir(data_dir);
    json_store::save(&dir, MANIFEST_FILE, manifest)
}

/// Write a single draft file atomically (temp + rename). The draft
/// content is plain UTF-8 text, not wrapped in JSON.
///
/// **Threading:** blocking I/O, call from background thread.
pub fn write_draft(data_dir: &Path, draft_id: &str, content: &str) -> Result<()> {
    let dir = drafts_dir(data_dir);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create drafts dir: {}", dir.display()))?;

    let path = dir.join(format!("{draft_id}.draft"));
    let tmp_path = dir.join(format!(".{draft_id}.draft.tmp"));

    let mut file = std::fs::File::create(&tmp_path)
        .with_context(|| format!("failed to create temp draft: {}", tmp_path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("failed to write temp draft: {}", tmp_path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush temp draft: {}", tmp_path.display()))?;

    std::fs::rename(&tmp_path, &path).with_context(|| {
        format!(
            "failed to rename {} to {}",
            tmp_path.display(),
            path.display()
        )
    })
}

/// Read a draft file's content. Returns `None` if the draft file
/// doesn't exist.
///
/// **Threading:** blocking I/O, call from background thread.
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
    use crate::model::draft::DraftEntry;
    use tempfile::TempDir;

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
        let dir = TempDir::new().unwrap();
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        write_draft(dir.path(), "abc123", content).unwrap();
        let read = read_draft(dir.path(), "abc123").unwrap();
        assert_eq!(read, Some(content.to_string()));
    }

    #[test]
    fn read_draft_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let result = read_draft(dir.path(), "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn delete_draft_file_removes_file() {
        let dir = TempDir::new().unwrap();
        write_draft(dir.path(), "abc123", "content").unwrap();
        delete_draft_file(dir.path(), "abc123").unwrap();
        let result = read_draft(dir.path(), "abc123").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn delete_draft_file_missing_is_noop() {
        let dir = TempDir::new().unwrap();
        // Should not error
        delete_draft_file(dir.path(), "nonexistent").unwrap();
    }

    #[test]
    fn manifest_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "abc".into(),
                original_path: Some(PathBuf::from("/a.rs")),
                original_mtime_secs: Some(1000),
                saved_at_secs: 2000,
            }],
        };
        save_manifest(dir.path(), &manifest).unwrap();
        let loaded = load_manifest(dir.path()).unwrap();
        assert_eq!(loaded, manifest);
    }

    #[test]
    fn load_manifest_missing_returns_default() {
        let dir = TempDir::new().unwrap();
        let manifest = load_manifest(dir.path()).unwrap();
        assert_eq!(manifest, DraftManifest::default());
    }

    #[test]
    fn cleanup_orphans_removes_entries_without_files() {
        let dir = TempDir::new().unwrap();
        let mut manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "ghost".into(),
                original_path: Some(PathBuf::from("/gone.rs")),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            }],
        };
        // Don't create the draft file — the manifest entry is orphaned
        std::fs::create_dir_all(drafts_dir(dir.path())).unwrap();
        let cleaned = cleanup_orphans(dir.path(), &mut manifest).unwrap();
        assert_eq!(cleaned, 1);
        assert!(manifest.drafts.is_empty());
    }

    #[test]
    fn cleanup_orphans_removes_files_without_entries() {
        let dir = TempDir::new().unwrap();
        let mut manifest = DraftManifest::default();
        // Create a draft file with no manifest entry
        write_draft(dir.path(), "orphan", "stale content").unwrap();
        let cleaned = cleanup_orphans(dir.path(), &mut manifest).unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(read_draft(dir.path(), "orphan").unwrap(), None);
    }

    #[test]
    fn cleanup_orphans_keeps_valid_entries() {
        let dir = TempDir::new().unwrap();
        write_draft(dir.path(), "valid", "content").unwrap();
        let mut manifest = DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: "valid".into(),
                original_path: Some(PathBuf::from("/a.rs")),
                original_mtime_secs: None,
                saved_at_secs: 1000,
            }],
        };
        let cleaned = cleanup_orphans(dir.path(), &mut manifest).unwrap();
        assert_eq!(cleaned, 0);
        assert_eq!(manifest.drafts.len(), 1);
    }

    #[test]
    fn write_draft_creates_directory() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("deep/path");
        write_draft(&nested, "abc", "content").unwrap();
        let result = read_draft(&nested, "abc").unwrap();
        assert_eq!(result, Some("content".to_string()));
    }

    #[test]
    fn write_draft_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        write_draft(dir.path(), "abc", "first").unwrap();
        write_draft(dir.path(), "abc", "second").unwrap();
        let result = read_draft(dir.path(), "abc").unwrap();
        assert_eq!(result, Some("second".to_string()));
    }
}

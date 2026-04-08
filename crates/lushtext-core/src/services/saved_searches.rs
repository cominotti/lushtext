// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved search persistence: load, save, add, and remove named searches.
//!
//! Saved searches are permanent (no cap) and user-managed — they persist
//! until explicitly deleted. Persisted to `saved-searches.json` via `json_store`.

use crate::model::content_search::SavedSearch;
use crate::services::json_store;
use std::path::Path;

/// Filename for the saved searches JSON file.
const SAVED_SEARCHES_FILE: &str = "saved-searches.json";

/// Load saved searches from disk. Returns an empty vec on missing or corrupt file.
pub fn load(data_dir: &Path) -> Vec<SavedSearch> {
    match json_store::load::<Vec<SavedSearch>>(data_dir, SAVED_SEARCHES_FILE) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("Failed to load saved searches, using empty: {e}");
            Vec::new()
        }
    }
}

/// Save saved searches to disk via atomic write.
pub fn save(data_dir: &Path, entries: &[SavedSearch]) -> anyhow::Result<()> {
    json_store::save(data_dir, SAVED_SEARCHES_FILE, &entries)
}

/// Add a new saved search, prepending it to the list.
///
/// No dedup and no cap — saved searches are user-named and permanent.
pub fn add(entries: &mut Vec<SavedSearch>, entry: SavedSearch) {
    entries.insert(0, entry);
}

/// Remove a saved search by index. No-op if index is out of bounds.
pub fn remove(entries: &mut Vec<SavedSearch>, index: usize) {
    if index < entries.len() {
        entries.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_saved(name: &str, query: &str) -> SavedSearch {
        SavedSearch {
            name: name.to_string(),
            query: query.to_string(),
            case_sensitive: false,
            regex: false,
            whole_word: false,
            gitignore: true,
            glob: None,
        }
    }

    #[test]
    fn test_add_prepends() {
        let mut entries = vec![make_saved("old", "old-query")];
        add(&mut entries, make_saved("new", "new-query"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "new");
        assert_eq!(entries[1].name, "old");
    }

    #[test]
    fn test_remove_valid_index() {
        let mut entries = vec![
            make_saved("a", "query-a"),
            make_saved("b", "query-b"),
            make_saved("c", "query-c"),
        ];
        remove(&mut entries, 1);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a");
        assert_eq!(entries[1].name, "c");
    }

    #[test]
    fn test_remove_out_of_bounds() {
        let mut entries = vec![make_saved("a", "query-a")];
        remove(&mut entries, 5); // Should not panic.
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_load_missing_file_returns_empty() {
        let dir = TempDir::new().unwrap();
        let entries = load(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let entries = vec![
            SavedSearch {
                name: "Rust files".to_string(),
                query: "fn main".to_string(),
                case_sensitive: true,
                regex: false,
                whole_word: true,
                gitignore: false,
                glob: Some("*.rs".to_string()),
            },
            make_saved("TODOs", "TODO"),
        ];
        save(dir.path(), &entries).unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded, entries);
    }
}

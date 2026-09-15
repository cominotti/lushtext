// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved search persistence: load, save, add, and remove named searches.
//!
//! Saved searches are permanent (no cap) and user-managed — they persist
//! until explicitly deleted. They use recovery-aware v1 envelopes so damaged
//! or unsupported files are preserved before replacement.

use crate::model::content_search::SavedSearch;
use crate::services::json_format::KIND_SAVED_SEARCHES;
use crate::services::recovery_metadata::{
    RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass, load_enveloped_json_or_default,
    save_enveloped_json_path,
};
use std::path::Path;

/// Filename for the saved searches JSON file.
const SAVED_SEARCHES_FILE: &str = "saved-searches.json";

/// Load saved searches from disk. Returns an empty vec on missing or corrupt file.
pub fn load(data_dir: &Path) -> Vec<SavedSearch> {
    let load = load_recovering(data_dir);
    for diagnostic in &load.diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    load.value
}

/// Load saved searches with preservation diagnostics for user-managed state.
#[must_use]
pub fn load_recovering(data_dir: &Path) -> RecoveryLoad<Vec<SavedSearch>> {
    let path = data_dir.join(SAVED_SEARCHES_FILE);
    load_enveloped_json_or_default(
        &RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::SavedSearches),
        KIND_SAVED_SEARCHES,
    )
}

/// Save saved searches to disk via atomic write.
///
/// # Errors
///
/// Returns an error if the saved-search file cannot be serialized or written.
pub fn save(data_dir: &Path, entries: &[SavedSearch]) -> anyhow::Result<()> {
    let path = data_dir.join(SAVED_SEARCHES_FILE);
    let config = RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::SavedSearches);
    let entries = entries.to_vec();
    let diagnostics = save_enveloped_json_path(&config, KIND_SAVED_SEARCHES, &entries)?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
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
    use crate::services::filesystem::fixture;
    use crate::services::recovery_metadata::RecoveryProblem;
    use tempfile::TempDir;

    fn make_saved(name: &str, query: &str) -> SavedSearch {
        SavedSearch::from_spec(
            name.to_string(),
            crate::model::content_search::SearchQuerySpec::new(
                query.to_string(),
                crate::model::content_search::ContentSearchOptions::default(),
            ),
        )
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
    fn test_remove_len_index_is_out_of_bounds() {
        let mut entries = vec![make_saved("a", "query-a")];
        remove(&mut entries, 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "a");
    }

    #[test]
    fn test_load_missing_file_returns_empty() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let entries = load(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let entries = vec![
            SavedSearch::from_spec(
                "Rust files".to_string(),
                crate::model::content_search::SearchQuerySpec::new(
                    "fn main".to_string(),
                    crate::model::content_search::ContentSearchOptions::new(
                        true,
                        false,
                        true,
                        false,
                        Some("*.rs".to_string()),
                    ),
                ),
            ),
            make_saved("TODOs", "TODO"),
        ];
        save(dir.path(), &entries).expect("expected operation to succeed");
        let loaded = load_recovering(dir.path());
        assert!(loaded.diagnostics.is_empty());
        let loaded = loaded.value;
        assert_eq!(loaded, entries);
        assert_eq!(load(dir.path()), entries);
    }

    #[test]
    fn load_recovering_preserves_unsupported_old_saved_searches() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join(SAVED_SEARCHES_FILE), r#"[{"name":"old"}]"#);

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.is_empty());
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::UnsupportedFormat { .. }
        ));
        let quarantine_path = loaded.diagnostics[0]
            .preservation
            .quarantine_path()
            .expect("saved searches quarantine");
        assert!(fixture::read_text(quarantine_path).contains("old"));
    }

    #[test]
    fn saved_searches_written_before_hidden_field_still_load() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let entries = vec![make_saved("Legacy", "needle")];
        save(dir.path(), &entries).expect("save");
        let path = dir.path().join(SAVED_SEARCHES_FILE);
        let json = crate::services::filesystem::read::text(&path).expect("read saved searches");
        assert!(
            json.contains("\"hidden\""),
            "fixture must contain the new field to strip"
        );
        let mut document: serde_json::Value = serde_json::from_str(&json).expect("parse envelope");
        fixture::strip_json_field(&mut document, "hidden");
        let legacy = serde_json::to_string_pretty(&document).expect("serialize legacy fixture");
        assert!(!legacy.contains("\"hidden\""));
        fixture::write_text(&path, &legacy);

        let load = load_recovering(dir.path());
        assert!(
            load.diagnostics.is_empty(),
            "no quarantine: {:?}",
            load.diagnostics
        );
        assert_eq!(load.value.len(), 1);
        assert_eq!(load.value[0].name, "Legacy");
        assert!(!load.value[0].spec.options.hidden);
    }
}

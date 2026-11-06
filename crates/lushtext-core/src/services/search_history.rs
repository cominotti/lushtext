// SPDX-License-Identifier: GPL-3.0-or-later

//! Search history persistence: load, save, and manage recent search entries.
//!
//! History is capped at 32 entries (FIFO), deduplicated on identical query +
//! toggle state, and persisted to a v1 envelope. This remains low-stakes state:
//! damaged history degrades to an empty list with diagnostics.

use crate::model::content_search::SearchHistoryEntry;
use crate::services::json_format::KIND_SEARCH_HISTORY;
use crate::services::recovery_metadata::{
    RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass, load_enveloped_json_or_default,
    save_enveloped_json_path,
};
use std::path::Path;

/// Maximum number of history entries to retain.
///
/// Kind: retention tier; 20 rounded up to a power of two
/// (`power-of-two-thresholds` D6).
const MAX_HISTORY: usize = 32;

/// Filename for the search history JSON file.
const HISTORY_FILE: &str = "search-history.json";

/// Load search history from disk. Returns an empty vec on missing or corrupt file.
pub fn load(data_dir: &Path) -> Vec<SearchHistoryEntry> {
    let load = load_recovering(data_dir);
    for diagnostic in &load.diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    load.value
}

/// Load recent search history with low-stakes recovery diagnostics.
#[must_use]
pub fn load_recovering(data_dir: &Path) -> RecoveryLoad<Vec<SearchHistoryEntry>> {
    let path = data_dir.join(HISTORY_FILE);
    load_enveloped_json_or_default(
        &RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::SearchHistory),
        KIND_SEARCH_HISTORY,
    )
}

/// Save search history to disk via atomic write.
///
/// # Errors
///
/// Returns an error if the history file cannot be serialized or written.
pub fn save(data_dir: &Path, entries: &[SearchHistoryEntry]) -> anyhow::Result<()> {
    let path = data_dir.join(HISTORY_FILE);
    let config = RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::SearchHistory);
    let entries = entries.to_vec();
    let diagnostics = save_enveloped_json_path(&config, KIND_SEARCH_HISTORY, &entries)?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
}

/// Add a new entry to the history, deduplicating and capping at `MAX_HISTORY` (32).
///
/// If an identical entry (same query + all toggle states + glob) already exists,
/// it is moved to the front instead of being duplicated.
pub fn add_entry(entries: &mut Vec<SearchHistoryEntry>, entry: SearchHistoryEntry) {
    entries.retain(|e| e != &entry);
    entries.insert(0, entry);
    entries.truncate(MAX_HISTORY);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::content_search::{ContentSearchOptions, SearchQuerySpec};
    use crate::services::filesystem::fixture;
    use crate::services::recovery_metadata::RecoveryProblem;
    use tempfile::TempDir;

    fn make_entry(query: &str) -> SearchHistoryEntry {
        SearchHistoryEntry::from_spec(crate::model::content_search::SearchQuerySpec::new(
            query.to_string(),
            crate::model::content_search::ContentSearchOptions::default(),
        ))
    }

    #[test]
    fn test_add_entry_prepends() {
        let mut entries = vec![make_entry("old")];
        add_entry(&mut entries, make_entry("new"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].spec.query, "new");
        assert_eq!(entries[1].spec.query, "old");
    }

    #[test]
    fn test_add_entry_caps_at_max_history() {
        let mut entries: Vec<_> = (0..MAX_HISTORY)
            .map(|i| make_entry(&format!("query-{i}")))
            .collect();
        assert_eq!(entries.len(), 32);

        add_entry(&mut entries, make_entry("query-new"));
        assert_eq!(entries.len(), MAX_HISTORY);
        assert_eq!(entries[0].spec.query, "query-new");
        // The oldest entry (query-31) should be removed.
        assert!(entries.iter().all(|e| e.spec.query != "query-31"));
    }

    #[test]
    fn test_add_entry_deduplicates() {
        let mut entries = vec![
            make_entry("first"),
            make_entry("second"),
            make_entry("third"),
        ];
        // Re-add "second" — should move to top, not duplicate.
        add_entry(&mut entries, make_entry("second"));
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].spec.query, "second");
        assert_eq!(entries[1].spec.query, "first");
        assert_eq!(entries[2].spec.query, "third");
    }

    #[test]
    fn test_add_entry_different_settings_not_dedup() {
        let mut entries = vec![make_entry("query")];
        let mut different = make_entry("query");
        different.spec.options.case_sensitive = true; // Different setting — not a duplicate.
        add_entry(&mut entries, different);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].spec.options.case_sensitive);
        assert!(!entries[1].spec.options.case_sensitive);
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
            SearchHistoryEntry::from_spec(crate::model::content_search::SearchQuerySpec::new(
                "hello".to_string(),
                crate::model::content_search::ContentSearchOptions::new(
                    true,
                    false,
                    true,
                    false,
                    Some("*.rs".to_string()),
                ),
            )),
            make_entry("world"),
        ];
        save(dir.path(), &entries).expect("expected operation to succeed");
        let loaded = load_recovering(dir.path());
        assert!(loaded.diagnostics.is_empty());
        let loaded = loaded.value;
        assert_eq!(loaded, entries);
        assert_eq!(load(dir.path()), entries);
    }

    #[test]
    fn load_recovering_reports_unsupported_old_history() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join(HISTORY_FILE), r#"[{"query":"old"}]"#);

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.is_empty());
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::UnsupportedFormat { .. }
        ));
        assert!(loaded.replacement_allowed());
    }

    #[test]
    fn history_written_before_hidden_field_still_loads() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let entries = vec![SearchHistoryEntry::from_spec(SearchQuerySpec::new(
            "needle".to_string(),
            ContentSearchOptions::default(),
        ))];
        save(dir.path(), &entries).expect("save");
        let path = dir.path().join(HISTORY_FILE);
        let json = crate::services::filesystem::read::text(&path).expect("read history");
        assert!(json.contains("\"hidden\""));
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
        assert!(!load.value[0].spec.options.hidden);
    }

    /// `power-of-two-thresholds` D6: a 20-entry history keeps all 20 entries
    /// plus the new search (the tier only grows).
    #[test]
    fn upgrading_keeps_a_20_entry_history_after_one_more_search() {
        let mut entries: Vec<_> = (0..20)
            .map(|i| make_entry(&format!("stored-{i}")))
            .collect();
        add_entry(&mut entries, make_entry("new"));
        assert_eq!(entries.len(), 21);
        assert!((0..20).all(|i| {
            entries
                .iter()
                .any(|e| e.spec.query == format!("stored-{i}"))
        }));
    }

    /// A full history of the largest entries the search entry admits (its
    /// 16 KiB paste cap, query and glob) loads in full through the bounded
    /// metadata read.
    #[test]
    fn a_full_history_of_the_largest_entries_loads_in_full() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let entries: Vec<_> = (0..MAX_HISTORY)
            .map(|i| {
                let options = ContentSearchOptions {
                    glob: Some(format!("{i:02}{}", "g".repeat(16 * 1024 - 2))),
                    ..ContentSearchOptions::default()
                };
                SearchHistoryEntry::from_spec(SearchQuerySpec::new(
                    format!("{i:02}{}", "q".repeat(16 * 1024 - 2)),
                    options,
                ))
            })
            .collect();
        save(dir.path(), &entries).expect("save a full history");
        assert_eq!(load(dir.path()), entries);
        assert_eq!(MAX_HISTORY, 32);
    }
}

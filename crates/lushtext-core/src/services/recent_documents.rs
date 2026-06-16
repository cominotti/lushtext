// SPDX-License-Identifier: GPL-3.0-or-later

//! Recent-document persistence and search for the Open popover.
//!
//! The service is GTK-free and uses the shared JSON/filesystem boundary. The
//! window adapter decides when an open was explicit enough to record; this
//! service only maintains the app-owned recent history once asked.

use anyhow::Result;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::recent_document::{RecentDocumentEntry, RecentDocumentFile, RecentDocumentRow};
use crate::services::filesystem::metadata as fs_metadata;
use crate::services::filesystem::read as fs_read;
use crate::services::filesystem::types::PathStatus;
use crate::services::json_store;

/// Recent-document storage filename under the app data directory.
pub const RECENT_DOCUMENTS_FILE: &str = "recent-documents.json";
/// Keep the full history bounded while still far exceeding the Open popover view.
const MAX_RECENTS: usize = 200;
/// Refuse oversized app-owned recent metadata before parsing or path probing.
const MAX_RECENT_DOCUMENTS_BYTES: u64 = 1024 * 1024;
/// Bound externally modified files before filesystem status probes run.
const MAX_RECENT_LOAD_CANDIDATES: usize = MAX_RECENTS * 2;
/// Keep startup diagnostics useful without allocating one warning per bad row.
const MAX_RECENT_LOAD_DIAGNOSTICS: usize = 20;

/// Result of loading and pruning recent-document persistence.
#[derive(Debug, Clone)]
pub struct RecentDocumentsLoad {
    /// Newest-first supported, existing local file entries.
    pub entries: Vec<RecentDocumentEntry>,
    /// Whether missing/unsupported/duplicate rows were removed while loading.
    pub pruned: bool,
    /// Recoverable diagnostics suitable for tracing.
    pub diagnostics: Vec<String>,
}

/// Plain service-level representation of a requested recent-document target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecentOpenTarget {
    /// Supported local filesystem path.
    Local(PathBuf),
    /// Unsupported URI retained only long enough to reject it.
    UnsupportedUri(String),
}

impl RecentOpenTarget {
    /// Build a local target from a filesystem path.
    #[must_use]
    pub fn local(path: PathBuf) -> Self {
        Self::Local(path)
    }

    /// Build an unsupported target from a non-local URI.
    #[must_use]
    pub fn unsupported_uri(uri: impl Into<String>) -> Self {
        Self::UnsupportedUri(uri.into())
    }
}

/// Load recent documents from disk, recovering corrupt files as an empty list.
#[must_use]
pub fn load(data_dir: &Path) -> RecentDocumentsLoad {
    let mut diagnostics = Vec::new();
    let path = data_dir.join(RECENT_DOCUMENTS_FILE);
    let (file, file_pruned) = load_recent_file(&path, &mut diagnostics);
    let (entries, entries_pruned) = dedupe_sort_prune_existing(file.entries, &mut diagnostics);
    RecentDocumentsLoad {
        pruned: file_pruned || entries_pruned,
        entries,
        diagnostics,
    }
}

/// Save recent documents to disk.
///
/// # Errors
///
/// Returns an error if the JSON file cannot be serialized or written.
pub fn save(data_dir: &Path, entries: &[RecentDocumentEntry]) -> Result<()> {
    let file = RecentDocumentFile {
        entries: entries.to_vec(),
    };
    json_store::save(data_dir, RECENT_DOCUMENTS_FILE, &file)
}

/// Add or update a recent entry, moving the matching path identity to the top.
pub fn add_or_update(
    entries: &mut Vec<RecentDocumentEntry>,
    path: PathBuf,
    canonical_path: Option<PathBuf>,
    last_opened_secs: u64,
) {
    let new_entry = RecentDocumentEntry::new(path, canonical_path, last_opened_secs);
    entries.retain(|entry| !same_identity(entry, &new_entry));
    entries.insert(0, new_entry);
    entries.sort_by(sort_newest_first);
    entries.truncate(MAX_RECENTS);
}

/// Remove one recent entry by display or canonical path.
pub fn remove(entries: &mut Vec<RecentDocumentEntry>, path: &Path) {
    entries.retain(|entry| !entry.matches_path(path));
}

/// Convert recent entries into visible rows excluding already-open identities.
#[must_use]
pub fn visible_rows(
    entries: &[RecentDocumentEntry],
    open_identities: &[PathBuf],
    now_secs: u64,
) -> Vec<RecentDocumentRow> {
    let open_identities = open_identities.iter().cloned().collect::<HashSet<_>>();
    visible_rows_for_open_set(entries, &open_identities, now_secs)
}

/// Convert recent entries into visible rows using the window's open-path set directly.
#[must_use]
pub fn visible_rows_for_open_set(
    entries: &[RecentDocumentEntry],
    open_identities: &HashSet<PathBuf>,
    now_secs: u64,
) -> Vec<RecentDocumentRow> {
    entries
        .iter()
        .filter(|entry| !entry_is_open(entry, open_identities))
        .map(|entry| RecentDocumentRow::from_entry(entry, now_secs))
        .collect()
}

/// Merge loaded startup rows into current state without replacing newer mutations.
pub fn merge_loaded_entries(
    current: &mut Vec<RecentDocumentEntry>,
    loaded: Vec<RecentDocumentEntry>,
) -> bool {
    let mut changed = false;
    for entry in loaded {
        if current
            .iter()
            .any(|current_entry| same_identity(current_entry, &entry))
        {
            continue;
        }
        current.push(entry);
        changed = true;
    }
    current.sort_by(sort_newest_first);
    if current.len() > MAX_RECENTS {
        current.truncate(MAX_RECENTS);
        changed = true;
    }
    changed
}

/// Search recent rows with prefix, substring, then fuzzy ranking.
#[must_use]
pub fn search_rows(rows: &[RecentDocumentRow], query: &str) -> Vec<RecentDocumentRow> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return rows.to_vec();
    }
    let query = trimmed.to_lowercase();
    let mut scored: Vec<_> = rows
        .iter()
        .filter_map(|row| {
            let title = row.title.to_lowercase();
            let subtitle = row.subtitle.to_lowercase();
            let path = row.path.display().to_string().to_lowercase();
            best_score(&query, [title.as_str(), subtitle.as_str(), path.as_str()])
                .map(|score| (score, row.clone()))
        })
        .collect();
    scored.sort_by(|(left_score, left), (right_score, right)| {
        left_score
            .cmp(right_score)
            .then_with(|| right.last_opened_secs.cmp(&left.last_opened_secs))
    });
    scored.into_iter().map(|(_, row)| row).collect()
}

/// Return a local path from a service-level target, rejecting unsupported URIs.
#[must_use]
pub fn local_path_from_target(target: RecentOpenTarget) -> Option<PathBuf> {
    match target {
        RecentOpenTarget::Local(path) => Some(path),
        RecentOpenTarget::UnsupportedUri(_) => None,
    }
}

/// Current wall-clock timestamp in seconds since the Unix epoch.
#[must_use]
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load_recent_file(path: &Path, diagnostics: &mut Vec<String>) -> (RecentDocumentFile, bool) {
    match fs_metadata::path_status(path) {
        Ok(PathStatus::Missing) => return (RecentDocumentFile::default(), false),
        Ok(PathStatus::File) => {}
        Ok(status) => {
            push_diagnostic(
                diagnostics,
                format!(
                    "recent documents ignored {}: unsupported status {status:?}",
                    path.display()
                ),
            );
            return (RecentDocumentFile::default(), false);
        }
        Err(error) => {
            push_diagnostic(
                diagnostics,
                format!("recent documents ignored {}: {error}", path.display()),
            );
            return (RecentDocumentFile::default(), false);
        }
    }

    match fs_metadata::file_facts(path) {
        Ok(facts) if facts.byte_size > MAX_RECENT_DOCUMENTS_BYTES => {
            push_diagnostic(
                diagnostics,
                format!(
                    "recent documents reset: {} is {} bytes, above the {} byte cap",
                    path.display(),
                    facts.byte_size,
                    MAX_RECENT_DOCUMENTS_BYTES
                ),
            );
            return (RecentDocumentFile::default(), true);
        }
        Ok(_) => {}
        Err(error) => {
            push_diagnostic(
                diagnostics,
                format!("recent documents ignored {}: {error}", path.display()),
            );
            return (RecentDocumentFile::default(), false);
        }
    }

    let bytes = match fs_read::bytes(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (RecentDocumentFile::default(), false);
        }
        Err(error) => {
            push_diagnostic(
                diagnostics,
                format!("recent documents ignored {}: {error}", path.display()),
            );
            return (RecentDocumentFile::default(), false);
        }
    };

    match serde_json::from_slice::<RecentDocumentFile>(&bytes) {
        Ok(file) => (file, false),
        Err(error) => {
            push_diagnostic(
                diagnostics,
                format!(
                    "recent documents reset: failed to parse {}: {error}",
                    path.display()
                ),
            );
            (RecentDocumentFile::default(), true)
        }
    }
}

fn dedupe_sort_prune_existing(
    mut entries: Vec<RecentDocumentEntry>,
    diagnostics: &mut Vec<String>,
) -> (Vec<RecentDocumentEntry>, bool) {
    let mut pruned = false;
    entries.sort_by(sort_newest_first);
    if entries.len() > MAX_RECENT_LOAD_CANDIDATES {
        pruned = true;
        push_diagnostic(
            diagnostics,
            format!(
                "recent documents truncated before pruning: {} rows exceeded the {} row load cap",
                entries.len(),
                MAX_RECENT_LOAD_CANDIDATES
            ),
        );
        entries.truncate(MAX_RECENT_LOAD_CANDIDATES);
    }

    if entries.len() > MAX_RECENTS {
        pruned = true;
    }

    let mut retained = Vec::new();
    for entry in entries {
        match fs_metadata::path_status(&entry.path) {
            Ok(PathStatus::File) => {
                if retained
                    .iter()
                    .any(|retained_entry| same_identity(retained_entry, &entry))
                {
                    // Entries are sorted newest-first; skipping later duplicates
                    // preserves the freshest spelling and timestamp for a path.
                    pruned = true;
                    continue;
                }
                add_or_update(
                    &mut retained,
                    entry.path,
                    entry.canonical_path,
                    entry.last_opened_secs,
                );
            }
            Ok(status) => {
                pruned = true;
                push_diagnostic(
                    diagnostics,
                    format!(
                        "recent document pruned {}: unsupported status {status:?}",
                        entry.path.display()
                    ),
                );
            }
            Err(error) => {
                pruned = true;
                push_diagnostic(
                    diagnostics,
                    format!("recent document pruned {}: {error}", entry.path.display()),
                );
            }
        }
    }
    retained.sort_by(sort_newest_first);
    retained.truncate(MAX_RECENTS);
    (retained, pruned)
}

fn entry_is_open(entry: &RecentDocumentEntry, open_identities: &HashSet<PathBuf>) -> bool {
    open_identities.contains(&entry.path)
        || entry
            .canonical_path
            .as_ref()
            .is_some_and(|path| open_identities.contains(path))
}

fn push_diagnostic(diagnostics: &mut Vec<String>, message: String) {
    if diagnostics.len() < MAX_RECENT_LOAD_DIAGNOSTICS {
        diagnostics.push(message);
    } else if diagnostics.len() == MAX_RECENT_LOAD_DIAGNOSTICS {
        diagnostics.push("recent document diagnostics truncated".to_string());
    }
}

fn sort_newest_first(left: &RecentDocumentEntry, right: &RecentDocumentEntry) -> Ordering {
    right
        .last_opened_secs
        .cmp(&left.last_opened_secs)
        .then_with(|| left.path.cmp(&right.path))
}

fn same_identity(left: &RecentDocumentEntry, right: &RecentDocumentEntry) -> bool {
    left.path == right.path
        || left
            .canonical_path
            .as_ref()
            .is_some_and(|path| path == &right.path || right.canonical_path.as_ref() == Some(path))
        || right
            .canonical_path
            .as_ref()
            .is_some_and(|path| path == &left.path)
}

fn best_score<'a>(query: &str, haystacks: impl IntoIterator<Item = &'a str>) -> Option<u8> {
    let mut best = None;
    for haystack in haystacks {
        let score = if haystack.starts_with(query) {
            Some(0)
        } else if haystack.contains(query) {
            Some(1)
        } else if fuzzy_match(haystack, query) {
            Some(2)
        } else {
            None
        };
        best = match (best, score) {
            (None, Some(score)) => Some(score),
            (Some(current), Some(score)) => Some(current.min(score)),
            (best, None) => best,
        };
    }
    best
}

fn fuzzy_match(haystack: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut chars = query.chars();
    let Some(mut expected) = chars.next() else {
        return true;
    };
    for candidate in haystack.chars() {
        if candidate == expected {
            let Some(next) = chars.next() else {
                return true;
            };
            expected = next;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn entry(path: &str, secs: u64) -> RecentDocumentEntry {
        RecentDocumentEntry::new(PathBuf::from(path), None, secs)
    }

    #[test]
    fn add_update_deduplicates_and_orders_newest_first() {
        let mut entries = vec![entry("/tmp/old.txt", 10), entry("/tmp/new.txt", 20)];

        add_or_update(&mut entries, PathBuf::from("/tmp/old.txt"), None, 30);

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.path.as_path())
                .collect::<Vec<_>>(),
            vec![Path::new("/tmp/old.txt"), Path::new("/tmp/new.txt")]
        );
        assert_eq!(entries[0].last_opened_secs, 30);
    }

    #[test]
    fn add_update_deduplicates_canonical_identity() {
        let mut entries = vec![RecentDocumentEntry::new(
            PathBuf::from("/tmp/link.txt"),
            Some(PathBuf::from("/real/file.txt")),
            10,
        )];

        add_or_update(
            &mut entries,
            PathBuf::from("/other/spelling.txt"),
            Some(PathBuf::from("/real/file.txt")),
            50,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("/other/spelling.txt"));
    }

    #[test]
    fn remove_matches_display_or_canonical_path() {
        let mut entries = vec![RecentDocumentEntry::new(
            PathBuf::from("/tmp/link.txt"),
            Some(PathBuf::from("/real/file.txt")),
            1,
        )];

        remove(&mut entries, Path::new("/real/file.txt"));

        assert!(entries.is_empty());
    }

    #[test]
    fn load_save_roundtrip_preserves_privacy_safe_fields() {
        let dir = TempDir::new().expect("recent tempdir");
        let path = dir.path().join("file.txt");
        fixture::write_text(&path, "content");
        let entries = vec![RecentDocumentEntry::new(path.clone(), Some(path), 123)];

        save(dir.path(), &entries).expect("save recents");
        let loaded = load(dir.path());

        assert!(loaded.diagnostics.is_empty());
        assert_eq!(loaded.entries, entries);
        let raw = fixture::read_text(&dir.path().join(RECENT_DOCUMENTS_FILE));
        assert!(!raw.contains("content"));
        assert!(!raw.contains("draft"));
    }

    #[test]
    fn load_recovers_corrupt_json_as_empty() {
        let dir = TempDir::new().expect("recent tempdir");
        fixture::write_text(&dir.path().join(RECENT_DOCUMENTS_FILE), "{ not-json");

        let loaded = load(dir.path());

        assert!(loaded.entries.is_empty());
        assert!(loaded.pruned);
        assert!(!loaded.diagnostics.is_empty());
    }

    #[test]
    fn load_rejects_oversized_recent_file_before_parse() {
        let dir = TempDir::new().expect("recent tempdir");
        fixture::write_repeated_bytes(
            &dir.path().join(RECENT_DOCUMENTS_FILE),
            b"x",
            MAX_RECENT_DOCUMENTS_BYTES + 1,
        );

        let loaded = load(dir.path());

        assert!(loaded.entries.is_empty());
        assert!(loaded.pruned);
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("above the"))
        );
    }

    #[test]
    fn load_bounds_external_rows_and_diagnostics_before_pruning() {
        let dir = TempDir::new().expect("recent tempdir");
        let entries = (0..(MAX_RECENT_LOAD_CANDIDATES + 5))
            .map(|idx| {
                RecentDocumentEntry::new(
                    dir.path().join(format!("missing-{idx}.txt")),
                    None,
                    u64::try_from(MAX_RECENT_LOAD_CANDIDATES + 5 - idx)
                        .expect("test timestamp fits u64"),
                )
            })
            .collect::<Vec<_>>();
        json_store::save(
            dir.path(),
            RECENT_DOCUMENTS_FILE,
            &RecentDocumentFile { entries },
        )
        .expect("seed oversized recent list");

        let loaded = load(dir.path());

        assert!(loaded.entries.is_empty());
        assert!(loaded.pruned);
        assert!(loaded.diagnostics.len() <= MAX_RECENT_LOAD_DIAGNOSTICS + 1);
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.contains("truncated before pruning") })
        );
        assert_eq!(
            loaded.diagnostics.last().map(String::as_str),
            Some("recent document diagnostics truncated")
        );
    }

    #[test]
    fn load_prunes_missing_and_unsupported_paths() {
        let dir = TempDir::new().expect("recent tempdir");
        let file = dir.path().join("present.txt");
        let missing = dir.path().join("missing.txt");
        let folder = dir.path().join("folder");
        fixture::write_text(&file, "present");
        fixture::create_dir(&folder);
        let entries = RecentDocumentFile {
            entries: vec![
                RecentDocumentEntry::new(file.clone(), None, 30),
                RecentDocumentEntry::new(missing, None, 20),
                RecentDocumentEntry::new(folder, None, 10),
            ],
        };
        json_store::save(dir.path(), RECENT_DOCUMENTS_FILE, &entries).expect("seed recents");

        let loaded = load(dir.path());

        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].path, file);
        assert!(loaded.pruned);
        assert_eq!(loaded.diagnostics.len(), 2);
    }

    #[test]
    fn load_deduplicates_duplicate_persisted_path_spellings() {
        let dir = TempDir::new().expect("recent duplicate tempdir");
        let path = dir.path().join("duplicate.txt");
        fixture::write_text(&path, "duplicate");
        let entries = RecentDocumentFile {
            entries: vec![
                RecentDocumentEntry::new(path.clone(), None, 10),
                RecentDocumentEntry::new(path.clone(), None, 30),
            ],
        };
        json_store::save(dir.path(), RECENT_DOCUMENTS_FILE, &entries)
            .expect("seed duplicate recents");

        let loaded = load(dir.path());

        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].path, path);
        assert_eq!(loaded.entries[0].last_opened_secs, 30);
        assert!(loaded.pruned);
    }

    #[test]
    fn visible_rows_excludes_open_tabs_by_display_or_canonical_identity() {
        let entries = vec![
            RecentDocumentEntry::new(PathBuf::from("/tmp/a.txt"), None, 30),
            RecentDocumentEntry::new(
                PathBuf::from("/tmp/link.txt"),
                Some(PathBuf::from("/real/b.txt")),
                20,
            ),
            RecentDocumentEntry::new(PathBuf::from("/tmp/c.txt"), None, 10),
        ];

        let rows = visible_rows(
            &entries,
            &[PathBuf::from("/tmp/a.txt"), PathBuf::from("/real/b.txt")],
            40,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, PathBuf::from("/tmp/c.txt"));
    }

    #[test]
    fn visible_rows_reappears_after_live_identity_is_removed() {
        let entry = RecentDocumentEntry::new(PathBuf::from("/tmp/reopened.txt"), None, 30);
        let entries = vec![entry];

        let open_rows = visible_rows(&entries, &[PathBuf::from("/tmp/reopened.txt")], 40);
        let closed_rows = visible_rows(&entries, &[], 50);

        assert!(open_rows.is_empty());
        assert_eq!(closed_rows.len(), 1);
        assert_eq!(closed_rows[0].path, PathBuf::from("/tmp/reopened.txt"));
    }

    #[test]
    fn visible_rows_keeps_display_and_canonical_entries_when_live_set_is_empty() {
        let entries = vec![
            RecentDocumentEntry::new(
                PathBuf::from("/tmp/stale-display.txt"),
                Some(PathBuf::from("/real/stale-display.txt")),
                20,
            ),
            RecentDocumentEntry::new(PathBuf::from("/tmp/plain.txt"), None, 10),
        ];

        let rows = visible_rows(&entries, &[], 30);

        assert_eq!(
            rows.iter()
                .map(|row| row.path.as_path())
                .collect::<Vec<_>>(),
            vec![
                Path::new("/tmp/stale-display.txt"),
                Path::new("/tmp/plain.txt")
            ]
        );
    }

    #[test]
    fn visible_rows_handles_mixed_open_and_closed_identity_sets() {
        let entries = vec![
            RecentDocumentEntry::new(PathBuf::from("/tmp/open-display.txt"), None, 40),
            RecentDocumentEntry::new(
                PathBuf::from("/tmp/open-link.txt"),
                Some(PathBuf::from("/real/open-target.txt")),
                30,
            ),
            RecentDocumentEntry::new(
                PathBuf::from("/tmp/closed-link.txt"),
                Some(PathBuf::from("/real/closed-target.txt")),
                20,
            ),
            RecentDocumentEntry::new(PathBuf::from("/tmp/closed-display.txt"), None, 10),
        ];

        let rows = visible_rows(
            &entries,
            &[
                PathBuf::from("/tmp/open-display.txt"),
                PathBuf::from("/real/open-target.txt"),
            ],
            50,
        );

        assert_eq!(
            rows.iter()
                .map(|row| row.path.as_path())
                .collect::<Vec<_>>(),
            vec![
                Path::new("/tmp/closed-link.txt"),
                Path::new("/tmp/closed-display.txt")
            ]
        );
    }

    #[test]
    fn merge_loaded_entries_preserves_current_mutations_by_identity() {
        let mut current = vec![RecentDocumentEntry::new(
            PathBuf::from("/tmp/current-spelling.txt"),
            Some(PathBuf::from("/real/shared.txt")),
            50,
        )];
        let loaded = vec![
            RecentDocumentEntry::new(
                PathBuf::from("/tmp/loaded-spelling.txt"),
                Some(PathBuf::from("/real/shared.txt")),
                10,
            ),
            entry("/tmp/other.txt", 30),
        ];

        assert!(merge_loaded_entries(&mut current, loaded));

        assert_eq!(current.len(), 2);
        assert_eq!(current[0].path, PathBuf::from("/tmp/current-spelling.txt"));
        assert_eq!(current[1].path, PathBuf::from("/tmp/other.txt"));
    }

    #[test]
    fn search_ranks_prefix_substring_and_fuzzy_matches() {
        let rows = vec![
            RecentDocumentRow::from_entry(&entry("/tmp/work/src/main.rs", 30), 30),
            RecentDocumentRow::from_entry(&entry("/tmp/work/domain.rs", 20), 30),
            RecentDocumentRow::from_entry(&entry("/tmp/work/notes.txt", 10), 30),
        ];

        let results = search_rows(&rows, "dom");
        assert_eq!(results[0].title, "domain.rs");

        let results = search_rows(&rows, "sr");
        assert_eq!(results[0].title, "main.rs");

        let results = search_rows(&rows, "absent");
        assert!(results.is_empty());
    }

    #[test]
    fn unsupported_non_local_target_returns_none() {
        let target = RecentOpenTarget::unsupported_uri("sftp://example.com/file.txt");

        assert!(local_path_from_target(target).is_none());
    }
}

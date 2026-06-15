// SPDX-License-Identifier: GPL-3.0-or-later

//! App-owned recent-document values for the GNOME-style Open popover.
//!
//! These types intentionally store only local path metadata and timestamps.
//! Document text, draft IDs, notes, and local-history identifiers stay out of
//! the recent-document file so the feature remains privacy-bounded.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persisted recent-document collection.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentDocumentFile {
    /// Newest-first persisted rows.
    pub entries: Vec<RecentDocumentEntry>,
}

/// One local file-backed document in recent history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentDocumentEntry {
    /// Path spelling LushText should reopen when the row is activated.
    pub path: PathBuf,
    /// Canonical filesystem identity captured near a successful load when
    /// available. Used only for dedupe and open-tab exclusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_path: Option<PathBuf>,
    /// Last successful explicit open time as seconds since the Unix epoch.
    pub last_opened_secs: u64,
}

impl RecentDocumentEntry {
    /// Create a recent document entry from a local file path.
    #[must_use]
    pub fn new(path: PathBuf, canonical_path: Option<PathBuf>, last_opened_secs: u64) -> Self {
        Self {
            path,
            canonical_path,
            last_opened_secs,
        }
    }

    /// Return true when either stored identity matches `path`.
    #[must_use]
    pub fn matches_path(&self, path: &Path) -> bool {
        self.path == path || self.canonical_path.as_deref() == Some(path)
    }
}

/// UI-ready row derived from recent persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentDocumentRow {
    /// Primary row title, normally the file name.
    pub title: String,
    /// Secondary location/context text.
    pub subtitle: String,
    /// Optional short age/context text for compact row disambiguation.
    pub age_label: Option<String>,
    /// Path opened when the row activates.
    pub path: PathBuf,
    /// Canonical filesystem identity used by the visible-list filter.
    pub canonical_path: Option<PathBuf>,
    /// Ordering metadata retained for tests and stable refreshes.
    pub last_opened_secs: u64,
}

impl RecentDocumentRow {
    /// Build display metadata from a persisted entry.
    #[must_use]
    pub fn from_entry(entry: &RecentDocumentEntry, now_secs: u64) -> Self {
        let title = entry
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| entry.path.display().to_string(), ToOwned::to_owned);
        let subtitle = entry
            .path
            .parent()
            .map_or_else(String::new, |parent| parent.display().to_string());
        let age_label = Some(age_label(entry.last_opened_secs, now_secs));
        Self {
            title,
            subtitle,
            age_label,
            path: entry.path.clone(),
            canonical_path: entry.canonical_path.clone(),
            last_opened_secs: entry.last_opened_secs,
        }
    }

    /// Return true when this row represents an open identity.
    #[must_use]
    pub fn matches_any_identity(&self, identities: &[PathBuf]) -> bool {
        identities.iter().any(|identity| {
            self.path == *identity || self.canonical_path.as_ref() == Some(identity)
        })
    }
}

fn age_label(last_opened_secs: u64, now_secs: u64) -> String {
    let elapsed = now_secs.saturating_sub(last_opened_secs);
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    if elapsed < MINUTE {
        "Now".to_string()
    } else if elapsed < HOUR {
        format!("{}m", elapsed / MINUTE)
    } else if elapsed < DAY {
        format!("{}h", elapsed / HOUR)
    } else {
        format!("{}d", elapsed / DAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_display_uses_file_name_parent_and_age() {
        let entry = RecentDocumentEntry::new(PathBuf::from("/tmp/project/main.rs"), None, 100);

        let row = RecentDocumentRow::from_entry(&entry, 100 + 3_600);

        assert_eq!(row.title, "main.rs");
        assert_eq!(row.subtitle, "/tmp/project");
        assert_eq!(row.age_label.as_deref(), Some("1h"));
    }

    #[test]
    fn identity_matches_display_or_canonical_path() {
        let entry = RecentDocumentEntry::new(
            PathBuf::from("/tmp/link.txt"),
            Some(PathBuf::from("/real/file.txt")),
            1,
        );

        assert!(entry.matches_path(Path::new("/tmp/link.txt")));
        assert!(entry.matches_path(Path::new("/real/file.txt")));
        assert!(!entry.matches_path(Path::new("/other.txt")));
    }
}

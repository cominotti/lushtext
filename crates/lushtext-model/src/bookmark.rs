// SPDX-License-Identifier: GPL-3.0-or-later

//! Bookmark sidecar model types.
//!
//! Bookmarks are persisted as lightweight point-in-file records. This module
//! stays GTK-free so persistence tests and export helpers can reason about
//! bookmark state without constructing live `GtkSourceMark` projections.

use serde::{Deserialize, Serialize};

use super::sidecar_identity::{DocumentSidecarIdentity, next_record_id, now_epoch_secs};

/// Stable identifier for one persisted bookmark entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct BookmarkId(pub String);

impl BookmarkId {
    /// Generate a fresh bookmark ID.
    #[must_use]
    pub fn new() -> Self {
        Self(next_record_id("bookmark"))
    }
}

impl Default for BookmarkId {
    fn default() -> Self {
        Self::new()
    }
}

/// One bookmark anchored to a zero-based line number in a file-backed document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookmarkRecord {
    /// Stable bookmark identity used by the live editor projection.
    pub id: BookmarkId,
    /// Zero-based line number tracked across sessions.
    pub line: u32,
    /// Optional user-facing label shown in bookmark lists and tooltips.
    pub label: Option<String>,
    /// Creation timestamp (seconds since epoch).
    pub created_at_secs: u64,
    /// Last mutation timestamp (seconds since epoch).
    pub updated_at_secs: u64,
}

impl BookmarkRecord {
    /// Create a new bookmark anchored to `line`.
    #[must_use]
    pub fn new(line: u32, label: Option<String>) -> Self {
        let now = now_epoch_secs();
        Self {
            id: BookmarkId::new(),
            line,
            label: normalize_label(label),
            created_at_secs: now,
            updated_at_secs: now,
        }
    }

    /// Update the stored label and mutation timestamp.
    pub fn set_label(&mut self, label: Option<String>) {
        self.label = normalize_label(label);
        self.updated_at_secs = now_epoch_secs();
    }

    /// Move the bookmark to a new line if the live editor projection shifted it.
    ///
    /// Returns `true` when the persisted line changed.
    pub fn move_to_line(&mut self, line: u32) -> bool {
        if self.line == line {
            return false;
        }

        self.line = line;
        self.updated_at_secs = now_epoch_secs();
        true
    }

    /// Human-friendly fallback label used by list rows and export flows.
    #[must_use]
    pub fn display_label(&self) -> String {
        self.label
            .clone()
            .unwrap_or_else(|| format!("Line {}", self.line.saturating_add(1)))
    }
}

/// Persisted bookmark collection for one file-backed document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookmarkDocument {
    /// Stable file identity backing this sidecar document.
    pub identity: DocumentSidecarIdentity,
    /// All bookmarks stored for the file.
    pub bookmarks: Vec<BookmarkRecord>,
}

impl BookmarkDocument {
    /// Create an empty bookmark document for a resolved file identity.
    #[must_use]
    pub fn empty(identity: DocumentSidecarIdentity) -> Self {
        Self {
            identity,
            bookmarks: Vec::new(),
        }
    }

    /// Sort bookmarks into deterministic line/id order before saving or export.
    pub fn sort_stable(&mut self) {
        self.bookmarks.sort_by(|left, right| {
            left.line
                .cmp(&right.line)
                .then_with(|| left.id.0.cmp(&right.id.0))
        });
    }

    /// Return the complete retained heap graph used during bounded source construction.
    #[must_use]
    pub fn retained_heap_byte_weight(&self) -> u64 {
        let records = self.bookmarks.iter().fold(0u64, |total, bookmark| {
            total
                .saturating_add(u64::try_from(bookmark.id.0.capacity()).unwrap_or(u64::MAX))
                .saturating_add(bookmark.label.as_ref().map_or(0, |label| {
                    u64::try_from(label.capacity()).unwrap_or(u64::MAX)
                }))
        });
        self.identity
            .retained_heap_byte_weight()
            .saturating_add(
                u64::try_from(
                    self.bookmarks
                        .capacity()
                        .saturating_mul(std::mem::size_of::<BookmarkRecord>()),
                )
                .unwrap_or(u64::MAX),
            )
            .saturating_add(records)
    }
}

fn normalize_label(label: Option<String>) -> Option<String> {
    label.and_then(|label| {
        let trimmed = label.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_identity() -> DocumentSidecarIdentity {
        DocumentSidecarIdentity::from_paths(
            PathBuf::from("/tmp/file.rs"),
            PathBuf::from("/tmp/file.rs"),
        )
    }

    #[test]
    fn empty_document_starts_without_bookmarks() {
        let document = BookmarkDocument::empty(sample_identity());
        assert!(document.bookmarks.is_empty());
    }

    #[test]
    fn new_bookmark_uses_line_fallback_label() {
        let bookmark = BookmarkRecord::new(4, None);
        assert_eq!(bookmark.display_label(), "Line 5");
    }

    #[test]
    fn set_label_trims_whitespace() {
        let mut bookmark = BookmarkRecord::new(0, None);
        bookmark.set_label(Some("  Important  ".to_string()));
        assert_eq!(bookmark.label.as_deref(), Some("Important"));
    }

    #[test]
    fn move_to_line_reports_real_changes() {
        let mut bookmark = BookmarkRecord::new(2, None);
        assert!(!bookmark.move_to_line(2));
        assert!(bookmark.move_to_line(6));
        assert_eq!(bookmark.line, 6);
    }
}

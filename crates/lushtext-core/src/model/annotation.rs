// SPDX-License-Identifier: GPL-3.0-or-later

//! Annotation sidecar model types.
//!
//! Annotations capture a persisted note, presentation style, and line range for
//! a saved file without mutating the source bytes. The live editor keeps range
//! anchors; this module keeps the serialized shape framework-free.

use serde::{Deserialize, Serialize};

use super::sidecar_identity::{DocumentSidecarIdentity, next_record_id, now_epoch_secs};

/// Stable identifier for one persisted annotation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AnnotationId(pub String);

impl AnnotationId {
    /// Generate a fresh annotation ID.
    #[must_use]
    pub fn new() -> Self {
        Self(next_record_id("annotation"))
    }
}

impl Default for AnnotationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Presentation style used for the first-release annotation highlight and list UI.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum AnnotationStyle {
    #[default]
    Note,
    Todo,
    Warning,
    Question,
}

impl AnnotationStyle {
    /// Human-friendly label shown in dialogs, list rows, and export output.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Todo => "Todo",
            Self::Warning => "Warning",
            Self::Question => "Question",
        }
    }
}

/// One persisted annotation anchored to an inclusive line range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationRecord {
    /// Stable annotation identity used by live range anchors and list workflows.
    pub id: AnnotationId,
    /// First zero-based line covered by the annotation.
    pub start_line: u32,
    /// Last zero-based line covered by the annotation.
    pub end_line: u32,
    /// User-authored note body stored outside the source file.
    pub note_text: String,
    /// Visual treatment used by first-release highlights and exports.
    pub style: AnnotationStyle,
    /// Creation timestamp (seconds since epoch).
    pub created_at_secs: u64,
    /// Last mutation timestamp (seconds since epoch).
    pub updated_at_secs: u64,
}

impl AnnotationRecord {
    /// Create a new annotation for an inclusive line range.
    #[must_use]
    pub fn new(start_line: u32, end_line: u32, note_text: String, style: AnnotationStyle) -> Self {
        let now = now_epoch_secs();
        Self {
            id: AnnotationId::new(),
            start_line,
            end_line: end_line.max(start_line),
            note_text: normalize_note_text(note_text),
            style,
            created_at_secs: now,
            updated_at_secs: now,
        }
    }

    /// Update the stored note body and style in one command-shaped mutation.
    pub fn update_content(&mut self, note_text: String, style: AnnotationStyle) {
        self.note_text = normalize_note_text(note_text);
        self.style = style;
        self.updated_at_secs = now_epoch_secs();
    }

    /// Move the annotation to a new inclusive line range after editor edits.
    ///
    /// Returns `true` when the persisted range changed.
    pub fn move_to_range(&mut self, start_line: u32, end_line: u32) -> bool {
        let end_line = end_line.max(start_line);
        if self.start_line == start_line && self.end_line == end_line {
            return false;
        }

        self.start_line = start_line;
        self.end_line = end_line;
        self.updated_at_secs = now_epoch_secs();
        true
    }

    /// Display the range in the 1-based form users expect in dialogs and exports.
    #[must_use]
    pub fn line_range_label(&self) -> String {
        let start = self.start_line.saturating_add(1);
        let end = self.end_line.saturating_add(1);
        if start == end {
            format!("Line {start}")
        } else {
            format!("Lines {start}-{end}")
        }
    }
}

/// Persisted annotation collection for one file-backed document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationDocument {
    /// Stable file identity backing this sidecar document.
    pub identity: DocumentSidecarIdentity,
    /// All annotations stored for the file.
    pub annotations: Vec<AnnotationRecord>,
}

impl AnnotationDocument {
    /// Create an empty annotation document for a resolved file identity.
    #[must_use]
    pub fn empty(identity: DocumentSidecarIdentity) -> Self {
        Self {
            identity,
            annotations: Vec::new(),
        }
    }

    /// Sort annotations into deterministic file-order before saving or export.
    pub fn sort_stable(&mut self) {
        self.annotations.sort_by(|left, right| {
            left.start_line
                .cmp(&right.start_line)
                .then_with(|| left.end_line.cmp(&right.end_line))
                .then_with(|| left.id.0.cmp(&right.id.0))
        });
    }
}

fn normalize_note_text(note_text: String) -> String {
    note_text.trim().to_string()
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
    fn empty_document_starts_without_annotations() {
        let document = AnnotationDocument::empty(sample_identity());
        assert!(document.annotations.is_empty());
    }

    #[test]
    fn new_annotation_clamps_end_line() {
        let annotation = AnnotationRecord::new(8, 4, "note".to_string(), AnnotationStyle::Note);
        assert_eq!(annotation.start_line, 8);
        assert_eq!(annotation.end_line, 8);
    }

    #[test]
    fn update_content_trims_note_text() {
        let mut annotation = AnnotationRecord::new(0, 0, "x".to_string(), AnnotationStyle::Note);
        annotation.update_content("  refined note  ".to_string(), AnnotationStyle::Todo);
        assert_eq!(annotation.note_text, "refined note");
        assert_eq!(annotation.style, AnnotationStyle::Todo);
    }

    #[test]
    fn line_range_label_uses_one_based_lines() {
        let annotation = AnnotationRecord::new(3, 5, "note".to_string(), AnnotationStyle::Note);
        assert_eq!(annotation.line_range_label(), "Lines 4-6");
    }
}

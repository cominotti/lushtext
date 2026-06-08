// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared note-body primitives for range, document, and folder notes.
//!
//! The app stores notes in a few different scopes, but they all need the same
//! core behavior: normalized UTF-8 text, stable timestamps, a short preview
//! string for browse rows, and a simple edit-versus-render mode vocabulary that
//! stays GTK-free in the model layer.

use serde::{Deserialize, Serialize};

use super::sidecar_identity::now_epoch_secs;

/// Lightweight edit/render mode contract shared by note surfaces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NoteViewMode {
    /// Editable text mode backed by a normal text buffer.
    #[default]
    Edit,
    /// Read-only rendered markdown mode backed by the preview widget.
    Render,
}

/// Shared persisted note body used by document and folder notes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RichNoteBody {
    /// User-authored UTF-8 note text stored outside user source files.
    pub text: String,
    /// Creation timestamp (seconds since epoch).
    pub created_at_secs: u64,
    /// Last mutation timestamp (seconds since epoch).
    pub updated_at_secs: u64,
}

impl RichNoteBody {
    /// Create one persisted note body from user-authored text.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let now = now_epoch_secs();
        Self {
            text: normalize_note_text(text),
            created_at_secs: now,
            updated_at_secs: now,
        }
    }

    /// Update the stored note text and timestamp in one command-shaped mutation.
    ///
    /// Returns `true` when the normalized text actually changed.
    pub fn update_text(&mut self, text: &str) -> bool {
        let normalized = normalize_note_text(text);
        if self.text == normalized {
            return false;
        }

        self.text = normalized;
        self.updated_at_secs = now_epoch_secs();
        true
    }

    /// Return whether the note body currently stores meaningful text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Human-friendly single-line preview used in list rows and compact shells.
    #[must_use]
    pub fn preview_line(&self) -> String {
        note_preview_line(&self.text)
    }
}

/// Normalize user-authored note text before persistence.
#[must_use]
pub fn normalize_note_text(text: &str) -> String {
    text.trim().to_string()
}

/// Build a short preview line from a note body without losing markdown symbols.
#[must_use]
pub fn note_preview_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_note_normalizes_text() {
        let note = RichNoteBody::new("  hello  ");
        assert_eq!(note.text, "hello");
    }

    #[test]
    fn update_text_skips_identical_normalized_body() {
        let mut note = RichNoteBody::new("hello");
        assert!(!note.update_text("  hello  "));
    }

    #[test]
    fn update_text_reports_real_changes_and_refreshes_timestamp() {
        let mut note = RichNoteBody {
            text: "hello".to_string(),
            created_at_secs: 1,
            updated_at_secs: 1,
        };

        assert!(note.update_text("  goodbye  "));
        assert_eq!(note.text, "goodbye");
        assert_eq!(note.created_at_secs, 1);
        assert!(note.updated_at_secs >= note.created_at_secs);
    }

    #[test]
    fn rich_note_empty_and_preview_use_normalized_text() {
        let empty = RichNoteBody::new("  \n\t  ");
        let note = RichNoteBody::new("\n\n  # heading  \nbody");

        assert!(empty.is_empty());
        assert_eq!(empty.preview_line(), "");
        assert!(!note.is_empty());
        assert_eq!(note.preview_line(), "# heading");
    }

    #[test]
    fn preview_line_skips_leading_blank_lines() {
        assert_eq!(note_preview_line("\n\n# heading\nbody"), "# heading");
    }
}

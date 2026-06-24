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

/// Pure note-editor presentation policy shared by GTK dialog adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteEditorPresentation {
    /// Normalized note body loaded before the editor was shown.
    loaded_text: String,
}

impl NoteEditorPresentation {
    /// Build the initial-mode and dirty-state policy from the loaded note body.
    #[must_use]
    pub fn from_loaded_text(loaded_text: Option<&str>) -> Self {
        Self {
            loaded_text: loaded_text.map(normalize_note_text).unwrap_or_default(),
        }
    }

    /// Select the first visible page for the shared Edit/Render note surface.
    #[must_use]
    pub fn initial_mode(&self) -> NoteViewMode {
        if self.loaded_text.is_empty() {
            NoteViewMode::Edit
        } else {
            NoteViewMode::Render
        }
    }

    /// Return whether Save should be actionable for the current editor buffer.
    #[must_use]
    pub fn save_enabled_for(&self, current_text: &str) -> bool {
        let current_text = current_text.trim();
        // Empty buffers are not a dirty Save state; existing notes use Clear,
        // and new notes need meaningful text before Save becomes actionable.
        !current_text.is_empty() && current_text != self.loaded_text.as_str()
    }
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

/// Merge decision between two persisted note bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RichNoteMergeDecision {
    /// The caller's note is newer and should replace the target body.
    UseSelf,
    /// The target note is newer, or both note bodies are identical.
    UseOther,
    /// Both notes changed at the same timestamp and differ in content.
    Conflict,
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

    /// Decide which note body should survive a sidecar migration merge.
    ///
    /// Document and folder notes share the same conflict policy: the newer
    /// body wins, identical bodies at the same timestamp are safe to coalesce,
    /// and different bodies with the same timestamp stay preserved as a conflict.
    #[must_use]
    pub fn merge_decision_against(&self, other: &Self) -> RichNoteMergeDecision {
        if self.updated_at_secs > other.updated_at_secs {
            RichNoteMergeDecision::UseSelf
        } else if other.updated_at_secs > self.updated_at_secs || self == other {
            RichNoteMergeDecision::UseOther
        } else {
            RichNoteMergeDecision::Conflict
        }
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
    fn rich_note_merge_decision_prefers_newer_body() {
        let older = RichNoteBody {
            text: "older".to_string(),
            created_at_secs: 1,
            updated_at_secs: 2,
        };
        let newer = RichNoteBody {
            text: "newer".to_string(),
            created_at_secs: 1,
            updated_at_secs: 3,
        };

        assert_eq!(
            newer.merge_decision_against(&older),
            RichNoteMergeDecision::UseSelf
        );
        assert_eq!(
            older.merge_decision_against(&newer),
            RichNoteMergeDecision::UseOther
        );
    }

    #[test]
    fn rich_note_merge_decision_accepts_identical_timestamp_ties() {
        let left = RichNoteBody {
            text: "same".to_string(),
            created_at_secs: 1,
            updated_at_secs: 2,
        };
        let right = left.clone();

        assert_eq!(
            left.merge_decision_against(&right),
            RichNoteMergeDecision::UseOther
        );
    }

    #[test]
    fn rich_note_merge_decision_reports_ambiguous_timestamp_ties() {
        let left = RichNoteBody {
            text: "left".to_string(),
            created_at_secs: 1,
            updated_at_secs: 2,
        };
        let right = RichNoteBody {
            text: "right".to_string(),
            created_at_secs: 1,
            updated_at_secs: 2,
        };

        assert_eq!(
            left.merge_decision_against(&right),
            RichNoteMergeDecision::Conflict
        );
    }

    #[test]
    fn preview_line_skips_leading_blank_lines() {
        assert_eq!(note_preview_line("\n\n# heading\nbody"), "# heading");
    }

    #[test]
    fn note_editor_presentation_opens_meaningful_loaded_text_in_render() {
        let presentation = NoteEditorPresentation::from_loaded_text(Some("  # heading\nbody  "));

        assert_eq!(presentation.initial_mode(), NoteViewMode::Render);
        assert!(!presentation.save_enabled_for("# heading\nbody"));
    }

    #[test]
    fn note_editor_presentation_opens_missing_empty_and_whitespace_text_in_edit() {
        for loaded_text in [None, Some(""), Some("  \n\t  ")] {
            let presentation = NoteEditorPresentation::from_loaded_text(loaded_text);

            assert_eq!(presentation.initial_mode(), NoteViewMode::Edit);
            assert!(!presentation.save_enabled_for(""));
            assert!(!presentation.save_enabled_for("  \n\t  "));
        }
    }

    #[test]
    fn note_editor_presentation_enables_save_for_meaningful_changes() {
        let presentation = NoteEditorPresentation::from_loaded_text(Some("Original"));

        assert!(presentation.save_enabled_for("Changed"));
        assert!(presentation.save_enabled_for("  Changed  "));
    }

    #[test]
    fn note_editor_presentation_disables_save_for_trim_only_changes_and_reverts() {
        let presentation = NoteEditorPresentation::from_loaded_text(Some("  Original  "));

        assert!(!presentation.save_enabled_for("Original"));
        assert!(!presentation.save_enabled_for("\nOriginal\t"));
        assert!(!presentation.save_enabled_for("  \n\t  "));
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for the pure note-dialog presentation policy.
//!
//! The generated cases keep GTK out of the loop and prove the normalized dirty
//! rule that both document-note and folder-note dialogs rely on.

use lushtext_core::model::note::{NoteEditorPresentation, NoteViewMode, normalize_note_text};
use proptest::prelude::*;

use crate::support;

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn initial_mode_tracks_normalized_loaded_text(loaded in note_text()) {
        let presentation = NoteEditorPresentation::from_loaded_text(Some(&loaded));
        let expected = if normalize_note_text(&loaded).is_empty() {
            NoteViewMode::Edit
        } else {
            NoteViewMode::Render
        };

        prop_assert_eq!(presentation.initial_mode(), expected);
    }

    #[test]
    fn save_enabled_tracks_normalized_dirty_state(
        loaded in prop::option::of(note_text()),
        current in note_text(),
    ) {
        let loaded_normalized = loaded.as_deref().map(normalize_note_text).unwrap_or_default();
        let current_normalized = normalize_note_text(&current);
        let presentation = NoteEditorPresentation::from_loaded_text(loaded.as_deref());
        let expected = !current_normalized.is_empty() && current_normalized != loaded_normalized;

        prop_assert_eq!(presentation.save_enabled_for(&current), expected);
    }
}

/// Generate bounded note text with enough whitespace to exercise normalization.
fn note_text() -> impl Strategy<Value = String> {
    prop::collection::vec(note_char(), 0..=support::MAX_TEXT_FRAGMENT_CHARS * 2)
        .prop_map(|chars| chars.into_iter().collect())
}

/// Generate one simple note character, including whitespace and Markdown markers.
fn note_char() -> impl Strategy<Value = char> {
    // Keep the alphabet small for readable shrinking while still covering
    // whitespace and Markdown punctuation that affect note normalization.
    (0u8..=45).prop_map(|code| match code {
        0..=25 => char::from(b'a' + code),
        26..=35 => char::from(b'0' + (code - 26)),
        36 => ' ',
        37 => '\n',
        38 => '\t',
        39 => '\r',
        40 => '#',
        41 => '*',
        42 => '-',
        43 => '_',
        44 => '.',
        _ => '/',
    })
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Accessibility projection for one editor tab.
//!
//! The editor's user-visible state is spread across file loading, save
//! interactivity, large-file gates, and preview presentation. Keeping the
//! projection here gives assistive technologies one coherent `GtkSourceView`
//! contract without making each workflow hand-roll GTK accessible metadata.

use gtk4::prelude::*;

use crate::ui::accessibility as a11y;

use super::{EditorLoadState, LushtextEditorPage};

/// Maximum file-name characters exposed as the editor identity.
///
/// Kind: text cap; 96 rounded up to a power of two (`power-of-two-thresholds`
/// stage 4). It bounds a user file name, so D2's materiality rule decides it:
/// 64 would cut the name for 7.96 % of corpus files, 128 for 0.0020 %.
const EDITOR_IDENTITY_LIMIT: usize = 128;

impl LushtextEditorPage {
    /// Refresh accessible metadata for the source editor from the current tab state.
    pub(crate) fn refresh_accessibility_metadata(&self) {
        // Every load, replacement and save state change refreshes the
        // metadata, so held typing follows the same calls (drop targets, and
        // the settle hook in a load's terminals).
        self.sync_held_input_window();
        self.apply_editor_accessibility(false);
    }

    /// Mark the source editor as temporarily hidden by preview-only mode.
    pub(crate) fn set_preview_only_accessibility(&self, preview_only: bool) {
        self.apply_editor_accessibility(preview_only);
    }

    /// Test seam for state projections that normally happen through workflows.
    #[cfg(feature = "test-utils")]
    pub fn refresh_accessibility_metadata_for_test(&self) {
        self.refresh_accessibility_metadata();
    }

    /// Test seam for preview-only metadata without constructing a full window shell.
    #[cfg(feature = "test-utils")]
    pub fn set_preview_only_accessibility_for_test(&self, preview_only: bool) {
        self.set_preview_only_accessibility(preview_only);
    }

    fn apply_editor_accessibility(&self, preview_only: bool) {
        let view = self.source_view();
        let identity = self.accessible_document_identity();
        let description = self.editor_accessible_description(&identity, preview_only);
        let loading = self.load_state() == EditorLoadState::Loading;
        let failed = self.load_state() == EditorLoadState::Failed;
        let saving = self.is_saving();
        let too_large = !self.document_scale().open_allowed;
        let disabled = preview_only || failed || self.is_evicted() || too_large;
        let read_only = preview_only
            || loading
            || saving
            || disabled
            || !view.is_editable()
            || self.edits_locked();

        // GtkSourceView already owns GTK_ACCESSIBLE_ROLE_TEXT_BOX; setting the
        // same role again emits a GTK critical while adding no extra semantics.
        a11y::set_labelled_description(view, &format!("Editor for {identity}"), &description);
        a11y::set_multi_line(view, true);
        a11y::set_read_only(view, read_only);
        a11y::set_busy(view, loading || saving);
        a11y::set_disabled(view, disabled);
        a11y::set_invalid(view, failed);
        a11y::set_hidden(view, preview_only);
    }

    fn accessible_document_identity(&self) -> String {
        let identity = self
            .file_path()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Untitled".to_string());
        a11y::bounded_announcement_text(&identity, EDITOR_IDENTITY_LIMIT).into_owned()
    }

    fn editor_accessible_description(&self, identity: &str, preview_only: bool) -> String {
        if preview_only {
            return format!(
                "Markdown preview-only mode is active. The source editor for {identity} is hidden."
            );
        }

        let mut details = match self.load_state() {
            EditorLoadState::Untitled => {
                "Untitled multiline text editor. Changes can be saved to a file.".to_string()
            }
            EditorLoadState::Loading => {
                format!("Loading {identity}. Editing is temporarily unavailable.")
            }
            EditorLoadState::Loaded => format!("Multiline text editor for {identity}."),
            EditorLoadState::Failed => {
                format!("{identity} failed to load. Review the inline error and retry.")
            }
        };

        if self.is_saving() {
            details.push_str(" Saving is in progress; editing is temporarily read-only.");
        }

        if self.has_held_text() {
            details.push_str(" Text you type is kept and inserted when the document is ready.");
        }

        if self.is_evicted() {
            details
                .push_str(" Content has been evicted from memory and will reload when selected.");
        }

        // Protected View (`protected-view-inspector-and-surfaces`): the text
        // AT reads is the folded view, and the Line Inspector is the reading
        // path to the hidden tails.
        if self.is_protected_view() {
            details.push_str(&format!(
                " View-only: very long lines are shortened in this view. Press {} to \
                 inspect or copy a whole line.",
                super::protected_view::policy::INSPECT_LINE_SHORTCUT_LABEL
            ));
        }

        let scale = self.document_scale();
        if !scale.open_allowed {
            details.push_str(" This document is too large to open.");
        } else if let Some(reduced) = scale.reduced.describe() {
            // The scale policy's reduced features and reason classes; never
            // document content, and bounded by the fixed feature set.
            details.push(' ');
            details.push_str(&reduced);
        }

        details
    }
}

#[cfg(test)]
mod tests {
    use super::{EDITOR_IDENTITY_LIMIT, a11y};

    /// `power-of-two-thresholds` stage 4: the identity cap is a text cap over
    /// user data, so D2's materiality rule decides it. Of 1,511,259 file names
    /// in the stage-4 corpus (`~/Workspace` without `.git`, `.claude`,
    /// `target`, `node_modules`, `.cache`), 7.96 % exceed 64 characters,
    /// 0.387 % exceed 96 and 0.0020 % exceed 128; 64 would cut the file name a
    /// screen reader hears for one file in thirteen.
    #[test]
    fn the_editor_identity_keeps_a_long_file_name_whole() {
        let name = format!("BUG-0120-{}.md", "a".repeat(100));
        assert_eq!(name.chars().count(), 112);
        assert_eq!(
            a11y::bounded_announcement_text(&name, EDITOR_IDENTITY_LIMIT),
            name
        );
        let too_long = "b".repeat(EDITOR_IDENTITY_LIMIT + 1);
        assert!(a11y::bounded_announcement_text(&too_long, EDITOR_IDENTITY_LIMIT).ends_with("..."));
        assert_eq!(EDITOR_IDENTITY_LIMIT, 128);
    }
}

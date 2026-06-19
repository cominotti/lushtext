// SPDX-License-Identifier: GPL-3.0-or-later

//! Accessibility projection for one editor tab.
//!
//! The editor's user-visible state is spread across file loading, save
//! interactivity, large-file gates, and preview presentation. Keeping the
//! projection here gives assistive technologies one coherent `GtkSourceView`
//! contract without making each workflow hand-roll GTK accessible metadata.

use gtk4::prelude::*;

use crate::services::file_limits::FileSizeCheck;
use crate::ui::accessibility as a11y;

use super::{EditorLoadState, LushtextEditorPage};

/// Maximum file-name characters exposed as the editor identity.
const EDITOR_IDENTITY_LIMIT: usize = 96;

impl LushtextEditorPage {
    /// Refresh accessible metadata for the source editor from the current tab state.
    pub(crate) fn refresh_accessibility_metadata(&self) {
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
        let too_large = self.size_check() == FileSizeCheck::TooLarge;
        let disabled = preview_only || failed || self.is_evicted() || too_large;
        let read_only = preview_only || loading || saving || disabled || !view.is_editable();

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

        if self.is_evicted() {
            details
                .push_str(" Content has been evicted from memory and will reload when selected.");
        }

        match self.size_check() {
            FileSizeCheck::Normal => {}
            FileSizeCheck::LargeFileToast => {
                details.push_str(" Large document mode is active.");
            }
            FileSizeCheck::DisableSyntax => {
                details.push_str(" Syntax highlighting is disabled for this large document.");
            }
            FileSizeCheck::DisableUndoAndSyntax => {
                details.push_str(
                    " Syntax highlighting and undo history are disabled for this huge document.",
                );
            }
            FileSizeCheck::TooLarge => {
                details.push_str(" This document is too large to open.");
            }
        }

        details
    }
}

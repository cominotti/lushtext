// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared document identity and metadata for one editor tab.
//!
//! **This is not one workflow's state**, which is why it lives here rather than
//! inside a workflow's role home. The display path, the canonical path, the
//! size classification, and the language projection derived from the path are
//! written or read by the document-load workflow ([`super::load`]), the
//! document-save workflow ([`super::save`]), the rename flow in
//! `ui/window/documents.rs`, the minimap, the encoding controls, accessibility
//! metadata, and local history. Cross-cutting eligibility counts owning
//! workflows, and this group has several, so it stays in a shared
//! `ui/editor_page/` location and each workflow reaches it through these named
//! operations.
//!
//! It arrived here when slot 3b dissolved `load_save.rs`; before that it sat
//! inside the file that also held both document workflows.

use std::path::{Path, PathBuf};

use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::services::file_limits::FileSizeCheck;

use super::{EditorLoadState, LushtextEditorPage};

impl LushtextEditorPage {
    /// Set the file path (used by Save As and rename) and refresh highlighting.
    pub fn set_file_path(&self, path: &Path) {
        self.set_file_path_with_canonical(path, None);
    }

    /// Set the display path and known canonical target as one coherent identity.
    pub(crate) fn set_file_path_with_canonical(
        &self,
        path: &Path,
        canonical_path: Option<PathBuf>,
    ) {
        self.advance_local_history_path_generation();
        self.imp().file_path.replace(Some(path.to_path_buf()));
        self.imp().canonical_file_path.replace(canonical_path);
        self.imp().load_state.set(EditorLoadState::Loaded);
        self.republish_document_identity();
    }

    /// Set a provisional path before an async load result is available.
    ///
    /// The window calls this before the document-load workflow's first stage, so
    /// duplicate-tab detection and the tab title work while the read is still in
    /// flight. It lives here rather than in [`super::load`] because it is the
    /// same identity mutation as
    /// [`set_file_path_with_canonical`](Self::set_file_path_with_canonical) with
    /// a different outcome: no canonical target is known yet, the previous
    /// size and failure state no longer describe this path, and the tab reports
    /// `Loading` instead of `Loaded`. The canonical identity stays empty until
    /// the load workflow's publish stage adopts what the read actually resolved.
    pub(crate) fn set_file_path_for_pending_load(&self, path: &Path) {
        self.advance_local_history_path_generation();
        self.imp().file_path.replace(Some(path.to_path_buf()));
        self.imp().canonical_file_path.borrow_mut().take();
        self.imp().file_size.set(None);
        self.imp().load_state.set(EditorLoadState::Loading);
        self.imp().latest_load_failed.set(false);
        self.republish_document_identity();
    }

    /// Re-derive the projections that follow any document-identity change.
    ///
    /// Shared by every operation above so a new one cannot forget a projection:
    /// language detection depends on the path, and the minimap, memory policy,
    /// and accessible metadata all describe the document the tab now claims to
    /// be.
    fn republish_document_identity(&self) {
        if self.imp().size_check.get().syntax_enabled() {
            self.reapply_language();
        }
        self.schedule_minimap_refresh();
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
    }

    /// The size classification from the last file load.
    ///
    /// Written by the load workflow's publish stage and by the save workflow's
    /// accept terminal; read by the minimap, undo, syntax, and eviction paths.
    #[must_use]
    pub fn size_check(&self) -> FileSizeCheck {
        self.imp().size_check.get()
    }

    /// Detect and apply syntax language from the current file path.
    pub(crate) fn reapply_language(&self) {
        let buffer = self.buffer();
        if let Some(ref file_path) = *self.imp().file_path.borrow() {
            let lang_manager = sourceview5::LanguageManager::default();
            if let Some(language) = lang_manager.guess_language(file_path.to_str(), None::<&str>) {
                buffer.set_language(Some(&language));
            }
        }
    }
}

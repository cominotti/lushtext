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
    /// be. The content-republished listeners fire last, so the window's Markdown
    /// preview follows a language flip (Save As to `notes.md`, a rename to
    /// `.txt`); during `open_document` no listener is registered yet.
    ///
    /// A language the document scale gated off is never re-applied: a rename
    /// or Save As changes the path, not the paragraphs the gate measured.
    fn republish_document_identity(&self) {
        if self.document_scale().syntax_enabled {
            self.reapply_language();
        }
        self.schedule_minimap_refresh();
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
        self.fire_content_republished();
    }

    /// Detect and apply syntax language from the current file path.
    ///
    /// The language is a projection of the path, so a path GtkSourceView cannot
    /// classify clears it: renaming `notes.md` to `notes.txt` must stop the tab
    /// claiming to be Markdown. An untitled tab has no path and keeps whatever
    /// language it has.
    pub(crate) fn reapply_language(&self) {
        let buffer = self.buffer();
        if let Some(ref file_path) = *self.imp().file_path.borrow() {
            let lang_manager = sourceview5::LanguageManager::default();
            let language = lang_manager.guess_language(file_path.to_str(), None::<&str>);
            if buffer.language() != language {
                buffer.set_language(language.as_ref());
            }
        }
    }

    /// The id of the document's language: the buffer's own, or, while the
    /// document scale holds syntax off (the gate clears the buffer language,
    /// ledger A44), the language its path names.
    ///
    /// The gate removes highlighting, not the document's kind: a 2 MiB
    /// `notes.md` is still Markdown, and the Markdown preview must still
    /// recognise it.
    #[must_use]
    pub fn document_language_id(&self) -> Option<gtk4::glib::GString> {
        if let Some(language) = self.buffer().language() {
            return Some(language.id());
        }
        if self.document_scale().syntax_enabled {
            return None;
        }
        let path = self.imp().file_path.borrow();
        let path = path.as_ref()?;
        sourceview5::LanguageManager::default()
            .guess_language(path.to_str(), None::<&str>)
            .map(|language| language.id())
    }
}

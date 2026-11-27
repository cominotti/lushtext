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

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use gtk4::subclass::prelude::ObjectSubclassIsExt;
use lushtext_model::document::encoding::{
    DocumentEncodingState, FileHealthFinding, InvisibleCharactersMode,
};
use lushtext_model::values::{
    AccessGeneration, DraftGeneration, LossyAnalysisGeneration, PolicyGeneration,
};
use sourceview5::prelude::*;

use super::{EditorLoadState, LushtextEditorPage};
use crate::ui::buffer_snapshot::BufferSnapshotHandle;

/// Whether automatic crash recovery is paused after a snapshot exceeded policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DraftRecoveryStatus {
    #[default]
    Normal,
    AutomaticLimited,
}

/// Draft-recovery state scoped to one editor tab.
#[derive(Default)]
pub struct DraftState {
    pub draft_dirty: Cell<bool>,
    pub dirty_generation: Cell<DraftGeneration>,
    pub draft_id: RefCell<Option<String>>,
    pub draft_restored: Cell<bool>,
    pub(crate) recovery_status: Cell<DraftRecoveryStatus>,
}

/// One editor-scoped warning action routed through the shared inline alert buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingWarningAction {
    /// Open the line-ending chooser so mixed endings can be normalized.
    NormalizeLineEndings,
    /// Restore the buffer state that existed before a local-history restore.
    UndoLocalHistoryRestore,
    /// Open the local-history browser, where a stale draft's edits were kept.
    ShowLocalHistory,
    /// Open the Line Inspector (the Protected View alert).
    InspectLine,
    /// Run the Protected View entry undo (the Protected View alert).
    UndoProtectedEntry,
}

/// Whether the editor's current inline alert survives the next successful load.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InlineAlertLifetime {
    SupersededByLoad,
    #[default]
    OutlivesLoad,
}

/// Shared info-bar action and load-survival policy for one inline alert.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct InlineAlertRoute {
    pub(crate) warning_action: Option<PendingWarningAction>,
    pub(crate) lifetime: InlineAlertLifetime,
}

/// Whether decoded on-disk bytes round-trip under the active encoding and BOM.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DecodeRoundTrip {
    #[default]
    Exact,
    Inexact,
}

/// Encoding, line-ending, health, and save-confirmation state for one tab.
#[derive(Default)]
pub struct DocumentMetadataState {
    pub encoding_state: Cell<DocumentEncodingState>,
    pub has_bom: Cell<bool>,
    pub(crate) decode_round_trips: Cell<DecodeRoundTrip>,
    pub file_health: RefCell<Vec<FileHealthFinding>>,
    pub invisible_mode: Cell<InvisibleCharactersMode>,
    pub(crate) alert: Cell<InlineAlertRoute>,
    pub allow_lossy_save_once: Cell<bool>,
    pub lossy_analysis_generation: Cell<LossyAnalysisGeneration>,
    /// Active large-buffer capture for the newest lossy-analysis request.
    pub lossy_analysis_snapshot: RefCell<Option<BufferSnapshotHandle>>,
}

impl DocumentMetadataState {
    pub(crate) fn set_decode_inexact(&self, inexact: bool) {
        self.decode_round_trips.set(if inexact {
            DecodeRoundTrip::Inexact
        } else {
            DecodeRoundTrip::Exact
        });
    }

    pub(crate) fn decode_is_inexact(&self) -> bool {
        self.decode_round_trips.get() == DecodeRoundTrip::Inexact
    }

    pub(crate) fn update_alert(&self, f: impl FnOnce(&mut InlineAlertRoute)) {
        let mut route = self.alert.get();
        f(&mut route);
        self.alert.set(route);
    }
}

/// Window-wide memory-residency and eviction bookkeeping for one editor tab.
#[derive(Default)]
pub struct MemoryResidencyState {
    pub evicted: Cell<bool>,
    pub access_generation: Cell<AccessGeneration>,
    pub policy_generation: Cell<PolicyGeneration>,
    #[cfg(feature = "test-utils")]
    pub estimate_override: Cell<Option<u64>>,
}

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
    fn republish_document_identity(&self) {
        self.request_language_attach();
        self.schedule_minimap_refresh();
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
        self.fire_content_republished();
    }

    /// The id of the document's language: the requested one while its
    /// attach stage is pending (`bound-post-open-settle`, review R2), the
    /// buffer's own, or, while the document scale holds syntax off (the gate
    /// clears the buffer language, ledger A44), the language its path names.
    ///
    /// The gate removes highlighting, not the document's kind: a 2 MiB
    /// `notes.md` is still Markdown, and the Markdown preview must still
    /// recognise it. A pending attach changes nothing for the kind either.
    #[must_use]
    pub fn document_language_id(&self) -> Option<gtk4::glib::GString> {
        if self.language_attach_pending()
            && let Some(id) = self.requested_language().id()
        {
            return Some(id.into());
        }
        if let Some(language) = self.buffer().language() {
            return Some(language.id());
        }
        if self.document_scale().syntax_enabled {
            return None;
        }
        let path = self.imp().file_path.borrow();
        let path = path.as_ref()?;
        crate::ui::language_catalog::manager()
            .guess_language(path.to_str(), None::<&str>)
            .map(|language| language.id())
    }
}

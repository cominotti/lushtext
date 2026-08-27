// SPDX-License-Identifier: GPL-3.0-or-later

//! The local-history workflow's observable state, in one typed value.
//!
//! **One surface for a workflow that spans two directories.** The capture half
//! lives in `ui/editor_page/local_history.rs` as a called surface, but its state
//! is this workflow's, so it is projected here rather than through a second
//! accessor: a workflow has one evidence surface, wherever its code sits.
//!
//! [`LocalHistoryEvidence`] folds in **both** pre-convention typed observations
//! the matrix named — `LocalHistoryPreviewCoordinatorSnapshot` and
//! `LocalHistoryPreviewInstallSnapshot` — so no second typed path remains.
//!
//! Reading evidence is pure observation: it never arms the periodic timer,
//! advances a capture generation, submits a preview read, or requires a browser
//! to be open. The `record_*` helpers are the workflow's own named operations.
//!
//! **Reentrancy constraint.** [`LushtextEditorPage::local_history_evidence`]
//! takes shared `RefCell` borrows of the clean-baseline slot, the periodic
//! snapshot handle, and the undo body. It must therefore not be called from code
//! already holding a `borrow_mut()` on any of them — which is why every derived
//! scalar below is computed and every `Ref` dropped **before** the struct
//! literal is built.
//!
//! **Disposed-widget rule.** The capture half's subject is the editor's buffer,
//! so the buffer-derived field reads through `TemplateChild::try_get()` and
//! answers honestly for a disposed page.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use sourceview5::prelude::*;

use crate::services::local_history_service::LocalHistoryAvailability;
use crate::ui::editor_page::LushtextEditorPage;

/// Process-local preview installation counters.
///
/// Process-wide rather than per-editor because the browser is a window-modal
/// dialog with at most one live preview installer, and the counters exist to
/// prove boundedness across a whole test process.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LocalHistoryPreviewInstallEvidence {
    /// GTK insertion slices completed by this process.
    pub slices: usize,
    /// Sliced installations cancelled by supersession or disposal.
    pub cancellations: usize,
}

/// Read the process-local preview installation counters.
#[must_use]
pub fn local_history_preview_install_evidence() -> LocalHistoryPreviewInstallEvidence {
    use std::sync::atomic::Ordering;
    LocalHistoryPreviewInstallEvidence {
        slices: super::preview_execution::PREVIEW_INSTALL_SLICES.load(Ordering::Acquire),
        cancellations: super::preview_execution::PREVIEW_INSTALL_CANCELLATIONS
            .load(Ordering::Acquire),
    }
}

/// One consistent read of the local-history workflow, for one editor.
///
/// Field groups follow the workflow's two stage orders: capture first, then the
/// browse/restore surface's editor-side state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalHistoryEvidence {
    // --- availability ---
    /// The size-derived mode for this document's saved file.
    pub availability: LocalHistoryAvailability,
    /// The size-derived mode for the **live buffer**, which can differ.
    ///
    /// A document can grow past the automatic-capture limit while open, so
    /// capture admission reads this rather than the file-derived value.
    pub live_availability: LocalHistoryAvailability,
    /// Whether this document can be browsed at all.
    pub browse_available: bool,
    /// Whether automatic capture is permitted for the live buffer.
    pub automatic_capture_available: bool,

    // --- capture ---
    /// Whether a clean-baseline body is retained for the next capture.
    ///
    /// A bool rather than the text: evidence must not widen what an observer can
    /// read, and the body is document-sized.
    pub baseline_candidate_present: bool,
    /// Retries left for a failed baseline before it is abandoned.
    pub baseline_retry_budget: u8,
    /// Whether this editor owns a weak waiter in the global admission FIFO.
    pub baseline_retry_pending: bool,
    /// Whether one automatic baseline or periodic payload owns admission.
    ///
    /// Process-wide: exactly one document-sized automatic capture payload may
    /// cross the worker boundary at a time.
    pub automatic_capture_inflight: bool,
    /// Whether automatic capture is suspended, as it is across a save.
    pub automatic_capture_suppressed: bool,
    /// Whether a chunked periodic snapshot still owns its cancellation handle.
    pub periodic_snapshot_inflight: bool,
    /// Whether the tab owns one scheduled periodic timer source.
    pub periodic_timer_pending: bool,
    /// Monotonic identity of the current periodic cycle.
    pub periodic_generation: u32,
    /// Monotonic identity advanced by every buffer edit.
    pub edit_generation: u64,
    /// Monotonic identity advanced by every path change.
    pub path_generation: u64,
    /// Monotonic identity advanced when the editor is disposed.
    pub editor_generation: u64,
    /// Monotonic identity advanced by each clean-baseline replacement.
    pub clean_baseline_generation: u64,

    // --- restore ---
    /// Whether an undo-restore body is available to the inline affordance.
    pub restore_undo_available: bool,

    // --- buffer ---
    /// Live buffer character count, or `None` when the template child is gone.
    pub buffer_char_count: Option<i32>,
}

impl LushtextEditorPage {
    /// Read this editor's whole local-history workflow state at once.
    ///
    /// See the module documentation for the reentrancy constraint and the
    /// disposed-widget rule.
    #[must_use]
    pub fn local_history_evidence(&self) -> LocalHistoryEvidence {
        let state = &self.imp().local_history;

        // Every borrow is taken, read, and dropped before the struct literal.
        let baseline_candidate_present = state.last_clean_text.borrow().is_some();
        let periodic_snapshot_inflight = state.periodic_snapshot.borrow().is_some();
        let restore_undo_available = state.restore_undo_text.borrow().is_some();
        // A disposed page has no source view, so this is the honest `None`.
        let buffer_char_count = self
            .imp()
            .source_view
            .try_get()
            .map(|view| view.buffer().char_count());
        let availability = self.local_history_availability();
        // Derived from the `try_get()` count above rather than re-reading the
        // buffer: `live_local_history_availability` would deref the template
        // child, which panics on a disposed page. A gone buffer falls back to the
        // file-derived mode, which is the honest answer — the document's saved
        // size is still known.
        let live_availability = buffer_char_count.map_or(availability, |char_count| {
            self.live_local_history_availability_for_chars(char_count)
        });

        LocalHistoryEvidence {
            availability,
            live_availability,
            browse_available: availability.allows_browsing(),
            automatic_capture_available: live_availability.allows_automatic_capture(),
            baseline_candidate_present,
            baseline_retry_budget: state.baseline_retry_budget.get(),
            baseline_retry_pending: state.baseline_retry_pending.get(),
            automatic_capture_inflight:
                crate::ui::editor_page::local_history::automatic_capture_inflight(),
            automatic_capture_suppressed: state.automatic_capture_suppressed.get(),
            periodic_snapshot_inflight,
            periodic_timer_pending: state.periodic_timer_token.get().is_some(),
            periodic_generation: state.periodic_generation.get(),
            edit_generation: state.edit_generation.get(),
            path_generation: state.path_generation.get(),
            editor_generation: state.editor_generation.get(),
            clean_baseline_generation: state.clean_baseline_generation.get(),
            restore_undo_available,
            buffer_char_count,
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The document-save workflow's observable state, in one typed value.
//!
//! [`SaveEvidence`] is the single source of truth for observers of this
//! workflow. Widget tests read it instead of calling per-field `*_for_test`
//! getters or reaching through `imp()`, and the read-only D-Bus automation
//! snapshot projects its documented fields from it rather than re-deriving the
//! same state from widgets. A test that needs a fact this surface does not carry
//! **extends the surface**; adding another per-field inspection function is the
//! regression this type exists to prevent.
//!
//! Reading evidence is pure observation: it never advances a generation, arms a
//! timer, drains a queue, releases an admission charge, or requires the workflow
//! to be in a particular stage. The recorders below (`record_*`) are the
//! workflow's own named operations, called from coordination as stages complete
//! — they are not part of the read path.
//!
//! Reentrancy constraint: [`LushtextEditorPage::save_evidence`] takes shared
//! `RefCell` borrows of the admitted ticket and the chunked-capture handle, and
//! it reads the process-wide admission coordinator through a shared borrow. It
//! must therefore be called from code that is not already holding a
//! `borrow_mut()` on any of those, or the borrow would panic. Every current
//! caller observes from outside a mutation — widget tests and the read-only
//! automation snapshot — so no live path can reach that state.

use std::path::PathBuf;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use super::policy::{QueuedSaveTicket, SaveCaptureMode, SaveWriteClassification};
use super::{admission, execution};
use crate::ui::editor_page::LushtextEditorPage;

/// One consistent read of the document-save workflow.
///
/// Field groups follow the workflow's stages: the queue and its shared byte
/// admission, the admitted request's identity, how the buffer was captured, and
/// how the durable write ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveEvidence {
    // --- in-flight save ---
    /// Whether a save currently owns this editor.
    ///
    /// This is what `tabs[].saving` projects and what the `save` readiness
    /// blocker consumes. It stays true from the moment the queue stage publishes
    /// ownership until a terminal releases it, so a close flow cannot treat an
    /// in-flight durable write as already safe.
    pub inflight: bool,
    /// Monotonic identity of the current save and its formatting installation.
    pub generation: u64,

    // --- admitted request identity ---
    /// The destination the admitted save is writing, when one is admitted.
    pub admitted_path: Option<PathBuf>,
    /// Whether the admitted save's destination was named by the user (Save As).
    ///
    /// Never observable as `cancel_pending_load`: that names a consequence the
    /// workflow derives, not the request property.
    pub admitted_explicit_destination: Option<bool>,
    /// Whether the admitted save required a modified buffer when it was queued.
    pub admitted_required_modified: Option<bool>,
    /// The close session an admitted close-with-changes save gates.
    pub admitted_close_session_identity: Option<u64>,

    // --- shared byte admission ---
    /// Compact save requests waiting for shared payload capacity.
    pub queued_count: usize,
    /// How many of those gate a tab or window close.
    pub queued_close_count: usize,
    /// Admitted saves currently holding a payload charge.
    pub active_count: usize,
    /// Byte weight those admitted saves currently hold.
    pub active_weight: u64,
    /// Highest byte weight this lane has ever held at once.
    pub high_water_weight: u64,
    /// Whether one overweight save is running exclusively.
    pub exclusive_active: bool,
    /// Whether an idle drain is already armed.
    pub drain_pending: bool,

    // --- buffer capture ---
    /// Which capture mode this editor's buffer would take right now.
    ///
    /// A live classification against the cross-cutting chunked threshold, not a
    /// record of a past save. While a save is in flight the view is read-only,
    /// so it also describes the capture actually running.
    pub capture_mode: SaveCaptureMode,
    /// Whether a chunked capture is currently yielding through the main loop.
    pub capture_in_flight: bool,

    // --- durable write outcome ---
    /// How the last durable write for this editor ended.
    ///
    /// The `FailedBeforeRename` and `DurabilityUnconfirmed` arms are kept
    /// distinct because conflating them is a data-safety failure: the first
    /// leaves the previous bytes intact, the second means new bytes reached the
    /// destination without a proven-durable directory entry.
    pub write_classification: SaveWriteClassification,
    /// Whether save formatting rewrote the text the worker wrote.
    pub formatting_rewrote_buffer: bool,
    /// Whether the formatted text was mirrored back into the live buffer.
    ///
    /// The saved bytes and the live buffer must agree before the tab goes clean,
    /// so a rewrite with no completed mirror-back is an unfinished save.
    pub mirror_back_completed: bool,
}

impl LushtextEditorPage {
    /// Read this editor's whole document-save workflow state at once.
    ///
    /// See the module documentation for the reentrancy constraint: this takes
    /// shared borrows, so it must not be called from inside a `borrow_mut()` on
    /// the admitted ticket or the capture handle.
    #[must_use]
    pub fn save_evidence(&self) -> SaveEvidence {
        let imp = self.imp();
        let admitted_borrow = imp.save.admitted.borrow();
        let admitted = admitted_borrow.as_ref();
        let admission = admission::admission_snapshot();
        SaveEvidence {
            inflight: imp.save.inflight.get(),
            generation: imp.save.generation.get(),
            admitted_path: admitted.map(|ticket| ticket.path.clone()),
            admitted_explicit_destination: admitted.map(|ticket| ticket.explicit_destination),
            admitted_required_modified: admitted.map(|ticket| ticket.required_modified),
            admitted_close_session_identity: admitted
                .and_then(|ticket| ticket.close_session_identity),
            queued_count: admission.queued_count,
            queued_close_count: admission.queued_close_count,
            active_count: admission.active_count,
            active_weight: admission.active_weight,
            high_water_weight: admission.high_water_weight,
            exclusive_active: admission.exclusive_active,
            drain_pending: admission::drain_pending(),
            capture_mode: execution::capture_mode(self),
            capture_in_flight: execution::capture_in_flight(self),
            write_classification: imp.save.write_classification.get(),
            formatting_rewrote_buffer: imp.save.formatting_rewrote_buffer.get(),
            mirror_back_completed: imp.save.mirror_back_completed.get(),
        }
    }
}

/// Record which request the workflow just admitted.
pub(super) fn record_admitted_ticket(editor: &LushtextEditorPage, ticket: QueuedSaveTicket) {
    editor.imp().save.admitted.replace(Some(ticket));
    editor.imp().save.formatting_rewrote_buffer.set(false);
    editor.imp().save.mirror_back_completed.set(false);
}

/// Record that the admitted request reached a terminal and released the editor.
pub(super) fn clear_admitted_ticket(editor: &LushtextEditorPage) {
    editor.imp().save.admitted.borrow_mut().take();
}

/// Record whether save formatting rewrote the text that was written.
pub(super) fn record_formatting_rewrite(editor: &LushtextEditorPage, rewrote: bool) {
    editor.imp().save.formatting_rewrote_buffer.set(rewrote);
}

/// Record that the formatted text finished installing back into the buffer.
pub(super) fn record_mirror_back_completed(editor: &LushtextEditorPage) {
    editor.imp().save.mirror_back_completed.set(true);
}

/// Record how the durable write ended, against the durable-write contract.
pub(super) fn record_write_classification(
    editor: &LushtextEditorPage,
    classification: SaveWriteClassification,
) {
    editor.imp().save.write_classification.set(classification);
}

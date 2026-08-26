// SPDX-License-Identifier: GPL-3.0-or-later

//! The document-load workflow's observable state, in one typed value.
//!
//! [`LoadEvidence`] is the single source of truth for observers of this
//! workflow. Widget tests read it instead of calling per-field `*_for_test`
//! getters or reaching through `imp()`, and the read-only D-Bus automation
//! snapshot projects its one documented field from it rather than re-deriving
//! the same state from widgets. A test that needs a fact this surface does not
//! carry **extends the surface**; adding another per-field inspection function
//! is the regression this type exists to prevent.
//!
//! Reading evidence is pure observation: it never advances a generation, arms a
//! timer, drains a queue, releases an admission charge or a disposal
//! reservation, or requires the workflow to be in a particular stage. The
//! recorders below (`record_*`) are the workflow's own named operations, called
//! from coordination as stages complete — they are not part of the read path.
//!
//! **Reentrancy constraint** (stated normatively in
//! `openspec/specs/workflow-evidence-surfaces/spec.md`):
//! [`LushtextEditorPage::load_evidence`] takes shared `RefCell` borrows of the
//! cancellation token and the installation session, and it reads the
//! process-wide admission coordinator through a shared borrow. It must
//! therefore never be called from code already holding a `borrow_mut()` on any
//! of those, or the borrow panics at runtime. The accessor deliberately
//! computes every derived scalar first and drops each `Ref` before building the
//! struct literal, so no borrow is held longer than the value it produced.
//!
//! No field here derives from a `TemplateChild`, so the surface stays readable
//! after GTK4 has disposed the page and cleared its template children. That is
//! proved rather than assumed — see the teardown case in the widget suite.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use super::admission;
use super::policy::{LoadInstallPhase, LoadOutcome};
use crate::ui::editor_page::{EditorLoadState, LushtextEditorPage};

/// One consistent read of the document-load workflow.
///
/// Field groups follow the workflow's stages: the request's freshness identity,
/// the shared byte admission it waits in, how the decoded payload is being
/// installed, and how the load ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadEvidence {
    // --- request identity and freshness ---
    /// Monotonic identity of this editor's newest load request.
    ///
    /// Advanced by every new request and by every cancellation or disposal, so
    /// a worker already in flight cannot publish against it.
    pub generation: u64,
    /// Stable identity of the cancellation token the newest request carries.
    ///
    /// A fresh `Arc` per request is what stops a newer load from un-cancelling
    /// an older worker, so token identity — not just the flag — is part of the
    /// freshness seam.
    pub cancel_token_identity: usize,
    /// Whether the newest request's cancellation token has been set.
    pub cancel_requested: bool,
    /// Whether a later request is parked behind bounded cleanup.
    pub pending_request_parked: bool,
    /// Whether the token the *previous* request carried had already been
    /// cancelled at the moment the newest request rotated identity.
    ///
    /// **This pins an ordering, not a possibility.** The entry stage cancels the
    /// outgoing request before it rotates, and rotation is the field's only
    /// writer, so on every reachable path this reads `Some(true)` once a second
    /// request has started. That is what makes it useful: swap those two steps
    /// and it reads `Some(false)`, which is exactly what
    /// `test_new_load_cancels_previous_token_without_reusing_identity` asserts.
    /// It replaced a test seam that returned the live `Arc`, which could observe
    /// the retired token directly; this observes the ordering that retired it.
    pub previous_request_cancelled: Option<bool>,

    // --- shared byte admission ---
    /// Planned load requests waiting for shared transient capacity.
    pub queued_count: usize,
    /// Admitted loads currently holding a transient charge.
    pub active_count: usize,
    /// Byte weight those admitted loads currently hold.
    pub active_weight: u64,
    /// Highest byte weight this lane has ever held at once.
    pub high_water_weight: u64,
    /// Whether one overweight load is running exclusively.
    pub exclusive_active: bool,
    /// Whether an idle drain is already armed.
    pub drain_pending: bool,
    /// Whether the lane is polling for the disposal lane to free capacity.
    pub disposal_wakeup_armed: bool,

    // --- installation ---
    /// Whether a bounded installation currently retains decoded text.
    pub installation_active: bool,
    /// Which phase that installation is running, when one is active.
    pub installation_phase: Option<LoadInstallPhase>,
    /// Admitted payload charge the active installation still retains.
    pub installation_weight: Option<u64>,
    /// Main-loop slices completed by the newest installation.
    ///
    /// Zero for a direct install, which is how an observer tells the two paths
    /// apart without inspecting the session.
    pub installation_slice_count: u64,
    /// Whether the newest installation took the bounded slicing path.
    pub installation_chunked: bool,
    /// Whether cancellation discarded a partially installed payload.
    ///
    /// Saving stays blocked while this is set: the buffer was intentionally
    /// emptied, so writing it out would truncate the user's file.
    pub installation_incomplete: bool,
    /// Whether signal-emitting final projection owns the editor lifecycle.
    pub finalizing: bool,
    /// Whether installation is suppressing document-amplifying projections.
    pub projection_suspended: bool,

    // --- outcome ---
    /// The lifecycle state this tab reports to the user and to automation.
    pub load_state: EditorLoadState,
    /// How the newest load for this editor ended.
    ///
    /// [`LoadOutcome::RefusedAsStale`] is kept distinct from both `Failed` and
    /// `Cancelled`: a completion the workflow declined to publish is neither a
    /// user-visible failure nor a user cancellation, and conflating them would
    /// hide the freshness seam this workflow exists to protect.
    pub outcome: LoadOutcome,
}

impl LushtextEditorPage {
    /// Read this editor's whole document-load workflow state at once.
    ///
    /// See the module documentation for the reentrancy constraint: this takes
    /// shared borrows of the cancellation token, the installation session, and
    /// the process-wide coordinator, so it must not be called from inside a
    /// `borrow_mut()` on any of them.
    #[must_use]
    pub fn load_evidence(&self) -> LoadEvidence {
        let imp = self.imp();

        let (cancel_token_identity, cancel_requested) = {
            let token = imp.load_tracking.cancel_token.borrow();
            (
                std::sync::Arc::as_ptr(&token) as usize,
                token.load(std::sync::atomic::Ordering::Acquire),
            )
        };

        let session = imp.load.installation.borrow().clone();
        let (installation_phase, installation_weight) =
            session.as_ref().map_or((None, None), |session| {
                let session = session.borrow();
                (Some(session.phase()), session.retained_weight())
            });
        drop(session);

        let admission_snapshot = admission::admission_snapshot();

        LoadEvidence {
            generation: imp.load_tracking.generation.get(),
            cancel_token_identity,
            cancel_requested,
            pending_request_parked: imp.load.pending_load.borrow().is_some(),
            previous_request_cancelled: imp.load.previous_request_cancelled.get(),
            queued_count: admission_snapshot.queued_count,
            active_count: admission_snapshot.active_count,
            active_weight: admission_snapshot.active_weight,
            high_water_weight: admission_snapshot.high_water_weight,
            exclusive_active: admission_snapshot.exclusive_active,
            drain_pending: admission::drain_pending(),
            disposal_wakeup_armed: admission::disposal_wakeup_armed(),
            installation_active: installation_phase.is_some(),
            installation_phase,
            installation_weight,
            installation_slice_count: imp.load.installation_slice_count.get(),
            installation_chunked: imp.load.installation_chunked.get(),
            installation_incomplete: imp.load.installation_incomplete.get(),
            finalizing: imp.load.finalizing.get(),
            projection_suspended: imp.load.projection_suspended.get(),
            load_state: imp.load_state.get(),
            outcome: imp.load.outcome.get(),
        }
    }
}

/// Record that a new load request has taken ownership of this editor.
pub(super) fn record_load_started(editor: &LushtextEditorPage) {
    editor.imp().load.outcome.set(LoadOutcome::InFlight);
}

/// Record whether the request this rotation replaced had been cancelled.
pub(super) fn record_retired_request_cancellation(editor: &LushtextEditorPage, cancelled: bool) {
    editor
        .imp()
        .load
        .previous_request_cancelled
        .set(Some(cancelled));
}

/// Record which installation path the accepted payload took.
pub(super) fn record_install_started(editor: &LushtextEditorPage, chunked: bool) {
    editor.imp().load.installation_chunked.set(chunked);
}

/// Record how the newest load for this editor ended.
pub(super) fn record_outcome(editor: &LushtextEditorPage, outcome: LoadOutcome) {
    editor.imp().load.outcome.set(outcome);
}

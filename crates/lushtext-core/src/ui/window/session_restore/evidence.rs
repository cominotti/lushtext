// SPDX-License-Identifier: GPL-3.0-or-later

//! The session-restore workflow's observable state, in one typed value.
//!
//! [`SessionRestoreEvidence`] is the single source of truth for observers of this
//! workflow. It folds in the pre-convention `SessionRestoreRuntimeSnapshot`, so
//! no second typed path remains, and it carries the journal's failure state so
//! widget tests stop reaching into `imp().session`.
//!
//! Reading evidence is pure observation: it never arms the debounce, advances a
//! generation, plans a turn, releases a permit, or requires a restore to be in
//! flight. [`record_restore_outcome`] is the workflow's own named operation,
//! called from admission as a generation reaches its terminal — it is not part of
//! the read path.
//!
//! **Reentrancy constraint.** [`LushtextWindow::session_restore_evidence`] takes
//! shared `RefCell` borrows of the restore runtime, the startup cancel token, and
//! the failure detail. It must therefore not be called from code already holding
//! a `borrow_mut()` on any of them — which is why every derived scalar below is
//! computed and every `Ref` dropped **before** the struct literal is built.
//!
//! **Disposed-widget rule.** Every field here derives from the window's own
//! `imp` state or from the tab view's page count. The tab view is a
//! `TemplateChild`, so the one field that needs it reads through
//! `TemplateChild::try_get()` and answers honestly when the child is gone: a
//! disposed window is a stage, and a teardown test may legitimately ask what the
//! restore recorded.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use super::policy::SessionRestoreTurnMetrics;
use crate::ui::window::LushtextWindow;

/// One consistent read of the session-restore workflow.
///
/// Field groups follow the workflow's two stage orders: the journal's write
/// state, then the restore generation's admission and boundedness.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionRestoreEvidence {
    // --- journal ---
    /// Whether the newest attempted session-file save failed and needs retry.
    ///
    /// **Session-file** save failure, not document-save state. Slot 3a's planning
    /// named this as a document-save site and then found it is written and cleared
    /// only by this workflow: a field whose name contains "save" is not thereby
    /// save-workflow state.
    pub save_failed: bool,
    /// Generation of the newest failed session save.
    pub failed_generation: u32,
    /// Whether a failure detail is retained for the close-flow warning.
    ///
    /// A bool rather than the string: the detail can contain a filesystem path,
    /// and evidence must not widen what an observer can read.
    pub failure_detail_present: bool,
    /// Whether close persistence must preserve the prior session file because
    /// startup has not yet published compact restore descriptors.
    pub startup_descriptors_pending: bool,
    /// Whether a cancellable startup journal read is still in flight.
    pub startup_load_cancellable: bool,

    // --- shared close-safety gate ---
    /// Whether the combined draft-and-session close-safety pass is running.
    ///
    /// **Shared** with the draft-recovery workflow: one pass runs
    /// `flush_dirty_drafts_async` and `save_session_for_close_async` together.
    /// Both workflows project it; neither owns it.
    pub close_safety_inflight: bool,
    /// One-shot bypass that lets the final close proceed after both halves pass.
    pub close_safety_bypass: bool,

    // --- restore generation ---
    /// Whether a bounded restore generation currently owns the window.
    pub active: bool,
    /// Whether that generation has a turn scheduled.
    pub scheduled_source: bool,
    /// Whether the restore flag is set, including before the first turn.
    pub restoring: bool,
    /// Whether tab-derived projections are deferred by a restore batch.
    pub projection_deferred: bool,
    /// The active generation's identity, or the last one's after it settled.
    pub generation: u64,
    /// Total compact descriptors this generation started with.
    pub total_descriptors: usize,
    /// Pages this generation has created.
    pub pages_created: usize,
    /// GTK main-loop turns this generation has consumed.
    pub gtk_turns: usize,
    /// Most pages any single turn created — the bounded-turn proof.
    pub max_pages_in_one_turn: usize,
    /// Peak concurrent background file-planning operations.
    pub max_inflight_file_plans: usize,
    /// Planning terminals counted so far.
    ///
    /// Every load terminal must either carry a parked request's planning owner
    /// into a restart or release it, so this rising to `total_descriptors` worth
    /// of file-backed tabs is what proves none was dropped.
    pub planning_terminals: usize,
    /// Descriptors not yet admitted.
    pub pending_descriptors: usize,
    /// Permits currently held.
    pub active_file_plans: usize,
    /// Terminal projection publications, which must never exceed one.
    pub terminal_projection_publications: usize,
    /// Aggregate tab-projection rebuilds for this window.
    ///
    /// Window-owned, not restore-owned: the tab workflow owns the projection and
    /// this surface only reports it.
    pub aggregate_projection_publications: u64,
    /// Whether the last generation was cancelled rather than completed.
    pub cancelled: bool,
    /// Accepted user or CLI tab-selection intents since construction.
    pub selection_generation: u64,

    // --- window shell ---
    /// Mounted tab pages, or `None` when the template child is gone.
    ///
    /// `None` is the honest answer for a disposed window, not a zero.
    pub mounted_pages: Option<i32>,
}

impl LushtextWindow {
    /// Read this window's whole session-restore workflow state at once.
    ///
    /// See the module documentation for the reentrancy constraint and the
    /// disposed-widget rule.
    #[must_use]
    pub fn session_restore_evidence(&self) -> SessionRestoreEvidence {
        let imp = self.imp();
        let session = &imp.session;

        // Every borrow is taken, read, and dropped before the struct literal, so
        // no `Ref` outlives the value it produced.
        let (active, scheduled_source, metrics) = {
            let runtime = session.restore_runtime.borrow();
            runtime.as_ref().map_or_else(
                || {
                    (
                        false,
                        false,
                        session.last_restore_outcome.get().unwrap_or_default(),
                    )
                },
                |runtime| {
                    (
                        true,
                        runtime.scheduled_source.is_some(),
                        runtime.policy.metrics(),
                    )
                },
            )
        };
        let startup_load_cancellable = session.restore_cancel.borrow().is_some();
        let failure_detail_present = session.failure_detail.borrow().is_some();
        let startup_descriptors_pending = self.startup_session_descriptors_pending();
        let projection_deferred = self.tab_projection_refresh_deferred();
        // A disposed window has no tab view, so this is the honest `None` rather
        // than the panicking `TemplateChild` accessor.
        let mounted_pages = imp.tab_view.try_get().map(|view| view.n_pages());

        SessionRestoreEvidence {
            save_failed: session.save_failed.get(),
            failed_generation: session.failed_generation.get(),
            failure_detail_present,
            startup_descriptors_pending,
            startup_load_cancellable,
            close_safety_inflight: session.close_safety_inflight.get(),
            close_safety_bypass: session.close_safety_bypass.get(),
            active,
            scheduled_source,
            restoring: session.restoring.get(),
            projection_deferred,
            generation: metrics.generation,
            total_descriptors: metrics.total_descriptors,
            pages_created: metrics.pages_created,
            gtk_turns: metrics.gtk_turns,
            max_pages_in_one_turn: metrics.max_pages_in_one_turn,
            max_inflight_file_plans: metrics.max_inflight_file_plans,
            planning_terminals: metrics.planning_terminals,
            pending_descriptors: metrics.pending_descriptors,
            active_file_plans: metrics.active_file_plans,
            terminal_projection_publications: metrics.terminal_projection_publications,
            aggregate_projection_publications: session.tab_projection_publications.get(),
            cancelled: metrics.cancelled,
            selection_generation: session.selection_generation.get(),
            mounted_pages,
        }
    }
}

/// Record how one restore generation ended, from the terminal that ended it.
///
/// This is a **last-restore outcome record**, not a cached evidence surface. The
/// distinction matters: the runtime that owned these counters is taken at the
/// terminal, so without retaining them an observer could never learn how the
/// restore that just finished behaved. The surface *projects* this field; it does
/// not read a cache of itself.
pub(super) fn record_restore_outcome(window: &LushtextWindow, metrics: SessionRestoreTurnMetrics) {
    window.imp().session.last_restore_outcome.set(Some(metrics));
}

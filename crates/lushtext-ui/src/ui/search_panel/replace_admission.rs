// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: Replace stage 4's admission, the wait for disposal capacity.
//!
//! This is the `admission` role for the Replace All stage order
//! (`replace-all-confirm-disposal-lane` design D2). The name carries the
//! stage-order qualifier because this panel holds two stage orders. A
//! confirmed Replace All reserves its undo descriptors on the shared
//! plain-data disposal lane before its worker starts; when another owner (a
//! palette file-index build, typically) holds that capacity, the run is
//! **parked** here instead of being refused, and resumes by itself once the
//! lane's capacity changes.
//!
//! The wait is the run's first phase, not a separate state: the window has
//! already opened the run (`replace_execution::begin_replace_run`), so the
//! button reads Cancel Replace All and `replace_in_flight` is true, and the
//! panel keeps the application's transaction (`journal`), so another
//! window's apply or Undo is refused as during a run. Every window's earlier
//! Undo stays installed until the run is admitted.
//!
//! # Owned state
//!
//! [`ReplaceAdmissionState`] keeps [`ReplaceAdmissionPhase`] (whether one
//! [`ParkedReplaceRun`] waits) alongside its one paced capacity wakeup.
//!
//! # Admission attempt
//!
//! [`LushtextSearchPanel::try_admit_replace_run`] is one attempt: it reads the
//! lane's capacity epoch, then tries the undo reservation. A refusal comes back
//! as a [`RefusedReplaceRun`] that carries that epoch, and only
//! [`LushtextSearchPanel::defer_replace_run`] consumes it, so a wait is always
//! armed from the epoch read before its own refused attempt and a release in
//! between still wakes it. The window decides, between the two, whether the
//! run is opened (first attempt) or already open (a resumption).
//!
//! # Control inversion
//!
//! [`LushtextSearchPanel::defer_replace_run`] returns once the wakeup is
//! armed. Control resumes in the wakeup closure, which takes the parked run,
//! asks `policy::replace_admission_resume` whether it may try again, and hands
//! the window a [`ReplaceAdmissionResumption`] through the
//! `replace_run_resumed` callback ([`LushtextSearchPanel::connect_replace_run_resumed`]): `Resume`
//! re-enters the window's admit-or-wait step (`ui/window/search.rs`), which may
//! park the run again; `Ended` ends the wait with nothing written. Cancel
//! Replace All reaches the same callback through
//! [`LushtextSearchPanel::cancel_parked_replace_run`]. Dispose cancels the
//! wakeup and drops the parked run without calling back (`imp::dispose`).

use std::cell::RefCell;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use lushtext_services::search::content_search::ReplaceJournalFreshness;

use super::policy::{self, ReplaceAdmissionDecision, ReplaceAdmissionFacts, ReplaceWaitEnd};
use super::{GuardedReplacements, LushtextSearchPanel};

/// Phase of a confirmed Replace All run awaiting disposal capacity.
#[derive(Default)]
pub(super) enum ReplaceAdmissionPhase {
    #[default]
    Idle,
    Parked(Box<ParkedReplaceRun>),
}

/// Owned Replace stage-4 admission state.
#[derive(Default)]
pub(super) struct ReplaceAdmissionState {
    pub(super) phase: RefCell<ReplaceAdmissionPhase>,
    pub(super) wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
}

/// A confirmed Replace All parked until the disposal lane can admit its undo
/// reservation: the checked selection and the journal freshness the window
/// took from the transaction. A seam value object, built once by the window at
/// the refusal and validated as a unit when the run resumes.
pub(crate) struct ParkedReplaceRun {
    /// The partitioned selection, which keeps its own disposal reservation.
    pub(crate) replacements: GuardedReplacements,
    /// The journal generation this run reserved when it claimed the transaction.
    pub(crate) freshness: ReplaceJournalFreshness,
}

/// Which admission attempt this is: the one at Confirm, or one resumed by
/// the capacity wakeup. Passed explicitly rather than inferred from panel
/// state, because the first attempt opens the run only once it is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdmissionAttempt {
    /// The attempt at Confirm; the run is not open yet.
    First,
    /// An attempt resumed after a wait; the run is open and parked no longer.
    Resumed,
}

/// The outcome of one admission attempt.
pub(crate) enum ReplaceAdmission {
    /// The undo reservation was granted: start the run.
    Admitted {
        /// The run to start.
        run: ParkedReplaceRun,
        /// The reservation that covers its undo descriptors.
        undo_reservation: crate::ui::plain_disposal::DisposalReservation,
    },
    /// The reservation was refused: the run waits.
    Refused(RefusedReplaceRun),
}

/// A run whose undo reservation was refused, with the capacity epoch read
/// before that attempt. Only [`LushtextSearchPanel::try_admit_replace_run`]
/// makes one, and only [`LushtextSearchPanel::defer_replace_run`] takes it.
pub(crate) struct RefusedReplaceRun {
    run: ParkedReplaceRun,
    observed_epoch: u64,
}

/// What the window receives when a waiting run's wait ends.
pub(crate) enum ReplaceAdmissionResumption {
    /// Capacity changed and the run still owns everything it needs: try the
    /// undo reservation again, after recomputing the unsaved-tab skip set.
    Resume(ParkedReplaceRun),
    /// The wait ended without starting the run; the selection is already on
    /// its way to the disposal lane and no file was written.
    Ended(ReplaceWaitEnd),
}

impl LushtextSearchPanel {
    /// One admission attempt: read the capacity epoch, then try the undo
    /// reservation, crediting the selection and every journal admission
    /// retires (`journal::try_reserve_undo_replacement`).
    pub(crate) fn try_admit_replace_run(&self, run: ParkedReplaceRun) -> ReplaceAdmission {
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        match self.try_reserve_undo_replacement(run.replacements.reservation_weight()) {
            Some(undo_reservation) => ReplaceAdmission::Admitted {
                run,
                undo_reservation,
            },
            None => ReplaceAdmission::Refused(RefusedReplaceRun {
                run,
                observed_epoch,
            }),
        }
    }

    /// Park a confirmed run whose undo reservation was refused, and arm the
    /// capacity wakeup from the epoch read before that attempt, so a release
    /// between the attempt and the arming still wakes it. The waiting
    /// description is projected once, when the wait begins: a resumed attempt
    /// that is refused again keeps it.
    pub(crate) fn defer_replace_run(&self, refused: RefusedReplaceRun, attempt: AdmissionAttempt) {
        let imp = self.imp();
        debug_assert!(
            self.replace_run_in_flight(),
            "the wait is a run's first phase"
        );
        let RefusedReplaceRun {
            run: parked,
            observed_epoch,
        } = refused;
        let previous = imp
            .replace_admission
            .phase
            .replace(ReplaceAdmissionPhase::Parked(Box::new(parked)));
        debug_assert!(
            matches!(previous, ReplaceAdmissionPhase::Idle),
            "one parked run per panel"
        );
        drop(previous);
        let panel_weak = self.downgrade();
        imp.replace_admission.wakeup.arm(observed_epoch, move || {
            if let Some(panel) = panel_weak.upgrade() {
                panel.resume_parked_replace_run();
            }
        });
        if attempt == AdmissionAttempt::First {
            self.project_replace_wait(true);
        }
    }

    /// Resumption point of the capacity wakeup: decide, then hand the
    /// outcome to the window.
    fn resume_parked_replace_run(&self) {
        let ReplaceAdmissionPhase::Parked(parked) = self.imp().replace_admission.phase.take()
        else {
            return;
        };
        let facts = ReplaceAdmissionFacts {
            transaction_claimed: self.replace_transaction_claimed(),
            freshness_current: parked.freshness.is_current(),
            cancel_requested: self.replace_cancel_requested(),
            run_control_present: self.replace_run_in_flight(),
        };
        let resumption = match policy::replace_admission_resume(facts) {
            ReplaceAdmissionDecision::Resume => ReplaceAdmissionResumption::Resume(*parked),
            ReplaceAdmissionDecision::Abandon(reason) => {
                // Counts only: a skip or wait diagnostic never carries content.
                tracing::info!(
                    reason = ?reason,
                    files = parked.replacements.len(),
                    "Replace All wait for disposal capacity ended before the run started"
                );
                self.retire_parked_selection(*parked);
                ReplaceAdmissionResumption::Ended(reason)
            }
        };
        self.hand_resumption_to_window(resumption);
    }

    /// Cancel Replace All while the run waits: end the wait at once, hand the
    /// parked selection to bounded retirement, and tell the window. Returns
    /// whether a wait was cancelled.
    pub(crate) fn cancel_parked_replace_run(&self) -> bool {
        let imp = self.imp();
        imp.replace_admission.wakeup.cancel();
        let ReplaceAdmissionPhase::Parked(parked) = imp.replace_admission.phase.take() else {
            return false;
        };
        self.retire_parked_selection(*parked);
        self.hand_resumption_to_window(ReplaceAdmissionResumption::Ended(
            ReplaceWaitEnd::Cancelled,
        ));
        true
    }

    /// End the run state of a wait that never started a worker, and release
    /// its transaction (design D4).
    ///
    /// Unlike `finish_replace_run`, this leaves the earlier run's skip report,
    /// its request and Show Skipped Files exactly as they were: the run that
    /// waited was never admitted, so it has no report to replace them with.
    pub(crate) fn end_deferred_replace_run(&self) {
        self.clear_deferred_replace_run_control();
        self.imp().replace_all_button.set_label("Replace All");
        self.update_replace_button_sensitivity();
        self.finish_replace_transaction();
    }

    /// Register the callback a Replace All waiting for disposal capacity
    /// resumes or ends through: `Resume` re-enters the window's admit-or-wait
    /// step, `Ended` ends the wait with nothing written. Kept beside the wait
    /// rather than in the facade, as `skipped_files` keeps
    /// `connect_replace_in_editor`.
    pub(crate) fn connect_replace_run_resumed<F: Fn(ReplaceAdmissionResumption) + 'static>(
        &self,
        f: F,
    ) {
        self.imp()
            .callbacks
            .replace_run_resumed_callback
            .replace(Some(std::rc::Rc::new(f)));
    }

    /// Whether a confirmed Replace All is waiting for disposal capacity.
    #[must_use]
    pub fn replace_admission_deferred(&self) -> bool {
        matches!(
            &*self.imp().replace_admission.phase.borrow(),
            ReplaceAdmissionPhase::Parked(_)
        )
    }

    fn retire_parked_selection(&self, parked: ParkedReplaceRun) {
        self.retire_superseded_selection(parked.replacements);
    }

    /// Invoke the window's resumption callback, then leave the waiting
    /// presentation unless the window parked the run again.
    fn hand_resumption_to_window(&self, resumption: ReplaceAdmissionResumption) {
        let callback = self
            .imp()
            .callbacks
            .replace_run_resumed_callback
            .borrow()
            .clone();
        if let Some(callback) = callback {
            callback(resumption);
        } else {
            // No window to resume with: end the wait here, writing nothing.
            if let ReplaceAdmissionResumption::Resume(parked) = resumption {
                self.retire_parked_selection(parked);
            }
            self.end_deferred_replace_run();
        }
        if !self.replace_admission_deferred() {
            self.project_replace_wait(false);
        }
    }
}

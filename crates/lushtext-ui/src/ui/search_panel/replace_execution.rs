// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the Replace All preview attempt and its checked apply.
//!
//! This is the `execution` role for the Replace All stage order — it performs
//! that stage order's primary work: opening one preview attempt, admitting it
//! against disposal capacity, coalescing superseding requests behind one worker,
//! publishing or retiring each completion, and claiming the user's checked
//! selection for the durable apply. The name carries the stage-order qualifier
//! the module-boundaries spec sanctions, because `execution.rs` in this same
//! directory is already the streaming *search* stage order's execution module.
//!
//! The durable undo journal is the other half of the Replace All stage order and
//! lives in `journal.rs`. This module never reads the journal's state directly:
//! it calls `journal`'s two named crossing predicates,
//! `replace_transaction_claimed` and
//! `replace_transaction_generation_reserved`.
//!
//! # Control inversions
//!
//! 1. **Capacity retry.** When disposal admission refuses a preview
//!    reservation, the request is parked and control resumes in the
//!    `preview_capacity_wakeup` closure, which revalidates the ticket with
//!    `may_dispatch` before re-dispatching.
//! 2. **Preview generation.** The worker builds the preview and returns through
//!    a completion closure that revalidates the attempt's
//!    [`ReplacePreviewTicket`] against live [`ReplacePreviewFacts`]. A stale
//!    completion publishes nothing and routes its payload to bounded
//!    retirement.
//! 3. **Queued-request drain.** `finish_preview_worker` re-enters itself when
//!    the retained request is no longer dispatchable, so the drain is a
//!    tail-recursion rather than a loop.
//! 4. **Checked apply.** The partition runs on a worker and resumes in a second
//!    completion closure with the same ticket revalidation. On success it hands
//!    the selection to the window's Replace All callback, so control leaves the
//!    panel entirely; it returns only through `journal`'s
//!    `publish_undo_journal_for_generation`,
//!    `clear_undo_backup_for_generation`, and `finish_replace_transaction`.
//! 5. **Bounded preview retirement.** Superseded payloads are released on the
//!    disposal lane and resume in a `glib::idle_add_once` callback that drains
//!    the queued request.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use lushtext_model::values::ReplacePreviewGeneration;
use lushtext_model::workspace::content_search::{
    ReplacePreviewOutcome, ReplacePreviewSkipReason, ReplaceSkipReport, SearchMatchId,
};
use lushtext_services::search::content_search::{
    ReplaceProgress, generate_replacement_preview_with_budget_and_cancel,
};

use super::LushtextSearchPanel;
use super::policy::{
    self, ReplacePreviewFacts, ReplacePreviewTicket, completed_preview_reservation_weight,
    preview_reservation_weight,
};

/// Phase of Replace All preview generation, active selection, and confirmation.
#[derive(Default)]
pub enum PreviewPhase {
    #[default]
    Idle,
    Generating {
        cancel_token: Option<Arc<AtomicBool>>,
    },
    Active {
        outcome: Box<crate::ui::plain_disposal::DisposalOwned<ReplacePreviewOutcome>>,
        checked_match_ids: HashSet<SearchMatchId>,
    },
    PreparingSelection,
}

/// Single-flight worker phase for Replace All preview and retirement.
#[derive(Default)]
pub(super) enum PreviewWorkerPhase {
    #[default]
    Idle,
    Busy {
        queued_request: Option<ReplacePreviewRequest>,
    },
}

/// Phase of an in-flight durable Replace All run.
#[derive(Default)]
pub(super) enum ReplaceRunPhase {
    #[default]
    Idle,
    InFlight {
        control: ReplaceRunControl,
        intent: Option<super::policy::ReplaceRunIntent>,
    },
}

/// Captured request intent from the most recently accepted preview.
#[derive(Default)]
pub(super) enum AcceptedPreviewIntent {
    #[default]
    None,
    Captured(super::policy::ReplaceRunIntent),
}

impl AcceptedPreviewIntent {
    fn as_intent(&self) -> Option<&super::policy::ReplaceRunIntent> {
        match self {
            Self::None => None,
            Self::Captured(intent) => Some(intent),
        }
    }
}

/// State owned by `replace_execution` for Replace Preview, selection, and run control.
#[derive(Default)]
pub struct ReplaceExecutionState {
    pub(super) preview: RefCell<PreviewPhase>,
    pub(super) worker: RefCell<PreviewWorkerPhase>,
    pub(super) run: RefCell<ReplaceRunPhase>,
    pub(super) accepted_intent: RefCell<AcceptedPreviewIntent>,
    pub(super) preview_generation: Cell<ReplacePreviewGeneration>,
    pub(super) preview_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
    pub(super) preview_retirement_jobs: Cell<u64>,
    pub(super) preview_retirement_pending: Arc<AtomicUsize>,
    pub(super) preview_selection_jobs: Cell<u64>,
}

impl ReplaceExecutionState {
    /// Whether pure preview generation or confirmation partitioning is pending.
    #[must_use]
    pub fn is_preview_pending(&self) -> bool {
        matches!(
            &*self.preview.borrow(),
            PreviewPhase::Generating { .. } | PreviewPhase::PreparingSelection
        )
    }

    /// Whether the panel currently holds an accepted preview selection.
    #[must_use]
    pub fn is_preview_mode(&self) -> bool {
        matches!(&*self.preview.borrow(), PreviewPhase::Active { .. })
    }

    /// Whether the single-flight preview/retirement worker lane is busy.
    #[must_use]
    pub fn is_preview_worker_running(&self) -> bool {
        matches!(&*self.worker.borrow(), PreviewWorkerPhase::Busy { .. })
    }

    /// Whether a latest superseding preview request is queued behind the active worker.
    pub(super) fn has_queued_request(&self) -> bool {
        matches!(
            &*self.worker.borrow(),
            PreviewWorkerPhase::Busy {
                queued_request: Some(_),
            }
        )
    }

    /// Update the checked status of one generated preview row.
    ///
    /// Returns `Some(has_checked)` when an active preview owns the row.
    pub(super) fn set_match_checked(&self, match_id: SearchMatchId, checked: bool) -> Option<bool> {
        let mut phase = self.preview.borrow_mut();
        let PreviewPhase::Active {
            checked_match_ids, ..
        } = &mut *phase
        else {
            return None;
        };
        if checked {
            checked_match_ids.insert(match_id);
        } else {
            checked_match_ids.remove(&match_id);
        }
        Some(!checked_match_ids.is_empty())
    }

    /// Set preview-pending phase directly for widget accessibility tests.
    #[cfg(feature = "test-utils")]
    pub fn set_preview_pending(&self, pending: bool) {
        if pending {
            if !self.is_preview_pending() {
                self.preview
                    .replace(PreviewPhase::Generating { cancel_token: None });
            }
        } else if self.is_preview_pending() {
            self.preview.replace(PreviewPhase::Idle);
        }
    }

    /// Clear all checked match IDs in the active preview (used by widget tests).
    #[cfg(feature = "test-utils")]
    pub fn clear_checked_matches(&self) {
        if let PreviewPhase::Active {
            checked_match_ids, ..
        } = &mut *self.preview.borrow_mut()
        {
            checked_match_ids.clear();
        }
    }

    /// Remove one match ID from the active preview selection (used by widget tests).
    #[cfg(feature = "test-utils")]
    pub fn uncheck_match(&self, match_id: SearchMatchId) {
        if let PreviewPhase::Active {
            checked_match_ids, ..
        } = &mut *self.preview.borrow_mut()
        {
            checked_match_ids.remove(&match_id);
        }
    }

    /// Read `(replacement, replaced_line)` for the first active preview row (used by widget tests).
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn first_preview_pair(&self) -> Option<(String, String)> {
        match &*self.preview.borrow() {
            PreviewPhase::Active { outcome, .. } => outcome
                .replacements
                .first()
                .map(|r| (r.replacement.to_string(), r.replaced_line.clone())),
            _ => None,
        }
    }

    /// Look up the match ID of the first active preview replacement in `path` (used by widget tests).
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn preview_match_id_for_path(&self, path: &std::path::Path) -> Option<SearchMatchId> {
        match &*self.preview.borrow() {
            PreviewPhase::Active { outcome, .. } => outcome
                .replacements
                .iter()
                .find(|replacement| replacement.path == path)
                .map(|replacement| replacement.match_id),
            _ => None,
        }
    }
}

/// The cancel flag and progress counters of one Replace All run, shared with
/// its worker (design D8). The panel holds them from the run's start to its
/// terminal; the window's progress poll reads the counters.
#[derive(Clone)]
pub(crate) struct ReplaceRunControl {
    /// Set by Cancel Replace All; the worker checks it per chunk.
    pub(crate) cancel: Arc<AtomicBool>,
    /// Files, bytes and phase the worker has reached.
    pub(crate) progress: Arc<ReplaceProgress>,
}

/// Latest plain-Rust preview request retained by the panel's single-flight worker.
pub(super) struct ReplacePreviewRequest {
    search_matches: std::sync::Arc<Vec<lushtext_model::workspace::content_search::SearchMatch>>,
    replacement_text: String,
    /// Identity of this attempt, validated as a unit when the worker resumes.
    ticket: ReplacePreviewTicket,
}

/// The status line when a preview is asked for while a Replace All run is in
/// flight, waiting for memory or writing.
const PREVIEW_REFUSED_DURING_RUN: &str = "Replace All is running; preview again when it finishes";

struct PreviewRetirementTerminal(Arc<AtomicUsize>);

impl Drop for PreviewRetirementTerminal {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl LushtextSearchPanel {
    /// Whether the panel is in preview mode.
    #[must_use]
    pub fn is_preview_mode(&self) -> bool {
        self.imp().replace_execution.is_preview_mode()
    }

    /// Detach any active preview outcome and checked IDs so `clear_results` can
    /// retire them after detaching visible search results.
    pub(super) fn take_active_preview_for_release(
        &self,
    ) -> Option<(
        crate::ui::plain_disposal::DisposalOwned<ReplacePreviewOutcome>,
        HashSet<SearchMatchId>,
    )> {
        let mut phase = self.imp().replace_execution.preview.borrow_mut();
        match std::mem::take(&mut *phase) {
            PreviewPhase::Active {
                outcome,
                checked_match_ids,
            } => Some((*outcome, checked_match_ids)),
            other => {
                *phase = other;
                None
            }
        }
    }

    /// Return `PreviewPhase` to `Idle` when clearing search results.
    pub(super) fn reset_preview_to_idle(&self) {
        self.imp()
            .replace_execution
            .preview
            .replace(PreviewPhase::Idle);
    }

    /// Clear run control when a deferred wait ends without starting a worker.
    pub(super) fn clear_deferred_replace_run_control(&self) {
        self.imp()
            .replace_execution
            .run
            .replace(ReplaceRunPhase::Idle);
    }

    /// Wire the Replace All, Skipped Files, Undo, and replacement entry controls.
    pub(super) fn setup_replace_controls(&self) {
        let imp = self.imp();

        let panel_weak = self.downgrade();
        imp.replace_all_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            if panel.replace_run_in_flight() {
                panel.request_replace_cancel();
            } else if panel.is_preview_mode() {
                panel.activate_confirm_replacements();
            } else {
                panel.activate_replace_preview();
            }
        });

        let panel_weak = self.downgrade();
        imp.skipped_files_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.show_skipped_files();
        });

        let panel_weak = self.downgrade();
        imp.undo_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.activate_undo_replacements();
        });

        let panel_weak = self.downgrade();
        imp.replace_entry.connect_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.invalidate_replace_preview_request();
            panel.update_replace_button_sensitivity();
        });
    }

    /// Reveal the replacement entry and pre-fill it, without starting a preview.
    ///
    /// Revealing the options row that contains the entry is this stage order's
    /// presentation, not one of the facade's entry-point query writes, so the
    /// facade delegates here rather than reaching for `more_toggle` itself.
    pub(super) fn reveal_replacement_entry(&self, text: &str) {
        let imp = self.imp();
        imp.more_toggle.set_active(true);
        imp.replace_entry.set_text(text);
    }

    /// Enter preview mode: generate replacement previews and switch the results
    /// list to show before/after with checkboxes.
    pub fn enter_preview_mode(&self, replacement_text: &str) {
        let imp = self.imp();
        // A Replace All run in flight, including one waiting for memory, owns
        // the button as Cancel Replace All; a new preview could not be
        // confirmed before it ends anyway (`replace-all-confirm-disposal-lane`),
        // so the request is refused, and says why.
        if self.replace_run_in_flight() {
            self.publish_journal_message(
                PREVIEW_REFUSED_DURING_RUN,
                lushtext_services::coordination::notifications::NotificationSeverity::Warning,
            );
            return;
        }
        let Some(search_matches) = self.accepted_search_matches() else {
            return;
        };

        let ticket = self.issue_preview_ticket();
        let retired_active = self.take_active_preview_for_release();
        imp.replace_execution
            .preview
            .replace(PreviewPhase::Generating { cancel_token: None });
        imp.replace_all_button.set_label("Preparing Preview…");
        imp.replace_all_button.set_sensitive(false);
        self.refresh_accessibility_state();

        if let Some((retired_outcome, retired_checked)) = retired_active {
            self.release_superseded_preview(Some(retired_outcome), retired_checked);
        }
        if imp.replace_execution.is_preview_worker_running() {
            if let PreviewWorkerPhase::Busy { queued_request } =
                &mut *imp.replace_execution.worker.borrow_mut()
            {
                if let Some(queued) = queued_request.as_mut() {
                    queued.search_matches = search_matches;
                    queued.replacement_text.clear();
                    queued.replacement_text.push_str(replacement_text);
                    queued.ticket.supersede(ticket);
                } else {
                    *queued_request = Some(ReplacePreviewRequest {
                        search_matches,
                        replacement_text: replacement_text.to_string(),
                        ticket,
                    });
                }
            }
            return;
        }
        self.spawn_preview_request(ReplacePreviewRequest {
            search_matches,
            replacement_text: replacement_text.to_string(),
            ticket,
        });
    }

    fn spawn_preview_request(&self, request: ReplacePreviewRequest) {
        let imp = self.imp();
        let budget = replace_preview_budget();
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let Some(mut reservation) =
            crate::ui::plain_disposal::try_reserve_for_gtk(preview_reservation_weight(budget))
        else {
            self.arm_preview_capacity_retry(request, observed_epoch);
            return;
        };
        imp.replace_execution
            .worker
            .replace(PreviewWorkerPhase::Busy {
                queued_request: None,
            });
        let cancel = Arc::new(AtomicBool::new(false));
        if let PreviewPhase::Generating { cancel_token } =
            &mut *imp.replace_execution.preview.borrow_mut()
        {
            *cancel_token = Some(cancel.clone());
        }
        let ticket = request.ticket.clone();
        // The request a Replace All of this preview applies (design D7): the
        // replacement entry is a single-line GTK field, so the copy is small.
        let intent = super::policy::ReplaceRunIntent {
            query: request.ticket.query_spec().clone(),
            replacement: request.replacement_text.clone(),
        };
        spawn_blocking_then(
            self.clone(),
            move || {
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_replace_preview();
                let outcome = generate_replacement_preview_with_budget_and_cancel(
                    &request.search_matches,
                    &request.ticket.query_spec().query,
                    &request.replacement_text,
                    &request.ticket.query_spec().options,
                    budget,
                    || cancel.load(Ordering::Relaxed),
                );
                reservation.shrink_to(completed_preview_reservation_weight(&outcome));
                reservation.own(outcome)
            },
            move |panel, outcome| {
                let imp = panel.imp();
                if let PreviewPhase::Generating { cancel_token } =
                    &mut *imp.replace_execution.preview.borrow_mut()
                {
                    *cancel_token = None;
                }
                if ticket.is_current(&panel.preview_facts()) {
                    let checked = outcome
                        .replacements
                        .iter()
                        .map(|replacement| replacement.match_id)
                        .collect();
                    let total = outcome.replacements.len();
                    imp.replace_execution
                        .accepted_intent
                        .replace(AcceptedPreviewIntent::Captured(intent));
                    imp.replace_execution.preview.replace(PreviewPhase::Active {
                        outcome: Box::new(outcome),
                        checked_match_ids: checked,
                    });

                    panel.refresh_preview_summary();
                    imp.replace_all_button.set_sensitive(total > 0);

                    panel.refresh_results_display();
                    panel.refresh_accessibility_state();
                    panel.finish_preview_worker();
                } else {
                    panel.spawn_guarded_preview_retirement(outcome);
                }
            },
        );
    }

    fn arm_preview_capacity_retry(&self, request: ReplacePreviewRequest, observed_epoch: u64) {
        let imp = self.imp();
        imp.replace_execution
            .worker
            .replace(PreviewWorkerPhase::Busy {
                queued_request: Some(request),
            });
        let panel_weak = self.downgrade();
        imp.replace_execution
            .preview_capacity_wakeup
            .arm(observed_epoch, move || {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let request = match panel.imp().replace_execution.worker.take() {
                    PreviewWorkerPhase::Busy { queued_request } => queued_request,
                    PreviewWorkerPhase::Idle => None,
                };
                if let Some(request) = request
                    && request.ticket.may_dispatch(&panel.preview_facts())
                {
                    panel.spawn_preview_request(request);
                }
            });
    }

    /// Detach accepted preview state in O(1) and release its plain payload on
    /// the same serial worker lane used by preview generation.
    pub(super) fn release_superseded_preview(
        &self,
        outcome: Option<crate::ui::plain_disposal::DisposalOwned<ReplacePreviewOutcome>>,
        checked_match_ids: HashSet<SearchMatchId>,
    ) {
        // The checked identities are plain `Copy` ids rather than a
        // document-sized payload, so they are released here on GTK either way;
        // only the outcome goes to the disposal lane.
        drop(checked_match_ids);
        if let Some(outcome) = outcome {
            self.mark_preview_worker_busy();
            self.spawn_guarded_preview_retirement(outcome);
        }
    }

    fn mark_preview_worker_busy(&self) {
        let mut worker = self.imp().replace_execution.worker.borrow_mut();
        if matches!(*worker, PreviewWorkerPhase::Idle) {
            *worker = PreviewWorkerPhase::Busy {
                queued_request: None,
            };
        }
    }

    /// Register the raw-vector callback used by the external widget harness
    /// (kept here, beside the apply it observes, so the facade stays within
    /// its line budget).
    #[cfg(feature = "test-utils")]
    pub fn connect_replace_all<
        F: Fn(Vec<lushtext_model::workspace::content_search::Replacement>) + 'static,
    >(
        &self,
        f: F,
    ) {
        self.connect_guarded_replace_all(move |run| {
            f(run.into_inner_for_current_install().replacements);
        });
    }

    /// Retire a parked Replace All selection whose wait ended before the run
    /// started, through the same bounded path as a superseded preview.
    pub(super) fn retire_superseded_selection(&self, selection: super::GuardedReplacements) {
        self.mark_preview_worker_busy();
        self.spawn_guarded_preview_retirement(selection);
    }

    /// Submit one superseded guarded preview payload to the disposal lane.
    ///
    /// Generic over the payload because the preview outcome and the partitioned
    /// selection are retired identically: charge the retirement counters, hand
    /// the payload its terminal, and let that terminal resume the queued-request
    /// drain on a later GTK turn.
    fn spawn_guarded_preview_retirement<T: Send + 'static>(
        &self,
        payload: crate::ui::plain_disposal::DisposalOwned<T>,
    ) {
        let exec = &self.imp().replace_execution;
        exec.preview_retirement_jobs
            .set(exec.preview_retirement_jobs.get().saturating_add(1));
        let retirement_pending = exec.preview_retirement_pending.clone();
        retirement_pending.fetch_add(1, Ordering::AcqRel);
        let terminal = PreviewRetirementTerminal(retirement_pending);
        let panel_weak = glib::thread_guard::ThreadGuard::new(self.downgrade());
        drop(payload.with_disposal_terminal(move || {
            drop(terminal);
            glib::idle_add_once(move || {
                if let Some(panel) = panel_weak.into_inner().upgrade() {
                    panel.finish_preview_worker();
                }
            });
        }));
    }

    fn finish_preview_worker(&self) {
        let queued = match self.imp().replace_execution.worker.take() {
            PreviewWorkerPhase::Busy { queued_request } => queued_request,
            PreviewWorkerPhase::Idle => None,
        };
        if let Some(request) = queued {
            if request.ticket.may_dispatch(&self.preview_facts()) {
                self.spawn_preview_request(request);
            } else {
                drop(request);
                self.finish_preview_worker();
            }
        }
    }

    /// Commit the user's confirmation of the visible preview selection.
    ///
    /// Replace stage 3 in one operation: claim the sole apply transaction, take
    /// the accepted outcome and its checked identities out of live state, open a
    /// fresh attempt so a resuming worker can prove it is still current, switch
    /// the visible summary and Replace All button into the preparing state, and
    /// hand the partition to [`Self::apply_checked_replacements`].
    ///
    /// Every early return leaves live state exactly as it was, including
    /// restoring the accepted outcome when the transaction is already claimed.
    pub(super) fn begin_confirmed_replacement(&self) {
        let imp = self.imp();
        if self.replace_transaction_claimed() || !self.is_preview_mode() {
            return;
        }
        let Some((outcome, checked)) = self.take_active_preview_for_release() else {
            return;
        };
        if self.begin_replace_transaction().is_none() {
            imp.replace_execution.preview.replace(PreviewPhase::Active {
                outcome: Box::new(outcome),
                checked_match_ids: checked,
            });
            return;
        }
        let ticket = self.issue_preview_ticket();
        imp.replace_execution
            .preview
            .replace(PreviewPhase::PreparingSelection);
        imp.replace_all_button.set_label("Preparing Selection…");
        imp.replace_all_button.set_sensitive(false);
        self.restore_search_summary();
        self.refresh_results_display();
        self.refresh_accessibility_state();
        self.apply_checked_replacements(ticket, outcome, checked);
    }

    /// Partition the checked preview rows on a worker, then hand them to the
    /// window's Replace All callback if the attempt is still current.
    pub(super) fn apply_checked_replacements(
        &self,
        ticket: ReplacePreviewTicket,
        outcome: crate::ui::plain_disposal::DisposalOwned<ReplacePreviewOutcome>,
        checked_match_ids: HashSet<SearchMatchId>,
    ) {
        let exec = &self.imp().replace_execution;
        debug_assert!(!exec.is_preview_worker_running());
        self.mark_preview_worker_busy();
        exec.preview_selection_jobs
            .set(exec.preview_selection_jobs.get().saturating_add(1));
        spawn_blocking_then(
            self.clone(),
            move || {
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_preview_selection();
                outcome.map_preserving_reservation(|outcome| {
                    outcome.into_checked_run(&checked_match_ids)
                })
            },
            move |panel, selected| {
                let imp = panel.imp();
                if ticket.is_current(&panel.preview_facts()) {
                    imp.replace_execution.preview.replace(PreviewPhase::Idle);
                    imp.replace_all_button.set_label("Replace All");
                    panel.update_replace_button_sensitivity();
                    panel.refresh_accessibility_state();
                    let callback = imp.callbacks.replace_callback.borrow().clone();
                    if selected.is_empty() {
                        panel.finish_replace_transaction();
                    } else if let Some(callback) = callback {
                        callback(selected);
                        if panel.replace_transaction_generation_reserved() {
                            panel.finish_replace_transaction();
                        }
                    } else {
                        panel.spawn_guarded_preview_retirement(selected);
                        panel.finish_replace_transaction();
                        return;
                    }
                    panel.finish_preview_worker();
                } else {
                    panel.spawn_guarded_preview_retirement(selected);
                    panel.finish_replace_transaction();
                }
            },
        );
    }

    /// Replace stage 4 begins: the durable run owns the panel until its
    /// terminal. The Replace All button becomes Cancel Replace All (design D8).
    pub(crate) fn begin_replace_run(&self) -> ReplaceRunControl {
        let imp = self.imp();
        let control = ReplaceRunControl {
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(ReplaceProgress::default()),
        };
        let intent = imp
            .replace_execution
            .accepted_intent
            .borrow()
            .as_intent()
            .cloned();
        imp.replace_execution
            .run
            .replace(ReplaceRunPhase::InFlight {
                control: control.clone(),
                intent,
            });
        imp.replace_all_button.set_label("Cancel Replace All");
        self.update_replace_button_sensitivity();
        control
    }

    /// The control of the run in flight, which a run admitted after a wait
    /// for disposal capacity reuses (`replace-all-confirm-disposal-lane`).
    #[must_use]
    pub(crate) fn replace_run_control(&self) -> Option<ReplaceRunControl> {
        match &*self.imp().replace_execution.run.borrow() {
            ReplaceRunPhase::Idle => None,
            ReplaceRunPhase::InFlight { control, .. } => Some(control.clone()),
        }
    }

    /// Whether a durable Replace All run is in flight.
    #[must_use]
    pub fn replace_run_in_flight(&self) -> bool {
        matches!(
            &*self.imp().replace_execution.run.borrow(),
            ReplaceRunPhase::InFlight { .. }
        )
    }

    /// Whether the run in flight was asked to stop.
    #[must_use]
    pub fn replace_cancel_requested(&self) -> bool {
        match &*self.imp().replace_execution.run.borrow() {
            ReplaceRunPhase::Idle => false,
            ReplaceRunPhase::InFlight { control, .. } => control.cancel.load(Ordering::Acquire),
        }
    }

    /// The progress counters of the run in flight, for the window's poll.
    #[must_use]
    pub(crate) fn replace_run_progress(&self) -> Option<Arc<ReplaceProgress>> {
        match &*self.imp().replace_execution.run.borrow() {
            ReplaceRunPhase::Idle => None,
            ReplaceRunPhase::InFlight { control, .. } => Some(Arc::clone(&control.progress)),
        }
    }

    /// Ask the run in flight to stop; it rolls back what it replaced. A run
    /// still waiting for disposal capacity has written nothing, so its wait
    /// ends at once (`replace_admission`).
    pub(crate) fn request_replace_cancel(&self) -> bool {
        if self.replace_admission_deferred() {
            return self.cancel_parked_replace_run();
        }
        let imp = self.imp();
        let requested = match &*imp.replace_execution.run.borrow() {
            ReplaceRunPhase::Idle => false,
            ReplaceRunPhase::InFlight { control, .. } => {
                !control.cancel.swap(true, Ordering::AcqRel)
            }
        };
        if requested {
            imp.replace_all_button.set_label("Cancelling…");
            self.update_replace_button_sensitivity();
        }
        requested
    }

    /// Replace stage 4 ends: restore the button and keep the run's skip report
    /// behind Show Skipped Files when it lists anything.
    pub(crate) fn finish_replace_run(&self, report: Option<ReplaceSkipReport>) {
        let imp = self.imp();
        let run_intent = match imp.replace_execution.run.take() {
            ReplaceRunPhase::Idle => None,
            ReplaceRunPhase::InFlight { intent, .. } => intent,
        };
        imp.replace_all_button.set_label("Replace All");
        let report = report.filter(|report| !report.is_empty()).map(Arc::new);
        // A run that never began (every file unsaved, 5.5a) applies the
        // accepted preview's request in this same turn.
        let intent = run_intent.or_else(|| {
            imp.replace_execution
                .accepted_intent
                .borrow()
                .as_intent()
                .cloned()
        });
        self.publish_skip_report(report, intent);
        self.update_replace_button_sensitivity();
    }

    /// Put the button back to Replace All, unless a run in flight, waiting or
    /// writing, owns it as Cancel Replace All (or Cancelling…).
    pub(super) fn reset_replace_button_label_unless_running(&self) {
        if !self.replace_run_in_flight() {
            self.imp().replace_all_button.set_label("Replace All");
        }
    }

    /// Exit preview mode: clear preview state and restore normal result display.
    pub fn exit_preview_mode(&self) {
        self.retire_visible_preview();
        self.update_replace_button_sensitivity();
    }

    fn retire_visible_preview(&self) {
        self.invalidate_active_preview();
        let retired = self.take_active_preview_for_release();
        self.reset_preview_to_idle();
        if let Some((outcome, checked_match_ids)) = retired {
            self.release_superseded_preview(Some(outcome), checked_match_ids);
        }
        self.reset_replace_button_label_unless_running();
        self.restore_search_summary();
        self.refresh_results_display();
        self.refresh_accessibility_state();
    }

    /// Update the "Replace All" / "Confirm Replace" button sensitivity.
    pub fn update_replace_button_sensitivity(&self) {
        let imp = self.imp();
        if self.replace_run_in_flight() {
            // The button is Cancel Replace All until Cancel is requested.
            imp.replace_all_button
                .set_sensitive(!self.replace_cancel_requested());
        } else if imp.replace_execution.is_preview_pending() || self.replace_transaction_claimed() {
            imp.replace_all_button.set_sensitive(false);
        } else {
            let sensitivity = match &*imp.replace_execution.preview.borrow() {
                PreviewPhase::Active {
                    checked_match_ids, ..
                } => !checked_match_ids.is_empty(),
                _ => imp.runtime.total_matches.get() > 0,
            };
            // Empty replacement text is allowed (deletes matches).
            imp.replace_all_button.set_sensitive(sensitivity);
        }
        self.refresh_accessibility_state();
    }

    /// Cancel any pending or visible replace preview after search state changes.
    ///
    /// Advancing the generation prevents late background preview results from
    /// restoring stale replacements.
    pub(crate) fn invalidate_replace_preview_request(&self) {
        let exec = &self.imp().replace_execution;
        if !exec.is_preview_pending() && !exec.is_preview_mode() {
            return;
        }
        self.retire_visible_preview();
    }

    /// Retire the current preview attempt so no in-flight result can publish.
    ///
    /// Advancing the generation is what makes a late worker completion fail its
    /// ticket check; the cancel token only lets the worker stop early.
    pub(super) fn invalidate_active_preview(&self) {
        let _ = self.open_preview_generation();
    }

    /// Open one preview attempt and capture its identity at the entry point.
    ///
    /// This is the only place a [`ReplacePreviewTicket`] is constructed, so
    /// generation and query spec cannot drift apart between the two preview
    /// entry points (preview generation and checked apply).
    pub(super) fn issue_preview_ticket(&self) -> ReplacePreviewTicket {
        let generation = self.open_preview_generation();
        ReplacePreviewTicket::new(generation, self.current_query_spec())
    }

    /// Live preview state a resuming worker completion must be validated against.
    ///
    /// Built eagerly at each of the four validation sites rather than behind a
    /// generation-first short-circuit: `current_query_spec()` is a pure read of
    /// visible GTK controls, and these sites are rare workflow completion and
    /// dispatch points, so one string/options clone there is accepted in
    /// exchange for validating the seam as a single value.
    pub(super) fn preview_facts(&self) -> ReplacePreviewFacts {
        let exec = &self.imp().replace_execution;
        ReplacePreviewFacts {
            generation: exec.preview_generation.get(),
            pending: exec.is_preview_pending(),
            query_spec: self.current_query_spec(),
        }
    }

    fn open_preview_generation(&self) -> ReplacePreviewGeneration {
        let exec = &self.imp().replace_execution;
        if let PreviewPhase::Generating { cancel_token } = &mut *exec.preview.borrow_mut()
            && let Some(cancel) = cancel_token.take()
        {
            cancel.store(true, Ordering::Relaxed);
        }
        let generation = exec.preview_generation.get().next();
        exec.preview_generation.set(generation);
        generation
    }

    /// Refresh visible and accessible confirmation feedback from accepted state.
    pub(crate) fn refresh_preview_summary(&self) {
        let imp = self.imp();
        let (checked, generated, omitted, truncated, stale_ranges) = {
            let phase = imp.replace_execution.preview.borrow();
            let PreviewPhase::Active {
                outcome,
                checked_match_ids,
            } = &*phase
            else {
                return;
            };
            (
                checked_match_ids.len(),
                outcome.replacements.len(),
                outcome.omitted_eligible,
                outcome
                    .skipped
                    .count(ReplacePreviewSkipReason::TruncatedSource),
                outcome
                    .skipped
                    .count(ReplacePreviewSkipReason::RegexRangeMismatch),
            )
        };
        let skipped = truncated.saturating_add(stale_ranges);
        imp.replace_all_button
            .set_label(&format!("Replace {checked} checked"));
        let summary = if generated == 0 {
            format!(
                "No eligible replacements; {omitted} omitted, {truncated} truncated, {stale_ranges} stale ranges"
            )
        } else if omitted > 0 || skipped > 0 {
            format!(
                "{generated} previewed, {checked} checked, {omitted} omitted, {truncated} truncated, {stale_ranges} stale ranges"
            )
        } else {
            format!("{generated} previewed, {checked} checked")
        };
        let summary = if imp.runtime.is_result_capped()
            && let Some(clause) = policy::search_limit_clause(
                imp.runtime.total_matches.get() as usize,
                imp.runtime.matches_found.get(),
            ) {
            format!("{summary}; {clause}")
        } else {
            summary
        };
        imp.count_label.set_text(&summary);
        crate::ui::accessibility::set_labelled_description(
            &*imp.replace_all_button,
            &format!("Apply {checked} checked replacements"),
            &summary,
        );
        self.refresh_accessibility_state();
    }
}

fn replace_preview_budget() -> lushtext_model::workspace::content_search::ReplacePreviewBudget {
    #[cfg(feature = "test-utils")]
    if let Some(budget) = super::test_policy::replace_preview_budget_override() {
        return budget;
    }
    lushtext_model::workspace::content_search::ReplacePreviewBudget::default()
}

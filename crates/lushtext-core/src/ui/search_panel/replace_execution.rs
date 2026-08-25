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

use crate::model::content_search::{
    ReplacePreviewOutcome, ReplacePreviewSkipReason, SearchMatchId,
    generate_replacement_preview_with_budget_and_cancel,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::LushtextSearchPanel;
use super::policy::{
    ReplacePreviewFacts, ReplacePreviewTicket, completed_preview_reservation_weight,
    preview_reservation_weight,
};

/// Latest plain-Rust preview request retained by the panel's single-flight worker.
pub(super) struct ReplacePreviewRequest {
    search_matches: std::sync::Arc<Vec<crate::model::content_search::SearchMatch>>,
    replacement_text: String,
    /// Identity of this attempt, validated as a unit when the worker resumes.
    ticket: ReplacePreviewTicket,
}

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
        self.imp().preview.preview_mode.get()
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
        let Some(search_matches) = self.accepted_search_matches() else {
            return;
        };

        let ticket = self.issue_preview_ticket();
        let retired_outcome = imp.preview.preview_outcome.take();
        let retired_checked = std::mem::take(&mut *imp.preview.checked_match_ids.borrow_mut());
        imp.preview.preview_pending.set(true);
        imp.preview.preview_mode.set(false);
        imp.replace_all_button.set_label("Preparing Preview…");
        imp.replace_all_button.set_sensitive(false);
        self.refresh_accessibility_state();

        self.release_superseded_preview(retired_outcome, retired_checked);
        if imp.preview.preview_worker_running.get() {
            if let Some(queued) = imp.preview.queued_preview_request.borrow_mut().as_mut() {
                queued.search_matches = search_matches;
                queued.replacement_text.clear();
                queued.replacement_text.push_str(replacement_text);
                queued.ticket.supersede(ticket);
            } else {
                imp.preview
                    .queued_preview_request
                    .replace(Some(ReplacePreviewRequest {
                        search_matches,
                        replacement_text: replacement_text.to_string(),
                        ticket,
                    }));
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
        imp.preview.preview_worker_running.set(true);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        imp.preview
            .preview_cancel_token
            .replace(Some(cancel.clone()));
        let ticket = request.ticket.clone();
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
                    || cancel.load(std::sync::atomic::Ordering::Relaxed),
                );
                reservation.shrink_to(completed_preview_reservation_weight(&outcome));
                reservation.own(outcome)
            },
            move |panel, outcome| {
                let imp = panel.imp();
                imp.preview.preview_cancel_token.replace(None);
                if ticket.is_current(&panel.preview_facts()) {
                    imp.preview.preview_pending.set(false);

                    let checked = outcome
                        .replacements
                        .iter()
                        .map(|replacement| replacement.match_id)
                        .collect();
                    imp.preview.checked_match_ids.replace(checked);
                    let total = outcome.replacements.len();
                    imp.preview.preview_outcome.replace(Some(outcome));
                    imp.preview.preview_mode.set(true);

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
        imp.preview.preview_worker_running.set(true);
        imp.preview.queued_preview_request.replace(Some(request));
        let panel_weak = self.downgrade();
        imp.preview
            .preview_capacity_wakeup
            .arm(observed_epoch, move || {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let request = panel.imp().preview.queued_preview_request.take();
                panel.imp().preview.preview_worker_running.set(false);
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
            self.imp().preview.preview_worker_running.set(true);
            self.spawn_guarded_preview_retirement(outcome);
        }
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
        let imp = self.imp();
        imp.preview
            .preview_retirement_jobs
            .set(imp.preview.preview_retirement_jobs.get().saturating_add(1));
        let retirement_pending = imp.preview.preview_retirement_pending.clone();
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
        let imp = self.imp();
        if let Some(request) = imp.preview.queued_preview_request.take() {
            if request.ticket.may_dispatch(&self.preview_facts()) {
                imp.preview.preview_worker_running.set(false);
                self.spawn_preview_request(request);
            } else {
                drop(request);
                self.finish_preview_worker();
            }
            return;
        }
        imp.preview.preview_worker_running.set(false);
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
        if self.replace_transaction_claimed() || !imp.preview.preview_mode.get() {
            return;
        }
        let Some(outcome) = imp.preview.preview_outcome.take() else {
            return;
        };
        if self.begin_replace_transaction().is_none() {
            imp.preview.preview_outcome.replace(Some(outcome));
            return;
        }
        let checked = std::mem::take(&mut *imp.preview.checked_match_ids.borrow_mut());
        let ticket = self.issue_preview_ticket();
        imp.preview.preview_mode.set(false);
        imp.preview.preview_pending.set(true);
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
        let imp = self.imp();
        debug_assert!(!imp.preview.preview_worker_running.get());
        imp.preview.preview_worker_running.set(true);
        imp.preview
            .preview_selection_jobs
            .set(imp.preview.preview_selection_jobs.get().saturating_add(1));
        spawn_blocking_then(
            self.clone(),
            move || {
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_preview_selection();
                outcome.map_preserving_reservation(|outcome| {
                    outcome.into_checked_replacements(&checked_match_ids)
                })
            },
            move |panel, selected| {
                let imp = panel.imp();
                if ticket.is_current(&panel.preview_facts()) {
                    imp.preview.preview_pending.set(false);
                    imp.replace_all_button.set_label("Replace All");
                    panel.update_replace_button_sensitivity();
                    panel.refresh_accessibility_state();
                    if selected.is_empty() {
                        panel.finish_replace_transaction();
                    } else if let Some(ref callback) = *imp.callbacks.replace_callback.borrow() {
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

    /// Exit preview mode: clear preview state and restore normal result display.
    pub fn exit_preview_mode(&self) {
        let imp = self.imp();
        self.invalidate_active_preview();
        imp.preview.preview_pending.set(false);
        imp.preview.preview_mode.set(false);
        self.release_superseded_preview(
            imp.preview.preview_outcome.take(),
            std::mem::take(&mut *imp.preview.checked_match_ids.borrow_mut()),
        );
        imp.replace_all_button.set_label("Replace All");
        self.restore_search_summary();
        self.update_replace_button_sensitivity();
        self.refresh_results_display();
        self.refresh_accessibility_state();
    }

    /// Update the "Replace All" / "Confirm Replace" button sensitivity.
    pub fn update_replace_button_sensitivity(&self) {
        let imp = self.imp();
        if imp.preview.preview_pending.get() || self.replace_transaction_claimed() {
            imp.replace_all_button.set_sensitive(false);
        } else if imp.preview.preview_mode.get() {
            imp.replace_all_button
                .set_sensitive(!imp.preview.checked_match_ids.borrow().is_empty());
        } else {
            // Empty replacement text is allowed (deletes matches).
            imp.replace_all_button
                .set_sensitive(imp.runtime.total_matches.get() > 0);
        }
        self.refresh_accessibility_state();
    }

    /// Cancel any pending or visible replace preview after search state changes.
    ///
    /// Advancing the generation prevents late background preview results from
    /// restoring stale replacements.
    pub(crate) fn invalidate_replace_preview_request(&self) {
        let imp = self.imp();
        if !imp.preview.preview_pending.get() && !imp.preview.preview_mode.get() {
            return;
        }
        self.invalidate_active_preview();
        imp.preview.preview_pending.set(false);
        imp.preview.preview_mode.set(false);
        self.release_superseded_preview(
            imp.preview.preview_outcome.take(),
            std::mem::take(&mut *imp.preview.checked_match_ids.borrow_mut()),
        );
        imp.replace_all_button.set_label("Replace All");
        self.restore_search_summary();
        self.refresh_results_display();
        self.refresh_accessibility_state();
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
        let imp = self.imp();
        ReplacePreviewFacts {
            generation: imp.preview.preview_generation.get(),
            pending: imp.preview.preview_pending.get(),
            query_spec: self.current_query_spec(),
        }
    }

    fn open_preview_generation(&self) -> u32 {
        let imp = self.imp();
        if let Some(cancel) = imp.preview.preview_cancel_token.take() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let generation = imp.preview.preview_generation.get().wrapping_add(1);
        imp.preview.preview_generation.set(generation);
        generation
    }

    /// Refresh visible and accessible confirmation feedback from accepted state.
    pub(crate) fn refresh_preview_summary(&self) {
        let imp = self.imp();
        let checked = imp.preview.checked_match_ids.borrow().len();
        let outcome = imp.preview.preview_outcome.borrow();
        let Some(outcome) = outcome.as_ref() else {
            return;
        };
        let generated = outcome.replacements.len();
        let omitted = outcome.omitted_eligible;
        let truncated = outcome
            .skipped
            .count(ReplacePreviewSkipReason::TruncatedSource);
        let stale_ranges = outcome
            .skipped
            .count(ReplacePreviewSkipReason::RegexRangeMismatch);
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
        imp.count_label.set_text(&summary);
        crate::ui::accessibility::set_labelled_description(
            &*imp.replace_all_button,
            &format!("Apply {checked} checked replacements"),
            &summary,
        );
        self.refresh_accessibility_state();
    }

    pub(super) fn restore_search_summary(&self) {
        let imp = self.imp();
        let total = imp.runtime.total_matches.get();
        let files = imp.runtime.total_files.get();
        if total == 0 {
            imp.count_label.set_text("No results found");
        } else if imp.runtime.result_capped.get() {
            imp.count_label
                .set_text("10,000+ results (truncated) — narrow your search");
            imp.count_label.add_css_class("warning");
        } else {
            imp.count_label.remove_css_class("warning");
            imp.count_label
                .set_text(&format!("{total} results in {files} files"));
        }
        crate::ui::accessibility::set_labelled_description(
            &*imp.replace_all_button,
            "Replace all matches",
            "Preview replacements before applying them",
        );
    }
}

fn replace_preview_budget() -> crate::model::content_search::ReplacePreviewBudget {
    #[cfg(feature = "test-utils")]
    if let Some(budget) = super::test_policy::replace_preview_budget_override() {
        return budget;
    }
    crate::model::content_search::ReplacePreviewBudget::default()
}

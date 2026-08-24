// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace All preview construction, apply selection, and undo-journal state.
//!
//! These methods stay on the widget because they mutate GTK state and preview
//! models, but isolating them here keeps streaming search execution separate
//! from replace/undo behavior.
//!
//! Control inversion: preview generation and checked-row selection both run on
//! worker threads, and both resume in a completion closure that revalidates the
//! attempt's [`ReplacePreviewTicket`] against the panel's live
//! [`ReplacePreviewFacts`]. A stale completion never publishes; it routes its
//! payload straight to bounded retirement instead.

use crate::model::content_search::{
    ReplacePreviewBudget, ReplacePreviewOutcome, ReplacePreviewSkipReason, Replacement,
    SearchMatchId, generate_replacement_preview_with_budget_and_cancel,
};
use crate::services::content_search::{
    MAX_REPLACE_UNDO_RETAINED_BYTES, ReplaceJournalFreshness, ReplaceUndoBackup,
    replace_undo_retained_byte_weight,
};
use crate::services::{json_store, search_backup};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::LushtextSearchPanel;
use super::policy::{ReplacePreviewFacts, ReplacePreviewTicket};

/// Latest plain-Rust preview request retained by the panel's single-flight worker.
pub(super) struct ReplacePreviewRequest {
    search_matches: std::sync::Arc<Vec<crate::model::content_search::SearchMatch>>,
    replacement_text: String,
    /// Identity of this attempt, validated as a unit when the worker resumes.
    ticket: ReplacePreviewTicket,
}

struct PersistedUndoStartupLoad {
    active_backup: Option<super::GuardedReplaceUndoBackup>,
}

struct PreviewRetirementTerminal(Arc<AtomicUsize>);

impl Drop for PreviewRetirementTerminal {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn retained_byte_weight(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

fn preview_reservation_weight(budget: ReplacePreviewBudget) -> u64 {
    retained_byte_weight(budget.max_bytes).saturating_add(retained_byte_weight(
        budget.max_rows.saturating_mul(
            std::mem::size_of::<Replacement>()
                .saturating_add(std::mem::size_of::<Option<usize>>())
                .saturating_add(std::mem::size_of::<SearchMatchId>()),
        ),
    ))
}

fn completed_preview_reservation_weight(outcome: &ReplacePreviewOutcome) -> u64 {
    retained_byte_weight(outcome.charged_bytes)
        .saturating_add(retained_byte_weight(
            outcome
                .replacements
                .capacity()
                .saturating_mul(std::mem::size_of::<Replacement>()),
        ))
        .saturating_add(retained_byte_weight(
            outcome
                .match_to_preview
                .capacity()
                .saturating_mul(std::mem::size_of::<Option<usize>>()),
        ))
}

#[cfg(feature = "test-utils")]
pub(crate) fn guard_undo_backup_on_worker(
    backup: ReplaceUndoBackup,
) -> Result<super::GuardedReplaceUndoBackup, ReplaceUndoBackup> {
    let weight = replace_undo_retained_byte_weight(&backup);
    crate::ui::plain_disposal::try_own_for_gtk(weight, backup)
}

pub(crate) fn own_reserved_undo_backup(
    mut reservation: crate::ui::plain_disposal::DisposalReservation,
    backup: ReplaceUndoBackup,
) -> super::GuardedReplaceUndoBackup {
    let retained_bytes = replace_undo_retained_byte_weight(&backup);
    debug_assert!(retained_bytes <= MAX_REPLACE_UNDO_RETAINED_BYTES);
    reservation.shrink_to(retained_bytes);
    reservation.own(backup)
}

impl LushtextSearchPanel {
    /// Reserve replacement ownership while every superseded guarded input remains installed.
    pub(crate) fn try_reserve_undo_replacement(
        &self,
        transient_input_weight: Option<u64>,
    ) -> Option<crate::ui::plain_disposal::DisposalReservation> {
        let current_weight = self
            .imp()
            .preview
            .undo_backup
            .borrow()
            .as_ref()
            .and_then(|backup| backup.reservation_weight());
        let replaces_guarded_owner = current_weight.is_some() || transient_input_weight.is_some();
        let replaced_weight = current_weight
            .unwrap_or(0)
            .saturating_add(transient_input_weight.unwrap_or(0));
        if replaces_guarded_owner {
            crate::ui::plain_disposal::try_reserve_replacement_for_gtk(
                MAX_REPLACE_UNDO_RETAINED_BYTES,
                replaced_weight,
            )
        } else {
            crate::ui::plain_disposal::try_reserve_for_gtk(MAX_REPLACE_UNDO_RETAINED_BYTES)
        }
    }
    /// Show the undo button (called after a successful replace).
    pub fn show_undo_button(&self) {
        let imp = self.imp();
        // Undo is time-sensitive recovery UI, so make the containing options
        // row visible instead of leaving the newly-shown button collapsed.
        imp.more_toggle.set_active(true);
        imp.undo_button.set_visible(true);
        self.refresh_accessibility_state();
    }

    /// Hide the undo button.
    pub fn hide_undo_button(&self) {
        self.imp().undo_button.set_visible(false);
        self.refresh_accessibility_state();
    }

    /// Store undo backup and persist it as the current retryable journal.
    ///
    /// # Panics
    ///
    /// Panics when the test process has deliberately saturated disposal admission.
    #[cfg(feature = "test-utils")]
    pub fn set_undo_backup(&self, backup: ReplaceUndoBackup) {
        let backup = Arc::new(
            guard_undo_backup_on_worker(backup)
                .expect("test undo backup should fit disposal admission"),
        );
        let (generation, retired) = self.set_undo_backup_in_memory(Arc::clone(&backup));
        self.save_undo_backup_on_disk(backup, retired, generation);
    }

    pub(crate) fn set_guarded_undo_backup(&self, backup: super::GuardedReplaceUndoBackup) {
        let backup = Arc::new(backup);
        let (generation, retired) = self.set_undo_backup_in_memory(Arc::clone(&backup));
        self.save_undo_backup_on_disk(backup, retired, generation);
    }

    /// Store undo backup after the replace service already wrote per-file journal entries.
    pub(crate) fn set_persisted_guarded_undo_backup(
        &self,
        backup: super::GuardedReplaceUndoBackup,
    ) {
        let (_, retired) = self.set_undo_backup_in_memory(Arc::new(backup));
        Self::retire_undo_backup_off_main(retired);
    }

    /// Install a service-persisted backup through the widget-test compatibility surface.
    ///
    /// # Panics
    ///
    /// Panics when the test process has deliberately saturated disposal admission.
    #[cfg(feature = "test-utils")]
    pub fn set_persisted_undo_backup(&self, backup: ReplaceUndoBackup) {
        self.set_persisted_guarded_undo_backup(
            guard_undo_backup_on_worker(backup)
                .expect("test persisted undo backup should fit disposal admission"),
        );
    }

    /// Reserve the journal generation before Replace All can commit on a worker.
    pub(crate) fn reserve_undo_backup_generation(&self) -> u32 {
        self.imp()
            .preview
            .undo_backup_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1)
    }

    /// Claim the sole UI Replace All apply/undo transaction and its journal generation.
    pub(crate) fn begin_replace_transaction(&self) -> Option<ReplaceJournalFreshness> {
        let imp = self.imp();
        if imp.preview.replace_transaction_pending.replace(true) {
            return None;
        }
        let generation = self.reserve_undo_backup_generation();
        imp.preview
            .replace_transaction_generation
            .set(Some(generation));
        imp.replace_all_button.set_sensitive(false);
        imp.undo_button.set_sensitive(false);
        self.refresh_accessibility_state();
        Some(ReplaceJournalFreshness::new(
            imp.preview.undo_backup_generation.clone(),
            generation,
        ))
    }

    /// Release the serialized transaction after its UI and journal state are published.
    pub(crate) fn finish_replace_transaction(&self) {
        let imp = self.imp();
        imp.preview.replace_transaction_generation.set(None);
        imp.preview.replace_transaction_pending.set(false);
        imp.undo_button.set_sensitive(self.has_undo_backup());
        self.update_replace_button_sensitivity();
        self.refresh_accessibility_state();
    }

    /// Hand the preview selection's reservation to the durable apply workflow.
    pub(crate) fn take_replace_transaction(&self) -> Option<ReplaceJournalFreshness> {
        let imp = self.imp();
        if !imp.preview.replace_transaction_pending.get() {
            return None;
        }
        let generation = imp.preview.replace_transaction_generation.take()?;
        Some(ReplaceJournalFreshness::new(
            imp.preview.undo_backup_generation.clone(),
            generation,
        ))
    }

    /// Retire the prior recovery projection once a newer Replace All is committed to start.
    pub(crate) fn supersede_prior_undo_for_replace(&self) {
        let retired = self.imp().preview.undo_backup.replace(None);
        self.hide_undo_button();
        Self::retire_undo_backup_off_main(retired);
    }

    /// Install a service-persisted journal only for its pre-worker reservation.
    pub(crate) fn set_persisted_undo_backup_for_generation(
        &self,
        backup: super::GuardedReplaceUndoBackup,
        generation: u32,
    ) -> bool {
        if self
            .imp()
            .preview
            .undo_backup_generation
            .load(Ordering::Acquire)
            != generation
        {
            drop(backup);
            return false;
        }
        let retired = self
            .imp()
            .preview
            .undo_backup
            .replace(Some(Arc::new(backup)));
        Self::retire_undo_backup_off_main(retired);
        self.refresh_accessibility_state();
        true
    }

    /// Clear an empty service result only if its reservation is still current.
    pub(crate) fn clear_undo_backup_for_generation(&self, generation: u32) -> bool {
        if self
            .imp()
            .preview
            .undo_backup_generation
            .load(Ordering::Acquire)
            != generation
        {
            return false;
        }
        self.clear_undo_backup();
        true
    }

    fn set_undo_backup_in_memory(
        &self,
        backup: Arc<super::GuardedReplaceUndoBackup>,
    ) -> (u32, Option<Arc<super::GuardedReplaceUndoBackup>>) {
        self.imp().preview.undo_capacity_wakeup.cancel();
        let previous = self
            .imp()
            .preview
            .undo_backup_generation
            .fetch_add(1, Ordering::AcqRel);
        let retired = self.imp().preview.undo_backup.replace(Some(backup));
        self.refresh_accessibility_state();
        (previous.wrapping_add(1), retired)
    }

    /// Restore a crash-interrupted active journal, or clean inactive stale state.
    pub(crate) fn load_persisted_undo_backup(&self) {
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let Some(reservation) =
            crate::ui::plain_disposal::try_reserve_for_gtk(MAX_REPLACE_UNDO_RETAINED_BYTES)
        else {
            tracing::warn!("Persisted Replace All undo backup deferred by disposal capacity");
            self.schedule_persisted_undo_backup_retry(observed_epoch);
            return;
        };
        let data_dir = json_store::data_dir();
        let generation = self
            .imp()
            .preview
            .undo_backup_generation
            .load(Ordering::Acquire);
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        let callback_generation_counter = generation_counter.clone();
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if generation_counter.load(Ordering::Acquire) != generation {
                    return Ok(PersistedUndoStartupLoad {
                        active_backup: None,
                    });
                }
                let recovery = search_backup::load_recovering(&data_dir);
                if recovery.active {
                    let backup = recovery.backup;
                    let active_backup = own_reserved_undo_backup(reservation, backup);
                    return Ok(PersistedUndoStartupLoad {
                        active_backup: Some(active_backup),
                    });
                }
                let mut diagnostics = recovery.diagnostics;
                diagnostics.extend(search_backup::cleanup_stale(&data_dir).diagnostics);
                report_startup_diagnostics(&diagnostics);
                Ok::<PersistedUndoStartupLoad, anyhow::Error>(PersistedUndoStartupLoad {
                    active_backup: None,
                })
            },
            move |panel, result| match result {
                Ok(load) => {
                    if callback_generation_counter.load(Ordering::Acquire) != generation {
                        return;
                    }
                    if let Some(backup) = load.active_backup {
                        panel.set_persisted_guarded_undo_backup(backup);
                        panel.show_undo_button();
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load persisted replace backup: {e}");
                }
            },
        );
    }

    fn schedule_persisted_undo_backup_retry(&self, observed_epoch: u64) {
        let panel_weak = self.downgrade();
        self.imp()
            .preview
            .undo_capacity_wakeup
            .arm(observed_epoch, move || {
                if let Some(panel) = panel_weak.upgrade() {
                    panel.load_persisted_undo_backup();
                }
            });
    }

    /// Clear undo backup and hide the undo button.
    pub(crate) fn clear_undo_backup(&self) {
        self.imp().preview.undo_capacity_wakeup.cancel();
        let generation = self
            .imp()
            .preview
            .undo_backup_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let retired = self.imp().preview.undo_backup.replace(None);
        self.hide_undo_button();
        self.delete_undo_backup_on_disk(generation, retired);
        self.refresh_accessibility_state();
    }

    /// Clear the durable undo journal through its production generation guard.
    #[cfg(feature = "test-utils")]
    pub fn clear_undo_backup_for_test(&self) {
        self.clear_undo_backup();
    }

    /// Reserve one production journal generation for ordering regressions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn reserve_undo_backup_generation_for_test(&self) -> u32 {
        self.reserve_undo_backup_generation()
    }

    /// Install a service-persisted backup under a test reservation.
    ///
    /// # Panics
    ///
    /// Panics when the test process has deliberately saturated disposal admission.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn set_persisted_undo_backup_for_generation_for_test(
        &self,
        backup: ReplaceUndoBackup,
        generation: u32,
    ) -> bool {
        self.set_persisted_undo_backup_for_generation(
            guard_undo_backup_on_worker(backup)
                .expect("test persisted undo backup should fit disposal admission"),
            generation,
        )
    }

    /// Claim the production transaction gate for widget race tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn begin_replace_transaction_for_test(&self) -> Option<u32> {
        self.begin_replace_transaction()
            .map(|freshness| freshness.expected())
    }

    /// Release the production transaction gate for widget race tests.
    #[cfg(feature = "test-utils")]
    pub fn finish_replace_transaction_for_test(&self) {
        self.finish_replace_transaction();
    }

    fn save_undo_backup_on_disk(
        &self,
        backup: Arc<super::GuardedReplaceUndoBackup>,
        retired: Option<Arc<super::GuardedReplaceUndoBackup>>,
        generation: u32,
    ) {
        let data_dir = json_store::data_dir();
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        spawn_blocking_then(
            self.clone(),
            move || {
                let _retired = retired;
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_undo_backup_disk();
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if generation_counter.load(Ordering::Acquire) != generation {
                    return Ok(());
                }
                search_backup::save(&data_dir, &backup)
            },
            move |_panel, result| {
                if let Err(e) = result {
                    tracing::error!("Failed to persist replace backup: {e}");
                }
            },
        );
    }

    fn delete_undo_backup_on_disk(
        &self,
        generation: u32,
        retired: Option<Arc<super::GuardedReplaceUndoBackup>>,
    ) {
        let data_dir = json_store::data_dir();
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        spawn_blocking_then(
            self.clone(),
            move || {
                let _retired = retired;
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_undo_backup_disk();
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if generation_counter.load(Ordering::Acquire) != generation {
                    return Ok(());
                }
                search_backup::delete(&data_dir)
            },
            move |_panel, result| {
                if let Err(e) = result {
                    tracing::warn!("Failed to delete replace backup after undo: {e}");
                }
            },
        );
    }

    fn retire_undo_backup_off_main(retired: Option<Arc<super::GuardedReplaceUndoBackup>>) {
        let Some(retired) = retired else {
            return;
        };
        drop(retired);
    }

    /// Whether the panel is in preview mode.
    #[must_use]
    pub fn is_preview_mode(&self) -> bool {
        self.imp().preview.preview_mode.get()
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
                    panel.spawn_guarded_preview_retirement(outcome, HashSet::new());
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
        if outcome.is_none() && checked_match_ids.is_empty() {
            return;
        }
        if let Some(outcome) = outcome {
            self.imp().preview.preview_worker_running.set(true);
            self.spawn_guarded_preview_retirement(outcome, checked_match_ids);
        } else {
            drop(checked_match_ids);
        }
    }

    fn spawn_guarded_preview_retirement(
        &self,
        outcome: crate::ui::plain_disposal::DisposalOwned<ReplacePreviewOutcome>,
        checked_match_ids: HashSet<SearchMatchId>,
    ) {
        let imp = self.imp();
        imp.preview
            .preview_retirement_jobs
            .set(imp.preview.preview_retirement_jobs.get().saturating_add(1));
        let retirement_pending = imp.preview.preview_retirement_pending.clone();
        retirement_pending.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let terminal = PreviewRetirementTerminal(retirement_pending);
        let panel_weak = glib::thread_guard::ThreadGuard::new(self.downgrade());
        drop(checked_match_ids);
        drop(outcome.with_disposal_terminal(move || {
            drop(terminal);
            glib::idle_add_once(move || {
                let panel_weak = panel_weak.into_inner();
                if let Some(panel) = panel_weak.upgrade() {
                    panel.finish_preview_worker();
                }
            });
        }));
    }

    fn spawn_selected_preview_retirement(&self, selected: super::GuardedReplacements) {
        let imp = self.imp();
        imp.preview
            .preview_retirement_jobs
            .set(imp.preview.preview_retirement_jobs.get().saturating_add(1));
        let retirement_pending = imp.preview.preview_retirement_pending.clone();
        retirement_pending.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let terminal = PreviewRetirementTerminal(retirement_pending);
        let panel_weak = glib::thread_guard::ThreadGuard::new(self.downgrade());
        drop(selected.with_disposal_terminal(move || {
            drop(terminal);
            glib::idle_add_once(move || {
                let panel_weak = panel_weak.into_inner();
                if let Some(panel) = panel_weak.upgrade() {
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
        if imp.preview.replace_transaction_pending.get() || !imp.preview.preview_mode.get() {
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
                        if imp.preview.replace_transaction_generation.get().is_some() {
                            panel.finish_replace_transaction();
                        }
                    } else {
                        panel.spawn_selected_preview_retirement(selected);
                        panel.finish_replace_transaction();
                        return;
                    }
                    panel.finish_preview_worker();
                } else {
                    panel.spawn_selected_preview_retirement(selected);
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
        if imp.preview.preview_pending.get() || imp.preview.replace_transaction_pending.get() {
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

fn report_startup_diagnostics(
    diagnostics: &[crate::services::recovery_metadata::RecoveryDiagnostic],
) {
    const DETAIL_LIMIT: usize = 8;
    for diagnostic in diagnostics.iter().take(DETAIL_LIMIT) {
        tracing::warn!(
            "Replace undo backup startup diagnostic: {}",
            diagnostic.summary()
        );
    }
    if diagnostics.len() > DETAIL_LIMIT {
        tracing::warn!(
            "Replace undo backup startup produced {} additional diagnostics",
            diagnostics.len() - DETAIL_LIMIT
        );
    }
}

fn replace_preview_budget() -> ReplacePreviewBudget {
    #[cfg(feature = "test-utils")]
    if let Some(budget) = super::test_policy::replace_preview_budget_override() {
        return budget;
    }
    ReplacePreviewBudget::default()
}

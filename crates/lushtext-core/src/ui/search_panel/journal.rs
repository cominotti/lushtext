// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the Replace All undo journal.
//!
//! `journal` is the coordination role for maintaining a durable,
//! generation-guarded record that a later stage of the same workflow reads back.
//! This module owns the whole life of the Replace All undo journal: the single
//! apply/undo transaction gate, the generation reservation, the
//! generation-guarded in-memory install and clear, the worker-side disk save and
//! delete, startup recovery with stale-record cleanup, the disposal-capacity
//! retry, the undo affordance, and the hand-back to the window's restore
//! workflow. It is deliberately not `retirement`, which destroys a payload the
//! workflow is finished with: this module preserves one so a later stage can
//! restore from it.
//!
//! # Owned state
//!
//! Three fields on `imp::SearchPreviewState` are touched by both Replace All
//! coordination modules, and this module is the single owner of all three. The
//! preview half never reads them directly; it calls the named crossing
//! predicates below.
//!
//! | Field | Crossing operation `replace_execution` calls |
//! | --- | --- |
//! | `replace_transaction_pending` | [`LushtextSearchPanel::replace_transaction_claimed`] |
//! | `replace_transaction_generation` | [`LushtextSearchPanel::replace_transaction_generation_reserved`] |
//! | `undo_backup_generation` | none; it crosses only to the window, inside a `ReplaceJournalFreshness` |
//!
//! # Control inversions
//!
//! 1. **Disk save and disk delete.** Both return as soon as the worker is
//!    dispatched. Control resumes in a `spawn_blocking_then` completion closure
//!    that only logs, because the in-memory journal was already published under
//!    its generation guard before the worker started.
//! 2. **Startup recovery, deferred by capacity.** When disposal admission
//!    refuses the recovery reservation, control resumes in the
//!    `undo_capacity_wakeup` closure, which re-enters
//!    [`LushtextSearchPanel::load_persisted_undo_backup`] from the top.
//! 3. **Startup recovery, on the worker.** The recovery read returns through a
//!    completion closure that re-checks the journal generation before installing
//!    the recovered backup and revealing the undo affordance.
//! 4. **Hand-back.** [`LushtextSearchPanel::hand_back_undo_backup`] ends by
//!    invoking the window's undo callback, so control leaves the panel. It
//!    returns only through [`LushtextSearchPanel::begin_undo_restore`] and
//!    [`LushtextSearchPanel::finish_undo_restore`].

use crate::services::content_search::{
    MAX_REPLACE_UNDO_RETAINED_BYTES, ReplaceJournalFreshness, ReplaceUndoBackup,
    replace_undo_retained_byte_weight,
};
use crate::services::{json_store, search_backup};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::LushtextSearchPanel;
use super::policy::{ReplaceApplyCounts, UndoReservationPlan};

/// Startup-recovery result carried back from the worker to GTK.
struct PersistedUndoStartupLoad {
    active_backup: Option<super::GuardedReplaceUndoBackup>,
}

/// Outcome of the window's attempt to claim the panel for one undo restore.
///
/// The undo restore has two independent admission steps — the single
/// apply/undo transaction and the disposal reservation — and the window needs
/// to tell them apart because only the second is a memory-pressure message.
/// Reifying the outcome keeps both guards, and the affordance restoration each
/// refusal owes the user, on the panel side where the rest of the journal lives.
pub(crate) enum UndoRestoreClaim {
    /// The panel owns the transaction, and this reservation covers the restore.
    Claimed(crate::ui::plain_disposal::DisposalReservation),
    /// Another apply or undo owns the transaction; the affordance is restored.
    TransactionBusy,
    /// Disposal capacity refused the reservation; the affordance is restored.
    CapacityDeferred,
}

/// Take ownership of a raw undo journal under a reservation made before the worker.
///
/// The window reserves disposal capacity on GTK, hands the reservation to a
/// worker, and only there learns the real journal it must guard. This is the
/// operation that joins the two, shrinking the worst-case reservation to the
/// journal's measured retained weight.
pub(crate) fn own_undo_journal_payload(
    mut reservation: crate::ui::plain_disposal::DisposalReservation,
    backup: ReplaceUndoBackup,
) -> super::GuardedReplaceUndoBackup {
    let retained_bytes = replace_undo_retained_byte_weight(&backup);
    debug_assert!(retained_bytes <= MAX_REPLACE_UNDO_RETAINED_BYTES);
    reservation.shrink_to(retained_bytes);
    reservation.own(backup)
}

/// Guard a raw journal through the widget-test compatibility surface.
#[cfg(feature = "test-utils")]
pub(crate) fn guard_undo_backup_on_worker(
    backup: ReplaceUndoBackup,
) -> Result<super::GuardedReplaceUndoBackup, ReplaceUndoBackup> {
    let weight = replace_undo_retained_byte_weight(&backup);
    crate::ui::plain_disposal::try_own_for_gtk(weight, backup)
}

/// Release this panel's reference to a superseded undo journal.
///
/// The destruction itself is **not** performed here. Releasing the last
/// `Arc` runs `DisposalOwned`'s drop, which submits the document-sized payload
/// to the disposal lane rather than destroying it inline, so a superseded
/// journal never runs its nested destructor on the GTK thread. This function's
/// own job is only to give up the panel's reference.
fn release_superseded_undo_journal(retired: Option<Arc<super::GuardedReplaceUndoBackup>>) {
    let Some(retired) = retired else {
        return;
    };
    drop(retired);
}

impl LushtextSearchPanel {
    /// Whether one Replace All apply or undo transaction owns journal mutation.
    ///
    /// The crossing predicate for `replace_transaction_pending`, which this
    /// module owns. `replace_execution` reads the transaction only through here.
    pub(super) fn replace_transaction_claimed(&self) -> bool {
        self.imp().preview.replace_transaction_pending.get()
    }

    /// Whether the claimed transaction still holds its reserved journal generation.
    ///
    /// The crossing predicate for `replace_transaction_generation`. The durable
    /// apply consumes that generation with [`Self::take_replace_transaction`],
    /// so a still-reserved generation means the window never took the handoff
    /// and the preview half must release the transaction itself.
    pub(super) fn replace_transaction_generation_reserved(&self) -> bool {
        self.imp()
            .preview
            .replace_transaction_generation
            .get()
            .is_some()
    }

    /// Reserve replacement ownership while every superseded guarded input remains installed.
    pub(crate) fn try_reserve_undo_replacement(
        &self,
        transient_input_weight: Option<u64>,
    ) -> Option<crate::ui::plain_disposal::DisposalReservation> {
        let installed_weight = self
            .imp()
            .preview
            .undo_backup
            .borrow()
            .as_ref()
            .and_then(|backup| backup.reservation_weight());
        match super::policy::plan_undo_reservation(installed_weight, transient_input_weight) {
            UndoReservationPlan::Replacement { replaced_weight } => {
                crate::ui::plain_disposal::try_reserve_replacement_for_gtk(
                    MAX_REPLACE_UNDO_RETAINED_BYTES,
                    replaced_weight,
                )
            }
            UndoReservationPlan::Fresh => {
                crate::ui::plain_disposal::try_reserve_for_gtk(MAX_REPLACE_UNDO_RETAINED_BYTES)
            }
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
        release_superseded_undo_journal(retired);
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

    /// Claim the panel for one undo restore, or restore the affordance and refuse.
    ///
    /// The window's undo stage starts here instead of reading and mutating panel
    /// state itself: this claims the single apply/undo transaction, reserves the
    /// disposal capacity the restore needs, and — on either refusal — puts the
    /// undo affordance back so the user can retry. The refusal is named, because
    /// only the capacity case is a memory-pressure message.
    pub(crate) fn begin_undo_restore(&self) -> UndoRestoreClaim {
        if self.begin_replace_transaction().is_none() {
            self.show_undo_button();
            return UndoRestoreClaim::TransactionBusy;
        }
        let Some(reservation) = self.try_reserve_undo_replacement(None) else {
            self.finish_replace_transaction();
            self.show_undo_button();
            return UndoRestoreClaim::CapacityDeferred;
        };
        UndoRestoreClaim::Claimed(reservation)
    }

    /// Publish the undo restore's outcome and release the transaction.
    ///
    /// A remaining journal is reinstalled and its affordance revealed so the
    /// user can retry the files that were not restored; a fully consumed journal
    /// is cleared, which also deletes it from disk. Either way the transaction is
    /// released last, exactly as the apply stage does.
    pub(crate) fn finish_undo_restore(&self, remaining: Option<super::GuardedReplaceUndoBackup>) {
        if let Some(remaining) = remaining {
            self.set_guarded_undo_backup(remaining);
            self.show_undo_button();
        } else {
            self.clear_undo_backup();
        }
        self.finish_replace_transaction();
    }

    /// Retire the prior recovery projection once a newer Replace All is committed to start.
    pub(crate) fn supersede_prior_undo_for_replace(&self) {
        let retired = self.imp().preview.undo_backup.replace(None);
        self.hide_undo_button();
        release_superseded_undo_journal(retired);
    }

    /// Record the counts the most recent durable apply reported.
    ///
    /// Observation only: the window already published the user-visible message
    /// from the same result, and this makes that outcome readable from the
    /// workflow's evidence surface instead of only from the status bar.
    pub(crate) fn record_replace_apply_counts(&self, counts: ReplaceApplyCounts) {
        self.imp().preview.last_apply_counts.set(Some(counts));
    }

    /// Publish a service-persisted journal and reveal its undo affordance.
    ///
    /// Returns whether the journal was still current. A superseded generation
    /// leaves the affordance hidden and abandons the payload, because a newer
    /// Replace All or undo already owns the journal.
    pub(crate) fn publish_undo_journal_for_generation(
        &self,
        backup: super::GuardedReplaceUndoBackup,
        generation: u32,
    ) -> bool {
        if !self.set_persisted_undo_backup_for_generation(backup, generation) {
            return false;
        }
        self.show_undo_button();
        true
    }

    /// Install a service-persisted journal only for its pre-worker reservation.
    pub(crate) fn set_persisted_undo_backup_for_generation(
        &self,
        backup: super::GuardedReplaceUndoBackup,
        generation: u32,
    ) -> bool {
        if !super::policy::journal_generation_is_current(
            self.imp()
                .preview
                .undo_backup_generation
                .load(Ordering::Acquire),
            generation,
        ) {
            drop(backup);
            return false;
        }
        let retired = self
            .imp()
            .preview
            .undo_backup
            .replace(Some(Arc::new(backup)));
        release_superseded_undo_journal(retired);
        self.refresh_accessibility_state();
        true
    }

    /// Clear an empty service result only if its reservation is still current.
    pub(crate) fn clear_undo_backup_for_generation(&self, generation: u32) -> bool {
        if !super::policy::journal_generation_is_current(
            self.imp()
                .preview
                .undo_backup_generation
                .load(Ordering::Acquire),
            generation,
        ) {
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
        self.charge_journal_disk_job();
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if !super::policy::journal_generation_is_current(
                    generation_counter.load(Ordering::Acquire),
                    generation,
                ) {
                    return Ok(PersistedUndoStartupLoad {
                        active_backup: None,
                    });
                }
                let recovery = search_backup::load_recovering(&data_dir);
                if recovery.active {
                    let backup = recovery.backup;
                    let active_backup = own_undo_journal_payload(reservation, backup);
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
            move |panel, result| {
                panel.finish_journal_disk_job();
                match result {
                    Ok(load) => {
                        if !super::policy::journal_generation_is_current(
                            callback_generation_counter.load(Ordering::Acquire),
                            generation,
                        ) {
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

    /// Charge one dispatched journal disk job to the evidence counters.
    fn charge_journal_disk_job(&self) {
        let preview = &self.imp().preview;
        preview
            .journal_disk_jobs
            .set(preview.journal_disk_jobs.get().saturating_add(1));
        preview
            .journal_disk_jobs_in_flight
            .set(preview.journal_disk_jobs_in_flight.get().saturating_add(1));
    }

    /// Release one journal disk job as its GTK completion runs.
    fn finish_journal_disk_job(&self) {
        let preview = &self.imp().preview;
        preview
            .journal_disk_jobs_in_flight
            .set(preview.journal_disk_jobs_in_flight.get().saturating_sub(1));
    }

    fn save_undo_backup_on_disk(
        &self,
        backup: Arc<super::GuardedReplaceUndoBackup>,
        retired: Option<Arc<super::GuardedReplaceUndoBackup>>,
        generation: u32,
    ) {
        let data_dir = json_store::data_dir();
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        self.charge_journal_disk_job();
        spawn_blocking_then(
            self.clone(),
            move || {
                let _retired = retired;
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_undo_backup_disk();
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if !super::policy::journal_generation_is_current(
                    generation_counter.load(Ordering::Acquire),
                    generation,
                ) {
                    return Ok(());
                }
                // Shrink rather than delete-then-rebuild: the only production
                // caller is a partial undo, whose unrestored files still hold
                // Replace All output. `save`'s rebuild window would leave them
                // with no durable rollback copy at all. `shrink_journal_to`
                // falls back to a full rewrite when the on-disk journal is not a
                // superset, so this is correct for every journal state.
                search_backup::shrink_journal_to(&data_dir, &backup)
            },
            move |panel, result| {
                panel.finish_journal_disk_job();
                if let Err(e) = result {
                    tracing::error!("Failed to persist replace backup: {e}");
                    // A durability failure on the only rollback copy must not be
                    // a log line the user never sees: undo still works in this
                    // session, but it will not survive a restart.
                    panel.publish_journal_message(
                        "Undo journal could not be saved; undo may not survive a restart",
                        crate::services::notifications::NotificationSeverity::Warning,
                    );
                }
            },
        );
    }

    /// Surface one journal message through the window's status lane.
    fn publish_journal_message(
        &self,
        message: &str,
        severity: crate::services::notifications::NotificationSeverity,
    ) {
        if let Some(ref callback) = *self.imp().callbacks.message_callback.borrow() {
            callback(message, severity);
        }
    }

    fn delete_undo_backup_on_disk(
        &self,
        generation: u32,
        retired: Option<Arc<super::GuardedReplaceUndoBackup>>,
    ) {
        let data_dir = json_store::data_dir();
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        self.charge_journal_disk_job();
        spawn_blocking_then(
            self.clone(),
            move || {
                let _retired = retired;
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_undo_backup_disk();
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if !super::policy::journal_generation_is_current(
                    generation_counter.load(Ordering::Acquire),
                    generation,
                ) {
                    return Ok(());
                }
                search_backup::delete(&data_dir)
            },
            move |panel, result| {
                panel.finish_journal_disk_job();
                if let Err(e) = result {
                    tracing::warn!("Failed to delete replace backup after undo: {e}");
                }
            },
        );
    }

    /// Hand the current undo journal back to the window's restore workflow.
    ///
    /// Replace stage 5 in one operation: refuse while the single apply
    /// transaction is still claimed, read the published backup, retract the
    /// affordance so the same journal cannot be handed back twice, and invoke
    /// the window callback. A missing callback or missing backup leaves live
    /// state untouched.
    pub(super) fn hand_back_undo_backup(&self) {
        let imp = self.imp();
        if self.replace_transaction_claimed() {
            return;
        }
        let Some(backup) = imp.preview.undo_backup.borrow().clone() else {
            return;
        };
        self.hide_undo_button();
        if let Some(ref callback) = *imp.callbacks.undo_callback.borrow() {
            callback(backup);
        }
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

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
//! [`JournalState`] owns the Replace All transaction phase
//! ([`ReplaceTransactionPhase`]), the installed undo journal
//! ([`UndoBackupSlot`]), the journal generation counter, the disposal capacity
//! wakeup, and the disk job/apply-count evidence. `replace_execution` reads
//! the transaction gate only through the named crossing predicates below.
//!
//! | State | Crossing operation `replace_execution` calls |
//! | --- | --- |
//! | `ReplaceTransactionPhase::Claimed` | [`LushtextSearchPanel::replace_transaction_claimed`] |
//! | `reserved_generation` | [`LushtextSearchPanel::replace_transaction_generation_reserved`] |
//! | `undo_backup_generation` | none; it crosses only to the window, inside a `ReplaceJournalFreshness` |
//!
//! # Control inversions
//!
//! 1. **Disk save and disk delete.** Both return as soon as the worker is
//!    dispatched. Control resumes in a `spawn_blocking_then` completion closure
//!    that only logs, because the in-memory journal was already published under
//!    its generation guard before the worker started.
//! 2. **Startup recovery, deferred by capacity or by a claimed transaction.**
//!    When disposal admission refuses the recovery reservation, control
//!    resumes in the `undo_capacity_wakeup` closure; while this panel's
//!    transaction is claimed, it resumes in
//!    [`LushtextSearchPanel::finish_replace_transaction`]. Both re-enter
//!    [`LushtextSearchPanel::load_persisted_undo_backup`] from the top, and a
//!    run's admission retires either wait in every panel.
//! 3. **Startup recovery, on the worker.** The recovery read returns through a
//!    completion closure that re-checks the journal generation before installing
//!    the recovered backup and revealing the undo affordance.
//! 4. **Hand-back.** [`LushtextSearchPanel::hand_back_undo_backup`] ends by
//!    invoking the window's undo callback, so control leaves the panel. It
//!    returns only through [`LushtextSearchPanel::begin_undo_restore`] and
//!    [`LushtextSearchPanel::finish_undo_restore`].

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use lushtext_model::values::{ReplaceTransactionGeneration, UndoBackupGeneration};
use lushtext_services::search::content_search::{
    MAX_REPLACE_UNDO_RETAINED_BYTES, ReplaceJournalFreshness, ReplaceUndoBackup,
    replace_undo_retained_byte_weight,
};
use lushtext_services::{persistence::json_store, search::search_backup};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::LushtextSearchPanel;
use super::policy::{ReplaceApplyCounts, UndoReservationPlan};

/// Transaction-gate phase owned by `journal`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ReplaceTransactionPhase {
    #[default]
    Idle,
    Claimed {
        reserved_generation: Option<ReplaceTransactionGeneration>,
        resume_persisted_load: bool,
    },
}

/// In-memory undo backup slot owned by `journal`.
#[derive(Default)]
pub enum UndoBackupSlot {
    #[default]
    Empty,
    Installed(Arc<super::GuardedReplaceUndoBackup>),
}

impl UndoBackupSlot {
    #[must_use]
    pub fn is_installed(&self) -> bool {
        matches!(self, Self::Installed(_))
    }

    #[must_use]
    pub fn as_backup(&self) -> Option<&Arc<super::GuardedReplaceUndoBackup>> {
        match self {
            Self::Empty => None,
            Self::Installed(backup) => Some(backup),
        }
    }

    fn take_backup(&mut self) -> Option<Arc<super::GuardedReplaceUndoBackup>> {
        match std::mem::take(self) {
            Self::Empty => None,
            Self::Installed(backup) => Some(backup),
        }
    }
}

/// Recorded apply counts from the most recent durable Replace All apply.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum LastApplyCounts {
    #[default]
    None,
    Recorded(ReplaceApplyCounts),
}

impl LastApplyCounts {
    pub(super) fn into_option(self) -> Option<ReplaceApplyCounts> {
        match self {
            Self::None => None,
            Self::Recorded(counts) => Some(counts),
        }
    }
}

/// Durable undo journal and transaction-gate state owned by `journal`.
#[derive(Default)]
pub struct JournalState {
    pub(super) transaction: Cell<ReplaceTransactionPhase>,
    pub(super) undo_backup: RefCell<UndoBackupSlot>,
    pub(super) undo_backup_generation: Arc<AtomicU32>,
    pub(super) undo_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
    pub(super) disk_jobs: Cell<u64>,
    pub(super) disk_jobs_in_flight: Cell<usize>,
    pub(super) last_apply_counts: Cell<LastApplyCounts>,
}

impl JournalState {
    fn current_generation(&self) -> UndoBackupGeneration {
        UndoBackupGeneration::new(self.undo_backup_generation.load(Ordering::Acquire))
    }

    /// Install or clear the in-memory undo backup directly (used by widget tests).
    #[cfg(feature = "test-utils")]
    pub fn replace_undo_backup(&self, backup: Option<Arc<super::GuardedReplaceUndoBackup>>) {
        let next = backup.map_or(UndoBackupSlot::Empty, UndoBackupSlot::Installed);
        let retired = self.undo_backup.replace(next).take_backup();
        release_superseded_undo_journal(retired);
    }

    /// Clone the currently installed undo backup handle, if any.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn undo_backup(&self) -> Option<Arc<super::GuardedReplaceUndoBackup>> {
        self.undo_backup.borrow().as_backup().cloned()
    }
}

fn generation_is_current(counter: &Arc<AtomicU32>, expected: UndoBackupGeneration) -> bool {
    super::policy::journal_generation_is_current(
        UndoBackupGeneration::new(counter.load(Ordering::Acquire)),
        expected,
    )
}

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

thread_local! {
    /// Every search panel of the application, weakly: all windows run on the
    /// GTK thread, and the undo journal is one per application (design D3a).
    static APPLICATION_PANELS: std::cell::RefCell<Vec<glib::WeakRef<LushtextSearchPanel>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The application's other live search panels.
fn other_application_panels(
    this: &LushtextSearchPanel,
) -> impl Iterator<Item = LushtextSearchPanel> {
    let panels: Vec<LushtextSearchPanel> = APPLICATION_PANELS.with(|panels| {
        panels
            .borrow()
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .filter(|panel| panel != this)
            .collect()
    });
    panels.into_iter()
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
    /// The crossing predicate for `transaction`, which this
    /// module owns. `replace_execution` reads the transaction only through here.
    pub(super) fn replace_transaction_claimed(&self) -> bool {
        matches!(
            self.imp().journal.transaction.get(),
            ReplaceTransactionPhase::Claimed { .. }
        )
    }

    /// Whether the claimed transaction still holds its reserved journal generation.
    ///
    /// The crossing predicate for `reserved_generation`. The durable
    /// apply consumes that generation with [`Self::take_replace_transaction`],
    /// so a still-reserved generation means the window never took the handoff
    /// and the preview half must release the transaction itself.
    pub(super) fn replace_transaction_generation_reserved(&self) -> bool {
        matches!(
            self.imp().journal.transaction.get(),
            ReplaceTransactionPhase::Claimed {
                reserved_generation: Some(_),
                ..
            }
        )
    }

    /// Reserve replacement ownership while every superseded guarded input remains installed.
    ///
    /// A Replace All (the caller with a transient input, its selection) credits
    /// every window's installed journal, not only this panel's: its admission
    /// retires all of them in the same turn (`supersede_prior_undo_for_replace`),
    /// so a reservation that left them charged could wait for capacity that only
    /// that admission frees (`replace-all-confirm-disposal-lane`). An Undo
    /// replaces only this panel's journal.
    pub(super) fn try_reserve_undo_replacement(
        &self,
        transient_input_weight: Option<u64>,
    ) -> Option<crate::ui::plain_disposal::DisposalReservation> {
        let own_weight = self.installed_undo_reservation_weight();
        let installed_weight = if transient_input_weight.is_some() {
            super::policy::combined_undo_weight(
                std::iter::once(own_weight).chain(
                    other_application_panels(self)
                        .map(|panel| panel.installed_undo_reservation_weight()),
                ),
            )
        } else {
            own_weight
        };
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

    /// The disposal weight this panel's installed journal holds, if any.
    fn installed_undo_reservation_weight(&self) -> Option<u64> {
        self.imp()
            .journal
            .undo_backup
            .borrow()
            .as_backup()
            .and_then(|backup| backup.reservation_weight())
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
    pub(crate) fn reserve_undo_backup_generation(&self) -> UndoBackupGeneration {
        UndoBackupGeneration::new(
            self.imp()
                .journal
                .undo_backup_generation
                .fetch_add(1, Ordering::AcqRel)
                .wrapping_add(1),
        )
    }

    /// Claim the sole UI Replace All apply/undo transaction and its journal generation.
    ///
    /// The journal is one per application (design D3a): while another
    /// window's panel holds its transaction (a run, its rollback or an Undo),
    /// this one is refused, with a status message, and nothing waits.
    pub(crate) fn begin_replace_transaction(&self) -> Option<ReplaceJournalFreshness> {
        let imp = self.imp();
        if self.replace_transaction_claimed() {
            return None;
        }
        if other_application_panels(self).any(|panel| panel.replace_transaction_claimed()) {
            self.publish_journal_message(
                "Another window is running Replace All or its Undo; try again when it finishes",
                lushtext_services::coordination::notifications::NotificationSeverity::Warning,
            );
            return None;
        }
        let generation = self.reserve_undo_backup_generation();
        imp.journal
            .transaction
            .set(ReplaceTransactionPhase::Claimed {
                reserved_generation: Some(ReplaceTransactionGeneration::new(generation.get())),
                resume_persisted_load: false,
            });
        imp.replace_all_button.set_sensitive(false);
        imp.undo_button.set_sensitive(false);
        self.refresh_accessibility_state();
        Some(ReplaceJournalFreshness::new(
            imp.journal.undo_backup_generation.clone(),
            generation,
        ))
    }

    /// Release the serialized transaction after its UI and journal state are published.
    pub(crate) fn finish_replace_transaction(&self) {
        let imp = self.imp();
        let previous = imp
            .journal
            .transaction
            .replace(ReplaceTransactionPhase::Idle);
        imp.undo_button.set_sensitive(self.has_undo_backup());
        self.update_replace_button_sensitivity();
        self.refresh_accessibility_state();
        // A startup recovery that stood down for this transaction resumes now;
        // a run's admission would already have retired it.
        if matches!(
            previous,
            ReplaceTransactionPhase::Claimed {
                resume_persisted_load: true,
                ..
            }
        ) {
            self.load_persisted_undo_backup();
        }
    }

    /// Release the transaction as the panel is disposed
    /// (`replace-all-confirm-disposal-lane` design D4).
    ///
    /// Resets only the transaction phase this module owns and touches no
    /// widget: GTK clears template children in `dispose()`, which
    /// [`Self::finish_replace_transaction`]'s button and accessibility updates
    /// would read. A disposed panel the application registry can still reach
    /// then no longer refuses another window's apply or Undo. Every worker that
    /// holds a transaction (a run, its rollback, an Undo) keeps a strong
    /// reference to its window until its completion has released it, so no live
    /// worker's transaction is released here.
    pub(super) fn release_replace_transaction_on_dispose(&self) {
        let journal = &self.imp().journal;
        journal.undo_capacity_wakeup.cancel();
        let retired = journal.undo_backup.take().take_backup();
        release_superseded_undo_journal(retired);
        journal.transaction.set(ReplaceTransactionPhase::Idle);
    }

    /// Hand the preview selection's reservation to the durable apply workflow.
    pub(crate) fn take_replace_transaction(&self) -> Option<ReplaceJournalFreshness> {
        let imp = self.imp();
        let ReplaceTransactionPhase::Claimed {
            reserved_generation: Some(generation),
            resume_persisted_load,
        } = imp.journal.transaction.get()
        else {
            return None;
        };
        imp.journal
            .transaction
            .set(ReplaceTransactionPhase::Claimed {
                reserved_generation: None,
                resume_persisted_load,
            });
        Some(ReplaceJournalFreshness::new(
            imp.journal.undo_backup_generation.clone(),
            UndoBackupGeneration::new(generation.get()),
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
    ///
    /// Supersession is application-wide (design D3a): the journal on disk
    /// keeps only the latest run, so every other window's panel retires its
    /// Undo too and says so once in its status bar.
    ///
    /// A startup recovery still waiting or in flight is retired with it, in
    /// every panel: what it read is the journal this run replaces. Here the
    /// pending load is cancelled; another panel's generation is advanced even
    /// when it has no journal installed, so a recovery or disk job of its own
    /// in flight returns instead of installing the superseded journal or
    /// touching the new run's. This panel's generation is the run's own
    /// reservation and stays; a recovery it dispatched read an older
    /// generation, and it dispatches none while its transaction is claimed.
    pub(crate) fn supersede_prior_undo_for_replace(&self) {
        self.cancel_persisted_undo_load();
        let retired = self.imp().journal.undo_backup.take().take_backup();
        self.hide_undo_button();
        release_superseded_undo_journal(retired);
        for panel in other_application_panels(self) {
            panel.cancel_persisted_undo_load();
            panel.reserve_undo_backup_generation();
            let retired = panel.imp().journal.undo_backup.take().take_backup();
            if retired.is_none() {
                continue;
            }
            panel.hide_undo_button();
            release_superseded_undo_journal(retired);
            panel.publish_journal_message(
                "Undo for the earlier Replace All is no longer available",
                lushtext_services::coordination::notifications::NotificationSeverity::Info,
            );
        }
    }

    /// Join the application's set of search panels (design D3a); called once
    /// from `constructed`.
    pub(super) fn register_application_panel(&self) {
        APPLICATION_PANELS.with(|panels| {
            let mut panels = panels.borrow_mut();
            panels.retain(|panel| panel.upgrade().is_some());
            panels.push(self.downgrade());
        });
    }

    /// Record the counts the most recent durable apply reported.
    ///
    /// Observation only: the window already published the user-visible message
    /// from the same result, and this makes that outcome readable from the
    /// workflow's evidence surface instead of only from the status bar.
    pub(crate) fn record_replace_apply_counts(&self, counts: ReplaceApplyCounts) {
        self.imp()
            .journal
            .last_apply_counts
            .set(LastApplyCounts::Recorded(counts));
    }

    /// Publish a service-persisted journal and reveal its undo affordance.
    ///
    /// Returns whether the journal was still current. A superseded generation
    /// leaves the affordance hidden and abandons the payload, because a newer
    /// Replace All or undo already owns the journal.
    pub(crate) fn publish_undo_journal_for_generation(
        &self,
        backup: super::GuardedReplaceUndoBackup,
        generation: UndoBackupGeneration,
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
        generation: UndoBackupGeneration,
    ) -> bool {
        if !super::policy::journal_generation_is_current(
            self.imp().journal.current_generation(),
            generation,
        ) {
            drop(backup);
            return false;
        }
        let retired = self
            .imp()
            .journal
            .undo_backup
            .replace(UndoBackupSlot::Installed(Arc::new(backup)))
            .take_backup();
        release_superseded_undo_journal(retired);
        self.refresh_accessibility_state();
        true
    }

    /// Clear an empty service result only if its reservation is still current.
    pub(crate) fn clear_undo_backup_for_generation(
        &self,
        generation: UndoBackupGeneration,
    ) -> bool {
        if !super::policy::journal_generation_is_current(
            self.imp().journal.current_generation(),
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
    ) -> (
        UndoBackupGeneration,
        Option<Arc<super::GuardedReplaceUndoBackup>>,
    ) {
        self.cancel_persisted_undo_load();
        let previous = self
            .imp()
            .journal
            .undo_backup_generation
            .fetch_add(1, Ordering::AcqRel);
        let retired = self
            .imp()
            .journal
            .undo_backup
            .replace(UndoBackupSlot::Installed(backup))
            .take_backup();
        self.refresh_accessibility_state();
        (UndoBackupGeneration::new(previous.wrapping_add(1)), retired)
    }

    /// Restore a crash-interrupted active journal, or clean inactive stale state.
    ///
    /// Stands down while this panel's transaction is claimed (a confirmed run,
    /// waiting or writing, or an Undo): its install would advance the journal
    /// generation that transaction reserved, superseding a waiting run, or
    /// install the old journal over a running one. It resumes when the
    /// transaction ends ([`Self::finish_replace_transaction`]), unless a run's
    /// admission retired it first.
    pub(crate) fn load_persisted_undo_backup(&self) {
        if let ReplaceTransactionPhase::Claimed {
            reserved_generation,
            ..
        } = self.imp().journal.transaction.get()
        {
            self.imp()
                .journal
                .transaction
                .set(ReplaceTransactionPhase::Claimed {
                    reserved_generation,
                    resume_persisted_load: true,
                });
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let Some(reservation) =
            crate::ui::plain_disposal::try_reserve_for_gtk(MAX_REPLACE_UNDO_RETAINED_BYTES)
        else {
            tracing::warn!("Persisted Replace All undo backup deferred by disposal capacity");
            self.schedule_persisted_undo_backup_retry(observed_epoch);
            return;
        };
        let data_dir = json_store::data_dir();
        let generation = self.imp().journal.current_generation();
        let generation_counter = self.imp().journal.undo_backup_generation.clone();
        let callback_generation_counter = generation_counter.clone();
        self.charge_journal_disk_job();
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if !generation_is_current(&generation_counter, generation) {
                    return Ok(PersistedUndoStartupLoad {
                        active_backup: None,
                    });
                }
                let recovery = search_backup::load_recovering(&data_dir);
                if recovery.active {
                    let backup = recovery.backup;
                    let active_backup = own_undo_journal_payload(reservation, backup);
                    #[cfg(feature = "test-utils")]
                    super::test_policy::delay_undo_backup_disk();
                    return Ok(PersistedUndoStartupLoad {
                        active_backup: Some(active_backup),
                    });
                }
                let mut diagnostics = recovery.diagnostics;
                diagnostics.extend(search_backup::cleanup_stale(&data_dir).diagnostics);
                report_startup_diagnostics(&diagnostics);
                Ok::<PersistedUndoStartupLoad, search_backup::SearchBackupError>(
                    PersistedUndoStartupLoad {
                        active_backup: None,
                    },
                )
            },
            move |panel, result| {
                panel.finish_journal_disk_job();
                match result {
                    Ok(load) => {
                        if !generation_is_current(&callback_generation_counter, generation) {
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

    /// Retire a startup recovery that has not dispatched yet: its capacity
    /// wakeup, or its wait for this panel's transaction to end.
    fn cancel_persisted_undo_load(&self) {
        let journal = &self.imp().journal;
        journal.undo_capacity_wakeup.cancel();
        if let ReplaceTransactionPhase::Claimed {
            reserved_generation,
            ..
        } = journal.transaction.get()
        {
            journal.transaction.set(ReplaceTransactionPhase::Claimed {
                reserved_generation,
                resume_persisted_load: false,
            });
        }
    }

    fn schedule_persisted_undo_backup_retry(&self, observed_epoch: u64) {
        let panel_weak = self.downgrade();
        self.imp()
            .journal
            .undo_capacity_wakeup
            .arm(observed_epoch, move || {
                if let Some(panel) = panel_weak.upgrade() {
                    panel.load_persisted_undo_backup();
                }
            });
    }

    /// Clear undo backup and hide the undo button.
    pub(crate) fn clear_undo_backup(&self) {
        self.cancel_persisted_undo_load();
        let generation = UndoBackupGeneration::new(
            self.imp()
                .journal
                .undo_backup_generation
                .fetch_add(1, Ordering::AcqRel)
                .wrapping_add(1),
        );
        let retired = self.imp().journal.undo_backup.take().take_backup();
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
    pub fn reserve_undo_backup_generation_for_test(&self) -> UndoBackupGeneration {
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
        generation: UndoBackupGeneration,
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
    pub fn begin_replace_transaction_for_test(&self) -> Option<UndoBackupGeneration> {
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
        let journal = &self.imp().journal;
        journal
            .disk_jobs
            .set(journal.disk_jobs.get().saturating_add(1));
        journal
            .disk_jobs_in_flight
            .set(journal.disk_jobs_in_flight.get().saturating_add(1));
    }

    /// Release one journal disk job as its GTK completion runs.
    fn finish_journal_disk_job(&self) {
        let journal = &self.imp().journal;
        journal
            .disk_jobs_in_flight
            .set(journal.disk_jobs_in_flight.get().saturating_sub(1));
    }

    fn save_undo_backup_on_disk(
        &self,
        backup: Arc<super::GuardedReplaceUndoBackup>,
        retired: Option<Arc<super::GuardedReplaceUndoBackup>>,
        generation: UndoBackupGeneration,
    ) {
        let data_dir = json_store::data_dir();
        let generation_counter = self.imp().journal.undo_backup_generation.clone();
        self.charge_journal_disk_job();
        spawn_blocking_then(
            self.clone(),
            move || {
                let _retired = retired;
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_undo_backup_disk();
                let is_current = || generation_is_current(&generation_counter, generation);
                let Some(_disk_guard) =
                    search_backup::acquire_journal_guard_while_current(is_current)?
                else {
                    return Ok(());
                };
                if !is_current() {
                    return Ok(());
                }
                // Shrink, never rebuild (design D3a): the only production
                // caller is a partial undo, whose unrestored files still hold
                // Replace All output. Descriptors carry no file bytes, so a
                // journal that no longer holds every retained entry is reported
                // and left as it is rather than rewritten.
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
                        lushtext_services::coordination::notifications::NotificationSeverity::Warning,
                    );
                }
            },
        );
    }

    /// Surface one journal message through the window's status lane.
    pub(super) fn publish_journal_message(
        &self,
        message: &str,
        severity: lushtext_services::coordination::notifications::NotificationSeverity,
    ) {
        let callback = self.imp().callbacks.message_callback.borrow().clone();
        if let Some(callback) = callback {
            callback(message, severity);
        }
    }

    fn delete_undo_backup_on_disk(
        &self,
        generation: UndoBackupGeneration,
        retired: Option<Arc<super::GuardedReplaceUndoBackup>>,
    ) {
        let data_dir = json_store::data_dir();
        let generation_counter = self.imp().journal.undo_backup_generation.clone();
        self.charge_journal_disk_job();
        spawn_blocking_then(
            self.clone(),
            move || {
                let _retired = retired;
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_undo_backup_disk();
                let is_current = || generation_is_current(&generation_counter, generation);
                let Some(_disk_guard) =
                    search_backup::acquire_journal_guard_while_current(is_current)?
                else {
                    return Ok(());
                };
                if !is_current() {
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
        let Some(backup) = imp.journal.undo_backup.borrow().as_backup().cloned() else {
            return;
        };
        self.hide_undo_button();
        let callback = imp.callbacks.undo_callback.borrow().clone();
        if let Some(callback) = callback {
            callback(backup);
        }
    }
}

fn report_startup_diagnostics(
    diagnostics: &[lushtext_services::persistence::recovery_metadata::RecoveryDiagnostic],
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! Admission to the draft journal: the process's one journal lane, draft-id
//! ownership across windows, once-per-process startup restore, and bounded
//! admission for restoring draft bodies into GTK.
//!
//! ## One journal per process
//!
//! The manifest write lock and the stable target guard are process-wide, but
//! they serialize single I/O steps only. What keeps orphan cleanup out of a
//! register → body write → commit pass, and out of a deletion's body-then-entry
//! steps, is the journal **lane**, and it has to be process-wide too: with a
//! lane per window, one window's cleanup retired the entry another window had
//! just registered, and deleted an untitled body another window had written
//! but not yet committed (the two-window Kani journal harness found both; see
//! the programme record, phase 4). [`ProcessDraftJournal`] is that lane, plus
//! the two other things a second window must not repeat: the claim on each
//! draft id (one window's autosave, restore, and deletion per id), and the
//! startup session restore, which the first window to reach it performs and no
//! later window repeats. It also keeps every window's manifest copy the
//! persisted one: a commit, an authority change, or a cleanup removal in one
//! window is mirrored into every other window's copy, so no window acts on a
//! stale entry another window already retired.
//!
//! It is per application, not a process global: widget tests build many
//! applications in one process, and each must start with its own journal.
//! Production has one application per process, so the two coincide. The
//! per-window `mutation_inflight` / `orphan_cleanup_inflight` flags stay, as
//! projections of "this window holds the lane" that `DraftEvidence` and the
//! `draft-autosave` readiness blocker read.
//!
//! ## Restoring bodies
//!
//! *Reserve then settle.* A restored draft body can be 64 MiB, so exactly one
//! crosses the worker boundary at a time and each one holds a disposal
//! reservation for its whole life. Startup's eagerly-preloaded bodies take a
//! **replacement** reservation out of the aggregate permit rather than a new one,
//! so the total never exceeds what the progress lane accounted for; when there is
//! no headroom, every eager body is demoted to a compact lazy marker *before*
//! this module returns, so GTK never owns an unguarded recovery body.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::draft::{
    DraftEntry, FileDraftRestoreResolution, PreloadedDraftRestore, PreloadedDraftSkip,
};
use crate::model::draft::{DraftManifest, DraftManifestAuthority};
use crate::services::draft_service::journal_core::{
    self, DraftClaim, JournalLaneAdmission, JournalLaneHolder,
};
use crate::services::{draft_service, json_store};
use crate::ui::editor_page::LushtextEditorPage;

use super::policy;
use super::seams::{
    DraftRestoreTicket, DraftRestoreTracking, GuardedDraftRestoreResolution,
    GuardedPreloadedDraftRestore,
};
use super::{
    DRAFT_RESTORE_DISPOSAL_RESERVATION_BYTES, attach_draft_body_disposal_probe,
    delay_draft_restore_for_test,
};
use crate::ui::window::LushtextWindow;

impl LushtextWindow {
    /// Move one eager body together with a replacement disposal reservation.
    ///
    /// The aggregate startup permit continues to own all other bodies until
    /// they are detached for worker retirement. If replacement headroom is
    /// unavailable, every eager body becomes a compact lazy marker before this
    /// method returns, so GTK never owns an unguarded recovery body.
    pub(super) fn take_preloaded_draft(
        &self,
        draft_id: &str,
    ) -> Option<GuardedPreloadedDraftRestore> {
        let mut preloaded = self.imp().drafts.preloaded.borrow_mut();
        let content = match preloaded.remove(draft_id)? {
            PreloadedDraftRestore::Skip(skip) => {
                return Some(GuardedPreloadedDraftRestore::Compact(skip));
            }
            PreloadedDraftRestore::Content(content) => content,
        };
        let body_weight = u64::try_from(content.capacity()).unwrap_or(u64::MAX);
        let reservation = preloaded.reservation_weight().map_or_else(
            || crate::ui::plain_disposal::try_reserve_progress_for_gtk(body_weight),
            |aggregate_weight| {
                crate::ui::plain_disposal::try_reserve_progress_replacement_for_gtk(
                    body_weight,
                    aggregate_weight,
                )
            },
        );
        let Some(reservation) = reservation else {
            // Reinsert the body so the aggregate release retires it on a worker
            // together with its peers; it must not drop on the GTK thread.
            preloaded.insert(
                draft_id.to_string(),
                PreloadedDraftRestore::Content(content),
            );
            super::retirement::release_eager_preloads(&mut preloaded);
            preloaded.remove(draft_id);
            return Some(GuardedPreloadedDraftRestore::Compact(
                PreloadedDraftSkip::LazyAggregateBudget,
            ));
        };

        if let Some(aggregate_weight) = preloaded.reservation_weight() {
            preloaded.shrink_reservation_to(aggregate_weight.saturating_sub(body_weight));
        }
        Some(GuardedPreloadedDraftRestore::Content(
            attach_draft_body_disposal_probe(reservation.own(content)),
        ))
    }

    /// Enqueue one non-preloaded body and start the serialized reader.
    ///
    /// Startup aggregate-budget skips and later on-demand fallbacks share this
    /// gate so completed 64 MiB reads cannot accumulate behind GTK installers.
    pub(super) fn queue_lazy_draft_restore(&self, editor: &LushtextEditorPage, entry: DraftEntry) {
        self.hold_draft_restore(&entry.draft_id);
        self.imp()
            .drafts
            .lazy_restore_queue
            .borrow_mut()
            .push_back(DraftRestoreTicket::capture(editor, entry));
        self.drive_lazy_draft_restore_queue();
    }

    /// Admit at most one lazy draft body to GTK and reject stale completions.
    pub(super) fn drive_lazy_draft_restore_queue(&self) {
        if self.imp().drafts.lazy_restore_inflight.get() {
            return;
        }
        if self.imp().drafts.lazy_restore_queue.borrow().is_empty() {
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::progress_disposal_capacity_epoch();
        let Some(reservation) = crate::ui::plain_disposal::try_reserve_progress_for_gtk(
            DRAFT_RESTORE_DISPOSAL_RESERVATION_BYTES,
        ) else {
            let window_weak = self.downgrade();
            self.imp()
                .drafts
                .lazy_restore_capacity_wakeup
                .arm(observed_epoch, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.drive_lazy_draft_restore_queue();
                    }
                });
            return;
        };
        let Some(candidate) = self
            .imp()
            .drafts
            .lazy_restore_queue
            .borrow_mut()
            .pop_front()
        else {
            return;
        };
        self.imp().drafts.lazy_restore_inflight.set(true);
        self.note_draft_restore_started();
        let data_dir = json_store::data_dir();
        let entry = candidate.entry.clone();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                delay_draft_restore_for_test();
                let mut reservation = reservation;
                draft_service::resolve_draft_restore(&data_dir, &entry).map(|resolution| {
                    match resolution {
                        FileDraftRestoreResolution::Restore { content } => {
                            reservation
                                .shrink_to(u64::try_from(content.capacity()).unwrap_or(u64::MAX));
                            GuardedDraftRestoreResolution::Restore(
                                attach_draft_body_disposal_probe(reservation.own(content)),
                            )
                        }
                        FileDraftRestoreResolution::Skip(skip) => {
                            GuardedDraftRestoreResolution::Compact(skip)
                        }
                    }
                })
            },
            move |(), result| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                window.finish_draft_restore(&candidate, result, DraftRestoreTracking::Lazy);
            },
        );
    }

    pub(super) fn note_draft_restore_started(&self) {
        let count = self.imp().drafts.restore_inflight_count.get();
        self.imp()
            .drafts
            .restore_inflight_count
            .set(count.saturating_add(1));
    }

    pub(super) fn note_draft_restore_finished(&self) {
        let count = self.imp().drafts.restore_inflight_count.get();
        self.imp()
            .drafts
            .restore_inflight_count
            .set(count.saturating_sub(1));
    }

    pub(super) fn finish_draft_restore_tracking(&self, tracking: DraftRestoreTracking) {
        if matches!(tracking, DraftRestoreTracking::Lazy) {
            self.imp().drafts.lazy_restore_inflight.set(false);
        }
        self.note_draft_restore_finished();
        if matches!(tracking, DraftRestoreTracking::Lazy) {
            self.drive_lazy_draft_restore_queue();
        }
    }

    /// Whether draft persistence or deferred startup restore blocks readiness.
    ///
    /// Reads the six live cells and delegates the verdict to
    /// `policy::draft_workflow_blocks_readiness`, which is the same function the
    /// evidence surface calls. Both paths must agree by construction: this
    /// accessor is the cheap one the readiness poller uses, and the surface is
    /// the one automation projects, so a hand-written second copy of the
    /// disjunction here could drift from the projected answer without any test
    /// comparing them.
    pub(crate) fn draft_workflow_blocks_readiness(&self) -> bool {
        let drafts = &self.imp().drafts;
        policy::draft_workflow_blocks_readiness(
            drafts.autosave_inflight.get(),
            drafts.mutation_inflight.get(),
            !drafts.pending_deletes.borrow().is_empty(),
            drafts.restore_inflight_count.get() > 0,
            drafts.lazy_restore_inflight.get(),
            !drafts.lazy_restore_queue.borrow().is_empty(),
        )
    }
}

/// Identity of one window inside its application's draft journal.
type JournalWindowKey = u64;

/// The draft journal state every window of one application shares.
///
/// GTK-thread only: every field is a `Cell` or `RefCell`, reached through the
/// `Rc` each window keeps from its construction.
#[derive(Default)]
pub(crate) struct ProcessDraftJournal {
    /// The application this journal belongs to.
    application: glib::WeakRef<gtk4::Application>,
    next_key: Cell<JournalWindowKey>,
    /// Every live window, in creation order (the order session saves use).
    windows: RefCell<Vec<(JournalWindowKey, glib::WeakRef<LushtextWindow>)>>,
    /// The window holding the journal lane.
    lane: Cell<Option<JournalWindowKey>>,
    /// Whether a wakeup of the windows waiting for the lane is queued.
    wake_queued: Cell<bool>,
    /// Which window owns each draft id it has restored, autosaved, or deleted.
    claims: RefCell<HashMap<String, JournalWindowKey>>,
    /// Whether a window already ran the startup session restore.
    session_restored: Cell<bool>,
    /// The window that schedules orphan cleanup for the process.
    cleanup_owner: Cell<Option<JournalWindowKey>>,
    /// Process-wide ordering for session saves from any window.
    session_generation: Cell<u64>,
}

thread_local! {
    /// One journal per live application; dead entries are pruned on lookup.
    static JOURNALS: RefCell<Vec<Rc<ProcessDraftJournal>>> = const { RefCell::new(Vec::new()) };
}

impl ProcessDraftJournal {
    /// The journal of `application`, created on first use.
    fn of(application: &gtk4::Application) -> Rc<Self> {
        JOURNALS.with(|journals| {
            let mut journals = journals.borrow_mut();
            journals.retain(|journal| journal.application.upgrade().is_some());
            if let Some(journal) = journals
                .iter()
                .find(|journal| journal.application.upgrade().as_ref() == Some(application))
            {
                return Rc::clone(journal);
            }
            let journal = Rc::new(Self::default());
            journal.application.set(Some(application));
            // A destroyed window leaves the application at once (ledger A21)
            // but is disposed only when its last reference drops, so the
            // journal is left from `window-removed`, not only from dispose: a
            // window a caller still references must not keep the lane, its
            // claims, or its tabs.
            application.connect_window_removed(|_, window| {
                if let Some(window) = window.downcast_ref::<LushtextWindow>() {
                    window.leave_process_draft_journal_unless_busy();
                }
            });
            journals.push(Rc::clone(&journal));
            journal
        })
    }

    fn window(&self, key: JournalWindowKey) -> Option<LushtextWindow> {
        self.windows
            .borrow()
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .and_then(|(_, window)| window.upgrade())
    }

    /// Every live window except `key`, in creation order.
    fn peers(&self, key: JournalWindowKey) -> Vec<LushtextWindow> {
        self.windows
            .borrow()
            .iter()
            .filter(|(candidate, _)| *candidate != key)
            .filter_map(|(_, window)| window.upgrade())
            .collect()
    }

    fn holder(&self, key: JournalWindowKey) -> JournalLaneHolder {
        match self.lane.get() {
            None => JournalLaneHolder::Nobody,
            Some(holder) if holder == key => JournalLaneHolder::ThisWindow,
            Some(_) => JournalLaneHolder::OtherWindow,
        }
    }

    /// Release the lane when `key` holds it, and wake the windows that were
    /// marked pending while it was held.
    fn release(self: &Rc<Self>, key: JournalWindowKey) {
        if self.lane.get() != Some(key) {
            return;
        }
        self.lane.set(None);
        // The releasing window's own continuation runs first (its completion
        // drives its pending work right after releasing); the other windows
        // are woken from an idle, and only if the lane is still free then.
        if self.wake_queued.replace(true) {
            return;
        }
        let journal = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            let Some(journal) = journal.upgrade() else {
                return;
            };
            journal.wake_queued.set(false);
            let waiting: Vec<LushtextWindow> = journal
                .windows
                .borrow()
                .iter()
                .filter_map(|(_, window)| window.upgrade())
                .collect();
            for window in waiting {
                if journal.lane.get().is_some() {
                    break;
                }
                window.drive_pending_draft_mutations();
            }
        });
    }
}

impl LushtextWindow {
    /// Join the draft journal of `application`. Called once, at construction.
    pub(in crate::ui::window) fn join_process_draft_journal(
        &self,
        application: &libadwaita::Application,
    ) {
        let journal = ProcessDraftJournal::of(application.upcast_ref());
        let key = journal.next_key.get().saturating_add(1);
        journal.next_key.set(key);
        journal.windows.borrow_mut().push((key, self.downgrade()));
        self.imp().drafts.journal_key.set(key);
        *self.imp().drafts.journal.borrow_mut() = Some(journal);
    }

    /// Leave the journal when the window leaves its application, or at the
    /// latest at dispose: release the lane and every claim this window holds,
    /// exactly once however often either path runs.
    pub(in crate::ui::window) fn leave_process_draft_journal(&self) {
        let Some(journal) = self.imp().drafts.journal.borrow_mut().take() else {
            return;
        };
        let key = self.imp().drafts.journal_key.get();
        journal
            .windows
            .borrow_mut()
            .retain(|(candidate, _)| *candidate != key);
        journal.claims.borrow_mut().retain(|_, owner| *owner != key);
        if journal.cleanup_owner.get() == Some(key) {
            journal.cleanup_owner.set(None);
        }
        journal.release(key);
    }

    /// Leave on `window-removed`, unless this window's journal work is still
    /// in flight: its lane must stay held until that work's completion
    /// releases it, or another window's cleanup could run inside the pass.
    /// The completion then leaves (`release_journal_lane`), and dispose does
    /// at the latest.
    fn leave_process_draft_journal_unless_busy(&self) {
        let drafts = &self.imp().drafts;
        if drafts.mutation_inflight.get() || drafts.orphan_cleanup_inflight.get() {
            return;
        }
        self.leave_process_draft_journal();
    }

    /// The journal this window takes part in; `None` only once it has left
    /// (every window joins at construction). A window that has left runs no
    /// more journal work, so every decision below fails closed on `None`: it
    /// may still be alive (ledger A21), and journal work outside the lane
    /// could run inside another window's pass.
    fn process_draft_journal(&self) -> Option<Rc<ProcessDraftJournal>> {
        self.imp().drafts.journal.borrow().clone()
    }

    /// Whether journal mutation work may not start now: this window's own
    /// mutation is in flight, or another window's journal work holds the
    /// lane. Every admission pre-check uses this one predicate, so none can
    /// forget the cross-window half.
    pub(super) fn journal_mutation_busy(&self) -> bool {
        self.imp().drafts.mutation_inflight.get()
            || self.process_draft_journal().is_some_and(|journal| {
                journal.holder(self.imp().drafts.journal_key.get())
                    == JournalLaneHolder::OtherWindow
            })
    }

    /// Take the lane for journal work, when it is free. Returns whether this
    /// window now holds it; on `false` the caller marks itself pending.
    pub(super) fn take_journal_lane(&self) -> bool {
        let Some(journal) = self.process_draft_journal() else {
            return false;
        };
        let key = self.imp().drafts.journal_key.get();
        if journal_core::journal_lane_admission(journal.holder(key))
            == JournalLaneAdmission::MarkPending
        {
            return false;
        }
        journal.lane.set(Some(key));
        true
    }

    /// Release the lane this window's journal work held.
    pub(super) fn release_journal_lane(&self) {
        if let Some(journal) = self.process_draft_journal() {
            journal.release(self.imp().drafts.journal_key.get());
        }
        // A window removed from its application while this work was in flight
        // deferred leaving the journal to here.
        if self.application().is_none() {
            self.leave_process_draft_journal();
        }
    }

    /// Give back a lane taken in this same GTK turn for work that turned out
    /// to be empty. No other window can have marked itself pending in between,
    /// so none is woken: waking would re-run a pass that just found nothing.
    pub(super) fn return_unused_journal_lane(&self) {
        if let Some(journal) = self.process_draft_journal()
            && journal.lane.get() == Some(self.imp().drafts.journal_key.get())
        {
            journal.lane.set(None);
        }
    }

    /// Whether this window has an editor whose draft id is `draft_id`.
    fn has_editor_for_draft_id(&self, draft_id: &str) -> bool {
        let Some(tab_view) = self.imp().tab_view.try_get() else {
            return false;
        };
        (0..tab_view.n_pages()).any(|index| {
            tab_view
                .nth_page(index)
                .child()
                .downcast_ref::<LushtextEditorPage>()
                .is_some_and(|editor| editor.has_draft_id(draft_id))
        })
    }

    /// Claim `draft_id` for this window's restore, autosave, or deletion.
    ///
    /// The first window to claim an id owns it while it keeps an editor for
    /// it; a claim whose owner no longer has one passes to the asking window.
    pub(super) fn claim_draft_id(&self, draft_id: &str) -> DraftClaim {
        let Some(journal) = self.process_draft_journal() else {
            return DraftClaim::OtherWindow;
        };
        let key = self.imp().drafts.journal_key.get();
        let owner = journal.claims.borrow().get(draft_id).copied();
        match owner {
            Some(owner) if owner == key => return DraftClaim::ThisWindow,
            Some(owner)
                if journal
                    .window(owner)
                    .is_some_and(|window| window.has_editor_for_draft_id(draft_id)) =>
            {
                return DraftClaim::OtherWindow;
            }
            Some(_) | None => {}
        }
        journal
            .claims
            .borrow_mut()
            .insert(draft_id.to_string(), key);
        DraftClaim::ThisWindow
    }

    /// Decide this window's startup restore: the first window of the process
    /// to ask restores the session and owns orphan-cleanup scheduling.
    pub(in crate::ui::window) fn claim_startup_restore(&self) -> journal_core::StartupRestore {
        let Some(journal) = self.process_draft_journal() else {
            return journal_core::StartupRestore::RestoreSession;
        };
        let decision = journal_core::startup_restore(journal.session_restored.get());
        if decision == journal_core::StartupRestore::RestoreSession {
            journal.session_restored.set(true);
            journal
                .cleanup_owner
                .set(Some(self.imp().drafts.journal_key.get()));
        }
        decision
    }

    /// Whether this window schedules orphan cleanup for the process. The
    /// first window that asks after the owner closed takes it over.
    pub(super) fn owns_process_orphan_cleanup(&self) -> bool {
        let Some(journal) = self.process_draft_journal() else {
            return false;
        };
        let key = self.imp().drafts.journal_key.get();
        if let Some(owner) = journal.cleanup_owner.get() {
            owner == key
        } else {
            journal.cleanup_owner.set(Some(key));
            true
        }
    }

    /// Every other live window of this window's journal, in creation order.
    fn journal_peers(&self) -> Vec<Self> {
        self.process_draft_journal()
            .map(|journal| journal.peers(self.imp().drafts.journal_key.get()))
            .unwrap_or_default()
    }

    /// Mirror a manifest this window just adopted into every other window's
    /// copy, minus each window's own tombstones, with its authority.
    pub(super) fn mirror_draft_manifest_to_peers(
        &self,
        manifest: &DraftManifest,
        authority: DraftManifestAuthority,
    ) {
        for peer in self.journal_peers() {
            peer.adopt_draft_manifest(manifest.clone(), authority);
        }
    }

    /// Mirror orphan-cleanup removals into every other window's copy.
    pub(super) fn mirror_orphan_removals_to_peers(
        &self,
        committed_by_id: &HashMap<String, draft_service::DraftEntryFingerprint>,
    ) {
        for peer in self.journal_peers() {
            draft_service::merge_committed_orphan_removals(
                &mut peer.imp().drafts.manifest.borrow_mut(),
                committed_by_id,
            );
        }
    }

    /// Mirror a revoked authority into every other window.
    pub(super) fn mirror_draft_authority_to_peers(&self, authority: DraftManifestAuthority) {
        for peer in self.journal_peers() {
            peer.imp().drafts.manifest_authority.set(authority);
            if !authority.is_trusted() {
                peer.imp().drafts.dispose_orphan_cleanup();
            }
        }
    }

    /// A manifest copy and authority from a window that already has one, for
    /// a window that starts without restoring the session.
    pub(super) fn peer_draft_manifest(&self) -> Option<(DraftManifest, DraftManifestAuthority)> {
        self.journal_peers().first().map(|peer| {
            (
                peer.imp().drafts.manifest.borrow().clone(),
                peer.imp().drafts.manifest_authority.get(),
            )
        })
    }

    /// Every live window of the process, in creation order.
    pub(in crate::ui::window) fn process_windows(&self) -> Vec<Self> {
        self.process_draft_journal().map_or_else(
            || vec![self.clone()],
            |journal| {
                journal
                    .windows
                    .borrow()
                    .iter()
                    .filter_map(|(_, window)| window.upgrade())
                    .collect()
            },
        )
    }

    /// The next process-wide session-save generation.
    pub(in crate::ui::window) fn next_process_session_generation(&self) -> u64 {
        let Some(journal) = self.process_draft_journal() else {
            return 0;
        };
        let next = journal.session_generation.get().saturating_add(1);
        journal.session_generation.set(next);
        next
    }
}

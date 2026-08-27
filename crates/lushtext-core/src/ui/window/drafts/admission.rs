// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded admission for restoring draft bodies into GTK.
//!
//! *Reserve then settle.* A restored draft body can be 64 MiB, so exactly one
//! crosses the worker boundary at a time and each one holds a disposal
//! reservation for its whole life. Startup's eagerly-preloaded bodies take a
//! **replacement** reservation out of the aggregate permit rather than a new one,
//! so the total never exceeds what the progress lane accounted for; when there is
//! no headroom, every eager body is demoted to a compact lazy marker *before*
//! this module returns, so GTK never owns an unguarded recovery body.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::draft::{
    DraftEntry, FileDraftRestoreResolution, PreloadedDraftRestore, PreloadedDraftSkip,
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

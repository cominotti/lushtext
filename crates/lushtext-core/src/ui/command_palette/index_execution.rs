// SPDX-License-Identifier: GPL-3.0-or-later

//! Serialized worker execution for the palette's file-index mutation stage order.
//!
//! Named `index_execution` rather than `execution` because the palette owns two
//! ordered stage orders and both need an `execution` module; the stage-order
//! qualifier keeps each role name accurate without widening the bounded set (see
//! `openspec/specs/gtk-adapter-module-boundaries/spec.md`).
//!
//! Admission has already decided that a flush may start and has reserved the
//! replacement index's byte weight. This module takes the queue, chooses the
//! batch kind, clones-and-mutates (or rebuilds) on a worker, and arbitrates the
//! result against the live index generation.
//!
//! Three control inversions live here:
//!
//! - [`LushtextCommandPalette::dispatch_index_mutation`] returns as soon as the
//!   worker is spawned. Control resumes in the `spawn_blocking_then` completion
//!   closure, holding the [`policy::FileIndexMutationTicket`] captured at
//!   dispatch.
//! - When the ticket loses to a concurrent full replacement, the completion does
//!   not drop the batch: the queue was already drained when the batch was built,
//!   so the mutations exist nowhere else. It sets the rebuild flag and re-arms
//!   the flush debounce, so control resumes in `index_admission` and the lost
//!   work is replayed through a filesystem rebuild.
//! - When the queue refilled while the worker ran, the completion arms the same
//!   flush debounce for a tail turn.

use std::sync::Arc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use crate::services::palette::FileIndex;
use crate::ui::plain_disposal::DisposalReservation;

use super::LushtextCommandPalette;
use super::policy::{
    self, FileIndexMutationArbitration, FileIndexMutationFacts, FileIndexMutationTicket,
    FileIndexRetirementKind, FileIndexUpdate, FileIndexUpdateBatchKind,
};
use super::retirement;

/// The queue contents one flush turn took, plus the kind of work they imply.
struct FileIndexMutationBatch {
    kind: FileIndexUpdateBatchKind,
    updates: Vec<FileIndexUpdate>,
}

impl LushtextCommandPalette {
    /// Install a whole replacement index and advance the mutation generation.
    ///
    /// This is the full-replacement path a workspace-folder change takes. It
    /// does not queue, debounce, or arbitrate — it *is* the event that makes
    /// [`Self::settle_index_mutation`]'s arbitration necessary, because
    /// advancing the generation here is what makes an in-flight incremental
    /// batch stale.
    pub(super) fn install_replacement_file_index(
        &self,
        index: crate::ui::plain_disposal::DisposalOwned<FileIndex>,
    ) {
        let imp = self.imp();
        let index = index.into_retained_current();
        let previous = std::mem::replace(&mut *imp.file_index.borrow_mut(), Arc::new(index));
        let last_owned = Arc::strong_count(&previous) == 1;
        let released_len = previous.len();
        drop(previous);
        retirement::record_file_index_retirement(
            FileIndexRetirementKind::FullReplacement,
            last_owned,
            released_len,
        );
        imp.file_index_generation
            .set(imp.file_index_generation.get().wrapping_add(1));
        imp.restart_query_if_open();
    }

    /// Index stages 4 to 8: take the queue, run the worker, arbitrate the result.
    pub(super) fn dispatch_index_mutation(&self, reservation: DisposalReservation) {
        let imp = self.imp();
        let updates = std::mem::take(&mut *imp.pending_index_updates.borrow_mut());
        imp.pending_index_update_bytes.set(0);
        let kind = policy::select_batch_kind(imp.index_update_rebuild_pending.replace(false));
        let batch = FileIndexMutationBatch { kind, updates };
        let ticket = FileIndexMutationTicket::new(imp.file_index_generation.get(), kind);
        let base = Arc::clone(&imp.file_index.borrow());
        imp.index_update_worker_running.set(true);
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                #[cfg(feature = "test-utils")]
                super::test_policy::delay_index_update_worker();
                let index = apply_index_mutation_batch(&base, batch);
                let retained_bytes = index.retained_byte_weight();
                let mut reservation = reservation;
                reservation.shrink_to(retained_bytes);
                reservation.own(index)
            },
            move |palette, index| palette.settle_index_mutation(ticket, index),
        );
    }

    /// Index stages 6 and 7: install the applied batch, or reject and replay it.
    fn settle_index_mutation(
        &self,
        ticket: FileIndexMutationTicket,
        index: crate::ui::plain_disposal::DisposalOwned<FileIndex>,
    ) {
        let imp = self.imp();
        imp.index_update_worker_running.set(false);
        let facts = FileIndexMutationFacts {
            live_generation: imp.file_index_generation.get(),
        };
        match ticket.arbitrate(facts) {
            FileIndexMutationArbitration::Accept { next_generation } => {
                let previous =
                    std::mem::replace(&mut *imp.file_index.borrow_mut(), Arc::new(index));
                let last_owned = Arc::strong_count(&previous) == 1;
                let released_len = previous.len();
                drop(previous);
                retirement::record_file_index_retirement(
                    FileIndexRetirementKind::AcceptedIncremental,
                    last_owned,
                    released_len,
                );
                imp.file_index_generation.set(next_generation);
                imp.restart_query_if_open();
            }
            FileIndexMutationArbitration::RejectAndReplay => {
                let released_len = index.len();
                drop(index);
                retirement::record_file_index_retirement(
                    FileIndexRetirementKind::RejectedIncremental,
                    true,
                    released_len,
                );
                // A full replacement won the race. Replay this worker's
                // mutations before newer queued ones so neither source of
                // truth is silently lost.
                imp.index_update_rebuild_pending.set(true);
                self.schedule_index_flush();
            }
        }
        // Index stage 8: a queue that refilled during the worker gets a tail turn.
        if !imp.pending_index_updates.borrow().is_empty() {
            self.schedule_index_flush();
        }
    }
}

/// Index stage 5: apply one batch to a worker-owned index on the worker thread.
fn apply_index_mutation_batch(
    base: &Arc<crate::ui::plain_disposal::DisposalOwned<FileIndex>>,
    batch: FileIndexMutationBatch,
) -> FileIndex {
    match batch.kind {
        FileIndexUpdateBatchKind::Incremental => {
            let mut index = (***base).clone();
            let mut ledger = index.incremental_mutation_ledger();
            for update in &batch.updates {
                update.apply(&mut index, &mut ledger);
            }
            debug_assert_eq!(ledger.retained_bytes(), index.retained_byte_weight());
            debug_assert!(
                ledger.peak_retained_bytes()
                    <= crate::services::palette::MAX_FILE_INDEX_RETAINED_BYTES
            );
            drop(batch.updates);
            index
        }
        FileIndexUpdateBatchKind::Rebuild => {
            drop(batch.updates);
            (***base).rebuild_current_workspace_folders()
        }
    }
}

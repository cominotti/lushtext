// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded admission for the palette's file-index mutation stage order.
//!
//! Sidebar file operations and watcher reconciliation produce individual index
//! mutations at whatever rate the filesystem changes. This module decides which
//! of them the workflow may retain, when a flush turn is allowed to start, and
//! how a refused disposal reservation is retried. It does **not** build batches,
//! spawn workers, or arbitrate completions: that is `index_execution`.
//!
//! Two control inversions start here:
//!
//! - [`LushtextCommandPalette::schedule_index_flush`] arms a
//!   [`policy::INDEX_UPDATE_DEBOUNCE_MS`] debounce and returns. Control resumes
//!   in the debounce callback, which re-enters
//!   [`LushtextCommandPalette::flush_index_updates`].
//! - When disposal admission refuses the replacement index's byte weight,
//!   [`LushtextCommandPalette::flush_index_updates`] arms the disposal-capacity
//!   wakeup and returns without dropping anything. Control resumes in that
//!   wakeup's callback when the capacity epoch changes, which re-enters the same
//!   flush. A refusal therefore delays the mutation; it never loses it.
//!
//! Overflow is the third way a mutation survives pressure: exceeding either
//! bounded ceiling escalates the queue to a full filesystem rebuild instead of
//! discarding the queued mutations, so no filesystem change is silently lost.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use super::LushtextCommandPalette;
use super::policy::{self, FileIndexUpdate, IndexUpdateAdmission, IndexUpdateQueueState};

impl LushtextCommandPalette {
    /// Index stages 1 and 2: retain one mutation, then arm the coalescing flush.
    pub(super) fn admit_index_update(&self, update: FileIndexUpdate) {
        self.retain_bounded_index_update(update);
        self.schedule_index_flush();
    }

    /// Index stage 1: retain one mutation under the bounded count and byte caps.
    fn retain_bounded_index_update(&self, update: FileIndexUpdate) {
        let imp = self.imp();
        let mut pending = imp.pending_index_updates.borrow_mut();
        let state = IndexUpdateQueueState {
            rebuild_pending: imp.index_update_rebuild_pending.get(),
            len: pending.len(),
            capacity: pending.capacity(),
            retained_bytes: imp.pending_index_update_bytes.get(),
        };
        match policy::admit_index_update(state, update.retained_byte_weight()) {
            IndexUpdateAdmission::AlreadyRebuilding => {}
            IndexUpdateAdmission::EscalateToRebuild => {
                imp.index_update_rebuild_pending.set(true);
            }
            IndexUpdateAdmission::Retain {
                reserve_additional,
                retained_bytes,
            } => {
                if reserve_additional > 0 {
                    pending.reserve_exact(reserve_additional);
                }
                pending.push(update);
                imp.pending_index_update_bytes.set(retained_bytes);
            }
        }
    }

    /// Index stage 2: coalesce a burst of mutations behind one debounce.
    pub(super) fn schedule_index_flush(&self) {
        self.imp().index_update_debounce.schedule(
            self,
            Duration::from_millis(policy::INDEX_UPDATE_DEBOUNCE_MS),
            move |palette, _| palette.flush_index_updates(),
        );
    }

    /// Index stage 3: reserve replacement capacity, then hand the batch onward.
    ///
    /// Returns without side effects when a flush is not allowed to start, and
    /// arms the disposal-capacity wakeup instead of dropping work when
    /// admission refuses the replacement index's byte weight.
    pub(super) fn flush_index_updates(&self) {
        let imp = self.imp();
        if policy::index_flush_is_blocked(
            imp.index_update_worker_running.get(),
            imp.pending_index_updates.borrow().is_empty(),
            imp.index_update_rebuild_pending.get(),
        ) {
            return;
        }

        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let replacement_weight = crate::services::palette::MAX_FILE_INDEX_RETAINED_BYTES;
        let reservation = imp.file_index.borrow().reservation_weight().map_or_else(
            || crate::ui::plain_disposal::try_reserve_for_gtk(replacement_weight),
            |current_weight| {
                crate::ui::plain_disposal::try_reserve_replacement_for_gtk(
                    replacement_weight,
                    current_weight,
                )
            },
        );
        let Some(reservation) = reservation else {
            let palette_weak = self.downgrade();
            imp.index_update_capacity_wakeup
                .arm(observed_epoch, move || {
                    if let Some(palette) = palette_weak.upgrade() {
                        palette.flush_index_updates();
                    }
                });
            return;
        };

        self.dispatch_index_mutation(reservation);
    }
}

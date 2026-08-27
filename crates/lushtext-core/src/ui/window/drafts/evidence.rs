// SPDX-License-Identifier: GPL-3.0-or-later

//! The draft-recovery workflow's observable state, in one typed value.
//!
//! [`DraftEvidence`] is the single source of truth for observers of this
//! workflow. It folds in the pre-convention `OrphanCleanupRuntimeSnapshot` so no
//! second typed path remains, and it makes the **durable path** observable:
//! manifest authority, autosave in-flight and pending state, the mutation lane's
//! ownership, retained body weight and its high-water mark, cleanup continuation
//! progress, tombstone state, and how each stage order ended.
//!
//! Reading evidence is pure observation: it never arms a timer, advances an
//! intent epoch, admits a restore, or requires a pass to be running.
//!
//! **Reentrancy constraint.** [`LushtextWindow::draft_evidence`] takes shared
//! `RefCell` borrows of the manifest, the mutation order, the pending-delete
//! queue, the tombstone map, the preload graph, and the lazy-restore queue. It
//! must therefore not be called from code already holding a `borrow_mut()` on any
//! of them — which is why every derived scalar below is computed and every `Ref`
//! dropped **before** the struct literal is built. The manifest in particular is
//! read as a **count**, never cloned: it can hold thousands of entries.
//!
//! **Disposed-widget rule.** The tab count comes from the window's tab view,
//! which is a `TemplateChild`, so it reads through `TemplateChild::try_get()` and
//! answers honestly when the child is gone.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use crate::model::draft::DraftManifestAuthority;
use crate::ui::window::LushtextWindow;

/// One consistent read of the draft-recovery workflow.
///
/// Field groups follow the workflow's three stage orders: the journal's record
/// and its serialization gate, the autosave lane, the restore lane, and orphan
/// cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEvidence {
    // --- the journal's record ---
    /// Entries in the in-memory manifest.
    ///
    /// A count, never the manifest: it can hold thousands of entries, and an
    /// evidence read must stay O(1)-ish and must not widen what an observer can
    /// read to include original file paths.
    pub manifest_entries: usize,
    /// Completeness and durable-replacement authority for the in-memory manifest.
    pub manifest_authority: DraftManifestAuthority,
    /// Whether that authority permits destructive orphan cleanup.
    ///
    /// The single most consequential bit in this surface: cleanup deletes user
    /// content, and it is refused outright while this is false.
    pub manifest_authority_trusted: bool,
    /// Whether a body/manifest/delete mutation currently owns the lane.
    pub mutation_inflight: bool,
    /// Compact deletes waiting behind an earlier mutation.
    pub pending_delete_count: usize,
    /// Deletions whose durable manifest removal outlived a failed body deletion.
    ///
    /// A non-zero count means at least one delete is explicitly retryable rather
    /// than silently forgotten.
    pub delete_tombstone_count: usize,
    /// Draft IDs explicitly discarded during an in-progress close flow.
    pub close_discard_count: usize,

    // --- autosave ---
    /// Whether an autosave batch is currently snapshotting or writing.
    pub autosave_inflight: bool,
    /// Whether another pass is needed after the in-flight batch finishes.
    ///
    /// Not a queue depth: only "a further pass is needed" is remembered, so a
    /// burst of ticks during one long pass cannot fan out.
    pub autosave_pending: bool,
    /// Whether the first-dirty autosave timer is armed.
    pub first_dirty_timer_pending: bool,
    /// Complete bodies currently held across a worker handoff.
    pub retained_complete_bodies: usize,
    /// Peak complete-body count observed by this window.
    ///
    /// The pipeline's boundedness proof: it must never exceed one.
    pub max_retained_complete_bodies: usize,

    // --- restore ---
    /// Eager preload entries still available to a restoring tab.
    pub preloaded_entries: usize,
    /// Retained byte weight the preload graph's disposal reservation holds.
    pub preloaded_reservation_weight: Option<u64>,
    /// Whether one aggregate-budget draft read is currently active.
    pub lazy_restore_inflight: bool,
    /// Compact lazy restore tickets waiting for admission.
    pub lazy_restore_queued: usize,
    /// Asynchronous draft resolutions not yet delivered to GTK.
    pub restores_inflight: usize,

    // --- orphan cleanup ---
    /// Whether a cleanup timer is armed.
    pub cleanup_timer_pending: bool,
    /// Whether a cleanup inspect/execute worker is active.
    pub cleanup_worker_active: bool,
    /// Cleanup workers started during this window lifetime.
    pub cleanup_workers_started: usize,
    /// Peak simultaneous cleanup workers observed during this window lifetime.
    ///
    /// Must never exceed one: two concurrent passes could each decide to delete
    /// a body the other had just re-validated.
    pub cleanup_workers_high_water: usize,
    /// Manifest offset a deferred continuation will resume from.
    pub cleanup_pending_offset: Option<usize>,
    /// Consecutive retryable cleanup failures driving the bounded backoff.
    pub cleanup_failure_streak: u32,

    // --- readiness ---
    /// Whether any draft lane blocks automation readiness.
    pub blocks_readiness: bool,

    // --- window shell ---
    /// Mounted tab pages, or `None` when the template child is gone.
    pub mounted_pages: Option<i32>,
}

impl LushtextWindow {
    /// Read this window's whole draft-recovery workflow state at once.
    ///
    /// See the module documentation for the reentrancy constraint and the
    /// disposed-widget rule.
    #[must_use]
    pub fn draft_evidence(&self) -> DraftEvidence {
        let imp = self.imp();
        let drafts = &imp.drafts;

        // Every borrow is taken, read, and dropped before the struct literal, so
        // no `Ref` outlives the value it produced.
        let manifest_entries = drafts.manifest.borrow().drafts.len();
        let pending_delete_count = drafts.pending_deletes.borrow().len();
        let delete_tombstone_count = drafts.delete_tombstones.borrow().len();
        let close_discard_count = drafts.close_discard_ids.borrow().len();
        let (preloaded_entries, preloaded_reservation_weight) = {
            let preloaded = drafts.preloaded.borrow();
            (preloaded.len(), preloaded.reservation_weight())
        };
        let lazy_restore_queued = drafts.lazy_restore_queue.borrow().len();
        let authority = drafts.manifest_authority.get();
        let restores_inflight = drafts.restore_inflight_count.get();
        let autosave_inflight = drafts.autosave_inflight.get();
        let mutation_inflight = drafts.mutation_inflight.get();
        let lazy_restore_inflight = drafts.lazy_restore_inflight.get();
        // A disposed window has no tab view, so this is the honest `None`.
        let mounted_pages = imp.tab_view.try_get().map(|view| view.n_pages());

        DraftEvidence {
            manifest_entries,
            manifest_authority: authority,
            manifest_authority_trusted: authority.is_trusted(),
            mutation_inflight,
            pending_delete_count,
            delete_tombstone_count,
            close_discard_count,
            autosave_inflight,
            autosave_pending: drafts.autosave_pending.get(),
            first_dirty_timer_pending: drafts.first_dirty_autosave_pending.get(),
            retained_complete_bodies: drafts.retained_complete_bodies.get(),
            max_retained_complete_bodies: drafts.max_retained_complete_bodies.get(),
            preloaded_entries,
            preloaded_reservation_weight,
            lazy_restore_inflight,
            lazy_restore_queued,
            restores_inflight,
            cleanup_timer_pending: drafts.orphan_cleanup_timer_pending.get(),
            cleanup_worker_active: drafts.orphan_cleanup_inflight.get(),
            cleanup_workers_started: drafts.orphan_cleanup_workers_started.get(),
            cleanup_workers_high_water: drafts.orphan_cleanup_workers_high_water.get(),
            cleanup_pending_offset: drafts.orphan_cleanup_pending_offset.get(),
            cleanup_failure_streak: drafts.orphan_cleanup_failure_streak.get(),
            blocks_readiness: super::policy::draft_workflow_blocks_readiness(
                autosave_inflight,
                mutation_inflight,
                pending_delete_count > 0,
                restores_inflight > 0,
                lazy_restore_inflight,
                lazy_restore_queued > 0,
            ),
            mounted_pages,
        }
    }

    /// Whether a specific draft still owns an explicit retry tombstone.
    ///
    /// The one keyed lookup this workflow's observers need, and it deliberately
    /// stays separate from the surface rather than being folded into it: the
    /// surface reports *how many* tombstones exist, while "is **this** draft
    /// tombstoned" takes an argument and so cannot be a field. It is a named
    /// workflow question, not a per-field getter of surface state.
    #[must_use]
    pub fn draft_delete_is_tombstoned(&self, draft_id: &str) -> bool {
        self.imp()
            .drafts
            .delete_tombstones
            .borrow()
            .contains_key(draft_id)
    }
}

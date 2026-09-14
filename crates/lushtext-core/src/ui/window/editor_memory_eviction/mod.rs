// SPDX-License-Identifier: GPL-3.0-or-later

//! Evict background editor buffers to stay inside the window's memory budget.
//!
//! Not a user-initiated operation in the ordinary sense — nobody asks for an
//! eviction — but one ordered stage sequence with a single trigger family:
//! *a residency transition happened, so re-check whether the window is over
//! budget and, if so, reclaim the least-recently-used clean tabs*. Opening,
//! closing, loading, selecting, and editing a tab all enter here.
//!
//! It is its own row rather than part of `WFR-EDITOR-MEMORY`. That row is
//! `exempt`, and its exemption covers `model/editor_memory.rs` — a pure module
//! with no toolkit dependency. Stretching an exemption granted for purity over
//! GTK orchestration that owns a generation counter, a bounded idle
//! continuation, and two race injectors would use it to excuse the least pure
//! code in the row.
//!
//! ## Stages
//!
//! Entry points: `track_editor_memory` (a page joins), `untrack_editor_memory`
//! (a page leaves), `mark_editor_memory_accessed` (selection or load makes a
//! page most-recent), `maybe_evict_background_tabs` (an explicit nudge), and
//! `reload_if_evicted` (the user returns to a page that was reclaimed).
//!
//! 1. **Record.** Update one scalar residency record. This is constant work: no
//!    tab walk, no hash table, no buffer read beyond the editor's own estimate.
//! 2. **Decide whether to look.** `policy::aggregate_pass_is_warranted` asks
//!    whether the new totals justify an aggregate pass at all. The ordinary edit
//!    path stops here.
//! 3. **Coalesce.** `schedule_editor_memory_evaluation` folds any burst of
//!    transitions into one next-idle pass.
//! 4. **Plan.** One full tab walk builds a residency snapshot, reconciles the
//!    ledger against it, and hands it to the domain policy in
//!    `model::editor_memory`, which returns the ranked candidates.
//! 5. **Apply.** `execution` clears one candidate per idle turn, revalidating
//!    each against live state immediately before clearing it.
//!
//! ## The two inversions
//!
//! Both are deferred, and both are where a stale plan could do damage:
//!
//! * **Stage 3 → 4.** `schedule_editor_memory_evaluation` arms a flag and posts
//!   `glib::idle_add_local_once`. **Control resumes in that callback**, which
//!   disarms and runs stage 4. The flag is what makes a burst coalesce; a
//!   transition arriving while the flag is already set adds nothing.
//! * **Stage 5, once per candidate.** `execution::apply_decision` posts
//!   `glib::idle_add_local` and **control resumes in that callback for every
//!   candidate in turn**, ending when the pass converges, exhausts its
//!   candidates, or is re-armed.
//!
//! The second inversion is the workflow's whole risk surface. The plan is built
//! in one turn and applied over many, so a tab can be closed, replaced,
//! re-selected, edited, or made unrecoverable in between. Every candidate is
//! therefore re-validated against live state by
//! `policy::candidate_is_still_evictable` immediately before its buffer is
//! cleared, and any foreign transition re-arms the pass so it resnapshots rather
//! than applying a plan built before the change.
//!
//! ## Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — the revalidation rule, the pass/untrack/continuation decisions |
//! | `execution` | coordination — applies one plan in bounded main-loop slices |
//! | `evidence` | evidence surface — the row's single observable state; `test-utils`-gated |
//!
//! The row owns **no `test_policy.rs`**: it has no timing or limit override. Its
//! two `test-utils` race injectors (`before_eviction_hook`, `after_eviction_hook`)
//! are **actuation seams**, not configuration — they inject a transition at an
//! exact point so a test can drive the stale-plan path that is otherwise
//! unreachable. They are pre-existing and are carried across this move
//! unchanged; the row spends no new actuation seam.

pub mod policy;

#[cfg(feature = "test-utils")]
pub mod evidence;
mod execution;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::model::editor_memory::{
    EditorMemoryBudgetOutcome, EditorResidency, evaluate_editor_memory_budget,
};
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;
use policy::UntrackDisposition;

impl LushtextWindow {
    /// Wire one editor's residency transitions into the window memory policy.
    ///
    /// GTK-main-thread callbacks use weak references so tabs and the window are
    /// not retained. Attaching the page installs one scalar ledger record.
    pub(super) fn track_editor_memory(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_memory_policy_changed(move || {
            if let (Some(window), Some(editor)) = (window_weak.upgrade(), editor_weak.upgrade()) {
                window.update_editor_memory_record(&editor);
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        // A completed load is newly resident, including during out-of-order
        // restore, so it receives a fresh window-wide LRU generation.
        editor.connect_file_loaded(move || {
            if let (Some(window), Some(editor)) = (window_weak.upgrade(), editor_weak.upgrade()) {
                window.mark_editor_memory_accessed(&editor);
            }
        });

        self.update_editor_memory_record(editor);
    }

    /// Remove one detached editor's scalar record without walking remaining tabs.
    pub(super) fn untrack_editor_memory(&self, editor: &LushtextEditorPage) {
        let editor_id = editor.as_ptr() as usize;
        let state = &self.imp().editor_memory;
        let update = state.ledger.borrow_mut().remove(editor_id);
        if state
            .active_editor
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
            .is_some_and(|active| active.as_ptr() == editor.as_ptr())
        {
            state.active_editor.borrow_mut().take();
        }

        match policy::untrack_disposition(
            state.evaluation_running.get(),
            state.applying_eviction.get(),
            update.map(|update| update.total_bytes),
            state.accounting_uncertain.get(),
        ) {
            UntrackDisposition::RearmRunningPass => state.evaluation_armed.set(true),
            UntrackDisposition::IgnoreOwnEviction => {}
            UntrackDisposition::SchedulePass => self.schedule_editor_memory_evaluation(),
            UntrackDisposition::RecordWithinBudget => state
                .last_outcome
                .set(EditorMemoryBudgetOutcome::WithinBudget),
        }
    }

    /// Assign the next window-wide recency generation to one live editor.
    pub(super) fn mark_editor_memory_accessed(&self, editor: &LushtextEditorPage) {
        let state = &self.imp().editor_memory;
        let previous = state
            .active_editor
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade);
        if let Some(previous) = previous
            && previous.as_ptr() != editor.as_ptr()
        {
            self.update_editor_memory_record(&previous);
        }
        let active_weak = editor.downgrade();
        state.active_editor.replace(Some(active_weak));
        let generation = state.next_access_generation.get().wrapping_add(1);
        state.next_access_generation.set(generation);
        editor.mark_memory_accessed(generation);
    }

    /// Reload a page the budget reclaimed, when the user returns to it.
    pub(super) fn reload_if_evicted(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        if !editor.is_evicted() {
            return;
        }
        if let Some(ref path) = editor.file_path() {
            editor.load_file_async(path);
        }
    }

    /// Schedule the same coalesced evaluation used by live editor callbacks.
    pub(super) fn maybe_evict_background_tabs(&self) {
        let memory = &self.imp().editor_memory;
        let total = memory.ledger.borrow().total_bytes();
        if policy::aggregate_pass_is_warranted(total, memory.accounting_uncertain.get()) {
            self.schedule_editor_memory_evaluation();
        }
    }

    /// Stages 1 and 2 — refresh one record and enforce only when needed.
    fn update_editor_memory_record(&self, editor: &LushtextEditorPage) {
        let state = &self.imp().editor_memory;
        let editor_id = editor.as_ptr() as usize;
        if !editor.is_ancestor(&*self.imp().tab_view) {
            // A delayed callback from a detached page must not resurrect its
            // record after the trusted detach delta removed it.
            state.ledger.borrow_mut().remove(editor_id);
            return;
        }
        let active = self.is_selected_editor(editor);
        let update = state.ledger.borrow_mut().upsert(EditorResidency {
            editor_id,
            estimated_bytes: editor.estimated_live_buffer_bytes(),
            access_generation: editor.memory_access_generation(),
            policy_generation: editor.memory_policy_generation(),
            eligible_for_eviction: editor.eligible_for_memory_eviction(active),
        });

        match policy::untrack_disposition(
            state.evaluation_running.get(),
            state.applying_eviction.get(),
            Some(update.total_bytes),
            state.accounting_uncertain.get(),
        ) {
            UntrackDisposition::RearmRunningPass => state.evaluation_armed.set(true),
            UntrackDisposition::IgnoreOwnEviction => {}
            UntrackDisposition::SchedulePass => self.schedule_editor_memory_evaluation(),
            UntrackDisposition::RecordWithinBudget => state
                .last_outcome
                .set(EditorMemoryBudgetOutcome::WithinBudget),
        }
    }

    /// Stage 3 — coalesce any number of transitions into one next-idle pass.
    ///
    /// The first inversion: control resumes in the posted idle callback.
    pub(super) fn schedule_editor_memory_evaluation(&self) {
        let state = &self.imp().editor_memory;
        // Remember transitions that happen between bounded eviction turns. A
        // signal emitted synchronously by `evict()` itself is already included
        // in the running pass and does not require a redundant scan.
        if state.evaluation_running.get() {
            if !state.applying_eviction.get() {
                state.evaluation_armed.set(true);
            }
            return;
        }
        if state.evaluation_armed.replace(true) {
            return;
        }

        let window_weak = self.downgrade();
        // Queue on GTK's main loop so a burst of buffer and tab signals becomes
        // one pass. The `_local` callback stays where widget access is safe.
        glib::idle_add_local_once(move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            window.imp().editor_memory.evaluation_armed.set(false);
            window.evaluate_editor_memory_budget();
        });
    }

    /// Stage 4 — one GTK-main-thread aggregate memory-policy pass.
    ///
    /// Scalar facts feed the GTK-free LRU policy in `model::editor_memory`, then
    /// `execution` re-finds and re-validates every candidate before clearing it.
    fn evaluate_editor_memory_budget(&self) {
        let memory = &self.imp().editor_memory;
        if memory.evaluation_running.replace(true) {
            return;
        }
        memory
            .evaluation_count
            .set(memory.evaluation_count.get().wrapping_add(1));
        memory
            .full_scan_count
            .set(memory.full_scan_count.get().wrapping_add(1));

        let snapshot = self.residency_snapshot();
        memory
            .ledger
            .borrow_mut()
            .reconcile(snapshot.iter().copied());
        memory.accounting_uncertain.set(false);

        let decision = evaluate_editor_memory_budget(&snapshot);
        memory.last_outcome.set(decision.outcome);
        if decision.candidates.is_empty() {
            memory.evaluation_running.set(false);
            return;
        }

        #[cfg(feature = "test-utils")]
        if let Some(hook) = memory.before_eviction_hook.borrow_mut().take() {
            hook();
        }

        // Stage 5, and the second inversion.
        execution::apply_decision(self, decision);
    }

    /// One full tab walk producing the scalar facts stage 4 plans from.
    fn residency_snapshot(&self) -> Vec<EditorResidency> {
        let tab_view = &self.imp().tab_view;
        let selected = tab_view.selected_page();
        let mut snapshot = Vec::with_capacity(usize::try_from(tab_view.n_pages()).unwrap_or(0));
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let active = selected.as_ref() == Some(&page);
                snapshot.push(EditorResidency {
                    editor_id: editor.as_ptr() as usize,
                    estimated_bytes: editor.estimated_live_buffer_bytes(),
                    access_generation: editor.memory_access_generation(),
                    policy_generation: editor.memory_policy_generation(),
                    // Every uncertain or non-recoverable state stays protected.
                    eligible_for_eviction: editor.eligible_for_memory_eviction(active),
                });
            }
        }
        snapshot
    }

    /// Inject one transition between candidate selection and safety rechecks.
    #[cfg(feature = "test-utils")]
    pub fn set_before_editor_memory_eviction_hook_for_test<F: FnOnce() + 'static>(&self, hook: F) {
        self.imp()
            .editor_memory
            .before_eviction_hook
            .replace(Some(Box::new(hook)));
    }

    /// Inject one transition between an applied eviction and the next turn.
    #[cfg(feature = "test-utils")]
    pub fn set_after_editor_memory_eviction_hook_for_test<F: FnOnce() + 'static>(&self, hook: F) {
        self.imp()
            .editor_memory
            .after_eviction_hook
            .replace(Some(Box::new(hook)));
    }
}

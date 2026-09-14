// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — **execution**. Applies one eviction plan in bounded
//! main-loop slices.
//!
//! Clearing a `GtkTextBuffer` and its projections is main-thread work, so the
//! plan is not applied in one turn. This module owns the continuation: one
//! candidate per idle dispatch, giving GTK a chance to serve input and render
//! between large clean-tab evictions.
//!
//! It owns the *bookkeeping* of that loop — the guard flags, the re-arm
//! handshake, the projected-byte running total. It owns **no decisions**: every
//! branch below asks `super::policy`.

use std::collections::{HashMap, HashSet, VecDeque};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::model::editor_memory::{EditorMemoryBudgetDecision, EditorMemoryBudgetOutcome};
use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;
use super::policy::{self, ContinuationStep, EvictionRecheck};

/// Weak handles to the pages one plan may touch.
type CandidatePages = HashMap<
    usize,
    (
        glib::WeakRef<libadwaita::TabPage>,
        glib::WeakRef<LushtextEditorPage>,
    ),
>;

/// Begin applying one over-budget decision, one candidate per idle turn.
///
/// The caller has already set `evaluation_running` and recorded the decision's
/// outcome; this function takes ownership of finishing the pass, including
/// clearing that guard on every exit path.
pub(super) fn apply_decision(window: &LushtextWindow, decision: EditorMemoryBudgetDecision) {
    // Build widget lookups only for an over-budget decision. The ordinary edit
    // path therefore pays for one scalar tab walk and no hash tables.
    let pages_by_editor = candidate_pages(window, &decision);
    let mut candidates = VecDeque::from(decision.candidates);
    let mut actual_projected = decision.total_bytes;
    let window_weak = window.downgrade();

    glib::idle_add_local(move || {
        let Some(window) = window_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let memory = &window.imp().editor_memory;
        #[cfg(feature = "test-utils")]
        memory
            .eviction_dispatch_count
            .set(memory.eviction_dispatch_count.get().wrapping_add(1));

        // Any transition outside `evict()` invalidates aggregate totals and
        // candidate ordering. Resnapshot instead of applying a stale plan.
        if memory.evaluation_armed.replace(false) {
            finish(&window, EditorMemoryBudgetOutcome::NoProgress);
            window.schedule_editor_memory_evaluation();
            return glib::ControlFlow::Break;
        }

        let Some(candidate) = candidates.pop_front() else {
            finish(&window, EditorMemoryBudgetOutcome::NoProgress);
            return glib::ControlFlow::Break;
        };

        if let Some((page_weak, editor_weak)) = pages_by_editor.get(&candidate.editor_id)
            && let (Some(page), Some(editor)) = (page_weak.upgrade(), editor_weak.upgrade())
        {
            let is_active = window.imp().tab_view.selected_page().as_ref() == Some(&page);
            let recheck = EvictionRecheck {
                // Widget ancestry proves current membership without scanning
                // every open tab on each bounded continuation callback.
                still_attached: editor.is_ancestor(&*window.imp().tab_view),
                same_child: page.child().as_ptr() == editor.upcast_ref::<gtk4::Widget>().as_ptr(),
                is_active,
                generations_unchanged: editor.memory_access_generation()
                    == candidate.access_generation
                    && editor.memory_policy_generation() == candidate.policy_generation,
                still_reloadable: editor.eligible_for_memory_eviction(is_active),
            };

            if policy::candidate_is_still_evictable(recheck) {
                tracing::info!("Evicting tab to free memory: {}", editor.title());
                let before = editor.estimated_live_buffer_bytes();
                memory.applying_eviction.set(true);
                editor.evict();
                memory.applying_eviction.set(false);

                if editor.buffer_replacement_in_progress() {
                    // A document-sized clear owns later GTK slices. Stop this
                    // pass until its terminal memory notification resnapshots
                    // residency and eligibility.
                    finish(&window, EditorMemoryBudgetOutcome::NoProgress);
                    return glib::ControlFlow::Break;
                }

                #[cfg(feature = "test-utils")]
                if let Some(hook) = memory.after_eviction_hook.borrow_mut().take() {
                    hook();
                }

                let reclaimed = before.saturating_sub(editor.estimated_live_buffer_bytes());
                actual_projected = actual_projected.saturating_sub(reclaimed);
            }
        }

        match policy::continuation_step(actual_projected, !candidates.is_empty()) {
            ContinuationStep::Continue => glib::ControlFlow::Continue,
            step => {
                let outcome = step
                    .outcome()
                    .unwrap_or(EditorMemoryBudgetOutcome::NoProgress);
                finish(&window, outcome);
                // A transition that arrived while this pass ran gets its own
                // pass rather than being folded into a plan built before it.
                if memory.evaluation_armed.replace(false) {
                    window.schedule_editor_memory_evaluation();
                }
                glib::ControlFlow::Break
            }
        }
    });
}

/// Record a pass outcome and release the overlap guard.
///
/// Written once because every exit path owes both, and a path that recorded the
/// outcome without releasing the guard would wedge the workflow: no later
/// residency change could ever start another pass.
fn finish(window: &LushtextWindow, outcome: EditorMemoryBudgetOutcome) {
    let memory = &window.imp().editor_memory;
    memory.last_outcome.set(outcome);
    memory.evaluation_running.set(false);
}

/// Weak page and editor handles for exactly the planned candidates.
fn candidate_pages(
    window: &LushtextWindow,
    decision: &EditorMemoryBudgetDecision,
) -> CandidatePages {
    let candidate_ids = decision
        .candidates
        .iter()
        .map(|candidate| candidate.editor_id)
        .collect::<HashSet<_>>();

    let tab_view = &window.imp().tab_view;
    let mut pages = HashMap::with_capacity(candidate_ids.len());
    for index in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(index);
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
            let editor_id = editor.as_ptr() as usize;
            if candidate_ids.contains(&editor_id) {
                pages.insert(editor_id, (page.downgrade(), editor.downgrade()));
            }
        }
    }
    pages
}

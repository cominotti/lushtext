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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;
use lushtext_model::document::editor_memory::{
    EditorMemoryBudgetDecision, EditorMemoryBudgetOutcome, EditorResidencyLedger,
};
use lushtext_model::values::{AccessGeneration, EditorId};

use super::super::LushtextWindow;
use super::policy::{self, ContinuationStep, EvictionRecheck};

/// Evaluation pass lifecycle for editor-memory eviction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum EditorMemoryEvaluationPhase {
    #[default]
    Idle,
    /// A coalesced next-idle aggregate evaluation pass is queued.
    Scheduled,
    /// An aggregate evaluation and/or bounded candidate eviction pass is active.
    Running {
        /// A foreign residency transition arrived and requires a fresh pass.
        rearm_after: bool,
        /// `EditorPage::evict` is running synchronously right now.
        applying_eviction: bool,
    },
}

/// One-shot race injectors used by `test-utils` stale-plan widget tests.
#[cfg(feature = "test-utils")]
#[derive(Default)]
pub(super) struct EditorMemoryTestHooks {
    pub(super) before_eviction: Option<Box<dyn FnOnce()>>,
    pub(super) after_eviction: Option<Box<dyn FnOnce()>>,
}

/// Editor-memory accounting shared by the eviction helpers.
#[derive(Default)]
pub struct EditorMemoryState {
    /// Algebraic pass lifecycle (`Idle`, `Scheduled`, or `Running`).
    pub(super) phase: Cell<EditorMemoryEvaluationPhase>,
    /// Constant-work per-editor residency records and saturating aggregates.
    pub(in crate::ui::window) ledger: RefCell<EditorResidencyLedger>,
    /// Previously selected editor, retained weakly for O(1) eligibility refresh.
    pub(super) active_editor: RefCell<Option<glib::WeakRef<LushtextEditorPage>>>,
    /// Exceptional lifecycle state requiring a current full reconciliation.
    pub(super) accounting_uncertain: Cell<bool>,
    /// Window-wide generation source used for deterministic least-recent use.
    pub(super) next_access_generation: Cell<AccessGeneration>,
    /// Number of aggregate evaluations, retained for coalescing assertions.
    pub(super) evaluation_count: Cell<u64>,
    /// Full tab scans performed only for enforcement or reconciliation.
    pub(super) full_scan_count: Cell<u64>,
    /// Stable result of the most recently completed aggregate policy pass.
    pub(super) last_outcome: Cell<EditorMemoryBudgetOutcome>,
    /// Idle dispatches used to prove that candidate application stays bounded.
    #[cfg(feature = "test-utils")]
    pub(super) eviction_dispatch_count: Cell<u64>,
    /// One-shot race injectors for stale-plan tests.
    #[cfg(feature = "test-utils")]
    pub(super) hooks: RefCell<EditorMemoryTestHooks>,
}

impl EditorMemoryState {
    pub(super) fn evaluation_running(&self) -> bool {
        matches!(
            self.phase.get(),
            EditorMemoryEvaluationPhase::Running { .. }
        )
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn evaluation_armed(&self) -> bool {
        matches!(
            self.phase.get(),
            EditorMemoryEvaluationPhase::Scheduled
                | EditorMemoryEvaluationPhase::Running {
                    rearm_after: true,
                    ..
                }
        )
    }

    pub(super) fn applying_eviction(&self) -> bool {
        matches!(
            self.phase.get(),
            EditorMemoryEvaluationPhase::Running {
                applying_eviction: true,
                ..
            }
        )
    }

    pub(super) fn set_rearm_after(&self) {
        if let EditorMemoryEvaluationPhase::Running {
            applying_eviction, ..
        } = self.phase.get()
        {
            self.phase.set(EditorMemoryEvaluationPhase::Running {
                rearm_after: true,
                applying_eviction,
            });
        }
    }

    pub(super) fn set_applying_eviction(&self, applying: bool) {
        if let EditorMemoryEvaluationPhase::Running { rearm_after, .. } = self.phase.get() {
            self.phase.set(EditorMemoryEvaluationPhase::Running {
                rearm_after,
                applying_eviction: applying,
            });
        }
    }

    pub(super) fn take_rearm_after(&self) -> bool {
        match self.phase.get() {
            EditorMemoryEvaluationPhase::Running {
                rearm_after: true,
                applying_eviction,
            } => {
                self.phase.set(EditorMemoryEvaluationPhase::Running {
                    rearm_after: false,
                    applying_eviction,
                });
                true
            }
            EditorMemoryEvaluationPhase::Scheduled => {
                self.phase.set(EditorMemoryEvaluationPhase::Idle);
                true
            }
            _ => false,
        }
    }
}

/// Weak handles to the pages one plan may touch.
type CandidatePages = HashMap<
    EditorId,
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
    let mut actual_projected = decision.total_bytes.as_bytes();
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
        if memory.take_rearm_after() {
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
                memory.set_applying_eviction(true);
                editor.evict();
                memory.set_applying_eviction(false);

                if editor.buffer_replacement_in_progress() {
                    // A document-sized clear owns later GTK slices. Stop this
                    // pass until its terminal memory notification resnapshots
                    // residency and eligibility.
                    finish(&window, EditorMemoryBudgetOutcome::NoProgress);
                    return glib::ControlFlow::Break;
                }

                #[cfg(feature = "test-utils")]
                {
                    let hook = memory.hooks.borrow_mut().after_eviction.take();
                    if let Some(hook) = hook {
                        hook();
                    }
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
                let rearm = finish(&window, outcome);
                // A transition that arrived while this pass ran gets its own
                // pass rather than being folded into a plan built before it.
                if rearm {
                    window.schedule_editor_memory_evaluation();
                }
                glib::ControlFlow::Break
            }
        }
    });
}

/// Record a pass outcome and release the overlap guard. Returns whether a
/// foreign transition arrived while the pass was running.
///
/// Written once because every exit path owes both, and a path that recorded the
/// outcome without releasing the guard would wedge the workflow: no later
/// residency change could ever start another pass.
fn finish(window: &LushtextWindow, outcome: EditorMemoryBudgetOutcome) -> bool {
    let memory = &window.imp().editor_memory;
    memory.last_outcome.set(outcome);
    let rearm = memory.take_rearm_after();
    memory.phase.set(EditorMemoryEvaluationPhase::Idle);
    rearm
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
            let editor_id = EditorId::new(editor.as_ptr() as usize);
            if candidate_ids.contains(&editor_id) {
                pages.insert(editor_id, (page.downgrade(), editor.downgrade()));
            }
        }
    }
    pages
}

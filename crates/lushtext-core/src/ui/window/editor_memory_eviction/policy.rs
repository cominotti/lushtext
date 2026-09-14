// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — when the shell evicts, and when it must not.
//!
//! The LRU ranking itself lives in `model::editor_memory`, which the census
//! resolved as `exempt` domain and which stays there. What lived inline in the
//! GTK adapter, and is separated here, is the *orchestration* decision set: when
//! a residency change is worth an aggregate pass, what an untracked editor
//! leaves behind, whether a planned candidate is still safe to evict by the time
//! its turn arrives, and when a pass has converged.
//!
//! **The revalidation rule is the data-safety-critical one.** Eviction clears a
//! live `GtkTextBuffer`. The plan is built in one main-loop turn and applied over
//! many, so between planning and application a tab can be closed, replaced,
//! re-selected, edited, or made unrecoverable. Every one of those must veto the
//! eviction, and a missing veto is silent: the buffer is cleared and the user
//! loses whatever the editor could no longer reload.
//!
//! This module imports no toolkit crate, which is what keeps it inside the
//! `ui/**/policy.rs` mutation scope.

use crate::model::editor_memory::{
    EDITOR_MEMORY_LOWER_WATER_BYTES, EDITOR_MEMORY_UPPER_BUDGET_BYTES, EditorMemoryBudgetOutcome,
};

/// Live facts about one planned candidate, observed when its turn arrives.
///
/// Seam value object. These five booleans are gathered at the top of the
/// continuation callback and consumed by one predicate — but they are five
/// same-typed values whose meanings are not interchangeable, which is the shape
/// where a swap compiles, passes every test that only asserts *something* was
/// evicted, and clears the wrong buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvictionRecheck {
    /// The editor is still a descendant of the live tab view.
    pub still_attached: bool,
    /// The page still holds this exact editor as its child.
    pub same_child: bool,
    /// The editor is the currently selected page.
    pub is_active: bool,
    /// Neither the access nor the policy generation moved since planning.
    pub generations_unchanged: bool,
    /// The editor is still recoverable and otherwise eligible.
    pub still_reloadable: bool,
}

/// Whether a planned candidate may still be evicted.
///
/// Every clause is a veto. `is_active` is inverted deliberately: the selected
/// page is never evicted, so being active is a reason to stop, while the other
/// four are reasons to proceed.
#[must_use]
pub fn candidate_is_still_evictable(recheck: EvictionRecheck) -> bool {
    recheck.still_attached
        && recheck.same_child
        && !recheck.is_active
        && recheck.generations_unchanged
        && recheck.still_reloadable
}

/// Whether an aggregate pass is worth scheduling for these totals.
///
/// Uncertain accounting forces a pass regardless of the total, because the total
/// is exactly what cannot be trusted in that state.
#[must_use]
pub fn aggregate_pass_is_warranted(total_bytes: u64, accounting_uncertain: bool) -> bool {
    total_bytes > EDITOR_MEMORY_UPPER_BUDGET_BYTES || accounting_uncertain
}

/// What removing one editor's record leaves the workflow to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UntrackDisposition {
    /// A pass is running; re-arm it so it resnapshots instead of applying a
    /// plan whose membership just changed.
    RearmRunningPass,
    /// A pass is running and *this* removal is the eviction the pass is
    /// applying, so it is not a foreign transition and must not re-arm.
    IgnoreOwnEviction,
    /// No pass is running and the remaining totals warrant one.
    SchedulePass,
    /// No pass is running and nothing needs doing.
    RecordWithinBudget,
}

/// Decide what an untracked editor leaves behind.
#[must_use]
pub fn untrack_disposition(
    evaluation_running: bool,
    applying_eviction: bool,
    remaining_total_bytes: Option<u64>,
    accounting_uncertain: bool,
) -> UntrackDisposition {
    if evaluation_running {
        if applying_eviction {
            return UntrackDisposition::IgnoreOwnEviction;
        }
        return UntrackDisposition::RearmRunningPass;
    }
    let warranted = remaining_total_bytes
        .is_some_and(|total| total > EDITOR_MEMORY_UPPER_BUDGET_BYTES)
        || accounting_uncertain;
    if warranted {
        UntrackDisposition::SchedulePass
    } else {
        UntrackDisposition::RecordWithinBudget
    }
}

/// How one continuation turn ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationStep {
    /// Enough was reclaimed; the pass converged.
    Converged,
    /// Nothing left to try and still over the water line.
    Exhausted,
    /// Still over budget with candidates remaining.
    Continue,
}

impl ContinuationStep {
    /// The outcome a finished pass records, or `None` while it continues.
    #[must_use]
    pub fn outcome(self) -> Option<EditorMemoryBudgetOutcome> {
        match self {
            Self::Converged => Some(EditorMemoryBudgetOutcome::Converged),
            Self::Exhausted => Some(EditorMemoryBudgetOutcome::NoProgress),
            Self::Continue => None,
        }
    }
}

/// Decide whether the bounded continuation keeps going.
#[must_use]
pub fn continuation_step(projected_bytes: u64, candidates_remaining: bool) -> ContinuationStep {
    if projected_bytes <= EDITOR_MEMORY_LOWER_WATER_BYTES {
        ContinuationStep::Converged
    } else if candidates_remaining {
        ContinuationStep::Continue
    } else {
        ContinuationStep::Exhausted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn all_clear() -> EvictionRecheck {
        EvictionRecheck {
            still_attached: true,
            same_child: true,
            is_active: false,
            generations_unchanged: true,
            still_reloadable: true,
        }
    }

    #[test]
    fn a_fully_clear_candidate_is_evictable() {
        assert!(candidate_is_still_evictable(all_clear()));
    }

    #[test]
    fn every_clause_is_an_independent_veto() {
        // Each field flipped alone must block the eviction. Five same-typed
        // booleans is exactly the shape where a swapped pair passes a test that
        // only checks the all-clear and one arbitrary failure.
        let mut detached = all_clear();
        detached.still_attached = false;
        let mut replaced = all_clear();
        replaced.same_child = false;
        let mut reselected = all_clear();
        reselected.is_active = true;
        let mut edited = all_clear();
        edited.generations_unchanged = false;
        let mut unreloadable = all_clear();
        unreloadable.still_reloadable = false;

        for (name, recheck) in [
            ("detached", detached),
            ("replaced child", replaced),
            ("re-selected", reselected),
            ("edited since planning", edited),
            ("no longer reloadable", unreloadable),
        ] {
            assert!(
                !candidate_is_still_evictable(recheck),
                "{name} must veto the eviction"
            );
        }
    }

    #[test]
    fn the_active_page_is_never_evicted() {
        let mut recheck = all_clear();
        recheck.is_active = true;
        assert!(!candidate_is_still_evictable(recheck));
    }

    #[test]
    fn a_pass_is_warranted_over_budget_or_when_accounting_is_uncertain() {
        assert!(!aggregate_pass_is_warranted(0, false));
        assert!(!aggregate_pass_is_warranted(
            EDITOR_MEMORY_UPPER_BUDGET_BYTES,
            false
        ));
        assert!(aggregate_pass_is_warranted(
            EDITOR_MEMORY_UPPER_BUDGET_BYTES + 1,
            false
        ));
        assert!(
            aggregate_pass_is_warranted(0, true),
            "an untrustworthy total must force the pass that recomputes it"
        );
    }

    #[test]
    fn an_untrack_during_our_own_eviction_does_not_rearm() {
        assert_eq!(
            untrack_disposition(true, true, Some(u64::MAX), true),
            UntrackDisposition::IgnoreOwnEviction
        );
    }

    #[test]
    fn a_foreign_untrack_during_a_pass_rearms_it() {
        assert_eq!(
            untrack_disposition(true, false, Some(0), false),
            UntrackDisposition::RearmRunningPass,
            "membership changed under a running plan; it must resnapshot"
        );
    }

    #[test]
    fn an_idle_untrack_schedules_only_when_warranted() {
        assert_eq!(
            untrack_disposition(
                false,
                false,
                Some(EDITOR_MEMORY_UPPER_BUDGET_BYTES + 1),
                false
            ),
            UntrackDisposition::SchedulePass
        );
        assert_eq!(
            untrack_disposition(false, false, Some(0), true),
            UntrackDisposition::SchedulePass
        );
        assert_eq!(
            untrack_disposition(false, false, Some(0), false),
            UntrackDisposition::RecordWithinBudget
        );
        assert_eq!(
            untrack_disposition(false, false, None, false),
            UntrackDisposition::RecordWithinBudget,
            "no remaining record and certain accounting needs no pass"
        );
    }

    #[test]
    fn the_continuation_converges_at_or_below_the_lower_water_line() {
        assert_eq!(
            continuation_step(EDITOR_MEMORY_LOWER_WATER_BYTES, true),
            ContinuationStep::Converged
        );
        assert_eq!(
            continuation_step(EDITOR_MEMORY_LOWER_WATER_BYTES + 1, true),
            ContinuationStep::Continue
        );
        assert_eq!(
            continuation_step(EDITOR_MEMORY_LOWER_WATER_BYTES + 1, false),
            ContinuationStep::Exhausted
        );
    }

    #[test]
    fn convergence_beats_exhaustion_when_both_could_apply() {
        // Under the water line with nothing left is Converged, not Exhausted:
        // the pass achieved its goal and must not record no-progress.
        assert_eq!(continuation_step(0, false), ContinuationStep::Converged);
    }

    #[test]
    fn only_a_finished_step_records_an_outcome() {
        assert_eq!(
            ContinuationStep::Converged.outcome(),
            Some(EditorMemoryBudgetOutcome::Converged)
        );
        assert_eq!(
            ContinuationStep::Exhausted.outcome(),
            Some(EditorMemoryBudgetOutcome::NoProgress)
        );
        assert_eq!(ContinuationStep::Continue.outcome(), None);
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust policy for bounding aggregate live editor-buffer residency.
//!
//! GTK adapters provide scalar snapshots to this module, which keeps least-
//! recently-used selection, hysteresis, and protected-work behavior fully
//! deterministic without retaining widgets or reading document text.

/// Aggregate live-editor estimate that starts safe background eviction.
///
/// 256 MiB preserves the established ceiling for keeping aggregate editor-text
/// residency comfortable on 8 GiB-class systems without ordinary-tab churn.
pub const EDITOR_MEMORY_UPPER_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

/// Target residency after an over-budget eviction pass.
///
/// Ninety percent leaves enough hysteresis to prevent estimate noise near the
/// upper threshold from repeatedly evicting one tab at a time.
pub const EDITOR_MEMORY_LOWER_WATER_BYTES: u64 =
    EDITOR_MEMORY_UPPER_BUDGET_BYTES.saturating_mul(90) / 100;

/// Fixed estimate retained for an evicted tab's scalar bookkeeping.
///
/// Four KiB acknowledges the page and tab metadata that remain after buffer
/// text is dropped without pretending to measure GTK allocator residency.
pub const EVICTED_EDITOR_BOOKKEEPING_BYTES: u64 = 4 * 1024;

/// Estimate one editor from bounded scalar state without reading its text.
///
/// Four bytes per Unicode scalar is a conservative UTF-8 bound. Keeping the
/// known file size as a floor prevents a clean loaded file from appearing
/// smaller than its last accepted on-disk representation.
#[must_use]
pub fn estimate_live_editor_bytes(
    character_count: u64,
    known_file_bytes: Option<u64>,
    evicted: bool,
) -> u64 {
    if evicted {
        return EVICTED_EDITOR_BOOKKEEPING_BYTES;
    }

    character_count
        .saturating_mul(4)
        .max(known_file_bytes.unwrap_or(0))
}

/// Scalar residency and safety facts for one open editor page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorResidency {
    /// Window-local stable identity used to re-find the page before eviction.
    pub editor_id: usize,
    /// Conservative current buffer estimate, excluding unrelated process RAM.
    pub estimated_bytes: u64,
    /// Window-wide recency generation; smaller values are less recently used.
    pub access_generation: u64,
    /// Page-local generation covering residency and eviction eligibility.
    pub policy_generation: u64,
    /// Whether current content can be discarded and reloaded without data loss.
    pub eligible_for_eviction: bool,
}

/// One least-recently-used eviction selected from a scalar snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorEvictionCandidate {
    /// Window-local editor identity captured by the policy pass.
    pub editor_id: usize,
    /// Estimate actually reclaimed after retained tab bookkeeping is deducted.
    pub reclaimable_bytes: u64,
    /// Captured recency generation used to reject newly accessed pages.
    pub access_generation: u64,
    /// Captured policy generation used to reject any eligibility transition.
    pub policy_generation: u64,
}

/// Stable result class for one aggregate policy evaluation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditorMemoryBudgetOutcome {
    /// Aggregate residency is at or below the upper threshold.
    #[default]
    WithinBudget,
    /// Selected candidates can bring the aggregate to the lower watermark.
    Converged,
    /// Safe candidates cannot currently reach the lower watermark.
    NoProgress,
}

/// Complete deterministic decision returned to the GTK window adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorMemoryBudgetDecision {
    /// Saturating aggregate estimate across every supplied editor.
    pub total_bytes: u64,
    /// Saturating aggregate estimate for protected editors only.
    pub protected_bytes: u64,
    /// Projected total after every selected candidate is evicted.
    pub projected_bytes: u64,
    /// Least-recently-used candidates in application order.
    pub candidates: Vec<EditorEvictionCandidate>,
    /// Whether the pass is already safe, converges, or must remain soft.
    pub outcome: EditorMemoryBudgetOutcome,
}

/// Evaluate one immutable editor snapshot against the aggregate memory policy.
///
/// Totals saturate instead of wrapping, and ties use editor identity so the
/// same input always produces the same candidate order.
#[must_use]
pub fn evaluate_editor_memory_budget(pages: &[EditorResidency]) -> EditorMemoryBudgetDecision {
    let total_bytes = pages.iter().fold(0u64, |total, page| {
        total.saturating_add(page.estimated_bytes)
    });
    let protected_bytes = pages
        .iter()
        .filter(|page| !page.eligible_for_eviction)
        .fold(0u64, |total, page| {
            total.saturating_add(page.estimated_bytes)
        });

    if total_bytes <= EDITOR_MEMORY_UPPER_BUDGET_BYTES {
        return EditorMemoryBudgetDecision {
            total_bytes,
            protected_bytes,
            projected_bytes: total_bytes,
            candidates: Vec::new(),
            outcome: EditorMemoryBudgetOutcome::WithinBudget,
        };
    }

    let mut eligible = pages
        .iter()
        .copied()
        .filter(|page| {
            page.eligible_for_eviction && page.estimated_bytes > EVICTED_EDITOR_BOOKKEEPING_BYTES
        })
        .collect::<Vec<_>>();
    // Select oldest access first with editor identity as a stable tie-breaker.
    // Continue to low water so small estimate changes do not retrigger eviction.
    eligible.sort_unstable_by_key(|page| (page.access_generation, page.editor_id));

    let mut projected_bytes = total_bytes;
    let mut candidates = Vec::new();
    for page in eligible {
        if projected_bytes <= EDITOR_MEMORY_LOWER_WATER_BYTES {
            break;
        }
        let reclaimable_bytes = page
            .estimated_bytes
            .saturating_sub(EVICTED_EDITOR_BOOKKEEPING_BYTES);
        projected_bytes = projected_bytes.saturating_sub(reclaimable_bytes);
        candidates.push(EditorEvictionCandidate {
            editor_id: page.editor_id,
            reclaimable_bytes,
            access_generation: page.access_generation,
            policy_generation: page.policy_generation,
        });
    }

    let outcome = if projected_bytes <= EDITOR_MEMORY_LOWER_WATER_BYTES {
        EditorMemoryBudgetOutcome::Converged
    } else {
        EditorMemoryBudgetOutcome::NoProgress
    };
    EditorMemoryBudgetDecision {
        total_bytes,
        protected_bytes,
        projected_bytes,
        candidates,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(
        editor_id: usize,
        estimated_bytes: u64,
        access_generation: u64,
        eligible_for_eviction: bool,
    ) -> EditorResidency {
        EditorResidency {
            editor_id,
            estimated_bytes,
            access_generation,
            policy_generation: 7,
            eligible_for_eviction,
        }
    }

    #[test]
    fn zero_one_and_exact_threshold_pages_stay_within_budget() {
        for pages in [
            Vec::new(),
            vec![page(1, 1, 1, true)],
            vec![page(1, EDITOR_MEMORY_UPPER_BUDGET_BYTES, 1, true)],
        ] {
            let decision = evaluate_editor_memory_budget(&pages);
            assert_eq!(decision.outcome, EditorMemoryBudgetOutcome::WithinBudget);
            assert!(decision.candidates.is_empty());
        }
    }

    #[test]
    fn many_pages_evict_lru_until_the_lower_watermark() {
        let quarter = EDITOR_MEMORY_UPPER_BUDGET_BYTES / 4;
        let decision = evaluate_editor_memory_budget(&[
            page(1, quarter, 1, true),
            page(2, quarter, 2, true),
            page(3, quarter, 3, true),
            page(4, quarter, 4, true),
            page(5, quarter, 5, true),
        ]);

        assert_eq!(decision.outcome, EditorMemoryBudgetOutcome::Converged);
        assert_eq!(decision.candidates.len(), 2);
        assert_eq!(decision.candidates[0].editor_id, 1);
        assert_eq!(decision.candidates[1].editor_id, 2);
        assert!(decision.projected_bytes <= EDITOR_MEMORY_LOWER_WATER_BYTES);
    }

    #[test]
    fn recency_ties_use_editor_identity_for_determinism() {
        let decision = evaluate_editor_memory_budget(&[
            page(9, EDITOR_MEMORY_UPPER_BUDGET_BYTES, 4, true),
            page(3, EVICTED_EDITOR_BOOKKEEPING_BYTES + 1, 4, true),
        ]);

        assert_eq!(decision.candidates[0].editor_id, 3);
        assert_eq!(decision.candidates[1].editor_id, 9);
    }

    #[test]
    fn totals_saturate_instead_of_wrapping() {
        let decision = evaluate_editor_memory_budget(&[
            page(1, u64::MAX, 1, false),
            page(2, u64::MAX, 2, false),
        ]);

        assert_eq!(decision.total_bytes, u64::MAX);
        assert_eq!(decision.protected_bytes, u64::MAX);
        assert_eq!(decision.outcome, EditorMemoryBudgetOutcome::NoProgress);
    }

    #[test]
    fn insufficient_candidates_report_stable_no_progress() {
        let protected = EDITOR_MEMORY_UPPER_BUDGET_BYTES;
        let decision =
            evaluate_editor_memory_budget(&[page(1, protected, 1, false), page(2, 1, 2, true)]);

        assert!(decision.candidates.is_empty());
        assert_eq!(decision.projected_bytes, protected + 1);
        assert_eq!(decision.outcome, EditorMemoryBudgetOutcome::NoProgress);
    }

    #[test]
    fn candidates_must_reclaim_more_than_retained_bookkeeping() {
        let decision = evaluate_editor_memory_budget(&[
            page(1, EDITOR_MEMORY_UPPER_BUDGET_BYTES, 1, false),
            page(2, EVICTED_EDITOR_BOOKKEEPING_BYTES - 1, 2, true),
            page(3, EVICTED_EDITOR_BOOKKEEPING_BYTES, 3, true),
            page(4, EVICTED_EDITOR_BOOKKEEPING_BYTES + 1, 4, true),
        ]);

        assert_eq!(decision.candidates.len(), 1);
        assert_eq!(decision.candidates[0].editor_id, 4);
        assert_eq!(decision.candidates[0].reclaimable_bytes, 1);
        assert_eq!(decision.outcome, EditorMemoryBudgetOutcome::NoProgress);
    }

    #[test]
    fn protected_over_budget_state_never_selects_user_work() {
        let decision = evaluate_editor_memory_budget(&[page(
            1,
            EDITOR_MEMORY_UPPER_BUDGET_BYTES + 1,
            1,
            false,
        )]);

        assert!(decision.candidates.is_empty());
        assert_eq!(decision.outcome, EditorMemoryBudgetOutcome::NoProgress);
        assert_eq!(decision.protected_bytes, decision.total_bytes);
    }

    #[test]
    fn live_estimate_covers_untitled_growth_file_floor_and_eviction() {
        assert_eq!(estimate_live_editor_bytes(3, None, false), 12);
        assert_eq!(estimate_live_editor_bytes(3, Some(100), false), 100);
        assert_eq!(estimate_live_editor_bytes(30, Some(100), false), 120);
        assert_eq!(estimate_live_editor_bytes(u64::MAX, None, false), u64::MAX);
        assert_eq!(
            estimate_live_editor_bytes(u64::MAX, Some(u64::MAX), true),
            EVICTED_EDITOR_BOOKKEEPING_BYTES
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust policy for bounding aggregate live editor-buffer residency.
//!
//! GTK adapters provide scalar snapshots to this module, which keeps least-
//! recently-used selection, hysteresis, and protected-work behavior fully
//! deterministic without retaining widgets or reading document text.

#![deny(clippy::float_arithmetic)]

use std::collections::BTreeMap;

#[cfg(kani)]
mod kani_proofs;

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

/// Result of one constant-work residency-ledger mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorResidencyUpdate {
    /// Saturating aggregate before the mutation.
    pub previous_total_bytes: u64,
    /// Saturating aggregate after the mutation.
    pub total_bytes: u64,
    /// Whether the update moved the aggregate across the enforcement threshold.
    pub crossed_upper_threshold: bool,
}

/// Incremental scalar residency records for one application window.
///
/// Exact `u128` accumulators make a later decrement trustworthy even while the
/// public aggregate is saturated at `u64::MAX`. The number of GTK editor pages
/// cannot approach the wider integer's capacity, so ordinary mutations remain
/// constant work relative to the open-tab count.
///
/// The map holds the records; the arithmetic lives in [`ResidencyTotals`],
/// which every upsert and remove drives with the record the map displaced.
/// Kani proves that arithmetic over every sequence of three upserts or
/// removes on two identities with every `u64` estimate: the saturating totals
/// equal a recomputation over the records and the totals
/// [`evaluate_editor_memory_budget`] computes from them, and
/// `crossed_upper_threshold` is exact (`editor_memory/kani_proofs.rs`). The
/// `BTreeMap` itself is trusted: a second insert into it exhausted 20 GiB in
/// CBMC, so the harnesses drive the totals with a fixed-array record set that
/// displaces records exactly as the map does.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditorResidencyLedger {
    records: BTreeMap<usize, EditorResidency>,
    totals: ResidencyTotals,
}

/// Exact aggregate accounting for [`EditorResidencyLedger`].
///
/// Exact `u128` accumulators make a later decrement trustworthy even while the
/// public aggregate is saturated at `u64::MAX`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ResidencyTotals {
    exact_total_bytes: u128,
    exact_protected_bytes: u128,
}

impl ResidencyTotals {
    /// Account for one record replacing another: `previous` is the record the
    /// map displaced (if any), `next` the record it now holds (if any).
    fn replace(
        &mut self,
        previous: Option<&EditorResidency>,
        next: Option<&EditorResidency>,
    ) -> EditorResidencyUpdate {
        let previous_total_bytes = self.total_bytes();
        if let Some(previous) = previous {
            self.exact_total_bytes = self
                .exact_total_bytes
                .saturating_sub(u128::from(previous.estimated_bytes));
            if !previous.eligible_for_eviction {
                self.exact_protected_bytes = self
                    .exact_protected_bytes
                    .saturating_sub(u128::from(previous.estimated_bytes));
            }
        }
        if let Some(next) = next {
            self.exact_total_bytes = self
                .exact_total_bytes
                .saturating_add(u128::from(next.estimated_bytes));
            if !next.eligible_for_eviction {
                self.exact_protected_bytes = self
                    .exact_protected_bytes
                    .saturating_add(u128::from(next.estimated_bytes));
            }
        }
        let total_bytes = self.total_bytes();
        EditorResidencyUpdate {
            previous_total_bytes,
            total_bytes,
            // Only an upsert can start enforcement; a removal never does.
            crossed_upper_threshold: next.is_some()
                && previous_total_bytes <= EDITOR_MEMORY_UPPER_BUDGET_BYTES
                && total_bytes > EDITOR_MEMORY_UPPER_BUDGET_BYTES,
        }
    }

    fn total_bytes(&self) -> u64 {
        u64::try_from(self.exact_total_bytes).unwrap_or(u64::MAX)
    }

    fn protected_bytes(&self) -> u64 {
        u64::try_from(self.exact_protected_bytes).unwrap_or(u64::MAX)
    }
}

impl EditorResidencyLedger {
    /// Insert or replace one current editor record using exact scalar deltas.
    pub fn upsert(&mut self, residency: EditorResidency) -> EditorResidencyUpdate {
        let previous = self.records.insert(residency.editor_id, residency);
        self.totals.replace(previous.as_ref(), Some(&residency))
    }

    /// Remove one detached editor record using the same exact delta path.
    pub fn remove(&mut self, editor_id: usize) -> Option<EditorResidencyUpdate> {
        let previous = self.records.remove(&editor_id)?;
        Some(self.totals.replace(Some(&previous), None))
    }

    /// Replace the ledger from a freshness-checked exceptional/full scan.
    pub fn reconcile(&mut self, records: impl IntoIterator<Item = EditorResidency>) {
        self.records.clear();
        self.totals = ResidencyTotals::default();
        for record in records {
            self.upsert(record);
        }
    }

    /// Return whether a stable editor identity is currently tracked.
    #[must_use]
    pub fn contains(&self, editor_id: usize) -> bool {
        self.records.contains_key(&editor_id)
    }

    /// Return the saturating aggregate across every tracked editor.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.totals.total_bytes()
    }

    /// Return the saturating aggregate for non-evictable editor residency.
    #[must_use]
    pub fn protected_bytes(&self) -> u64 {
        self.totals.protected_bytes()
    }

    /// Copy current scalar records only when policy enforcement needs a plan.
    #[must_use]
    pub fn snapshot(&self) -> Vec<EditorResidency> {
        self.records.values().copied().collect()
    }

    /// Number of scalar records, used by reconciliation evidence.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the ledger currently has no editor records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
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
///
/// Kani proves (`editor_memory/kani_proofs.rs`), over every `u64` estimate and
/// generation and every snapshot of up to three pages with distinct
/// identities: no protected or bookkeeping-sized page is selected; candidates
/// are the least-recently-used prefix in (access generation, identity) order;
/// selection stops at the lower watermark; the projected total is the
/// aggregate minus the reclaimed sum, saturating; and the outcome agrees with
/// it (`WithinBudget` at or below the upper budget, `Converged` exactly at or
/// below the watermark, `NoProgress` only once every eligible page is chosen).
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

    // Select oldest access first with editor identity as a stable tie-breaker,
    // and continue to low water so small estimate changes do not retrigger
    // eviction. Each step takes the least-recently-used remaining page, which
    // yields exactly the prefix a full sort would, in O(pages x candidates)
    // rather than O(pages log pages) (a pass usually selects one or two). It is
    // also what lets Kani check this function: `sort_unstable`'s pivot
    // recursion never terminated symbolic execution over a snapshot of
    // symbolic length (`editor_memory/kani_proofs.rs`).
    let mut projected_bytes = total_bytes;
    let mut candidates = Vec::new();
    while projected_bytes > EDITOR_MEMORY_LOWER_WATER_BYTES {
        let Some(index) = eligible
            .iter()
            .enumerate()
            .min_by_key(|(_, page)| (page.access_generation, page.editor_id))
            .map(|(index, _)| index)
        else {
            break;
        };
        let page = eligible.swap_remove(index);
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

    /// The selection the policy used before its least-recently-used loop
    /// replaced the full sort: sort every eligible page, take the prefix.
    fn sorted_prefix_reference(pages: &[EditorResidency]) -> Vec<usize> {
        let total = pages.iter().fold(0u64, |total, page| {
            total.saturating_add(page.estimated_bytes)
        });
        if total <= EDITOR_MEMORY_UPPER_BUDGET_BYTES {
            return Vec::new();
        }
        let mut eligible = pages
            .iter()
            .copied()
            .filter(|page| {
                page.eligible_for_eviction
                    && page.estimated_bytes > EVICTED_EDITOR_BOOKKEEPING_BYTES
            })
            .collect::<Vec<_>>();
        eligible.sort_unstable_by_key(|page| (page.access_generation, page.editor_id));
        let mut projected = total;
        let mut selected = Vec::new();
        for page in eligible {
            if projected <= EDITOR_MEMORY_LOWER_WATER_BYTES {
                break;
            }
            projected =
                projected.saturating_sub(page.estimated_bytes - EVICTED_EDITOR_BOOKKEEPING_BYTES);
            selected.push(page.editor_id);
        }
        selected
    }

    #[test]
    fn least_recently_used_loop_selects_the_sorted_prefix() {
        // A small deterministic generator: every snapshot of up to eight pages
        // mixes sizes around the budget, shared access generations (so the
        // identity tie-breaker decides), and protected pages.
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..2_000 {
            let count = usize::try_from(next() % 9).expect("a count below 9 fits usize");
            let mut ids: Vec<usize> = (0..count).collect();
            ids.reverse();
            let pages = ids
                .into_iter()
                .map(|editor_id| {
                    page(
                        editor_id,
                        next() % (EDITOR_MEMORY_UPPER_BUDGET_BYTES / 2),
                        next() % 4,
                        next() % 4 != 0,
                    )
                })
                .collect::<Vec<_>>();
            let selected = evaluate_editor_memory_budget(&pages)
                .candidates
                .iter()
                .map(|candidate| candidate.editor_id)
                .collect::<Vec<_>>();
            assert_eq!(selected, sorted_prefix_reference(&pages), "{pages:?}");
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

    #[test]
    fn incremental_ledger_updates_one_record_and_crosses_once() {
        let mut ledger = EditorResidencyLedger::default();
        let first = page(1, EDITOR_MEMORY_UPPER_BUDGET_BYTES - 10, 1, false);
        let second = page(2, 9, 2, true);
        assert!(!ledger.upsert(first).crossed_upper_threshold);
        assert!(!ledger.upsert(second).crossed_upper_threshold);

        let crossing = ledger.upsert(page(2, 11, 2, true));
        assert!(crossing.crossed_upper_threshold);
        assert_eq!(crossing.total_bytes, EDITOR_MEMORY_UPPER_BUDGET_BYTES + 1);
        assert_eq!(ledger.protected_bytes(), first.estimated_bytes);

        let later = ledger.upsert(page(2, 12, 3, false));
        assert!(!later.crossed_upper_threshold);
        assert_eq!(ledger.protected_bytes(), ledger.total_bytes());
    }

    #[test]
    fn incremental_ledger_removes_and_reconciles_records() {
        let mut ledger = EditorResidencyLedger::default();
        ledger.upsert(page(1, 10, 1, false));
        ledger.upsert(page(2, 20, 2, true));
        assert_eq!(ledger.remove(1).map(|update| update.total_bytes), Some(20));
        assert_eq!(ledger.protected_bytes(), 0);
        assert!(ledger.remove(1).is_none());

        ledger.reconcile([page(7, 30, 3, false), page(9, 40, 4, true)]);
        assert_eq!(ledger.len(), 2);
        assert!(ledger.contains(7));
        assert_eq!(ledger.total_bytes(), 70);
        assert_eq!(ledger.protected_bytes(), 30);
    }

    #[test]
    fn incremental_ledger_recovers_below_u64_saturation_after_removal() {
        let mut ledger = EditorResidencyLedger::default();
        ledger.upsert(page(1, u64::MAX, 1, false));
        ledger.upsert(page(2, u64::MAX, 2, true));
        assert_eq!(ledger.total_bytes(), u64::MAX);
        assert_eq!(ledger.protected_bytes(), u64::MAX);

        ledger.remove(1);
        assert_eq!(ledger.total_bytes(), u64::MAX);
        assert_eq!(ledger.protected_bytes(), 0);
        ledger.remove(2);
        assert_eq!(ledger.total_bytes(), 0);
        assert!(ledger.is_empty());
    }
}

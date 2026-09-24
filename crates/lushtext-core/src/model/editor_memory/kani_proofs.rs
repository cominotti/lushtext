// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the live editor-memory policy:
//! [`estimate_live_editor_bytes`], [`evaluate_editor_memory_budget`], and
//! [`EditorResidencyLedger`]'s accounting.
//!
//! Domain: every `u64` estimate, character count, file size, access
//! generation, and policy generation, so saturation is exercised. A snapshot
//! holds three pages whose editor identities are distinct values in `0..3`.
//! That also covers every smaller snapshot: a protected page with a zero
//! estimate adds nothing to either total and is never a candidate, so it is
//! indistinguishable from an absent page. The loops over a snapshot — the
//! totals' folds, the eligibility filter, and the least-recently-used
//! selection with its inner minimum search — run at most three times, so the
//! harnesses use `unwind(4)`. (The selection sorted every eligible page with
//! `sort_unstable_by_key` when these harnesses were written; symbolic
//! execution of the sort's pivot recursion did not finish in 20 minutes even
//! for two pages, so the policy now takes the least-recently-used remaining
//! page per step, which selects the same prefix.)
//!
//! Ledger sequences are three upserts or removes over identities `{0, 1}`.
//! They drive the production [`ResidencyTotals`] accounting through a
//! fixed-array record set that displaces records exactly as the ledger's
//! `BTreeMap` does. The map itself is trusted: a second insert into it
//! exhausted 20 GiB in CBMC, and three operations over the whole ledger passed
//! 55 GiB.
//!
//! The incremental enforcement scheduling in
//! `ui/window/editor_memory_eviction/` is GTK coordination, not this policy,
//! and is not modelled here.
//!
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds and tests
//! never see this module. The harnesses check the functions that ship.

use super::{
    EDITOR_MEMORY_LOWER_WATER_BYTES, EDITOR_MEMORY_UPPER_BUDGET_BYTES,
    EVICTED_EDITOR_BOOKKEEPING_BYTES, EditorMemoryBudgetDecision, EditorMemoryBudgetOutcome,
    EditorResidency, EditorResidencyLedger, EditorResidencyUpdate, ResidencyTotals,
    estimate_live_editor_bytes, evaluate_editor_memory_budget,
};

/// Pages in the largest modelled snapshot.
const PAGES: usize = 3;

/// Upserts or removes in the longest modelled ledger sequence (see the module
/// documentation for why it is two).
const LEDGER_OPERATIONS: usize = 3;

/// One page with every scalar arbitrary and the given identity.
fn any_page(editor_id: usize) -> EditorResidency {
    EditorResidency {
        editor_id,
        estimated_bytes: kani::any(),
        access_generation: kani::any(),
        policy_generation: kani::any(),
        eligible_for_eviction: kani::any(),
    }
}

/// A snapshot of [`PAGES`] pages with distinct identities in `0..PAGES`
/// (smaller snapshots are covered by zero-estimate protected pages).
struct Snapshot {
    pages: [EditorResidency; PAGES],
}

impl Snapshot {
    fn any() -> Self {
        let ids: [usize; PAGES] = [kani::any(), kani::any(), kani::any()];
        for id in ids {
            kani::assume(id < PAGES);
        }
        kani::assume(ids[0] != ids[1] && ids[0] != ids[2] && ids[1] != ids[2]);
        Self {
            pages: [any_page(ids[0]), any_page(ids[1]), any_page(ids[2])],
        }
    }

    fn pages(&self) -> &[EditorResidency] {
        &self.pages
    }
}

/// The eligibility filter as the spec states it.
fn eligible(page: &EditorResidency) -> bool {
    page.eligible_for_eviction && page.estimated_bytes > EVICTED_EDITOR_BOOKKEEPING_BYTES
}

/// The saturating aggregate of `pages`, computed exactly and then clamped.
fn saturating_total(pages: &[EditorResidency], only_protected: bool) -> u64 {
    let exact: u128 = pages
        .iter()
        .filter(|page| !only_protected || !page.eligible_for_eviction)
        .map(|page| u128::from(page.estimated_bytes))
        .sum();
    u64::try_from(exact).unwrap_or(u64::MAX)
}

fn selected(decision: &EditorMemoryBudgetDecision, editor_id: usize) -> bool {
    decision
        .candidates
        .iter()
        .any(|candidate| candidate.editor_id == editor_id)
}

/// An evicted editor's estimate is the bookkeeping figure; a loaded editor's
/// is at least its known file size and at least four bytes per character,
/// saturating rather than wrapping.
#[kani::proof]
fn estimate_is_bookkeeping_when_evicted_and_floored_by_file_size_otherwise() {
    let characters: u64 = kani::any();
    let known: Option<u64> = kani::any();
    let evicted: bool = kani::any();
    let estimate = estimate_live_editor_bytes(characters, known, evicted);
    if evicted {
        assert!(estimate == EVICTED_EDITOR_BOOKKEEPING_BYTES);
    } else {
        assert!(estimate >= known.unwrap_or(0));
        let four_per_character = u128::from(characters) * 4;
        let expected = four_per_character.max(u128::from(known.unwrap_or(0)));
        assert!(u128::from(estimate) == expected.min(u128::from(u64::MAX)));
    }
}

/// No selected candidate is protected, or at or below the bookkeeping figure.
#[kani::proof]
#[kani::unwind(4)]
fn budget_never_selects_protected_or_bookkeeping_pages() {
    let snapshot = Snapshot::any();
    let decision = evaluate_editor_memory_budget(snapshot.pages());
    for page in snapshot.pages() {
        if selected(&decision, page.editor_id) {
            assert!(eligible(page));
        }
    }
}

/// Candidates come in ascending (access generation, identity) order, and no
/// eligible page outside the selection is less recently used than a selected
/// one: the selection is a prefix of the least-recently-used order.
#[kani::proof]
#[kani::unwind(4)]
fn budget_selects_least_recently_used_first() {
    let snapshot = Snapshot::any();
    let decision = evaluate_editor_memory_budget(snapshot.pages());
    for pair in decision.candidates.windows(2) {
        assert!(
            (pair[0].access_generation, pair[0].editor_id)
                < (pair[1].access_generation, pair[1].editor_id)
        );
    }
    for page in snapshot.pages() {
        if !eligible(page) || selected(&decision, page.editor_id) {
            continue;
        }
        for candidate in &decision.candidates {
            assert!(
                (candidate.access_generation, candidate.editor_id)
                    < (page.access_generation, page.editor_id)
            );
        }
    }
}

/// Selection stops at the lower watermark: before the last candidate was
/// applied, the projected total was still above it (hysteresis, no more than
/// needed).
#[kani::proof]
#[kani::unwind(4)]
fn budget_stops_at_the_lower_watermark() {
    let snapshot = Snapshot::any();
    let decision = evaluate_editor_memory_budget(snapshot.pages());
    if let Some(last) = decision.candidates.last() {
        assert!(
            u128::from(decision.projected_bytes) + u128::from(last.reclaimable_bytes)
                > u128::from(EDITOR_MEMORY_LOWER_WATER_BYTES)
        );
    }
}

/// The totals are saturating sums, the projected total is the aggregate minus
/// the reclaimable sum (saturating), `Converged` holds exactly when that is at
/// or below the watermark, and `NoProgress` means every eligible page was
/// selected.
#[kani::proof]
#[kani::unwind(4)]
fn budget_outcome_matches_the_projected_total() {
    let snapshot = Snapshot::any();
    let decision = evaluate_editor_memory_budget(snapshot.pages());
    assert!(decision.total_bytes == saturating_total(snapshot.pages(), false));
    assert!(decision.protected_bytes == saturating_total(snapshot.pages(), true));
    let reclaimed: u128 = decision
        .candidates
        .iter()
        .map(|candidate| u128::from(candidate.reclaimable_bytes))
        .sum();
    assert!(
        u128::from(decision.projected_bytes)
            == u128::from(decision.total_bytes).saturating_sub(reclaimed)
    );
    for candidate in &decision.candidates {
        let page = snapshot
            .pages()
            .iter()
            .find(|page| page.editor_id == candidate.editor_id)
            .expect("a candidate names a supplied page");
        assert!(
            candidate.reclaimable_bytes == page.estimated_bytes - EVICTED_EDITOR_BOOKKEEPING_BYTES
        );
        assert!(candidate.access_generation == page.access_generation);
        assert!(candidate.policy_generation == page.policy_generation);
    }
    match decision.outcome {
        EditorMemoryBudgetOutcome::WithinBudget => {
            kani::cover!(true, "WithinBudget");
            assert!(decision.total_bytes <= EDITOR_MEMORY_UPPER_BUDGET_BYTES);
        }
        EditorMemoryBudgetOutcome::Converged => {
            kani::cover!(true, "Converged");
            assert!(decision.total_bytes > EDITOR_MEMORY_UPPER_BUDGET_BYTES);
            assert!(decision.projected_bytes <= EDITOR_MEMORY_LOWER_WATER_BYTES);
        }
        EditorMemoryBudgetOutcome::NoProgress => {
            kani::cover!(true, "NoProgress");
            assert!(decision.total_bytes > EDITOR_MEMORY_UPPER_BUDGET_BYTES);
            assert!(decision.projected_bytes > EDITOR_MEMORY_LOWER_WATER_BYTES);
            for page in snapshot.pages() {
                if eligible(page) {
                    assert!(selected(&decision, page.editor_id));
                }
            }
        }
    }
}

/// An aggregate at or below the upper budget selects nothing and reports
/// `WithinBudget`; above it, the outcome is never `WithinBudget`.
#[kani::proof]
#[kani::unwind(4)]
fn within_budget_selects_nothing() {
    let snapshot = Snapshot::any();
    let decision = evaluate_editor_memory_budget(snapshot.pages());
    if decision.total_bytes <= EDITOR_MEMORY_UPPER_BUDGET_BYTES {
        assert!(decision.candidates.is_empty());
        assert!(decision.outcome == EditorMemoryBudgetOutcome::WithinBudget);
        assert!(decision.projected_bytes == decision.total_bytes);
    } else {
        assert!(decision.outcome != EditorMemoryBudgetOutcome::WithinBudget);
    }
}

/// The ledger's record set as the harnesses model it: one slot per identity,
/// displacing records exactly as `BTreeMap::insert` and `BTreeMap::remove` do.
/// The map itself is trusted (see [`EditorResidencyLedger`]); what is checked
/// is the production [`ResidencyTotals`] accounting the ledger drives with the
/// displaced record.
struct ModelLedger {
    slots: [Option<EditorResidency>; 2],
    totals: ResidencyTotals,
}

impl ModelLedger {
    fn new() -> Self {
        Self {
            slots: [None, None],
            totals: ResidencyTotals::default(),
        }
    }

    /// `EditorResidencyLedger::upsert`, over the slots.
    fn upsert(&mut self, record: EditorResidency) -> EditorResidencyUpdate {
        let previous = self.slots[record.editor_id].replace(record);
        self.totals.replace(previous.as_ref(), Some(&record))
    }

    /// `EditorResidencyLedger::remove`, over the slots.
    fn remove(&mut self, editor_id: usize) -> Option<EditorResidencyUpdate> {
        let previous = self.slots[editor_id].take()?;
        Some(self.totals.replace(Some(&previous), None))
    }

    /// `EditorResidencyLedger::snapshot`, over the slots.
    fn records(&self) -> ([EditorResidency; 2], usize) {
        let mut records = [any_page(0); 2];
        let mut len = 0;
        for record in self.slots.iter().flatten() {
            records[len] = *record;
            len += 1;
        }
        (records, len)
    }

    /// One arbitrary upsert or removal on identity 0 or 1; returns whether it
    /// was an upsert, and the update the ledger would return.
    fn any_operation(&mut self) -> (bool, Option<EditorResidencyUpdate>) {
        let editor_id = usize::from(kani::any::<bool>());
        if kani::any() {
            (true, Some(self.upsert(any_page(editor_id))))
        } else {
            (false, self.remove(editor_id))
        }
    }
}

/// After every step of any sequence of [`LEDGER_OPERATIONS`] upserts and
/// removes, the ledger's saturating totals equal a recomputation over its
/// records, each update reports the totals before and after it, and the totals
/// equal those `evaluate_editor_memory_budget` computes from the same records:
/// the incremental accounting agrees with the full scan.
#[kani::proof]
#[kani::unwind(4)]
fn ledger_totals_match_a_recomputation() {
    let mut ledger = ModelLedger::new();
    for _ in 0..LEDGER_OPERATIONS {
        let previous = ledger.totals.total_bytes();
        let (_, update) = ledger.any_operation();
        let (records, len) = ledger.records();
        let records = &records[..len];
        assert!(ledger.totals.total_bytes() == saturating_total(records, false));
        assert!(ledger.totals.protected_bytes() == saturating_total(records, true));
        match update {
            Some(update) => {
                assert!(update.previous_total_bytes == previous);
                assert!(update.total_bytes == ledger.totals.total_bytes());
            }
            None => assert!(ledger.totals.total_bytes() == previous),
        }
        let decision = evaluate_editor_memory_budget(records);
        assert!(decision.total_bytes == ledger.totals.total_bytes());
        assert!(decision.protected_bytes == ledger.totals.protected_bytes());
    }
}

/// `crossed_upper_threshold` is true exactly when an upsert moves the
/// aggregate from at or below the upper budget to above it, and a removal
/// never reports a crossing.
#[kani::proof]
#[kani::unwind(4)]
fn ledger_crossing_flag_is_exact() {
    let mut ledger = ModelLedger::new();
    for _ in 0..LEDGER_OPERATIONS {
        let (upserted, update) = ledger.any_operation();
        if let Some(update) = update {
            let crossed = update.previous_total_bytes <= EDITOR_MEMORY_UPPER_BUDGET_BYTES
                && update.total_bytes > EDITOR_MEMORY_UPPER_BUDGET_BYTES;
            kani::cover!(upserted && crossed, "an upsert crosses the upper budget");
            assert!(update.crossed_upper_threshold == (upserted && crossed));
        }
    }
}

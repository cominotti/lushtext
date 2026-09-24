## ADDED Requirements

### Requirement: Memory-budget decisions are machine-checked on a stated domain
The project SHALL keep Kani harnesses over the production functions
`estimate_live_editor_bytes`, `evaluate_editor_memory_budget`, and
`EditorResidencyLedger`. Each harness SHALL state its input domain. The domain
SHALL cover every `u64` estimate, access generation, and policy generation, so
that saturation is exercised, and snapshots of up to three pages with distinct
editor identities. Ledger sequences SHALL be bounded, with the bound stated.
The ledger's record map is standard-library code and MAY be trusted: the
harnesses then drive the production accounting the ledger applies to each
displaced record through a fixed-size record set that displaces records as
the map does, and the harness states that trust. The harnesses SHALL prove:

- an evicted editor's estimate is the fixed bookkeeping figure, and a loaded
  editor's estimate is at least its known file size and never wraps;
- an aggregate at or below the upper budget selects no candidate and reports
  `WithinBudget`;
- no candidate is protected, and no candidate's estimate is at or below the
  bookkeeping figure;
- candidates come in ascending (access generation, editor identity) order, and
  no eligible page outside the selection is less recently used than a selected
  one;
- selection stops at the lower watermark: before the last candidate was
  applied, the projected total was above the watermark;
- the projected total equals the aggregate minus the reclaimable sum,
  saturating;
- `Converged` holds exactly when the projected total is at or below the
  watermark;
- `NoProgress` implies that every eligible page was selected;
- the ledger's saturating totals equal a recomputation over its records after
  every bounded sequence of upserts and removes;
- `crossed_upper_threshold` is true exactly when an upsert moves the aggregate
  from at or below the upper budget to above it.

#### Scenario: Protected pages are never selected
- **WHEN** the harness explores every snapshot within its domain whose aggregate exceeds the upper budget
- **THEN** no selected candidate is a protected page, in any explored snapshot

#### Scenario: Hysteresis stops at the watermark
- **WHEN** a snapshot's eligible pages can bring the aggregate to the lower watermark
- **THEN** the outcome is `Converged` and removing the last selected candidate would leave the projected total above the watermark

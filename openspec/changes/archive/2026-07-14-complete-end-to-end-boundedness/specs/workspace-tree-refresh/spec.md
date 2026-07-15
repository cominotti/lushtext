## ADDED Requirements

### Requirement: Large tree reconciliation applies in bounded GTK batches
An accepted workspace-directory refresh whose reconciliation exceeds the calibrated synchronous threshold SHALL apply model changes through generation-guarded GTK batches. Reconciliation planning MUST use plain row state outside repeated GObject scans where practical, and expansion, selection, row caches, watcher targets, and readiness MUST finalize only for the complete current plan. A stale or replaced plan MUST stop without announcing refresh completion.

#### Scenario: Broad expanded directory changes near the start
- **WHEN** refresh changes a large prefix or middle range of an expanded directory containing thousands of visible rows
- **THEN** GTK constructs and splices only a bounded row batch per main-loop turn
- **AND** input, drawing, and manual Refresh remain schedulable between batches

#### Scenario: Refresh is superseded between batches
- **WHEN** a newer scan generation or section lifetime replaces an active reconciliation plan
- **THEN** remaining batches from the stale plan stop
- **AND** stale cache, expansion, selection, watcher-target, and readiness finalization do not overwrite the newer plan

#### Scenario: Small reconciliation remains direct
- **WHEN** the changed range is below the calibrated synchronous threshold
- **THEN** the section MAY reconcile it in one GTK callback
- **AND** it observes the same generation and terminal-finalization contract

#### Scenario: Batched reconciliation completes
- **WHEN** the final accepted batch has been applied
- **THEN** row caches and surviving expansion and selection state are reconciled once against the completed model
- **AND** workspace refresh readiness becomes complete exactly once

## MODIFIED Requirements

### Requirement: Memory-budget eviction never risks user work
The system MUST NOT evict the active editor or an editor that is modified, untitled, loading, saving, failed to load safely, or otherwise unable to reload its current state from a backing file. Eligibility MUST be revalidated immediately before eviction and during any multi-turn clear. An editor SHALL be considered evicted and its released residency counted only after its current buffer has been completely cleared through the bounded GTK replacement contract.

#### Scenario: Modified editor remains resident
- **WHEN** aggregate estimated residency exceeds the budget and an inactive editor has unsaved modifications
- **THEN** that editor is excluded from eviction
- **AND** its unsaved content remains resident

#### Scenario: Save begins after candidate selection
- **WHEN** an editor was selected as an eviction candidate but begins saving before eviction is applied
- **THEN** the eligibility recheck rejects that stale candidate
- **AND** the save continues without eviction interference

#### Scenario: Active tab changes before eviction
- **WHEN** a selected eviction candidate becomes the active editor before the policy applies it
- **THEN** the stale eviction decision is ignored
- **AND** the newly active editor remains loaded

#### Scenario: Large eviction clears across main-loop turns
- **WHEN** an accepted clean candidate exceeds the synchronous clear threshold
- **THEN** its content is removed in bounded GTK slices while it remains unavailable for conflicting work
- **AND** memory accounting records eviction only after the complete current clear reaches its terminal outcome

#### Scenario: Eviction is invalidated during clearing
- **WHEN** editor lifetime or eligibility changes between clear slices
- **THEN** the session stops without claiming a completed eviction
- **AND** policy schedules a fresh current-state evaluation instead of using stale released-memory estimates

# Live Editor Memory Budget Specification

## Purpose

Define conservative live editor memory accounting and safe, deterministic eviction behavior that bounds reloadable editor residency without risking user work.

## Requirements

### Requirement: Loaded editors report a conservative live memory estimate
The system SHALL estimate each loaded editor's live buffer residency from its current buffer length without copying the buffer text. The estimate MUST account for untitled content and growth beyond the last known on-disk file size, MUST use saturating arithmetic, and MUST update after accepted load, edit, save, restore, eviction, and reload transitions.

#### Scenario: Untitled content contributes to the budget
- **WHEN** an untitled loaded editor contains text
- **THEN** its current buffer length contributes a non-zero conservative estimate to aggregate editor residency
- **AND** the estimate does not depend on a backing file size

#### Scenario: Edited file grows beyond its loaded size
- **WHEN** a file-backed editor grows substantially after its original file size was recorded
- **THEN** the live estimate increases to reflect the current buffer
- **AND** aggregate accounting does not remain fixed at the stale on-disk size

#### Scenario: Counting does not copy a large buffer
- **WHEN** the system refreshes accounting for a large loaded editor
- **THEN** it derives the estimate from bounded scalar GTK state
- **AND** it does not materialize the editor text solely to compute memory residency

### Requirement: Aggregate memory enforcement reacts to live state
The system SHALL coalesce editor residency and eligibility changes into aggregate budget evaluation on the GTK main loop. When estimated loaded-editor residency exceeds `256 * 1024 * 1024` bytes, the system MUST select safe inactive editors for eviction until the estimate reaches the configured lower watermark or no eligible editor remains.

#### Scenario: Unsaved growth crosses the budget
- **WHEN** editing causes aggregate estimated residency to cross the budget
- **THEN** the window schedules a coalesced budget evaluation without waiting for tab selection or another file load
- **AND** eligible inactive clean editors are considered for eviction

#### Scenario: Burst edits coalesce policy work
- **WHEN** many text-length notifications occur before the pending evaluation runs
- **THEN** the window performs one current aggregate evaluation for that burst
- **AND** it does not scan all tabs once per keystroke

#### Scenario: Eviction reaches a lower watermark
- **WHEN** enough eligible inactive editors exist to reduce residency below the budget
- **THEN** the policy evicts least-recently-used eligible editors until the lower watermark is reached
- **AND** ordinary estimate noise near the upper threshold does not cause immediate repeated eviction

### Requirement: Memory-budget eviction never risks user work
The system MUST NOT evict the active editor or an editor that is modified, untitled, loading, saving, failed to load safely, or otherwise unable to reload its current state from a backing file. Eligibility MUST be revalidated immediately before eviction.

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

### Requirement: Protected over-budget state is soft and stable
The system SHALL allow estimated residency to remain above the budget when protected editors alone exceed it. In that state the system MUST preserve all protected content, MUST avoid repeated no-progress eviction loops, and MUST re-evaluate when residency or eligibility later changes.

#### Scenario: Protected documents exceed the budget
- **WHEN** active, modified, untitled, loading, or saving editors together exceed the memory budget
- **THEN** the system keeps those editors resident
- **AND** it records a no-eligible-candidate policy outcome instead of discarding content

#### Scenario: Protected editor later becomes evictable
- **WHEN** a protected over-budget editor is saved, becomes inactive, and can be reloaded safely
- **THEN** the later eligibility transition schedules a fresh evaluation
- **AND** the editor may then participate in least-recently-used eviction

### Requirement: Live memory policy has deterministic scale coverage
The project SHALL cover live editor memory accounting and eviction through pure policy tests, editor/window integration tests, and scale-oriented fixtures. Coverage MUST include zero tabs, one tab, many tabs, awkward Unicode text, untitled buffers, large unsaved growth, delayed session restore, stale callbacks, and protected over-budget states.

#### Scenario: Delayed session restore remains safe
- **WHEN** many session tabs finish loading in a different order from the order they were requested
- **THEN** each accepted load updates current aggregate accounting
- **AND** stale completions cannot evict the active or modified page

#### Scenario: Many clean tabs converge below the watermark
- **WHEN** a scale fixture loads enough clean reloadable tabs to exceed the budget
- **THEN** the policy deterministically selects least-recently-used candidates
- **AND** the retained estimated residency reaches the lower watermark when sufficient candidates exist

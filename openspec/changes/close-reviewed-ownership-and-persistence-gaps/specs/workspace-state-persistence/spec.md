## ADDED Requirements

### Requirement: Workspace persistence failures remain pending, visible, and close-safe
Workspace state persistence SHALL track requested, in-flight, durably accepted, and failed generations explicitly. Starting a background write MUST NOT mark its generation clean before success. A failed current generation MUST remain retryable, MUST expose user-visible failure feedback, and MUST continue blocking workspace-persistence readiness until a later durable success supersedes it. Window close MUST flush the newest requested workspace snapshot after any in-flight write and MUST abort close without destroying the window when that terminal snapshot cannot be saved.

#### Scenario: Background workspace write fails without a newer mutation
- **WHEN** the current debounced workspace save returns an I/O error
- **THEN** the failed generation remains pending or failed rather than appearing clean
- **AND** the user sees retryable failure feedback
- **AND** automation readiness continues to report workspace persistence as unsettled

#### Scenario: Newer mutation arrives during an older write
- **WHEN** workspace state changes while an earlier snapshot is in flight
- **THEN** success of the older generation does not mark the newer generation durable
- **AND** the newest snapshot is scheduled next without an older completion overwriting its membership, order, rename, or scope

#### Scenario: Failure is followed by recovery
- **WHEN** retry, a later mutation, or close flush successfully saves the newest requested generation after a failure
- **THEN** failed state and visible failure feedback resolve for that generation
- **AND** readiness becomes settled only after no newer dirty or in-flight generation remains

#### Scenario: Window closes before debounce fires
- **WHEN** workspace state is dirty and the user closes the window before the debounce deadline
- **THEN** close safety bypasses the debounce and asynchronously persists the newest snapshot
- **AND** the window is destroyed only after that generation is durably accepted together with the existing draft and session close guarantees

#### Scenario: Window closes while workspace write is in flight
- **WHEN** close safety starts during an older workspace save and a newer snapshot exists
- **THEN** close waits for the in-flight terminal and then flushes the newest generation if needed
- **AND** it neither launches conflicting writes nor accepts the older generation as terminal for close

#### Scenario: Close-time workspace save fails
- **WHEN** the newest workspace snapshot cannot be saved during close safety
- **THEN** close is cancelled, the window becomes usable again, and workspace state remains retryable
- **AND** no final session save or window destruction claims the close transaction completed

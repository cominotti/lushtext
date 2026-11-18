## ADDED Requirements

### Requirement: A failed set-aside copy of an unrestored body keeps its hold
When a recovery body is disposed of without being restored and its
preservation copy fails, the system SHALL keep that draft id's autosave hold
until a copy exists. Autosave then never replaces the only copy of a body the
user has not seen, and no retry schedule MUST release the hold. A successful
retry SHALL release the hold and publish the preservation outcome exactly
once. The first failure SHALL be visible to the user, and later failures SHALL
be logged at debug level only.

#### Scenario: Recovery after backoff releases the hold once
- **WHEN** a retry after one or more backed-off failures succeeds in creating the copy
- **THEN** the hold on that draft id is released and the preservation outcome is published once
- **AND** the retry entry is removed from the workflow evidence

### Requirement: Unrestored-copy retries back off on the periodic autosave timer
Retries of a failed unrestored-body copy SHALL run only from the periodic
autosave timer. An autosave pass started by the first-dirty debounce or by a
pending rerun SHALL NOT retry. The retries SHALL be spaced by an
exponentially growing number of timer ticks, capped at no more than one
minute of ticks. The workflow evidence SHALL expose the pending retry count,
each retry's next due tick, and the timer's tick count.

#### Scenario: A persistent copy failure does not retry every tick
- **WHEN** the set-aside copy of an unrestored body fails on consecutive attempts
- **THEN** successive retries are spaced by a doubling number of autosave timer ticks up to the cap
- **AND** the draft id stays held throughout, so no autosave replaces its body

#### Scenario: Autosave passes outside the timer do not retry
- **WHEN** an autosave pass runs from the first-dirty debounce or a pending rerun while a copy retry is waiting
- **THEN** no copy is attempted, and the retry's failure count and next due tick are unchanged

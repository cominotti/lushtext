## ADDED Requirements

### Requirement: A failed set-aside copy of an unrestored body keeps its hold and retries with bounded backoff
When a recovery body is disposed without being restored and its preservation
copy fails, the system SHALL keep that draft id's autosave hold until a copy
exists, so autosave never replaces the only copy of a body the user has not
seen. Retries SHALL run from the autosave tick with an exponentially growing
delay, measured in ticks and capped at a bounded maximum, rather than on every
tick. The first failure SHALL be visible to the user; later failures SHALL be
logged at debug level only. The backoff and its cap MUST NOT release the
hold. A successful retry SHALL release the hold and publish the preservation
outcome exactly once. The workflow evidence SHALL expose the pending retry
count and each retry's next due tick.

#### Scenario: A persistent copy failure does not retry every tick
- **WHEN** the set-aside copy of an unrestored body fails on consecutive attempts
- **THEN** successive retries are spaced by a doubling number of autosave ticks up to the cap
- **AND** the draft id stays held throughout, so no autosave replaces its body

#### Scenario: Recovery after backoff releases the hold once
- **WHEN** a retry after one or more backed-off failures succeeds in creating the copy
- **THEN** the hold on that draft id is released and the preservation outcome is published once
- **AND** the retry entry is removed from the workflow evidence

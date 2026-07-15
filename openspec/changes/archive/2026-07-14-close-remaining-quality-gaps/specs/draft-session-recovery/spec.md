## MODIFIED Requirements

### Requirement: Orphan cleanup remains bounded and non-blocking
The system SHALL keep orphan inspection and mutation off the GTK main thread and SHALL inspect no more than the configured bounded entry count in one pass. Every trusted outcome whose typed `has_more_work` value is true MUST schedule exactly one coalesced deferred follow-up without looping synchronously. A continuation cursor SHALL resume bounded pagination; retryable work without a cursor SHALL restart from the safe beginning with bounded backoff. Untrusted or ambiguous recovery state MUST remain preserved.

#### Scenario: Damaged directory exceeds the scan bound
- **WHEN** a drafts directory contains more candidates than one cleanup pass permits
- **THEN** the pass inspects only the configured maximum
- **AND** the outcome records that more work may remain and schedules one deferred continuation
- **AND** startup and the GTK main loop remain usable

#### Scenario: Retryable final-page failure has no cursor
- **WHEN** deletion, status inspection, or manifest persistence fails after a pass reaches its final directory and manifest page
- **THEN** the outcome retains the failed artifact and reports `has_more_work`
- **AND** the window schedules one backoff-controlled retry from the safe beginning even though no continuation cursor exists

#### Scenario: Repeated failures do not multiply cleanup work
- **WHEN** several deferred cleanup passes encounter the same persistent retryable failure
- **THEN** at most one follow-up timer and one cleanup worker remain owned by the window
- **AND** retry delay grows to a bounded cap while diagnostics and retained artifacts remain bounded

#### Scenario: Cleanup reaches a terminal complete outcome
- **WHEN** a trusted pass reports no directory continuation, no manifest continuation, and `has_more_work` is false
- **THEN** no further cleanup timer is scheduled
- **AND** prior retry backoff state is reset

#### Scenario: Untrusted startup state skips cleanup
- **WHEN** startup recovery cannot trust the draft manifest
- **THEN** orphan cleanup is not executed or retried automatically
- **AND** ambiguous draft bodies and metadata remain preserved for repair or diagnosis

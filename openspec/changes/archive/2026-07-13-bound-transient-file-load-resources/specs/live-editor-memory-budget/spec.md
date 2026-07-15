## ADDED Requirements

### Requirement: File loads use byte-weighted transient admission
The system SHALL govern raw bytes, decoded text, queued load results, and in-progress GTK installation through a process-wide transient load budget. Admission MUST occur before document-sized payload work enters the background-worker queue, and ownership MUST remain charged until the payload is consumed or discarded.

#### Scenario: Several ordinary loads fit the transient budget
- **WHEN** multiple current file-load plans have a combined conservative weight within the transient budget
- **THEN** they may execute concurrently subject to the existing worker cap
- **AND** each retains its admission ownership through result consumption

#### Scenario: Concurrent loads exceed the transient budget
- **WHEN** admitting another current load would exceed the transient budget
- **THEN** that load remains queued as compact scalar state without reading or decoding its body
- **AND** it is reconsidered when earlier payload ownership is released

#### Scenario: One supported file exceeds the shared budget
- **WHEN** an individually supported file has a conservative transient weight above the ordinary shared budget
- **THEN** it may run only as the exclusive admitted load
- **AND** no second document-sized load payload overlaps it

#### Scenario: Queued load becomes stale
- **WHEN** a tab closes, reloads, or advances load generation before admission
- **THEN** its compact queued request is removed or skipped
- **AND** it consumes neither worker capacity nor transient payload budget

### Requirement: Transient admission never weakens user-work protection
The transient load policy MUST NOT evict, discard, or overwrite active, modified, saving, failed-load, or otherwise non-recoverable editor state. If protected work keeps memory above a preferred threshold, new loads SHALL wait or fail visibly rather than reclaiming user content.

#### Scenario: Protected editors already exceed the preferred budget
- **WHEN** protected live editors and one admitted payload leave no capacity for another load
- **THEN** the newer load remains queued or reports a bounded visible refusal
- **AND** protected editor contents remain unchanged

#### Scenario: Active tab changes while a load is queued
- **WHEN** a queued page becomes active before admission
- **THEN** current priority and freshness are reevaluated
- **AND** the policy does not discard another protected page to force admission

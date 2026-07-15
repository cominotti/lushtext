## ADDED Requirements

### Requirement: Save payload admission precedes complete buffer capture
The system SHALL acquire conservative byte-weighted admission before capturing a complete document snapshot for an asynchronous save. A queued save MUST retain only compact scalar and weak identity state, and its payload ownership MUST remain charged until snapshot, transformation, encoding, and durable-write inputs are consumed or discarded.

#### Scenario: Several saves would exceed the payload budget
- **WHEN** admitting another save's conservative snapshot and encoding charge would exceed the process save-payload budget
- **THEN** that save remains queued without capturing complete document text
- **AND** it is reconsidered after earlier payload ownership is released

#### Scenario: One supported save exceeds the shared budget
- **WHEN** one supported document has a conservative save charge larger than the ordinary shared budget
- **THEN** it runs only as the exclusive admitted save payload
- **AND** no second document-sized save payload overlaps it

#### Scenario: Queued save becomes stale
- **WHEN** an editor closes, changes save generation, changes destination identity, or no longer needs the queued save before admission
- **THEN** the compact request is skipped or removed
- **AND** it consumes neither document payload budget nor worker capacity

### Requirement: Multi-document close saves are ordered and recovery-safe
When a close decision saves multiple modified documents, the system SHALL complete those saves sequentially and SHALL NOT capture the next complete document body while the preceding close save still owns its payload. The close flow MUST preserve existing dirty-state, durability-warning, Save As, draft-recovery, and explicit-discard semantics.

#### Scenario: Window close saves several modified files
- **WHEN** the user chooses to save several modified file-backed tabs during window close
- **THEN** the flow admits, snapshots, writes, and releases one selected document before admitting the next
- **AND** the window closes only after every selected save succeeds

#### Scenario: A close save fails before replacement
- **WHEN** one selected close save fails before replacing its destination
- **THEN** the document remains modified and recoverable
- **AND** the window and remaining selected tabs do not close as though all saves succeeded

#### Scenario: A close save has unconfirmed durability
- **WHEN** one selected close save reaches disk but its durability cannot be confirmed
- **THEN** the document remains modified and its draft recovery is retained
- **AND** later close saves and final window closure do not proceed as though the warning were a successful terminal save

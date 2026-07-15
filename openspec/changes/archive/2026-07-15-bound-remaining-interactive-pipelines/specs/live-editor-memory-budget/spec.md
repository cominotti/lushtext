## ADDED Requirements

### Requirement: Routine editor residency accounting is incremental
The system SHALL maintain per-editor scalar residency records and a saturating aggregate so an accepted ordinary edit updates accounting without walking or allocating a snapshot of every open tab. A full candidate snapshot SHALL be built only for threshold enforcement, an active enforcement session, lifecycle transitions that require it, or detected accounting uncertainty.

#### Scenario: Many tabs remain below the upper threshold
- **WHEN** the active editor changes length while aggregate estimated residency remains below the upper threshold
- **THEN** the system updates that editor's record and aggregate in constant work relative to tab count
- **AND** it does not allocate or sort a full-tab eviction snapshot

#### Scenario: Incremental update crosses the threshold
- **WHEN** a scalar residency delta moves aggregate loaded-editor residency above the upper threshold
- **THEN** the window schedules one current eviction evaluation
- **AND** that evaluation builds and freshness-checks the full candidate snapshot needed for policy enforcement

#### Scenario: Accounting becomes uncertain
- **WHEN** attach, detach, destruction, stale completion, or an exceptional lifecycle path prevents a trusted delta update
- **THEN** the aggregate is marked for reconciliation
- **AND** enforcement uses a current full scan before claiming released residency

### Requirement: Save payloads share process-wide transient pressure bounds
The system SHALL account admitted save snapshots and encoding/write payloads alongside other process-wide document-sized transient work. Save admission MUST preserve user-work priority and MUST NOT reclaim active, modified, saving, or otherwise protected editor content to make room.

#### Scenario: Live editors and a load leave insufficient save capacity
- **WHEN** protected residency and already admitted transient work leave insufficient capacity for another save payload
- **THEN** the save waits as a compact priority request or reports a bounded visible failure
- **AND** protected editor content remains unchanged

#### Scenario: Close save waits for transient capacity
- **WHEN** a close-triggered save cannot yet acquire its conservative payload charge
- **THEN** it receives priority over later ordinary transient requests once current ownership is released
- **AND** the close flow remains pending rather than snapshotting outside the budget

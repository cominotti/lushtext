## ADDED Requirements

### Requirement: Lineage migration falls back to copying only across filesystems
The system SHALL move local-history snapshot bodies with a durable rename and
SHALL fall back to a durable copy only when the rename fails because source and
target are on different filesystems. Every other rename failure SHALL fail the
migration retryably, including a rename that took effect but whose
parent-directory sync failed, and SHALL leave the pending migration record for
startup reconciliation.

#### Scenario: Cross-device rename uses the copy fallback
- **WHEN** a snapshot move's rename fails with a cross-device error
- **THEN** the snapshot is copied durably to the target lineage and the migration continues

#### Scenario: A parent-sync failure after rename does not trigger a copy
- **WHEN** a snapshot move's rename takes effect but the parent-directory sync fails
- **THEN** no copy of the already-moved source is attempted
- **AND** the migration fails retryably and a later retry completes from the source-absent, target-present state

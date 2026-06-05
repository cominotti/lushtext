## ADDED Requirements

### Requirement: Bookmark sidecar corruption is isolated and diagnostic
The system SHALL isolate malformed bookmark sidecars from valid bookmark state. A malformed bookmark sidecar MUST be preserved when possible, reported through recovery diagnostics, and excluded from normal bookmark restoration until repaired or replaced.

#### Scenario: Malformed bookmark sidecar does not clear active bookmarks elsewhere
- **WHEN** one bookmark sidecar cannot be parsed during workspace bookmark listing
- **THEN** valid bookmark sidecars continue to load and appear in browse surfaces
- **AND** the malformed sidecar is reported as a recovery diagnostic

#### Scenario: Reopening file with corrupt bookmark sidecar warns without modifying source
- **WHEN** a saved file is opened and its bookmark sidecar is malformed
- **THEN** the editor opens the source file without bookmark indicators from that sidecar
- **AND** the source file bytes remain unchanged
- **AND** a diagnostic is logged or surfaced

#### Scenario: Corrupt bookmark sidecar is preserved before replacement
- **WHEN** the user later creates a new bookmark for a file whose prior bookmark sidecar was malformed
- **THEN** the malformed sidecar is quarantined or otherwise preserved before a replacement sidecar is written

### Requirement: Bookmark migrations are retryable after in-app renames
The system SHALL record pending bookmark sidecar migrations before or as part of the post-rename sidecar migration workflow. If migration or cleanup fails, the pending state MUST survive restart and be retried during startup reconciliation.

#### Scenario: Pending bookmark migration survives restart
- **WHEN** an in-app rename succeeds but bookmark sidecar migration fails before completion
- **THEN** a pending migration record remains in app data
- **AND** restarting LushText retries the bookmark migration

#### Scenario: Completed bookmark migration clears pending state
- **WHEN** bookmark sidecar migration succeeds and obsolete sidecars are cleaned up or safely reconciled
- **THEN** the pending bookmark migration record is removed durably

#### Scenario: Repeated migration failure is bounded
- **WHEN** the same bookmark migration fails repeatedly
- **THEN** the system reports the persistent failure without blocking startup
- **AND** it does not retry in an unbounded tight loop

### Requirement: Bookmark reconciliation never deletes the only non-empty bookmark copy
The system SHALL reconcile duplicate old and new bookmark sidecars conservatively. It MUST NOT delete the only non-empty bookmark sidecar unless a merged or migrated replacement has already been durably written.

#### Scenario: Duplicate bookmark sidecars merge deterministically
- **WHEN** old and new bookmark sidecars both exist after a rename retry
- **THEN** the system merges bookmark records using stable bookmark identities and ordering
- **AND** it writes the merged target before removing any obsolete sidecar

#### Scenario: Obsolete cleanup failure remains diagnostic
- **WHEN** a migrated bookmark sidecar is written but the obsolete sidecar cannot be removed
- **THEN** the system reports a cleanup diagnostic
- **AND** later startup reconciliation attempts cleanup again

#### Scenario: Ambiguous duplicate bookmarks are preserved
- **WHEN** duplicate bookmark sidecars cannot be merged deterministically
- **THEN** the system preserves both copies or quarantines the ambiguous copy
- **AND** it reports that automatic bookmark reconciliation was incomplete

### Requirement: Bookmark reliability has layered automated coverage
The project SHALL add deterministic service, integration, and widget coverage for bookmark sidecar corruption, retryable migrations, duplicate reconciliation, and partial browser behavior.

#### Scenario: Service tests cover corrupt bookmark sidecars
- **WHEN** service tests load malformed bookmark sidecar bytes
- **THEN** the result preserves or quarantines the sidecar and returns recovery diagnostics
- **AND** no valid bookmark sidecar is dropped because of the malformed one

#### Scenario: Migration tests cover retry ledger behavior
- **WHEN** tests simulate an in-app rename whose bookmark migration fails after the source rename
- **THEN** a pending migration record survives restart
- **AND** a later successful retry removes the record durably

#### Scenario: Widget tests cover partial bookmark browsing
- **WHEN** one bookmark sidecar is corrupt and another is valid
- **THEN** the notes or bookmark browser remains usable
- **AND** the user receives visible partial-recovery feedback

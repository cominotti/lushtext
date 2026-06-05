## ADDED Requirements

### Requirement: Document-note sidecar corruption is isolated and diagnostic
The system SHALL isolate malformed document-note sidecars from valid document-note state. A malformed document-note sidecar MUST be preserved when possible, reported through recovery diagnostics, and excluded from normal note restoration until repaired or replaced.

#### Scenario: Malformed document note does not block unrelated notes
- **WHEN** one document-note sidecar cannot be parsed during notes browser listing
- **THEN** valid document notes continue to load and appear in the notes browser
- **AND** the malformed sidecar is reported as a recovery diagnostic

#### Scenario: Opening a file with corrupt document note keeps file usable
- **WHEN** a saved file is opened and its document-note sidecar is malformed
- **THEN** the file opens normally
- **AND** the document-note workflow reports that the saved note could not be loaded

#### Scenario: Replacement preserves corrupt note evidence
- **WHEN** the user saves a new document note for an identity whose previous note sidecar was malformed
- **THEN** the malformed sidecar is quarantined or otherwise preserved before replacement

### Requirement: Document-note migrations are retryable after in-app renames
The system SHALL record pending document-note sidecar migrations before or as part of the post-rename sidecar migration workflow. If migration or cleanup fails, the pending state MUST survive restart and be retried during startup reconciliation.

#### Scenario: Pending document-note migration survives restart
- **WHEN** an in-app rename succeeds but document-note migration fails before completion
- **THEN** a pending migration record remains in app data
- **AND** restarting LushText retries the document-note migration

#### Scenario: Completed document-note migration clears pending state
- **WHEN** document-note migration succeeds and obsolete sidecars are cleaned up or safely reconciled
- **THEN** the pending document-note migration record is removed durably

#### Scenario: Migration failure warns without losing note text
- **WHEN** document-note migration fails after the source file rename succeeded
- **THEN** the user receives warning feedback
- **AND** the existing note sidecar remains preserved for retry or inspection

### Requirement: Document-note reconciliation preserves the newest durable note body
The system SHALL reconcile duplicate old and new document-note sidecars conservatively. It MUST preserve the newest durable note body when timestamps or deterministic identity evidence make that choice safe, and MUST preserve evidence instead of guessing when the conflict is ambiguous.

#### Scenario: Duplicate document notes choose deterministic newest body
- **WHEN** old and new document-note sidecars both exist and one can be identified as the newer durable save
- **THEN** the newer note body is kept for the migrated identity
- **AND** the older copy is removed only after the target note is durably written

#### Scenario: Ambiguous document-note conflict is preserved
- **WHEN** duplicate document notes conflict and the newest body cannot be determined safely
- **THEN** the system does not discard either note body silently
- **AND** it reports that automatic document-note reconciliation was incomplete

#### Scenario: Notes browser reports partial note recovery
- **WHEN** the notes browser omits or quarantines a malformed document note
- **THEN** it still displays valid notes
- **AND** it exposes a warning that some note data could not be loaded

### Requirement: Document-note reliability has layered automated coverage
The project SHALL add deterministic service, integration, and widget coverage for document-note sidecar corruption, retryable migrations, duplicate reconciliation, and partial notes-browser behavior.

#### Scenario: Service tests cover corrupt document-note sidecars
- **WHEN** service tests load malformed document-note sidecar bytes
- **THEN** the result preserves or quarantines the sidecar and returns recovery diagnostics
- **AND** unrelated valid document notes still load

#### Scenario: Migration tests cover document-note retry state
- **WHEN** tests simulate a document rename whose document-note migration fails after the source rename
- **THEN** a pending migration record survives restart
- **AND** a later successful retry removes the record durably

#### Scenario: Widget tests cover partial notes browsing
- **WHEN** the notes browser sees one corrupt document note and at least one valid note
- **THEN** the valid notes remain browsable
- **AND** visible partial-recovery feedback is shown

## ADDED Requirements

### Requirement: Workspace-note sidecar corruption is isolated and diagnostic
The system SHALL isolate malformed workspace-note sidecars from valid workspace-note state. A malformed workspace-note sidecar MUST be preserved when possible, reported through recovery diagnostics, and excluded from normal note restoration until repaired or replaced.

#### Scenario: Malformed workspace note does not block unrelated notes
- **WHEN** one workspace-note sidecar cannot be parsed during notes browser listing
- **THEN** valid workspace notes and document notes continue to load and appear in the notes browser
- **AND** the malformed workspace note is reported as a recovery diagnostic

#### Scenario: Opening a workspace with corrupt note keeps workspace usable
- **WHEN** a workspace root has a malformed workspace-note sidecar
- **THEN** the workspace loads and remains selectable
- **AND** the workspace-note workflow reports that the saved note could not be loaded

#### Scenario: Replacement preserves corrupt workspace-note evidence
- **WHEN** the user saves a new workspace note for a root whose previous note sidecar was malformed
- **THEN** the malformed sidecar is quarantined or otherwise preserved before replacement

### Requirement: Workspace-note root migrations are retryable
The system SHALL record pending workspace-note migrations before or as part of the post-rename workspace-root migration workflow. If migration or cleanup fails, the pending state MUST survive restart and be retried during startup reconciliation.

#### Scenario: Pending workspace-note migration survives restart
- **WHEN** an in-app workspace root rename succeeds but workspace-note migration fails before completion
- **THEN** a pending migration record remains in app data
- **AND** restarting LushText retries the workspace-note migration

#### Scenario: Completed workspace-note migration clears pending state
- **WHEN** workspace-note migration succeeds and obsolete sidecars are cleaned up or safely reconciled
- **THEN** the pending workspace-note migration record is removed durably

#### Scenario: Migration failure warns without losing root note text
- **WHEN** workspace-note migration fails after the root rename succeeded
- **THEN** the user receives warning feedback
- **AND** the existing workspace-note sidecar remains preserved for retry or inspection

### Requirement: Workspace-note reconciliation preserves root-note content
The system SHALL reconcile duplicate old and new workspace-note sidecars conservatively. It MUST preserve root-note text when deterministic identity or timestamp evidence makes a safe merge possible, and MUST preserve evidence instead of guessing when the conflict is ambiguous.

#### Scenario: Duplicate workspace notes choose deterministic newest body
- **WHEN** old and new workspace-note sidecars both exist and one can be identified as the newer durable save
- **THEN** the newer note body is kept for the migrated root identity
- **AND** the older copy is removed only after the target note is durably written

#### Scenario: Ambiguous workspace-note conflict is preserved
- **WHEN** duplicate workspace notes conflict and the newest body cannot be determined safely
- **THEN** the system does not discard either note body silently
- **AND** it reports that automatic workspace-note reconciliation was incomplete

#### Scenario: Aggregate notes browser reports partial workspace-note recovery
- **WHEN** the notes browser omits or quarantines a malformed workspace note in `All workspaces`
- **THEN** it still displays valid notes from other workspaces
- **AND** it exposes a warning that some workspace-note data could not be loaded

### Requirement: Workspace-note reliability has layered automated coverage
The project SHALL add deterministic service, integration, and widget coverage for workspace-note corruption, root-rename retry state, duplicate reconciliation, and partial notes-browser behavior.

#### Scenario: Service tests cover corrupt workspace-note sidecars
- **WHEN** service tests load malformed workspace-note sidecar bytes
- **THEN** the result preserves or quarantines the sidecar and returns recovery diagnostics
- **AND** unrelated valid workspace notes still load

#### Scenario: Migration tests cover workspace-note retry state
- **WHEN** tests simulate a workspace root rename whose workspace-note migration fails after the root rename
- **THEN** a pending migration record survives restart
- **AND** a later successful retry removes the record durably

#### Scenario: Widget tests cover partial workspace-note browsing
- **WHEN** the notes browser sees one corrupt workspace note and at least one valid note
- **THEN** the valid notes remain browsable
- **AND** visible partial-recovery feedback is shown

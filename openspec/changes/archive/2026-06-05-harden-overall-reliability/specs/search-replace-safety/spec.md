## ADDED Requirements

### Requirement: Replace All undo-journal corruption is isolated and diagnostic
The system SHALL handle malformed, partial, or unsupported Replace All undo-journal state as recoverable metadata. Corrupt journal entries MUST be preserved when possible, reported through recovery diagnostics, and MUST NOT be treated as a valid undo source.

#### Scenario: Malformed journal entry is preserved and excluded
- **WHEN** startup or search-panel initialization finds a malformed Replace All undo-journal entry
- **THEN** the malformed entry is quarantined or left untouched when quarantine fails
- **AND** the entry is not offered as an undo source
- **AND** a recovery diagnostic is recorded

#### Scenario: Partial journal does not imply successful undo state
- **WHEN** an undo-journal directory contains only some required metadata for a Replace All run
- **THEN** the system does not expose the Replace All undo action for that incomplete run
- **AND** surviving journal files are preserved or cleaned only after diagnostic handling succeeds

#### Scenario: Legacy backup corruption is reported during cleanup
- **WHEN** stale legacy `replace-backup.json` exists but cannot be parsed or removed at startup
- **THEN** the system records a diagnostic with the backup path and failure kind
- **AND** the new session does not claim that stale Replace All undo is available

### Requirement: Replace All journal cleanup is restart-safe
The system SHALL make cleanup of stale, completed, or invalid Replace All undo-journal state restart-safe. Cleanup MUST be ordered so that a crash during cleanup cannot convert a stale or corrupt journal into an active undo affordance after restart.

#### Scenario: Cleanup marker prevents resurrecting stale undo
- **WHEN** startup begins cleaning a stale Replace All journal and the process terminates before cleanup completes
- **THEN** relaunching LushText continues cleanup or reports the incomplete cleanup
- **AND** it does not expose the stale journal as an active undo affordance

#### Scenario: Undo completion removes journal durably
- **WHEN** the user successfully undoes a Replace All run
- **THEN** the journal is removed or marked inactive through a durable cleanup path
- **AND** restarting LushText cannot offer that completed undo again

#### Scenario: Cleanup failure remains diagnostic
- **WHEN** Replace All journal cleanup fails because of filesystem permissions or transient I/O errors
- **THEN** the system reports a diagnostic and retries later
- **AND** it does not spin in a tight retry loop

### Requirement: Replace All journal reliability has layered automated coverage
The project SHALL add deterministic service, integration, smoke, and property or fuzz-adjacent coverage for malformed undo journals, partial journals, restart-safe cleanup, and stale legacy backup handling.

#### Scenario: Service tests cover malformed journal entries
- **WHEN** service tests load malformed Replace All journal entries
- **THEN** the loader excludes those entries from active undo state
- **AND** it preserves or quarantines the original bytes and returns diagnostics

#### Scenario: Integration tests cover cleanup interruption
- **WHEN** tests simulate a crash or restart between stale journal cleanup steps
- **THEN** the next startup continues cleanup or reports incomplete cleanup
- **AND** the stale journal is never exposed as active undo state

#### Scenario: Generated journal states never produce invalid undo
- **WHEN** generated partial, duplicated, or corrupt journal directory states are loaded
- **THEN** the system never offers undo for a journal that lacks complete validated original bytes

#### Scenario: Crash smoke records stale journal handling when exercised
- **WHEN** crash recovery smoke creates or encounters Replace All journal state
- **THEN** the smoke artifacts record whether the journal was active, cleaned, quarantined, or skipped

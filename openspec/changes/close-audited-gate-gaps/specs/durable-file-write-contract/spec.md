# Spec Delta

## ADDED Requirements

### Requirement: Durable write failures name their phase and expose their cause
A durable write failure SHALL describe which phase failed (before the destination rename, or after it with durability unconfirmed) in its own message, and SHALL expose the underlying I/O error as its source, so an error chain shows the phase once and the cause once.

#### Scenario: Before-rename failure chain
- **WHEN** a durable write fails while syncing its temp file
- **THEN** the error's message says the destination keeps its previous bytes
- **AND** its source is the I/O error from the sync

#### Scenario: After-rename failure chain
- **WHEN** the parent-directory sync fails after the rename
- **THEN** the error's message says the new bytes are in place but not yet proven durable
- **AND** its source is the I/O error from the directory sync

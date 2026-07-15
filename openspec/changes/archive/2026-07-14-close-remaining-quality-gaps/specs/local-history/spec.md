## ADDED Requirements

### Requirement: Local-history preview loading and installation are bounded and superseding
The system SHALL retain at most one active local-history preview load and one latest compact selection request. Snapshot reading MUST remain size-gated and cooperatively cancellable, and accepted text above the synchronous threshold MUST be installed into the read-only preview buffer in bounded UTF-8-safe GTK slices. Copy and Restore MUST stay bound to one completely installed current snapshot.

#### Scenario: User rapidly selects large snapshots
- **WHEN** a large snapshot load is active and the user selects one or more different snapshots
- **THEN** the active load is cancelled cooperatively and only the latest pending selection is retained
- **AND** no stale text, title, metadata, Copy target, or Restore target is published

#### Scenario: Accepted preview requires several slices
- **WHEN** the current snapshot text exceeds the synchronous preview-install threshold
- **THEN** the preview buffer is cleared and populated through bounded UTF-8-safe main-loop slices
- **AND** repaint, input, and current asynchronous completions can run between slices

#### Scenario: Preview installation is superseded
- **WHEN** selection changes or the browser closes between preview-install slices
- **THEN** remaining slices stop without enabling Copy or Restore for the stale snapshot
- **AND** temporary sources and retained stale payloads are released

#### Scenario: Small, empty, missing, and failed snapshots terminate directly
- **WHEN** the selected snapshot is below the synchronous threshold, empty, missing, or unreadable
- **THEN** the browser reaches the corresponding existing content, empty, missing, or error state without scheduling unnecessary slices
- **AND** action sensitivity remains consistent with that terminal state

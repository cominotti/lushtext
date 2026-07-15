## ADDED Requirements

### Requirement: Document-sized GTK buffer replacement yields in bounded slices
Any workflow that clears or replaces document-sized editor content SHALL use one editor-owned bounded GTK mutation session above the synchronous threshold. The session MUST carry weak editor ownership, workflow-specific freshness identity, source ownership, projection suppression, and one typed terminal outcome. While a replacement is partial, the editor MUST remain non-editable and non-saveable, and modified, eviction, history, draft, cursor, monitor, and projection finalization MUST occur only after the complete current generation is installed or safely cancelled.

#### Scenario: Large clean editor is evicted
- **WHEN** memory policy accepts eviction of a clean reloadable editor whose buffer exceeds the synchronous replacement threshold
- **THEN** GTK clears the buffer in bounded main-loop slices
- **AND** the editor is marked evicted and its residency is released only after the current clear session completes

#### Scenario: Large recovery or history body is installed
- **WHEN** draft recovery, local-history restore, or local-history undo replaces a large buffer
- **THEN** GTK clears and inserts text through bounded slices with scheduling points between them
- **AND** no partial body becomes editable, saveable, or visible as a completed restore

#### Scenario: Save formatting rewrites a large live buffer
- **WHEN** save-time formatting produces document-sized text different from the live buffer
- **THEN** the accepted text is installed through the same bounded replacement contract
- **AND** save finalization cannot apply to a newer edit, path, save, or load generation

#### Scenario: Replacement becomes stale between slices
- **WHEN** the editor closes, changes workflow generation, or otherwise invalidates an active replacement
- **THEN** remaining slices stop and release their source and retained text exactly once
- **AND** the workflow reports a typed cancellation or failure without publishing successful terminal state

#### Scenario: Small replacement remains direct
- **WHEN** both the existing buffer and replacement text are below the calibrated synchronous threshold
- **THEN** the workflow MAY replace text in one GTK turn
- **AND** it observes the same freshness and terminal-finalization rules as a sliced replacement

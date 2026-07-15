## MODIFIED Requirements

### Requirement: Local-history restore is safe and reversible
The system SHALL restore historical snapshots into the active editor buffer without writing directly to disk. Before replacing buffer content, the system MUST store the current eligible buffer state as a fresh local-history snapshot. After a complete current restore, the system MUST mark the editor modified and MUST provide an immediate undo path. Large restore and undo bodies MUST use the bounded GTK replacement contract, and a partial replacement MUST remain non-editable and non-saveable until exact finalization. The system SHALL also provide a non-destructive copy action for the selected snapshot.

#### Scenario: Restore a historical snapshot
- **WHEN** the user chooses Restore for a selected snapshot in the local-history browser
- **THEN** the system stores the current eligible buffer content as a fresh local-history snapshot before applying the selected snapshot
- **AND** the editor buffer is replaced with the selected snapshot content
- **AND** the editor is marked modified only after complete installation

#### Scenario: Restore a large historical snapshot
- **WHEN** the selected or current body exceeds the synchronous replacement threshold
- **THEN** history preparation and buffer replacement retain bounded full-body ownership and yield between GTK slices
- **AND** no partial snapshot can be edited, saved, or reported as restored

#### Scenario: Undo a restore
- **WHEN** the user restores a snapshot and then invokes the immediate undo affordance for that restore
- **THEN** the system returns the editor buffer to the content that was active immediately before the restore
- **AND** a large undo body observes the same bounded installation and freshness rules

#### Scenario: Restore becomes stale
- **WHEN** editor lifetime, path identity, or history generation changes while replacement is pending
- **THEN** remaining work is cancelled without publishing successful restore state
- **AND** retained source and undo bodies are released exactly once

#### Scenario: Copy snapshot content
- **WHEN** the user chooses Copy for a selected snapshot in the local-history browser
- **THEN** the system copies that snapshot content without modifying the active editor buffer

## ADDED Requirements

### Requirement: Workspace refresh stays scoped to one persisted root directory
The system SHALL treat refresh and watcher behavior as section-local behavior for exactly one persisted workspace root directory. A workspace section MUST refresh only its own persisted root and the drill-down descendants derived from that root. It MUST NOT merge multiple persisted roots or standalone file entries into one refreshable section.

#### Scenario: Replacing a workspace root changes the refresh base
- **WHEN** the user replaces a workspace root and later triggers manual or automatic refresh
- **THEN** that workspace section refreshes only the new persisted root directory
- **AND** it stops treating the previous root as part of the same section's refresh scope

#### Scenario: Normalized sibling workspaces refresh independently
- **WHEN** legacy multi-root workspace data has been normalized into multiple single-root workspaces
- **THEN** each resulting workspace section refreshes only its own root directory
- **AND** changes under one normalized workspace root do not appear inside another workspace section unless the user is using the explicit aggregate `All workspaces` shell view

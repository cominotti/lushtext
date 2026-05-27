## REMOVED Requirements

### Requirement: Workspace-aware note and export flows honor the current workspace scope
**Reason**: Range-note export and annotation browser workflows are removed; the remaining scope contract covers note and bookmark browse workflows.

## ADDED Requirements

### Requirement: Workspace-aware note and bookmark flows honor the current workspace scope
The system SHALL scope workspace-aware note and bookmark browse workflows to the current shared workspace scope. A concrete workspace scope MUST limit those flows to one workspace root. The aggregate scope MUST include all restored workspaces.

#### Scenario: Bookmark and note browsers stay inside the selected workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens a workspace-scoped bookmark or notes browser
- **THEN** the browser lists only supported records from that workspace's root directory
- **AND** records from other workspaces are excluded

#### Scenario: Aggregate scope browses across all workspaces
- **WHEN** `All workspaces` is the current shared scope and the user opens a workspace-scoped bookmark or notes browser
- **THEN** the browser includes supported records from every restored workspace
- **AND** the browser is not silently narrowed to one workspace

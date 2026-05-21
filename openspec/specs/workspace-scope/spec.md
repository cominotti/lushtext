# workspace-scope Specification

## Purpose
Define the shared current workspace scope that the sidebar, search, palette indexing, and workspace-aware note/export workflows all honor consistently.
## Requirements
### Requirement: Workspace scope is a shared app-wide concept
The system SHALL maintain one current workspace scope that all workspace-aware features share. The legal scope values MUST be either a specific workspace or the explicit aggregate scope `All workspaces`. The sidebar selector MUST update that shared scope instead of acting as a sidebar-only visibility filter.

#### Scenario: Selecting one workspace updates the shared scope
- **WHEN** the user selects a specific workspace from the sidebar scope selector
- **THEN** that workspace becomes the current shared workspace scope
- **AND** workspace-aware features observe that same selection

#### Scenario: Selecting All workspaces activates the aggregate scope
- **WHEN** the user selects `All workspaces` from the sidebar scope selector
- **THEN** the current shared workspace scope becomes the aggregate `All workspaces` scope
- **AND** workspace-aware features treat that aggregate scope as the active scope until the user chooses a concrete workspace

### Requirement: Search and palette results honor the current workspace scope
The system SHALL scope workspace-aware search and command palette workspace-file indexing to the current shared workspace scope. When a concrete workspace is selected, workspace-aware search and the command palette workspace file group MUST use only that workspace's root directory. When `All workspaces` is selected, they MUST aggregate the roots of every workspace. The command palette MAY also present matching open file-backed tabs in a separate `Open Tabs` group before workspace-indexed file results; that active-document group MUST NOT change the current workspace scope and MUST NOT cause open files to be relabeled as workspace-indexed results.

#### Scenario: Search stays inside the selected workspace
- **WHEN** a concrete workspace is the current shared scope and the user runs a workspace search
- **THEN** search results come only from that workspace's root directory
- **AND** files outside that workspace do not appear in the results

#### Scenario: Palette workspace group stays inside the selected workspace
- **WHEN** a concrete workspace is the current shared scope and the user runs a file-palette lookup
- **THEN** workspace-indexed palette results come only from that workspace's root directory
- **AND** matching open file-backed tabs outside that workspace may appear only in the separate `Open Tabs` group

#### Scenario: Aggregate scope searches across all workspaces
- **WHEN** `All workspaces` is the current shared scope and the user runs a workspace search or file-palette lookup
- **THEN** the workspace-aware feature searches across the roots of every restored workspace
- **AND** results from multiple workspaces may appear together

### Requirement: Workspace-aware note and export flows honor the current workspace scope
The system SHALL scope workspace-aware note, bookmark, annotation, and export workflows to the current shared workspace scope. A concrete workspace scope MUST limit those flows to one workspace root. The aggregate scope MUST include all restored workspaces.

#### Scenario: Bookmark and annotation browsers stay inside the selected workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens a workspace-scoped bookmark or annotation browser
- **THEN** the browser lists only records from that workspace's root directory
- **AND** records from other workspaces are excluded

#### Scenario: Aggregate scope exports across all workspaces
- **WHEN** `All workspaces` is the current shared scope and the user runs a workspace-scoped export workflow
- **THEN** the export includes data from every restored workspace
- **AND** the export is not silently narrowed to one workspace

### Requirement: Workspace creation and removal update shared scope predictably
The system SHALL update the current shared workspace scope predictably when workspaces are created or removed. Creating a workspace MUST select that new workspace as the current shared scope. Removing the currently selected workspace MUST fall back to the explicit aggregate `All workspaces` scope instead of silently rebasing to another concrete workspace.

#### Scenario: Creating a workspace selects it immediately
- **WHEN** the user creates a new workspace from the sidebar shell
- **THEN** the new workspace becomes the current shared workspace scope
- **AND** workspace-aware features update to use that workspace

#### Scenario: Removing the selected workspace falls back to All workspaces
- **WHEN** the user removes the currently selected workspace while other workspaces still exist
- **THEN** the current shared workspace scope becomes `All workspaces`
- **AND** the app does not silently choose a different concrete workspace instead

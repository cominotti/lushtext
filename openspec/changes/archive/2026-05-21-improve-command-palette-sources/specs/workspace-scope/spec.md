## MODIFIED Requirements

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

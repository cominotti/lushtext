## ADDED Requirements

### Requirement: Command palette mode selection is mouse and keyboard accessible
The system SHALL expose command palette modes `All`, `Files`, and `Commands` through a mouse-usable selector. The system MUST also preserve Tab as a keyboard shortcut that cycles through the same modes and keeps the selector state synchronized.

#### Scenario: Mouse changes palette mode
- **WHEN** the command palette is open and the user chooses `Files` from the mode selector with the mouse
- **THEN** the active palette mode becomes `Files`
- **AND** the result list refreshes for file-oriented results

#### Scenario: Tab changes palette mode
- **WHEN** the command palette is open and the user presses Tab
- **THEN** the active palette mode advances to the next mode
- **AND** the visible mode selector reflects the new active mode

### Requirement: Files mode groups file results by source
The system SHALL present file results in `Files` mode under labeled source groups. `Open Tabs` MUST appear before the workspace file group when both groups have matching results. The workspace file group label MUST be `Selected Workspace` when the current sidebar scope is one concrete workspace and `All Workspaces` when the current sidebar scope is the aggregate `All workspaces` scope.

#### Scenario: Open tabs appear before selected workspace files
- **WHEN** the command palette is in `Files` mode
- **AND** the query matches both an open file-backed tab and a file from the selected workspace
- **THEN** the matching open tab appears under `Open Tabs`
- **AND** the matching workspace file appears under `Selected Workspace`
- **AND** the `Open Tabs` group is presented before `Selected Workspace`

#### Scenario: Aggregate workspace scope uses All Workspaces label
- **WHEN** the command palette is in `Files` mode
- **AND** the sidebar scope selector is set to `All workspaces`
- **AND** the query matches files from restored workspaces
- **THEN** matching workspace-indexed files appear under `All Workspaces`

### Requirement: All mode groups open tabs, workspace files, and commands by priority
The system SHALL present `All` mode results in labeled groups ordered as `Open Tabs`, then the current workspace-scope file group, then `Commands`. The workspace file group MUST use the same `Selected Workspace` or `All Workspaces` label rules as `Files` mode.

#### Scenario: All mode preserves source priority
- **WHEN** the command palette is in `All` mode
- **AND** the query matches an open file-backed tab, a workspace-indexed file, and a command
- **THEN** the open tab appears under `Open Tabs`
- **AND** the workspace-indexed file appears under the current workspace-scope group
- **AND** the command appears under `Commands`
- **AND** the groups are presented in that order

### Requirement: File results are deduplicated across source groups
The system SHALL show a file path at most once in grouped command palette results. If a matching file is both an open file-backed tab and a workspace-indexed file, the result MUST appear only under `Open Tabs`.

#### Scenario: Open tab suppresses duplicate workspace result
- **WHEN** a file is open in a tab
- **AND** the same file is included in the current workspace file index
- **AND** the command palette query matches that file
- **THEN** the file appears under `Open Tabs`
- **AND** the same file does not also appear under the workspace file group

### Requirement: Group headers are presentation-only
The system SHALL render source labels as presentation-only group headers. Group headers MUST NOT activate a file or command, and keyboard result navigation MUST move between activatable result rows.

#### Scenario: Activating a grouped result ignores headers
- **WHEN** grouped command palette results are visible
- **AND** the user activates a selected result row
- **THEN** only file and command result rows can trigger file opening or command execution
- **AND** source group headers do not trigger activation callbacks

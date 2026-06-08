## MODIFIED Requirements

### Requirement: Files mode groups file results by source
The system SHALL present file results in `Files` mode under labeled source groups. `Open Tabs` MUST appear before the workspace file group when both groups have matching results. The workspace file group label MUST be `Selected Workspace` when the current sidebar scope is one concrete workspace and `All Workspaces` when the current sidebar scope is the aggregate `All workspaces` scope. Workspace file results SHALL be built from the current workspace scope's ordered folder set and MUST de-duplicate canonical file identities across overlapping folders.

#### Scenario: Open tabs appear before selected workspace files
- **WHEN** the command palette is in `Files` mode
- **AND** the query matches both an open file-backed tab and a file from the selected workspace folder set
- **THEN** the matching open tab appears under `Open Tabs`
- **AND** the matching workspace file appears under `Selected Workspace`
- **AND** the `Open Tabs` group is presented before `Selected Workspace`

#### Scenario: Aggregate workspace scope uses All Workspaces label
- **WHEN** the command palette is in `Files` mode
- **AND** the sidebar scope selector is set to `All workspaces`
- **AND** the query matches files from restored workspace folders
- **THEN** matching workspace-indexed files appear under `All Workspaces`

#### Scenario: Empty selected workspace has no workspace file group rows
- **WHEN** the command palette is in `Files` mode
- **AND** the selected workspace contains zero folders
- **AND** the query has no matching open file-backed tabs
- **THEN** the palette does not show stale workspace-indexed file rows from another workspace
- **AND** the visible result state remains stable and searchable

### Requirement: All mode groups open tabs, workspace files, and commands by priority
The system SHALL present `All` mode results in labeled groups ordered as `Open Tabs`, then the current workspace-scope file group, then `Commands`. The workspace file group MUST use the same `Selected Workspace` or `All Workspaces` label rules as `Files` mode. Workspace-indexed files SHALL come from the current workspace scope's ordered folder set and MUST be de-duplicated by canonical file identity before command rows are mixed in.

#### Scenario: All mode preserves source priority
- **WHEN** the command palette is in `All` mode
- **AND** the query matches an open file-backed tab, a workspace-indexed file, and a command
- **THEN** the open tab appears under `Open Tabs`
- **AND** the workspace-indexed file appears under the current workspace-scope group
- **AND** the command appears under `Commands`
- **AND** the groups are presented in that order

#### Scenario: All mode suppresses overlapping workspace duplicates
- **WHEN** the selected workspace contains overlapping folders that both cover `/repo/src/main.rs`
- **AND** the command palette query matches that file and one command
- **THEN** `/repo/src/main.rs` appears at most once in the workspace-scope file group
- **AND** the matching command remains available under `Commands`

### Requirement: File results are deduplicated across source groups
The system SHALL show a file path at most once in grouped command palette results. If a matching file is both an open file-backed tab and a workspace-indexed file, the result MUST appear only under `Open Tabs`. If overlapping folders in the current workspace scope index the same canonical file, the workspace file group MUST contain only one result for that file, using folder order to choose the primary workspace/folder context displayed for the row.

#### Scenario: Open tab suppresses duplicate workspace result
- **WHEN** a file is open in a tab
- **AND** the same file is included in the current workspace file index
- **AND** the command palette query matches that file
- **THEN** the file appears under `Open Tabs`
- **AND** the same file does not also appear under the workspace file group

#### Scenario: Overlapping workspace folders suppress duplicate workspace rows
- **WHEN** the selected workspace contains folders `/repo` and `/repo/src`
- **AND** `/repo/src/main.rs` is indexed through both folders
- **AND** the command palette query matches `main.rs`
- **THEN** the workspace file group shows one row for `/repo/src/main.rs`
- **AND** the row's source context is based on the earliest covering folder in the selected workspace order

#### Scenario: Same file in different workspaces is deduplicated in aggregate scope
- **WHEN** `All workspaces` is the current shared scope
- **AND** two workspaces contain the same canonical folder or overlapping folders that cover the same file
- **AND** the command palette query matches that file
- **THEN** the aggregate workspace file group shows that canonical file at most once
- **AND** the row's primary context is chosen by workspace order and then folder order

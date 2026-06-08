# command-palette-source-groups Specification

## Purpose
Define how command palette modes and source-grouped results present files, commands, open tabs, and workspace-scoped files.

## Requirements
### Requirement: Command palette mode selection is mouse and keyboard accessible
The system SHALL expose command palette modes `All`, `Files`, `Notes`, and `Commands` through a mouse-usable selector. The selector order MUST be `All`, then `Files`, then `Notes`, then `Commands`. The system MUST also preserve Tab as a keyboard shortcut that cycles through the same modes in selector order and keeps the selector state synchronized.

#### Scenario: Mouse changes palette mode
- **WHEN** the command palette is open and the user chooses `Files` from the mode selector with the mouse
- **THEN** the active palette mode becomes `Files`
- **AND** the result list refreshes for file-oriented results

#### Scenario: Mouse changes palette to Notes mode
- **WHEN** the command palette is open and the user chooses `Notes` from the mode selector with the mouse
- **THEN** the active palette mode becomes `Notes`
- **AND** the result list refreshes for note and bookmark workflow commands

#### Scenario: Tab changes palette mode
- **WHEN** the command palette is open and the user presses Tab from `Files` mode
- **THEN** the active palette mode advances to `Notes`
- **AND** the visible mode selector reflects the new active mode

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

### Requirement: All mode groups open tabs, workspace files, notes, and commands by priority
The system SHALL present `All` mode results in labeled groups ordered as `Open Tabs`, then the current workspace-scope file group, then `Notes`, then `Commands`. The workspace file group MUST use the same `Selected Workspace` or `All Workspaces` label rules as `Files` mode. Workspace-indexed files SHALL come from the current workspace scope's ordered folder set and MUST be de-duplicated by canonical file identity before command rows are mixed in. The `Notes` group MUST contain matching commands in the `Notes` command category. The `Commands` group MUST contain matching non-note commands and MUST NOT duplicate commands already shown in `Notes`.

#### Scenario: All mode preserves source priority
- **WHEN** the command palette is in `All` mode
- **AND** the query matches an open file-backed tab, a workspace-indexed file, a note command, and a non-note command
- **THEN** the open tab appears under `Open Tabs`
- **AND** the workspace-indexed file appears under the current workspace-scope group
- **AND** the note command appears under `Notes`
- **AND** the non-note command appears under `Commands`
- **AND** the groups are presented in that order

#### Scenario: All mode suppresses overlapping workspace duplicates
- **WHEN** the selected workspace contains overlapping folders that both cover `/repo/src/main.rs`
- **AND** the command palette query matches that file and one command
- **THEN** `/repo/src/main.rs` appears at most once in the workspace-scope file group
- **AND** the matching command remains available under `Commands`

#### Scenario: All mode does not duplicate note commands
- **WHEN** the command palette is in `All` mode
- **AND** the query matches a command in the `Notes` command category
- **THEN** the matching note command appears under `Notes`
- **AND** the same command does not also appear under `Commands`

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

### Requirement: Notes command category identifies note workflows
The system SHALL define a `Notes` command category for note and bookmark workflows exposed by the command palette. The category MUST include `Browse Notes`, `Browse Bookmarks`, `Toggle Bookmark`, `Edit Bookmark`, `Next Bookmark`, `Previous Bookmark`, `Open Document Note`, and `Open Folder Note`. Palette subtitles for those commands MUST display `Notes` and MUST preserve any existing shortcut hint.

#### Scenario: Note commands use the Notes category
- **WHEN** the command palette displays `Browse Notes`, `Browse Bookmarks`, `Toggle Bookmark`, `Edit Bookmark`, `Next Bookmark`, `Previous Bookmark`, `Open Document Note`, or `Open Folder Note`
- **THEN** each row is categorized as `Notes`
- **AND** each row keeps its existing action id and shortcut hint

#### Scenario: Non-note commands keep their existing categories
- **WHEN** the command palette displays file, edit, view, or app commands that are not note or bookmark workflows
- **THEN** those commands remain categorized as `File`, `Edit`, `View`, or `App`

### Requirement: Notes mode groups note workflow launchers by intent
The system SHALL present `Notes` mode results as command launchers grouped by intent sections. Matching rows MUST appear under section headers ordered as `Browse`, `Current Document`, `Bookmark Navigation`, and `Workspace`. `Browse` MUST contain `Browse Notes` and `Browse Bookmarks`; `Current Document` MUST contain `Toggle Bookmark`, `Edit Bookmark`, and `Open Document Note`; `Bookmark Navigation` MUST contain `Next Bookmark` and `Previous Bookmark`; `Workspace` MUST contain `Open Folder Note`. Sections with no matching rows MUST be omitted.

#### Scenario: Notes mode shows intent sections
- **WHEN** the command palette is in `Notes` mode
- **AND** the current query matches commands from every Notes intent section
- **THEN** matching browse commands appear under `Browse`
- **AND** matching document-local commands appear under `Current Document`
- **AND** matching bookmark movement commands appear under `Bookmark Navigation`
- **AND** matching workspace commands appear under `Workspace`
- **AND** the sections are presented in that order

#### Scenario: Notes mode excludes files and non-note commands
- **WHEN** the command palette is in `Notes` mode
- **AND** the query matches workspace files, open file-backed tabs, note commands, and non-note commands
- **THEN** only matching commands in the `Notes` command category appear
- **AND** workspace files, open file-backed tabs, and non-note commands are not shown

#### Scenario: Notes mode delegates stored-note search to Browse Notes
- **WHEN** the command palette is in `Notes` mode
- **THEN** the palette does not enumerate individual bookmarks, document notes, folder notes, or note-body matches
- **AND** activating `Browse Notes` opens the existing notes browser for stored-note search

### Requirement: Commands mode remains the complete command search
The system SHALL keep `Commands` mode as the complete command registry search surface. Commands in the `Notes` category MUST remain searchable and activatable in `Commands` mode, and their subtitles MUST identify them as `Notes`.

#### Scenario: Commands mode includes note commands
- **WHEN** the command palette is in `Commands` mode
- **AND** the query matches `Browse Notes`
- **THEN** `Browse Notes` appears in the command results
- **AND** its subtitle identifies it as a `Notes` command

#### Scenario: Commands mode includes non-note commands
- **WHEN** the command palette is in `Commands` mode
- **AND** the query matches a non-note command
- **THEN** the matching non-note command appears in the command results
- **AND** it keeps its existing command category

### Requirement: Group headers are presentation-only
The system SHALL render source labels as presentation-only group headers. Group headers MUST NOT activate a file or command, and keyboard result navigation MUST move between activatable result rows.

#### Scenario: Activating a grouped result ignores headers
- **WHEN** grouped command palette results are visible
- **AND** the user activates a selected result row
- **THEN** only file and command result rows can trigger file opening or command execution
- **AND** source group headers do not trigger activation callbacks

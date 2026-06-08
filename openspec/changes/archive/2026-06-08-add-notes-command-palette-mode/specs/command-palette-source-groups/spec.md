## MODIFIED Requirements

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

### Requirement: All mode groups open tabs, workspace files, notes, and commands by priority
The system SHALL present `All` mode results in labeled groups ordered as `Open Tabs`, then the current workspace-scope file group, then `Notes`, then `Commands`. The workspace file group MUST use the same `Selected Workspace` or `All Workspaces` label rules as `Files` mode. The `Notes` group MUST contain matching commands in the `Notes` command category. The `Commands` group MUST contain matching non-note commands and MUST NOT duplicate commands already shown in `Notes`.

#### Scenario: All mode preserves source priority
- **WHEN** the command palette is in `All` mode
- **AND** the query matches an open file-backed tab, a workspace-indexed file, a note command, and a non-note command
- **THEN** the open tab appears under `Open Tabs`
- **AND** the workspace-indexed file appears under the current workspace-scope group
- **AND** the note command appears under `Notes`
- **AND** the non-note command appears under `Commands`
- **AND** the groups are presented in that order

#### Scenario: All mode does not duplicate note commands
- **WHEN** the command palette is in `All` mode
- **AND** the query matches a command in the `Notes` command category
- **THEN** the matching note command appears under `Notes`
- **AND** the same command does not also appear under `Commands`

## ADDED Requirements

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

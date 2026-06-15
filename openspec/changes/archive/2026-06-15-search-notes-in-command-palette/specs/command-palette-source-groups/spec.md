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
- **AND** the result list refreshes for searchable note and bookmark records

#### Scenario: Tab changes palette mode
- **WHEN** the command palette is open and the user presses Tab from `Files` mode
- **THEN** the active palette mode advances to `Notes`
- **AND** the visible mode selector reflects the new active mode

### Requirement: All mode groups open tabs, workspace files, notes, and commands by priority
The system SHALL present `All` mode results in labeled groups ordered as `Open Tabs`, then the current workspace-scope file group, then note record groups, then `Commands`. The workspace file group MUST use the same `Selected Workspace` or `All Workspaces` label rules as `Files` mode. Workspace-indexed files SHALL come from the current workspace scope's ordered folder set and MUST be de-duplicated by canonical file identity before note and command rows are mixed in. Note record groups MUST use the Notes browser category vocabulary, ordered as `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tab Notes`; empty note groups MUST be omitted. The `Commands` group MUST contain matching command registry rows, including commands in the `Notes` command category, and MUST NOT contain note records.

#### Scenario: All mode preserves source priority
- **WHEN** the command palette is in `All` mode
- **AND** the query matches an open file-backed tab, a workspace-indexed file, a bookmark row, a document-note body, a note command, and a non-note command
- **THEN** the open file-backed tab appears under `Open Tabs`
- **AND** the workspace-indexed file appears under the current workspace-scope group
- **AND** the bookmark row appears under `Bookmarks`
- **AND** the document-note row appears under `Document Notes`
- **AND** the note command and non-note command appear under `Commands`
- **AND** the groups are presented in that order

#### Scenario: All mode suppresses overlapping workspace duplicates
- **WHEN** the selected workspace contains overlapping folders that both cover `/repo/src/main.rs`
- **AND** the command palette query matches that file and one command
- **THEN** `/repo/src/main.rs` appears at most once in the workspace-scope file group
- **AND** the matching command remains available under `Commands`

#### Scenario: All mode keeps note commands in Commands
- **WHEN** the command palette is in `All` mode
- **AND** the query matches a command in the `Notes` command category
- **THEN** the matching note command appears under `Commands`
- **AND** the same command does not appear under any note record group

#### Scenario: All mode separates open-tab files from open-tab notes
- **WHEN** the command palette is in `All` mode
- **AND** the query matches an open file-backed tab and a note record attached to a saved open tab outside the current workspace scope
- **THEN** the open file-backed tab appears under `Open Tabs`
- **AND** the note record appears under `Open Tab Notes`

### Requirement: Group headers are presentation-only
The system SHALL render source labels as presentation-only group headers. Group headers MUST NOT activate a file, note, or command, and keyboard result navigation MUST move between activatable result rows.

#### Scenario: Activating a grouped result ignores headers
- **WHEN** grouped command palette results are visible
- **AND** the user activates a selected result row
- **THEN** only file, note, and command result rows can trigger file opening, note opening, or command execution
- **AND** source group headers do not trigger activation callbacks

## ADDED Requirements

### Requirement: Notes mode groups searchable note records by category
The system SHALL present `Notes` mode as a searchable note-record surface rather than a command-launcher subset. Matching rows MUST appear under category headers ordered as `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tabs`. The system MUST omit empty categories, MUST NOT show workspace files or command rows in `Notes` mode, and MUST keep the search entry, mode selector, result list, and close behavior usable when no note records match.

#### Scenario: Notes mode shows note category sections
- **WHEN** the command palette is in `Notes` mode
- **AND** the current workspace scope contains matching bookmarks, folder notes, document notes, and eligible saved open-tab note rows
- **THEN** matching bookmark rows appear under `Bookmarks`
- **AND** matching folder-note rows appear under `Folder Notes`
- **AND** matching document-note rows appear under `Document Notes`
- **AND** matching saved open-tab note rows outside the current workspace scope appear under `Open Tabs`
- **AND** the sections are presented in that order

#### Scenario: Notes mode searches note bodies and note metadata
- **WHEN** the command palette is in `Notes` mode
- **AND** the query matches a document-note body, a folder-note body, a bookmark label, a bookmark line number, a saved file path, a workspace name, or a workspace folder path
- **THEN** the matching note or bookmark rows appear in their note category sections
- **AND** non-matching note and bookmark rows are hidden without changing persisted note data

#### Scenario: Notes mode excludes commands and files
- **WHEN** the command palette is in `Notes` mode
- **AND** the query matches workspace files, open file-backed tabs, note commands, non-note commands, and note records
- **THEN** only matching note and bookmark records appear
- **AND** workspace files, open file-backed tabs, note commands, and non-note commands are not shown

#### Scenario: Empty Notes mode stays readable
- **WHEN** the command palette is in `Notes` mode
- **AND** there are no browsable notes, no bookmarks, no matching open-tab note rows, or the query filters every note row out
- **THEN** the palette shows its explicit no-results state without fake note rows
- **AND** the search entry, mode selector, keyboard mode cycling, Escape dismissal, and click-away dismissal remain usable

#### Scenario: Many Notes mode matches stay bounded
- **WHEN** the command palette is in `Notes` mode
- **AND** the current workspace scope contains many matching note and bookmark rows with long titles, long paths, or long note body matches
- **THEN** result rows remain sectioned by note category
- **AND** the result list remains the only scrolling region
- **AND** row text is ellipsized or clipped within the palette without introducing unintended horizontal scrolling

#### Scenario: Bookmark source excerpts do not drive palette search
- **WHEN** the command palette is in `Notes` mode
- **AND** the query appears only in the closed source file text around a bookmark, not in the bookmark label, line metadata, file metadata, workspace metadata, or note metadata
- **THEN** the bookmark row does not match merely because of the closed source excerpt
- **AND** the palette does not read closed source files solely to decide bookmark search matches

### Requirement: Note result activation opens the associated note target
The system SHALL activate note result rows through the same target workflows used by `Browse Notes...`. Bookmark rows MUST open or focus the bookmarked file at the bookmarked line, folder-note rows MUST open the targeted folder note, and document-note rows MUST open or focus the associated saved file and open its document-note surface. Activating a note row MUST close the command palette only after dispatching the target workflow.

#### Scenario: Activate a bookmark result
- **WHEN** the command palette is in `Notes` mode or `All` mode
- **AND** the selected result is a bookmark row
- **AND** the user activates the row
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line

#### Scenario: Activate a folder-note result
- **WHEN** the command palette is in `Notes` mode or `All` mode
- **AND** the selected result is a folder-note row
- **AND** the user activates the row
- **THEN** the system opens that folder note's surface
- **AND** it does not require an active document tab

#### Scenario: Activate a document-note result
- **WHEN** the command palette is in `Notes` mode or `All` mode
- **AND** the selected result is a document-note row
- **AND** the user activates the row
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the system opens that file's document-note surface

## REMOVED Requirements

### Requirement: Notes mode groups note workflow launchers by intent
**Reason**: `Notes` mode is being redefined as a searchable note-record surface. Note workflow launchers duplicate the complete `Commands` mode when shown as top-level Notes results.

**Migration**: Note workflow commands remain available through `Commands` mode and the `Commands` group in `All` mode, with their existing action IDs, shortcuts, and `Notes` command subtitles.

## MODIFIED Requirements

### Requirement: Notes menu availability follows editor and workspace context
The system SHALL keep the `Notes` menu surface aligned with the current window context while treating `Browse Notes…` as a window-scoped entry point. When the header bar itself is visible, the `Notes` menu button MUST remain visible even when the window has no active editor and no restored workspace folders. Menu items MUST use sensitivity to reflect actionability: actions that require a saved file MUST be insensitive when the active document has no stable path, the bookmark toggle MUST say `Remove Bookmark` when the cursor is on an existing bookmark and `Add Bookmark` otherwise, and `Open Folder Note…` MUST be insensitive when the current shared scope is `All workspaces` or no concrete workspace folder target is selected. `Browse Notes…` MUST remain actionable from the header menu and MUST open the existing Notes browser, including its explicit empty state when there are no browsable notes or bookmarks.

#### Scenario: Show folder-note actions without a saved document
- **WHEN** the active document is untitled and the current window has a concrete workspace scope
- **THEN** the `Notes` menu button remains visible
- **AND** the bookmark toggle and `Open Document Note…` are insensitive
- **AND** `Open Folder Note…` and `Browse Notes…` remain actionable

#### Scenario: Show Browse Notes after closing the last tab
- **WHEN** the window has restored workspace folders
- **AND** the user closes the last open tab
- **THEN** the header bar still shows the `Notes` menu button
- **AND** `Browse Notes…` remains actionable
- **AND** actions that require an active saved document are insensitive

#### Scenario: Show Browse Notes in an empty no-workspace window
- **WHEN** the window has no active editor
- **AND** no workspace folders are restored
- **THEN** the header bar still shows the `Notes` menu button
- **AND** `Browse Notes…` remains actionable
- **AND** activating `Browse Notes…` opens the Notes browser empty state without creating workspace, bookmark, or document-note data

#### Scenario: Reflect bookmark toggle state in the menu label
- **WHEN** the active document is saved and the cursor is on a line without a bookmark
- **THEN** the bookmark toggle menu item is labeled `Add Bookmark`
- **AND** activating it adds a bookmark for the active line
- **WHEN** the cursor moves onto a bookmarked line
- **THEN** the bookmark toggle menu item is labeled `Remove Bookmark`
- **AND** activating it removes the bookmark for the active line

#### Scenario: Aggregate scope disables the single-folder note action
- **WHEN** the current shared scope is `All workspaces`
- **THEN** `Open Folder Note…` is insensitive
- **AND** `Browse Notes…` remains actionable

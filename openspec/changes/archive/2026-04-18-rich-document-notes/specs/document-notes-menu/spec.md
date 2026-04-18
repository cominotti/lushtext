## MODIFIED Requirements

### Requirement: Notes workflows use a dedicated secondary menu
The system SHALL expose bookmark workflows together with range-note, document-note, and workspace-note workflows through a dedicated `Notes` secondary menu in the window header bar. Once the `Notes` menu is available, the primary menu MUST not list bookmark or note commands, and the notes surface MUST not rely on nested submenus.

#### Scenario: Open note workflows from the header bar
- **WHEN** the current window can surface one or more bookmark or note workflows
- **THEN** the header bar shows a dedicated `Notes` menu button
- **AND** opening that menu reveals bookmark and note commands
- **AND** opening the primary menu does not reveal bookmark or note commands

### Requirement: Notes menu groups actions by scope
The system SHALL organize the `Notes` menu into a current-document section and a workspace section. The current-document section MUST contain `Toggle Bookmark`, `Edit Bookmark Label…`, `Add Range Note…`, `Edit Range Note…`, and `Open Document Note…`. The workspace section MUST contain `Open Workspace Note…`, `Browse Bookmarks…`, `Browse Notes…`, and `Export Range Notes…`.

#### Scenario: Open the Notes menu for a saved file in a concrete workspace
- **WHEN** the active window has a saved document and a concrete current workspace scope
- **THEN** the `Notes` menu shows the current-document section before the workspace section
- **AND** the current-document section contains only document-scoped bookmark and note actions
- **AND** the workspace section contains only workspace-scoped browse, open, and export actions

### Requirement: Notes menu availability follows editor and workspace context
The system SHALL keep the `Notes` menu surface aligned with the current window context. The `Notes` menu button MUST be hidden when the window has neither an active editor nor a current workspace scope. Menu items MUST use sensitivity to reflect actionability: actions that require a saved file MUST be insensitive when the active document has no stable path, cursor-specific range-note edit actions MUST be insensitive when the cursor is not on an eligible range note, and `Open Workspace Note…` MUST be insensitive when the current shared scope is `All workspaces`. Workspace-scoped browse and export actions MUST remain actionable whenever the window still has workspace scope.

#### Scenario: Show workspace note actions without a saved document
- **WHEN** the active document is untitled and the current window has a concrete workspace scope
- **THEN** the `Notes` menu button remains visible
- **AND** `Toggle Bookmark`, `Edit Bookmark Label…`, `Add Range Note…`, `Edit Range Note…`, and `Open Document Note…` are insensitive
- **AND** `Open Workspace Note…`, `Browse Bookmarks…`, `Browse Notes…`, and `Export Range Notes…` remain actionable

#### Scenario: Disable cursor-specific edit actions when no note is eligible
- **WHEN** the active document is saved but the cursor is not on a bookmarked or ranged-note location
- **THEN** `Toggle Bookmark`, `Add Range Note…`, and `Open Document Note…` remain actionable
- **AND** the cursor-specific edit action without an eligible item is insensitive

#### Scenario: Aggregate scope disables the single-workspace note action
- **WHEN** the current shared scope is `All workspaces`
- **THEN** `Open Workspace Note…` is insensitive
- **AND** `Browse Bookmarks…`, `Browse Notes…`, and `Export Range Notes…` remain actionable

#### Scenario: Hide the Notes menu when no note workflow is available
- **WHEN** the window has no active editor and no current workspace scope
- **THEN** the header bar does not show the `Notes` menu button

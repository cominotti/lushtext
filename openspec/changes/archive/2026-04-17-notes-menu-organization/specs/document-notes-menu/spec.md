## ADDED Requirements

### Requirement: Notes workflows use a dedicated secondary menu
The system SHALL expose bookmark and annotation workflows through a dedicated `Notes` secondary menu in the window header bar. Once the `Notes` menu is available, the primary menu MUST not list bookmark or annotation commands, and the notes surface MUST not rely on nested submenus.

#### Scenario: Open note workflows from the header bar
- **WHEN** the current window can surface one or more note workflows
- **THEN** the header bar shows a dedicated `Notes` menu button
- **AND** opening that menu reveals bookmark and annotation commands
- **AND** opening the primary menu does not reveal bookmark or annotation commands

### Requirement: Notes placement follows the header-bar menu hierarchy
While the app-wide `Main Menu` remains in the header bar, it SHALL stay the outermost end-aligned menu button. When both `Notes` and `Main Menu` are visible in the header bar, the `Notes` menu MUST appear immediately to the left of `Main Menu` and MUST NOT appear to its right.

#### Scenario: Render both Notes and Main Menu in the header bar
- **WHEN** the window shows both the `Notes` secondary menu and the app-wide `Main Menu`
- **THEN** `Main Menu` is the outermost end-aligned header-bar menu
- **AND** `Notes` appears immediately to its left
- **AND** `Notes` does not appear to the right of `Main Menu`

### Requirement: Notes menu groups actions by scope
The system SHALL organize the `Notes` menu into a current-document section and a workspace section. The current-document section MUST contain `Toggle Bookmark`, `Edit Bookmark Label…`, `Add Annotation…`, and `Edit Annotation…`. The workspace section MUST contain `Browse Bookmarks…`, `Browse Annotations…`, and `Export Annotations…`.

#### Scenario: Open the Notes menu for a saved file in a workspace
- **WHEN** the active window has a saved document and a current workspace scope
- **THEN** the `Notes` menu shows the current-document section before the workspace section
- **AND** the current-document section contains only document-scoped bookmark and annotation actions
- **AND** the workspace section contains only workspace-scoped browse and export actions

### Requirement: Notes menu availability follows editor and workspace context
The system SHALL keep the `Notes` menu surface aligned with the current window context. The `Notes` menu button MUST be hidden when the window has neither an active editor nor a current workspace scope. Menu items MUST use sensitivity to reflect actionability: actions that require a saved file MUST be insensitive when the active document has no stable path, cursor-specific edit actions MUST be insensitive when the cursor is not on an eligible bookmark or annotation, and workspace-scoped actions MUST be insensitive when no workspace scope exists.

#### Scenario: Show workspace notes without a saved document
- **WHEN** the active document is untitled and the current window still has a workspace scope
- **THEN** the `Notes` menu button remains visible
- **AND** `Toggle Bookmark`, `Edit Bookmark Label…`, `Add Annotation…`, and `Edit Annotation…` are insensitive
- **AND** `Browse Bookmarks…`, `Browse Annotations…`, and `Export Annotations…` remain actionable

#### Scenario: Disable cursor-specific edit actions when no note is eligible
- **WHEN** the active document is saved but the cursor is not on a bookmarked or annotated location
- **THEN** `Toggle Bookmark` and `Add Annotation…` remain actionable
- **AND** the cursor-specific edit action without an eligible note is insensitive

#### Scenario: Hide the Notes menu when no notes workflow is available
- **WHEN** the window has no active editor and no current workspace scope
- **THEN** the header bar does not show the `Notes` menu button

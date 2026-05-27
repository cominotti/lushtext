# document-notes-menu Specification

## Purpose
TBD - created by archiving change notes-menu-organization. Update Purpose after archive.
## Requirements
### Requirement: Notes workflows use a dedicated secondary menu
The system SHALL expose bookmark workflows together with document-note and workspace-note workflows through a dedicated `Notes` secondary menu in the window header bar. Once the `Notes` menu is available, the primary menu MUST not list bookmark or note commands, and the notes surface MUST not rely on nested submenus. The `Notes` menu MUST act as a concise entry-point surface rather than a complete list of every bookmark and note command.

#### Scenario: Open note workflows from the header bar
- **WHEN** the current window can surface one or more bookmark or note workflows
- **THEN** the header bar shows a dedicated `Notes` menu button
- **AND** opening that menu reveals high-level bookmark and note entry points
- **AND** opening the primary menu does not reveal bookmark or note commands

### Requirement: Notes placement follows the header-bar menu hierarchy
While the app-wide `Main Menu` remains in the header bar, it SHALL stay the outermost end-aligned menu button. When both `Notes` and `Main Menu` are visible in the header bar, the `Notes` menu MUST appear immediately to the left of `Main Menu` and MUST NOT appear to its right.

#### Scenario: Render both Notes and Main Menu in the header bar
- **WHEN** the window shows both the `Notes` secondary menu and the app-wide `Main Menu`
- **THEN** `Main Menu` is the outermost end-aligned header-bar menu
- **AND** `Notes` appears immediately to its left
- **AND** `Notes` does not appear to the right of `Main Menu`

### Requirement: Notes menu groups actions by scope
The system SHALL organize the `Notes` menu into a short set of high-level entry points. The menu MUST contain `Browse Notes…`, a context-sensitive bookmark toggle labeled `Add Bookmark` or `Remove Bookmark`, `Open Document Note…`, and `Open Workspace Note…`. The menu MUST NOT contain separate `Browse Bookmarks…` or `Edit Bookmark Label…` entries.

#### Scenario: Open the Notes menu for a saved file in a concrete workspace
- **WHEN** the active window has a saved document and a concrete current workspace scope
- **THEN** the `Notes` menu shows `Browse Notes…`
- **AND** the menu shows the current-document entry points `Add Bookmark` or `Remove Bookmark` and `Open Document Note…`
- **AND** the menu shows the workspace entry point `Open Workspace Note…`
- **AND** the menu does not show `Browse Bookmarks…` or `Edit Bookmark Label…`

### Requirement: Notes menu availability follows editor and workspace context
The system SHALL keep the `Notes` menu surface aligned with the current window context. The `Notes` menu button MUST be hidden when the window has neither an active editor nor a current workspace scope. Menu items MUST use sensitivity to reflect actionability: actions that require a saved file MUST be insensitive when the active document has no stable path, the bookmark toggle MUST say `Remove Bookmark` when the cursor is on an existing bookmark and `Add Bookmark` otherwise, and `Open Workspace Note…` MUST be insensitive when the current shared scope is `All workspaces`. Workspace-scoped browse actions MUST remain actionable whenever the window still has workspace scope.

#### Scenario: Show workspace note actions without a saved document
- **WHEN** the active document is untitled and the current window has a concrete workspace scope
- **THEN** the `Notes` menu button remains visible
- **AND** the bookmark toggle and `Open Document Note…` are insensitive
- **AND** `Open Workspace Note…` and `Browse Notes…` remain actionable

#### Scenario: Reflect bookmark toggle state in the menu label
- **WHEN** the active document is saved and the cursor is on a line without a bookmark
- **THEN** the bookmark toggle menu item is labeled `Add Bookmark`
- **AND** activating it adds a bookmark for the active line
- **WHEN** the cursor moves onto a bookmarked line
- **THEN** the bookmark toggle menu item is labeled `Remove Bookmark`
- **AND** activating it removes the bookmark for the active line

#### Scenario: Aggregate scope disables the single-workspace note action
- **WHEN** the current shared scope is `All workspaces`
- **THEN** `Open Workspace Note…` is insensitive
- **AND** `Browse Notes…` remains actionable

#### Scenario: Hide the Notes menu when no note workflow is available
- **WHEN** the window has no active editor and no current workspace scope
- **THEN** the header bar does not show the `Notes` menu button

### Requirement: Notes menu popup activation is stable
The system SHALL open a visible `Notes` menu popup when the user activates the visible header-bar `Notes` button. The system MUST NOT rebuild, replace, or clear the menu model during the popup activation path in a way that prevents GTK from showing the popover.

#### Scenario: Click the Notes button opens the menu
- **WHEN** the window context makes the `Notes` menu button visible
- **AND** the user activates the `Notes` menu button
- **THEN** the `Notes` menu popup becomes open
- **AND** the popup exposes the current note entry points

#### Scenario: Dynamic bookmark label does not cancel popup opening
- **WHEN** the active saved document changes the bookmark-toggle label between `Add Bookmark` and `Remove Bookmark`
- **AND** the user activates the `Notes` menu button after that state refresh
- **THEN** the `Notes` menu popup opens normally
- **AND** the bookmark-toggle label reflects the current cursor state

### Requirement: Context menus expose note actions for clear targets
The system SHALL expose note workflows in context menus only when the context identifies a clear target. Contextual note actions MUST reuse the same bookmark, document-note, and workspace-note workflows as the header menu and command palette.

#### Scenario: Open a document note from a file context menu
- **WHEN** the user opens the sidebar context menu for a file inside a workspace
- **THEN** the context menu offers `Open Document Note…`
- **AND** activating it opens that file's document-note surface without changing the source file bytes

#### Scenario: Open a workspace note from a workspace header context menu
- **WHEN** the user opens a workspace header context menu for one concrete workspace
- **THEN** the context menu offers `Open Workspace Note…`
- **AND** activating it opens that workspace root's workspace-note surface

#### Scenario: Edit note-specific data from editor context
- **WHEN** the editor context identifies an existing bookmark at the cursor
- **THEN** the editor context menu offers the bookmark edit action
- **AND** activating that action routes through the existing bookmark-label editor workflow

# document-notes-menu Specification

## Purpose
Define the dedicated Notes menu and contextual note entry points for bookmarks, document notes, and folder notes without crowding the primary app menu.
## Requirements
### Requirement: Notes workflows use a dedicated secondary menu
The system SHALL expose bookmark workflows together with document-note and folder-note workflows through a dedicated `Notes` secondary menu in the window header bar. Once the `Notes` menu is available, the primary menu MUST not list bookmark or note commands, and the notes surface MUST not rely on nested submenus. The `Notes` menu MUST act as a concise entry-point surface rather than a complete list of every bookmark and note command.

#### Scenario: Open note workflows from the header bar
- **WHEN** the current window can surface one or more bookmark, document-note, or folder-note workflows
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
The system SHALL organize the `Notes` menu into a short set of high-level entry points. The menu MUST contain `Browse Notes...`, a context-sensitive bookmark toggle labeled `Add Bookmark` or `Remove Bookmark`, `Open Document Note...`, and `Open Folder Note...`. The menu MUST NOT contain separate bookmark-browse, bookmark-label-edit, or legacy workspace-level note entries.

#### Scenario: Open the Notes menu for a saved file in a concrete one-folder workspace
- **WHEN** the active window has a saved document and a concrete current workspace scope with exactly one folder
- **THEN** the `Notes` menu shows `Browse Notes...`
- **AND** the menu shows the current-document entry points `Add Bookmark` or `Remove Bookmark` and `Open Document Note...`
- **AND** the menu shows the folder entry point `Open Folder Note...`
- **AND** the menu does not show legacy bookmark-browse, bookmark-label-edit, or workspace-level note entries

#### Scenario: Open the Notes menu for a multi-folder workspace
- **WHEN** the active window has a concrete current workspace scope with two or more folders
- **THEN** the `Notes` menu shows `Open Folder Note...`
- **AND** activating it requires a clear folder choice or opens `Browse Notes...` focused to folder notes
- **AND** it does not choose a folder implicitly

### Requirement: Notes menu availability follows editor and workspace context
The system SHALL keep the `Notes` menu surface aligned with the current window context while treating `Browse Notes...` as a window-scoped entry point. When the header bar itself is visible, the `Notes` menu button MUST remain visible even when the window has no active editor and no restored workspace folders. Menu items MUST use sensitivity to reflect actionability: actions that require a saved file MUST be insensitive when the active document has no stable path, the bookmark toggle MUST say `Remove Bookmark` when the cursor is on an existing bookmark and `Add Bookmark` otherwise, and `Open Folder Note...` MUST be insensitive when the current shared scope is `All workspaces` or no concrete workspace folder target is selected. `Browse Notes...` MUST remain actionable from the header menu and MUST open the existing Notes browser, including its explicit empty state when there are no browsable notes or bookmarks.

#### Scenario: Show folder-note actions without a saved document
- **WHEN** the active document is untitled and the current window has a concrete workspace scope with at least one folder
- **THEN** the `Notes` menu button remains visible
- **AND** the bookmark toggle and `Open Document Note...` are insensitive
- **AND** `Open Folder Note...` and `Browse Notes...` remain actionable

#### Scenario: Show Browse Notes after closing the last tab
- **WHEN** the window has restored workspace folders
- **AND** the user closes the last open tab
- **THEN** the header bar still shows the `Notes` menu button
- **AND** `Browse Notes...` remains actionable
- **AND** actions that require an active saved document are insensitive

#### Scenario: Show Browse Notes in an empty no-workspace window
- **WHEN** the window has no active editor
- **AND** no workspace folders are restored
- **THEN** the header bar still shows the `Notes` menu button
- **AND** `Browse Notes...` remains actionable
- **AND** activating `Browse Notes...` opens the Notes browser empty state without creating workspace, bookmark, or document-note data

#### Scenario: Reflect bookmark toggle state in the menu label
- **WHEN** the active document is saved and the cursor is on a line without a bookmark
- **THEN** the bookmark toggle menu item is labeled `Add Bookmark`
- **AND** activating it adds a bookmark for the active line
- **WHEN** the cursor moves onto a bookmarked line
- **THEN** the bookmark toggle menu item is labeled `Remove Bookmark`
- **AND** activating it removes the bookmark for the active line

#### Scenario: Aggregate scope disables the single-folder note action
- **WHEN** the current shared scope is `All workspaces`
- **THEN** `Open Folder Note...` is insensitive
- **AND** `Browse Notes...` remains actionable

#### Scenario: Zero-folder workspace disables folder-note opening
- **WHEN** the current shared scope is a concrete workspace with zero folders
- **THEN** `Open Folder Note...` is insensitive or reports that the workspace has no folders
- **AND** `Browse Notes...` remains actionable and opens the explicit Notes browser empty state when there are no eligible notes or open-tab rows

### Requirement: Empty Notes browser remains readable
The system SHALL present the empty `Browse Notes...` browser as a readable modal browser state. When there are no bookmarks, document notes, folder notes, workspace folders, or open-tab note rows to show, the empty Notes browser MUST keep a stable compact-browser size, MUST allocate enough width for the empty-state title and description to remain legible, MUST allocate enough height for the empty-state content to fit without vertical scrolling, and MUST NOT collapse to the natural width of the empty-state content. The empty browser MUST continue to avoid creating fake sidebar rows, note sidecars, bookmarks, workspaces, or document-note data merely by being opened.

#### Scenario: Open empty Notes browser from a no-workspace window
- **WHEN** the window has no active editor
- **AND** no workspace folders are restored
- **AND** the user activates `Browse Notes...`
- **THEN** the Notes browser opens an empty state titled `No notes yet`
- **AND** the empty-state modal keeps a readable compact-browser allocation
- **AND** the modal does not materialize a Notes sidebar or fake note rows

#### Scenario: Empty Notes browser sizing does not follow status-page collapse
- **WHEN** the empty Notes browser is presented
- **THEN** the dialog uses its intended compact-browser content dimensions rather than following the natural size of the status page
- **AND** the empty-state title and description remain readable within the dialog bounds
- **AND** the empty-state content fits without a vertical scrollbar

#### Scenario: Empty Notes browser preserves dismissal behavior
- **WHEN** the empty Notes browser is visible
- **THEN** the visible close control dismisses the dialog
- **AND** pressing Escape dismisses the dialog

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
The system SHALL expose note workflows in context menus only when the context identifies a clear target. Contextual note actions MUST reuse the same bookmark, document-note, and folder-note workflows as the header menu and command palette. Context menus MUST use folder terminology for folder notes and MUST NOT expose legacy workspace-level note entries.

#### Scenario: Open a document note from a file context menu
- **WHEN** the user opens the sidebar context menu for a file inside a workspace folder
- **THEN** the context menu offers `Open Document Note...`
- **AND** activating it opens that file's document-note surface without changing the source file bytes

#### Scenario: Open a folder note from a folder row context menu
- **WHEN** the user opens a context menu for one top-level workspace folder row
- **THEN** the context menu offers `Open Folder Note...`
- **AND** activating it opens that folder's folder-note surface

#### Scenario: Edit note-specific data from editor context
- **WHEN** the editor context identifies an existing bookmark at the cursor
- **THEN** the editor context menu offers the bookmark edit action
- **AND** activating that action routes through the existing bookmark-label editor workflow

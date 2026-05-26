# workspace-notes Specification

## Purpose
TBD - created by archiving change rich-document-notes. Update Purpose after archive.
## Requirements
### Requirement: Users can create and manage one workspace note for each workspace root
The system SHALL allow users to create, edit, view, and clear one workspace note for a concrete workspace root without requiring an active document. The single-workspace-note workflow MUST target one workspace at a time and MUST NOT guess a target when the current shared scope is `All workspaces`.

#### Scenario: Open or create a workspace note for the selected workspace
- **WHEN** a concrete workspace is the current shared scope and the user invokes `Open Workspace Note…`
- **THEN** the system opens that workspace's workspace-note surface
- **AND** the system creates the workspace note lazily if it did not already exist

#### Scenario: Clear an existing workspace note
- **WHEN** the user clears the workspace note attached to the current workspace
- **THEN** the persisted workspace note for that workspace root is removed
- **AND** reopening that workspace starts without an empty workspace-note payload

#### Scenario: Attempt to open a workspace note from aggregate scope
- **WHEN** the current shared scope is `All workspaces` and the user invokes the single-workspace note workflow
- **THEN** the system does not choose an arbitrary workspace note target
- **AND** the user is directed to select a concrete workspace or use `Browse Notes…`

### Requirement: Workspace notes support edit and rendered markdown reading modes
The system SHALL let users switch a workspace note between editable text mode and a read-only rendered mode based on the stored note text. Switching modes MUST NOT discard in-progress note text.

#### Scenario: Render a workspace note as markdown
- **WHEN** the user opens a workspace note containing markdown syntax and switches to render mode
- **THEN** the system shows a read-only rendered markdown view of the current note text
- **AND** the rendered view does not permit direct editing

#### Scenario: Return from render mode to edit mode
- **WHEN** the user switches a workspace note from edit mode to render mode and back again
- **THEN** the editable note text remains the same
- **AND** the note returns to an editable text surface without losing content

### Requirement: Workspace-note persistence follows workspace-root identity
The system SHALL persist workspace notes under app data using a stable identity derived from the workspace root's canonical path. Renaming a workspace label MUST keep the same workspace note. Renaming the workspace root through LushText's in-app rename workflow MUST migrate the workspace note to the renamed root identity. Removing a workspace MUST NOT delete the workspace note for that root, so re-adding the same root MUST restore the same workspace note. Adding a different root MUST use that different root's own workspace-note identity.

#### Scenario: Renaming a workspace label keeps the same workspace note
- **WHEN** the user renames a workspace label without changing its root directory
- **THEN** the existing workspace note remains attached to that workspace root
- **AND** the note content does not reset

#### Scenario: In-app root rename preserves a workspace note
- **WHEN** the user renames the workspace root directory through LushText's in-app rename workflow
- **THEN** the persisted workspace note is migrated to the renamed root identity
- **AND** reopening that renamed workspace restores the same workspace note

#### Scenario: Remove and re-add the same root restores the same workspace note
- **WHEN** the user removes a workspace that has a workspace note and later adds the same root directory again
- **THEN** the system restores the same workspace note for that root
- **AND** the note does not depend on the old workspace slot identifier

#### Scenario: Adding a different root uses that root's workspace-note identity
- **WHEN** the user removes one workspace and later creates a workspace for a different root directory
- **THEN** the different root uses its own workspace-note identity
- **AND** the previous root keeps its existing workspace note data for a future remove-and-readd flow

### Requirement: Users can browse notes within the current workspace scope
The system SHALL provide a workspace-scoped `Browse Notes…` surface that lists bookmarks, workspace notes, document notes, and range notes that fall inside the current shared workspace scope. In a concrete workspace scope, the browser MUST be limited to that workspace. In `All workspaces`, the browser MUST aggregate bookmarks and notes across restored workspace roots while preserving each item's scope in the list presentation.

#### Scenario: Browse notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes…`
- **THEN** the browser lists that workspace's bookmarks and workspace note together with document and range notes that belong to files inside that workspace root
- **AND** bookmarks and notes from other workspaces are excluded

#### Scenario: Browse notes across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes…`
- **THEN** the browser aggregates bookmarks, workspace notes, document notes, and range notes from every restored workspace root
- **AND** each row preserves enough scope metadata for the user to tell which workspace it belongs to

#### Scenario: Open a workspace note from the notes browser
- **WHEN** the user activates a workspace-note row in `Browse Notes…`
- **THEN** the system opens that workspace note's surface
- **AND** the system does not require an active document tab for that workspace

#### Scenario: Open a bookmark from the notes browser
- **WHEN** the user activates a bookmark row in `Browse Notes…`
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line

### Requirement: Workspace-note browser entries use the native Adwaita sidebar rail
The system SHALL present workspace-note entries in the workspace-scoped `Browse Notes...` surface through an `AdwSidebar` section rather than a hand-built `GtkListBox` rail. The sidebar section MUST preserve the existing workspace-scope filtering, workspace-note preview, preview-only pointer selection, and explicit Open behavior.

#### Scenario: Browse workspace notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more workspace notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those workspace notes in a dedicated `AdwSidebar` section
- **AND** each workspace-note item identifies the workspace/root it belongs to

#### Scenario: Preview a workspace note from the sidebar rail
- **WHEN** the user selects a workspace-note item in the Notes browser sidebar rail
- **THEN** the browser updates the preview pane with that workspace note's rendered Markdown content or explicit empty-note state
- **AND** the Open action targets the selected workspace note

#### Scenario: Click a workspace-note item without opening the editor
- **WHEN** the user clicks a workspace-note item in the Notes browser sidebar rail
- **THEN** the browser updates the selected item and preview pane only
- **AND** the workspace-note editing surface is not opened

#### Scenario: Open a workspace note explicitly from the browser
- **WHEN** the user selects a workspace-note item and invokes the browser's Open action
- **THEN** the system opens that workspace note's editing surface
- **AND** the Open action does not require an active document tab

#### Scenario: Filtered workspace-note state remains explicit
- **WHEN** the Notes browser search text filters out every workspace-note item
- **THEN** the workspace-note sidebar section no longer shows matching items
- **AND** if no notes of any kind match, the browser shows an explicit empty filtered state

### Requirement: Workspace-note editor mode switching is layout-stable
The system SHALL keep the shared workspace-note editor popup visually stable when switching between Edit and Render. The edit and rendered note surfaces MUST keep matching text-origin padding so the same plain note content does not shift horizontally or vertically when changing modes. The popup MUST keep the same outer size with no visible shrink or expansion, including when the note starts empty and the user types before the first Render switch.

#### Scenario: Switch a workspace note from Edit to Render
- **WHEN** the user opens a workspace-note editing popup and switches from Edit to Render
- **THEN** the popup keeps the same outer size
- **AND** the rendered text starts at the same visual origin as the editable text

#### Scenario: Switch a newly typed workspace note from Edit to Render
- **WHEN** the user opens an initially empty workspace-note editing popup
- **AND** the user types note text in Edit mode
- **AND** the user switches to Render for the first time
- **THEN** the popup keeps the same outer size with no visible shrink or expansion
- **AND** the rendered text starts at the same visual origin as the editable text

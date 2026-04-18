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
The system SHALL persist workspace notes under app data using a stable identity derived from the workspace root's canonical path. Renaming a workspace label MUST keep the same workspace note. Renaming the workspace root through LushText's in-app rename workflow MUST migrate the workspace note to the renamed root identity. Replacing a workspace root with a different directory MUST start a fresh workspace-note identity instead of carrying the previous workspace note forward.

#### Scenario: Renaming a workspace label keeps the same workspace note
- **WHEN** the user renames a workspace label without changing its root directory
- **THEN** the existing workspace note remains attached to that workspace root
- **AND** the note content does not reset

#### Scenario: In-app root rename preserves a workspace note
- **WHEN** the user renames the workspace root directory through LushText's in-app rename workflow
- **THEN** the persisted workspace note is migrated to the renamed root identity
- **AND** reopening that renamed workspace restores the same workspace note

#### Scenario: Replace Workspace Root starts a new workspace-note identity
- **WHEN** the user replaces one workspace root with a different directory through `Replace Workspace Root`
- **THEN** the newly selected root starts without a copied workspace note by default
- **AND** the previous root keeps its existing workspace note data

#### Scenario: Unlist and re-add the same root restores the same workspace note
- **WHEN** the user unlists a workspace that has a workspace note and later adds the same root directory again
- **THEN** the system restores the same workspace note for that root
- **AND** the note does not depend on the old workspace slot identifier

### Requirement: Users can browse notes within the current workspace scope
The system SHALL provide a workspace-scoped `Browse Notes…` surface that lists workspace notes together with document notes and range notes that fall inside the current shared workspace scope. In a concrete workspace scope, the browser MUST be limited to that workspace. In `All workspaces`, the browser MUST aggregate notes across restored workspace roots while preserving each note's scope in the list presentation.

#### Scenario: Browse notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes…`
- **THEN** the browser lists that workspace's workspace note together with document and range notes that belong to files inside that workspace root
- **AND** notes from other workspaces are excluded

#### Scenario: Browse notes across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes…`
- **THEN** the browser aggregates workspace, document, and range notes from every restored workspace root
- **AND** each row preserves enough scope metadata for the user to tell which workspace it belongs to

#### Scenario: Open a workspace note from the notes browser
- **WHEN** the user activates a workspace-note row in `Browse Notes…`
- **THEN** the system opens that workspace note's surface
- **AND** the system does not require an active document tab for that workspace


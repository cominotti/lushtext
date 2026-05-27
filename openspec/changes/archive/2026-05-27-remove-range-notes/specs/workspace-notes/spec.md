## MODIFIED Requirements

### Requirement: Users can browse notes within the current workspace scope
The system SHALL provide a workspace-scoped `Browse Notes...` surface that lists bookmarks, workspace notes, and document notes that fall inside the current shared workspace scope. In a concrete workspace scope, the browser MUST be limited to that workspace. In `All workspaces`, the browser MUST aggregate bookmarks and notes across restored workspace roots while preserving each item's scope in the list presentation.

#### Scenario: Browse notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes...`
- **THEN** the browser lists that workspace's bookmarks and workspace note together with document notes that belong to files inside that workspace root
- **AND** bookmarks and notes from other workspaces are excluded

#### Scenario: Browse notes across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes...`
- **THEN** the browser aggregates bookmarks, workspace notes, and document notes from every restored workspace root
- **AND** each row preserves enough scope metadata for the user to tell which workspace it belongs to

#### Scenario: Open a workspace note from the notes browser
- **WHEN** the user activates a workspace-note row in `Browse Notes...`
- **THEN** the system opens that workspace note's surface
- **AND** the system does not require an active document tab for that workspace

#### Scenario: Open a bookmark from the notes browser
- **WHEN** the user activates a bookmark row in `Browse Notes...`
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line

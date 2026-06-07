## MODIFIED Requirements

### Requirement: Document notes appear in the workspace-scoped notes browser
The system SHALL include document notes in `Browse Notes...` whenever their saved file falls inside the current shared workspace scope, and SHALL also include existing document notes attached to saved open tabs outside that current scope. Workspace-scoped document-note entries MUST appear in the dedicated `Document Notes` section. Open-tab document-note entries outside the current workspace scope MUST appear in the dedicated `Open Tabs` section, identify themselves as open-tab rows, and MUST NOT be represented as belonging to a fake workspace. Opening a document-note browser entry MUST focus that file and open its document-note surface.

#### Scenario: Browse document notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes...`
- **THEN** the browser lists document notes only for saved files inside that workspace root in the `Document Notes` section
- **AND** closed-file document notes outside that workspace are excluded
- **AND** existing document notes attached to saved open tabs outside that workspace appear only in the `Open Tabs` section

#### Scenario: Browse document notes across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes...`
- **THEN** the browser lists document notes for saved files inside every restored workspace root in the `Document Notes` section
- **AND** each workspace document-note row preserves enough workspace and file metadata for the user to tell where it belongs
- **AND** existing document notes attached to saved open tabs outside every restored workspace root appear only in the `Open Tabs` section

#### Scenario: Browse open-tab document notes without a workspace
- **WHEN** no workspace roots are restored
- **AND** a saved open tab has an existing document note
- **AND** the user opens `Browse Notes...`
- **THEN** the browser lists that document note in the `Open Tabs` section
- **AND** the document-note row identifies the saved file path without requiring workspace metadata

#### Scenario: Open a document note from the notes browser
- **WHEN** the user activates a document-note row in `Browse Notes...`
- **THEN** the system opens or focuses the associated file tab
- **AND** the system opens that file's document-note surface

#### Scenario: Search open-tab document notes
- **WHEN** the user searches in `Browse Notes...`
- **THEN** open-tab document-note rows match by title, saved file metadata, open-tab source metadata, or note body text
- **AND** non-matching document-note rows are hidden without changing persisted document-note data

## MODIFIED Requirements

### Requirement: Document notes appear in the workspace-scoped notes browser
The system SHALL include document notes in `Browse Notes...` whenever their saved file falls inside the current shared workspace scope's folder coverage, and SHALL also include existing document notes attached to saved open tabs outside that current scope. Workspace-scoped document-note entries MUST appear in the dedicated `Document Notes` section. Open-tab document-note entries outside the current workspace scope MUST appear in the dedicated `Open Tabs` section, identify themselves as open-tab rows, and MUST NOT be represented as belonging to a fake workspace. When overlapping folders cover the same saved file, the document-note browser MUST show that document note only once, using workspace order and folder order to choose the primary context shown for the row. Opening a document-note browser entry MUST focus that file and open its document-note surface.

#### Scenario: Browse document notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes...`
- **THEN** the browser lists document notes only for saved files covered by that workspace's folder set in the `Document Notes` section
- **AND** closed-file document notes outside that workspace's folder set are excluded
- **AND** existing document notes attached to saved open tabs outside that workspace appear only in the `Open Tabs` section

#### Scenario: Browse document notes across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes...`
- **THEN** the browser lists document notes for saved files covered by every restored workspace folder in the `Document Notes` section
- **AND** each workspace document-note row preserves enough workspace, primary folder, and file metadata for the user to tell where it belongs
- **AND** existing document notes attached to saved open tabs outside every restored workspace folder appear only in the `Open Tabs` section

#### Scenario: Overlapping folders do not duplicate a document note
- **WHEN** the selected workspace contains folders `/repo` and `/repo/src`
- **AND** `/repo/src/main.rs` has a document note
- **AND** the user opens `Browse Notes...`
- **THEN** the `Document Notes` section shows one row for `/repo/src/main.rs`
- **AND** the row uses the earliest covering folder by workspace folder order as its primary context

#### Scenario: Browse open-tab document notes without a workspace folder
- **WHEN** no workspace folders are restored
- **AND** a saved open tab has an existing document note
- **AND** the user opens `Browse Notes...`
- **THEN** the browser lists that document note in the `Open Tabs` section
- **AND** the document-note row identifies the saved file path without requiring workspace or folder metadata

#### Scenario: Open a document note from the notes browser
- **WHEN** the user activates a document-note row in `Browse Notes...`
- **THEN** the system opens or focuses the associated file tab
- **AND** the system opens that file's document-note surface

#### Scenario: Search open-tab document notes
- **WHEN** the user searches in `Browse Notes...`
- **THEN** open-tab document-note rows match by title, saved file metadata, open-tab source metadata, or note body text
- **AND** non-matching document-note rows are hidden without changing persisted document-note data

### Requirement: Document-note browser entries use the native Adwaita sidebar rail
The system SHALL present document-note entries in the workspace-scoped `Browse Notes...` surface through an `AdwSidebar` section rather than a hand-built `GtkListBox` rail. The sidebar section MUST preserve the existing workspace-scope filtering, document-note Markdown preview, preview-only pointer selection, explicit Open behavior, folder-set coverage, and overlapping-folder de-duplication.

#### Scenario: Browse document notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more document notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those document notes in a dedicated `AdwSidebar` section
- **AND** each document-note item identifies the saved file, workspace, and primary covering folder it belongs to

#### Scenario: Preview a document note from the sidebar rail
- **WHEN** the user selects a document-note item in the Notes browser sidebar rail
- **THEN** the browser updates the preview pane with that document note's rendered Markdown content or explicit empty-note state
- **AND** the Open action targets the selected document note

#### Scenario: Click a document-note item without opening the editor
- **WHEN** the user clicks a document-note item in the Notes browser sidebar rail
- **THEN** the browser updates the selected item and preview pane only
- **AND** the document-note editing surface is not opened

#### Scenario: Open a document note explicitly from the browser
- **WHEN** the user selects a document-note item and invokes the browser's Open action
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the system opens that file's document-note surface

#### Scenario: Document-note search keeps file and body matching
- **WHEN** the user searches in the Notes browser
- **THEN** document-note sidebar items match by title, saved file metadata, workspace metadata, primary folder metadata, or note body text
- **AND** non-matching document-note items are hidden without changing the stored note data

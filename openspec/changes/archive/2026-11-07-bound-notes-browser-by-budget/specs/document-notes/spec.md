## RENAMED Requirements

- FROM: `### Requirement: Document-note browser entries use the native Adwaita sidebar rail`
- TO: `### Requirement: Document-note browser entries use the virtualized sidebar rail`

## MODIFIED Requirements

### Requirement: Document-note browser entries use the virtualized sidebar rail
The system SHALL present document-note entries in the workspace-scoped `Browse Notes...` surface in a dedicated `Document Notes` section of the browser's virtualized sidebar rail: a `GtkListView` styled `navigation-sidebar` whose section titles come from a header factory and whose row widgets are created only for rows on or near the screen. The rail MUST NOT build one widget per off-screen row. The section MUST preserve the existing workspace-scope filtering, document-note Markdown preview, preview-only pointer selection, explicit Open behavior, folder-set coverage, and overlapping-folder de-duplication.

#### Scenario: Browse document notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more document notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those document notes in a dedicated `Document Notes` section of the sidebar rail
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

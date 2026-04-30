## ADDED Requirements

### Requirement: Document-note browser entries use the native Adwaita sidebar rail
The system SHALL present document-note entries in the workspace-scoped `Browse Notes...` surface through an `AdwSidebar` section rather than a hand-built `GtkListBox` rail. The sidebar section MUST preserve the existing workspace-scope filtering, document-note Markdown preview, preview-only pointer selection, and explicit Open behavior.

#### Scenario: Browse document notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more document notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those document notes in a dedicated `AdwSidebar` section
- **AND** each document-note item identifies the saved file and workspace it belongs to

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
- **THEN** document-note sidebar items match by title, saved file metadata, workspace metadata, or note body text
- **AND** non-matching document-note items are hidden without changing the stored note data

### Requirement: Document-note editor mode switching is layout-stable
The system SHALL keep the document-note editing popup visually stable when switching between Edit and Render. The edit and rendered note surfaces MUST keep matching text-origin padding so the same plain note content does not shift horizontally or vertically when changing modes.

#### Scenario: Switch a document note from Edit to Render
- **WHEN** the user opens a document-note editing popup and switches from Edit to Render
- **THEN** the popup keeps the same outer size
- **AND** the rendered text starts at the same visual origin as the editable text

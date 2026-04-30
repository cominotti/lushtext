# document-notes Specification

## Purpose
TBD - created by archiving change rich-document-notes. Update Purpose after archive.
## Requirements
### Requirement: Users can create and manage one document note for each saved document
The system SHALL allow users to create, edit, view, and clear one document note for a saved file without modifying the underlying file text. Document notes MUST require a stable saved path and MUST NOT be created for untitled buffers.

#### Scenario: Open or create a document note for a saved file
- **WHEN** the active editor is a saved document and the user invokes `Open Document Note…`
- **THEN** the system opens that file's document-note surface
- **AND** the system creates the document note lazily if it did not already exist
- **AND** the source file bytes remain unchanged

#### Scenario: Clear an existing document note
- **WHEN** the user clears the document note attached to a saved file
- **THEN** the persisted document note for that file is removed
- **AND** reopening the file does not restore an empty document note payload

#### Scenario: Attempt to open a document note for an untitled buffer
- **WHEN** the active editor has no stable file path and the user invokes the document-note workflow
- **THEN** the system does not create a document note
- **AND** the user receives feedback that document notes require a saved file

### Requirement: Document notes support edit and rendered markdown reading modes
The system SHALL let users switch a document note between editable text mode and a read-only rendered mode based on the stored note text. Switching modes MUST NOT discard in-progress note text.

#### Scenario: Render a document note as markdown
- **WHEN** the user opens a document note containing markdown syntax and switches to render mode
- **THEN** the system shows a read-only rendered markdown view of the current note text
- **AND** the rendered view does not permit direct editing

#### Scenario: Return from render mode to edit mode
- **WHEN** the user switches a document note from edit mode to render mode and back again
- **THEN** the editable note text remains the same
- **AND** the note returns to an editable text surface without losing content

### Requirement: Document note persistence follows saved-document identity
The system SHALL persist document notes under app data using the same saved-document identity rules as other file-backed note sidecars. In-app file or directory renames MUST migrate the document note to the renamed identity, and Save As MUST start a fresh document-note identity without automatically copying the original note.

#### Scenario: Reopen a saved file with an existing document note
- **WHEN** the user closes LushText and later reopens a saved file that already had a document note
- **THEN** the system restores the document note for that file
- **AND** the note content is available without modifying the source file

#### Scenario: In-app rename preserves a document note
- **WHEN** the user renames a saved document or one of its ancestor directories through LushText's in-app rename workflow
- **THEN** the persisted document note is migrated to the renamed identity
- **AND** reopening the renamed file restores the same document note

#### Scenario: Save As starts a new document-note identity
- **WHEN** the user saves a file with an existing document note to a new path through Save As
- **THEN** the new saved file starts without a copied document note by default
- **AND** the original saved file keeps its existing document note

### Requirement: Document notes appear in the workspace-scoped notes browser
The system SHALL include document notes in the workspace-scoped `Browse Notes…` surface whenever their saved file falls inside the current shared workspace scope. Opening a document-note browser entry MUST focus that file and open its document-note surface.

#### Scenario: Browse document notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes…`
- **THEN** the browser lists document notes only for saved files inside that workspace root
- **AND** document notes for files outside that workspace are excluded

#### Scenario: Open a document note from the notes browser
- **WHEN** the user activates a document-note row in `Browse Notes…`
- **THEN** the system opens or focuses the associated file tab
- **AND** the system opens that file's document-note surface

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

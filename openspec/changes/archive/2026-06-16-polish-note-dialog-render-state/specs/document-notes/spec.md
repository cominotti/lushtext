## MODIFIED Requirements

### Requirement: Document notes support edit and rendered markdown reading modes
The system SHALL let users switch a document note between editable text mode and a read-only rendered mode based on the stored note text. Switching modes MUST NOT discard in-progress note text. When a document note opens with non-empty, non-whitespace loaded text, the dialog MUST select Render initially. When the loaded document note is missing, empty, or whitespace-only, the dialog MUST select Edit initially. The Save action MUST remain visible and MUST be enabled only when the normalized current note text is non-empty and differs from the normalized loaded note text.

#### Scenario: Open an existing document note for reading
- **WHEN** the user opens a saved file's document note that contains non-whitespace text
- **THEN** the document-note dialog opens with Render selected
- **AND** the rendered note view is read-only
- **AND** Save is visible but disabled

#### Scenario: Open a missing or empty document note for writing
- **WHEN** the user opens a document-note workflow for a saved file with no meaningful saved document-note text
- **THEN** the document-note dialog opens with Edit selected
- **AND** Save is visible but disabled until the user enters meaningful note text

#### Scenario: Render a document note as markdown
- **WHEN** the user opens a document note containing markdown syntax and switches to render mode
- **THEN** the system shows a read-only rendered markdown view of the current note text
- **AND** the rendered view does not permit direct editing

#### Scenario: Return from render mode to edit mode
- **WHEN** the user switches a document note from edit mode to render mode and back again
- **THEN** the editable note text remains the same
- **AND** the note returns to an editable text surface without losing content

#### Scenario: Enable Save after a meaningful document-note edit
- **WHEN** a document-note dialog is open
- **AND** the user changes the note text so the normalized current text is non-empty and differs from the loaded text
- **THEN** Save becomes enabled
- **AND** Save remains enabled if the user switches to Render before saving

#### Scenario: Disable Save after reverting a document-note edit
- **WHEN** a document-note dialog has unsaved edits
- **AND** the user changes the note text back to the normalized loaded text
- **THEN** Save becomes disabled

#### Scenario: Keep Save disabled for whitespace-only document-note text
- **WHEN** a document-note dialog is open
- **AND** the current note text contains only whitespace
- **THEN** Save is disabled

#### Scenario: Save document-note edits after reviewing Render
- **WHEN** the user edits a document note, switches to Render, and activates Save
- **THEN** the system persists the current note text
- **AND** the source file bytes remain unchanged

## MODIFIED Requirements

### Requirement: Folder notes support edit and rendered markdown reading modes
The system SHALL let users switch a folder note between editable text mode and a read-only rendered mode based on the stored note text. Switching modes MUST NOT discard in-progress note text. When a folder note opens with non-empty, non-whitespace loaded text, the dialog MUST select Render initially. When the loaded folder note is missing, empty, or whitespace-only, the dialog MUST select Edit initially. The Save action MUST remain visible and MUST be enabled only when the normalized current note text is non-empty and differs from the normalized loaded note text.

#### Scenario: Open an existing folder note for reading
- **WHEN** the user opens a folder note that contains non-whitespace text
- **THEN** the folder-note dialog opens with Render selected
- **AND** the rendered note view is read-only
- **AND** Save is visible but disabled

#### Scenario: Open a missing or empty folder note for writing
- **WHEN** the user opens a folder-note workflow for a folder with no meaningful saved folder-note text
- **THEN** the folder-note dialog opens with Edit selected
- **AND** Save is visible but disabled until the user enters meaningful note text

#### Scenario: Render a folder note as markdown
- **WHEN** the user opens a folder note containing markdown syntax and switches to render mode
- **THEN** the system shows a read-only rendered markdown view of the current note text
- **AND** the rendered view does not permit direct editing

#### Scenario: Return from render mode to edit mode
- **WHEN** the user switches a folder note from edit mode to render mode and back again
- **THEN** the editable note text remains the same
- **AND** the note returns to an editable text surface without losing content

#### Scenario: Enable Save after a meaningful folder-note edit
- **WHEN** a folder-note dialog is open
- **AND** the user changes the note text so the normalized current text is non-empty and differs from the loaded text
- **THEN** Save becomes enabled
- **AND** Save remains enabled if the user switches to Render before saving

#### Scenario: Disable Save after reverting a folder-note edit
- **WHEN** a folder-note dialog has unsaved edits
- **AND** the user changes the note text back to the normalized loaded text
- **THEN** Save becomes disabled

#### Scenario: Keep Save disabled for whitespace-only folder-note text
- **WHEN** a folder-note dialog is open
- **AND** the current note text contains only whitespace
- **THEN** Save is disabled

#### Scenario: Save folder-note edits after reviewing Render
- **WHEN** the user edits a folder note, switches to Render, and activates Save
- **THEN** the system persists the current note text
- **AND** the folder's source files remain unchanged

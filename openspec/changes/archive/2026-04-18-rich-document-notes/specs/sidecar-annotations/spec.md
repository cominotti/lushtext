## MODIFIED Requirements

### Requirement: Users can create and manage sidecar annotations on saved files
The system SHALL allow users to create, edit, render, and delete saved-file range notes on one or more lines of a file-backed document without modifying the underlying file text. Each annotation MUST store user-entered note text and a presentation style that can be shown consistently in range-note UI. The range-note surface MUST provide an editable text mode and a read-only rendered markdown mode based on the stored note text.

#### Scenario: Create an annotation for a selected line range
- **WHEN** the user selects one or more lines in a saved file and creates an annotation
- **THEN** the system stores an annotation for that file and line range
- **AND** the editor shows an annotation indicator for that range
- **AND** the document text remains unchanged

#### Scenario: Switch an existing range note between edit and render modes
- **WHEN** the user opens an existing annotation and switches between edit mode and render mode
- **THEN** render mode shows a read-only rendered markdown view of the current note text
- **AND** returning to edit mode preserves the note text

#### Scenario: Edit or delete an existing annotation
- **WHEN** the user opens an existing annotation from its indicator or list entry
- **THEN** the user can update the annotation text or presentation style
- **AND** the user can delete the annotation entirely

#### Scenario: Attempt to annotate an untitled document
- **WHEN** the user tries to create an annotation in a document that has not yet been saved to disk
- **THEN** the system does not create the annotation
- **AND** the user receives feedback that annotations require a saved file

### Requirement: Users can export annotations for review and handoff
The system SHALL export saved-file range notes for the current workspace into a markdown document grouped by file, including each note's line range, note text, and a short source excerpt.

#### Scenario: Export workspace range notes
- **WHEN** the user runs the range-note export workflow for the current workspace
- **THEN** the system creates a markdown document containing the workspace's saved-file range notes grouped by file path
- **AND** each exported note includes its line range, saved note text, and surrounding context

## ADDED Requirements

### Requirement: Range notes appear in the workspace-scoped notes browser
The system SHALL include saved-file range notes in the workspace-scoped `Browse Notes…` surface. Range-note rows MUST identify the associated file and line range, and activating a row MUST focus that file and reopen the selected range note.

#### Scenario: Browse range notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes…`
- **THEN** the browser lists range notes only for files inside that workspace root
- **AND** each range-note row identifies the file and annotated line range

#### Scenario: Open a range note from the notes browser
- **WHEN** the user activates a range-note row in `Browse Notes…`
- **THEN** the system opens or focuses the associated file tab
- **AND** the system focuses the selected range note in that file


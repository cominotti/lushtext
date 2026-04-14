# sidecar-annotations Specification

## Purpose
Let users attach persistent sidecar annotations to saved-file line ranges without modifying the source file bytes, while keeping those notes editable, restorable, and exportable.
## Requirements
### Requirement: Users can create and manage sidecar annotations on saved files
The system SHALL allow users to create, edit, and delete annotations on one or more lines of a file-backed document without modifying the underlying file text. Each annotation MUST store user-entered note text and a presentation style that can be shown consistently in annotation UI.

#### Scenario: Create an annotation for a selected line range
- **WHEN** the user selects one or more lines in a saved file and creates an annotation
- **THEN** the system stores an annotation for that file and line range
- **AND** the editor shows an annotation indicator for that range
- **AND** the document text remains unchanged

#### Scenario: Edit or delete an existing annotation
- **WHEN** the user opens an existing annotation from its indicator or list entry
- **THEN** the user can update the annotation text or presentation style
- **AND** the user can delete the annotation entirely

#### Scenario: Attempt to annotate an untitled document
- **WHEN** the user tries to create an annotation in a document that has not yet been saved to disk
- **THEN** the system does not create the annotation
- **AND** the user receives feedback that annotations require a saved file

### Requirement: Annotations persist independently from source file content
The system SHALL persist annotations in sidecar data outside the edited file and SHALL restore them when the file is reopened in a later session.

#### Scenario: Reopen a file with saved annotations
- **WHEN** the user closes LushText and later reopens a file that already had saved annotations
- **THEN** the system restores the annotations for that file
- **AND** the annotation indicators and note content are available again

#### Scenario: Create an annotation without changing the file bytes
- **WHEN** the user creates or edits annotations for a file
- **THEN** the source file content on disk is not modified by the annotation workflow
- **AND** only sidecar metadata changes are persisted

### Requirement: Annotation anchors track edits while a file is open
The system SHALL keep annotation ranges aligned with normal in-editor line insertions and deletions while the annotated file remains open. If an edit removes the entire annotated range, the system MUST remove that annotation from the active document state.

#### Scenario: Insert lines above an annotated range
- **WHEN** the user inserts new lines above an existing annotation while the file remains open
- **THEN** the system shifts the annotation range downward to stay attached to the same logical content

#### Scenario: Delete the entire annotated range
- **WHEN** the user deletes every line covered by an annotation while the file remains open
- **THEN** the system removes the annotation from the document's active annotation set
- **AND** the annotation no longer appears in the gutter or annotation list

### Requirement: Users can export annotations for review and handoff
The system SHALL export annotations for the current workspace into a markdown document grouped by file, including each annotation's line range, note text, and a short source excerpt.

#### Scenario: Export workspace annotations
- **WHEN** the user runs the export-annotations workflow for the current workspace
- **THEN** the system creates a markdown document containing the workspace's annotations grouped by file path
- **AND** each exported annotation includes its line range, saved note text, and surrounding context

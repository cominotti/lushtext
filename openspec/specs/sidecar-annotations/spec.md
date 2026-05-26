# sidecar-annotations Specification

## Purpose
Let users attach persistent sidecar annotations to saved-file line ranges without modifying the source file bytes, while keeping those notes editable, restorable, and exportable.
## Requirements
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
The system SHALL export saved-file range notes for the current workspace into a markdown document grouped by file, including each note's line range, note text, and a short source excerpt.

#### Scenario: Export workspace range notes
- **WHEN** the user runs the range-note export workflow for the current workspace
- **THEN** the system creates a markdown document containing the workspace's saved-file range notes grouped by file path
- **AND** each exported note includes its line range, saved note text, and surrounding context

### Requirement: Annotation identity follows in-app renames and resets on Save As
The system SHALL key persisted annotation sidecars by a saved-document identity derived from the document's canonical path under `$XDG_DATA_HOME/lushtext/annotations/`. When a saved document or its parent path is renamed through LushText's in-app rename workflow, the system MUST migrate the existing annotation sidecar to the renamed identity. When a document is saved through Save As, the new path MUST start with a fresh annotation identity instead of inheriting the original annotation set automatically.

#### Scenario: In-app rename preserves annotation sidecars
- **WHEN** the user renames a saved annotated document or one of its ancestor directories through the LushText sidebar workflow
- **THEN** the persisted annotation sidecar is migrated to the renamed identity
- **AND** reopening the renamed file restores the same annotations

#### Scenario: Save As starts a new annotation identity
- **WHEN** the user saves an annotated document to a new path through Save As
- **THEN** the new saved document starts without copied annotations by default
- **AND** the original document keeps its existing annotation sidecar

### Requirement: Empty annotation state removes its sidecar file
The system SHALL remove an annotation sidecar file when a document no longer has any persisted annotations, instead of leaving an empty annotation sidecar behind indefinitely.

#### Scenario: Removing the final annotation deletes the annotation sidecar
- **WHEN** the user removes the last remaining annotation for a saved document
- **THEN** the persisted annotation sidecar for that document is deleted from the app data directory
- **AND** reopening the document no longer restores an empty annotation sidecar payload

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

### Requirement: Range-note browser entries use the native Adwaita sidebar rail
The system SHALL present saved-file range-note entries in the workspace-scoped `Browse Notes...` surface through an `AdwSidebar` section rather than a hand-built `GtkListBox` rail. The sidebar section MUST preserve the existing workspace-scope filtering, range-note Markdown preview, file/line metadata, preview-only pointer selection, and explicit Open behavior.

#### Scenario: Browse range notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more saved-file range notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those range notes in a dedicated `AdwSidebar` section
- **AND** each range-note item identifies the saved file, workspace, presentation style, and annotated line range

#### Scenario: Preview a range note from the sidebar rail
- **WHEN** the user selects a range-note item in the Notes browser sidebar rail
- **THEN** the browser updates the preview pane with that range note's rendered Markdown content or explicit empty-note state
- **AND** the Open action targets the selected range note

#### Scenario: Click a range-note item without opening the editor
- **WHEN** the user clicks a range-note item in the Notes browser sidebar rail
- **THEN** the browser updates the selected item and preview pane only
- **AND** the range-note editing surface is not opened

#### Scenario: Open a range note explicitly from the browser
- **WHEN** the user selects a range-note item and invokes the browser's Open action
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the system focuses and opens the selected range note in that file

#### Scenario: Range-note search keeps annotation metadata matching
- **WHEN** the user searches in the Notes browser
- **THEN** range-note sidebar items match by title, saved file metadata, workspace metadata, line-range metadata, or note body text
- **AND** non-matching range-note items are hidden without changing persisted annotation data

### Requirement: Range-note editor mode switching is layout-stable
The system SHALL keep the range-note editing popup visually stable when switching between Edit and Render. The edit and rendered note surfaces MUST keep matching text-origin padding so the same plain note content does not shift horizontally or vertically when changing modes. The popup MUST keep the same outer size with no visible shrink or expansion, including when the note starts empty and the user types before the first Render switch.

#### Scenario: Switch a range note from Edit to Render
- **WHEN** the user opens a range-note editing popup and switches from Edit to Render
- **THEN** the popup keeps the same outer size
- **AND** the rendered text starts at the same visual origin as the editable text

#### Scenario: Switch a newly typed range note from Edit to Render
- **WHEN** the user opens an initially empty range-note editing popup
- **AND** the user types note text in Edit mode
- **AND** the user switches to Render for the first time
- **THEN** the popup keeps the same outer size with no visible shrink or expansion
- **AND** the rendered text starts at the same visual origin as the editable text

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

### Requirement: Document-note sidecars use the public v1 JSON envelope
The system SHALL persist document-note sidecars as supported v1 app-owned JSON envelopes under `$XDG_DATA_HOME/lushtext/document-notes/`. Runtime loading MUST require the document-note sidecar kind and supported version before reading the note payload.

#### Scenario: Save document note as v1
- **WHEN** a saved document's document note is persisted
- **THEN** the document-note sidecar is written as a pretty JSON envelope with the document-note document kind
- **AND** the payload stores the document identity and rich note body

#### Scenario: Unsupported document-note sidecar is isolated
- **WHEN** a document-note sidecar is bare pre-public JSON, wrong-kind JSON, unsupported-version JSON, or malformed JSON
- **THEN** that sidecar is preserved through recovery diagnostics before replacement is allowed
- **AND** unrelated valid document notes continue to load

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
The system SHALL keep the document-note editing popup visually stable when switching between Edit and Render. The edit and rendered note surfaces MUST keep matching text-origin padding so the same plain note content does not shift horizontally or vertically when changing modes. The popup MUST keep the same outer size with no visible shrink or expansion, including when the note starts empty and the user types before the first Render switch.

#### Scenario: Switch a document note from Edit to Render
- **WHEN** the user opens a document-note editing popup and switches from Edit to Render
- **THEN** the popup keeps the same outer size
- **AND** the rendered text starts at the same visual origin as the editable text

#### Scenario: Switch a newly typed document note from Edit to Render
- **WHEN** the user opens an initially empty document-note editing popup
- **AND** the user types note text in Edit mode
- **AND** the user switches to Render for the first time
- **THEN** the popup keeps the same outer size with no visible shrink or expansion
- **AND** the rendered text starts at the same visual origin as the editable text

### Requirement: Document-note sidecar corruption is isolated and diagnostic
The system SHALL isolate malformed document-note sidecars from valid document-note state. A malformed document-note sidecar MUST be preserved when possible, reported through recovery diagnostics, and excluded from normal note restoration until repaired or replaced.

#### Scenario: Malformed document note does not block unrelated notes
- **WHEN** one document-note sidecar cannot be parsed during notes browser listing
- **THEN** valid document notes continue to load and appear in the notes browser
- **AND** the malformed sidecar is reported as a recovery diagnostic

#### Scenario: Opening a file with corrupt document note keeps file usable
- **WHEN** a saved file is opened and its document-note sidecar is malformed
- **THEN** the file opens normally
- **AND** the document-note workflow reports that the saved note could not be loaded

#### Scenario: Replacement preserves corrupt note evidence
- **WHEN** the user saves a new document note for an identity whose previous note sidecar was malformed
- **THEN** the malformed sidecar is quarantined or otherwise preserved before replacement

### Requirement: Document-note migrations are retryable after in-app renames
The system SHALL record pending document-note sidecar migrations before or as part of the post-rename sidecar migration workflow. If migration or cleanup fails, the pending state MUST survive restart and be retried during startup reconciliation.

#### Scenario: Pending document-note migration survives restart
- **WHEN** an in-app rename succeeds but document-note migration fails before completion
- **THEN** a pending migration record remains in app data
- **AND** restarting LushText retries the document-note migration

#### Scenario: Completed document-note migration clears pending state
- **WHEN** document-note migration succeeds and obsolete sidecars are cleaned up or safely reconciled
- **THEN** the pending document-note migration record is removed durably

#### Scenario: Migration failure warns without losing note text
- **WHEN** document-note migration fails after the source file rename succeeded
- **THEN** the user receives warning feedback
- **AND** the existing note sidecar remains preserved for retry or inspection

### Requirement: Document-note reconciliation preserves the newest durable note body
The system SHALL reconcile duplicate old and new document-note sidecars conservatively. It MUST preserve the newest durable note body when timestamps or deterministic identity evidence make that choice safe, and MUST preserve evidence instead of guessing when the conflict is ambiguous.

#### Scenario: Duplicate document notes choose deterministic newest body
- **WHEN** old and new document-note sidecars both exist and one can be identified as the newer durable save
- **THEN** the newer note body is kept for the migrated identity
- **AND** the older copy is removed only after the target note is durably written

#### Scenario: Ambiguous document-note conflict is preserved
- **WHEN** duplicate document notes conflict and the newest body cannot be determined safely
- **THEN** the system does not discard either note body silently
- **AND** it reports that automatic document-note reconciliation was incomplete

#### Scenario: Notes browser reports partial note recovery
- **WHEN** the notes browser omits or quarantines a malformed document note
- **THEN** it still displays valid notes
- **AND** it exposes a warning that some note data could not be loaded

### Requirement: Document-note reliability has layered automated coverage
The project SHALL add deterministic service, integration, and widget coverage for document-note sidecar corruption, retryable migrations, duplicate reconciliation, and partial notes-browser behavior.

#### Scenario: Service tests cover corrupt document-note sidecars
- **WHEN** service tests load malformed document-note sidecar bytes
- **THEN** the result preserves or quarantines the sidecar and returns recovery diagnostics
- **AND** unrelated valid document notes still load

#### Scenario: Migration tests cover document-note retry state
- **WHEN** tests simulate a document rename whose document-note migration fails after the source rename
- **THEN** a pending migration record survives restart
- **AND** a later successful retry removes the record durably

#### Scenario: Widget tests cover partial notes browsing
- **WHEN** the notes browser sees one corrupt document note and at least one valid note
- **THEN** the valid notes remain browsable
- **AND** visible partial-recovery feedback is shown

# local-history Specification

## Purpose
TBD - created by archiving change session-time-travel. Update Purpose after archive.
## Requirements
### Requirement: The system captures local-history snapshots for saved documents automatically
The system SHALL capture local-history snapshots for saved, file-backed documents without blocking the GTK main thread. The system MUST record a baseline snapshot when a clean saved document first becomes modified, MUST record additional snapshots no more than once every five minutes while that document remains modified, and MUST record a snapshot after each successful save. The system MUST skip writing a new snapshot when the candidate content is identical to the most recent snapshot already stored for that document.

#### Scenario: Baseline snapshot on first dirty transition
- **WHEN** a saved document with no unsaved changes becomes modified for the first time in an editing cycle
- **THEN** the system records a local-history snapshot of the document state that existed immediately before those unsaved edits

#### Scenario: Periodic snapshot during a long unsaved edit session
- **WHEN** a saved document remains modified and at least five minutes have elapsed since its last local-history snapshot
- **THEN** the system records a new local-history snapshot in the background

#### Scenario: Deduplicated snapshot candidate
- **WHEN** the system reaches a local-history capture boundary but the candidate content is identical to the newest stored snapshot for that document
- **THEN** the system does not create a duplicate snapshot

#### Scenario: Post-save snapshot
- **WHEN** the user successfully saves a saved document
- **THEN** the system records a local-history snapshot representing the saved content

### Requirement: Users can browse local history for the active saved document
The system SHALL provide a deliberate browse action for local history on the
active saved document. Opening local history MUST present an adaptive
GTK-native browser that shows snapshots in newest-first order together with a
read-only preview of the currently selected snapshot. On windows wide enough to
show both areas side by side, the browser MUST open as a large, viewer-first
dialog that occupies most of the parent window while remaining smaller than the
parent window, and the preview MUST receive the majority of the side-by-side
width. The browser MUST distinguish between "no snapshots yet", "empty
historical snapshot", and "preview could not be loaded" states. When the
selected snapshot contains no text, the browser MUST show an explicit
empty-snapshot explanation instead of an ambiguous blank preview, and the
snapshot metadata MUST describe the state semantically rather than relying only
on a raw `0 B` size. Empty snapshots remain valid history and MUST still be
restorable. The browser MUST avoid surfacing fresh baseline entries that exist
only because a file-backed draft was restored over stale on-disk content. The
browser MAY hide legacy empty baseline rows from older history data when they
match the known stale-disk draft-restore noise pattern, but it MUST preserve
the underlying stored history on disk. The MVP MUST NOT require or expose
diff-only controls in order to browse history. The browser MUST be reachable
from a keyboard shortcut and from native context menus on eligible saved files
in both the sidebar and the active editor content surface.

#### Scenario: Open local history for a saved file
- **WHEN** the active editor is a saved document and the user invokes the local-history action
- **THEN** the system opens a local-history browser for that document
- **AND** the browser lists available snapshots from newest to oldest
- **AND** the selected snapshot is shown in a read-only preview

#### Scenario: Wide-window local history opens as a large viewer
- **WHEN** the local-history browser is opened in a window width that can
  comfortably show the snapshot list and preview side by side
- **THEN** the browser opens as a large dialog that occupies most of the parent
  window without exceeding it
- **AND** the preview receives the majority of the side-by-side width
- **AND** the snapshot list remains available as a narrower browse rail

#### Scenario: Empty historical snapshot explains itself
- **WHEN** the user selects a stored snapshot whose text body is empty
- **THEN** the preview shows an explicit empty-snapshot explanation instead of
  a blank content area
- **AND** the explanation makes clear that the snapshot itself contained no
  text when captured
- **AND** the browser does not present the state as a preview failure

#### Scenario: Empty snapshot metadata is semantic
- **WHEN** the browser lists or focuses a snapshot whose text body is empty
- **THEN** the snapshot metadata indicates that the snapshot is empty
- **AND** the browser does not rely only on `0 B` to communicate that state

#### Scenario: Empty snapshot keeps restore available
- **WHEN** the user selects a stored snapshot whose text body is empty
- **THEN** the restore action remains available
- **AND** secondary copy behavior reflects that there is no text content to copy

#### Scenario: Legacy stale-disk empty baselines are hidden from view
- **WHEN** a stored history timeline contains older empty baseline rows that
  match the known stale-disk draft-restore noise pattern
- **THEN** the browser omits those rows from the visible snapshot list
- **AND** the remaining visible rows still preserve correct preview and action
  behavior

#### Scenario: Hidden legacy rows remain stored
- **WHEN** the browser suppresses a legacy stale-disk empty baseline row from
  view
- **THEN** the underlying stored local-history data remains unchanged on disk

#### Scenario: Open local history from the keyboard
- **WHEN** the active editor is an eligible saved document and the user presses the local-history shortcut
- **THEN** the system opens the local-history browser for that document

#### Scenario: Open local history from the sidebar context menu
- **WHEN** the user right-clicks an eligible saved file row in the sidebar and chooses `Local History`
- **THEN** the system opens the local-history browser for that file

#### Scenario: Open local history from the editor context menu
- **WHEN** the active editor is an eligible saved document and the user chooses `Local History` from the editor content context menu
- **THEN** the system opens the local-history browser for that document

#### Scenario: Narrow-window local history browsing
- **WHEN** the local-history browser is opened in a window width that cannot comfortably show the snapshot list and preview side by side
- **THEN** the system adapts the browser into a navigation flow that still allows the user to reach both the snapshot list and the selected snapshot preview

#### Scenario: No snapshots available
- **WHEN** the user opens local history for a saved document that has no stored snapshots
- **THEN** the browser shows an empty state instead of a broken or blank list

#### Scenario: Preview text keeps deliberate inner spacing
- **WHEN** the browser shows a read-only snapshot preview
- **THEN** the preview text is padded inside its scrollable surface instead of rendering flush against the frame edge

### Requirement: Local-history restore is safe and reversible
The system SHALL restore historical snapshots into the active editor buffer without writing directly to disk. Before replacing the buffer content, the system MUST store the current buffer state as a fresh local-history snapshot. After restore, the system MUST mark the editor modified and MUST provide an immediate undo path. The system SHALL also provide a non-destructive copy action for the selected snapshot.

#### Scenario: Restore a historical snapshot
- **WHEN** the user chooses Restore for a selected snapshot in the local-history browser
- **THEN** the system stores the current buffer content as a fresh local-history snapshot before applying the selected snapshot
- **AND** the editor buffer is replaced with the selected snapshot content
- **AND** the editor is marked modified after restore

#### Scenario: Undo a restore
- **WHEN** the user restores a snapshot and then invokes the immediate undo affordance for that restore
- **THEN** the system returns the editor buffer to the content that was active immediately before the restore

#### Scenario: Copy snapshot content
- **WHEN** the user chooses Copy for a selected snapshot in the local-history browser
- **THEN** the system copies that snapshot content without modifying the active editor buffer

### Requirement: Local-history identity follows in-app renames and resets on Save As
The system SHALL key local history by a stable saved-document identity derived from the document’s canonical path. When a saved document or its parent path is renamed through LushText’s in-app rename workflow, the system MUST migrate the existing local-history lineage to the new path identity. When a document is saved through Save As, the system MUST start a new local-history lineage for the new path instead of merging histories.

#### Scenario: In-app rename preserves history lineage
- **WHEN** the user renames a saved document or one of its ancestor directories through LushText’s in-app rename workflow
- **THEN** the system keeps that document’s existing local-history snapshots associated with the renamed path

#### Scenario: Save As starts a new history lineage
- **WHEN** the user saves a document to a new path through Save As
- **THEN** the new path starts with its own local-history lineage
- **AND** the previous path’s local-history snapshots are not merged into the new path automatically

### Requirement: Local history respects large-file safety policy
The system SHALL apply LushText’s existing large-file safety thresholds to local history. For files above 10 MB and at or below 50 MB, the system MUST limit history capture to save-boundary snapshots. For files above 50 MB, the system MUST make local history unavailable and MUST not capture or preview historical snapshots for that document.

#### Scenario: Reduced history capture for very large but still openable files
- **WHEN** the active saved document is larger than 10 MB and not larger than 50 MB
- **THEN** the system limits local-history capture to save-boundary snapshots

#### Scenario: Local history unavailable for huge files
- **WHEN** the active saved document is larger than 50 MB
- **THEN** the system does not offer local-history browsing for that document
- **AND** the system does not create new local-history snapshots for that document

### Requirement: Local history is stored as app-data lineages keyed by saved-document identity
The system SHALL persist local history under `$XDG_DATA_HOME/lushtext/local-history/` using one lineage per saved-document identity derived from the document's canonical path. Snapshot metadata and snapshot text MUST live under that lineage rather than inside the source-file tree, so history remains separate from user documents and version-controlled project files.

#### Scenario: First snapshot creates an app-data lineage for the document
- **WHEN** the system captures the first local-history snapshot for a saved document
- **THEN** the snapshot is stored under the app data directory in that document's local-history lineage
- **AND** the source document's own directory is not used as the history store

### Requirement: Local-history retention stays bounded across documents
The system SHALL keep local-history retention bounded by trimming the oldest stored snapshots after newer ones are recorded. The shipped retention policy MUST keep at most 48 snapshots for one document lineage and at most 240 snapshots across the whole app-data history store.

#### Scenario: One document lineage trims its oldest snapshots after the per-document cap
- **WHEN** a document's local-history lineage grows beyond 48 stored snapshots
- **THEN** the oldest snapshots in that lineage are removed
- **AND** the newest snapshots remain available for browsing and restore

#### Scenario: Global retention trims the oldest snapshots across all lineages
- **WHEN** the total number of stored local-history snapshots across the app exceeds 240
- **THEN** the oldest stored snapshots across all lineages are trimmed
- **AND** newer snapshots remain available across the retained lineages


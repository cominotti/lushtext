## MODIFIED Requirements

### Requirement: The system captures local-history snapshots for saved documents automatically
The system SHALL capture local-history snapshots for saved, file-backed
documents without blocking the GTK main thread. The system MUST record a
baseline snapshot when a clean saved document first becomes modified, MUST
record additional snapshots no more than once every five minutes while that
document remains modified, and MUST record a snapshot after each successful
save. The system MUST skip writing a new snapshot when the candidate content is
identical to the most recent snapshot already stored for that document. When a
file-backed draft is restored at open time, the system MUST treat that restored
content as continuity of prior unsaved work rather than as a fresh editing
cycle for baseline-capture purposes, and it MUST NOT create a new baseline
snapshot solely for the stale pre-restore on-disk file state.

#### Scenario: Baseline snapshot on first dirty transition
- **WHEN** a saved document with no unsaved changes becomes modified for the
  first time in an editing cycle
- **THEN** the system records a local-history snapshot of the document state
  that existed immediately before those unsaved edits

#### Scenario: Periodic snapshot during a long unsaved edit session
- **WHEN** a saved document remains modified and at least five minutes have
  elapsed since its last local-history snapshot
- **THEN** the system records a new local-history snapshot in the background

#### Scenario: Deduplicated snapshot candidate
- **WHEN** the system reaches a local-history capture boundary but the candidate
  content is identical to the newest stored snapshot for that document
- **THEN** the system does not create a duplicate snapshot

#### Scenario: Post-save snapshot
- **WHEN** the user successfully saves a saved document
- **THEN** the system records a local-history snapshot representing the saved
  content

#### Scenario: Draft-restored file does not add a fresh stale-disk baseline
- **WHEN** a saved file opens, file-backed draft recovery restores unsaved text,
  and the editor becomes modified because of that draft restoration
- **THEN** the system does not add a new baseline snapshot solely for the
  pre-restore on-disk file contents
- **AND** the visible local-history timeline reflects the restored working
  document and later meaningful history states instead of a fresh stale-disk
  checkpoint

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
MVP MUST NOT require or expose diff-only controls in order to browse history.
The browser MUST be reachable from a keyboard shortcut and from native context
menus on eligible saved files in both the sidebar and the active editor content
surface.

#### Scenario: Open local history for a saved file
- **WHEN** the active editor is a saved document and the user invokes the
  local-history action
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

#### Scenario: Draft-restored timeline omits stale-disk baseline noise
- **WHEN** the user opens local history for a saved file whose unsaved draft was
  restored at open time
- **THEN** the visible snapshot list does not include a fresh baseline row that
  only represents the stale pre-restore on-disk file contents

#### Scenario: Open local history from the keyboard
- **WHEN** the active editor is an eligible saved document and the user presses
  the local-history shortcut
- **THEN** the system opens the local-history browser for that document

#### Scenario: Open local history from the sidebar context menu
- **WHEN** the user right-clicks an eligible saved file row in the sidebar and
  chooses `Local History`
- **THEN** the system opens the local-history browser for that file

#### Scenario: Open local history from the editor context menu
- **WHEN** the active editor is an eligible saved document and the user chooses
  `Local History` from the editor content context menu
- **THEN** the system opens the local-history browser for that document

#### Scenario: Narrow-window local history browsing
- **WHEN** the local-history browser is opened in a window width that cannot
  comfortably show the snapshot list and preview side by side
- **THEN** the system adapts the browser into a navigation flow that still
  allows the user to reach both the snapshot list and the selected snapshot
  preview

#### Scenario: No snapshots available
- **WHEN** the user opens local history for a saved document that has no stored
  snapshots
- **THEN** the browser shows an empty state instead of a broken or blank list

#### Scenario: Preview text keeps deliberate inner spacing
- **WHEN** the browser shows a read-only snapshot preview
- **THEN** the preview text is padded inside its scrollable surface instead of
  rendering flush against the frame edge

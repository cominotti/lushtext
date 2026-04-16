## MODIFIED Requirements

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

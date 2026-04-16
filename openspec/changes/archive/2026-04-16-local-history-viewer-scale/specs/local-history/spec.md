## MODIFIED Requirements

### Requirement: Users can browse local history for the active saved document
The system SHALL provide a deliberate browse action for local history on the
active saved document. Opening local history MUST present an adaptive
GTK-native browser that shows snapshots in newest-first order together with a
read-only preview of the currently selected snapshot. On windows wide enough to
show both areas side by side, the browser MUST open as a large, viewer-first
dialog that occupies most of the parent window while remaining smaller than the
parent window, and the preview MUST receive the majority of the side-by-side
width. The MVP MUST NOT require or expose diff-only controls in order to browse
history. The browser MUST be reachable from a keyboard shortcut and from native
context menus on eligible saved files in both the sidebar and the active editor
content surface.

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

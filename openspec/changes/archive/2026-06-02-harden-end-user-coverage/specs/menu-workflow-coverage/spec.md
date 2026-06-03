## ADDED Requirements

### Requirement: Zoom controls are covered through user-visible actions
The test suite SHALL cover zoom workflow behavior through the same actions,
shortcuts, or menu controls available to users.

#### Scenario: Zoom in and out update the active editor
- **WHEN** the user invokes Zoom In or Zoom Out
- **THEN** the active editor's zoom level changes by the configured step
- **AND** the visible editor text reflects the updated zoom state

#### Scenario: Reset zoom restores the default level
- **WHEN** the active editor has a non-default zoom level
- **AND** the user invokes Reset Zoom
- **THEN** the editor returns to the default zoom level
- **AND** zoom-related controls report the correct enabled or disabled state

#### Scenario: Zoom state is scoped correctly
- **WHEN** the user switches tabs after changing zoom
- **THEN** the tested zoom contract is preserved for the active editor or shared
  setting according to the product behavior documented by the implementation

### Requirement: Theme selection is covered through the real preference/action path
The test suite SHALL cover the user-visible theme or style-selection workflow
that changes LushText's appearance.

#### Scenario: Theme selection updates the current window
- **WHEN** the user selects a supported theme preference
- **THEN** the current window updates its style preference and editor style
  scheme consistently

#### Scenario: Theme selection applies to newly opened editors
- **WHEN** a theme preference has been selected
- **AND** the user opens a new document tab
- **THEN** the new editor uses the selected style behavior without requiring a
  restart

#### Scenario: Invalid or missing style scheme falls back safely
- **WHEN** the stored style preference references a missing or invalid scheme
- **THEN** the app falls back to a supported style without crashing or leaving
  unreadable editor colors

### Requirement: Invisible-character controls are covered
The test suite SHALL cover the visible controls and actions that toggle
invisible-character rendering.

#### Scenario: Invisible-character mode cycles through supported values
- **WHEN** the user invokes the invisible-character mode control repeatedly
- **THEN** each supported mode is reached in the documented order
- **AND** the active editor's space-drawing configuration reflects the selected
  mode

#### Scenario: Invisible-character preference persists
- **WHEN** the user changes the invisible-character mode
- **THEN** the selected mode is stored through the normal preferences path
- **AND** newly opened editor tabs use that mode

### Requirement: Print workflow is covered without requiring a physical printer
The test suite or smoke lane SHALL cover print action wiring and failure/cancel
behavior through a testable print operation path.

#### Scenario: Print action creates a print operation for the active document
- **WHEN** the user invokes Print with an active document
- **THEN** LushText prepares a print operation containing the active document
  content and metadata
- **AND** the app remains responsive while the print dialog or operation is
  active

#### Scenario: Print cancel leaves document state unchanged
- **WHEN** the print operation is canceled
- **THEN** the document content, modified flag, path identity, and draft state are
  unchanged

#### Scenario: Print failure reports feedback
- **WHEN** the print operation fails before completion
- **THEN** LushText reports failure through the normal feedback path
- **AND** the document remains editable and unchanged

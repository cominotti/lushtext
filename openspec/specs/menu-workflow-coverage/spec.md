# menu-workflow-coverage Specification

## Purpose
Define LushText's menu and action workflow coverage so visible commands remain
connected to the document, preference, print, and editor behavior users expect.

## Requirements
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

### Requirement: Visible commands SHALL be represented in the action catalog
The test suite and documentation SHALL keep visible commands, menus, shortcuts,
command-palette entries, toolbar buttons, context-menu items, and status-bar
controls aligned with the action catalog. A visible command MUST either map to a
stable action or document why it requires a different interaction surface.

#### Scenario: Primary menu commands map to actions
- **WHEN** the primary menu model is audited
- **THEN** each actionable menu item maps to a cataloged app or window action
- **AND** the catalog names the owning workflow, label, shortcut when present,
  parameter type, and coverage lane

#### Scenario: Notes, sidebar, tab, and search commands map to actions
- **WHEN** notes menu items, sidebar context menu items, tab context menu items,
  search controls, status controls, and command-palette commands are audited
- **THEN** each visible command maps to a cataloged action or documented
  non-action control
- **AND** unsupported automation gaps are tracked as explicit follow-ups rather
  than hidden test assumptions

#### Scenario: Command palette uses cataloged commands
- **WHEN** the command palette indexes commands
- **THEN** indexed commands use cataloged action IDs and labels
- **AND** stale command IDs, missing actions, or undocumented command entries
  fail tests or catalog checks

### Requirement: Menu and action workflow tests SHALL use the same public actions as users
Menu workflow coverage SHALL prove behavior through user-visible actions and
their D-Bus/catalog representation, while still using widget assertions for
visible state and accessibility assertions where appropriate.

#### Scenario: Action activation and menu activation agree
- **WHEN** a workflow can be invoked through both a menu item and direct action
  activation
- **THEN** tests verify both paths reach the same state change or documented
  no-op
- **AND** the action catalog records both invocation surfaces

#### Scenario: Disabled commands are observable
- **WHEN** a command is unavailable because no document, no workspace folder, no
  search context, save in progress, or another precondition applies
- **THEN** the action enablement, menu sensitivity, command-palette
  availability, and automation snapshot agree according to the documented rule

#### Scenario: Parameterized commands are covered
- **WHEN** a command accepts a parameter such as search text, workspace scope,
  tab identity, preview mode, or zoom direction
- **THEN** tests cover valid values, invalid values, and no-context behavior
- **AND** the catalog documents parameter type and accepted values

### Requirement: Command documentation SHALL stay synchronized
The project SHALL document public commands in user-facing and developer-facing
references and MUST keep command docs synchronized with action registration,
menu resources, shortcuts, command palette entries, and tests.

#### Scenario: User-facing command docs are current
- **WHEN** a public command, shortcut, menu label, command-palette label, or
  visible control changes
- **THEN** user-facing docs such as README, shortcuts references, and automation
  examples are updated in the same change

#### Scenario: Developer command reference is current
- **WHEN** an action is added, removed, renamed, retargeted, or changes
  parameter/state type
- **THEN** the developer reference and action catalog are updated
- **AND** the documentation drift check fails if they are stale

### Requirement: Keyboard Shortcuts Command Is Registered And Covered
The visible Keyboard Shortcuts command SHALL resolve to a registered user-facing
window action and SHALL be covered through the same menu, command-palette, and
automation contracts as other visible commands.

#### Scenario: Visible command resolves to registered action
- **WHEN** LushText builds its window actions, primary menu, and command palette
- **THEN** `win.show-help-overlay` is registered as a window action
- **AND** the primary menu and command palette Keyboard Shortcuts entries
  reference that registered action
- **AND** the action catalog no longer marks the command as
  `visible-unregistered-gap` or `unsupported-gap`

#### Scenario: Action opens shipped shortcut window
- **WHEN** the user or an automation client activates `win.show-help-overlay`
- **THEN** LushText presents the shipped shortcut help window from
  `resources/ui/shortcuts.ui`
- **AND** the shortcut window is associated with the active LushText window
- **AND** activating the action does not modify document contents, tab state,
  workspace state, or persistent settings

#### Scenario: Empty or no-document state can open shortcuts
- **WHEN** LushText has no file-backed active document or starts in an
  empty/no-context state
- **THEN** the Keyboard Shortcuts action remains available
- **AND** the shortcut window opens without requiring an editor, workspace,
  note, bookmark, or search context

#### Scenario: Shortcut window remains usable with many shortcuts
- **WHEN** the shortcut help window contains several groups or more shortcuts
  than fit vertically
- **THEN** the shortcut content scrolls within the help window or
  toolkit-provided shortcut surface
- **AND** the window title/header, close affordance, and essential actions
  remain reachable
- **AND** no fake shortcut rows are inserted to satisfy tests

#### Scenario: Shortcut window remains usable in constrained geometry
- **WHEN** the Keyboard Shortcuts action is activated while the main window or
  virtual monitor is narrow or short
- **THEN** the shortcut help window remains bounded to the visible monitor area
- **AND** text, section labels, and close controls do not overlap incoherently or
  disappear behind unrelated app chrome

#### Scenario: Documentation and audits reflect supported status
- **WHEN** maintainers run action catalog, visible-static-action,
  command-palette, and automation documentation drift checks
- **THEN** `win.show-help-overlay` is represented as a supported exported action
  with documented surfaces, safety classification, enablement rule, docs anchor,
  and coverage lanes
- **AND** stale documentation that still describes it as an unsupported gap
  fails validation

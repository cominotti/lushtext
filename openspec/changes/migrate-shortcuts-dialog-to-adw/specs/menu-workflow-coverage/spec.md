## MODIFIED Requirements

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

#### Scenario: Action opens shipped shortcut dialog
- **WHEN** the user or an automation client activates `win.show-help-overlay`
- **THEN** LushText presents the shipped Libadwaita shortcut help dialog from
  `resources/ui/shortcuts.ui`
- **AND** the shortcut dialog is associated with the active LushText window
- **AND** activating the action does not modify document contents, tab state,
  workspace state, or persistent settings

#### Scenario: Empty or no-document state can open shortcuts
- **WHEN** LushText has no file-backed active document or starts in an
  empty/no-context state
- **THEN** the Keyboard Shortcuts action remains available
- **AND** the shortcut dialog opens without requiring an editor, workspace,
  note, bookmark, or search context

#### Scenario: Shortcut dialog remains usable with many shortcuts
- **WHEN** the shortcut help dialog contains several groups or more shortcuts
  than fit vertically
- **THEN** the shortcut content scrolls within the toolkit-provided shortcut
  surface
- **AND** the dialog title/header, close affordance, and essential actions
  remain reachable
- **AND** no fake shortcut rows are inserted to satisfy tests

#### Scenario: Shortcut dialog remains usable in constrained geometry
- **WHEN** the Keyboard Shortcuts action is activated while the main window or
  virtual monitor is narrow or short
- **THEN** the shortcut help dialog remains bounded to the visible monitor area
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

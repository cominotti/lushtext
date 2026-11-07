## MODIFIED Requirements

### Requirement: Shell transient surfaces dismiss through a topmost owner
The system SHALL route Escape and equivalent dismiss requests through a window-level transient-surface owner that closes only the topmost visible dismissible surface. The topmost owner MUST close the command palette before lower-priority non-modal surfaces such as the workspace search panel or Focus Mode affordance. When the command palette shows a category view, Escape MUST first return the palette to its grouped results, keeping it open with its query, and a later Escape closes it. Real modal dialogs, file choosers, destructive confirmations, menu popovers, dropdown popups, and child popups MUST retain their own cancel/default behavior and MUST NOT be bypassed by the shell dismissal path.

#### Scenario: Escape closes command palette regardless of focus
- **WHEN** the command palette is visible showing grouped results
- **AND** keyboard focus is in the editor, sidebar, status bar, or another non-modal child outside the palette
- **THEN** pressing Escape closes the command palette
- **AND** the command palette's search text and results are cleared
- **AND** focus is restored through the saved-focus fallback path

#### Scenario: Escape closes one topmost surface
- **WHEN** the command palette is visible above the workspace search panel
- **AND** the user presses Escape
- **THEN** the command palette closes
- **AND** the workspace search panel remains visible
- **AND** a later Escape may dismiss the next topmost eligible surface according to its existing close contract

#### Scenario: Modal child owns Escape first
- **WHEN** a real modal dialog, destructive confirmation, file chooser, menu popover, dropdown popup, or command-palette child popup is open
- **AND** the user presses Escape
- **THEN** that child surface receives its own cancel or close behavior first
- **AND** the shell-level transient closer does not bypass the child surface's semantics

#### Scenario: Escape leaves a palette category view first
- **WHEN** the command palette shows a category view
- **AND** the user presses Escape with focus in the query entry, in the palette's list, or outside the palette
- **THEN** the palette shows its grouped results with the query kept and stays open
- **AND** a second Escape closes the palette through the shared close path

## ADDED Requirements

### Requirement: Opening a palette category view keeps the palette open
Activating a command palette "More" row by keyboard, pointer or automation SHALL open that category's view without closing the palette and without running the window's result activation path. Pointer activation outside the palette SHALL still close the palette from a category view, through the shared close path, and reopening the palette SHALL show grouped results.

#### Scenario: Activating a More row
- **WHEN** the user double-clicks a More row or presses Enter with it selected
- **THEN** the palette stays visible and shows that category's view

#### Scenario: Outside click from a category view
- **WHEN** a category view is shown and the user clicks the editor
- **THEN** the palette closes once through the shared close path and focus is restored as for any palette close

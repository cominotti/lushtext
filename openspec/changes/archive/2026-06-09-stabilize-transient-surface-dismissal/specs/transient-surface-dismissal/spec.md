## ADDED Requirements

### Requirement: Shell transient surfaces dismiss through a topmost owner
The system SHALL route Escape and equivalent dismiss requests through a window-level transient-surface owner that closes only the topmost visible dismissible surface. The topmost owner MUST close the command palette before lower-priority non-modal surfaces such as the workspace search panel or Focus Mode affordance. Real modal dialogs, file choosers, destructive confirmations, menu popovers, dropdown popups, and child popups MUST retain their own cancel/default behavior and MUST NOT be bypassed by the shell dismissal path.

#### Scenario: Escape closes command palette regardless of focus
- **WHEN** the command palette is visible
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

### Requirement: Modal dialogs keep explicit dismissal semantics
The system SHALL NOT treat real modal dialogs as click-away transient overlays by default. `AdwDialog`, `AdwAlertDialog`, file chooser dialogs, preferences dialogs, and browser-style dialogs such as Notes or Local History MUST close through their explicit close control, Escape or equivalent dialog close shortcut, a dialog response, or a surface-specific close gesture such as a bottom-sheet swipe where the toolkit provides it. Pointer activation outside a modal dialog MUST NOT close that dialog unless a future surface-specific requirement explicitly opts into click-away behavior and preserves any confirmation, response, or unsaved-state semantics.

#### Scenario: Empty Notes dialog does not close on outside click
- **WHEN** the empty Notes browser is visible as a modal dialog
- **AND** the user clicks outside the dialog content
- **THEN** the dialog remains visible
- **AND** the visible close control and Escape continue to dismiss it

#### Scenario: Destructive or response dialogs are not bypassed
- **WHEN** a destructive confirmation, save-changes prompt, file chooser, preferences dialog, or other response-oriented modal is visible
- **AND** the user clicks outside the dialog content
- **THEN** the dialog remains visible
- **AND** the user must choose an explicit response, close control, Escape, or toolkit-provided close gesture according to that dialog's contract

### Requirement: Command palette dismisses on outside pointer activation
The system SHALL close the command palette when the user performs a pointer activation outside the visible palette surface. Pointer activation inside the palette, its result rows, mode selector, scroll area, or child popups MUST NOT close the palette unless the interaction itself activates a result or explicit close behavior. Outside-click dismissal MUST use the same close path as Escape so saved focus is consumed once and restored consistently.

#### Scenario: Outside click closes command palette
- **WHEN** the command palette is visible
- **AND** the user clicks the editor, sidebar, status bar, tab strip, or other window chrome outside the palette
- **THEN** the command palette closes
- **AND** focus restoration follows the same saved-focus/editor/no-editor fallback used by normal palette close

#### Scenario: Inside click keeps command palette open
- **WHEN** the command palette is visible
- **AND** the user clicks the search entry, mode selector, scroll area, a presentation header, or a non-activated result row inside the palette
- **THEN** the command palette remains visible
- **AND** the intended child interaction remains usable

#### Scenario: Result activation still closes command palette
- **WHEN** the command palette is visible with representative file or command results
- **AND** the user activates an actionable result by pointer or keyboard
- **THEN** the selected file or command action runs through the existing activation path
- **AND** the command palette closes once through the shared close path

### Requirement: Transient dismissal preserves palette state extremes
The system SHALL keep command-palette dismissal behavior stable across empty, populated, dense, and constrained UI states. Independent command-palette access MUST remain available without an open tab or workspace, empty result states MUST remain readable while visible, dense result states MUST keep scrolling inside the result region, and dismissal controls MUST NOT introduce fake rows, unrelated workspace dependencies, unintended scrollbars, or hidden header controls.

#### Scenario: No tab or workspace still dismisses
- **WHEN** no document tab or workspace folder is available
- **AND** the command palette is visible
- **THEN** Escape and outside click still close the command palette
- **AND** no fake row or unrelated context is required for dismissal

#### Scenario: Dense results keep dismissal predictable
- **WHEN** the command palette shows many or awkward results with long labels or deep paths
- **AND** the result region scrolls
- **THEN** Escape and outside click still close the palette through the shared close path
- **AND** the result list remains the only scrolling region while the palette is visible

#### Scenario: Constrained geometry preserves controls
- **WHEN** the command palette is visible in a narrow or short supported window
- **THEN** the search entry, mode selector, result region, and empty state remain reachable or readable
- **AND** Escape and outside click remain available without requiring focus to be inside the palette

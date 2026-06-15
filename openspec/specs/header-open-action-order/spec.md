# header-open-action-order Specification

## Purpose
Define the start-side header ordering for the GNOME-style Open control and the New File/New Tab control.

## Requirements
### Requirement: Header Start Actions Put Open Before New
The system SHALL render the GNOME-style Open menu button before the New File/New Tab button on the start side of the main window header bar. The ordering MUST remain stable across the wide `Open` label presentation and the constrained folder-icon presentation, while preserving the existing actions, shortcuts, tooltips, accessible meanings, and popover behavior of both controls.

#### Scenario: Wide header renders Open before New
- **WHEN** the main window has enough header width for the Open control's wide label presentation
- **THEN** the header start controls show the Open menu button before the New File/New Tab button
- **AND** the Open control still displays the `Open` label with its down-chevron indicator
- **AND** the New File/New Tab control remains visible and reachable after Open

#### Scenario: Constrained header keeps Open first
- **WHEN** the main window is constrained enough for the Open control to use its folder-symbolic presentation
- **THEN** the folder-symbolic Open menu button remains before the New File/New Tab button
- **AND** both controls remain reachable by keyboard and accessibility APIs
- **AND** the ordering does not introduce clipping, overlap, or unintended horizontal scrolling in the header

#### Scenario: Actions and shortcuts are unchanged
- **WHEN** the header controls have been reordered
- **THEN** activating Open still opens the recent-document popover
- **AND** activating New File/New Tab still creates a new untitled tab through `win.new-tab`
- **AND** `Ctrl+K`, `Ctrl+O`, and `Ctrl+N` keep their existing meanings

#### Scenario: Open popover behavior is unchanged by ordering
- **WHEN** the user opens the Open popover from the reordered header
- **THEN** the search entry, file-chooser button, recent-list or empty state, keyboard navigation, and dismissal behavior match the existing Open popover contract
- **AND** the popover remains anchored to the Open menu button now rendered first

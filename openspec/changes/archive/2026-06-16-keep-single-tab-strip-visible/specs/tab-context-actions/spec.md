## ADDED Requirements

### Requirement: Tab strip remains reachable for every normal open-tab state
The system SHALL render the tab strip as a visible tab-management surface in normal mode whenever the window has one or more open tabs. The tab strip MUST remain hidden when the window has no open tabs, and Focus Mode MUST continue to suppress the ordinary tab strip according to the focused-writing contract.

#### Scenario: Empty window keeps tab strip hidden
- **WHEN** the window has no open tabs
- **THEN** the empty document state remains visible
- **AND** the tab strip is not rendered as an inert or blank control

#### Scenario: Single unpinned tab exposes tab context target
- **WHEN** exactly one unpinned tab is open in normal mode
- **THEN** the tab strip is visible
- **AND** the open tab is visible as a tab-strip page target
- **AND** the tab context menu can be opened for that tab
- **AND** the Pin action is reachable from that menu

#### Scenario: Single pinned tab keeps the same visible tab surface
- **WHEN** exactly one pinned tab is open in normal mode
- **THEN** the tab strip remains visible
- **AND** the pinned state indicator remains visible on the tab-strip page
- **AND** the Unpin action is reachable from that tab's context menu

#### Scenario: Multiple tabs preserve existing tab-management behavior
- **WHEN** two or more tabs are open in normal mode
- **THEN** the tab strip remains visible
- **AND** existing tab context actions continue to target the tab whose context menu was opened
- **AND** pinned and unpinned tabs keep their existing grouping and ordering behavior

#### Scenario: Focus Mode continues to suppress tab strip
- **WHEN** Focus Mode is active with one or more open tabs
- **THEN** the ordinary tab strip is not rendered
- **AND** exiting Focus Mode restores the normal-mode tab strip when at least one tab remains open

#### Scenario: Constrained normal geometry keeps tab strip usable
- **WHEN** the normal-mode window is at its supported constrained height or width with at least one open tab
- **THEN** the tab strip retains a nonzero visible allocation
- **AND** the editor viewport and status bar remain usable
- **AND** the window does not gain unintended root scrolling or overlapping persistent chrome

# tab-context-actions Specification

## Purpose
TBD - created by archiving change tab-context-actions. Update Purpose after archive.
## Requirements
### Requirement: Tab context actions target the clicked tab
The system SHALL provide a context menu for tab-strip pages. That menu MUST include `Pin` or `Unpin`, `Close All Tabs to the Right`, `Close Other Tabs`, `Move Left`, and `Move Right`. Menu actions MUST apply to the tab whose context menu was opened, even when that tab is not the currently selected editor page.

#### Scenario: Open the menu on a background tab
- **WHEN** the user opens the tab context menu on a tab that is not currently selected
- **THEN** the menu presents the tab-management actions for that clicked tab
- **AND** choosing one of those actions applies it to the clicked tab without requiring the user to activate it first

#### Scenario: Unavailable actions are disabled
- **WHEN** the user opens the context menu on a tab that has no eligible tab to its right or cannot move farther in one direction
- **THEN** the corresponding close or move action is unavailable
- **AND** the remaining valid actions stay available

### Requirement: Pinning creates a persistent leading tab segment
The system SHALL let users pin and unpin tabs from the context menu. Pinned tabs MUST remain grouped at the leading edge of the tab strip ahead of unpinned tabs. The system MUST restore pinned state and relative tab order across sessions.

#### Scenario: Pin an unpinned tab
- **WHEN** the user pins a tab while at least one unpinned tab is open
- **THEN** the pinned tab moves into the leading pinned segment
- **AND** it remains ahead of every unpinned tab

#### Scenario: Restore pinned layout on restart
- **WHEN** the user restarts LushText after arranging a mix of pinned and unpinned tabs
- **THEN** pinned tabs restore before unpinned tabs
- **AND** each group keeps the same relative order it had when the session was last saved

### Requirement: Bulk-close actions preserve pinned tabs and use safe confirmation
The system SHALL provide `Close Other Tabs` and `Close All Tabs to the Right` on the tab context menu. Those actions MUST compute their target set from the clicked tab's current position, MUST exclude pinned tabs from the bulk-close set, and MUST reuse the existing save-changes confirmation flow before closing any modified target tabs.

#### Scenario: Close other tabs keeps pinned tabs open
- **WHEN** the user runs `Close Other Tabs` on an unpinned tab while pinned tabs and other unpinned tabs are open
- **THEN** the pinned tabs remain open
- **AND** every other unpinned tab except the clicked tab closes

#### Scenario: Modified target tabs require confirmation
- **WHEN** the user runs a bulk-close action and at least one targeted tab has unsaved changes
- **THEN** the existing save-changes confirmation flow appears before those tabs close
- **AND** cancelling that confirmation leaves the targeted tabs open

### Requirement: Move Left and Move Right reorder within the current tab segment
The system SHALL let users move the clicked tab one position left or right from the context menu. A pinned tab MUST move only within the pinned segment, and an unpinned tab MUST move only within the unpinned segment. The resulting order MUST persist across session restore.

#### Scenario: Move an unpinned tab left without crossing the pinned boundary
- **WHEN** the user chooses `Move Left` on an unpinned tab that has another unpinned tab immediately before it
- **THEN** the tab moves one position left within the unpinned segment
- **AND** it does not move ahead of any pinned tab

#### Scenario: Restore moved tab order on restart
- **WHEN** the user restarts LushText after moving tabs left or right
- **THEN** the restored tab order matches the last saved order within each pinned-state segment
- **AND** the moved tab keeps its pinned or unpinned state

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

# Spec Delta

## Purpose

Let users scroll an overflowing tab strip and reach any open tab from visible,
keyboard-accessible controls, without changing how the strip lays out its tabs.

## ADDED Requirements

### Requirement: Scroll controls appear only while the unpinned tabs overflow
The system SHALL show Scroll Tabs Back and Scroll Tabs Forward buttons at the trailing end of the tab strip once the unpinned tabs do not fit the strip, and hide them once those tabs fit beside the buttons. Between the two widths the buttons keep their state. Showing or hiding them MUST NOT change the strip's height or move the pinned tabs.

#### Scenario: Tabs fit the strip
- **WHEN** the window has open tabs that all fit the strip in normal mode
- **THEN** neither scroll button is shown

#### Scenario: Opening tabs until they overflow
- **WHEN** the user opens tabs until the unpinned tabs no longer fit the strip
- **THEN** both scroll buttons appear at the strip's trailing end, after the tabs
- **AND** the strip's height is the same before and after they appear

#### Scenario: Closing tabs until they fit beside the buttons
- **WHEN** the user closes tabs until the remaining unpinned tabs fit the strip while the buttons are shown
- **THEN** the scroll buttons disappear

#### Scenario: Between the two widths
- **WHEN** the unpinned tabs would fit without the buttons but not beside them
- **THEN** the buttons stay in the state they were in, shown or hidden
- **AND** at any fixed window width and tab set they do not change state

#### Scenario: Resizing the window across the overflow point
- **WHEN** the user narrows the window until the tabs overflow, then widens it until they fit beside the buttons
- **THEN** the buttons appear once and disappear once

#### Scenario: Only pinned tabs overflow
- **WHEN** the pinned tabs alone are wider than their share of the strip and the unpinned tabs fit
- **THEN** no scroll button is shown

#### Scenario: Restoring a session with many tabs
- **WHEN** LushText restores a session whose tabs overflow the strip
- **THEN** the scroll buttons appear once, after the restored tabs are in place, not once per restored batch

#### Scenario: Right-to-left layout
- **WHEN** the interface direction is right-to-left and the strip overflows
- **THEN** the buttons sit at the strip's trailing (left) end and Back points toward the first tab

### Requirement: Scroll buttons move the strip without changing the selected tab
Activating a scroll button SHALL scroll the unpinned tabs by about one visible strip width toward the first tab (Back) or the last tab (Forward), without selecting, focusing or reordering any tab. Each button MUST be disabled at its edge. Holding a button pressed MUST keep scrolling until release or the edge.

#### Scenario: Scroll forward from the start
- **WHEN** the strip overflows and shows its first unpinned tab, and the user activates Scroll Tabs Forward
- **THEN** a tab that was entirely beyond the trailing edge is now visible
- **AND** the selected tab, the editor's text and the keyboard focus are unchanged

#### Scenario: Buttons at the edges
- **WHEN** the strip is scrolled to its start
- **THEN** Scroll Tabs Back is disabled and Scroll Tabs Forward is enabled
- **AND** at its end, Scroll Tabs Forward is disabled and Scroll Tabs Back is enabled

#### Scenario: Holding a scroll button
- **WHEN** the user presses and holds Scroll Tabs Forward on a strip with many hidden tabs
- **THEN** the strip keeps scrolling forward while the button is held and stops at the end
- **AND** releasing the button adds no extra step

#### Scenario: A focused button becomes disabled
- **WHEN** keyboard focus is on Scroll Tabs Forward and the strip reaches its end
- **THEN** focus moves to Scroll Tabs Back

#### Scenario: Pinned tabs stay put
- **WHEN** pinned tabs are open and the user scrolls the strip in either direction
- **THEN** the pinned tabs stay visible in place and only the unpinned tabs scroll

#### Scenario: Selecting a tab during a scroll
- **WHEN** a scroll step or a hold is in progress and a tab out of view is selected, by keyboard, by the All Tabs popover or by opening a document
- **THEN** the scroll stops and the selected tab is brought into view

#### Scenario: Wheel during a scroll step
- **WHEN** a scroll step or a hold is in progress and the user scrolls over the strip with the mouse wheel
- **THEN** the step or hold stops and the wheel's scroll is applied

#### Scenario: Wheel and selection scrolling still work
- **WHEN** the user scrolls the strip with the mouse wheel, or selects a tab out of view with the keyboard
- **THEN** the strip scrolls as it does on main
- **AND** the scroll buttons' enabled states follow the new position

#### Scenario: Animations disabled
- **WHEN** the desktop disables animations and the user activates a scroll button
- **THEN** the strip moves to its new position at once

### Requirement: The All Tabs button shows the tab count
The system SHALL show an All Tabs button at the trailing end of the tab strip, after the scroll buttons, whenever the strip is shown with two or more open tabs. It MUST show the full number of open tabs. It MUST be hidden with fewer than two tabs and whenever the strip is hidden, including the no-tab state and Focus Mode.

#### Scenario: One tab
- **WHEN** exactly one tab is open in normal mode
- **THEN** the All Tabs button is not shown and the strip shows no empty space for it

#### Scenario: Several tabs that fit
- **WHEN** three tabs are open and all fit the strip
- **THEN** the All Tabs button is shown with the count 3 and no scroll button is shown

#### Scenario: Count follows tab changes
- **WHEN** the user opens or closes a tab
- **THEN** the count equals the number of open tabs after the change

#### Scenario: Large counts and large text
- **WHEN** 128 tabs are open, including with the desktop's large-text setting
- **THEN** the button shows 128 in full, without clipping, and the strip keeps a usable width for tabs at the window's minimum width

#### Scenario: Focus Mode and no tabs
- **WHEN** Focus Mode is active, or the window has no open tabs
- **THEN** neither the All Tabs button nor the scroll buttons are shown

### Requirement: The All Tabs popover lists every open tab
Activating the All Tabs button SHALL open a popover with a search entry and a list of every open tab of the window in strip order, pinned tabs first. Each row MUST show the tab's title, its path or an untitled label, its unsaved-changes and pinned states, and whether it is the selected tab. The list MUST NOT show document text.

#### Scenario: Mixed tabs
- **WHEN** the window has a pinned saved file, a modified saved file and an untitled document, and the user opens the popover
- **THEN** all three rows are listed, the pinned one first
- **AND** the modified row shows its unsaved-changes indicator and the untitled row says it is not saved to a file

#### Scenario: The selected tab is marked
- **WHEN** the user opens the popover
- **THEN** the selected tab's row is marked as current and is the highlighted row

#### Scenario: Many tabs
- **WHEN** 256 tabs are open and the user opens the popover at the window's minimum size
- **THEN** only the row list scrolls, the search entry stays visible, and every tab can be reached by scrolling or searching

### Requirement: Searching the All Tabs popover filters the tabs
Typing in the popover's search entry SHALL narrow the list to tabs whose title or path matches the query, best match first and highlighted. An empty query MUST list every tab in strip order with the selected tab highlighted. When nothing matches, the popover MUST say that no open tab matches instead of showing an empty list.

#### Scenario: Filter by name
- **WHEN** tabs `notes.md`, `main.rs` and `README.md` are open and the user types `md`
- **THEN** the `notes.md` and `README.md` rows are listed, `main.rs` is not, and the first row is highlighted

#### Scenario: Filter by folder
- **WHEN** two tabs named `mod.rs` live in folders `search` and `palette`, and the user types `palette`
- **THEN** the `palette` folder's `mod.rs` is listed and highlighted first

#### Scenario: No match
- **WHEN** the user types a query no open tab matches
- **THEN** the popover says no open tab matches
- **AND** clearing the query lists every tab again with the selected tab highlighted

### Requirement: Choosing a tab in the popover selects it
Activating a popover row SHALL close the popover, select that tab, scroll the strip until the tab is visible, and focus its editor. Escape, a click outside, or the shortcut again MUST close the popover without changing the selected tab and return focus to where it was before it opened, or to the selected editor if that widget is gone.

#### Scenario: Pick a tab out of view
- **WHEN** the strip overflows and the user activates the row of a tab that is out of view
- **THEN** that tab is selected and visible in the strip, and typing goes into its document

#### Scenario: Keyboard only
- **WHEN** the user presses `Ctrl+Shift+O` in the editor, types part of a title, and presses Enter
- **THEN** the highlighted best match is selected and its editor has focus

#### Scenario: Dismiss
- **WHEN** the user opens the popover from the editor and presses Escape, or presses `Ctrl+Shift+O` again
- **THEN** the popover closes, the selected tab is unchanged, and the editor has focus again

### Requirement: The All Tabs popover follows tab changes while open
While the popover is open, the system SHALL update its rows when a tab opens, closes, is renamed, is saved, or changes pinned state, keeping the current query. Activating a row whose tab closed in the meantime MUST only close the popover.

#### Scenario: Tab closed while the popover is open
- **WHEN** the popover is open and a listed tab closes, for example through a file deletion in the sidebar
- **THEN** that row disappears and the remaining rows are the open tabs that match the query

#### Scenario: Stale activation
- **WHEN** a row is activated after its tab closed
- **THEN** the popover closes, no selection changes and no error is shown

### Requirement: Tab overflow navigation is keyboard and assistive-technology accessible
The scroll buttons and the All Tabs button SHALL be focusable, with accessible names and tooltips (Scroll Tabs Back, Scroll Tabs Forward, and "All Tabs" with the count). Each control's action MUST be disabled exactly when the control is hidden or disabled. `Ctrl+Shift+O` MUST toggle the popover whenever the All Tabs button is shown. Popover rows MUST expose bounded names.

#### Scenario: Screen reader names
- **WHEN** an assistive technology reads the end of an overflowing strip with 12 tabs
- **THEN** it finds Scroll Tabs Back, Scroll Tabs Forward and "All Tabs, 12 open"

#### Scenario: Shortcut unavailable
- **WHEN** one tab is open, or Focus Mode is active, and the user presses `Ctrl+Shift+O` in the editor
- **THEN** nothing opens and the editor keeps its focus and text

#### Scenario: Result announcement
- **WHEN** the user types a query in the popover
- **THEN** the number of matching tabs is announced once the typing settles, not on every keystroke

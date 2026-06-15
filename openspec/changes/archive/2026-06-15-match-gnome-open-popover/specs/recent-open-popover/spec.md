## ADDED Requirements

### Requirement: Header Open Control Matches GNOME Text Editor
The system SHALL replace the direct header Open button with a GNOME Text Editor-style Open menu button. In wide layouts, the button MUST render a flat `Open` label with a down chevron. In constrained layouts where the text form cannot fit comfortably, the button MUST render a flat folder icon while preserving the same popover behavior and accessibility meaning.

#### Scenario: Wide header shows Open label and chevron
- **WHEN** the main window has enough header width for the wide Open control
- **THEN** the header Open control is a menu button rather than a direct file-chooser button
- **AND** it displays the `Open` label with a down-chevron indicator
- **AND** activating the button opens the recent-document popover

#### Scenario: Constrained header falls back to folder icon
- **WHEN** the main window is in a constrained header width where the wide Open control would crowd adjacent controls
- **THEN** the header Open control uses a folder-symbolic icon presentation
- **AND** activating the icon opens the same recent-document popover
- **AND** the control remains reachable by keyboard and accessibility APIs

#### Scenario: File chooser action remains available
- **WHEN** the user activates the file-chooser action from the Open popover
- **THEN** the Open popover closes
- **AND** the normal Open File chooser opens through the existing file-open workflow

#### Scenario: Ctrl+O still opens the file chooser directly
- **WHEN** the main window has focus and the user presses `Ctrl+O`
- **THEN** LushText opens the normal Open File chooser directly
- **AND** it does not first require interacting with the recent-document list

### Requirement: Open Popover Uses GNOME Text Editor Layout
The Open popover SHALL use the GNOME Text Editor visual structure: a top search row, a compact file-chooser button, a separator, and a stack that switches between a recent-document list and a no-recents empty state. The popover MUST NOT use fake rows or unrelated workspace/open-tab context to satisfy empty, dense, or visual states.

#### Scenario: Popover opens with fixed search and chooser controls
- **WHEN** the user opens the Open popover
- **THEN** the search entry is visible at the top of the popover
- **AND** the compact file-chooser button is visible beside the search entry
- **AND** a separator divides the fixed controls from the list or empty state

#### Scenario: No recent documents shows readable empty state
- **WHEN** the recent-document model has no eligible rows
- **AND** the user opens the Open popover
- **THEN** the popover shows a readable empty state with a recent-document icon and `No Recent Documents` text
- **AND** the search entry and file-chooser action remain visible and reachable
- **AND** no fake recent row is inserted

#### Scenario: Representative rows use GNOME-style row content
- **WHEN** the recent-document model contains representative local file-backed documents
- **THEN** each recent row shows a title derived from the file name
- **AND** each row shows a secondary location or context line
- **AND** long titles and paths ellipsize without creating a horizontal scrollbar

#### Scenario: Row remove control is compact and non-disruptive
- **WHEN** a recent row exposes a remove control
- **THEN** the control is a compact flat circular close button
- **AND** activating it removes only that recent row from the recent-document model
- **AND** it does not open the document or close the popover

### Requirement: Recent Document Model Preserves Full Searchable History
The system SHALL maintain an app-owned recent-document model sorted newest first. The model MAY contain more entries than are visible at once, MUST deduplicate repeated paths by moving the latest successful open to the top, and MUST exclude already-open file-backed documents from the visible recent rows.

#### Scenario: Successful local opens are recorded
- **WHEN** a local file-backed document opens successfully through the file chooser, recent row, sidebar, command palette, desktop activation, or CLI activation path selected for this feature
- **THEN** the document becomes eligible for the recent-document model
- **AND** its latest successful open time is used for newest-first ordering

#### Scenario: Duplicate recent path moves to top
- **WHEN** a path already exists in recent-document persistence
- **AND** the same path opens successfully again
- **THEN** the recent-document model contains a single row for that path
- **AND** that row is ordered as the newest matching entry

#### Scenario: Already-open documents are hidden from recents
- **WHEN** a file-backed document is already open in a tab
- **THEN** the Open popover recent list does not show that document as a duplicate recent row
- **AND** the document remains reachable through the tab bar and existing open-document workflows

#### Scenario: Closed documents can reappear in recents
- **WHEN** a file-backed document is closed without being removed from recent history
- **THEN** that document can appear in the Open popover recent list again
- **AND** activating it routes through the normal duplicate-safe `open_document()` workflow

#### Scenario: Missing or unsupported paths do not create dead rows
- **WHEN** recent-document persistence contains a missing local path or unsupported non-local URI
- **THEN** the Open popover does not show an activation row that would silently fail
- **AND** pruning or diagnostics do not block the GTK main thread

### Requirement: Search And Activation Match GNOME Text Editor Behavior
The Open popover SHALL focus search on open, clear stale search text, reset list scroll to the top, filter rows as the user types, and open recent documents through existing document-open workflows. Keyboard and pointer activation MUST close the popover exactly once after a successful activation path starts.

#### Scenario: Opening popover resets search and scroll
- **WHEN** the Open popover is opened after a previous search or scroll position
- **THEN** the search entry text is empty
- **AND** the recent list is scrolled to the top
- **AND** keyboard focus is in the search entry

#### Scenario: Search filters by title and path context
- **WHEN** the user types text in the Open popover search entry
- **THEN** visible rows are filtered by case-insensitive matching against document title and path context
- **AND** prefix, substring, and fuzzy matches are ranked ahead of non-matches

#### Scenario: Enter opens first visible match
- **WHEN** the Open popover search entry contains a query with at least one visible result
- **AND** the user presses `Enter`
- **THEN** LushText opens the first visible result through the normal document-open workflow
- **AND** the popover closes
- **AND** the search text is cleared for the next open

#### Scenario: Single row activation opens document
- **WHEN** the Open popover shows recent rows
- **AND** the user activates a row by pointer or keyboard
- **THEN** LushText opens or focuses that document through the normal document-open workflow
- **AND** the popover closes

#### Scenario: Empty search result keeps chooser reachable
- **WHEN** the user types a query that matches no recent rows
- **THEN** the popover remains open with an empty or no-results state
- **AND** the search entry and file-chooser action remain reachable
- **AND** pressing `Enter` does not open an unrelated document

### Requirement: Keyboard Navigation And Dismissal Are Stable
The Open popover SHALL be fully usable with the keyboard. `Ctrl+K` MUST open the popover and focus search. Arrow navigation MUST move predictably between the search entry and rows. Escape or search cancellation MUST dismiss the popover without disturbing document state and should restore focus to the active editor when one exists.

#### Scenario: Ctrl+K opens recent search
- **WHEN** the main window has focus and the user presses `Ctrl+K`
- **THEN** the Open popover opens
- **AND** keyboard focus is in the popover search entry

#### Scenario: Down from search moves to first row
- **WHEN** the Open popover search entry has focus
- **AND** at least one visible row exists
- **AND** the user presses `Down`
- **THEN** focus moves to the first visible recent row

#### Scenario: Up from first row returns to search
- **WHEN** the first visible recent row has keyboard focus
- **AND** the user presses `Up`
- **THEN** focus returns to the search entry
- **AND** the popover remains open

#### Scenario: Escape closes without state mutation
- **WHEN** the Open popover is visible
- **AND** the user presses `Escape`
- **THEN** the popover closes
- **AND** no document is opened, closed, saved, or modified
- **AND** focus returns to the active editor when an active editor exists

#### Scenario: Dismissal works with no document context
- **WHEN** the Open popover is visible with no open document tab or workspace folder
- **AND** the user presses `Escape`
- **THEN** the popover closes cleanly
- **AND** no unrelated document, workspace, or fake row is required

### Requirement: Ten Visible Rows Fit Before Item-Region Scrolling
At default GTK text scale, the Open popover SHALL show 10 recent rows before the recent-list region scrolls. The full recent-document model MUST remain available through list scrolling and search. On a 720p display, the popover MUST fit with LushText's header bar present and MUST keep the search row, chooser action, and list region coherent.

#### Scenario: Exactly ten rows are visible without list scrolling
- **WHEN** the recent-document model contains exactly 10 eligible rows
- **AND** the Open popover is opened at default text scale
- **THEN** all 10 rows are visible without scrolling the list region
- **AND** no vertical scrollbar is needed to reach any of those 10 rows

#### Scenario: Eleventh row requires item-region scrolling
- **WHEN** the recent-document model contains at least 11 eligible rows
- **AND** the Open popover is opened at default text scale
- **THEN** only 10 rows are visible at one time before scrolling
- **AND** the eleventh and later rows are reachable by scrolling the recent-list region
- **AND** the search entry, file-chooser button, separator, and popover chrome do not scroll away

#### Scenario: Full history remains searchable beyond visible rows
- **WHEN** the recent-document model contains more than 10 eligible rows
- **AND** the user searches for a row that is not initially visible
- **THEN** the matching row can become visible through filtering
- **AND** the row can be opened without manually scrolling to its original position

#### Scenario: Ten-row popover fits on 720p display
- **WHEN** LushText runs in a 720p-height display or visual smoke fixture at default text scale
- **AND** the Open popover contains at least 10 eligible recent rows
- **THEN** the popover fits within the visible window/display area with the app header bar present
- **AND** the search row, chooser action, all 10 visible rows, and popover border are not clipped or overlapped

#### Scenario: Awkward labels do not create unintended scrollbars
- **WHEN** recent rows contain long file names, deep paths, spaces, symbols, or mixed-width text
- **THEN** row text ellipsizes within the popover width
- **AND** no horizontal scrollbar appears
- **AND** row height remains within the tested 10-row viewport contract at default text scale

### Requirement: Open Popover Exposes Stable Accessibility And Automation Anchors
The Open popover SHALL expose meaningful accessible names, roles, and states for the header menu button, search entry, file-chooser action, list rows, remove controls, empty state, and scrollable recent-list region. Documentation and automation references MUST be updated when these anchors or public actions change.

#### Scenario: Header Open control is accessible
- **WHEN** the main window is visible
- **THEN** the header Open menu button exposes a stable accessible name and role describing the recent Open workflow
- **AND** the accessible meaning remains stable in both wide and constrained icon presentations

#### Scenario: Popover controls are accessible
- **WHEN** the Open popover is visible
- **THEN** the search entry, file-chooser button, recent-list region, recent rows, row remove controls, and empty state expose meaningful accessible names or roles
- **AND** keyboard focus order reaches the primary controls without requiring pointer input

#### Scenario: Automation documentation is synchronized
- **WHEN** new actions, accessible anchors, readiness fields, or automation-observable Open popover states are added or changed
- **THEN** the action catalog and automation documentation are updated in the same change
- **AND** documentation drift checks fail if they are stale

### Requirement: Open Popover Verification Covers Subtle UI States
The implementation SHALL include broad tests and smoke coverage for the Open popover's subtle behavior. Coverage MUST include model/service tests, widget tests, keyboard tests, accessibility checks, visual geometry proof, and regression tests for state extremes.

#### Scenario: Model and persistence tests cover recent history rules
- **WHEN** the recent-document model and service tests run
- **THEN** they cover ordering, deduplication, missing-path pruning, open-tab exclusion, search matching, unsupported URI rejection, persistence load/save, and corrupt-file recovery

#### Scenario: Widget tests cover state matrix
- **WHEN** Open popover widget tests run
- **THEN** they cover no recents, one row, representative rows, exactly 10 rows, 11 or more rows, awkward labels, filtered no-results, stale duplicate rows, no active editor, and constrained geometry

#### Scenario: Keyboard tests cover subtle navigation
- **WHEN** Open popover keyboard tests run
- **THEN** they cover `Ctrl+K`, search focus, stale search clearing, top scroll reset, `Enter` first-match activation, `Down` from search, `Up` from first row, `Escape` dismissal, and file chooser button routing

#### Scenario: Visual proof covers rendered geometry
- **WHEN** visual geometry or smoke proof runs for this change
- **THEN** it captures the GNOME-style Open button and popover
- **AND** it verifies 10 visible rows, item-region-only scrolling, readable empty state, no unintended horizontal scrollbar, and 720p fit with header chrome present

#### Scenario: Accessibility smoke covers visible anchors
- **WHEN** the accessibility smoke lane runs for the Open popover
- **THEN** it verifies stable accessible names, roles, focus order, and dismissibility for empty, representative, dense, and constrained states

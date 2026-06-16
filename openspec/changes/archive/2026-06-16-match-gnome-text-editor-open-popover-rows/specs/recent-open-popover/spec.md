## ADDED Requirements

### Requirement: Open Popover Recent Rows Match GNOME Text Editor 50.1 Source
The Open popover recent-list rows SHALL match GNOME Text Editor 50.1's source-defined row structure and interaction styling. LushText MUST preserve its own recent-document model and activation callbacks, but row layout, row CSS, scroller sizing, and highlight behavior MUST be source-compatible with GNOME Text Editor 50.1 rather than merely approximate.

#### Scenario: Recent list uses no selection accent state
- **WHEN** the Open popover shows eligible recent rows
- **THEN** the list model uses a no-selection interaction model equivalent to GNOME Text Editor's `GtkNoSelection`
- **AND** keyboard focus, hover, and activation do not create a persistent selected/accent-colored recent row
- **AND** row activation resolves the document by visible list position rather than selected-row state

#### Scenario: Recent row widget has GNOME source-compatible structure
- **WHEN** a recent row is bound for display
- **THEN** the row child uses a grid-shaped structure equivalent to GNOME Text Editor 50.1's `EditorSidebarRow`
- **AND** the grid contains a leading homogeneous marker/spacer column before the text columns
- **AND** the title, subtitle, and optional age text use GTK text widgets with source-compatible `GtkInscription` overflow behavior
- **AND** the remove button occupies the trailing column and spans both text rows

#### Scenario: Row text and spacing match GNOME source rules
- **WHEN** a recent row contains a normal title, long title, deep path, mixed-width text, spaces, or symbols
- **THEN** the title middle-ellipsizes like GNOME Text Editor's title inscription
- **AND** the subtitle end-ellipsizes with `caption` and `dim-label` styling
- **AND** row grid margins, row spacing, column spacing, row margins, first-row top margin, row border radius, and button padding match the GNOME Text Editor 50.1 source constants
- **AND** no horizontal scrollbar appears because of row content

#### Scenario: Remove control matches GNOME placement and behavior
- **WHEN** a recent row exposes the remove control
- **THEN** the control is a trailing `window-close-symbolic` button with `flat` and `circular` styling
- **AND** its minimum size, padding, margin, vertical alignment, and two-row span match GNOME Text Editor 50.1
- **AND** activating the control removes only that recent entry from LushText's recent-document model
- **AND** activating the control does not activate the row or close the Open popover

#### Scenario: Recent scroller matches GNOME source sizing
- **WHEN** the Open popover renders the recent-list stack child
- **THEN** the recent-list scroller uses GNOME Text Editor 50.1-compatible sizing, including 250px minimum and maximum content width, 600px maximum content height, natural-height propagation, vertical expansion, and no horizontal scrollbar
- **AND** only the recent-list item region scrolls when the visible rows exceed the scroller height
- **AND** the search entry, file-chooser button, separator, and popover border remain fixed and visible

#### Scenario: GNOME row parity holds across visible state extremes
- **WHEN** Open popover visual, geometry, and widget proof runs
- **THEN** it covers no recents, one recent, representative recents, exactly ten recents, more than ten recents, awkward labels, all recents currently open, all recents closed, constrained header width, constrained popover geometry, and light and dark style contexts
- **AND** each populated case verifies GNOME-compatible row structure, no selected/accent row state, close-button alignment, readable text, item-region-only scrolling, and absence of unintended horizontal scrollbars

## MODIFIED Requirements

### Requirement: Open Popover Verification Covers Subtle UI States
The implementation SHALL include broad tests and smoke coverage for the Open popover's subtle behavior. Coverage MUST include model/service tests, widget tests, keyboard tests, accessibility checks, visual geometry proof, source-compatible GNOME Text Editor row parity checks, and regression tests for state extremes.

#### Scenario: Model and persistence tests cover recent history rules
- **WHEN** the recent-document model and service tests run
- **THEN** they cover ordering, deduplication, missing-path pruning, open-tab exclusion, search matching, unsupported URI rejection, persistence load/save, corrupt-file recovery, duplicate path spelling, and reopen-after-close behavior

#### Scenario: Widget tests cover state matrix
- **WHEN** Open popover widget tests run
- **THEN** they cover no recents, one row, representative rows, exactly 10 rows, 11 or more rows, awkward labels, filtered no-results, stale duplicate rows, no active editor, all recents open, all recents closed, constrained geometry, and light and dark style contexts
- **AND** they assert the recent list uses no-selection row interaction, source-compatible row child structure, source-compatible text overflow widgets, and source-compatible remove-button placement

#### Scenario: Keyboard tests cover subtle navigation
- **WHEN** Open popover keyboard tests run
- **THEN** they cover `Ctrl+K`, search focus, stale search clearing, top scroll reset, `Enter` first-match activation, `Down` from search, `Up` from first row, row activation by position, activation after filtering, `Escape` dismissal, and file chooser button routing
- **AND** they prove keyboard navigation does not rely on persistent selected-row state

#### Scenario: Pointer and remove tests cover event separation
- **WHEN** pointer activation and remove-control regression tests run
- **THEN** they cover single-click row activation, close-button activation, close-button activation while the row is focused, removal while the popover is visible, and repeated removals until the empty state appears
- **AND** they prove remove-button activation never opens the document and never closes the popover

#### Scenario: Visual proof covers rendered geometry
- **WHEN** visual geometry or smoke proof runs for this change
- **THEN** it captures the GNOME-style Open button and popover
- **AND** it verifies GNOME Text Editor 50.1-compatible recent-row margins, spacing, first-row offset, border radius, close-button size and alignment, 250px list content width, 600px list max height, item-region-only scrolling, readable empty state, no selected/accent row state, no unintended horizontal scrollbar, and 720p fit with header chrome present

#### Scenario: Accessibility smoke covers visible anchors
- **WHEN** the accessibility smoke lane runs for the Open popover
- **THEN** it verifies stable accessible names, roles, focus order, and dismissibility for empty, representative, dense, all-open, all-closed, constrained, and filtered states
- **AND** it verifies the row and remove-control accessible labels remain meaningful after adopting the GNOME-shaped row widget

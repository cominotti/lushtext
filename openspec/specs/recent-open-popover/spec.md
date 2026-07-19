# recent-open-popover Specification

## Purpose
Define the GNOME Text Editor-style Open header control, recent-document popover, recent-history behavior, keyboard and accessibility contract, ten-row viewport sizing, and verification expectations.

## Requirements
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

### Requirement: Open Popover Recent Rows Reveal Full Paths On Hover
The Open popover SHALL expose the full absolute activation path for every visible recent-document row through row hover tooltip text. The tooltip MUST use the same path that row activation opens, MUST remain complete and unellipsized even when row title or subtitle text is visually ellipsized, and MUST NOT change GNOME-style row layout, scrolling, or remove-control behavior.

#### Scenario: Hovering a recent row shows the full activation path
- **WHEN** the Open popover shows a recent row for a local file-backed document
- **THEN** hovering the row's non-action surface exposes tooltip text equal to the full absolute path that activating the row opens
- **AND** the tooltip text is not shortened to the title, parent directory, canonical identity, or visually ellipsized row subtitle
- **AND** the row keeps the existing compact GNOME-style layout and no-horizontal-scrollbar behavior

#### Scenario: Awkward and deep paths remain inspectable
- **WHEN** a visible recent row represents a file with a long absolute path containing deep directories, spaces, symbols, or mixed-width text
- **THEN** the row hover tooltip exposes the complete absolute path string for that file
- **AND** the row title and subtitle may still ellipsize inside the popover width without changing the tooltip string

#### Scenario: Remove control keeps its action tooltip
- **WHEN** a visible recent row exposes its trailing remove control
- **THEN** hovering the remove control exposes the remove action tooltip
- **AND** the remove control tooltip is not replaced by the document path tooltip
- **AND** activating the remove control still removes only that recent row without opening the document or closing the Open popover

#### Scenario: Recycled recent rows refresh tooltip text
- **WHEN** the recent list contains enough rows, filtering, or scrolling to cause GTK to reuse row widgets for different recent documents
- **THEN** every rebound row exposes tooltip text for the currently bound row's full absolute activation path
- **AND** no visible row exposes a tooltip from a previously bound recent document

#### Scenario: Empty and filtered states do not create fake path tooltips
- **WHEN** the Open popover has no eligible recent rows or the current search filters every recent row out
- **THEN** no fake recent row is inserted to provide path tooltip behavior
- **AND** the search entry and file-chooser action remain visible and reachable

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

### Requirement: Recent Visibility Follows Live Tab Reality
The Open popover SHALL exclude only file-backed documents that are currently mounted in live editor tabs. Stale duplicate-detection, canonical-path, failed-load, or previously detached tab bookkeeping MUST NOT hide persisted recent documents once no matching live tab remains.

#### Scenario: Closed same-session documents reappear despite stale identity bookkeeping
- **WHEN** a local file-backed document is opened successfully and recorded as recent
- **AND** every tab for that document is closed in the same application session
- **THEN** opening the Open popover shows that document as an eligible recent row
- **AND** no stale open-path or canonical-path identity keeps it hidden

#### Scenario: Startup-loaded recents remain visible with no restored tabs
- **WHEN** recent-document persistence contains existing local file-backed documents
- **AND** LushText starts with no restored file-backed tabs
- **THEN** opening the Open popover shows those persisted rows
- **AND** the empty state is not shown as a substitute for valid rows

#### Scenario: Real open and close workflows keep recents synchronized
- **WHEN** a document is opened through file chooser, sidebar, command palette, desktop activation, CLI activation, or recent-row activation
- **AND** the tab is later closed through tab close, close action, close-tab-for-path, bulk close, or delete/rename workflows
- **THEN** the Open popover visibility filter reflects the remaining live tabs only
- **AND** closed recent rows reappear without restarting LushText

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

### Requirement: Recent Open uses shared fuzzy scoring within explicit tiers
The system SHALL rank non-empty Recent Open queries through case-insensitive Prefix, Substring, and Fuzzy tiers in that order. Fuzzy-tier matches MUST use the same GTK-free nucleo query configuration as the command palette and MUST sort by descending nucleo score before recency and path tie-breaks.

#### Scenario: Prefix outranks stronger fuzzy candidate
- **WHEN** one recent row has a case-insensitive field prefix match and another has only a high-scoring fuzzy match
- **THEN** the prefix row appears first
- **AND** nucleo score does not cross the explicit tier boundary

#### Scenario: Substring outranks fuzzy candidate
- **WHEN** one recent row contains the query as a case-insensitive substring and another matches only as a subsequence
- **THEN** the substring row appears first

#### Scenario: Better fuzzy score outranks newer weak fuzzy match
- **WHEN** two recent rows match only in the fuzzy tier and one receives a higher nucleo score
- **THEN** the higher-scoring row appears first
- **AND** recency is consulted only after equal fuzzy score

#### Scenario: Best matching field determines the row tier
- **WHEN** a row matches fuzzily by title but as a substring in its subtitle or path
- **THEN** the row receives the Substring tier
- **AND** it is not ranked only from its weaker title match

### Requirement: Recent ranking remains deterministic and bounded
The system SHALL preserve newest-first results for an empty trimmed query, the 200-entry recent-history cap, open-tab exclusion, and no-result behavior. Equal non-empty ranks MUST sort by descending last-opened timestamp and then ascending path so repeated searches return stable ordering.

#### Scenario: Empty query stays newest-first
- **WHEN** the Open popover query is empty or whitespace-only
- **THEN** eligible recent rows are ordered newest first
- **AND** equal timestamps are ordered deterministically by path
- **AND** no fuzzy scorer is required to produce the list

#### Scenario: Equal fuzzy ranks use recency
- **WHEN** two rows have the same fuzzy tier and nucleo score
- **THEN** the more recently opened row appears first

#### Scenario: Equal scores and timestamps use path
- **WHEN** two rows have equal tier, fuzzy score, and last-opened timestamp
- **THEN** ascending path order determines their stable order

#### Scenario: No candidate matches
- **WHEN** prefix, substring, and shared fuzzy scoring reject every eligible recent row
- **THEN** the Open popover shows its existing no-results state
- **AND** the file chooser and search controls remain reachable

### Requirement: Shared fuzzy abstraction remains GTK-free and concrete
The project SHALL keep reusable nucleo query state in a GTK-free service module as one concrete helper. Palette and Recent Open MAY apply different higher-level tier or grouping policies, but MUST use the same case and normalization configuration for true fuzzy score calculation. The change MUST NOT introduce a generic matcher trait, global mutable matcher, or UI dependency.

#### Scenario: Palette and recents score the same candidate
- **WHEN** a cross-surface fixture passes the same non-empty query and candidate to palette fuzzy scoring and Recent Open's fuzzy tier
- **THEN** both receive the same nucleo match acceptance and score
- **AND** each surface may still apply its own surrounding ordering policy

#### Scenario: Independent queries do not share mutable state
- **WHEN** palette and Recent Open searches run with different queries
- **THEN** each query owns its matcher and conversion buffer
- **AND** one search cannot change another search's score results

### Requirement: Fuzzy ranking coverage includes realistic state extremes
The project SHALL add pure service and Open-popover regression tests for empty, one-row, representative, many-row, no-result, Unicode, composed/decomposed text, mixed case, deep and awkward paths, equal timestamps, equal scores, and all-recents-open states.

#### Scenario: Unicode and awkward paths remain searchable
- **WHEN** recent titles or paths contain accented Unicode, spaces, symbols, or deep components
- **THEN** matching follows the shared case and normalization policy
- **AND** result rows preserve their existing readable, ellipsized presentation

#### Scenario: All matching recents are already open
- **WHEN** ranking would match rows but open-tab exclusion removes every one
- **THEN** the popover shows its established empty eligible state
- **AND** ranking does not reintroduce open documents as fake recent rows

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

### Requirement: Recent Open Regression Coverage Is Broad
The implementation SHALL include regression coverage across pure services, window state, GTK widgets, D-Bus/automation action paths, visual geometry, and accessibility-relevant popover states for recent-open synchronization.

#### Scenario: Regression tests cover stale identity edge cases
- **WHEN** the recent-open regression suite runs
- **THEN** it covers stale display paths, stale canonical paths, duplicate path spellings, failed loads, cancelled loads, Save As, sidebar rename/delete, session restore, app startup from persisted recents, open while the popover is visible, close while the popover is visible, and multiple recent rows where all or only some are still open

#### Scenario: Regression tests cover visible state extremes
- **WHEN** Open popover UI and smoke tests run
- **THEN** they cover no eligible recents, one closed recent, representative recents, many recents, awkward/deep path labels, all recent documents currently open, all recent documents closed, constrained geometry, keyboard navigation, accessible roles/names, and item-region-only scrolling

### Requirement: Recent-document metadata limits are enforced during ingestion
Recent-document loading SHALL use the filesystem boundary's bounded byte reader with MAX_RECENT_DOCUMENTS_BYTES. Metadata MAY reject an already oversized file early but MUST NOT be the allocation boundary; exact-limit input MUST reach ordinary JSON parsing, while content that exceeds the cap before or during the read MUST be rejected without allocating or parsing the enlarged body.

#### Scenario: Metadata file is exactly at the cap
- **WHEN** the recent-document file contains exactly MAX_RECENT_DOCUMENTS_BYTES
- **THEN** bounded ingestion accepts it for normal JSON validation
- **AND** size alone does not reset valid exact-limit content

#### Scenario: File grows after metadata inspection
- **WHEN** the file appears within the cap during metadata inspection but grows beyond it before or during ingestion
- **THEN** the read stops at the bounded limit without allocating the full enlarged body
- **AND** the oversized body is not passed to the JSON parser

#### Scenario: Oversized metadata is recovered
- **WHEN** recent-document persistence exceeds the cap
- **THEN** the service applies its existing reset or prune recovery policy and emits a bounded diagnostic
- **AND** the popover remains usable with an empty or recovered model

#### Scenario: Missing or malformed metadata is loaded
- **WHEN** the file is missing or bounded input is invalid JSON
- **THEN** the established missing-file and corruption-recovery behavior remains unchanged
- **AND** no raw filesystem read bypass is introduced

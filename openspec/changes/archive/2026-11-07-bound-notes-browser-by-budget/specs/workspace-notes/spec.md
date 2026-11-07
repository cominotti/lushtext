## RENAMED Requirements

- FROM: `### Requirement: Folder-note browser entries use the native Adwaita sidebar rail`
- TO: `### Requirement: Folder-note browser entries use the virtualized sidebar rail`

## MODIFIED Requirements

### Requirement: Folder-note browser entries use the virtualized sidebar rail
The system SHALL present folder-note entries in the workspace-scoped `Browse Notes...` surface in a dedicated `Folder Notes` section of the browser's virtualized sidebar rail: a `GtkListView` styled `navigation-sidebar` whose section titles come from a header factory and whose row widgets are created only for rows on or near the screen. The rail MUST NOT build one widget per off-screen row. The section MUST preserve workspace-scope filtering, folder-note preview, preview-only pointer selection, and explicit Open behavior.

#### Scenario: Browse folder notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more folder notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those folder notes in a dedicated `Folder Notes` section of the sidebar rail
- **AND** each folder-note item identifies the workspace folder it belongs to

#### Scenario: Preview a folder note from the sidebar rail
- **WHEN** the user selects a folder-note item in the Notes browser sidebar rail
- **THEN** the browser updates the preview pane with that folder note's rendered Markdown content or explicit empty-note state
- **AND** the Open action targets the selected folder note

#### Scenario: Click a folder-note item without opening the editor
- **WHEN** the user clicks a folder-note item in the Notes browser sidebar rail
- **THEN** the browser updates the selected item and preview pane only
- **AND** the folder-note editing surface is not opened

#### Scenario: Open a folder note explicitly from the browser
- **WHEN** the user selects a folder-note item and invokes the browser's Open action
- **THEN** the system opens that folder note's editing surface
- **AND** the Open action does not require an active document tab

#### Scenario: Filtered folder-note state remains explicit
- **WHEN** the Notes browser search text filters out every folder-note item
- **THEN** the folder-note sidebar section no longer shows matching items
- **AND** if no notes of any kind match, the browser shows an explicit empty filtered state

### Requirement: Notes browser source construction and querying remain bounded
The system SHALL construct the `Browse Notes...` source with explicit searchable-text byte, retained-graph byte, sidecar path-byte, construction-scratch byte, open-editor snapshot byte, and recovery-diagnostic limits. The browser source SHALL NOT be limited by a row count, a sidecar count, or an open-editor snapshot count. A row that does not fit a byte limit SHALL be skipped while later rows that fit are still admitted, so the browser never retains fewer rows than the count-and-byte rule it replaces, which stopped construction at its first refusal. The command palette's note source keeps stopping at its first refusal. The open-editor snapshot SHALL be captured on GTK in time-sliced turns admitted against its retained-byte limit, and each editor's bookmarks SHALL be captured from one consistent bookmark generation. The browser SHALL retain one bounded immutable source, SHALL execute note-body matching and category grouping outside GTK, and SHALL retain at most one active query plus one latest compact superseding query. A source cut MUST be explicit without changing workspace-scope, section ordering, canonical de-duplication, preview, or Open semantics for admitted rows.

#### Scenario: Aggregate Notes source exceeds admission limits
- **WHEN** bookmarks, folder notes, document notes, and eligible open-tab rows exceed a browser byte limit
- **THEN** source construction retains no more than the configured byte budgets, and rows that fit are still admitted after a row that did not
- **AND** construction keeps counting the rows it parses without retaining them, and the browser states how many rows it shows of how many it found instead of presenting the admitted source as complete

#### Scenario: A byte cut in one category does not hide the later categories
- **WHEN** the browser's searchable-text limit is reached while bookmarks are admitted, and folder notes and document notes that fit the remaining budgets exist
- **THEN** those folder notes and document notes are still admitted and listed in their sections
- **AND** the limited state counts the refused bookmarks among the rows found

#### Scenario: Open editors contain many bookmark rows
- **WHEN** open editors hold more bookmark rows than one GTK turn can snapshot within the turn slice
- **THEN** the snapshot is captured over several turns, each stopping at the first unit started at or after the slice, and admission stops at the open-editor retained-byte limit with no row-count limit
- **AND** an editor whose bookmarks change between its slices is captured again, so no editor's bookmarks are mixed from two generations
- **AND** no `usize::MAX` or equivalent unbounded collection bypasses byte admission

#### Scenario: Queries change faster than matching completes
- **WHEN** the user types several Notes queries while an earlier full-source match is active
- **THEN** the active query is cancelled cooperatively and only the latest compact pending query is retained
- **AND** stale matches never replace the listed rows or change the selected preview

#### Scenario: Current query exceeds the render limit
- **WHEN** the current background match finds more rows than the 512-row render cap that earlier versions applied
- **THEN** every matching admitted row is published and listed, grouped by section, with no render cap and no refine-your-search notice
- **AND** publishing the result costs GTK work that does not grow with the number of matching rows

#### Scenario: Notes browser closes during source or query work
- **WHEN** the dialog is disposed while the sliced snapshot capture, bounded source construction, or query matching is active
- **THEN** current work is cancelled or discarded without a later GTK callback, and the snapshot turn source is removed
- **AND** retained source, pending query, and result payloads are released

### Requirement: Bookmark-only browsing reuses the bounded Notes pipeline
The dedicated bookmark browser SHALL be a generation-scoped bookmark-only mode of the unified Browse Notes inventory, query, projection, and disposal workflow. It MUST preserve the existing Show Bookmarks action and bookmark-specific activation, scope, empty, limited-state, recovery, keyboard, and accessibility behavior without retaining a separate uncapped loader or synchronous widget-rebuild path.

#### Scenario: Show Bookmarks opens
- **WHEN** the user activates the existing bookmark-browser action
- **THEN** the unified Notes workflow starts with a bookmark-only source filter and current workspace scope
- **AND** document notes and folder notes cannot appear in the result inventory

#### Scenario: Bookmark inventory is large
- **WHEN** bookmark sidecars and open editors contain tens of thousands of bookmark rows
- **THEN** source loading, matching, and GTK projection obey the same byte, active-plus-latest, turn-slice, and disposal bounds as Browse Notes, and every admitted bookmark is listed
- **AND** GTK does not synchronously scan the full source or build a row widget tree for any bookmark that is not on or near the screen

#### Scenario: Bookmark query has no matches
- **WHEN** a bookmark-only query yields no accepted rows
- **THEN** the browser reaches the bookmark-specific empty state through bounded query completion
- **AND** the main loop remains responsive while prior rows retire

#### Scenario: A bookmark row is activated
- **WHEN** the user activates a current bookmark-only result
- **THEN** the existing bookmark navigation semantics open or focus the file and line
- **AND** a stale generation cannot activate a replaced row

#### Scenario: Bookmark metadata is malformed or truncated
- **WHEN** bounded inventory loading encounters recovery diagnostics or a byte limit
- **THEN** the browser exposes the existing accessible recovery state, or the limited state stating the bookmarks shown of those found
- **AND** valid admitted bookmarks remain usable

#### Scenario: Production bookmark inventory is requested
- **WHEN** an interactive caller constructs the bookmark-only inventory
- **THEN** it must supply the unified Notes byte budgets, generation, and cancellation policy
- **AND** no unrestricted aggregate bookmark-vector API remains available to production UI code

### Requirement: Notes source construction scratch is byte-bounded
The unified Notes source loader SHALL enforce explicit conservative byte ceilings for concurrently retained construction scratch and sidecar traversal paths in addition to the searchable-text, final retained-source, open-editor, and diagnostic limits. Row-count and sidecar-count limits SHALL be optional per consumer: the command palette supplies them, and the Notes browser does not. No consumer can omit a byte ceiling. Accounting MUST use saturating arithmetic and MUST include the current recovery-aware sidecar input, retained path batch, canonical identity copies, diagnostic storage, temporary category/capacity ownership, and other construction allocations that overlap the final source. Reaching a construction or path ceiling MUST stop at a deterministic complete boundary and publish a distinct typed truncation reason with compact current/peak metrics, and the count of rows found MUST then be marked as a lower bound.

#### Scenario: Sidecar directory contains long Unicode paths
- **WHEN** a sidecar directory's paths would exceed the traversal path-byte ceiling, below any sidecar count a consumer supplies
- **THEN** the byte-bounded scanner retains only complete entries within the applicable limits
- **AND** source feedback distinguishes path-byte truncation from sidecar-count truncation

#### Scenario: One near-limit sidecar overlaps admitted rows
- **WHEN** recovery-aware loading holds a sidecar input near its metadata byte limit while final rows and construction scratch already exist
- **THEN** measured peak construction ownership remains within the documented scratch ceiling
- **AND** the loader stops before another complete allocation would exceed that ceiling

#### Scenario: Diagnostics and canonicalization consume scratch
- **WHEN** malformed sidecars and many folder identities produce bounded recovery diagnostics and canonical path copies
- **THEN** those allocations contribute to construction metrics rather than bypassing admission
- **AND** valid rows admitted before the deterministic boundary remain ordered, browsable, and activatable

#### Scenario: Construction is cancelled
- **WHEN** source generation is superseded or the Notes browser closes during sidecar traversal or parsing
- **THEN** cancellation releases path, sidecar, diagnostic, and category scratch on the worker
- **AND** no large construction allocation crosses to GTK in the cancelled outcome

#### Scenario: Final source reaches GTK
- **WHEN** bounded construction completes with admitted rows
- **THEN** only the final measured retained source and compact metrics, including the rows found and whether that count is a lower bound, cross to GTK under the existing progress reservation
- **AND** construction scratch has already been released and is not hidden inside diagnostic payloads

## ADDED Requirements

### Requirement: Notes browser lists every admitted row in a virtualized list
The Notes browser SHALL list every row its source admitted and its current query matched, in both Browse Notes and Browse Bookmarks modes, grouped into the `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tabs` sections in that order, with no row-count cap. The list SHALL create row widgets only for rows on or near the screen, and SHALL create row items on demand from the published grouped indexes rather than one object per row on GTK. Publishing a result SHALL cost the same whatever the number of rows: the row items the list holds SHALL be updated in place, and only the difference in length SHALL be announced, as a removal or as an addition, never as one change that replaces realized rows, which makes the toolkit look up every added row (ledger A93). The flat visible row order, with section headers excluded, SHALL remain the index space of `win.select-notes-browser-row`. Selection SHALL drive the preview, activating a row SHALL select and preview it without opening it, and the Open button and a bookmark row's Open button SHALL open it. Arrow keys SHALL move the selection across section boundaries, and section headers SHALL never take focus or selection.

#### Scenario: Thousands of matching rows are all reachable
- **WHEN** the source admits 20,000 bookmarks and the query matches all of them
- **THEN** the list reports 20,000 rows, the last bookmark can be reached by scrolling and by keyboard, and it is drawn on screen when reached
- **AND** no notice asks the user to refine the search to see more

#### Scenario: Off-screen rows build no widgets
- **WHEN** 131,072 rows are listed in a presented browser
- **THEN** the number of realized row widgets stays within the toolkit's realized-row cap regardless of the row count

#### Scenario: A previously selected row survives a query
- **WHEN** the selected row still matches after the query changes
- **THEN** it stays selected at its new position and its preview is not reloaded

#### Scenario: Keyboard navigation crosses a section boundary
- **WHEN** the last bookmark row is selected and the user presses Down
- **THEN** the first row of the next non-empty section is selected and previewed, and the section header is skipped

### Requirement: Notes browser states a byte-budget cut once
When a byte limit cuts the Notes browser source, the browser SHALL show a persistent label stating the rows shown and the rows found, in the wording of the mode: "Showing X of Y notes and bookmarks" or "Showing X of Y bookmarks", followed by a sentence saying the rest exceed the memory budget. When a scan ceiling stopped the count, the label SHALL say "of at least Y". While a query is active the label SHALL say that X of Y rows were searched. The browser SHALL raise one polite announcement when a source generation publishes as limited, and SHALL NOT announce it again for queries, selection changes, or re-presenting the same generation. When no byte limit cut the source, no limit label SHALL be shown.

#### Scenario: Source cut by the text budget
- **WHEN** the browser source reaches its searchable-text byte limit with rows still unparsed
- **THEN** the label states the rows shown of the rows found, and one announcement is raised
- **AND** typing queries afterwards raises no further limit announcement

#### Scenario: Count stopped by a scan ceiling
- **WHEN** the sidecar path-byte ceiling stops construction before every sidecar was parsed
- **THEN** the label says "of at least Y"

#### Scenario: Nothing was cut
- **WHEN** every row fits the byte limits
- **THEN** no limit label is visible, whatever the number of rows

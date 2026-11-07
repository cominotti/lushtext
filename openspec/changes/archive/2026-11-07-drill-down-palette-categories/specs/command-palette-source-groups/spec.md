## ADDED Requirements

### Requirement: A truncated palette category ends with a "More" row
The grouped command palette SHALL end every result group that holds fewer rows than the query matches in that category with one activatable "More" row. The row SHALL state how many matches the group does not show, with the count grouped in thousands, and SHALL be reachable with the arrow keys, Page Up and Page Down like any result row. A group that shows every match SHALL have no "More" row. The count SHALL be exact for every query, including the empty query: for a non-empty query it is the number of candidates that matched; for the empty query it is the number of items the category includes, with the same exclusions the group applies (open tabs excluded from the workspace group). Counting SHALL retain no row beyond the group's shown rows and SHALL observe cancellation at the selector's existing interval. The grouped view's rows, order, per-category cap, source priority and deduplication SHALL be unchanged.

#### Scenario: A broad query in a large workspace
- **WHEN** the workspace index holds 131,072 files, 40,000 of which match the query
- **THEN** the workspace group shows its 64 best matches followed by one row stating 39,936 more

#### Scenario: The empty query counts what it does not show
- **WHEN** the query is empty, the index holds 131,072 files and 32 of them are open in tabs
- **THEN** the workspace group shows its first 64 files in source order followed by one row stating 131,072 − 32 − 64 more
- **AND** no full-source result collection is materialized

#### Scenario: A category that fits shows no More row
- **WHEN** a category matches 64 or fewer items
- **THEN** its group ends with its last result row and no More row

#### Scenario: Every category is counted in All mode
- **WHEN** All mode shows open tabs, workspace files and each note category, each with more matches than it shows
- **THEN** each group ends with its own More row, and the group order and labels are unchanged
- **AND** the Commands group, whose 35 registered commands fit the cap, ends with no More row

#### Scenario: The rule is the same for every group
- **WHEN** the per-group limit is forced below a group's match count, for each group of each mode including Commands
- **THEN** that group ends with a More row stating the exact number not shown

### Requirement: A palette category view lists every ranked result of one category
Activating a "More" row SHALL show that category's view: only that category, every result it matches, ranked exactly as the grouped view ranks them (descending score, then source ordinal; source order for the empty query), so the view's first rows are the rows the grouped view showed. The view SHALL name its category and its result count, SHALL list its rows in a virtualized list whose model creates row objects only for rows the toolkit requests, and SHALL keep no per-row text for rows that are not shown. The ranked rows SHALL be bounded by a retained-byte budget through the shared list-budget policy, chosen so that the file index's largest admitted size fits it whole; if the budget cuts a category, the view SHALL say "Showing X of Y results". Activating a row in the view SHALL open the file, open the note or run the command exactly as the same row does in the grouped view.

#### Scenario: Opening the workspace category
- **WHEN** the user selects the workspace group's More row and presses Enter
- **THEN** the palette shows only workspace files, titled with the group's label and the number of matches
- **AND** its first 64 rows are the 64 rows the grouped view showed, in the same order

#### Scenario: Every result is reachable
- **WHEN** a category view lists 131,072 files
- **THEN** the user can reach and open the last file with the keyboard and with the pointer

#### Scenario: The budget cuts a category
- **WHEN** a category matches more rows than its retained-byte budget holds
- **THEN** the view lists the best-ranked rows the budget holds and says "Showing X of Y results"

#### Scenario: A note category view whose source was cut
- **WHEN** a note category view is shown and the palette's note source was cut by a byte ceiling
- **THEN** the view says how many notes the source holds of how many it found

### Requirement: The palette category view keeps the query live and returns to grouped results
In a category view, typing SHALL filter within the category through the palette's one query flight (the same debounce, generations, cancellation and one-active/one-latest coordinator), and a completion SHALL publish only if its generation is current and its category is still the one shown. Escape, a Backspace press on a query that was already empty (a key release having been seen since it became empty, so an auto-repeat of the Backspace that emptied it does not leave), the view's back button and a mode change SHALL return to the grouped view with the query kept (in the new mode for a mode change), and SHALL select that category's "More" row again when it still exists. Closing the palette SHALL leave the category view, and reopening SHALL show the grouped All view.

#### Scenario: Typing filters within the category
- **WHEN** a category view is shown and the user types a query
- **THEN** only that category's matches are listed, ranked for the new query

#### Scenario: Escape returns with the query kept
- **WHEN** a category view is shown with the query `rs` and the user presses Escape
- **THEN** the grouped view is shown for `rs`, the palette stays open, and the category's More row is selected

#### Scenario: Backspace on an empty query returns
- **WHEN** a category view is shown, the query is empty, and the user presses Backspace
- **THEN** the grouped view is shown and the query stays empty

#### Scenario: Holding Backspace to clear the query stays in the category
- **WHEN** a category view is shown with the query `rs` and the user holds Backspace, so its auto-repeat presses continue after the query is empty without a key release
- **THEN** the query is empty and the category view is still shown

#### Scenario: A stale completion after leaving publishes nothing
- **WHEN** a category query is running and the user returns to the grouped view before it completes
- **THEN** the category result is not published and the grouped result is

#### Scenario: Sources change while a category is shown
- **WHEN** the file index is replaced while the workspace category view is shown
- **THEN** the view is republished from the new index and stays in that category
- **AND** the selected row stays selected, clamped to the new length, while a new query selects the first row

## MODIFIED Requirements

### Requirement: Palette source inventories are bounded and superseding
The system SHALL keep palette source construction bounded before query scoring begins. File-index construction MUST retain at most 131,072 canonical files (a presentation cap rounded up from 100,000 by `power-of-two-thresholds`), MUST avoid materializing an unbounded flat directory, and MUST cooperatively cancel superseded traversal. Note-source construction MUST be bounded by its byte ceilings alone (64 MiB of aggregate searchable UTF-8 note text, a 64 MiB retained graph, 8 MiB of sidecar paths and 96 MiB of construction scratch) with no row or sidecar count, MUST capture open editors' bookmarks in sliced main-loop turns bounded by the 4 MiB open-editor snapshot budget with no row count, counting the rows that budget cannot retain, MUST skip and count a row a byte ceiling refuses while admitting later rows that fit, MUST therefore retain every note the former count-and-prefix rule retained, MUST report deterministic truncation diagnostics with the number of rows found, and MUST NOT retain bodies beyond those limits. File-index and note-source coordinators SHALL each own at most one active request plus one compact latest request.

#### Scenario: Huge flat directory exceeds remaining index capacity
- **WHEN** one workspace directory contains more visible entries than the remaining 131,072-file index capacity
- **THEN** traversal retains only bounded directory and index state while selecting the admitted entries
- **AND** it does not materialize and sort the complete flat directory before enforcing the index limit

#### Scenario: Workspace scope changes during index construction
- **WHEN** a newer workspace scope requests an index while an older traversal is active
- **THEN** the older traversal observes cooperative cancellation
- **AND** the coordinator retains only the latest compact scope request rather than queueing full indexes

#### Scenario: Note corpus exceeds aggregate bounds
- **WHEN** eligible note sidecars exceed one of the note source's byte ceilings
- **THEN** source construction skips and counts each refused row, admits later rows that fit, and retains no body beyond the ceilings
- **AND** the palette remains usable with a visible bounded-truncation diagnostic that states the rows found

#### Scenario: Many open-editor bookmarks
- **WHEN** open editors hold more than 16,384 bookmarks and the palette's note source refreshes
- **THEN** their capture runs in turns that each fit `B_SETTLE_US`, retains every bookmark the 4 MiB snapshot budget holds, and counts the rest

#### Scenario: Note refresh is superseded repeatedly
- **WHEN** note or bookmark edits request several refreshes while one sidecar load is active
- **THEN** only one latest compact refresh request remains pending
- **AND** stale workers release loaded bodies without replacing the current source

### Requirement: Command palette retains only bounded top results while scoring
For each command-palette source, the system SHALL retain at most the configured result limit while scoring candidates: 64 rows per group in the grouped view, and the rows the retained-byte budget holds in a category view. Results MUST be ordered by descending fuzzy score with source ordinal as the deterministic equal-score tie-break, and grouped output MUST preserve existing source priority and canonical-file deduplication. Every source MUST report how many candidates it matched, so a group can state what it does not show.

#### Scenario: Many candidates match one source
- **WHEN** a query matches more source items than the configured result limit
- **THEN** scoring retains only a bounded top-result structure
- **AND** the final rows equal the highest-ranked results under the defined score and tie order
- **AND** the source reports the number of candidates that matched

#### Scenario: Equal-score candidates are repeated
- **WHEN** several candidates receive the same fuzzy score
- **THEN** their relative order follows source ordinal deterministically
- **AND** repeated identical queries return the same rows and order

#### Scenario: Empty query uses source order
- **WHEN** the query is empty
- **THEN** each source returns its first bounded items in source order
- **AND** no full-source result collection is materialized
- **AND** the source counts its remaining included items without retaining or scoring them

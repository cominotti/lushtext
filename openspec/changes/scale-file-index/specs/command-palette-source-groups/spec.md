## ADDED Requirements

### Requirement: The file index interns its paths
The file index SHALL store each directory once and each file as a fixed record over shared name storage, reconstructing display path, relative display and canonical path on demand. A non-symlink entry's canonical identity SHALL be derived from its directory's canonical path and its name; only symlink entries SHALL be resolved one by one. Results, order, deduplication and identities SHALL equal the per-file representation's.

#### Scenario: A monorepo-shaped workspace of 1,048,576 files
- **WHEN** the workspace folder holds 1,048,576 visible files in about 100,000 directories with names of typical length (about 18 bytes)
- **THEN** the build indexes every file within the 64 MiB installed and 128 MiB construction ledgers and reports no truncation

#### Scenario: Ordinary files are not canonicalized one by one
- **WHEN** a build indexes a tree with no symlinks
- **THEN** it resolves canonical paths once per workspace folder and never once per file
- **AND** each file's canonical identity equals what resolving that file would return

#### Scenario: A symlinked file keeps its resolved identity
- **WHEN** a workspace contains a symlink to a file that is also indexed under its own path
- **THEN** the file appears once, under the path traversal reaches first, as on the per-file representation
- **AND** an open tab of either path excludes it from the workspace group

#### Scenario: A non-UTF-8 file name
- **WHEN** a file's name is not valid UTF-8
- **THEN** it is scored by the same lossy name as before and opens at its exact path

#### Scenario: Parity with the per-file representation
- **WHEN** generated trees, overlapping workspace folders and mutation sequences are applied to both representations
- **THEN** both list the same files in the same order with the same identities, return the same results and counts for every query, and report the same file-limit and directory-limit truncations
- **AND** under a byte limit the arena's files are a prefix of the reference's untruncated list, in the same order

### Requirement: File-index mutations scale with the change, not the index
An incremental mutation batch SHALL copy the installed index without a per-file allocation, SHALL deduplicate an added path by its directory and canonical identity without comparing it with every indexed path, and SHALL rename a directory by rewriting that directory's record only, re-keying its descendants' canonical identities without a filesystem call. Generation arbitration, replay and the overflow rebuild SHALL be unchanged.

#### Scenario: A file is added to a 1,048,576-file index
- **WHEN** one created file reaches the mutation worker
- **THEN** the batch admits it once, after any equal path or canonical identity already indexed has been found without a full path scan

#### Scenario: A directory with many descendants is renamed
- **WHEN** a directory holding 100,000 indexed files is renamed inside the workspace
- **THEN** every descendant appears under the new path and no descendant is canonicalized again

#### Scenario: A mutation batch loses its generation race
- **WHEN** a full rebuild is installed while a batch runs
- **THEN** the batch's index is rejected and its updates replay on the new index, as before

### Requirement: File-index storage left by mutations is reclaimed
Name bytes and directory records that removals and renames leave unreferenced SHALL be reclaimed on the mutation worker before they can cause a later addition to be refused, so the installed index's charged weight follows its live files.

#### Scenario: Many removals and renames on a long-lived index
- **WHEN** a long session removes and renames many files and directories and then adds new ones
- **THEN** every addition the live files' size admits is accepted, whatever the removed and renamed entries once occupied

### Requirement: The workspace group finds open tabs through the index
The workspace file group SHALL exclude open tabs and count the empty query's remaining files by looking each open canonical path up in the index, not by reading every indexed file's canonical path. The counts SHALL stay exact.

#### Scenario: The empty query at 1,048,576 files with open tabs
- **WHEN** the query is empty, the index holds 1,048,576 files and 256 of them are open in tabs
- **THEN** the workspace group states 1,048,576 − 256 − 64 more, without a pass over every indexed file

#### Scenario: An open tab outside the workspace
- **WHEN** an open tab's file is not in the index
- **THEN** it does not change the workspace group's rows or count

### Requirement: Palette queries over a large index stay prompt
A non-empty query over a 1,048,576-file index SHALL complete on the worker within 50 ms (median of a verdict run on the reference machine), and the palette's GTK dispatches SHALL keep their budgets (`B_INTERACT_US` for input, `B_SETTLE_US` for a publish) at that size.

#### Scenario: Typing in a large workspace
- **WHEN** the user types a query in Files mode over a 1,048,576-file index
- **THEN** the ranked rows publish within 200 ms of the last keystroke (150 ms debounce plus the worker query) and no dispatch exceeds its budget

## MODIFIED Requirements

### Requirement: Palette source inventories are bounded and superseding
The system SHALL keep palette source construction bounded before query scoring begins. File-index construction MUST retain at most 1,048,576 canonical files (a presentation cap and the query-latency bound; memory is bounded by the file-index byte ledgers), MUST avoid materializing an unbounded flat directory, and MUST cooperatively cancel superseded traversal. Note-source construction MUST be bounded by its byte ceilings alone (64 MiB of aggregate searchable UTF-8 note text, a 64 MiB retained graph, 8 MiB of sidecar paths and 96 MiB of construction scratch) with no row or sidecar count, MUST capture open editors' bookmarks in sliced main-loop turns bounded by the 4 MiB open-editor snapshot budget with no row count, counting the rows that budget cannot retain, MUST skip and count a row a byte ceiling refuses while admitting later rows that fit, MUST therefore retain every note the former count-and-prefix rule retained, MUST report deterministic truncation diagnostics with the number of rows found, and MUST NOT retain bodies beyond those limits. File-index and note-source coordinators SHALL each own at most one active request plus one compact latest request.

#### Scenario: Huge flat directory exceeds remaining index capacity
- **WHEN** one workspace directory contains more visible entries than the remaining 1,048,576-file index capacity
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
For each command-palette source, the system SHALL retain at most the configured result limit while scoring candidates: 64 rows per group in the grouped view, and the rows the category view's row cap and retained-byte budget hold in a category view. A grouped query MAY score its source in at most four contiguous chunks in parallel, each retaining at most the group's limit, merged into the group's limit before publication; a category view's ranking SHALL retain at most its own limit in all. Results MUST be ordered by descending fuzzy score with source ordinal as the deterministic equal-score tie-break, and grouped output MUST preserve existing source priority and canonical-file deduplication. Every source MUST report how many candidates it matched, so a group can state what it does not show.

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

### Requirement: Palette traversal bounds directory work independently from file admission
File-index construction SHALL examine and retain at most 1,048,576 distinct canonical directory identities per build, independently from the 1,048,576-file admission limit. A directory MUST consume directory budget before its identity is retained or descendants are scheduled, canonical aliases MUST consume the budget only once, and exhaustion MUST return the typed directory-retention truncation reason with a usable bounded partial index and direct retained-state metrics. Cooperative cancellation MUST remain observable before additional directory batches are admitted.

#### Scenario: Directory-only forest reaches its budget
- **WHEN** a workspace tree contains more than 1,048,576 distinct directories but few or no indexable files
- **THEN** the build retains and scans no more than the directory budget
- **AND** it completes with the typed directory-retention truncation reason
- **AND** its retained-directory high-water metric does not grow with the unvisited remainder

#### Scenario: File and directory limits remain independent
- **WHEN** one fixture reaches the file limit in a shallow tree and another reaches the directory limit with few files
- **THEN** each build reports the limit it encountered deterministically
- **AND** neither limit is inferred from the other resource count

#### Scenario: Canonical aliases do not amplify directory retention
- **WHEN** overlapping workspace folder paths or filesystem aliases resolve to a directory identity already visited by the build
- **THEN** that identity consumes one retained-directory slot
- **AND** traversal does not rescan its descendants through the alias

#### Scenario: Supersession stops directory admission
- **WHEN** a newer workspace-scope build cancels an active directory-heavy traversal
- **THEN** the active traversal stops before admitting another bounded directory batch
- **AND** only the latest compact build request remains pending

### Requirement: File-index construction enforces its working-set byte budget
Command-palette file indexing SHALL enforce a conservative O(1) byte ledger before retaining each allocation: index record vectors, name storage, canonical-key entries, symlink identities, directory records, visited-directory identities, hash-table buckets, pending directories, scan-page entries, and owned workspace-folder paths. Peak construction ownership MUST stay within MAX_FILE_INDEX_BUILD_RETAINED_BYTES, defined as twice the 64 MiB installed-result budget, while completed output MUST remain within MAX_FILE_INDEX_RETAINED_BYTES. The ledger MUST release temporary charges when ownership ends and MUST stop with a typed `RetainedByteLimit` (installed cap) or `BuildByteLimit` (construction cap) outcome before either applicable cap is exceeded.

#### Scenario: Long paths dominate a large traversal
- **WHEN** indexing encounters many long or deeply nested paths before reaching the item-count limit
- **THEN** each prospective retained owner is charged before insertion
- **AND** measured build high water remains at or below 128 MiB and installed output remains at or below 64 MiB

#### Scenario: Directory-only traversal grows pending state
- **WHEN** a broad tree contains many directories but few indexable files
- **THEN** visited, pending, scan-page, and workspace-folder path ownership still consumes the byte ledger
- **AND** the traversal cannot bypass the cap merely because final output is small

#### Scenario: Scan batch approaches remaining scratch capacity
- **WHEN** the filesystem scanner would return a batch larger than the ledger's remaining build capacity
- **THEN** scanning honors a byte limit or yields a bounded batch that can be charged before retention
- **AND** scan entries are included in peak build metrics

#### Scenario: Next item would exceed the byte budget
- **WHEN** retaining a path or scan batch would cross the remaining byte budget
- **THEN** indexing stops before taking that ownership and reports `RetainedByteLimit` for the installed cap or `BuildByteLimit` for the construction cap
- **AND** it returns the deterministic usable partial index already admitted

#### Scenario: Indexing is cancelled
- **WHEN** cancellation wins during a byte-bounded traversal
- **THEN** temporary ledger charges and owned traversal state are released through the existing worker lifecycle
- **AND** the cancelled generation cannot publish its partial index as current

### Requirement: A palette category view lists every ranked result of one category
Activating a "More" row SHALL show that category's view: only that category, every result it matches up to the view's row cap, ranked exactly as the grouped view ranks them (descending score, then source ordinal; source order for the empty query), so the view's first rows are the rows the grouped view showed. The view SHALL name its category and its result count, SHALL list its rows in a virtualized list whose model creates row objects only for rows the toolkit requests, and SHALL keep no per-row text for rows that are not shown. The ranked rows SHALL be bounded by a row cap of 131,072 (the size at which a far select-and-scroll still fits `B_INTERACT_US` in the application, ledger A95) and by a retained-byte budget through the shared list-budget policy, chosen so that the row cap fits it whole; if the cap or the budget cuts a category, the view SHALL say "Showing X of Y results". Activating a row in the view SHALL open the file, open the note or run the command exactly as the same row does in the grouped view.

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

#### Scenario: A category larger than the row cap
- **WHEN** a query matches 500,000 files of a 1,048,576-file index and the user opens the workspace category
- **THEN** the view lists the best-ranked 131,072 and says "Showing 131,072 of 500,000 results"
- **AND** its far select-and-scroll stays within `B_INTERACT_US`

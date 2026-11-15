# command-palette-source-groups Specification

## Purpose
Define how command palette modes and source-grouped results present files, note records, commands, open tabs, and workspace-scoped files.

## Requirements

### Requirement: Command palette mode selection is mouse and keyboard accessible
The system SHALL expose command palette modes `All`, `Files`, `Notes`, and `Commands` through a mouse-usable selector. The selector order MUST be `All`, then `Files`, then `Notes`, then `Commands`. The system MUST also preserve Tab as a keyboard shortcut that cycles through the same modes in selector order and keeps the selector state synchronized.

#### Scenario: Mouse changes palette mode
- **WHEN** the command palette is open and the user chooses `Files` from the mode selector with the mouse
- **THEN** the active palette mode becomes `Files`
- **AND** the result list refreshes for file-oriented results

#### Scenario: Mouse changes palette to Notes mode
- **WHEN** the command palette is open and the user chooses `Notes` from the mode selector with the mouse
- **THEN** the active palette mode becomes `Notes`
- **AND** the result list refreshes for searchable note and bookmark records

#### Scenario: Tab changes palette mode
- **WHEN** the command palette is open and the user presses Tab from `Files` mode
- **THEN** the active palette mode advances to `Notes`
- **AND** the visible mode selector reflects the new active mode

### Requirement: Files mode groups file results by source
The system SHALL present file results in `Files` mode under labeled source groups. `Open Tabs` MUST appear before the workspace file group when both groups have matching results. The workspace file group label MUST be `Selected Workspace` when the current sidebar scope is one concrete workspace and `All Workspaces` when the current sidebar scope is the aggregate `All workspaces` scope. Workspace file results SHALL be built from the current workspace scope's ordered folder set and MUST de-duplicate canonical file identities across overlapping folders.

#### Scenario: Open tabs appear before selected workspace files
- **WHEN** the command palette is in `Files` mode
- **AND** the query matches both an open file-backed tab and a file from the selected workspace folder set
- **THEN** the matching open tab appears under `Open Tabs`
- **AND** the matching workspace file appears under `Selected Workspace`
- **AND** the `Open Tabs` group is presented before `Selected Workspace`

#### Scenario: Aggregate workspace scope uses All Workspaces label
- **WHEN** the command palette is in `Files` mode
- **AND** the sidebar scope selector is set to `All workspaces`
- **AND** the query matches files from restored workspace folders
- **THEN** matching workspace-indexed files appear under `All Workspaces`

#### Scenario: Empty selected workspace has no workspace file group rows
- **WHEN** the command palette is in `Files` mode
- **AND** the selected workspace contains zero folders
- **AND** the query has no matching open file-backed tabs
- **THEN** the palette does not show stale workspace-indexed file rows from another workspace
- **AND** the visible result state remains stable and searchable

### Requirement: All mode groups open tabs, workspace files, notes, and commands by priority
The system SHALL present `All` mode results in labeled groups ordered as `Open Tabs`, then the current workspace-scope file group, then note record groups, then `Commands`. The workspace file group MUST use the same `Selected Workspace` or `All Workspaces` label rules as `Files` mode. Workspace-indexed files SHALL come from the current workspace scope's ordered folder set and MUST be de-duplicated by canonical file identity before note and command rows are mixed in. Note record groups MUST use the Notes browser category vocabulary, ordered as `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tab Notes`; empty note groups MUST be omitted. The `Commands` group MUST contain matching command registry rows, including commands in the `Notes` command category, and MUST NOT contain note records.

#### Scenario: All mode preserves source priority
- **WHEN** the command palette is in `All` mode
- **AND** the query matches an open file-backed tab, a workspace-indexed file, a bookmark row, a document-note body, a note command, and a non-note command
- **THEN** the open file-backed tab appears under `Open Tabs`
- **AND** the workspace-indexed file appears under the current workspace-scope group
- **AND** the bookmark row appears under `Bookmarks`
- **AND** the document-note row appears under `Document Notes`
- **AND** the note command and non-note command appear under `Commands`
- **AND** the groups are presented in that order

#### Scenario: All mode suppresses overlapping workspace duplicates
- **WHEN** the selected workspace contains overlapping folders that both cover `/repo/src/main.rs`
- **AND** the command palette query matches that file and one command
- **THEN** `/repo/src/main.rs` appears at most once in the workspace-scope file group
- **AND** the matching command remains available under `Commands`

#### Scenario: All mode keeps note commands in Commands
- **WHEN** the command palette is in `All` mode
- **AND** the query matches a command in the `Notes` command category
- **THEN** the matching note command appears under `Commands`
- **AND** the same command does not appear under any note record group

#### Scenario: All mode separates open-tab files from open-tab notes
- **WHEN** the command palette is in `All` mode
- **AND** the query matches an open file-backed tab and a note record attached to a saved open tab outside the current workspace scope
- **THEN** the open file-backed tab appears under `Open Tabs`
- **AND** the note record appears under `Open Tab Notes`

### Requirement: File results are deduplicated across source groups
The system SHALL show a canonical file identity at most once in grouped command palette results. Display and activation paths MAY preserve the user-visible path, but deduplication MUST use canonical identity captured before bounded selection. If a matching file is both an open file-backed tab and a workspace-indexed file, every open canonical identity MUST be excluded from workspace top-result retention so the result appears only under `Open Tabs` without consuming a workspace result slot. If overlapping folders in the current workspace scope index the same canonical file, the workspace file group MUST contain only one result for that file, using workspace order and then folder order to choose its primary context.

#### Scenario: Open tab suppresses duplicate workspace result
- **WHEN** a file is open in a tab
- **AND** the same canonical file is included in the current workspace file index
- **AND** the command palette query matches that file
- **THEN** the file appears under `Open Tabs`
- **AND** the same canonical identity does not also appear under the workspace file group

#### Scenario: Alias path resolves to an open file
- **WHEN** an open tab and a workspace entry reach the same file through different symlink or overlapping-root paths
- **AND** the query matches that file
- **THEN** canonical identity suppresses the workspace alias
- **AND** activation retains the selected open tab's normal display and action path

#### Scenario: Excluded best match does not underfill workspace results
- **WHEN** the highest-scoring workspace candidate is already represented by an open tab
- **AND** a lower-scoring distinct workspace file also matches within the configured result limit
- **THEN** the open canonical identity is excluded before workspace top-result retention
- **AND** the distinct workspace file remains eligible to fill the workspace result group

#### Scenario: Overlapping workspace folders suppress duplicate workspace rows
- **WHEN** the selected workspace contains folders `/repo` and `/repo/src`
- **AND** `/repo/src/main.rs` is indexed through both folders
- **AND** the command palette query matches `main.rs`
- **THEN** the workspace file group shows one row for `/repo/src/main.rs`
- **AND** the row's source context is based on the earliest covering folder in the selected workspace order

#### Scenario: Same file in different workspaces is deduplicated in aggregate scope
- **WHEN** `All workspaces` is the current shared scope
- **AND** two workspaces contain the same canonical folder or overlapping folders that cover the same file
- **AND** the command palette query matches that file
- **THEN** the aggregate workspace file group shows that canonical file at most once
- **AND** the row's primary context is chosen by workspace order and then folder order

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

### Requirement: Note scoring prunes only non-contributing body work
Command-palette note scoring SHALL preserve the current eligible rows, fuzzy scores, deterministic source-ordinal tie order, category grouping, Unicode behavior, and cancellation semantics. The scorer MAY skip a note body only after another searchable field has established eligibility and the scoring policy proves that the body cannot improve that row's result. A body MUST still be searched when it can establish eligibility or improve ordering.

#### Scenario: Metadata match already dominates a large body
- **WHEN** note title, path, workspace, line metadata, or another searchable metadata field establishes the row's best score and the body cannot exceed it
- **THEN** the scorer does not scan the body
- **AND** the row keeps the same score, identity, group, and deterministic position as the unpruned reference

#### Scenario: Query matches only the note body
- **WHEN** no searchable metadata field matches but the query appears in the note body
- **THEN** the body remains eligible for bounded scoring
- **AND** the matching row is neither pruned nor reordered solely by the optimization

#### Scenario: Unicode and equal scores remain equivalent
- **WHEN** generated notes contain Unicode text, empty fields, equal metadata and body scores, and source-order ties
- **THEN** optimized and unpruned reference scoring publish identical selected identities and order
- **AND** per-source result retention remains within the configured top-result bound

#### Scenario: New query cancels a pruned or body-scanning pass
- **WHEN** a newer palette query supersedes scoring during metadata or body evaluation
- **THEN** the active scorer stops at the existing bounded cancellation checkpoint
- **AND** only the latest query may publish note rows or searching state

### Requirement: Palette source behavior survives bounded selection
Bounded selection MUST preserve open-tab precedence, workspace-scope labels, note-category order, command grouping, and duplicate suppression across sources.

#### Scenario: Open tab and workspace file both match
- **WHEN** the same canonical file is selected into both source-local top sets
- **THEN** grouped output retains only the `Open Tabs` row
- **AND** bounded selection does not reintroduce the workspace duplicate

#### Scenario: Mixed All-mode sources exceed their limits
- **WHEN** open tabs, workspace files, notes, and commands all have more matches than their per-source limits
- **THEN** each group remains individually bounded
- **AND** group ordering and category labels remain unchanged

### Requirement: Notes command category identifies note workflows
The system SHALL define a `Notes` command category for note and bookmark workflows exposed by the command palette. The category MUST include `Browse Notes`, `Browse Bookmarks`, `Toggle Bookmark`, `Edit Bookmark`, `Next Bookmark`, `Previous Bookmark`, `Open Document Note`, and `Open Folder Note`. Palette subtitles for those commands MUST display `Notes` and MUST preserve any existing shortcut hint.

#### Scenario: Note commands use the Notes category
- **WHEN** the command palette displays `Browse Notes`, `Browse Bookmarks`, `Toggle Bookmark`, `Edit Bookmark`, `Next Bookmark`, `Previous Bookmark`, `Open Document Note`, or `Open Folder Note`
- **THEN** each row is categorized as `Notes`
- **AND** each row keeps its existing action id and shortcut hint

#### Scenario: Non-note commands keep their existing categories
- **WHEN** the command palette displays file, edit, view, or app commands that are not note or bookmark workflows
- **THEN** those commands remain categorized as `File`, `Edit`, `View`, or `App`

### Requirement: Notes mode groups searchable note records by category
The system SHALL present `Notes` mode as a searchable note-record surface rather than a command-launcher subset. Matching rows MUST appear under category headers ordered as `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tabs`. The system MUST omit empty categories, MUST NOT show workspace files or command rows in `Notes` mode, and MUST keep the search entry, mode selector, result list, and close behavior usable when no note records match.

#### Scenario: Notes mode shows note category sections
- **WHEN** the command palette is in `Notes` mode
- **AND** the current workspace scope contains matching bookmarks, folder notes, document notes, and eligible saved open-tab note rows
- **THEN** matching bookmark rows appear under `Bookmarks`
- **AND** matching folder-note rows appear under `Folder Notes`
- **AND** matching document-note rows appear under `Document Notes`
- **AND** matching saved open-tab note rows outside the current workspace scope appear under `Open Tabs`
- **AND** the sections are presented in that order

#### Scenario: Notes mode searches note bodies and note metadata
- **WHEN** the command palette is in `Notes` mode
- **AND** the query matches a document-note body, a folder-note body, a bookmark label, a bookmark line number, a saved file path, a workspace name, or a workspace folder path
- **THEN** the matching note or bookmark rows appear in their note category sections
- **AND** non-matching note and bookmark rows are hidden without changing persisted note data

#### Scenario: Notes mode excludes commands and files
- **WHEN** the command palette is in `Notes` mode
- **AND** the query matches workspace files, open file-backed tabs, note commands, non-note commands, and note records
- **THEN** only matching note and bookmark records appear
- **AND** workspace files, open file-backed tabs, note commands, and non-note commands are not shown

#### Scenario: Empty Notes mode stays readable
- **WHEN** the command palette is in `Notes` mode
- **AND** there are no browsable notes, no bookmarks, no matching open-tab note rows, or the query filters every note row out
- **THEN** the palette shows its explicit no-results state without fake note rows
- **AND** the search entry, mode selector, keyboard mode cycling, Escape dismissal, and click-away dismissal remain usable

#### Scenario: Many Notes mode matches stay bounded
- **WHEN** the command palette is in `Notes` mode
- **AND** the current workspace scope contains many matching note and bookmark rows with long titles, long paths, or long note body matches
- **THEN** result rows remain sectioned by note category
- **AND** the result list remains the only scrolling region
- **AND** row text is ellipsized or clipped within the palette without introducing unintended horizontal scrolling

#### Scenario: Bookmark source excerpts do not drive palette search
- **WHEN** the command palette is in `Notes` mode
- **AND** the query appears only in the closed source file text around a bookmark, not in the bookmark label, line metadata, file metadata, workspace metadata, or note metadata
- **THEN** the bookmark row does not match merely because of the closed source excerpt
- **AND** the palette does not read closed source files solely to decide bookmark search matches

### Requirement: Note result activation opens the associated note target
The system SHALL activate note result rows through the same target workflows used by `Browse Notes...`. Bookmark rows MUST open or focus the bookmarked file at the bookmarked line, folder-note rows MUST open the targeted folder note, and document-note rows MUST open or focus the associated saved file and open its document-note surface. Activating a note row MUST close the command palette only after dispatching the target workflow.

#### Scenario: Activate a bookmark result
- **WHEN** the command palette is in `Notes` mode or `All` mode
- **AND** the selected result is a bookmark row
- **AND** the user activates the row
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line

#### Scenario: Activate a folder-note result
- **WHEN** the command palette is in `Notes` mode or `All` mode
- **AND** the selected result is a folder-note row
- **AND** the user activates the row
- **THEN** the system opens that folder note's surface
- **AND** it does not require an active document tab

#### Scenario: Activate a document-note result
- **WHEN** the command palette is in `Notes` mode or `All` mode
- **AND** the selected result is a document-note row
- **AND** the user activates the row
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the system opens that file's document-note surface

### Requirement: Commands mode remains the complete command search
The system SHALL keep `Commands` mode as the complete command registry search surface. Commands in the `Notes` category MUST remain searchable and activatable in `Commands` mode, and their subtitles MUST identify them as `Notes`.

#### Scenario: Commands mode includes note commands
- **WHEN** the command palette is in `Commands` mode
- **AND** the query matches `Browse Notes`
- **THEN** `Browse Notes` appears in the command results
- **AND** its subtitle identifies it as a `Notes` command

#### Scenario: Commands mode includes non-note commands
- **WHEN** the command palette is in `Commands` mode
- **AND** the query matches a non-note command
- **THEN** the matching non-note command appears in the command results
- **AND** it keeps its existing command category

### Requirement: Group headers are presentation-only
The system SHALL render source labels as presentation-only group headers. Group headers MUST NOT activate a file, note, or command, and keyboard result navigation MUST move between activatable result rows.

#### Scenario: Activating a grouped result ignores headers
- **WHEN** grouped command palette results are visible
- **AND** the user activates a selected result row
- **THEN** only file, note, and command result rows can trigger file opening, note opening, or command execution
- **AND** source group headers do not trigger activation callbacks

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

### Requirement: The file index reflects externally changed workspace files
The command-palette file index SHALL reflect files created, removed, or renamed under the current workspace scope by processes other than LushText, at any depth the index covers, without requiring the user to change workspace scope, toggle a visibility setting, or restart the application. Freshness MUST be established by the attention-moment refresh, and MUST apply the index's existing coverage rule — excluded directory names and workspace entry visibility — exactly as a full build does.

#### Scenario: A branch switch introduces new files
- **WHEN** a version-control operation outside LushText replaces the working tree of a workspace folder with one containing files the index has never seen, including files in directories that were never expanded in the sidebar
- **AND** the user returns to the window and opens the command palette
- **THEN** the new files are searchable in the palette

#### Scenario: A file created from a terminal in a deep subdirectory
- **WHEN** a file is created by another process in a directory several levels below a workspace folder that has never been expanded in the sidebar
- **AND** the user returns to the window and opens the command palette
- **THEN** that file is searchable in the palette

#### Scenario: Externally removed files stop appearing
- **WHEN** an indexed file is removed from disk by another process
- **AND** an attention-triggered refresh settles
- **THEN** the palette no longer offers that file

#### Scenario: Excluded directories stay excluded after refresh
- **WHEN** external changes occur inside a directory the index excludes by name or by workspace entry visibility
- **THEN** the refreshed index does not admit files from that directory

#### Scenario: A refresh cannot overwrite a newer index
- **WHEN** a newer workspace-scope build is accepted while an attention-triggered refresh is in flight
- **THEN** the older refresh cannot install its result as current

### Requirement: In-app saves update the file index
Saving a document to a path inside the current workspace scope SHALL update the file index, including the first save of an untitled document and a Save As to a new path. The admitted entry MUST carry the canonical identity of the file the durable write actually resolved, so that a symlinked save is indexed as its target. The index MUST NOT depend on a later refresh to learn about a file the application itself wrote.

#### Scenario: Saving an untitled document indexes it
- **WHEN** an untitled document is saved to a path inside a workspace folder
- **THEN** that path is searchable in the command palette without waiting for a refresh

#### Scenario: Save As to a new workspace path indexes the new path
- **WHEN** a document is saved under a new name inside a workspace folder
- **THEN** the new path is searchable in the command palette

#### Scenario: A symlinked save indexes the resolved target
- **WHEN** a document is saved through a path that is a symbolic link, so the durable write resolves and writes the link's target
- **THEN** the index admits the resolved target's canonical identity
- **AND** activating the palette result opens the file whose bytes were written

#### Scenario: Saving outside the workspace scope does not index
- **WHEN** a document is saved to a path outside every folder in the current workspace scope
- **THEN** the index does not admit that path

#### Scenario: The workspace-membership test does not block the save terminal
- **WHEN** a save completes and the index must decide whether the written path belongs to a workspace folder
- **THEN** the decision is a path comparison against the configured workspace folders that requires no filesystem access
- **AND** the canonical identity is resolved on the index mutation worker, not on the GTK thread

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
Name bytes and directory records that removals and renames leave unreferenced SHALL be reclaimed on the mutation worker: at the latest when an addition would otherwise be refused while such storage exists, so that no addition the live files' size admits is refused.

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
A non-empty grouped query (the grouped view's rows and counts) over a 1,048,576-file index SHALL complete on the worker within 50 ms (median of a verdict run on the reference machine). A category view's ranking SHALL run on the worker as one bounded pass whose time at that size is measured and recorded. The palette's GTK dispatches SHALL keep their budgets (`B_INTERACT_US` for input, `B_SETTLE_US` for a publish) at that size, in both views.

#### Scenario: Typing in a large workspace
- **WHEN** the user types a query in Files mode's grouped view over a 1,048,576-file index
- **THEN** the ranked rows publish within 200 ms of the last keystroke (150 ms debounce plus the worker query) and no dispatch exceeds its budget

#### Scenario: Opening a large category
- **WHEN** the user opens the workspace category of a query matching more than 131,072 of 1,048,576 files
- **THEN** the ranking runs off the GTK thread and no palette dispatch exceeds its budget while it runs or when its rows publish

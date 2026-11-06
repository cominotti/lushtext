## MODIFIED Requirements

### Requirement: Every tree-model row is rendered regardless of row count
The workspace sidebar SHALL render every row present in a workspace section's `GtkTreeListModel`. The set of rows a user can reach by scrolling the workspace-section area MUST equal the set of rows in the model, for any row count, including every entry of a directory that fits the per-directory retained-byte budget (`SIDEBAR_DIRECTORY_RETAINED_BYTES`) and the budget row that ends a directory past it. A directory SHALL have no entry-count cap. Rendering completeness MUST NOT depend on `GtkListView`'s realized-widget cap (`GTK_LIST_VIEW_MAX_LIST_ITEMS`, 200 plus extra items).

#### Scenario: Single directory above the GTK realized-widget cap
- **WHEN** an expanded directory holds 300 visible entries and the outer sidebar scroller is moved to its lower edge
- **THEN** the row for the last entry in sort order is realized, allocated a non-zero height, and its label text is the last entry's file name
- **AND** no vertical band inside the section's allocated list height is left without a realized row

#### Scenario: Boundary row counts around the cap
- **WHEN** an expanded directory holds exactly 199, 200, 201, 202, or 205 visible entries
- **THEN** for each count the last row in sort order is reachable and rendered after scrolling to the lower edge
- **AND** the number of rendered rows equals the number of model rows for the directory when the whole directory fits the viewport, or the last row is rendered when it does not

#### Scenario: Nested large directory
- **WHEN** a directory with 400 visible entries sits two levels below a workspace folder and both ancestors plus the directory are expanded
- **THEN** every one of the 400 rows is reachable by scrolling and the last one is rendered

#### Scenario: Large directory at one thousand rows
- **WHEN** an expanded directory holds 1,000 visible entries
- **THEN** the first, a middle, and the last row are each rendered when scrolled into view
- **AND** the outer scroller's upper bound equals the sum of the section's header height and all row heights

#### Scenario: Truncation placeholder is reachable
- **WHEN** an expanded directory exceeds the per-directory retained-byte budget, so that the tree model ends with the budget row stating "Showing X of Y entries"
- **THEN** the budget row is rendered when the outer scroller reaches its lower edge
- **AND** X equals the entries listed above it and Y equals the directory's visible entry count

## ADDED Requirements

### Requirement: Large directories load completely and incrementally
A directory whose entries fit `SIDEBAR_DIRECTORY_RETAINED_BYTES` SHALL list every visible entry, with no placeholder row once loading ends. The budget SHALL be the power of two at or above the larger of two derived floors: `main`'s worst-case retention for 10,000 entries, and the retention of 131,072 entries with names of 32 bytes or fewer under a parent path of 256 bytes. Compile-time assertions SHALL hold both inequalities. A directory that fits SHALL be scanned in one pass. The deterministic two-pass top-k selection SHALL run only for a directory that exceeds the budget. Rows SHALL be applied from the top in time-sliced batches. Until the last batch, a loading row stating "Loading entries… X of Y", with a stable accessible name, SHALL end the directory, and that row SHALL be removed at the directory's terminal. The `workspace-refresh-complete` readiness predicate SHALL stay pending until the terminal.

#### Scenario: A directory past 10,000 entries is listed in full
- **WHEN** a directory of 16,385 entries is expanded
- **THEN** after loading, all 16,385 entries are in the model in sort order, and the last one is rendered when scrolled into view
- **AND** no "items - showing first" placeholder and no budget row exist

#### Scenario: A directory of 131,072 entries
- **WHEN** a directory of 131,072 entries with names of at most 32 bytes, under a parent path of at most 256 bytes, is expanded
- **THEN** every entry is listed and reachable by keyboard traversal and by `win.reveal-workspace-path`, with realized rows bounded as for any directory
- **AND** the first screen of rows is interactive before the last batch is applied

#### Scenario: The loading row reports progress
- **WHEN** a large directory is still loading
- **THEN** its last row states the entries listed and the total
- **AND** that row disappears at the terminal, and readiness becomes complete only then

#### Scenario: Refresh of a loading directory
- **WHEN** a refresh or collapse supersedes a directory that is still loading
- **THEN** the remaining batches stop through the existing generation guard
- **AND** no stale batch, cache update, or loading row reaches the newer generation

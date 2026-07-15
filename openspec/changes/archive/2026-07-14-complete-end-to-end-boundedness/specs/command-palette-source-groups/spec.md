## MODIFIED Requirements

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
- **THEN** the workspace file group shows one row for the canonical identity of `/repo/src/main.rs`
- **AND** the row's source context is based on the earliest covering folder in the selected workspace order

#### Scenario: Same file in different workspaces is deduplicated in aggregate scope
- **WHEN** `All workspaces` is the current shared scope
- **AND** two workspaces contain the same canonical folder or overlapping folders that cover the same file
- **AND** the command palette query matches that file
- **THEN** the aggregate workspace file group shows that canonical file at most once
- **AND** the row's primary context is chosen by workspace order and then folder order

## ADDED Requirements

### Requirement: Palette source inventories are bounded and superseding
The system SHALL keep palette source construction bounded before query scoring begins. File-index construction MUST retain at most 100,000 canonical files, MUST avoid materializing an unbounded flat directory, and MUST cooperatively cancel superseded traversal. Note-source construction MUST retain at most 10,000 entries and at most 64 MiB of aggregate searchable UTF-8 note text, MUST report deterministic truncation diagnostics, and MUST NOT retain bodies beyond those limits. File-index and note-source coordinators SHALL each own at most one active request plus one compact latest request.

#### Scenario: Huge flat directory exceeds remaining index capacity
- **WHEN** one workspace directory contains more visible entries than the remaining 100,000-file index capacity
- **THEN** traversal retains only bounded directory and index state while selecting the admitted entries
- **AND** it does not materialize and sort the complete flat directory before enforcing the index limit

#### Scenario: Workspace scope changes during index construction
- **WHEN** a newer workspace scope requests an index while an older traversal is active
- **THEN** the older traversal observes cooperative cancellation
- **AND** the coordinator retains only the latest compact scope request rather than queueing full indexes

#### Scenario: Note corpus exceeds aggregate bounds
- **WHEN** eligible note sidecars exceed either 10,000 entries or 64 MiB of aggregate searchable UTF-8 text
- **THEN** source construction stops retaining additional note bodies according to deterministic source order
- **AND** the palette remains usable with a visible bounded-truncation diagnostic

#### Scenario: Note refresh is superseded repeatedly
- **WHEN** note or bookmark edits request several refreshes while one sidecar load is active
- **THEN** only one latest compact refresh request remains pending
- **AND** stale workers release loaded bodies without replacing the current source

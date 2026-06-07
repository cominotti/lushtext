## MODIFIED Requirements

### Requirement: Bookmarks appear in the unified notes browser
The system SHALL include saved-file bookmarks in the `Browse Notes...` surface when they either belong to the current workspace scope or belong to saved open tabs outside that scope. Workspace-scoped bookmark entries MUST appear in the dedicated `Bookmarks` section, respect the current workspace scope, preview bookmark metadata explicitly, and open or focus the bookmarked file at the bookmarked line. Open-tab bookmark entries outside the current workspace scope MUST appear in the dedicated `Open Tabs` section, identify themselves as open-tab rows, reflect the current live editor bookmark state even when debounced bookmark sidecar persistence has not completed, and MUST NOT be represented as belonging to a fake workspace.

#### Scenario: Browse bookmarks with notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes...`
- **THEN** the browser lists bookmarks for saved files inside that workspace root in a dedicated `Bookmarks` section
- **AND** closed-file bookmarks outside that workspace are excluded
- **AND** bookmarks attached to saved open tabs outside that workspace appear only in the `Open Tabs` section

#### Scenario: Browse bookmarks across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes...`
- **THEN** the browser aggregates bookmarks from every restored workspace root
- **AND** each workspace bookmark row preserves enough workspace and file metadata for the user to tell where it belongs
- **AND** bookmarks attached to saved open tabs outside every restored workspace root appear only in the `Open Tabs` section

#### Scenario: Browse open-tab bookmarks without a workspace
- **WHEN** no workspace roots are restored
- **AND** a saved open editor has one or more bookmarks
- **AND** the user opens `Browse Notes...`
- **THEN** the browser lists those bookmarks in the `Open Tabs` section
- **AND** the bookmark rows identify the saved file path and line number without requiring workspace metadata

#### Scenario: Browse freshly changed open-editor bookmarks
- **WHEN** an open saved editor inside or outside the current workspace scope has bookmarks added, removed, labeled, or moved
- **AND** the user opens `Browse Notes...` before debounced bookmark sidecar persistence completes
- **THEN** the browser lists the current live bookmarks for that open file in the appropriate workspace or `Open Tabs` section
- **AND** stale persisted bookmark rows for that open file are not duplicated or shown instead of the current live state

#### Scenario: Preview a bookmark from the unified notes browser
- **WHEN** the user selects a bookmark row in `Browse Notes...`
- **THEN** the preview pane identifies the bookmark label or fallback line title
- **AND** the preview pane shows the associated source, file path, and line number instead of an empty markdown note
- **AND** the Open action targets the selected bookmark

#### Scenario: Open a bookmark from the unified notes browser
- **WHEN** the user selects a bookmark row in `Browse Notes...` and invokes Open
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line

#### Scenario: Search bookmarks in the unified notes browser
- **WHEN** the user searches in `Browse Notes...`
- **THEN** bookmark rows match by label, saved file metadata, workspace or open-tab source metadata, or line number
- **AND** non-matching bookmark rows are hidden without changing persisted bookmark data

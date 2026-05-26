## ADDED Requirements

### Requirement: Bookmarks appear in the unified notes browser
The system SHALL include saved-file bookmarks in the workspace-scoped `Browse Notes…` surface. Bookmark entries MUST appear in a dedicated `Bookmarks` section, respect the current workspace scope, preview bookmark metadata explicitly, and open or focus the bookmarked file at the bookmarked line.

#### Scenario: Browse bookmarks with notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes…`
- **THEN** the browser lists bookmarks for saved files inside that workspace root in a dedicated `Bookmarks` section
- **AND** bookmarks outside that workspace are excluded

#### Scenario: Browse bookmarks across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes…`
- **THEN** the browser aggregates bookmarks from every restored workspace root
- **AND** each bookmark row preserves enough workspace and file metadata for the user to tell where it belongs

#### Scenario: Preview a bookmark from the unified notes browser
- **WHEN** the user selects a bookmark row in `Browse Notes…`
- **THEN** the preview pane identifies the bookmark label or fallback line title
- **AND** the preview pane shows the associated workspace, file path, and line number instead of an empty markdown note
- **AND** the Open action targets the selected bookmark

#### Scenario: Open a bookmark from the unified notes browser
- **WHEN** the user selects a bookmark row in `Browse Notes…` and invokes Open
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line

#### Scenario: Search bookmarks in the unified notes browser
- **WHEN** the user searches in `Browse Notes…`
- **THEN** bookmark rows match by label, saved file metadata, workspace metadata, or line number
- **AND** non-matching bookmark rows are hidden without changing persisted bookmark data

## MODIFIED Requirements

### Requirement: Bookmarks appear in the unified notes browser
The system SHALL include saved-file bookmarks in the `Browse Notes...` surface when they either belong to the current workspace scope's folder coverage or belong to saved open tabs outside that scope. Workspace-scoped bookmark entries MUST appear in the dedicated `Bookmarks` section, respect the current workspace scope, preview bookmark metadata explicitly, and open or focus the bookmarked file at the bookmarked line. Open-tab bookmark entries outside the current workspace scope MUST appear in the dedicated `Open Tabs` section, identify themselves as open-tab rows, reflect the current live editor bookmark state even when debounced bookmark sidecar persistence has not completed, and MUST NOT be represented as belonging to a fake workspace. When overlapping folders cover the same saved file, the browser MUST show one bookmark row per bookmark identity, not one duplicate row per covering folder.

#### Scenario: Browse bookmarks with notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes...`
- **THEN** the browser lists bookmarks for saved files covered by that workspace's folder set in a dedicated `Bookmarks` section
- **AND** closed-file bookmarks outside that workspace's folder set are excluded
- **AND** bookmarks attached to saved open tabs outside that workspace appear only in the `Open Tabs` section

#### Scenario: Browse bookmarks across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes...`
- **THEN** the browser aggregates bookmarks from every restored workspace folder
- **AND** each bookmark row preserves enough workspace, primary folder, and file metadata for the user to tell where it belongs
- **AND** bookmarks attached to saved open tabs outside every restored workspace folder appear only in the `Open Tabs` section

#### Scenario: Overlapping folders do not duplicate a bookmark
- **WHEN** the selected workspace contains folders `/repo` and `/repo/src`
- **AND** `/repo/src/main.rs` has one bookmark
- **AND** the user opens `Browse Notes...`
- **THEN** the `Bookmarks` section shows one row for that bookmark
- **AND** the row uses the earliest covering folder by workspace folder order as its primary context

#### Scenario: Browse open-tab bookmarks without a workspace folder
- **WHEN** no workspace folders are restored
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
- **AND** the preview pane shows the associated source, file path, line number, and source context instead of an empty markdown note
- **AND** the Open action targets the selected bookmark

#### Scenario: Open a bookmark from the unified notes browser
- **WHEN** the user selects a bookmark row in `Browse Notes...` and invokes Open
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line

#### Scenario: Search bookmarks in the unified notes browser
- **WHEN** the user searches in `Browse Notes...`
- **THEN** bookmark rows match by label, saved file metadata, workspace metadata, primary folder metadata, open-tab source metadata, or line number
- **AND** non-matching bookmark rows are hidden without changing persisted bookmark data

### Requirement: Bookmark browser previews show anchored source excerpts
The system SHALL show a contextual source excerpt in the `Browse Notes...` preview pane when the selected entry is a bookmark. The excerpt MUST include the bookmarked line, bounded nearby context before and after that line, and visible bookmark metadata. For Markdown-like files, the excerpt MUST render through the Markdown preview surface using the bookmarked file as render context. For other UTF-8 text files, the excerpt MUST render as raw monospace text and visually emphasize the bookmarked line. The Open action MUST continue to open or focus the bookmarked file at the bookmarked line.

#### Scenario: Preview a Markdown bookmark as rendered context
- **WHEN** the user selects a bookmark row for a Markdown-like saved file in `Browse Notes...`
- **THEN** the preview pane renders a bounded Markdown excerpt around the bookmarked line
- **AND** relative preview context uses the bookmarked file path and current workspace folder coverage
- **AND** the preview still identifies the bookmark label or fallback line title, source, file path, and line number

#### Scenario: Preview a plain text bookmark as raw context
- **WHEN** the user selects a bookmark row for a non-Markdown UTF-8 text file in `Browse Notes...`
- **THEN** the preview pane shows a bounded raw monospace excerpt around the bookmarked line
- **AND** the bookmarked line is visually distinguished from neighboring context lines
- **AND** the file content is not interpreted as Markdown

#### Scenario: Excerpt includes context before and after the bookmark
- **WHEN** the bookmarked line has readable neighboring lines before and after it
- **AND** the user selects that bookmark in `Browse Notes...`
- **THEN** the preview excerpt includes at least one preceding line and at least one following line when available within the configured excerpt budget
- **AND** the bookmarked line remains identifiable inside the excerpt

#### Scenario: Open-editor bookmark preview uses live buffer text
- **WHEN** a bookmark belongs to an open saved editor whose buffer has unsaved text changes near the bookmarked line
- **AND** the user selects that bookmark in `Browse Notes...`
- **THEN** the preview excerpt reflects the current live editor buffer text
- **AND** the preview does not wait for a debounced bookmark sidecar save or a disk read to show the live excerpt

#### Scenario: Closed-file bookmark preview uses bounded disk reading
- **WHEN** a bookmark belongs to a saved file that is not currently open
- **AND** the user selects that bookmark in `Browse Notes...`
- **THEN** the system loads the excerpt from disk through a bounded background read
- **AND** the GTK main thread remains responsive while the excerpt is loading
- **AND** the completed preview is applied only if the same bookmark row is still selected

#### Scenario: Unavailable bookmark excerpt remains explicit
- **WHEN** the bookmarked file is missing, unreadable, binary or non-UTF-8, too large for preview, or the bookmarked line cannot be reached within the preview scan budget
- **AND** the user selects that bookmark in `Browse Notes...`
- **THEN** the preview pane shows an explicit unavailable state with the bookmark metadata
- **AND** the Open action remains available only according to the existing file-opening rules

#### Scenario: Bookmark preview does not change persisted bookmark data
- **WHEN** the user previews bookmark excerpts in `Browse Notes...`
- **THEN** the system does not write excerpt text into bookmark sidecars
- **AND** preview loading does not modify the source document bytes

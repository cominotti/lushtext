# line-bookmarks Specification

## Purpose
Let users persist lightweight, non-text bookmarks on saved files so they can label important lines, revisit them quickly, and keep them stable across restarts and in-app renames.
## Requirements
### Requirement: Users can toggle bookmarks on file-backed lines
The system SHALL allow users to add or remove a bookmark on the active line of a file-backed document without modifying the document text. The system MUST block bookmark creation for documents that do not yet have a stable file path.

#### Scenario: Add a bookmark on the current line
- **WHEN** the user triggers bookmark toggle on a line that does not already have a bookmark
- **THEN** the system creates a bookmark for that file and line
- **AND** the editor shows a bookmark indicator in the gutter
- **AND** the document text remains unchanged

#### Scenario: Remove an existing bookmark
- **WHEN** the user triggers bookmark toggle on a line that already has a bookmark
- **THEN** the system removes the bookmark from that file
- **AND** the editor removes the corresponding gutter indicator

#### Scenario: Attempt to bookmark an untitled document
- **WHEN** the user triggers bookmark creation in a document that has not yet been saved to disk
- **THEN** the system does not create a bookmark
- **AND** the user receives feedback that bookmarks require a saved file

### Requirement: Users can label and revisit bookmarks
The system SHALL let users assign or update an optional label for a bookmark and SHALL provide bookmark navigation and lookup workflows that jump back to the bookmarked location.

#### Scenario: Add or update a bookmark label
- **WHEN** the user edits the label for an existing bookmark
- **THEN** the system saves the new label with that bookmark
- **AND** later bookmark lists and navigation surfaces show the updated label

#### Scenario: Navigate to the next bookmark in a file
- **WHEN** the active file contains multiple bookmarks and the user invokes next-bookmark navigation
- **THEN** the system moves the cursor to the next bookmarked line in that file
- **AND** the newly focused bookmark becomes the active navigation target

#### Scenario: Jump from a bookmark list
- **WHEN** the user selects a bookmark from the searchable bookmark list
- **THEN** the system opens or focuses the bookmarked file
- **AND** the editor moves the cursor to the bookmarked line

### Requirement: Bookmarks persist across restarts and in-app renames
The system SHALL restore bookmarks when a bookmarked file is reopened in a later session and SHALL preserve those bookmarks when the file is renamed from within LushText. A Save As operation MUST create a new file identity that does not automatically inherit the original bookmark set.

#### Scenario: Restore bookmarks after reopening the app
- **WHEN** the user reopens a file that previously had bookmarks after closing and relaunching LushText
- **THEN** the system restores the saved bookmarks for that file
- **AND** the gutter indicators and bookmark navigation behave as they did before the app was closed

#### Scenario: Preserve bookmarks across an in-app rename
- **WHEN** the user renames a bookmarked file through the LushText sidebar workflow
- **THEN** the system keeps the file's bookmark set associated with the renamed file
- **AND** reopening the renamed file restores the same bookmarks

#### Scenario: Save As starts a new bookmark identity
- **WHEN** the user saves a bookmarked document to a new file path with Save As
- **THEN** the newly saved file opens without copied bookmarks by default
- **AND** the original file keeps its existing bookmark set

### Requirement: Bookmark sidecars use canonical saved-document identity in app data
The system SHALL persist bookmark sidecars under `$XDG_DATA_HOME/lushtext/bookmarks/` using a saved-document identity derived from the document's canonical path rather than by modifying the source file. The persisted sidecar identity MUST remain separate from the source document bytes and MUST be recomputed for a new Save As destination instead of copying the prior bookmark set automatically.

#### Scenario: Reopening the same saved document restores bookmarks from app data
- **WHEN** the user reopens a saved document that already has persisted bookmarks
- **THEN** the bookmark set is restored from bookmark sidecar data stored under the app data directory
- **AND** the source file itself remains unchanged by bookmark persistence

### Requirement: Empty bookmark state removes its sidecar file
The system SHALL remove a bookmark sidecar file when a document no longer has any persisted bookmarks, instead of leaving an empty bookmark sidecar behind indefinitely.

#### Scenario: Removing the final bookmark deletes the bookmark sidecar
- **WHEN** the user removes the last remaining bookmark for a saved document
- **THEN** the persisted bookmark sidecar for that document is deleted from the app data directory
- **AND** reopening the document no longer restores an empty bookmark sidecar payload

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

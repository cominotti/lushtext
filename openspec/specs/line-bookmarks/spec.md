# line-bookmarks Specification

## Purpose
TBD - created by archiving change bookmarks-annotations. Update Purpose after archive.
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


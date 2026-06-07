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

### Requirement: Bookmark gutter marks expose direct editing
The system SHALL open a bookmark edit dialog when the user activates an existing bookmark gutter mark in a saved, loaded editor. The dialog MUST identify the activated bookmark, show its current label and 1-based line number, and leave source file bytes unchanged while editing bookmark metadata.

#### Scenario: Activate a bookmark gutter mark
- **WHEN** the active saved editor contains a bookmark gutter mark
- **AND** the user activates that bookmark mark in the line-mark gutter
- **THEN** the system opens a bookmark edit dialog for that bookmark
- **AND** the dialog shows the bookmark label if one exists
- **AND** the dialog shows the bookmark line using the 1-based line number users expect

#### Scenario: Activate a non-bookmark line mark
- **WHEN** the user activates a line mark that is not a LushText bookmark
- **THEN** the system does not open the bookmark edit dialog
- **AND** persisted bookmark data remains unchanged

### Requirement: Bookmark edit dialog can reassign a bookmark line
The system SHALL let users update an existing bookmark's label and line number from the bookmark edit dialog. Saving a valid edit MUST move the same bookmark identity to the target line, refresh live bookmark presentation, and persist through the existing bookmark sidecar save path. Invalid edits MUST keep the existing bookmark unchanged.

#### Scenario: Save a bookmark with a new label
- **WHEN** the bookmark edit dialog is open for an existing bookmark
- **AND** the user changes the label and saves
- **THEN** the same bookmark record stores the new normalized label
- **AND** later bookmark tooltips, browse surfaces, and navigation labels show the updated label

#### Scenario: Move a bookmark to another line
- **WHEN** the bookmark edit dialog is open for an existing bookmark
- **AND** the user enters a valid target line that does not already contain another bookmark
- **AND** the user saves
- **THEN** the same bookmark identity moves to the target line
- **AND** the gutter indicator and minimap marker move to that line
- **AND** reopening the file restores the bookmark at the new line

#### Scenario: Reject an out-of-range target line
- **WHEN** the bookmark edit dialog is open for an existing bookmark
- **AND** the user enters a target line outside the active buffer's line range
- **AND** the user attempts to save
- **THEN** the system keeps the dialog open and reports validation feedback
- **AND** the bookmark line and label remain unchanged

#### Scenario: Reject a target line with another bookmark
- **WHEN** the bookmark edit dialog is open for one bookmark
- **AND** the user enters a target line that already contains a different bookmark
- **AND** the user attempts to save
- **THEN** the system keeps the dialog open and reports validation feedback
- **AND** neither bookmark is merged, overwritten, or removed

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

### Requirement: Bookmark sidecars use the public v1 JSON envelope
The system SHALL persist bookmark sidecars as supported v1 app-owned JSON envelopes under `$XDG_DATA_HOME/lushtext/bookmarks/`. Runtime loading MUST require the bookmark-sidecar document kind and supported version before reading bookmark records.

#### Scenario: Save bookmark sidecar as v1
- **WHEN** a saved document's bookmarks are persisted
- **THEN** the bookmark sidecar is written as a pretty JSON envelope with the bookmark-sidecar document kind
- **AND** the payload stores the document identity and bookmark records

#### Scenario: Unsupported bookmark sidecar is isolated
- **WHEN** a bookmark sidecar is bare pre-public JSON, wrong-kind JSON, unsupported-version JSON, or malformed JSON
- **THEN** that sidecar is preserved through recovery diagnostics before replacement is allowed
- **AND** unrelated valid bookmark sidecars continue to load

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
- **AND** each bookmark row preserves enough workspace and file metadata for the user to tell where it belongs
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

### Requirement: Bookmark browser previews show anchored source excerpts
The system SHALL show a contextual source excerpt in the `Browse Notes...` preview pane when the selected entry is a bookmark. The excerpt MUST include the bookmarked line, bounded nearby context before and after that line, and visible bookmark metadata. For Markdown-like files, the excerpt MUST render through the Markdown preview surface using the bookmarked file as render context. For other UTF-8 text files, the excerpt MUST render as raw monospace text and visually emphasize the bookmarked line. The Open action MUST continue to open or focus the bookmarked file at the bookmarked line.

#### Scenario: Preview a Markdown bookmark as rendered context
- **WHEN** the user selects a bookmark row for a Markdown-like saved file in `Browse Notes...`
- **THEN** the preview pane renders a bounded Markdown excerpt around the bookmarked line
- **AND** relative preview context uses the bookmarked file path and available workspace roots
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

### Requirement: Bookmark sidecar corruption is isolated and diagnostic
The system SHALL isolate malformed bookmark sidecars from valid bookmark state. A malformed bookmark sidecar MUST be preserved when possible, reported through recovery diagnostics, and excluded from normal bookmark restoration until repaired or replaced.

#### Scenario: Malformed bookmark sidecar does not clear active bookmarks elsewhere
- **WHEN** one bookmark sidecar cannot be parsed during workspace bookmark listing
- **THEN** valid bookmark sidecars continue to load and appear in browse surfaces
- **AND** the malformed sidecar is reported as a recovery diagnostic

#### Scenario: Reopening file with corrupt bookmark sidecar warns without modifying source
- **WHEN** a saved file is opened and its bookmark sidecar is malformed
- **THEN** the editor opens the source file without bookmark indicators from that sidecar
- **AND** the source file bytes remain unchanged
- **AND** a diagnostic is logged or surfaced

#### Scenario: Corrupt bookmark sidecar is preserved before replacement
- **WHEN** the user later creates a new bookmark for a file whose prior bookmark sidecar was malformed
- **THEN** the malformed sidecar is quarantined or otherwise preserved before a replacement sidecar is written

### Requirement: Bookmark migrations are retryable after in-app renames
The system SHALL record pending bookmark sidecar migrations before or as part of the post-rename sidecar migration workflow. If migration or cleanup fails, the pending state MUST survive restart and be retried during startup reconciliation.

#### Scenario: Pending bookmark migration survives restart
- **WHEN** an in-app rename succeeds but bookmark sidecar migration fails before completion
- **THEN** a pending migration record remains in app data
- **AND** restarting LushText retries the bookmark migration

#### Scenario: Completed bookmark migration clears pending state
- **WHEN** bookmark sidecar migration succeeds and obsolete sidecars are cleaned up or safely reconciled
- **THEN** the pending bookmark migration record is removed durably

#### Scenario: Repeated migration failure is bounded
- **WHEN** the same bookmark migration fails repeatedly
- **THEN** the system reports the persistent failure without blocking startup
- **AND** it does not retry in an unbounded tight loop

### Requirement: Bookmark reconciliation never deletes the only non-empty bookmark copy
The system SHALL reconcile duplicate old and new bookmark sidecars conservatively. It MUST NOT delete the only non-empty bookmark sidecar unless a merged or migrated replacement has already been durably written.

#### Scenario: Duplicate bookmark sidecars merge deterministically
- **WHEN** old and new bookmark sidecars both exist after a rename retry
- **THEN** the system merges bookmark records using stable bookmark identities and ordering
- **AND** it writes the merged target before removing any obsolete sidecar

#### Scenario: Obsolete cleanup failure remains diagnostic
- **WHEN** a migrated bookmark sidecar is written but the obsolete sidecar cannot be removed
- **THEN** the system reports a cleanup diagnostic
- **AND** later startup reconciliation attempts cleanup again

#### Scenario: Ambiguous duplicate bookmarks are preserved
- **WHEN** duplicate bookmark sidecars cannot be merged deterministically
- **THEN** the system preserves both copies or quarantines the ambiguous copy
- **AND** it reports that automatic bookmark reconciliation was incomplete

### Requirement: Bookmark reliability has layered automated coverage
The project SHALL add deterministic service, integration, and widget coverage for bookmark sidecar corruption, retryable migrations, duplicate reconciliation, and partial browser behavior.

#### Scenario: Service tests cover corrupt bookmark sidecars
- **WHEN** service tests load malformed bookmark sidecar bytes
- **THEN** the result preserves or quarantines the sidecar and returns recovery diagnostics
- **AND** no valid bookmark sidecar is dropped because of the malformed one

#### Scenario: Migration tests cover retry ledger behavior
- **WHEN** tests simulate an in-app rename whose bookmark migration fails after the source rename
- **THEN** a pending migration record survives restart
- **AND** a later successful retry removes the record durably

#### Scenario: Widget tests cover partial bookmark browsing
- **WHEN** one bookmark sidecar is corrupt and another is valid
- **THEN** the notes or bookmark browser remains usable
- **AND** the user receives visible partial-recovery feedback

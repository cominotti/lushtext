## ADDED Requirements

### Requirement: Notes browser loads bookmark excerpts lazily and safely
The system SHALL load bookmark source excerpts in `Browse Notes...` only when a bookmark row is selected. Closed-file excerpt loading MUST run off the GTK main thread, MUST be bounded by explicit size, line, and scan budgets, and MUST ignore stale completions after selection changes. Loading, unavailable, and rendered states MUST preserve the existing notes-browser layout, search field, sidebar selection, preview-only row activation behavior, and Open action semantics.

#### Scenario: Selecting a bookmark starts a lazy preview load
- **WHEN** `Browse Notes...` is open with bookmark rows
- **AND** the user selects a bookmark whose source file is not already open
- **THEN** the preview pane shows a bookmark-specific loading state
- **AND** the notes sidebar remains interactive while the excerpt is loaded
- **AND** the browser does not pre-load excerpts for unselected bookmark rows

#### Scenario: Stale bookmark preview completion is ignored
- **WHEN** a closed-file bookmark excerpt load is in progress
- **AND** the user selects a different notes-browser row before that load completes
- **THEN** the earlier load completion does not replace the currently selected row's preview
- **AND** the currently selected row's Open action target is not changed by the stale completion

#### Scenario: Bookmark excerpt preview keeps dialog geometry stable
- **WHEN** the user changes selection between bookmark rows, workspace-note rows, and document-note rows
- **THEN** the notes-browser dialog keeps its settled outer allocation stable
- **AND** the preview pane uses internal scrolling or clipping rather than resizing the dialog around the excerpt

#### Scenario: Bookmark excerpt text does not drive browser search
- **WHEN** the user searches in `Browse Notes...`
- **THEN** bookmark filtering continues to use bookmark label, saved file metadata, source metadata, and line number
- **AND** the browser does not read closed-file excerpt text merely to decide whether a bookmark row matches the search

#### Scenario: Markdown and raw bookmark previews coexist with note previews
- **WHEN** the user selects Markdown bookmark rows, raw text bookmark rows, workspace-note rows, and document-note rows in one `Browse Notes...` session
- **THEN** each selection renders through the correct preview mode for that row
- **AND** switching preview modes does not leave stale Markdown, raw text, loading, or unavailable content visible for the next selected row

#### Scenario: Open-tab bookmark rows use the same preview behavior
- **WHEN** a bookmark row appears in the `Open Tabs` section
- **AND** the user selects that bookmark
- **THEN** the preview pane uses the live open-editor excerpt behavior for that row
- **AND** the row remains labeled as an open-tab source rather than a fake workspace row

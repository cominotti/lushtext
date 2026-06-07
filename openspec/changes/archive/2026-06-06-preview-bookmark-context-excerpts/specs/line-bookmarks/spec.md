## ADDED Requirements

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

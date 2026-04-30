## ADDED Requirements

### Requirement: Range-note browser entries use the native Adwaita sidebar rail
The system SHALL present saved-file range-note entries in the workspace-scoped `Browse Notes...` surface through an `AdwSidebar` section rather than a hand-built `GtkListBox` rail. The sidebar section MUST preserve the existing workspace-scope filtering, range-note Markdown preview, file/line metadata, preview-only pointer selection, and explicit Open behavior.

#### Scenario: Browse range notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more saved-file range notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those range notes in a dedicated `AdwSidebar` section
- **AND** each range-note item identifies the saved file, workspace, presentation style, and annotated line range

#### Scenario: Preview a range note from the sidebar rail
- **WHEN** the user selects a range-note item in the Notes browser sidebar rail
- **THEN** the browser updates the preview pane with that range note's rendered Markdown content or explicit empty-note state
- **AND** the Open action targets the selected range note

#### Scenario: Click a range-note item without opening the editor
- **WHEN** the user clicks a range-note item in the Notes browser sidebar rail
- **THEN** the browser updates the selected item and preview pane only
- **AND** the range-note editing surface is not opened

#### Scenario: Open a range note explicitly from the browser
- **WHEN** the user selects a range-note item and invokes the browser's Open action
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the system focuses and opens the selected range note in that file

#### Scenario: Range-note search keeps annotation metadata matching
- **WHEN** the user searches in the Notes browser
- **THEN** range-note sidebar items match by title, saved file metadata, workspace metadata, line-range metadata, or note body text
- **AND** non-matching range-note items are hidden without changing persisted annotation data

### Requirement: Range-note editor mode switching is layout-stable
The system SHALL keep the range-note editing popup visually stable when switching between Edit and Render. The edit and rendered note surfaces MUST keep matching text-origin padding so the same plain note content does not shift horizontally or vertically when changing modes.

#### Scenario: Switch a range note from Edit to Render
- **WHEN** the user opens a range-note editing popup and switches from Edit to Render
- **THEN** the popup keeps the same outer size
- **AND** the rendered text starts at the same visual origin as the editable text

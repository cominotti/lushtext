## ADDED Requirements

### Requirement: Workspace-note browser entries use the native Adwaita sidebar rail
The system SHALL present workspace-note entries in the workspace-scoped `Browse Notes...` surface through an `AdwSidebar` section rather than a hand-built `GtkListBox` rail. The sidebar section MUST preserve the existing workspace-scope filtering, workspace-note preview, preview-only pointer selection, and explicit Open behavior.

#### Scenario: Browse workspace notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more workspace notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those workspace notes in a dedicated `AdwSidebar` section
- **AND** each workspace-note item identifies the workspace/root it belongs to

#### Scenario: Preview a workspace note from the sidebar rail
- **WHEN** the user selects a workspace-note item in the Notes browser sidebar rail
- **THEN** the browser updates the preview pane with that workspace note's rendered Markdown content or explicit empty-note state
- **AND** the Open action targets the selected workspace note

#### Scenario: Click a workspace-note item without opening the editor
- **WHEN** the user clicks a workspace-note item in the Notes browser sidebar rail
- **THEN** the browser updates the selected item and preview pane only
- **AND** the workspace-note editing surface is not opened

#### Scenario: Open a workspace note explicitly from the browser
- **WHEN** the user selects a workspace-note item and invokes the browser's Open action
- **THEN** the system opens that workspace note's editing surface
- **AND** the Open action does not require an active document tab

#### Scenario: Filtered workspace-note state remains explicit
- **WHEN** the Notes browser search text filters out every workspace-note item
- **THEN** the workspace-note sidebar section no longer shows matching items
- **AND** if no notes of any kind match, the browser shows an explicit empty filtered state

### Requirement: Workspace-note editor mode switching is layout-stable
The system SHALL keep the shared workspace-note editor popup visually stable when switching between Edit and Render. The edit and rendered note surfaces MUST keep matching text-origin padding so the same plain note content does not shift horizontally or vertically when changing modes.

#### Scenario: Switch a workspace note from Edit to Render
- **WHEN** the user opens a workspace-note editing popup and switches from Edit to Render
- **THEN** the popup keeps the same outer size
- **AND** the rendered text starts at the same visual origin as the editable text

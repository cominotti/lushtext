## ADDED Requirements

### Requirement: Minimap preferences stay grouped on the Editor page
The system SHALL expose minimap-specific preferences in a dedicated `Minimap` group on the existing `Editor` preferences page. The system MUST NOT add a top-level `Minimap` preferences page while the minimap preference surface contains only the minimap visibility control and the long-line marker visibility control.

#### Scenario: Editor page shows minimap controls together
- **WHEN** the user opens Preferences and views the `Editor` page
- **THEN** the page contains a `Minimap` group
- **AND** that group contains the `Show Minimap` control
- **AND** that group contains the `Show Long-Line Markers` control

#### Scenario: Long-line marker preference defaults off
- **WHEN** LushText starts with default settings
- **THEN** the `Show Long-Line Markers` preference is disabled
- **AND** enabling the minimap does not automatically enable long-line markers

## MODIFIED Requirements

### Requirement: The minimap shows semantic navigation markers
The system SHALL render visually distinguishable semantic markers in the minimap for editor-local navigation signals. The default supported marker set SHALL include bookmarks, active in-tab search matches, and modified-since-save line regions. Lines above the minimap warning threshold SHALL be represented by long-line warning markers only when the user enables the long-line marker preference.

#### Scenario: Bookmarks appear in the minimap
- **WHEN** the active editor page contains bookmarks
- **THEN** the minimap shows bookmark markers at the corresponding document regions
- **AND** removing a bookmark removes its minimap marker

#### Scenario: Active search matches appear in the minimap
- **WHEN** the user has an active in-tab search with one or more matches in the editor
- **THEN** the minimap shows search-match markers at the corresponding regions
- **AND** clearing or closing the in-tab search removes those search markers

#### Scenario: Modified-since-save markers clear after save
- **WHEN** the user edits a document and then saves it successfully
- **THEN** the minimap removes modified-since-save markers for those saved changes
- **AND** later unsaved edits create new modified markers again

#### Scenario: Long-line markers stay hidden by default
- **WHEN** the active document contains lines longer than the minimap warning threshold
- **AND** the minimap is enabled
- **AND** the long-line marker preference is disabled
- **THEN** the minimap does not show long-line warning markers for those regions

#### Scenario: Enabled long-line markers flag long lines
- **WHEN** the active document contains lines longer than the minimap warning threshold
- **AND** the minimap is enabled
- **AND** the long-line marker preference is enabled
- **THEN** the minimap shows long-line warning markers for those regions
- **AND** shortening those lines removes the corresponding warnings

#### Scenario: Disabling long-line markers removes existing warnings
- **WHEN** long-line warning markers are visible in the minimap
- **AND** the user disables the long-line marker preference
- **THEN** the minimap removes long-line warning markers
- **AND** bookmark, search-match, and modified-since-save markers remain governed by their existing behavior

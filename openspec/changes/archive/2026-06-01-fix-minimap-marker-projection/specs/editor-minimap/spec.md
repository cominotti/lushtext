## MODIFIED Requirements

### Requirement: The minimap shows semantic navigation markers
The system SHALL render visually distinguishable semantic markers in the minimap for editor-local navigation signals. The default supported marker set SHALL include bookmarks, active in-tab search matches, and modified-since-save line regions. Lines above the minimap warning threshold SHALL be represented by long-line warning markers only when the user enables the long-line marker preference. Semantic marker positions SHALL be projected from the same rendered `GtkSourceMap` geometry used by the minimap content, including top margin, dynamic EOF overscroll, and allocation changes. Semantic markers MUST NOT be spread across blank EOF overscroll tail space when their corresponding document lines render above that tail.

#### Scenario: Bookmarks appear in the minimap
- **WHEN** the active editor page contains bookmarks
- **THEN** the minimap shows bookmark markers at the corresponding document regions
- **AND** removing a bookmark removes its minimap marker

#### Scenario: Active search matches appear in the minimap
- **WHEN** the user has an active in-tab search with one or more matches in the editor
- **THEN** the minimap shows search-match markers at the corresponding regions
- **AND** clearing or closing the in-tab search removes those search markers

#### Scenario: Search markers respect EOF overscroll geometry
- **WHEN** the active editor page has dynamic EOF overscroll
- **AND** an active in-tab search produces markers for document lines above the final rendered content line
- **THEN** search-match markers align with the corresponding rendered minimap content regions
- **AND** search-match markers do not extend into the blank EOF overscroll tail below the rendered document content

#### Scenario: All semantic marker categories share source-map projection
- **WHEN** the active editor page shows bookmark, search-match, modified-since-save, and enabled long-line warning markers
- **THEN** each marker category is positioned using the same source-map layout geometry
- **AND** resizing or reallocation refreshes marker positions without changing which semantic lines are marked

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

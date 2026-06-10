## MODIFIED Requirements

### Requirement: The minimap shows a clear viewport overlay
The system SHALL render the native `GtkSourceMap` rectangular semi-transparent viewport highlight inside the minimap so users can see which portion of the document is currently visible in the main editor. The highlight SHALL preserve the existing neutral editor-chrome fill, border, sizing, and interaction behavior. LushText MUST NOT replace the visible native highlight with an app-owned duplicate or restyled substitute for this behavior. The highlight SHALL remain visible whenever the minimap itself is visible, MUST NOT use system accent color styling, and MUST remain visually subordinate to semantic markers such as bookmarks, search matches, modified lines, and long-line warnings.

#### Scenario: Viewport overlay tracks the visible region during scrolling
- **WHEN** the user scrolls the active editor page while the minimap is visible
- **THEN** the minimap updates the native viewport highlight position to the corresponding document region
- **AND** the highlight remains visually distinct from semantic markers behind it
- **AND** the highlight does not use system accent color styling

#### Scenario: Viewport overlay fills the minimap when the full document is visible
- **WHEN** the active document fits entirely within the editor viewport and the minimap is enabled
- **THEN** the minimap remains visible
- **AND** the native viewport highlight expands to indicate that the full document is currently visible
- **AND** the expanded highlight remains neutral and visually calm rather than appearing as an accent-colored block

#### Scenario: Native viewport effect is not replaced
- **WHEN** the minimap is visible for a supported document
- **THEN** users see the existing native `GtkSourceMap` viewport highlight effect
- **AND** no second visible viewport overlay duplicates or hides that native effect
- **AND** minimap click and drag navigation continue to use the normal source-map interaction path

## ADDED Requirements

### Requirement: Native minimap viewport edge keeps its rendered content anchor
The minimap SHALL keep the rendered top edge of the native `GtkSourceMap` viewport highlight stable relative to the first rendered minimap content row when the active editor is anchored at the top of the document. Sidebar show/hide, width-only reflow, word-wrap changes, style changes, and dynamic overscroll refreshes MUST NOT remove, clip, or shift that native top-edge treatment by more than the declared pixel tolerance for the current visual scenario.

#### Scenario: Sidebar show preserves native viewport edge
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** the workspace sidebar is shown at a width where the editor allocation changes
- **THEN** the native minimap viewport highlight top edge remains rendered
- **AND** the first minimap content row remains rendered
- **AND** the vertical delta between those rendered screenshot anchors remains within the declared tolerance

#### Scenario: Sidebar hide preserves native viewport edge
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** the workspace sidebar is hidden at a width where the editor allocation changes
- **THEN** the native minimap viewport highlight top edge remains rendered
- **AND** the first minimap content row remains rendered
- **AND** the vertical delta between those rendered screenshot anchors remains within the declared tolerance

#### Scenario: Theme and wrap controls preserve native edge identity
- **WHEN** the minimap is visible at the top of the document
- **AND** the visual scenario runs in light and dark style preferences with word wrap enabled and disabled
- **THEN** each run finds the native viewport top-edge anchor using the style-appropriate screenshot detector
- **AND** no run accepts viewport fill pixels, minimap background pixels, or semantic marker pixels as a replacement for the top-edge anchor

#### Scenario: Mid-file native highlight remains synchronized
- **WHEN** the minimap is visible and the editor is scrolled to a representative middle document range
- **AND** shell reflow changes the editor width
- **THEN** the native viewport highlight remains synchronized with the settled visible editor range
- **AND** the highlight bounds remain projectable through the same source-map geometry as semantic minimap markers
